use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use serde::{Deserialize, Serialize};

use super::{
    CacheManager,
    manager::{DEFAULT_MAX_CACHE_BYTES, cleanup_stale_partials},
};
use crate::time::unix_secs;

/// Entry in the audio cache index, mapping song ID to filename and duration.
#[derive(Clone, Deserialize, Serialize)]
pub(super) struct CacheEntry {
    pub(super) filename: String,
    #[serde(default)]
    pub(super) duration: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub(super) accessed_at: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(super) pic_url: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub(super) uploaded_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) thirdparty: Option<Arc<sonar::Song>>,
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

/// Saturating atomic subtraction, MSRV-safe (`AtomicU64::try_update` is newer
/// than our declared rust-version).
pub(super) fn saturating_sub_total(total: &AtomicU64, bytes: u64) {
    let mut current = total.load(Ordering::Relaxed);
    loop {
        let next = current.saturating_sub(bytes);
        match total.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(actual) => current = actual,
        }
    }
}

pub(super) type CacheIndex = HashMap<u64, CacheEntry>;

#[derive(Serialize, Default, Deserialize)]
pub(super) struct CacheIndexFile {
    #[serde(default)]
    pub(super) songs: HashMap<u64, CacheEntry>,
}

/// Borrowed view of [`CacheIndexFile`] used when serializing: avoids cloning the
/// entire index map just to write it to disk.
#[derive(Serialize)]
struct CacheIndexFileRef<'a> {
    songs: &'a HashMap<u64, CacheEntry>,
}

impl CacheManager {
    pub fn new(downloads_dir: PathBuf, base_dir: PathBuf, template: String) -> Self {
        let lyrics_dir = base_dir.join("lyrics");
        let covers_dir = base_dir.join("covers");
        cleanup_stale_partials(&downloads_dir);
        let index_file = Self::load_index_static(&downloads_dir)
            .unwrap_or_else(|| Self::rebuild_index_static(&downloads_dir, &template));
        let total = Self::compute_total_bytes(&downloads_dir, &index_file.songs);
        Self {
            downloads_dir,
            lyrics_dir,
            covers_dir,
            template,
            index: Arc::new(RwLock::new(index_file.songs)),
            max_cache_bytes: DEFAULT_MAX_CACHE_BYTES,
            cached_total_bytes: Arc::new(AtomicU64::new(total)),
            index_dirty: Arc::new(AtomicBool::new(false)),
            index_write_pending: Arc::new(AtomicBool::new(false)),
        }
    }

    fn compute_total_bytes(downloads_dir: &Path, index: &CacheIndex) -> u64 {
        index
            .values()
            .filter_map(|e| {
                let path = downloads_dir.join(&e.filename);
                fs::metadata(&path).ok().map(|m| m.len())
            })
            .sum()
    }

    fn index_path(dir: &Path) -> PathBuf {
        dir.join("cache_index.json")
    }

    /// Load the index. `None` means the file exists but could not be read or
    /// parsed; the caller then rebuilds it from the download directory instead
    /// of silently dropping every cached file.
    fn load_index_static(dir: &Path) -> Option<CacheIndexFile> {
        let path = Self::index_path(dir);
        let data = match fs::read_to_string(&path) {
            Ok(data) => data,
            // A missing index is treated like a corrupt one: rebuild from the
            // download directory so existing audio files are not orphaned.
            Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
            Err(e) => {
                log::warn!("Failed to read cache index {}: {e}", path.display());
                return None;
            }
        };
        match serde_json::from_str(&data) {
            Ok(index) => Some(index),
            Err(e) => {
                log::warn!("Failed to parse cache index {}: {e}", path.display());
                None
            }
        }
    }

