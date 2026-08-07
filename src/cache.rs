use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use ncm_api::SongInfo;
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};
use stream_download::storage::StorageProvider;

use crate::state::{ContentState, PaginationInfo};

#[derive(Serialize, Deserialize)]
struct ContentCacheEntry {
    data: ContentState,
    #[serde(default)]
    pagination: Option<PaginationInfo>,
    cached_at: u64,
}

#[derive(Serialize)]
struct ContentCacheEntryRef<'a> {
    data: &'a ContentState,
    pagination: Option<&'a PaginationInfo>,
    cached_at: u64,
}

/// Entry in the audio cache index, mapping song ID to filename and duration.
#[derive(Clone, Deserialize)]
struct CacheEntry {
    filename: String,
    #[serde(default)]
    duration: u64,
    #[serde(default)]
    accessed_at: u64,
    #[serde(default)]
    pic_url: String,
    #[serde(default)]
    uploaded_at: u64,
    /// Original sonar search-result song, present only for third-party songs
    /// that have actually been played and cached. Lets playback/lyrics/covers
    /// be re-resolved via the original provider after a restart without
    /// re-searching or persisting a separate registry.
    #[serde(default)]
    thirdparty: Option<sonar::Song>,
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

type CacheIndex = HashMap<u64, CacheEntry>;

#[derive(Serialize, Default, Deserialize)]
struct CacheIndexFile {
    #[serde(default)]
    songs: HashMap<u64, CacheEntry>,
}

/// Default maximum cache size in bytes (2 GB).
const DEFAULT_MAX_CACHE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Clone)]
/// Manages content, lyrics, and audio caches on disk.
pub struct CacheManager {
    downloads_dir: PathBuf,
    lyrics_dir: PathBuf,
    content_dir: PathBuf,
    covers_dir: PathBuf,
    template: String,
    index: Arc<RwLock<CacheIndex>>,
    max_cache_bytes: u64,
    cached_total_bytes: Arc<AtomicU64>,
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
    fn save_index(&self) {
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

    fn sanitize_filename(s: &str) -> String {
        let s = s.trim();
        s.chars()
            .map(|c| match c {
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                _ => c,
            })
            .collect()
    }

    fn resolve_filename(&self, song: &SongInfo, ext: &str) -> String {
        if self.template == "{id}" {
            use std::fmt::Write;
            let mut out = String::with_capacity(16 + ext.len());
            let _ = write!(out, "{}.{}", song.id, ext);
            return out;
        }
        let mut out = String::with_capacity(self.template.len() + ext.len() + 16);
        let mut rest = self.template.as_str();
        while let Some(pos) = rest.find('{') {
            out.push_str(&rest[..pos]);
            let end = rest[pos..]
                .find('}')
                .map(|e| pos + e + 1)
                .unwrap_or(rest.len());
            let placeholder = &rest[pos..end];
            match placeholder {
                "{id}" => {
                    use std::fmt::Write;
                    let _ = write!(out, "{}", song.id);
                }
                "{name}" => out.push_str(&Self::sanitize_filename(&song.name)),
                "{singer}" => out.push_str(&Self::sanitize_filename(&song.singer)),
                "{album}" => out.push_str(&Self::sanitize_filename(&song.album)),
                other => out.push_str(other),
            }
            rest = &rest[end..];
        }
        out.push_str(rest);
        out.push('.');
        out.push_str(ext);
        out
    }

    pub fn cache_path(&self, id: u64, ext: &str) -> PathBuf {
        let index = self.index.read().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = index.get(&id) {
            return self.downloads_dir.join(&entry.filename);
        }
        drop(index);
        self.downloads_dir.join(format!("{}.{}", id, ext))
    }

    pub fn is_cached(&self, id: u64, ext: &str) -> bool {
        self.cache_path(id, ext).exists()
    }

    /// Find the first cached extension for a song in a single lock + iteration.
    /// Returns `Some(ext)` if any cached file exists, `None` otherwise.
    pub fn find_cached_extension(&self, id: u64) -> Option<&'static str> {
        const EXTENSIONS: &[&str] = &["mp3", "flac", "m4a", "ogg"];
        let index = self.index.read().unwrap_or_else(|e| e.into_inner());
        if index.contains_key(&id) {
            // Index entry exists — check which extension file is on disk
            for &ext in EXTENSIONS {
                if self.cache_path(id, ext).exists() {
                    return Some(ext);
                }
            }
        }
        None
    }

    pub fn ensure_dir(&self) -> io::Result<()> {
        fs::create_dir_all(&self.downloads_dir)
    }

