use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ncm_api::{NcmClient, SongInfo, SongQuality};
use stream_download::{Settings, StreamDownload};
use y7dl::Client;

use super::player::{AudioInput, SharedReader};
use crate::cache::CacheManager;

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

    /// Parse a duration string like "3:33" or "1:02:33" into seconds.
    fn parse_duration_str(s: &str) -> Option<u64> {
        let parts: Vec<&str> = s.split(':').collect();
        match parts.len() {
            2 => {
                let min: u64 = parts[0].parse().ok()?;
                let sec: u64 = parts[1].parse().ok()?;
                Some(min * 60 + sec)
            }
            3 => {
                let hr: u64 = parts[0].parse().ok()?;
                let min: u64 = parts[1].parse().ok()?;
                let sec: u64 = parts[2].parse().ok()?;
                Some(hr * 3600 + min * 60 + sec)
            }
            _ => None,
        }
    }

    /// Score a YouTube search result against the original NCM song.
    /// Higher is better. 0 means no match at all.
    fn score_match(
        title: &str,
        author: &str,
        yt_duration_secs: Option<u64>,
        song: &SongInfo,
    ) -> u32 {
        let title_lower = title.to_lowercase();
        let author_lower = author.to_lowercase();
        let name_lower = song.name.to_lowercase();
        let singer_lower = song.singer.to_lowercase();

        let mut score = 0u32;

        if title_lower.contains(&name_lower) {
            score += 10;
        }
        if author_lower.contains(&singer_lower) || singer_lower.contains(&author_lower) {
            score += 10;
        }
        if song.duration > 0 {
            let ncm_secs = song.duration / 1000;
            if let Some(yt_secs) = yt_duration_secs {
                let diff = ncm_secs.abs_diff(yt_secs);
                if diff <= 10 {
                    score += 5;
                } else if diff <= 30 {
                    score += 2;
                }
            }
        }

        score
    }

    /// Search YouTube for a song and stream the best-matching result.
    async fn youtube_fallback(&self, song: &SongInfo) -> Result<AudioInput, String> {
        let query = format!("{} {}", song.name, song.singer);
        let results = self
            .y7dl
            .search(&query, 5, None)
            .await
            .map_err(|e| format!("YouTube搜索失败: {e}"))?;

        if results.is_empty() {
            return Err("YouTube未找到相关结果".into());
        }

        let best = results
            .iter()
            .map(|r| {
                let yt_dur = Self::parse_duration_str(&r.duration);
                let score = Self::score_match(&r.title, &r.author, yt_dur, song);
                (r, score)
            })
            .max_by_key(|(_, score)| *score)
            .filter(|(_, score)| *score > 0);

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

        let ext = "mp3";
        let provider = self
            .cache
            .create_provider(song, ext)
            .map_err(|e| format!("缓存创建失败: {e}"))?;

        let reader = StreamDownload::new_http(url, provider, Settings::default())
            .await
            .map_err(|e| format!("YouTube流下载失败: {e}"))?;

        Ok(SharedReader(Arc::new(Mutex::new(Box::new(reader)))))
    }

    pub async fn resolve(&self, song: &SongInfo) -> Result<AudioInput, String> {
        let ext = "mp3";

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

        if self.cache.is_cached(song.id, ext) {
            let cache = self.cache.clone();
            let song_id = song.id;
            let file = tokio::task::spawn_blocking(move || cache.open_cached(song_id, ext))
                .await
                .map_err(|e| format!("无法打开缓存文件: {e}"))?
                .map_err(|e| format!("无法打开缓存文件: {e}"))?;
            return Ok(SharedReader(Arc::new(Mutex::new(Box::new(file)))));
        }

        // Try NCM source first, fall back to YouTube on failure
        // VIP/Payment songs: skip NCM API, go straight to YouTube
        let ncm_result = async {
            if matches!(
                song.copyright,
                ncm_api::SongCopyright::VipOnly
                    | ncm_api::SongCopyright::Payment
                    | ncm_api::SongCopyright::VipOnlyHighRate
            ) {
                return Err("该歌曲需要会员".to_string());
            }

            let urls = self
                .api
                .songs_url_v1(&[song.id], self.quality)
                .await
                .map_err(|e| format!("获取歌曲URL失败: {e}"))?;

            let url_str = urls
                .first()
                .filter(|u| !u.url.is_empty())
                .map(|u| &u.url)
                .ok_or_else(|| "该歌曲暂无播放源".to_string())?;

            let url = url::Url::parse(url_str).map_err(|e| format!("URL解析失败: {e}"))?;

            let provider = self
                .cache
                .create_provider(song, ext)
                .map_err(|e| format!("缓存创建失败: {e}"))?;

            let reader = StreamDownload::new_http(url, provider, Settings::default())
                .await
                .map_err(|e| format!("流下载失败: {e}"))?;

            Ok::<AudioInput, String>(SharedReader(Arc::new(Mutex::new(Box::new(reader)))))
        }
        .await;

        match ncm_result {
            Ok(input) => Ok(input),
            Err(_) => {
                log::info!(
                    "NCM无源，尝试YouTube fallback: {} - {}",
                    song.name,
                    song.singer
                );
                self.youtube_fallback(song).await
            }
        }
    }
}
