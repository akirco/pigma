//! IPC snapshot state: the live status/queue snapshots served to `pigma
//! status` over the IPC socket, the broadcast fan-out for `subscribe`
//! clients, and the rebuild/broadcast logic that keeps them current.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::ipc::{QueueSnapshot, StatusSnapshot};

use super::App;

pub(super) struct IpcState {
    /// Live playback snapshot served to `pigma status` over the IPC socket.
    pub status: Arc<Mutex<StatusSnapshot>>,
    /// Live playback queue served to `pigma status -L` over the IPC socket.
    pub queue: Arc<Mutex<QueueSnapshot>>,
    /// Fan-out channel for snapshot changes, consumed by the IPC `subscribe`
    /// handler so long-running clients get event push.
    pub status_tx: tokio::sync::broadcast::Sender<StatusSnapshot>,
    /// When the last snapshot was broadcast; discrete changes fire immediately,
    /// position refresh (playing) is throttled to once per second.
    last_broadcast: Instant,
    /// Last queue version the `queue` snapshot was built from; rebuilds only on
    /// change instead of cloning the whole queue every event-loop iteration.
    last_queue_version: u64,
}

impl IpcState {
    pub(super) fn new() -> Self {
        let (status_tx, _status_rx) = tokio::sync::broadcast::channel(16);
        Self {
            status: Arc::new(Mutex::new(StatusSnapshot::default())),
            queue: Arc::new(Mutex::new(QueueSnapshot::default())),
            status_tx,
            last_broadcast: Instant::now(),
            // Force the first `update_status_snapshot` to populate the queue,
            // e.g. when a session is restored from disk during engine startup.
            last_queue_version: u64::MAX,
        }
    }
}

impl App {
    /// Refresh the IPC status snapshot from the live playback state. The status
    /// (current song + progress) is cheap and rebuilt each loop; the full queue
    /// listing is only rebuilt when the queue actually changed.
    ///
    /// A snapshot change is broadcast to IPC `subscribe` clients immediately;
    /// while a track is playing the position also advances, but that refresh is
    /// throttled to once per second so the daemon does not spam subscribers at
    /// the event-loop rate.
    pub(super) fn update_status_snapshot(&mut self) {
        let snapshot = StatusSnapshot::from_playback(&self.playback.state);
        let mut changed = false;
        if let Ok(mut stored) = self.ipc.status.lock() {
            changed = stored.meaningfully_differs(&snapshot);
            *stored = snapshot.clone();
        }
        let elapsed = self.ipc.last_broadcast.elapsed();
        let position_stale =
            snapshot.playing && !snapshot.paused && elapsed >= Duration::from_secs(1);
        if changed || position_stale {
            self.ipc.last_broadcast = Instant::now();
            let _ = self.ipc.status_tx.send(snapshot);
        }
        let version = self.playback.queue_version();
        if version != self.ipc.last_queue_version {
            self.ipc.last_queue_version = version;
            if let Ok(mut queue) = self.ipc.queue.lock() {
                *queue = QueueSnapshot {
                    current_index: self.playback.queue_current_index(),
                    songs: self
                        .playback
                        .queue_songs()
                        .iter()
                        .map(|s| crate::ipc::QueueEntry::from_song(s))
                        .collect(),
                };
            }
        }
    }
}
