//! Lyrics loading (NetEase + sonar fallback) with cache integration.

use std::{collections::HashMap, sync::Arc};

use super::ApiService;
use crate::{
    playback::{LyricLine, parse_lyric_lines},
    utils::lock_recover,
};

impl ApiService {
    /// Load lyrics for a song, with cache integration.
    pub async fn load_lyrics(&self, song_id: u64) -> Option<ncm_api::Lyrics> {
        if let Some(cached) = self.cache.load_lyrics_cache_async(song_id).await {
            return Some(cached);
        }
        match self.client.song_lyric(song_id).await {
            Ok(lyrics) => {
                let cache = self.cache.clone();
                tokio::task::spawn_blocking(move || {
                    cache.save_lyrics_cache(song_id, &lyrics);
                    lyrics
                })
                .await
                .ok()
            }
            Err(e) => {
                log::error!("Failed to fetch lyrics for {song_id}: {e}");
                None
            }
        }
    }

    /// Load lyrics for a third-party (sonar) song, with cache integration.
    ///
    /// Serves cached lyrics first (keyed by song id, shared with the NCM path)
    /// so replays don't hit the provider again. On a miss, the original sonar
    /// song is resolved from `registry` (falling back to the disk cache) and the
    /// provider's lyrics are fetched and cached.
    ///
    /// Returns `(lyric, translated)` lines, or `None` when no usable lyrics
    /// exist. `registry` maps synthetic song ids back to the sonar `Song`.
    pub async fn load_sonar_lyrics(
        &self,
        song_id: u64,
        finder: Arc<sonar::SonarFinder>,
        registry: &std::sync::Mutex<HashMap<u64, Arc<sonar::Song>>>,
    ) -> Option<(Vec<LyricLine>, Vec<LyricLine>)> {
        // Serve cached lyrics first so replays don't hit the provider again.
        if let Some(cached) = self.cache.load_lyrics_cache_async(song_id).await {
            let lyric_lines = parse_lyric_lines(&cached.lyric);
            if !lyric_lines.is_empty() {
                let tlyric_lines = parse_lyric_lines(&cached.tlyric);
                return Some((lyric_lines, tlyric_lines));
            }
        }

        let msong = lock_recover(registry)
            .get(&song_id)
            .cloned()
            .or_else(|| self.cache.thirdparty_song(song_id))?;
        let lrc = finder.get_lyrics_fallback(&msong).await?;
        let lines: Vec<String> = lrc.lines().map(|s| s.to_string()).collect();
        let lyric_lines = parse_lyric_lines(&lines);
        if lyric_lines.is_empty() {
            return None;
        }
        self.cache.save_lyrics_cache(
            song_id,
            &ncm_api::Lyrics {
                lyric: lines,
                tlyric: Vec::new(),
            },
        );
        Some((lyric_lines, Vec::new()))
    }
}
