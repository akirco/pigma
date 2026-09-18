//! Snapshot models served to `pigma status` / `pigma msg search` clients.

use serde::{Deserialize, Serialize};

use crate::playback::PlayMode;

/// Live playback state snapshot served to `pigma status`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    pub playing: bool,
    pub paused: bool,
    /// Stable play-mode key: `sequential` / `repeat_one` / `repeat_all` /
    /// `shuffle` / `heartbeat`.
    pub mode: String,
    pub liked: bool,
}

impl StatusSnapshot {
    pub fn from_playback(state: &crate::playback::PlaybackState) -> Self {
        let song = state.current_song.as_ref();
        let duration_ms = song.map(|s| s.duration).unwrap_or(0);
        let position_ms = song
            .map(|s| (state.progress * s.duration as f64) as u64)
            .unwrap_or(0);
        Self {
            id: song.map(|s| s.id).unwrap_or(0),
            name: song.map(|s| s.name.clone()).unwrap_or_default(),
            artist: song.map(|s| s.singer.clone()).unwrap_or_default(),
            album: song.map(|s| s.album.clone()).unwrap_or_default(),
            duration_ms,
            position_ms,
            volume: state.volume,
            playing: state.playing,
            paused: state.paused,
            mode: mode_key(&state.mode).to_string(),
            liked: state.liked,
        }
    }

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
            || self.playing != other.playing
            || self.paused != other.paused
            || self.mode != other.mode
            || self.liked != other.liked
    }
}

fn mode_key(mode: &PlayMode) -> &'static str {
    match mode {
        PlayMode::Sequential => "sequential",
        PlayMode::RepeatOne => "repeat_one",
        PlayMode::RepeatAll => "repeat_all",
        PlayMode::Shuffle => "shuffle",
        PlayMode::Heartbeat { .. } => "heartbeat",
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

impl QueueEntry {
    pub fn from_song(song: &ncm_api::SongInfo) -> Self {
        Self {
            id: song.id,
            name: song.name.clone(),
            singer: song.singer.clone(),
            album: song.album.clone(),
            duration_ms: song.duration,
        }
    }
}

/// Full queue listing served to `pigma status -L`: the current song's queue
/// index (0-based, `None` when nothing is queued) plus the songs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueueSnapshot {
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

impl SearchEntry {
    pub fn from_song(song: &ncm_api::SongInfo, source: &str) -> Self {
        Self {
            id: song.id,
            name: song.name.clone(),
            singer: song.singer.clone(),
            album: song.album.clone(),
            duration_ms: song.duration,
            source: source.to_string(),
        }
    }
}
