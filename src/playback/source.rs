use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use ncm_api::{NcmError, SongInfo, SongQuality};
use sonar::{PlayUrlResult, Quality, SearchQuery, SonarFinder, Song};
use stream_download::http::HttpStream;
use stream_download::{Settings, StreamDownload, StreamPhase};
use tokio::sync::mpsc;

use super::player::{AudioInput, SharedReader};
use super::stream_client::HeadersClient;
use crate::cache::CacheManager;
use crate::event::{Event, PlaybackEvent};
#[cfg(target_os = "linux")]
use crate::playback::mem_rss_kb;
use crate::service::ApiService;

/// Resolves audio inputs for songs via local files, NCM streaming, or sonar fallback.
#[derive(Clone)]
pub struct AudioSource {
    service: ApiService,
    pub cache: CacheManager,
    quality: SongQuality,
    finder: Arc<SonarFinder>,
    sonar_enabled: bool,
    /// HTTP client used for streaming play URLs (proxy + headers applied).
    stream_client: reqwest::Client,
    event_tx: mpsc::UnboundedSender<Event>,
    /// Original sonar songs for search results, keyed by the synthetic
    /// `SongInfo` id so playback can resolve the source via the same provider.
    pub(crate) sonar_songs: Arc<Mutex<HashMap<u64, Arc<Song>>>>,
}

