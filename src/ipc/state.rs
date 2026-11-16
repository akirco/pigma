//! Live IPC snapshot state: the status/queue snapshots served to `pigma
//! status`, the broadcast fan-out for `subscribe` clients, and the rebuild
//! logic that keeps them current from the playback engine.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use super::snapshot::{QueueEntry, QueueSnapshot, StatusSnapshot};
use crate::{lock::lock_recover, playback::PlaybackEngine};

pub(crate) struct IpcState {
    /// Live playback snapshot served to `pigma status` over the IPC socket.
    pub(crate) status: Arc<Mutex<StatusSnapshot>>,
    /// Live playback queue served to `pigma status -L` over the IPC socket.
    pub(crate) queue: Arc<Mutex<QueueSnapshot>>,
    /// Fan-out channel for snapshot changes, consumed by the IPC `subscribe`
    /// handler so long-running clients get event push.
    pub(crate) status_tx: tokio::sync::broadcast::Sender<StatusSnapshot>,
    /// When the last snapshot was broadcast; discrete changes fire immediately,
    /// position refresh (playing) is throttled to once per second.
    last_broadcast: Instant,
    /// Last queue version the `queue` snapshot was built from; rebuilds only on
    /// change instead of cloning the whole queue every event-loop iteration.
    last_queue_version: u64,
}

impl IpcState {
    pub(crate) fn new() -> Self {
        let (status_tx, _status_rx) = tokio::sync::broadcast::channel(16);
        Self {
            status: Arc::new(Mutex::new(StatusSnapshot::default())),
            queue: Arc::new(Mutex::new(QueueSnapshot::default())),
            status_tx,
            last_broadcast: Instant::now(),
            // Force the first `update` to populate the queue, e.g. when a
            // session is restored from disk during engine startup.
            last_queue_version: u64::MAX,
        }
    }

    /// Refresh the status snapshot from the live playback state. The status
    /// (current song + progress) is cheap and rebuilt each loop; the full queue
    /// listing is only rebuilt when the queue actually changed.
    ///
    /// A snapshot change is broadcast to IPC `subscribe` clients immediately;
    /// while a track is playing the position also advances, but that refresh is
    /// throttled to once per second so the daemon does not spam subscribers at
    /// the event-loop rate.
    pub(crate) fn update(&mut self, playback: &PlaybackEngine) {
        let state = &playback.state;
        let position_ms = state
            .current_song
            .as_ref()
            .map(|s| (state.progress * s.duration as f64) as u64)
            .unwrap_or(0);
        let elapsed = self.last_broadcast.elapsed();
        let position_stale = state.playing && !state.paused && elapsed >= Duration::from_secs(1);

        // Compare against the playback state directly, so the common
        // "only the position moved" path does not clone name/artist/album.
        let changed = {
            let mut stored = lock_recover(&self.status);
            let changed = !stored.matches_playback(state);
            stored.position_ms = position_ms;
            changed
        };
        if changed || position_stale {
            let snapshot = StatusSnapshot::from_playback(state);
            if changed {
                *lock_recover(&self.status) = snapshot.clone();
            }
            self.last_broadcast = Instant::now();
            let _ = self.status_tx.send(snapshot);
        }
        let version = playback.queue_version();
        if version != self.last_queue_version {
            self.last_queue_version = version;
            *lock_recover(&self.queue) = QueueSnapshot {
                key: playback.queue_key().to_string(),
                current_index: playback.queue_current_index(),
                songs: playback
                    .queue_songs()
                    .iter()
                    .map(|s| QueueEntry::from_song(s))
                    .collect(),
            };
        }
    }
}
