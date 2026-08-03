use std::sync::{Arc, Mutex};

use musicx::{MusicFinder, Quality, SearchQuery};
use ncm_api::{NcmError, SongInfo, SongQuality};
use stream_download::{Settings, StreamDownload};

use super::player::{AudioInput, SharedReader};
use crate::cache::CacheManager;
#[cfg(target_os = "linux")]
use crate::playback::mem_rss_kb;
use crate::service::ApiService;

/// Resolves audio inputs for songs via local files, NCM streaming, or musicx fallback.
#[derive(Clone)]
pub struct AudioSource {
    service: ApiService,
    pub cache: CacheManager,
    quality: SongQuality,
    finder: Arc<MusicFinder>,
    musicx_enabled: bool,
}

impl AudioSource {
    pub fn new(
        service: ApiService,
        cache: CacheManager,
        quality: SongQuality,
        _proxy: String,
        finder: MusicFinder,
        musicx_enabled: bool,
    ) -> Self {
        Self {
            service,
            cache,
            quality,
            finder: Arc::new(finder),
            musicx_enabled,
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

    fn to_musicx_quality(quality: SongQuality) -> Quality {
        match quality {
            SongQuality::Lossless
            | SongQuality::HiRes
            | SongQuality::Surround
            | SongQuality::Master
            | SongQuality::AudioVivid => Quality::Lossless,
            SongQuality::Standard => Quality::Standard,
            _ => Quality::High,
        }
    }

    /// Search all configured musicx sources for the best playable match and
    /// stream it.
    async fn musicx_fallback(&self, song: &SongInfo) -> Result<AudioInput, String> {
        let keyword = format!("{} {}", song.name, song.singer);
        let query = SearchQuery::new(keyword).with_duration(song.duration);

        let (found, play) = self
            .finder
            .search_and_get_url(&query, Some(Self::to_musicx_quality(self.quality)))
            .await
            .map_err(|e| format!("musicx 兜底失败: {e}"))?;

        #[cfg(target_os = "linux")]
        log::info!(
            "[HEAP] after musicx search (id={}): {} kB — {} ({})",
            song.id,
            mem_rss_kb(),
            found.name,
            found.source
        );

        let url = url::Url::parse(&play.url).map_err(|e| format!("musicx URL解析失败: {e}"))?;

        let ext = Self::ext_from_url(&play.url);
        let provider = self
            .cache
            .create_provider(song, ext)
            .map_err(|e| format!("缓存创建失败: {e}"))?;

        let reader = StreamDownload::new_http(url, provider, Settings::default())
            .await
            .map_err(|e| format!("musicx 流下载失败: {e}"))?;

        #[cfg(target_os = "linux")]
        log::info!(
            "[HEAP] after StreamDownload::new_http (id={}): {} kB",
            song.id,
            mem_rss_kb()
        );

        self.cache.mark_cached(song, ext);

        Ok(SharedReader(Arc::new(Mutex::new(Box::new(reader)))))
    }

    /// Try to resolve a song from NCM streaming.
    async fn resolve_ncm(&self, song: &SongInfo) -> Result<AudioInput, String> {
        let urls = self.service.fetch_song_urls(&[song.id], self.quality).await;

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

        self.cache.mark_cached(song, ext);

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
            // album is not a valid file path, fall through to cache/NCM/musicx
        }

        if let Some(ext) = self.cache.find_cached_extension(song.id) {
            let cache = self.cache.clone();
            let song_id = song.id;
            let ext = ext.to_string();
            let file = tokio::task::spawn_blocking(move || cache.open_cached(song_id, &ext))
                .await
                .map_err(|e| format!("无法打开缓存文件: {e}"))?
                .map_err(|e| format!("无法打开缓存文件: {e}"))?;
            return Ok(SharedReader(Arc::new(Mutex::new(Box::new(file)))));
        }

        // Try NCM source — transient network failures are retried once,
        // then fall back to musicx (kugou/kuwo/bilivideo/youtube). All other
        // failures fall back immediately.
        for attempt in 0..2 {
            match self.resolve_ncm(song).await {
                Ok(input) => return Ok(input),
                Err(e) if e.starts_with("NETWORK:") && attempt < 1 => {
                    log::warn!(
                        "NCM网络错误，重试 {}/2: {} - {}: {}",
                        attempt + 1,
                        song.name,
                        song.singer,
                        &e["NETWORK:".len()..]
                    );
                }
                Err(e) => {
                    #[cfg(target_os = "linux")]
                    log::info!(
                        "[HEAP] after resolve_ncm FAIL (id={}): {} kB — {}",
                        song.id,
                        mem_rss_kb(),
                        e
                    );
                    if self.musicx_enabled {
                        log::info!(
                            "NCM解析失败，尝试musicx fallback: {} - {} ({})",
                            song.name,
                            song.singer,
                            e
                        );
                        return self.musicx_fallback(song).await;
                    }
                    return Err(e);
                }
            }
        }

        #[cfg(target_os = "linux")]
        log::info!(
            "[HEAP] after resolve_ncm retries exhausted (id={}): {} kB",
            song.id,
            mem_rss_kb()
        );
        log::warn!(
            "NCM网络错误，2次重试失败，fallback到musicx: {} - {}",
            song.name,
            song.singer
        );
        if self.musicx_enabled {
            self.musicx_fallback(song).await
        } else {
            Err("NCM网络错误，2次重试失败".into())
        }
    }
}
