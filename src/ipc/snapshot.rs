//! Snapshot models served to `pigma status` / `pigma msg search` clients.

use serde::{Deserialize, Serialize};

/// Live playback state snapshot served to `pigma status`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusSnapshot {
    pub id: u64,
    pub name: String,
    pub artist: String,
    pub album: String,
    /// Total length in milliseconds (0 when nothing is loaded).
    pub duration_ms: u64,
    /// Playback position in milliseconds.
    pub position_ms: u64,
    /// Volume as a fraction of 0..=1.
    pub volume: f64,
    /// Playback speed factor (1.0 = normal).
    pub speed: f64,
    /// Whether the speed change keeps the pitch (WSOLA stretch, badge `*`).
    pub pitch_locked: bool,
    pub playing: bool,
    pub paused: bool,
    /// Stable play-mode key: `sequential` / `repeat_one` / `repeat_all` /
    /// `shuffle` / `heartbeat`.
    pub mode: String,
    pub liked: bool,
}

/// A brand-new snapshot has no track loaded, but the speed must still read as
/// normal (1.0) instead of the derived default 0.0.
impl Default for StatusSnapshot {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            artist: String::new(),
            album: String::new(),
            duration_ms: 0,
            position_ms: 0,
            volume: 0.0,
            speed: 1.0,
            pitch_locked: true,
            playing: false,
            paused: false,
            mode: String::new(),
            liked: false,
        }
    }
}

impl StatusSnapshot {
    /// Whether `self` differs from `other` in any field that is *not* the
    /// playback position. The app loop compares snapshots with this before
    /// deciding to broadcast, so a running track does not spam subscribers on
    /// every progress tick — position refreshes are instead throttled by time.
    pub fn meaningfully_differs(&self, other: &Self) -> bool {
        self.id != other.id
            || self.name != other.name
            || self.artist != other.artist
            || self.album != other.album
            || self.duration_ms != other.duration_ms
            || self.volume != other.volume
            || self.speed != other.speed
            || self.pitch_locked != other.pitch_locked
            || self.playing != other.playing
            || self.paused != other.paused
            || self.mode != other.mode
            || self.liked != other.liked
    }
}

/// A single entry in the playback queue, served to `pigma status -L`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueEntry {
    pub id: u64,
    pub name: String,
    pub singer: String,
    pub album: String,
    pub duration_ms: u64,
}

/// Full queue listing served to `pigma status -L`: the active queue's display
/// key, the current song's index (0-based, `None` when nothing is queued) and
/// the songs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueueSnapshot {
    /// Display key of the active queue — the queue that `next` / `pigma msg
    /// play <id>` operate on. Changes when the queue is switched.
    pub key: String,
    pub current_index: Option<usize>,
    pub songs: Vec<QueueEntry>,
}

/// A search hit served to `pigma msg search <keyword>`. `source` tags the
/// provider: `netease` for NetEase Cloud, otherwise the sonar provider name
/// (`kugou` / `kuwo` / `bilivideo` / `youtube`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchEntry {
    pub id: u64,
    pub name: String,
    pub singer: String,
    pub album: String,
    pub duration_ms: u64,
    pub source: String,
}
