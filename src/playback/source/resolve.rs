//! Provider resolution order: cache, sonar songs, local files, NCM streaming
//! with one retry, then the sonar fallback.

use std::sync::{Arc, Mutex};

use ncm_api::{NcmError, SongInfo, SongQuality};
use sonar::{PlayUrlResult, Quality, SearchQuery};

use super::{
    super::player::{AudioInput, SharedReader},
    AudioSource,
};
use crate::lock::lock_recover;

impl AudioSource {
    /// Derive a file extension from a streaming URL.
    fn ext_from_url(url: &str) -> &'static str {
        let path = url::Url::parse(url)
            .ok()
            .and_then(|u| {
                u.path_segments()
                    .and_then(|mut s| s.next_back().map(|s| s.to_string()))
            })
            .unwrap_or_default();
        crate::cache::canonical_ext(path.rsplit('.').nth(1).unwrap_or(""))
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
        song: &Arc<SongInfo>,
        msong: Option<&sonar::Song>,
        play: PlayUrlResult,
    ) -> Result<AudioInput, String> {
        let url = url::Url::parse(&play.url).map_err(|e| format!("sonar URL解析失败: {e}"))?;
        let ext = Self::ext_from_url(&play.url);
        self.build_stream(url, song, ext, msong.cloned()).await
    }

    /// Search all configured sonar sources for the best playable match and
    /// stream it (cross-provider fallback).
    async fn resolve_providers(&self, song: &Arc<SongInfo>) -> Result<AudioInput, String> {
        let keyword = format!("{} {}", song.name, song.singer);
        let query = SearchQuery::new(keyword).with_duration(song.duration);

        let (found, play) = self
            .finder
            .search_and_get_url(&query, Some(Self::to_sonar_quality(self.quality)))
            .await
            .map_err(|e| format!("sonar 兜底失败: {e}"))?;

        self.stream_play_url(song, Some(&found), play).await
    }

    /// Resolve a sonar search result directly via the provider that found it.
    async fn resolve_by_provider(&self, song: &Arc<SongInfo>) -> Result<AudioInput, String> {
        let msong = lock_recover(&self.sonar_songs)
            .get(&song.id)
            .cloned()
            .or_else(|| self.cache.thirdparty_song(song.id))
            .ok_or_else(|| "搜索结果音源信息丢失".to_string())?;

        let play = self
            .finder
            .get_play_url_for_song(&msong, Some(Self::to_sonar_quality(self.quality)))
            .await
            .map_err(|e| format!("获取音源失败 ({}): {e}", msong.source))?;

        self.stream_play_url(song, Some(&msong), play).await
    }

    /// Try to resolve a song from NCM streaming.
    async fn resolve_ncm(&self, song: &Arc<SongInfo>) -> Result<AudioInput, String> {
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

        self.build_stream(url, song, ext, None).await
    }

    /// Return a previously cached audio file for `song`, if one exists.
    async fn resolve_cached(&self, song: &Arc<SongInfo>) -> Option<AudioInput> {
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
    async fn resolve_local(&self, song: &Arc<SongInfo>) -> Option<AudioInput> {
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

    pub(crate) async fn resolve(&self, song: &Arc<SongInfo>) -> Result<AudioInput, String> {
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
