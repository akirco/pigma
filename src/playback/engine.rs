//! Playback engine core: the [`PlaybackEngine`] struct, construction, and the
//! song-resolve / playback-event pipeline. Queue operations live in
//! [`queueing`], queue identity & persistence in [`session`], and transport
//! controls in [`transport`].

mod heartbeat;
mod queueing;
mod session;
mod transport;

#[cfg(all(target_os = "linux", target_env = "gnu"))]
use crate::utils::process::mem_rss_kb;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use ncm_api::{SongInfo, SongQuality};
use tokio::sync::mpsc;

use super::{
    controller::PlaybackHandle,
    lyrics::LyricLine,
    mode::{self, Strategy},
    queue::PlaylistQueue,
    source::AudioSource,
    state::PlaybackState,
    storage::PlaylistStorage,
};
use crate::{
    cache::CacheManager,
    event::{Event, PlaybackEvent},
    service::ApiService,
};

/// Fixed queue key (display name) shared by every third-party (sonar) search
/// queue; all such songs are stored in the single `thirdparty_source.json`.
pub const THIRD_PARTY_QUEUE_KEY: &str = "第三方搜索";

/// Fixed queue key (display name) shared by every NCM search queue; all such
/// songs are stored in the single `ncm_search.json`.
pub const NCM_SEARCH_QUEUE_KEY: &str = "官方搜索";

/// Return whether an error is a transient audio stream error (buffer underrun/overrun). These
/// occur frequently when the stream download can't keep up with playback (e.g. YouTube), and
/// rodio recovers automatically, so they should be ignored. Case-insensitive to cover cpal's
/// "Buffer underrun/overrun occurred." and various decoder-specific wordings.
fn is_transient_stream_error(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("underrun") || lower.contains("overrun")
}

/// Orchestrates audio playback, queue management, and player strategies.
pub struct PlaybackEngine {
    pub state: PlaybackState,
    pub(super) queue: PlaylistQueue,
    /// Display name of the currently loaded queue (tab title).
    active_queue_key: String,
    /// Display name of the queue the current song was started from. Unlike
    /// `active_queue_key`, it is not changed when browsing other tabs while
    /// playing, so the tab bar can highlight where the music actually comes
    /// from.
    playing_queue_key: String,
    /// Canonical id of the currently loaded queue (`q_<hash>` / `q_thirdparty`).
    /// Only this queue stays in memory; the others live on disk and are loaded
    /// on demand when switched.
    active_queue_id: String,
    /// Cached `(id, display)` of all known queues (disk + active), refreshed on
    /// queue activation so the Playlist page tab bar doesn't scan the disk per
    /// frame. The id is the real index; display is only for humans.
    queue_entries_cache: Vec<(String, String)>,
    /// Display names of all known queues, derived from `queue_entries_cache`
    /// for the tab bar rendering.
    queue_keys_cache: Vec<String>,
    strategy: Strategy,
    storage: PlaylistStorage,
    source: AudioSource,
    pub(super) controller: PlaybackHandle,
    pub(super) event_tx: mpsc::UnboundedSender<Event>,
    pub(super) service: ApiService,
    playlist_id: Option<u64>,
    consecutive_errors: u32,
    /// Set of song IDs in the user's "我喜欢的音乐" collection (shares the same `Arc` with `App`).
    liked_ids: Arc<std::sync::Mutex<HashSet<u64>>>,
    /// In-flight `start_current_song` resolve task. Aborted when the user
    /// switches songs so a stale resolve neither consumes bandwidth nor sends a
    /// `Play` command for a song that is no longer current.
    current_resolve: Option<tokio::task::JoinHandle<()>>,
}