    /// Best-effort index recovery after a corrupt/missing index. The song id is
    /// encoded in the filename, so it can be parsed for the common templates:
    /// `{id}` and any template without `{id}` (which appends `-<id>`).
    fn rebuild_index_static(dir: &Path, template: &str) -> CacheIndexFile {
        let mut songs = HashMap::new();
        let idless_template = !template.contains("{id}");
        if !idless_template && template != "{id}" {
            log::warn!(
                "Cannot recover cache ids for template `{template}`; downloads remain on disk unindexed"
            );
            return CacheIndexFile { songs };
        }

        let Ok(entries) = fs::read_dir(dir) else {
            return CacheIndexFile { songs };
        };
        let now = unix_secs();
        for entry in entries.flatten() {
            let path = entry.path();
            if !entry.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            let Some(filename) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if filename.starts_with('.') || filename == "cache_index.json" {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let id = if template == "{id}" {
                stem.parse::<u64>().ok()
            } else {
                stem.rsplit_once('-')
                    .and_then(|(_, id)| id.parse::<u64>().ok())
            };
            let Some(id) = id else {
                continue;
            };
            songs.insert(
                id,
                CacheEntry {
                    filename: filename.to_string(),
                    duration: 0,
                    accessed_at: now,
                    pic_url: String::new(),
                    uploaded_at: 0,
                    thirdparty: None,
                },
            );
        }
        if !songs.is_empty() {
            log::warn!("Recovered {} cache entries from disk", songs.len());
        }
        CacheIndexFile { songs }
    }

    /// Coalesced snapshot of the index to disk. Marks the index dirty and hands
    /// off to a single in-flight writer loop: a burst of mutations (e.g. many
    /// completed downloads) triggers only one write of the current state. Runs
    /// on a blocking task so a large index never stalls the caller (e.g. the
    /// stream-completion callback), and never drops a mutation: a writer loops
    /// until the dirty flag is clear, handing the guard off to anyone who
    /// mutates during the handoff window.
    pub(super) fn save_index(&self) {
        self.index_dirty.store(true, Ordering::SeqCst);
        if self.index_write_pending.swap(true, Ordering::SeqCst) {
            // A writer loop already owns the flag and will drain the dirty bit.
            return;
        }

        let index = Arc::clone(&self.index);
        let downloads_dir = self.downloads_dir.clone();
        let dirty = Arc::clone(&self.index_dirty);
        let pending = Arc::clone(&self.index_write_pending);
        tokio::task::spawn_blocking(move || {
            'writer: loop {
                Self::write_index(&index, &downloads_dir);
                dirty.store(false, Ordering::SeqCst);
                if dirty.swap(false, Ordering::SeqCst) {
                    // A mutation landed while we were writing; write again.
                    continue;
                }
                // Release the guard only after confirming no mutation sneaked in
                // between the last dirty check and the release. If one did and it
                // missed taking the guard, re-claim it and loop once more.
                pending.store(false, Ordering::SeqCst);
                if !dirty.load(Ordering::SeqCst) {
                    break 'writer;
                }
                if pending.swap(true, Ordering::SeqCst) {
                    break 'writer; // the mutator scheduled its own writer
                }
            }
        });
    }

    /// Blocking variant of [`Self::save_index`], used on the shutdown path where
    /// a detached `spawn_blocking` task might not run before the runtime is torn
    /// down. Serializes and writes on the calling thread.
    pub(super) fn save_index_sync(&self) {
        Self::write_index(&self.index, &self.downloads_dir);
    }

    fn write_index(index: &RwLock<CacheIndex>, downloads_dir: &Path) {
        let snapshot = {
            let index = index.read().unwrap_or_else(|e| e.into_inner());
            let file = CacheIndexFileRef { songs: &index };
            match serde_json::to_string(&file) {
                Ok(snapshot) => snapshot,
                Err(e) => {
                    // Writing an empty file here would orphan every cached
                    // download; keep the previous index instead.
                    log::warn!("Failed to serialize cache index, keeping previous file: {e}");
                    return;
                }
            }
        };
        let path = Self::index_path(downloads_dir);
        if let Err(e) = crate::persist::write_atomic(&path, snapshot.as_bytes()) {
            log::warn!("Failed to write cache index: {e}");
        }
    }

