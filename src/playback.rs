mod controller;
mod heartbeat;
mod mode;
mod player;
mod queue;
mod scan;
mod source;
mod storage;
pub mod types;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ncm_api::{NcmClient, SongInfo};
use tokio::sync::mpsc;

use crate::event::{AppEvent, Event};

use self::controller::PlaybackHandle;
use self::mode::PlayStrategy;
use self::queue::PlaylistQueue;
use self::source::AudioSource;
use self::storage::PlaylistStorage;
use self::types::{LyricLine, PlayMode, PlaybackState};

pub use self::types::{PlayMode as EnginePlayMode, PlaybackState as EngineState};

pub struct PlaybackEngine {
    pub state: PlaybackState,
    pub(super) queue: PlaylistQueue,
    strategy: Box<dyn PlayStrategy>,
    storage: PlaylistStorage,
    source: AudioSource,
    pub(super) controller: PlaybackHandle,
    pub(super) event_tx: mpsc::UnboundedSender<Event>,
    pub(super) api: Arc<NcmClient>,
    playlist_id: Option<u64>,
    consecutive_errors: u32,
}

impl PlaybackEngine {
    pub fn new(event_tx: mpsc::UnboundedSender<Event>, api: Arc<NcmClient>) -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("pigma")
            .join("songs");

        let storage = PlaylistStorage::new();
        let mut this = Self {
            state: PlaybackState::default(),
            queue: PlaylistQueue::new(),
            strategy: Box::new(mode::Sequential),
            storage,
            source: AudioSource::new(api.clone(), cache_dir),
            controller: PlaybackHandle::new(event_tx.clone()),
            event_tx: event_tx.clone(),
            api,
            playlist_id: None,
            consecutive_errors: 0,
        };
        this.restore_session();
        this
    }

    pub fn current_song(&self) -> Option<SongInfo> {
        self.queue.current_song().cloned()
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

    pub fn song_at(&self, index: usize) -> Option<&SongInfo> {
        self.queue.songs.get(index)
    }

    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    pub fn queue_songs(&self) -> &[SongInfo] {
        &self.queue.songs
    }

    pub fn queue_history(&self) -> &[SongInfo] {
        &self.queue.history
    }

    pub fn queue_current_index(&self) -> Option<usize> {
        self.queue.current_index
    }

    pub fn set_queue_songs(&mut self, songs: Vec<SongInfo>) {
        self.queue.songs = songs;
    }

    pub fn cache(&self) -> &crate::cache::CacheManager {
        &self.source.cache
    }

    pub fn play_songs(&mut self, songs: Vec<SongInfo>, index: usize) {
        if songs.is_empty() || index >= songs.len() {
            return;
        }
        self.controller.stop();
        self.queue = PlaylistQueue::from_songs(songs, index);
        self.strategy =
            mode::create_strategy(&self.state.mode, self.queue.len(), self.queue.current_index);
        self.start_current_song(None);
    }

    pub fn append_and_play(&mut self, songs: &[SongInfo], index: usize) {
        if songs.is_empty() || index >= songs.len() {
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
        self.queue.advance_to(index);
        self.strategy =
            mode::create_strategy(&self.state.mode, self.queue.len(), self.queue.current_index);
        self.start_current_song(None);
    }

    pub fn next(&mut self) {
        if self.queue.is_empty() {
            return;
        }

        if matches!(self.state.mode, PlayMode::Heartbeat { .. }) {
            self.next_heartbeat();
            return;
        }

        match self.queue.next_index(&mut *self.strategy) {
            Some(idx) => {
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

        if let Some(prev_song) = self.queue.pop_history()
            && let Some(pos) = self.queue.find_song_index(prev_song.id)
        {
            self.queue.current_index = Some(pos);
            self.start_current_song(None);
            return;
        }

        if let Some(idx) = self.queue.prev_index(&mut *self.strategy) {
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

    pub fn volume(&self) -> f64 {
        self.state.volume
    }

    pub fn set_playlist_id(&mut self, id: u64) {
        self.playlist_id = Some(id);
    }

    pub fn cycle_mode(&mut self) {
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
        self.set_mode(next);
    }

    pub fn set_mode(&mut self, mode: PlayMode) {
        self.state.mode = mode.clone();
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

    pub fn on_playback_finished(&mut self) {
        self.handle_finished();
    }

    pub fn on_playback_error(&mut self, err: String) {
        self.state.on_error(err);
        self.consecutive_errors += 1;
        if self.consecutive_errors >= 3 {
            self.stop();
        } else {
            self.next();
        }
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
        self.storage.save_auto(
            &self.queue.songs,
            &self.queue.history,
            self.queue.current_index,
            &self.state.mode,
            self.state.volume,
            self.state.progress,
        );
    }

    pub fn save_playlist(&self, name: &str) -> bool {
        self.storage.save_playlist(name, &self.queue.songs)
    }

    pub fn load_playlist(&mut self, name: &str) -> bool {
        match self.storage.load_playlist(name) {
            Some(songs) if !songs.is_empty() => {
                self.play_songs(songs, 0);
                true
            }
            _ => false,
        }
    }

    pub fn delete_playlist(&self, name: &str) -> bool {
        self.storage.delete_playlist(name)
    }

    pub fn list_playlists(&self) -> Vec<String> {
        self.storage.list_playlists()
    }

    fn restore_session(&mut self) {
        if let Some(saved) = self.storage.load_auto()
            && !saved.queue.is_empty()
        {
            self.queue = PlaylistQueue {
                songs: saved.queue,
                history: saved.history,
                current_index: saved.current_index,
            };
            self.state.mode = saved.mode.clone();
            self.state.volume = saved.volume;
            self.strategy =
                mode::create_strategy(&saved.mode, self.queue.len(), self.queue.current_index);
            self.controller.set_volume(saved.volume as f32);

            if saved.current_index.is_some() {
                self.state.current_song = self.queue.current_song().cloned().map(Arc::new);
                self.state.progress = saved.progress;
            }
        }
    }

    pub(super) fn start_current_song(&mut self, seek_time: Option<Duration>) {
        let song = match self.queue.current_song() {
            Some(s) => Arc::new(s.clone()),
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

        if event_tx
            .send(Event::App(AppEvent::PlaybackStarted))
            .is_err()
        {
            log::error!("Failed to send PlaybackStarted: receiver dropped");
        }

        tokio::spawn(async move {
            let input = match source.resolve(&song).await {
                Ok(input) => input,
                Err(e) => {
                    if event_tx
                        .send(Event::App(AppEvent::PlaybackError(e)))
                        .is_err()
                    {
                        log::error!("Failed to send PlaybackError: receiver dropped");
                    }
                    return;
                }
            };
            controller.request(input, seek_time);
        });
    }

}

pub use self::scan::scan_local_music;
