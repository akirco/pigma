//! Queue identity, persistence and session restore for [`PlaybackEngine`]:
//! activating/switching queues, on-disk playlist storage and startup restore.

use std::sync::Arc;

use super::{PlaybackEngine, mode};
use crate::{
    playback::{queue::PlaylistQueue, storage::PlaylistStorage},
    utils::time::local_month_day,
};

impl PlaybackEngine {
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
    fn refresh_queue_keys(&mut self) {
        let mut entries = std::mem::take(&mut self.queue_entries_cache);
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
    pub(super) fn persist_active_queue(&self) {
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
        self.activate_by_id(key, &id, true);
    }

    /// Activate a queue by its canonical id and display name. Playback state is
    /// untouched — this only swaps which queue the engine operates on.
    ///
    /// When `persist_previous` is `true`, the previously loaded queue is saved
    /// first. Callers that persist explicitly (e.g. `append_songs_to_key`, which
    /// must save the target queue after appending) pass `false` to avoid a
    /// redundant full-queue serialization of an unchanged queue.
    pub(super) fn activate_by_id(&mut self, display: &str, id: &str, persist_previous: bool) {
        if self.active_queue_id == id {
            return;
        }
        if persist_previous {
            self.persist_active_queue();
        }
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
        let entries = &self.queue_entries_cache;
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
        let (next_display, next_id) = (next.1.clone(), next.0.clone());
        self.activate_by_id(&next_display, &next_id, true);
        Some(next_display)
    }

    /// Append today's `MM-DD` to a breadcrumb context, producing the full queue
    /// key that is stored in the queue file (and shown as its tab). Time-based
    /// content like 每日推荐 therefore gets one queue per day; replaying the
    /// same context on the same day reuses the existing queue instead of
    /// creating a new one.
    pub(super) fn dated_key(&self, context: &str) -> String {
        format!("{context} {}", local_month_day())
    }

    /// The dated queue key `context` maps to (same derivation as `play_songs`),
    /// so background tasks can address the exact queue being played.
    pub fn queue_key_for(&self, context: &str) -> String {
        self.dated_key(context)
    }

    pub fn set_playlist_id(&mut self, id: u64) {
        self.playlist_id = Some(id);
    }

    pub fn save_session(&self) {
        self.persist_active_queue_blocking();
        self.source.cache.cleanup_index();
        self.source.cache.flush_index();
    }

    /// Restore the last active queue (songs, mode, volume, progress) from disk
    /// at engine construction.
    pub(super) fn restore_session(&mut self) {
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
        self.update_liked_status();
    }
}