impl PlaybackEngine {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        event_tx: mpsc::UnboundedSender<Event>,
        service: ApiService,
        cache: Arc<CacheManager>,
        base_dir: std::path::PathBuf,
        quality: SongQuality,
        save_on_play: bool,
        stream_client: reqwest::Client,
        finder: Arc<sonar::SonarFinder>,
        sonar_enabled: bool,
        sonar_songs: Arc<std::sync::Mutex<HashMap<u64, Arc<sonar::Song>>>>,
        liked_ids: Arc<std::sync::Mutex<HashSet<u64>>>,
    ) -> Self {
        let storage = PlaylistStorage::new(base_dir);
        let mut this = Self {
            state: PlaybackState::default(),
            queue: PlaylistQueue::new(),
            active_queue_key: String::new(),
            playing_queue_key: String::new(),
            active_queue_id: String::new(),
            queue_entries_cache: Vec::new(),
            queue_keys_cache: Vec::new(),
            strategy: mode::Strategy::Sequential,
            storage,
            source: AudioSource::new(
                service.clone(),
                cache,
                quality,
                save_on_play,
                stream_client,
                finder,
                sonar_enabled,
                sonar_songs,
                event_tx.clone(),
            ),
            controller: PlaybackHandle::new(event_tx.clone()),
            event_tx: event_tx.clone(),
            service,
            playlist_id: None,
            consecutive_errors: 0,
            liked_ids,
            current_resolve: None,
        };
        this.restore_session();
        this
    }

    pub fn current_song(&self) -> Option<Arc<SongInfo>> {
        self.state.current_song.clone()
    }

    /// Refresh `state.liked` for the player bar based on whether the current song is in the
    /// "我喜欢的音乐" set.
    pub fn update_liked_status(&mut self) {
        self.state.liked = self
            .state
            .current_song
            .as_ref()
            .map(|s| {
                self.liked_ids
                    .lock()
                    .map(|g| g.contains(&s.id))
                    .unwrap_or(false)
            })
            .unwrap_or(false);
    }

    pub fn is_currently_playing(&self, song_id: u64) -> bool {
        self.state.playing
            && self
                .state
                .current_song
                .as_ref()
                .is_some_and(|s| s.id == song_id)
    }

    pub fn finish_and_snapshot(&mut self) -> Option<(u64, u64, f64)> {
        let info = self.state.current_song.as_ref().map(|s| {
            let progress = self.state.progress;
            (s.id, s.duration, progress)
        });
        self.handle_finished();
        info
    }

    fn handle_finished(&mut self) {
        let should_advance = self.state.on_finished();
        if should_advance {
            self.next();
        }
        if !self.state.playing {
            self.state.clear_after_stopped();
        }
    }

    pub fn on_playback_started(&mut self) {
        self.state.on_started();
        self.consecutive_errors = 0;
    }

    pub fn on_playback_progress(&mut self, position: Duration, total: Option<Duration>) {
        self.state.on_progress(position, total);
    }

    pub fn on_playback_error(&mut self, err: String) {
        // Buffer underruns/overruns are transient events (e.g. YouTube streams downloading
        // slower than playback), and rodio recovers automatically, so ignore them to avoid
        // wrongly switching songs or duplicating error reports.
        if is_transient_stream_error(&err) {
            log::warn!("忽略瞬时音频流错误: {err}");
            return;
        }
        // If error is from cached file, delete cache and retry same song
        let retryable = err.starts_with("无法打开缓存文件") || err.starts_with("decode:");
        self.state.on_error(err);
        self.consecutive_errors += 1;
        if self.consecutive_errors >= 3 {
            self.stop();
            return;
        }
        if retryable && let Some(song) = self.state.current_song.as_ref() {
            let song_id = song.id;
            let cache = self.source.cache.clone();
            tokio::task::spawn_blocking(move || {
                let _ = std::fs::remove_file(cache.cache_path(song_id, "mp3"));
                cache.remove_from_index(song_id);
            });
            self.start_current_song(None);
            return;
        }
        self.next();
    }

    pub fn on_lyrics_loaded(
        &mut self,
        song_id: u64,
        lyrics: Vec<LyricLine>,
        translated_lyrics: Vec<LyricLine>,
    ) {
        self.state
            .on_lyrics_loaded(song_id, lyrics, translated_lyrics);
    }

    /// Toggle save-on-play at runtime: takes effect the next time the audio source is resolved.
    pub fn set_save_on_play(&mut self, enabled: bool) {
        self.source.set_save_on_play(enabled);
    }

    pub(super) fn start_current_song(&mut self, seek_time: Option<Duration>) {
        // Cancel any in-flight resolve for a previously requested song before
        // starting a new one, so a stale network request cannot finish late and
        // send a `Play` command for the wrong song (or waste bandwidth).
        if let Some(prev) = self.current_resolve.take() {
            prev.abort();
        }
        let song = match self.queue.current_song() {
            Some(s) => s.clone(),
            None => return,
        };

        self.state.playing = true;
        self.state.paused = false;
        self.state.seeking = true;
        self.state.current_song = Some(song.clone());
        self.state.error = None;
        self.state.cached = self.source.cache.is_cached(song.id, "mp3");
        self.update_liked_status();

        if let Some(t) = seek_time {
            let total_secs = song.duration as f64 / 1000.0;
            if total_secs > 0.0 {
                self.state.progress = (t.as_secs_f64() / total_secs).clamp(0.0, 1.0);
            }
        }

        let source = self.source.clone();
        let event_tx = self.event_tx.clone();
        let controller = self.controller.clone();

        if event_tx.send(PlaybackEvent::Started.into()).is_err() {
            log::error!("Failed to send PlaybackStarted: receiver dropped");
        }

        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        let song_id = song.id;
        self.current_resolve = Some(tokio::spawn(async move {
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            log::info!(
                "[HEAP] before resolve {} (id={}): {} kB",
                song.name,
                song_id,
                mem_rss_kb()
            );
            let input = match source.resolve(&song).await {
                Ok(input) => input,
                Err(e) => {
                    #[cfg(all(target_os = "linux", target_env = "gnu"))]
                    log::info!(
                        "[HEAP] after resolve FAIL {} (id={}): {} kB",
                        song.name,
                        song_id,
                        mem_rss_kb()
                    );
                    #[cfg(all(target_os = "linux", target_env = "gnu"))]
                    unsafe {
                        libc::malloc_trim(0);
                    }
                    if event_tx.send(PlaybackEvent::Error(e).into()).is_err() {
                        log::error!("Failed to send PlaybackError: receiver dropped");
                    }
                    return;
                }
            };
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            log::info!(
                "[HEAP] after resolve OK {} (id={}): {} kB",
                song.name,
                song_id,
                mem_rss_kb()
            );
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            unsafe {
                libc::malloc_trim(0);
            }
            controller.request(input, seek_time);
        }));
    }
}