    pub fn open_cached(&self, id: u64, ext: &str) -> io::Result<File> {
        let file = File::open(self.cache_path(id, ext))?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if let Ok(mut index) = self.index.write()
            && let Some(entry) = index.get_mut(&id)
        {
            entry.accessed_at = now;
        }
        Ok(file)
    }

    pub fn create_provider(&self, song: &SongInfo, ext: &str) -> io::Result<CacheFileProvider> {
        self.ensure_dir()?;
        let filename = self.resolve_filename(song, ext);
        let path = self.downloads_dir.join(&filename);

        // Evict oldest entries if cache exceeds size limit
        self.evict();

        Ok(CacheFileProvider { path })
    }

    /// Mark a song as successfully cached. Call this only after the download
    /// completes, so the index never contains entries for incomplete/failed
    /// downloads. Persists the index so a completed download survives an
    /// immediate quit.
    pub fn mark_cached(&self, song: &SongInfo, ext: &str, thirdparty: Option<sonar::Song>) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let filename = self.resolve_filename(song, ext);
        let path = self.downloads_dir.join(&filename);
        let file_bytes = fs::metadata(&path).ok().map(|m| m.len()).unwrap_or(0);
        self.index
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                song.id,
                CacheEntry {
                    filename,
                    duration: song.duration,
                    accessed_at: now,
                    pic_url: song.pic_url.clone(),
                    uploaded_at: 0,
                    thirdparty,
                },
            );
        self.cached_total_bytes
            .fetch_add(file_bytes, Ordering::Relaxed);
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

    fn lyrics_path(&self, id: u64) -> PathBuf {
        self.lyrics_dir.join(format!("{}.json", id))
    }

    fn content_path(&self, api: &str) -> PathBuf {
        self.content_dir.join(format!("{}.json", api))
    }

    pub async fn load_lyrics_cache_async(&self, id: u64) -> Option<ncm_api::Lyrics> {
        let path = self.lyrics_path(id);
        tokio::task::spawn_blocking(move || {
            let data = fs::read_to_string(path).ok()?;
            serde_json::from_str(&data).ok()
        })
        .await
        .ok()
        .flatten()
    }

    pub fn save_lyrics_cache(&self, id: u64, lyrics: &ncm_api::Lyrics) {
        if let Err(e) = fs::create_dir_all(&self.lyrics_dir) {
            log::warn!("Failed to create lyrics cache dir: {e}");
            return;
        }
        match serde_json::to_string(lyrics) {
            Ok(json) => {
                if let Err(e) = fs::write(self.lyrics_path(id), json) {
                    log::warn!("Failed to write lyrics cache for {id}: {e}");
                }
            }
            Err(e) => {
                log::warn!("Failed to serialize lyrics cache for {id}: {e}");
            }
        }
    }

    fn cover_path(&self, song_id: u64) -> PathBuf {
        self.covers_dir.join(format!("{song_id}.jpg"))
    }

    pub async fn load_cover_async(&self, song_id: u64) -> Option<Vec<u8>> {
        let path = self.cover_path(song_id);
        tokio::task::spawn_blocking(move || fs::read(path).ok())
            .await
            .ok()
            .flatten()
    }

    pub fn save_cover(&self, song_id: u64, data: &[u8]) {
        if let Err(e) = fs::create_dir_all(&self.covers_dir) {
            log::warn!("Failed to create covers cache dir: {e}");
            return;
        }
        if let Err(e) = fs::write(self.cover_path(song_id), data) {
            log::warn!("Failed to write cover cache for {song_id}: {e}");
        }
    }

    pub async fn load_content_cache_async(
        &self,
        api: &str,
        ttl_secs: u64,
    ) -> Option<(ContentState, Option<PaginationInfo>)> {
        let path = self.content_path(api);
        tokio::task::spawn_blocking(move || {
            let data = fs::read_to_string(path).ok()?;
            let entry: ContentCacheEntry = serde_json::from_str(&data).ok()?;
            let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
            if now - entry.cached_at > ttl_secs {
                return None;
            }
            Some((entry.data, entry.pagination))
        })
        .await
        .ok()
        .flatten()
    }

    /// Collect cached songs by iterating the index under a read lock, avoiding
    /// a full clone of the HashMap.
    fn collect_cached_songs(&self, index: &CacheIndex) -> Vec<SongInfo> {
        let mut songs = Vec::with_capacity(index.len());
        for (id, entry) in index {
            let path = self.downloads_dir.join(&entry.filename);
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let (name, singer) = self.parse_filename(stem, *id);
            let name = if entry.uploaded_at > 0 {
                format!("{name} ↑")
            } else {
                name
            };
            songs.push(SongInfo {
                id: *id,
                name,
                singer,
                artist_id: 0,
                album: String::new(),
                album_id: 0,
                pic_url: entry.pic_url.clone(),
                duration: entry.duration,
                copyright: ncm_api::SongCopyright::Unknown,
            });
        }
        songs
    }

    pub async fn list_cached_songs_async(&self) -> Vec<SongInfo> {
        {
            let index = self.index.read().unwrap_or_else(|e| e.into_inner());
            self.collect_cached_songs(&index)
        }
    }

    /// Parse a cached filename stem into (name, singer) using the template.
    fn parse_filename(&self, stem: &str, id: u64) -> (String, String) {
        Self::parse_filename_static(stem, id, &self.template)
    }

    fn parse_filename_static(stem: &str, id: u64, template: &str) -> (String, String) {
        if template == "{id}" {
            return (id.to_string(), String::new());
        }

        // Find the last literal separator in the template
        let placeholders = ["{id}", "{name}", "{singer}", "{album}"];
        let mut last_sep_start = 0;
        let mut last_sep_len = 0;
        let mut remaining = template;
        let mut offset = 0;
        while !remaining.is_empty() {
            let mut earliest = remaining.len();
            let mut earliest_len = 0;
            for ph in &placeholders {
                if let Some(pos) = remaining.find(ph)
                    && pos < earliest
                {
                    earliest = pos;
                    earliest_len = ph.len();
                }
            }
            if earliest_len == 0 {
                break;
            }
            if earliest > 0 {
                last_sep_start = offset;
                last_sep_len = earliest;
            }
            remaining = &remaining[earliest + earliest_len..];
            offset += earliest + earliest_len;
        }

        if last_sep_len == 0 {
            return (stem.to_string(), String::new());
        }

        let sep = &template[last_sep_start..last_sep_start + last_sep_len];

        if sep.is_empty() {
            return (stem.to_string(), String::new());
        }

        // Split from the right by the separator
        if let Some(pos) = stem.rfind(sep) {
            let name = stem[..pos].to_string();
            let singer = stem[pos + sep.len()..].to_string();
            return (name, singer);
        }

        (stem.to_string(), String::new())
    }

    pub fn save_content_cache(
        &self,
        api: &str,
        content: &ContentState,
        pagination: Option<&PaginationInfo>,
    ) {
        if let Err(e) = fs::create_dir_all(&self.content_dir) {
            log::warn!("Failed to create content cache dir: {e}");
            return;
        }
        let cached_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let entry = ContentCacheEntryRef {
            data: content,
            pagination,
            cached_at,
        };
        match serde_json::to_string(&entry) {
            Ok(json) => {
                if let Err(e) = fs::write(self.content_path(api), json) {
                    log::warn!("Failed to write content cache for {api}: {e}");
                }
            }
            Err(e) => {
                log::warn!("Failed to serialize content cache for {api}: {e}");
            }
        }
    }

    pub fn evict(&self) -> usize {
        let total_bytes = self.cached_total_bytes.load(Ordering::Relaxed);
        if total_bytes <= self.max_cache_bytes {
            return 0;
        }

        let mut index = self.index.write().unwrap_or_else(|e| e.into_inner());
        let total = self.cached_total_bytes.load(Ordering::Relaxed);
        if total <= self.max_cache_bytes {
            return 0;
        }

        // Sort by accessed_at ascending — collect only IDs, avoid cloning filenames
        let mut entries: Vec<(u64, u64)> =
            index.iter().map(|(id, e)| (*id, e.accessed_at)).collect();
        entries.sort_by_key(|e| e.1);

        let mut evicted = 0;
        let mut freed = 0u64;
        for (id, _) in &entries {
            if total - freed <= self.max_cache_bytes {
                break;
            }
            if let Some(entry) = index.get(id) {
                let path = self.downloads_dir.join(&entry.filename);
                if let Ok(meta) = fs::metadata(&path) {
                    freed += meta.len();
                }
                let _ = fs::remove_file(&path);
            }
            index.remove(id);
            evicted += 1;
        }

        if evicted > 0 {
            self.cached_total_bytes.fetch_sub(freed, Ordering::Relaxed);
            log::info!("Evicted {evicted} cached songs, freed {freed} bytes");
        }
        evicted
    }
}

pub struct CacheFileProvider {
    path: PathBuf,
}

impl StorageProvider for CacheFileProvider {
    type Reader = File;
    type Writer = File;

    fn into_reader_writer(
        self,
        _content_length: Option<u64>,
    ) -> io::Result<(Self::Reader, Self::Writer)> {
        let writer = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .read(true)
            .open(&self.path)?;
        let reader = File::open(&self.path)?;
        Ok((reader, writer))
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
