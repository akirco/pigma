use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

use ncm_api::SongInfo;
use stream_download::storage::StorageProvider;

use super::CacheManager;
use super::index::{CacheEntry, CacheIndex};

use crate::utils::sanitize_filename;

impl CacheManager {
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
                "{name}" => out.push_str(&sanitize_filename(&song.name)),
                "{singer}" => out.push_str(&sanitize_filename(&song.singer)),
                "{album}" => out.push_str(&sanitize_filename(&song.album)),
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
