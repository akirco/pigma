//! On-disk queue snapshot model and its serde helpers (including legacy
//! `history` formats).

use std::sync::Arc;

use ncm_api::SongInfo;
use serde::{Deserialize, Deserializer, Serialize, de::IgnoredAny};

use super::super::PlayMode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SavedQueue {
    /// Legacy display key, only present in queues written before the
    /// `q_<hash>-<display>.json` naming was introduced. New files keep the
    /// display name in the file name instead.
    #[serde(default)]
    pub key: String,
    pub queue: Vec<SongInfo>,
    #[serde(deserialize_with = "deserialize_history")]
    pub history: Vec<u64>,
    pub current_index: Option<usize>,
    pub mode: PlayMode,
    pub volume: f64,
    /// Persisted playback speed factor. Absent in files written before speed
    /// support, where the default is normal (1.0x).
    #[serde(default = "default_speed")]
    pub speed: f64,
    /// Whether the speed keeps the pitch (WSOLA). Absent in files written
    /// before the mode existed, where pitch keeping is the default.
    #[serde(default = "default_true")]
    pub pitch_locked: bool,
    #[serde(default)]
    pub progress: f64,
}

fn default_speed() -> f64 {
    1.0
}

fn default_true() -> bool {
    true
}

/// Deserialize the on-disk `history` field, which may be either the new
/// compact format (`[u64]`) or the legacy format (`[SongInfo]`). In the legacy
/// case only the song ids are kept; malformed entries are dropped so the rest
/// of the queue still loads.
#[derive(Deserialize)]
#[serde(untagged)]
enum HistoryEntry {
    Id(u64),
    Song { id: u64 },
    Other(IgnoredAny),
}

fn deserialize_history<'de, D>(deserializer: D) -> Result<Vec<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    let entries = Vec::<HistoryEntry>::deserialize(deserializer)?;
    Ok(entries
        .into_iter()
        .filter_map(|entry| match entry {
            HistoryEntry::Id(id) | HistoryEntry::Song { id } => Some(id),
            HistoryEntry::Other(_) => None,
        })
        .collect())
}

#[derive(Serialize)]
pub(super) struct SavedQueueRef<'a> {
    pub(super) queue: &'a [Arc<SongInfo>],
    pub(super) history: &'a [u64],
    pub(super) current_index: Option<usize>,
    pub(super) mode: &'a PlayMode,
    pub(super) volume: f64,
    pub(super) speed: f64,
    pub(super) pitch_locked: bool,
    pub(super) progress: f64,
}
