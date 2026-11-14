//! `PlaylistStorage`: queue-file naming, loading, listing, saving and deletion.

use std::{fs, path::PathBuf, sync::Arc};

use ncm_api::SongInfo;

use super::{
    super::PlayMode,
    model::{SavedQueue, SavedQueueRef},
    queues::{
        NCM_SEARCH_FILE, NCM_SEARCH_ID, THIRD_PARTY_FILE, THIRD_PARTY_ID, special_queue,
        special_queue_by_id,
    },
    write::SaveGate,
};
use crate::persist::sanitize_filename;

pub(crate) struct PlaylistStorage {
    pub(super) base_dir: PathBuf,
    pub(super) save_gate: Arc<SaveGate>,
}

impl PlaylistStorage {
    /// `base_dir` is the pigma cache root (`~/.cache/pigma`); playlists live in
    /// its `playlists/` subdirectory.
    pub(crate) fn new(base_dir: PathBuf) -> Self {
        let base_dir = base_dir.join("playlists");
        let _ = fs::create_dir_all(&base_dir);
        Self {
            base_dir,
            save_gate: Arc::new(SaveGate::default()),
        }
    }

    /// Records which per-key queue was active at last shutdown.
    pub(crate) fn active_path(&self) -> PathBuf {
        self.base_dir.join("active_queue.txt")
    }

    /// Stable id for a queue: `q_<fnv1a64(key)>`, or a fixed id for the unified
    /// search queues. This is the real index used by `active_queue.txt` and for
    /// loading/saving; display names are only for humans and must never be
    /// re-hashed as the id.
    pub(crate) fn queue_id(key: &str) -> String {
        if let Some((id, _)) = special_queue(key) {
            id.to_string()
        } else {
            format!("q_{}", Self::queue_hash(key))
        }
    }

    /// File for a queue, `q_<hash>-<display>.json`. The `q_<hash>` prefix is
    /// the real index; the display suffix is only for humans. Search queues
    /// always land in their single shared file.
    pub(crate) fn queue_path(&self, id: &str, display: &str) -> PathBuf {
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
    pub(crate) fn list_queues(&self) -> Vec<(String, String)> {
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
    pub(crate) fn display_for_id(&self, id: &str) -> Option<String> {
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
    pub(crate) fn load_active_id(&self) -> Option<String> {
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
    // Persisted queue snapshots carry every resumable playback field.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn save_queue(
        &self,
        id: &str,
        display: &str,
        queue: &[Arc<SongInfo>],
        history: &[u64],
        current_index: Option<usize>,
        mode: &PlayMode,
        volume: f64,
        speed: f64,
        pitch_locked: bool,
        progress: f64,
    ) {
        if id.is_empty() {
            return;
        }
        // Take the sequence before snapshotting: save order must follow call
        // order even if two callers ever race.
        let seq = self.save_gate.next_seq();
        // Move owned (cheap Arc) copies into the blocking task so both the
        // JSON serialization and the disk writes run off the UI thread.
        let queue = queue.to_vec();
        let history = history.to_vec();
        let mode = *mode;
        let queue_path = self.queue_path(id, display);
        let active_path = self.active_path();
        let id = id.to_string();
        let gate = Arc::clone(&self.save_gate);
        tokio::task::spawn_blocking(move || {
            // A newer save already reached disk; skip the (expensive)
            // serialization of a snapshot that can never be used.
            if gate.is_stale(seq) {
                return;
            }
            let saved = SavedQueueRef {
                queue: &queue,
                history: &history,
                current_index,
                mode: &mode,
                volume,
                speed,
                pitch_locked,
                progress,
            };
            if let Ok(json) = serde_json::to_string(&saved) {
                gate.commit(seq, &queue_path, &active_path, &id, &json);
            }
        });
    }

    /// Blocking variant of [`Self::save_queue`], used on the shutdown path where
    /// a detached `spawn_blocking` task might not run before the runtime is torn
    /// down. Serializes then writes on the calling thread.
    // Same resumable playback fields as `save_queue`, without the background task.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn save_queue_sync(
        &self,
        id: &str,
        display: &str,
        queue: &[Arc<SongInfo>],
        history: &[u64],
        current_index: Option<usize>,
        mode: &PlayMode,
        volume: f64,
        speed: f64,
        pitch_locked: bool,
        progress: f64,
    ) {
        if id.is_empty() {
            return;
        }
        let seq = self.save_gate.next_seq();
        let saved = SavedQueueRef {
            queue,
            history,
            current_index,
            mode,
            volume,
            speed,
            pitch_locked,
            progress,
        };
        let queue_path = self.queue_path(id, display);
        let active_path = self.active_path();
        if let Ok(json) = serde_json::to_string(&saved) {
            self.save_gate
                .commit(seq, &queue_path, &active_path, id, &json);
        }
    }

    /// Load a queue by its canonical id (returns `None` when unknown).
    pub(crate) fn load_queue_by_id(&self, id: &str) -> Option<SavedQueue> {
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
    ///
    /// Synchronous deletion: the caller (e.g. `clear_queue`) immediately calls
    /// `refresh_queue_keys()` to rescan from disk; if this deleted asynchronously via
    /// `spawn_blocking`, the file might still exist during the scan and cause a just-cleared
    /// queue tab to reappear.
    pub(crate) fn delete_queue(&self, id: &str, display: &str) {
        if id.is_empty() {
            return;
        }
        let queue_path = self.queue_path(id, display);
        let active_path = self.active_path();
        let _ = fs::remove_file(&queue_path);
        if fs::read_to_string(&active_path).ok().as_deref() == Some(id) {
            let _ = fs::remove_file(&active_path);
        }
    }
}
