use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use ncm_api::SongInfo;
use serde::{Deserialize, Serialize};

use super::PlayMode;
use super::{NCM_SEARCH_QUEUE_KEY, THIRD_PARTY_QUEUE_KEY};

use crate::utils::sanitize_filename;

/// File name for the unified NCM search queue.
pub const NCM_SEARCH_FILE: &str = "ncm_search.json";
/// Stable id for the NCM search queue: a single shared file, no date/hash.
const NCM_SEARCH_ID: &str = "q_ncm_search";

/// File name for the unified third-party (sonar) search queue.
pub const THIRD_PARTY_FILE: &str = "thirdparty_source.json";
/// Stable id for the third-party queue: a single shared file, no date/hash.
const THIRD_PARTY_ID: &str = "q_thirdparty";

/// Map a fixed search-queue key to its stable `(id, file)` pair.
fn special_queue(key: &str) -> Option<(&'static str, &'static str)> {
    match key {
        NCM_SEARCH_QUEUE_KEY => Some((NCM_SEARCH_ID, NCM_SEARCH_FILE)),
        THIRD_PARTY_QUEUE_KEY => Some((THIRD_PARTY_ID, THIRD_PARTY_FILE)),
        _ => None,
    }
}

/// Reverse lookup: a fixed queue `id` to its `(display name, file)` pair.
fn special_queue_by_id(id: &str) -> Option<(&'static str, &'static str)> {
    match id {
        NCM_SEARCH_ID => Some((NCM_SEARCH_QUEUE_KEY, NCM_SEARCH_FILE)),
        THIRD_PARTY_ID => Some((THIRD_PARTY_QUEUE_KEY, THIRD_PARTY_FILE)),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedQueue {
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
    #[serde(default)]
    pub progress: f64,
}

/// Deserialize the on-disk `history` field, which may be either the new
/// compact format (`[u64]`) or the legacy format (`[SongInfo]`). In the legacy
/// case we keep only the song ids; if a value is malformed we drop it so the
/// rest of the queue still loads.
fn deserialize_history<'de, D>(deserializer: D) -> Result<Vec<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;

    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Array(items) => {
            let mut ids = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    serde_json::Value::Number(n) => {
                        if let Some(id) = n.as_u64() {
                            ids.push(id);
                        }
                    }
                    serde_json::Value::Object(obj) => {
                        if let Some(serde_json::Value::Number(n)) = obj.get("id")
                            && let Some(id) = n.as_u64()
                        {
                            ids.push(id);
                        }
                    }
                    _ => {}
                }
            }
            Ok(ids)
        }
        _ => Err(Error::custom("history must be an array")),
    }
}

struct SavedQueueRef<'a> {
    queue: &'a [Arc<SongInfo>],
    history: &'a [u64],
    current_index: Option<usize>,
    mode: &'a PlayMode,
    volume: f64,
    progress: f64,
}

impl Serialize for SavedQueueRef<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("SavedQueueRef", 6)?;
        let queue: Vec<&SongInfo> = self.queue.iter().map(|s| s.as_ref()).collect();
        s.serialize_field("queue", &queue)?;
        s.serialize_field("history", &self.history)?;
        s.serialize_field("current_index", &self.current_index)?;
        s.serialize_field("mode", self.mode)?;
        s.serialize_field("volume", &self.volume)?;
        s.serialize_field("progress", &self.progress)?;
        s.end()
    }
}

pub struct PlaylistStorage {
    base_dir: PathBuf,
}

impl PlaylistStorage {
    /// `base_dir` is the pigma cache root (`~/.cache/pigma`); playlists live in
    /// its `playlists/` subdirectory.
    pub fn new(base_dir: PathBuf) -> Self {
        let base_dir = base_dir.join("playlists");
        let _ = fs::create_dir_all(&base_dir);
        Self { base_dir }
    }

    /// Records which per-key queue was active at last shutdown.
    fn active_path(&self) -> PathBuf {
        self.base_dir.join("active_queue.txt")
    }

