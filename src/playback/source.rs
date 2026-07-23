use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ncm_api::{NcmClient, NcmError, SongInfo, SongQuality};
use stream_download::{Settings, StreamDownload};
use y7dl::Client;

use super::player::{AudioInput, SharedReader};
use crate::cache::CacheManager;
use crate::utils::youtube::{clean_search_query, parse_duration_str, score_match};

/// Resolves audio inputs for songs via local files, NCM streaming, or YouTube fallback.
#[derive(Clone)]
pub struct AudioSource {
    api: Arc<NcmClient>,
    pub cache: CacheManager,
    quality: SongQuality,
    y7dl: Arc<Client>,
}

impl AudioSource {
    pub fn new(
        api: Arc<NcmClient>,
        downloads_dir: PathBuf,
        base_dir: PathBuf,
        quality: SongQuality,
        template: String,
        proxy: String,
    ) -> Self {
        let y7dl_client = if proxy.is_empty() {
            Client::new()
        } else {
            Client::with_proxy(&proxy)
        };
        Self {
            api,
            cache: CacheManager::new(downloads_dir, base_dir, template),
            quality,
            y7dl: Arc::new(y7dl_client),
        }
    }

    /// Derive a file extension from a streaming URL.
    fn ext_from_url(url: &str) -> &'static str {
        let path = url::Url::parse(url)
            .ok()
            .and_then(|u| {
                u.path_segments()
                    .and_then(|mut s| s.next_back().map(|s| s.to_string()))
            })
            .unwrap_or_default();
        let stem = path.rsplit('.').nth(1).unwrap_or("");
        match stem {
            "flac" => "flac",
            "ogg" => "ogg",
            "wav" => "wav",
            "m4a" | "mp4" => "m4a",
            _ => "mp3",
        }
    }

    /// Search YouTube for a song and stream the best-matching result.
    async fn youtube_fallback(&self, song: &SongInfo) -> Result<AudioInput, String> {
        let query = format!("{} {}", clean_search_query(&song.name), song.singer);
        let results = self
            .y7dl
            .search(&query, 8, None)
            .await
            .map_err(|e| format!("YouTube搜索失败: {e}"))?;

        if results.is_empty() {
            return Err("YouTube未找到相关结果".into());
        }

        let best = results
            .iter()
            .map(|r| {
                let yt_dur = parse_duration_str(&r.duration);
                let score = score_match(&r.title, &r.author, &r.views, yt_dur, song);
                (r, score)
            })
            .max_by_key(|(_, score)| *score)
            .filter(|(_, score)| *score >= 15);

        let (result, _score) = match best {
            Some((r, s)) => (r, s),
            None => return Err("YouTube未找到匹配的歌曲".into()),
        };

        let video = self
            .y7dl
            .get_video(&result.video_id)
            .await
            .map_err(|e| format!("获取YouTube视频信息失败: {e}"))?;

        let format = video
            .audio_formats()
            .into_iter()
            .filter(|f| {
                // Only keep formats rodio/symphonia can decode
                f.mime_type.starts_with("audio/mpeg")
                    || f.mime_type.starts_with("audio/mp4")
                    || f.mime_type.starts_with("audio/ogg")
                    || f.mime_type.starts_with("audio/flac")
                    || f.mime_type.starts_with("audio/wav")
            })
            .max_by_key(|f| f.bitrate.unwrap_or(0))
            .ok_or_else(|| "YouTube视频无可用水源格式".to_string())?;

        let stream_url_str = self
            .y7dl
            .stream_url(&video, format)
            .await
            .map_err(|e| format!("获取YouTube流地址失败: {e}"))?;

        let url =
            url::Url::parse(&stream_url_str).map_err(|e| format!("YouTube URL解析失败: {e}"))?;

        let ext = Self::ext_from_url(&stream_url_str);
        let provider = self
            .cache
            .create_provider(song, ext)
            .map_err(|e| format!("缓存创建失败: {e}"))?;

        let reader = StreamDownload::new_http(url, provider, Settings::default())
            .await
            .map_err(|e| format!("YouTube流下载失败: {e}"))?;

        Ok(SharedReader(Arc::new(Mutex::new(Box::new(reader)))))
    }

    /// Try to resolve a song from NCM streaming.
    async fn resolve_ncm(&self, song: &SongInfo) -> Result<AudioInput, String> {
        let urls = self.api.songs_url_v1(&[song.id], self.quality).await;

        let urls = match urls {
            Ok(u) => u,
            Err(NcmError::Http(e)) => {
                return Err(format!("NETWORK:获取歌曲URL失败: {e}"));
            }
            Err(NcmError::Session(e)) => {
                return Err(format!("NETWORK:会话异常: {e}"));
            }
            Err(e) => {
                return Err(format!("获取歌曲URL失败: {e}"));
            }
        };

        let url_str = urls
            .iter()
            .find(|u| !u.url.is_empty() && !u.free_trial)
            .map(|u| &u.url)
            .ok_or_else(|| "该歌曲暂无播放源".to_string())?;

        let url = url::Url::parse(url_str).map_err(|e| format!("URL解析失败: {e}"))?;
        let ext = Self::ext_from_url(url_str);

        let provider = self
            .cache
            .create_provider(song, ext)
            .map_err(|e| format!("缓存创建失败: {e}"))?;

        let reader = StreamDownload::new_http(url, provider, Settings::default())
            .await
            .map_err(|e| format!("流下载失败: {e}"))?;

        Ok(SharedReader(Arc::new(Mutex::new(Box::new(reader)))))
    }

    pub async fn resolve(&self, song: &SongInfo) -> Result<AudioInput, String> {
        // Free songs: only use local file if the path actually exists
        if song.copyright == ncm_api::SongCopyright::Free {
            let path = std::path::Path::new(&song.album);
            if path.exists() {
                let path = path.to_path_buf();
                let file = tokio::task::spawn_blocking(move || std::fs::File::open(path))
                    .await
                    .map_err(|e| format!("无法打开本地文件: {e}"))?
                    .map_err(|e| format!("无法打开本地文件: {e}"))?;
                return Ok(SharedReader(Arc::new(Mutex::new(Box::new(file)))));
            }
            // album is not a valid file path, fall through to cache/NCM/YouTube
        }

        if self.cache.is_cached(song.id, "mp3")
            || self.cache.is_cached(song.id, "flac")
            || self.cache.is_cached(song.id, "m4a")
            || self.cache.is_cached(song.id, "ogg")
        {
            // Try common extensions, prefer the one that exists
            for ext in &["mp3", "flac", "m4a", "ogg"] {
                if self.cache.is_cached(song.id, ext) {
                    let cache = self.cache.clone();
                    let song_id = song.id;
                    let ext = ext.to_string();
                    let file =
                        tokio::task::spawn_blocking(move || cache.open_cached(song_id, &ext))
                            .await
                            .map_err(|e| format!("无法打开缓存文件: {e}"))?
                            .map_err(|e| format!("无法打开缓存文件: {e}"))?;
                    return Ok(SharedReader(Arc::new(Mutex::new(Box::new(file)))));
                }
            }
            unreachable!("is_cached checked above");
        }

        // Try NCM source — transient network failures are retried up to 3 times,
        // then fall back to YouTube. All other failures fall back immediately.
        for attempt in 0..3 {
            match self.resolve_ncm(song).await {
                Ok(input) => return Ok(input),
                Err(e) if e.starts_with("NETWORK:") && attempt < 2 => {
                    log::warn!(
                        "NCM网络错误，重试 {}/3: {} - {}: {}",
                        attempt + 1,
                        song.name,
                        song.singer,
                        &e["NETWORK:".len()..]
                    );
                }
                Err(e) => {
                    log::info!(
                        "NCM解析失败，尝试YouTube fallback: {} - {} ({})",
                        song.name,
                        song.singer,
                        e
                    );
                    return self.youtube_fallback(song).await;
                }
            }
        }

        log::warn!(
            "NCM网络错误，3次重试失败，fallback到YouTube: {} - {}",
            song.name,
            song.singer
        );
        self.youtube_fallback(song).await
    }
}