impl AudioSource {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        service: ApiService,
        cache: CacheManager,
        quality: SongQuality,
        stream_client: reqwest::Client,
        finder: Arc<SonarFinder>,
        sonar_enabled: bool,
        sonar_songs: Arc<Mutex<HashMap<u64, Arc<Song>>>>,
        event_tx: mpsc::UnboundedSender<Event>,
    ) -> Self {
        Self {
            service,
            cache,
            quality,
            finder,
            sonar_enabled,
            stream_client,
            event_tx,
            sonar_songs,
        }
    }

    /// Build stream-download settings that persist the cache entry and notify
    /// the UI once the stream has finished caching to disk. The entry is only
    /// recorded when the download actually completes, so the index never
    /// contains partial files — those caused cache misses (and a re-download)
    /// on restart.
    fn download_settings(
        &self,
        song: &SongInfo,
        ext: &'static str,
        msong: Option<sonar::Song>,
    ) -> Settings<HttpStream<HeadersClient>> {
        let event_tx = self.event_tx.clone();
        let cache = self.cache.clone();
        let sent = Arc::new(AtomicBool::new(false));
        let song = song.clone();
        Settings::default().on_progress(move |_, state, _| {
            if state.phase == StreamPhase::Complete && !sent.swap(true, Ordering::SeqCst) {
                cache.mark_cached(&song, ext, msong.clone());
                let _ = event_tx.send(PlaybackEvent::Cached(song.id).into());
            }
        })
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

    fn to_sonar_quality(quality: SongQuality) -> Quality {
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

    /// Stream a resolved play URL through the cache layer.
    async fn stream_play_url(
        &self,
        song: &SongInfo,
        msong: Option<&sonar::Song>,
        play: PlayUrlResult,
    ) -> Result<AudioInput, String> {
        let url = url::Url::parse(&play.url).map_err(|e| format!("sonar URL解析失败: {e}"))?;

        let ext = Self::ext_from_url(&play.url);
        let provider = self
            .cache
            .create_provider(song, ext)
            .map_err(|e| format!("缓存创建失败: {e}"))?;

        let stream = HttpStream::new(HeadersClient::new(self.stream_client.clone()), url)
            .await
            .map_err(|e| format!("sonar 流初始化失败: {e}"))?;

        let reader = StreamDownload::from_stream(
            stream,
            provider,
            self.download_settings(song, ext, msong.cloned()),
        )
        .await
        .map_err(|e| format!("sonar 流下载失败: {e}"))?;

        Ok(SharedReader(Arc::new(Mutex::new(Box::new(reader)))))
    }

    /// Search all configured sonar sources for the best playable match and
    /// stream it (cross-provider fallback).
    async fn resolve_providers(&self, song: &SongInfo) -> Result<AudioInput, String> {
        let keyword = format!("{} {}", song.name, song.singer);
        let query = SearchQuery::new(keyword).with_duration(song.duration);

        let (found, play) = self
            .finder
            .search_and_get_url(&query, Some(Self::to_sonar_quality(self.quality)))
            .await
            .map_err(|e| format!("sonar 兜底失败: {e}"))?;

        #[cfg(target_os = "linux")]
        log::info!(
            "[HEAP] after sonar search (id={}): {} kB — {} ({})",
            song.id,
            mem_rss_kb(),
            found.name,
            found.source
        );

        self.stream_play_url(song, Some(&found), play).await
    }

    /// Resolve a sonar search result directly via the provider that found it.
    async fn resolve_by_provider(&self, song: &SongInfo) -> Result<AudioInput, String> {
        let msong = self
            .sonar_songs
            .lock()
            .map_err(|_| "sonar 歌曲注册表损坏".to_string())?
            .get(&song.id)
            .cloned()
            .or_else(|| self.cache.thirdparty_song(song.id).map(Arc::new))
            .ok_or_else(|| "搜索结果音源信息丢失".to_string())?;

        let play = self
            .finder
            .get_play_url_for_song(&msong, Some(Self::to_sonar_quality(self.quality)))
            .await
            .map_err(|e| format!("获取音源失败 ({}): {e}", msong.source))?;

        #[cfg(target_os = "linux")]
        log::info!(
            "[HEAP] after get_play_url_for_song (id={}): {} kB — {} ({})",
            song.id,
            mem_rss_kb(),
            msong.name,
            msong.source
        );

        self.stream_play_url(song, Some(&msong), play).await
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

        let stream = HttpStream::new(HeadersClient::new(self.stream_client.clone()), url)
            .await
            .map_err(|e| format!("流初始化失败: {e}"))?;

        let reader =
            StreamDownload::from_stream(stream, provider, self.download_settings(song, ext, None))
                .await
                .map_err(|e| format!("流下载失败: {e}"))?;

        Ok(SharedReader(Arc::new(Mutex::new(Box::new(reader)))))
    }

    /// Return a previously cached audio file for `song`, if one exists.
    async fn resolve_cached(&self, song: &SongInfo) -> Option<AudioInput> {
        let ext = self.cache.find_cached_extension(song.id)?.to_string();
        let cache = self.cache.clone();
        let song_id = song.id;
        let file = tokio::task::spawn_blocking(move || cache.open_cached(song_id, &ext))
            .await
            .ok()?
            .ok()?;
        Some(SharedReader(Arc::new(Mutex::new(Box::new(file)))))
    }

    /// Open a local file for a `Free` song whose `album` field is a real path.
    async fn resolve_local(&self, song: &SongInfo) -> Option<AudioInput> {
        if song.copyright != ncm_api::SongCopyright::Free {
            return None;
        }
        let path = std::path::Path::new(&song.album);
        if !path.exists() {
            return None;
        }
        let path = path.to_path_buf();
        let file = tokio::task::spawn_blocking(move || std::fs::File::open(path))
            .await
            .ok()?
            .ok()?;
        Some(SharedReader(Arc::new(Mutex::new(Box::new(file)))))
    }

    pub async fn resolve(&self, song: &SongInfo) -> Result<AudioInput, String> {
        // 1. Cache wins for every source.
        if let Some(input) = self.resolve_cached(song).await {
            return Ok(input);
        }

        // 2. Third-party (sonar) songs: direct provider, then cross-provider fallback.
        if sonar::is_sonar_song_id(song.id) {
            match self.resolve_by_provider(song).await {
                Ok(input) => return Ok(input),
                Err(e) => log::warn!("sonar 直接解析失败，改用兜底搜索: {e}"),
            }
            if self.sonar_enabled {
                return self.resolve_providers(song).await;
            }
            return Err("sonar 未启用，第三方音源无法解析".into());
        }

        // 3. Free NCM songs may point at a local file path.
        if let Some(input) = self.resolve_local(song).await {
            return Ok(input);
        }

        // 4. NCM streaming: transient network failures retried once, then sonar fallback.
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
                    if self.sonar_enabled {
                        log::info!(
                            "NCM解析失败，尝试sonar fallback: {} - {} ({})",
                            song.name,
                            song.singer,
                            e
                        );
                        return self.resolve_providers(song).await;
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
            "NCM网络错误，2次重试失败，fallback到sonar: {} - {}",
            song.name,
            song.singer
        );
        if self.sonar_enabled {
            self.resolve_providers(song).await
        } else {
            Err("NCM网络错误，2次重试失败".into())
        }
    }
}
