use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use serde::{Deserialize, Serialize, Serializer, ser::SerializeMap};

use super::{CacheManager, DEFAULT_MAX_CACHE_BYTES};
use crate::utils::unix_secs;

/// Entry in the audio cache index, mapping song ID to filename and duration.
#[derive(Clone, Deserialize)]
pub(super) struct CacheEntry {
    pub(super) filename: String,
    #[serde(default)]
    pub(super) duration: u64,
    #[serde(default)]
    pub(super) accessed_at: u64,
    #[serde(default)]
    pub(super) pic_url: String,
    #[serde(default)]
    pub(super) uploaded_at: u64,
    #[serde(default)]
    pub(super) thirdparty: Option<Arc<sonar::Song>>,
}

impl Serialize for CacheEntry {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let n = 2
            + usize::from(self.accessed_at > 0)
            + usize::from(!self.pic_url.is_empty())
            + usize::from(self.uploaded_at > 0)
            + usize::from(self.thirdparty.is_some());
        let mut map = serializer.serialize_map(Some(n))?;
        map.serialize_entry("filename", &self.filename)?;
        map.serialize_entry("duration", &self.duration)?;
        if self.accessed_at > 0 {
            map.serialize_entry("accessed_at", &self.accessed_at)?;
        }
        if !self.pic_url.is_empty() {
            map.serialize_entry("pic_url", &self.pic_url)?;
        }
        if self.uploaded_at > 0 {
            map.serialize_entry("uploaded_at", &self.uploaded_at)?;
        }
        if let Some(song) = &self.thirdparty {
            map.serialize_entry("thirdparty", song)?;
        }
        map.end()
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
        let content_dir = base_dir.join("content");
        let covers_dir = base_dir.join("covers");
        let index_file = Self::load_index_static(&downloads_dir);
        let total = Self::compute_total_bytes(&downloads_dir, &index_file.songs);
        Self {
            downloads_dir,
            lyrics_dir,
            content_dir,
            covers_dir,
            template,
            index: Arc::new(RwLock::new(index_file.songs)),
            max_cache_bytes: DEFAULT_MAX_CACHE_BYTES,
            cached_total_bytes: Arc::new(AtomicU64::new(total)),
            index_dirty: Arc::new(AtomicBool::new(false)),
            index_write_pending: Arc::new(AtomicBool::new(false)),
            content_pending: Arc::new(Mutex::new(HashMap::new())),
            content_writer: Arc::new(AtomicBool::new(false)),
            content_write_lock: Arc::new(Mutex::new(())),
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

    fn load_index_static(dir: &Path) -> CacheIndexFile {
        let path = Self::index_path(dir);
        if !path.exists() {
            return CacheIndexFile::default();
        }
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
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
        if let Err(e) = super::write_atomic(&path, snapshot.as_bytes()) {
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

    pub fn remove_from_index(&self, song_id: u64) {
        let mut index = self.index.write().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = index.remove(&song_id) {
            let path = self.downloads_dir.join(&entry.filename);
            if let Ok(meta) = fs::metadata(&path) {
                self.cached_total_bytes
                    .fetch_sub(meta.len(), Ordering::Relaxed);
            }
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
        let back: CacheIndexFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.songs.get(&186150).unwrap().filename, "夜曲-周杰伦.mp3");
    }
}
