use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};

use super::{CacheManager, DEFAULT_MAX_CACHE_BYTES};

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
    /// Original sonar search-result song, present only for third-party songs
    /// that have actually been played and cached. Lets playback/lyrics/covers
    /// be re-resolved via the original provider after a restart without
    /// re-searching or persisting a separate registry.
    #[serde(default)]
    pub(super) thirdparty: Option<sonar::Song>,
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

    /// Snapshot the index under a read lock, then serialize and write to disk
    /// without holding any lock.
    pub(super) fn save_index(&self) {
        if let Err(e) = fs::create_dir_all(&self.downloads_dir) {
            log::warn!("Failed to create downloads dir: {e}");
            return;
        }
        let snapshot = {
            let index = self.index.read().unwrap_or_else(|e| e.into_inner());
            let file = CacheIndexFile {
                songs: index.clone(),
            };
            serde_json::to_string(&file).unwrap_or_default()
        };
        let path = Self::index_path(&self.downloads_dir);
        if let Err(e) = fs::write(&path, snapshot) {
            log::warn!("Failed to write cache index: {e}");
        }
    }

    /// The original third-party (sonar) song recorded for a cached song id, if
    /// any. Used to re-resolve playback/lyrics/covers after a restart.
    pub fn thirdparty_song(&self, song_id: u64) -> Option<sonar::Song> {
        self.index
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&song_id)
            .and_then(|e| e.thirdparty.clone())
    }

    pub fn mark_uploaded(&self, song_id: u64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
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

    /// Persist the in-memory cache index to disk.
    pub fn flush_index(&self) {
        self.save_index();
    }

    /// Remove index entries whose files no longer exist or are empty.
    pub fn cleanup_index(&self) {
        let mut index = self.index.write().unwrap_or_else(|e| e.into_inner());
        let stale: Vec<u64> = index
            .iter()
            .filter(|(_, entry)| {
                let path = self.downloads_dir.join(&entry.filename);
                match fs::metadata(&path) {
                    Ok(m) => m.len() == 0,
                    Err(_) => true,
                }
            })
            .map(|(id, _)| *id)
            .collect();
        for id in &stale {
            index.remove(id);
        }
        if !stale.is_empty() {
            log::info!("Cleaned up {} stale cache entries", stale.len());
        }
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
