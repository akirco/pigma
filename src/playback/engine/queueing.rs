//! Queue operations and playback navigation for [`PlaybackEngine`]:
//! loading/switching songs, next/prev, pause toggling and queue mutation.

use std::{sync::Arc, time::Duration};

use ncm_api::SongInfo;

use super::{
    PlaybackEngine,
    mode::{self, PlayMode},
};
use crate::playback::{queue::PlaylistQueue, storage::PlaylistStorage};

impl PlaybackEngine {
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

    /// Monotonic version of the queue, bumped on every mutation. Callers that
    /// mirror the queue elsewhere (e.g. the IPC snapshot) can skip rebuilding
    /// when this is unchanged.
    pub fn queue_version(&self) -> u64 {
        self.queue.version()
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

    /// Replace the queue for `key` with `songs` and start playing `index`.
    pub fn play_songs(&mut self, key: &str, songs: Vec<Arc<SongInfo>>, index: usize) {
        self.load_songs(key, songs, index);
        self.start_current_song(None);
    }

    /// Load `songs` as the active queue without starting playback, so the first
    /// song is shown (paused/stopped) and can be started later via `play`/
    /// toggle. Same queue bookkeeping as `play_songs`, minus `start_current_song`.
    pub fn load_songs(&mut self, key: &str, songs: Vec<Arc<SongInfo>>, index: usize) {
        if songs.is_empty() || index >= songs.len() {
            return;
        }
        let key = self.dated_key(key);
        self.activate_queue(&key);
        self.playing_queue_key = self.active_queue_key.clone();
        self.controller.stop();
        self.queue = PlaylistQueue::from_songs(songs, index);
        self.strategy =
            mode::create_strategy(&self.state.mode, self.queue.len(), self.queue.current_index);
        self.state.current_song = self.queue.current_song().cloned();
        self.state.progress = 0.0;
        self.state.playing = false;
        self.state.paused = false;
        self.state.seeking = false;
        self.update_liked_status();
    }

    /// Append `songs` to the queue identified by `key` without interrupting
    /// playback. Used by lazy pagination: after `play_songs` seeds the queue
    /// with the first page, background pages are appended here. If `key` is not
    /// the active queue (the user switched elsewhere mid-load), the target
    /// queue is loaded, appended, persisted, then the previous one restored.
    /// Duplicate ids are skipped. `songs` are already shared `Arc`s, so no
    /// deep clone happens.
    pub fn append_songs_to_key(&mut self, key: &str, songs: Vec<Arc<SongInfo>>) -> bool {
        if songs.is_empty() {
            return false;
        }
        let id = PlaylistStorage::queue_id(key);
        if self.active_queue_id == id {
            self.queue.append(&songs);
            self.strategy =
                mode::create_strategy(&self.state.mode, self.queue.len(), self.queue.current_index);
            return true;
        }
        let prev = (self.active_queue_id.clone(), self.active_queue_key.clone());
        // Persist the previous queue once, then switch without a redundant
        // re-save. After appending, persist the target queue once, then switch
        // back without another redundant save (2 serializations total instead of 4).
        self.persist_active_queue();
        self.activate_by_id(key, &id, false);
        self.queue.append(&songs);
        self.strategy =
            mode::create_strategy(&self.state.mode, self.queue.len(), self.queue.current_index);
        self.persist_active_queue();
        if !prev.0.is_empty() {
            self.activate_by_id(&prev.1, &prev.0, false);
        }
        true
    }

    /// Append `songs` to a fixed, non-dated queue key and start playing
    /// `index`. Used by search (third-party & NCM) so all such songs share one
    /// queue instead of one per keyword/day. If the song is already in the
    /// queue, just play the existing entry instead of adding a duplicate
    /// (re-pressing Enter on a search result).
    pub fn append_and_play_key(&mut self, key: &str, songs: &[Arc<SongInfo>], index: usize) {
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

    /// Jump to the song with `song_id` in the active queue and start playing
    /// it. Returns `false` when the id is not in the queue (e.g. `pigma msg
    /// play <id>` targeting a song from another list).
    pub fn play_song_by_id(&mut self, song_id: u64) -> bool {
        let Some(index) = self.queue.find_song_index(song_id) else {
            return false;
        };
        self.play_index(index);
        true
    }

    /// Add `song` to the current queue right after the playing song.
    /// Keeps the rest of the queue intact. Does not interrupt the currently
    /// playing song.
    pub fn add_next(&mut self, song: Arc<SongInfo>) {
        self.queue.insert_next(vec![song]);
        self.persist_active_queue();
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
            self.queue.bump();
            self.start_current_song(None);
            return;
        }

        if let Some(idx) = self.queue.prev_index(&mut self.strategy) {
            self.controller.stop();
            self.queue.current_index = Some(idx);
            self.queue.bump();
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

    pub(super) fn stop(&mut self) {
        self.controller.stop();
        self.queue.current_index = None;
        self.state.playing = false;
        self.state.paused = false;
        self.state.current_song = None;
        self.state.progress = 0.0;
        // Playback has stopped; exit the 32ms seeking poll loop, otherwise the event loop
        // would spin forever.
        self.state.seeking = false;
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
            self.stop();
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
}