    /// The original third-party (sonar) song recorded for a cached song id, if
    /// any. Used to re-resolve playback/lyrics/covers after a restart.
    pub fn thirdparty_song(&self, song_id: u64) -> Option<Arc<sonar::Song>> {
        self.index
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&song_id)
            .and_then(|e| e.thirdparty.clone())
    }

    pub fn mark_uploaded(&self, song_id: u64) {
        let now = unix_secs();
        if let Ok(mut index) = self.index.write()
            && let Some(entry) = index.get_mut(&song_id)
        {
            entry.uploaded_at = now;
        }
        self.save_index();
    }

    /// Drop a song's cache entry from the in-memory index (and schedule the
    /// index write), returning the file that should be deleted.
    ///
    /// This is synchronous and IO-free, so callers can invalidate a corrupt
    /// entry *before* retrying playback; the actual file deletion can then run
    /// on a blocking thread via [`Self::delete_cached_file`].
    pub fn invalidate_cached(&self, song_id: u64) -> Option<PathBuf> {
        let entry = self
            .index
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&song_id)?;
        self.save_index();
        Some(self.downloads_dir.join(entry.filename))
    }

    /// Delete a cache file returned by [`Self::invalidate_cached`], subtracting
    /// its size from the running total. Blocking; call from a blocking task.
    pub fn delete_cached_file(&self, path: &Path) {
        // Measure before deleting: after `remove_file` the size is gone and the
        // running total would keep counting the deleted file, evicting good
        // entries later to "make room" that was already freed.
        let bytes = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        if let Err(e) = fs::remove_file(path)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            log::warn!("Failed to remove cached file {}: {e}", path.display());
        }
        saturating_sub_total(&self.cached_total_bytes, bytes);
    }

    /// Remove a cached song completely (index entry + file). Used by tests and
    /// one-shot callers; production retry code should prefer
    /// [`Self::invalidate_cached`] + a blocking [`Self::delete_cached_file`].
    pub fn remove_cached(&self, song_id: u64) {
        if let Some(path) = self.invalidate_cached(song_id) {
            self.delete_cached_file(&path);
        }
    }

    /// Persist the in-memory cache index to disk. Synchronous — safe for the
    /// shutdown path where detached tasks might not run before runtime teardown.
    pub fn flush_index(&self) {
        self.save_index_sync();
    }

    /// Remove index entries whose files no longer exist or are empty.
    pub fn cleanup_index(&self) {
        let stale: Vec<(u64, String)> = {
            let index = self.index.read().unwrap_or_else(|e| e.into_inner());
            index
                .iter()
                .filter(|(_, entry)| {
                    let path = self.downloads_dir.join(&entry.filename);
                    match fs::metadata(&path) {
                        Ok(m) => m.len() == 0,
                        Err(_) => true,
                    }
                })
                .map(|(id, entry)| (*id, entry.filename.clone()))
                .collect()
        };
        if stale.is_empty() {
            return;
        }
        {
            let mut index = self.index.write().unwrap_or_else(|e| e.into_inner());
            for (id, _) in &stale {
                index.remove(id);
            }
        }
        log::info!("Cleaned up {} stale cache entries", stale.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> CacheIndexFile {
        serde_json::from_str(json).unwrap_or_else(|e| panic!("failed: {e}"))
    }

    #[test]
    fn parses_empty_new_format() {
        let idx = parse(r#"{"songs":{}}"#);
        assert!(idx.songs.is_empty());
    }

    #[test]
    fn parses_new_format_with_entries() {
        let idx = parse(
            r#"{"songs":{"186150":{"filename":"夜曲-周杰伦.mp3","duration":226827,"accessed_at":1,"pic_url":"http://p"}}}"#,
        );
        let e = idx.songs.get(&186150).unwrap();
        assert_eq!(e.filename, "夜曲-周杰伦.mp3");
        assert_eq!(e.duration, 226827);
        assert_eq!(e.pic_url, "http://p");
    }

    #[test]
    fn parses_inline_thirdparty() {
        let idx = parse(
            r#"{"songs":{"11201139274454706721":{"filename":"a.mp3","duration":1,"thirdparty":{"id":11201139274454706721,"source_id":"47444754","name":"x","singer":"y","album":"z","duration":1,"source":"Kuwo","pic_url":"","meta":{"high_hash":null,"lossless_hash":null,"album_id":""}}}}}"#,
        );
        let e = idx.songs.get(&11201139274454706721).unwrap();
        let tp = e.thirdparty.as_ref().unwrap();
        assert_eq!(tp.source_id, "47444754");
        assert_eq!(tp.singer, "y");
    }

    #[test]
    fn round_trips_saved_index() {
        let mut songs = CacheIndex::new();
        songs.insert(
            186150,
            CacheEntry {
                filename: "夜曲-周杰伦.mp3".into(),
                duration: 226827,
                accessed_at: 1,
                pic_url: "http://p".into(),
                uploaded_at: 0,
                thirdparty: None,
            },
        );
        let file = CacheIndexFile { songs };
        let json = serde_json::to_string(&file).unwrap();
        // Default/empty fields stay out of the file, as the old hand-written
        // serializer did.
        assert!(!json.contains("uploaded_at"), "{json}");
        assert!(!json.contains("thirdparty"), "{json}");
        let back: CacheIndexFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.songs.get(&186150).unwrap().filename, "夜曲-周杰伦.mp3");
    }
}