    /// Stable id for a queue: `q_<fnv1a64(key)>`, or a fixed id for the unified
    /// search queues. This is the real index used by `active_queue.txt` and for
    /// loading/saving; display names are only for humans and must never be
    /// re-hashed as the id.
    pub fn queue_id(key: &str) -> String {
        if let Some((id, _)) = special_queue(key) {
            id.to_string()
        } else {
            format!("q_{}", Self::queue_hash(key))
        }
    }

    /// File for a queue, `q_<hash>-<display>.json`. The `q_<hash>` prefix is
    /// the real index; the display suffix is only for humans. Search queues
    /// always land in their single shared file.
    fn queue_path(&self, id: &str, display: &str) -> PathBuf {
        if let Some((_, file)) = special_queue_by_id(id) {
            return self.base_dir.join(file);
        }
        self.base_dir
            .join(format!("{id}-{}.json", sanitize_filename(display)))
    }

    fn queue_hash(key: &str) -> String {
        let mut hash: u64 = 0xcbf29ce484222325;
        for b in key.bytes() {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        format!("{hash:016x}")
    }

    /// All persisted queues as `(id, display)` pairs, for the Playlist page tab
    /// bar and queue switching. The id is canonical; display is for humans.
    pub fn list_queues(&self) -> Vec<(String, String)> {
        let mut queues = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.base_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let name = entry.file_name().to_string_lossy().to_string();
                if let Some(pair) = self.queue_id_display_from_name(&name) {
                    queues.push(pair);
                }
            }
        }
        for (id, file) in [NCM_SEARCH_ID, THIRD_PARTY_ID]
            .into_iter()
            .zip([NCM_SEARCH_FILE, THIRD_PARTY_FILE])
        {
            if self.base_dir.join(file).exists() {
                let display = special_queue_by_id(id).map(|(d, _)| d).unwrap_or(id);
                queues.push((id.to_string(), display.to_string()));
            }
        }
        queues.sort();
        queues
    }

    /// Parse `q_<hash>-<display>.json` into `(q_<hash>, display)`. Tolerates
    /// legacy `q_<hash>.json` files by reading their embedded key.
    fn queue_id_display_from_name(&self, name: &str) -> Option<(String, String)> {
        let rest = name.strip_prefix("q_")?.strip_suffix(".json")?;
        match rest.split_once('-') {
            Some((hash, display)) if !display.is_empty() => {
                Some((format!("q_{hash}"), display.to_string()))
            }
            _ => {
                let content = fs::read_to_string(self.base_dir.join(name)).ok()?;
                let saved: SavedQueue = serde_json::from_str(&content).ok()?;
                (!saved.key.is_empty()).then(|| (format!("q_{rest}"), saved.key))
            }
        }
    }

    /// Resolve a `q_<hash>` id to its display name (or a fixed search label).
    pub fn display_for_id(&self, id: &str) -> Option<String> {
        if let Some((display, file)) = special_queue_by_id(id) {
            return self
                .base_dir
                .join(file)
                .exists()
                .then(|| display.to_string());
        }
        let prefix = format!("{id}-");
        if let Ok(entries) = fs::read_dir(&self.base_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(&prefix) && name.ends_with(".json") {
                    return Some(name[prefix.len()..name.len() - 5].to_string());
                }
            }
        }
        let legacy = self.base_dir.join(format!("{id}.json"));
        let content = fs::read_to_string(legacy).ok()?;
        let saved: SavedQueue = serde_json::from_str(&content).ok()?;
        (!saved.key.is_empty()).then_some(saved.key)
    }

    /// The id that was active at last shutdown, if any. Tolerates legacy files
    /// that recorded the display name directly.
    pub fn load_active_id(&self) -> Option<String> {
        let path = self.active_path();
        if !path.exists() {
            return None;
        }
        let value = fs::read_to_string(&path).ok()?.trim().to_string();
        if value.is_empty() {
            return None;
        }
        if value.starts_with("q_") {
            Some(value)
        } else {
            Some(Self::queue_id(&value))
        }
    }

    /// Persist `id`'s queue (displayed as `display`) and record its id as the
    /// active one.
    #[allow(clippy::too_many_arguments)]
    pub fn save_queue(
        &self,
        id: &str,
        display: &str,
        queue: &[Arc<SongInfo>],
        history: &[u64],
        current_index: Option<usize>,
        mode: &PlayMode,
        volume: f64,
        progress: f64,
    ) {
        if id.is_empty() {
            return;
        }
        let saved = SavedQueueRef {
            queue,
            history,
            current_index,
            mode,
            volume,
            progress,
        };
        if let Ok(json) = serde_json::to_string(&saved) {
            let queue_path = self.queue_path(id, display);
            let active_path = self.active_path();
            let id = id.to_string();
            tokio::task::spawn_blocking(move || {
                let _ = fs::write(&queue_path, &json);
                let _ = fs::write(&active_path, id);
            });
        }
    }

    /// Blocking variant of [`Self::save_queue`], used on the shutdown path where
    /// a detached `spawn_blocking` task might not run before the runtime is torn
    /// down. Serializes then writes on the calling thread.
    #[allow(clippy::too_many_arguments)]
    pub fn save_queue_sync(
        &self,
        id: &str,
        display: &str,
        queue: &[Arc<SongInfo>],
        history: &[u64],
        current_index: Option<usize>,
        mode: &PlayMode,
        volume: f64,
        progress: f64,
    ) {
        if id.is_empty() {
            return;
        }
        let saved = SavedQueueRef {
            queue,
            history,
            current_index,
            mode,
            volume,
            progress,
        };
        if let Ok(json) = serde_json::to_string(&saved) {
            let _ = fs::write(self.queue_path(id, display), &json);
            let _ = fs::write(self.active_path(), id);
        }
    }

    /// Load a queue by its canonical id (returns `None` when unknown).
    pub fn load_queue_by_id(&self, id: &str) -> Option<SavedQueue> {
        let path = if let Some((_, file)) = special_queue_by_id(id) {
            self.base_dir.join(file)
        } else {
            let prefix = format!("{id}-");
            let mut found = None;
            if let Ok(entries) = fs::read_dir(&self.base_dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with(&prefix) && name.ends_with(".json") {
                        found = Some(entry.path());
                        break;
                    }
                }
            }
            // Legacy `q_<hash>.json` (pre display-suffix naming).
            found.unwrap_or_else(|| self.base_dir.join(format!("{id}.json")))
        };
        if !path.exists() {
            return None;
        }
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Remove a queue file. If it was the active queue, clear the
    /// `active_queue.txt` marker so a later restart doesn't point at it.
    pub fn delete_queue(&self, id: &str, display: &str) {
        if id.is_empty() {
            return;
        }
        let queue_path = self.queue_path(id, display);
        let active_path = self.active_path();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let _ = fs::remove_file(&queue_path);
            if fs::read_to_string(&active_path).ok().as_deref() == Some(id.as_str()) {
                let _ = fs::remove_file(&active_path);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_history_accepts_new_format() {
        let json = r#"{
            "queue": [],
            "history": [1, 2, 3],
            "current_index": null,
            "mode": "Sequential",
            "volume": 50.0
        }"#;
        let saved: SavedQueue = serde_json::from_str(json).unwrap();
        assert_eq!(saved.history, vec![1, 2, 3]);
    }

    #[test]
    fn deserialize_history_accepts_legacy_format() {
        let json = r#"{
            "queue": [],
            "history": [{"id": 11, "name": "a"}, {"id": 22, "name": "b"}],
            "current_index": null,
            "mode": "Sequential",
            "volume": 50.0
        }"#;
        let saved: SavedQueue = serde_json::from_str(json).unwrap();
        assert_eq!(saved.history, vec![11, 22]);
    }
}
