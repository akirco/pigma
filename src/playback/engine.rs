use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use ncm_api::{SongInfo, SongQuality};
use tokio::sync::mpsc;

use crate::cache::CacheManager;
use crate::event::{Event, PlaybackEvent};
use crate::service::ApiService;
use crate::utils::time::local_month_day;

use super::controller::PlaybackHandle;
use super::lyrics::LyricLine;
use super::mode::{self, PlayMode, Strategy};
use super::queue::PlaylistQueue;
use super::source::AudioSource;
use super::state::PlaybackState;
use super::storage::PlaylistStorage;

/// Read the current RSS in KB from /proc/self/status.
#[cfg(target_os = "linux")]
pub fn mem_rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse().ok())
        })
        .unwrap_or(0)
}

/// Fixed queue key (display name) shared by every third-party (sonar) search
/// queue; all such songs are stored in the single `thirdparty_source.json`.
pub const THIRD_PARTY_QUEUE_KEY: &str = "第三方搜索";

/// Fixed queue key (display name) shared by every NCM search queue; all such
/// songs are stored in the single `ncm_search.json`.
pub const NCM_SEARCH_QUEUE_KEY: &str = "官方搜索";

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
}

impl PlaybackEngine {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        event_tx: mpsc::UnboundedSender<Event>,
        service: ApiService,
        cache: CacheManager,
        base_dir: std::path::PathBuf,
        quality: SongQuality,
        stream_client: reqwest::Client,
        finder: Arc<sonar::SonarFinder>,
        sonar_enabled: bool,
        sonar_songs: Arc<std::sync::Mutex<HashMap<u64, Arc<sonar::Song>>>>,
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
        };
        this.restore_session();
        this
    }

    pub fn current_song(&self) -> Option<Arc<SongInfo>> {
        self.state.current_song.clone()
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

    pub fn song_at(&self, index: usize) -> Option<&Arc<SongInfo>> {
        self.queue.songs.get(index)
    }

    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    pub fn queue_songs(&self) -> &[Arc<SongInfo>] {
        &self.queue.songs
    }

    pub fn queue_current_index(&self) -> Option<usize> {
        self.queue.current_index
    }

    pub fn set_queue_songs(&mut self, songs: Vec<Arc<SongInfo>>) {
        self.queue.set_songs(songs);
    }

    /// Replace the queue with a subset of `full` selected by `indices`, cloning
    /// only the selected songs (avoids cloning the whole list on every filter
    /// keystroke).
    pub fn set_queue_indices(&mut self, full: &[Arc<SongInfo>], indices: &[usize]) {
        let songs = indices
            .iter()
            .filter_map(|&i| full.get(i).map(Arc::clone))
            .collect();
        self.queue.set_songs(songs);
    }

    /// Key of the currently loaded queue.
    pub fn queue_key(&self) -> &str {
        &self.active_queue_key
    }

    /// Key of the queue the current song was started from (may differ from
    /// [`Self::queue_key`] while browsing other tabs during playback).
    pub fn playing_queue_key(&self) -> &str {
        &self.playing_queue_key
    }

    /// Cached breadcrumb keys of all queues (disk + active), in no particular
    /// order. Refresh with [`Self::refresh_queue_keys`] after any change.
    pub fn queue_keys(&self) -> &[String] {
        &self.queue_keys_cache
    }

    /// Rebuild the cached queue list from disk plus the active queue. The list
    /// only ever grows: a queue's file is written asynchronously
    /// (`spawn_blocking`), so a key seen once (or still being persisted) must
    /// not vanish from the cache on a later synchronous scan.
    pub fn refresh_queue_keys(&mut self) {
        let mut entries = self.queue_entries_cache.clone();
        for (id, display) in self.storage.list_queues() {
            if !entries.iter().any(|(i, _)| i == &id) {
                entries.push((id, display));
            }
        }
        if !self.active_queue_id.is_empty()
            && !entries.iter().any(|(i, _)| i == &self.active_queue_id)
        {
            entries.push((self.active_queue_id.clone(), self.active_queue_key.clone()));
        }
        entries.sort();
        self.queue_entries_cache = entries;
        self.queue_keys_cache = self
            .queue_entries_cache
            .iter()
            .map(|(_, d)| d.clone())
            .collect();
    }

    /// Persist the currently loaded queue to its per-id file and record it as
    /// the active one. A no-op when no id is set (nothing played yet).
    fn persist_active_queue(&self) {
        if self.active_queue_id.is_empty() {
            return;
        }
        self.storage.save_queue(
            &self.active_queue_id,
            &self.active_queue_key,
            &self.queue.songs,
            &self.queue.history,
            self.queue.current_index,
            &self.state.mode,
            self.state.volume,
            self.state.progress,
        );
    }

    /// Blocking counterpart of [`Self::persist_active_queue`], used on shutdown
    /// so the queue file is guaranteed written before the process exits.
    fn persist_active_queue_blocking(&self) {
        if self.active_queue_id.is_empty() {
            return;
        }
        self.storage.save_queue_sync(
            &self.active_queue_id,
            &self.active_queue_key,
            &self.queue.songs,
            &self.queue.history,
            self.queue.current_index,
            &self.state.mode,
            self.state.volume,
            self.state.progress,
        );
    }

    /// Activate the queue whose display key is `key`, deriving its canonical id
    /// from the display name. Called when playing dated content where `key` is
    /// freshly produced (`dated_key`), so hashing it is lossless.
    pub fn activate_queue(&mut self, key: &str) {
        let id = PlaylistStorage::queue_id(key);
        self.activate_by_id(key, &id);
    }

    /// Activate a queue by its canonical id and display name, saving the
    /// previously loaded queue first. Playback state is untouched — this only
    /// swaps which queue the engine operates on.
    fn activate_by_id(&mut self, display: &str, id: &str) {
        if self.active_queue_id == id {
            return;
        }
        self.persist_active_queue();
        let saved = self.storage.load_queue_by_id(id);
        let queue = match saved {
            Some(s) => PlaylistQueue::from_parts(
                s.queue.into_iter().map(Arc::new).collect(),
                s.history,
                s.current_index,
            ),
            None => PlaylistQueue::new(),
        };
        self.queue = queue;
        self.active_queue_key = display.to_string();
        self.active_queue_id = id.to_string();
        self.strategy =
            mode::create_strategy(&self.state.mode, self.queue.len(), self.queue.current_index);
        self.refresh_queue_keys();
    }

    /// Switch to the next/previous saved queue (for the Playlist page Tab
    /// binding). Returns the key that was activated, or `None` when there is
    /// only one queue to choose from.
    pub fn switch_queue(&mut self, forward: bool) -> Option<String> {
        self.refresh_queue_keys();
        let entries = self.queue_entries_cache.clone();
        if entries.len() <= 1 {
            return None;
        }
        let idx = entries
            .iter()
            .position(|(id, _)| id == &self.active_queue_id)
            .unwrap_or(0);
        let next = if forward {
            &entries[(idx + 1) % entries.len()]
        } else {
            &entries[(idx + entries.len() - 1) % entries.len()]
        };
        if next.0 == self.active_queue_id {
            return None;
        }
        self.activate_by_id(&next.1, &next.0);
        Some(self.active_queue_key.clone())
    }

    /// Append today's `MM-DD` to a breadcrumb context, producing the full queue
    /// key that is stored in the queue file (and shown as its tab). Time-based
    /// content like 每日推荐 therefore gets one queue per day; replaying the
    /// same context on the same day reuses the existing queue instead of
    /// creating a new one.
    fn dated_key(&self, context: &str) -> String {
        format!("{context} {}", local_month_day())
    }

    /// Replace the queue for `key` with `songs` and start playing `index`.
    pub fn play_songs(&mut self, key: &str, songs: Vec<SongInfo>, index: usize) {
        if songs.is_empty() || index >= songs.len() {
            return;
        }
        let key = self.dated_key(key);
        self.activate_queue(&key);
        self.playing_queue_key = self.active_queue_key.clone();
        self.controller.stop();
        let songs: Vec<Arc<SongInfo>> = songs.into_iter().map(Arc::new).collect();
        self.queue = PlaylistQueue::from_songs(songs, index);
        self.strategy =
            mode::create_strategy(&self.state.mode, self.queue.len(), self.queue.current_index);
        self.start_current_song(None);
    }

    /// Append `songs` to a fixed, non-dated queue key and start playing
    /// `index`. Used by search (third-party & NCM) so all such songs share one
    /// queue instead of one per keyword/day. If the song is already in the
    /// queue, just play the existing entry instead of adding a duplicate
    /// (re-pressing Enter on a search result).
    pub fn append_and_play_key(&mut self, key: &str, songs: &[SongInfo], index: usize) {
        if songs.is_empty() || index >= songs.len() {
            return;
        }
        self.activate_queue(key);
        self.playing_queue_key = self.active_queue_key.clone();
        if let Some(pos) = self.queue.find_song_index(songs[index].id) {
            self.play_index(pos);
            return;
        }
        self.controller.stop();
        let offset = self.queue.append(songs);
        self.queue.current_index = Some(offset + index);
        self.strategy =
            mode::create_strategy(&self.state.mode, self.queue.len(), self.queue.current_index);
        self.start_current_song(None);
    }

    pub fn play_index(&mut self, index: usize) {
        if index >= self.queue.len() {
            return;
        }

        self.controller.stop();
        self.playing_queue_key = self.active_queue_key.clone();
        self.queue.advance_to(index);
        self.strategy =
            mode::create_strategy(&self.state.mode, self.queue.len(), self.queue.current_index);
        self.start_current_song(None);
    }

    /// Add `song` to the current queue right after the playing song.
    /// Keeps the rest of the queue intact. Does not interrupt the currently
    /// playing song.
    pub fn add_next(&mut self, song: Arc<SongInfo>) {
        self.queue.insert_next(vec![song]);
    }

    pub fn next(&mut self) {
        if self.queue.is_empty() {
            return;
        }

        if matches!(self.state.mode, PlayMode::Heartbeat { .. }) {
            self.next_heartbeat();
            return;
        }

        match self.queue.next_index(&mut self.strategy) {
            Some(idx) => {
                self.controller.stop();
                self.queue.advance_to(idx);
                self.start_current_song(None);
            }
            None => {
                self.stop();
            }
        }
    }

    pub fn prev(&mut self) {
        if self.queue.is_empty() {
            return;
        }

        if let Some(prev_id) = self.queue.pop_history()
            && let Some(pos) = self.queue.find_song_index(prev_id)
        {
            self.controller.stop();
            self.queue.current_index = Some(pos);
            self.start_current_song(None);
            return;
        }

        if let Some(idx) = self.queue.prev_index(&mut self.strategy) {
            self.controller.stop();
            self.queue.current_index = Some(idx);
            self.start_current_song(None);
        }
    }

    pub fn toggle_pause(&mut self) {
        if !self.state.playing && self.queue.current_index.is_some() {
            let seek_time = if self.state.progress > 0.0 {
                self.queue.current_song().and_then(|s| {
                    let secs = self.state.progress * (s.duration as f64 / 1000.0);
                    (secs > 0.0).then(|| Duration::from_secs_f64(secs))
                })
            } else {
                None
            };
            self.start_current_song(seek_time);
            return;
        }
        if self.state.paused {
            self.controller.resume();
        } else {
            self.controller.pause();
        }
        self.state.paused = !self.state.paused;
    }

    pub fn stop(&mut self) {
        self.controller.stop();
        self.queue.current_index = None;
        self.state.playing = false;
        self.state.paused = false;
        self.state.current_song = None;
        self.state.progress = 0.0;
    }

    pub fn clear_queue(&mut self) {
        // Only stop the current playback when the cleared queue is the one the
        // playing song came from. Clearing a different queue (viewed on the
        // Playlist page after switching) must not kill the current song.
        let playing_from_this_queue = self
            .queue
            .current_song()
            .zip(self.state.current_song.as_ref())
            .is_some_and(|(a, b)| a.id == b.id);
        self.queue = PlaylistQueue::new();
        self.strategy = mode::create_strategy(&self.state.mode, 0, None);
        if playing_from_this_queue {
            self.controller.stop();
            self.state.playing = false;
            self.state.paused = false;
            self.state.current_song = None;
            self.state.progress = 0.0;
        }
        if let Ok(mut registry) = self.source.sonar_songs.lock() {
            registry.clear();
        }
        if !self.active_queue_id.is_empty() {
            self.storage
                .delete_queue(&self.active_queue_id, &self.active_queue_key);
            self.queue_keys_cache
                .retain(|k| k != &self.active_queue_key);
            self.queue_entries_cache
                .retain(|(i, _)| i != &self.active_queue_id);
            self.active_queue_id.clear();
            self.active_queue_key.clear();
        }
        self.playing_queue_key.clear();
    }
    pub fn seek_relative(&mut self, delta_secs: f64) {
        let duration = match self.queue.current_song() {
            Some(s) => s.duration,
            None => return,
        };
        let total_secs = duration as f64 / 1000.0;
        if total_secs <= 0.0 {
            return;
        }

        if !self.state.playing {
            let cur_secs = self.state.progress * total_secs;
            self.state.progress =
                ((cur_secs + delta_secs).clamp(0.0, total_secs) / total_secs).clamp(0.0, 1.0);
            return;
        }

        let cur_secs = self.state.progress * total_secs;
        let new_secs = (cur_secs + delta_secs).clamp(0.0, total_secs);
        self.state.progress = new_secs / total_secs;
        self.state.seeking = true;
        self.controller.seek_to(Duration::from_secs_f64(new_secs));
    }

    pub fn set_volume(&mut self, volume: f64) {
        self.state.volume = volume;
        self.controller.set_volume(volume as f32);
    }

    pub fn set_playlist_id(&mut self, id: u64) {
        self.playlist_id = Some(id);
    }

    pub fn cycle_mode(&mut self) -> PlayMode {
        let next = match self.state.mode {
            PlayMode::Sequential => PlayMode::RepeatOne,
            PlayMode::RepeatOne => PlayMode::RepeatAll,
            PlayMode::RepeatAll => PlayMode::Shuffle,
            PlayMode::Shuffle => match self.playlist_id {
                Some(pid) if pid > 0 => PlayMode::Heartbeat { playlist_id: pid },
                _ => PlayMode::Sequential,
            },
            PlayMode::Heartbeat { .. } => PlayMode::Sequential,
        };
        self.set_mode(next.clone());
        next
    }

    pub fn set_mode(&mut self, mode: PlayMode) {
        self.state.mode = mode;
        self.strategy =
            mode::create_strategy(&self.state.mode, self.queue.len(), self.queue.current_index);
    }

    pub fn handle_finished(&mut self) {
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
        //todo 这种实现可能存在问题，重写
        // buffer underrun/overrun is transient — rodio recovers automatically
        if err.contains("buffer underrun") || err.contains("overrun") {
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

    pub fn save_session(&self) {
        self.persist_active_queue_blocking();
        self.source.cache.cleanup_index();
        self.source.cache.flush_index();
    }

    fn restore_session(&mut self) {
        if let Some(id) = self.storage.load_active_id() {
            let display = self
                .storage
                .display_for_id(&id)
                .unwrap_or_else(|| id.clone());
            if let Some(saved) = self.storage.load_queue_by_id(&id)
                && !saved.queue.is_empty()
            {
                self.active_queue_id = id;
                self.active_queue_key = display;
                self.queue = PlaylistQueue::from_parts(
                    saved.queue.into_iter().map(Arc::new).collect(),
                    saved.history,
                    saved.current_index,
                );
                self.state.volume = saved.volume;
                self.strategy =
                    mode::create_strategy(&saved.mode, self.queue.len(), self.queue.current_index);
                self.state.mode = saved.mode;
                self.controller.set_volume(saved.volume as f32);

                if saved.current_index.is_some() {
                    self.state.current_song = self.queue.current_song().cloned();
                    self.state.progress = saved.progress;
                }
            }
        }
        self.refresh_queue_keys();
    }

    pub(super) fn start_current_song(&mut self, seek_time: Option<Duration>) {
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

        #[cfg(target_os = "linux")]
        let song_id = song.id;
        tokio::spawn(async move {
            #[cfg(target_os = "linux")]
            log::info!(
                "[HEAP] before resolve {} (id={}): {} kB",
                song.name,
                song_id,
                mem_rss_kb()
            );
            let input = match source.resolve(&song).await {
                Ok(input) => input,
                Err(e) => {
                    #[cfg(target_os = "linux")]
                    log::info!(
                        "[HEAP] after resolve FAIL {} (id={}): {} kB",
                        song.name,
                        song_id,
                        mem_rss_kb()
                    );
                    #[cfg(target_os = "linux")]
                    unsafe {
                        libc::malloc_trim(0);
                    }
                    if event_tx.send(PlaybackEvent::Error(e).into()).is_err() {
                        log::error!("Failed to send PlaybackError: receiver dropped");
                    }
                    return;
                }
            };
            #[cfg(target_os = "linux")]
            log::info!(
                "[HEAP] after resolve OK {} (id={}): {} kB",
                song.name,
                song_id,
                mem_rss_kb()
            );
            #[cfg(target_os = "linux")]
            unsafe {
                libc::malloc_trim(0);
            }
            controller.request(input, seek_time);
        });
    }
}
