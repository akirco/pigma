use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use ncm_api::SongInfo;
use serde::{Deserialize, Deserializer, Serialize};
use stream_download::storage::StorageProvider;

use crate::state::ContentState;

#[derive(Serialize, Deserialize)]
struct ContentCacheEntry {
    data: ContentState,
    cached_at: u64,
}

/// Entry in the audio cache index, mapping song ID to filename and duration.
#[derive(Clone, Serialize, Deserialize)]
struct CacheEntry {
    filename: String,
    #[serde(default)]
    duration: u64,
}

/// Backward-compatible deserializer: accepts both the old format (plain string)
/// and the new format (object with filename + duration).
fn deserialize_cache_entry<'de, D>(deserializer: D) -> Result<CacheEntry, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Str(String),
        Obj(CacheEntry),
    }

    match Raw::deserialize(deserializer)? {
        Raw::Str(filename) => Ok(CacheEntry {
            filename,
            duration: 0,
        }),
        Raw::Obj(entry) => Ok(entry),
    }
}

type CacheIndex = HashMap<u64, CacheEntryWrapper>;

#[derive(Clone, Serialize)]
struct CacheEntryWrapper {
    filename: String,
    duration: u64,
}

impl<'de> Deserialize<'de> for CacheEntryWrapper {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entry = deserialize_cache_entry(deserializer)?;
        Ok(Self {
            filename: entry.filename,
            duration: entry.duration,
        })
    }
}

#[derive(Clone)]
/// Manages content, lyrics, and audio caches on disk.
pub struct CacheManager {
    downloads_dir: PathBuf,
    lyrics_dir: PathBuf,
    content_dir: PathBuf,
    template: String,
    index: Arc<Mutex<CacheIndex>>,
}

use std::sync::Arc;

impl CacheManager {
    pub fn new(downloads_dir: PathBuf, base_dir: PathBuf, template: String) -> Self {
        let lyrics_dir = base_dir.join("lyrics");
        let content_dir = base_dir.join("content");
        let index = Self::load_index_static(&downloads_dir);
        Self {
            downloads_dir,
            lyrics_dir,
            content_dir,
            template,
            index: Arc::new(Mutex::new(index)),
        }
    }

    fn index_path(dir: &Path) -> PathBuf {
        dir.join("cache_index.json")
    }

    fn load_index_static(dir: &Path) -> CacheIndex {
        let path = Self::index_path(dir);
        if !path.exists() {
            return HashMap::new();
        }
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save_index(&self) {
        let index = self.index.lock().unwrap();
        let path = Self::index_path(&self.downloads_dir);
        if let Err(e) = fs::write(
            &path,
            serde_json::to_string_pretty(&*index).unwrap_or_default(),
        ) {
            log::warn!("Failed to write cache index: {e}");
        }
    }

    pub fn remove_from_index(&self, song_id: u64) {
        self.index.lock().unwrap().remove(&song_id);
    }

    /// Persist the in-memory cache index to disk.
    pub fn flush_index(&self) {
        self.save_index();
    }

    fn sanitize_filename(s: &str) -> String {
        s.chars()
            .map(|c| match c {
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                _ => c,
            })
            .collect::<String>()
            .trim()
            .to_string()
    }

    fn resolve_filename(&self, song: &SongInfo, ext: &str) -> String {
        if self.template == "{id}" {
            return format!("{}.{}", song.id, ext);
        }
        let name = self
            .template
            .replace("{id}", &song.id.to_string())
            .replace("{name}", &Self::sanitize_filename(&song.name))
            .replace("{singer}", &Self::sanitize_filename(&song.singer))
            .replace("{album}", &Self::sanitize_filename(&song.album));
        format!("{}.{}", name, ext)
    }

    pub fn cache_path_for(&self, song: &SongInfo, ext: &str) -> PathBuf {
        let index = self.index.lock().unwrap();
        if let Some(entry) = index.get(&song.id) {
            return self.downloads_dir.join(&entry.filename);
        }
        let filename = self.resolve_filename(song, ext);
        self.downloads_dir.join(filename)
    }

    pub fn cache_path(&self, id: u64, ext: &str) -> PathBuf {
        let index = self.index.lock().unwrap();
        if let Some(entry) = index.get(&id) {
            return self.downloads_dir.join(&entry.filename);
        }
        drop(index);
        self.downloads_dir.join(format!("{}.{}", id, ext))
    }

    pub fn is_cached(&self, id: u64, ext: &str) -> bool {
        self.cache_path(id, ext).exists()
    }

    pub fn ensure_dir(&self) -> io::Result<()> {
        fs::create_dir_all(&self.downloads_dir)
    }

    pub fn open_cached(&self, id: u64, ext: &str) -> io::Result<File> {
        File::open(self.cache_path(id, ext))
    }

    pub fn create_provider(&self, song: &SongInfo, ext: &str) -> io::Result<CacheFileProvider> {
        self.ensure_dir()?;
        let filename = self.resolve_filename(song, ext);
        let path = self.downloads_dir.join(&filename);

        {
            let mut index = self.index.lock().unwrap();
            index.insert(
                song.id,
                CacheEntryWrapper {
                    filename,
                    duration: song.duration,
                },
            );
        }

        Ok(CacheFileProvider { path })
    }

    fn lyrics_path(&self, id: u64) -> PathBuf {
        self.lyrics_dir.join(format!("{}.json", id))
    }

    fn content_path(&self, api: &str) -> PathBuf {
        self.content_dir.join(format!("{}.json", api))
    }

    pub fn load_lyrics_cache(&self, id: u64) -> Option<ncm_api::Lyrics> {
        let path = self.lyrics_path(id);
        let data = fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
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

    pub fn load_content_cache(&self, api: &str, ttl_secs: u64) -> Option<ContentState> {
        let path = self.content_path(api);
        let data = fs::read_to_string(path).ok()?;
        let entry: ContentCacheEntry = serde_json::from_str(&data).ok()?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
        if now - entry.cached_at > ttl_secs {
            return None;
        }
        Some(entry.data)
    }

    pub async fn load_content_cache_async(&self, api: &str, ttl_secs: u64) -> Option<ContentState> {
        let path = self.content_path(api);
        tokio::task::spawn_blocking(move || {
            let data = fs::read_to_string(path).ok()?;
            let entry: ContentCacheEntry = serde_json::from_str(&data).ok()?;
            let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
            if now - entry.cached_at > ttl_secs {
                return None;
            }
            Some(entry.data)
        })
        .await
        .ok()
        .flatten()
    }

    pub fn list_cached_songs(&self) -> Vec<SongInfo> {
        let index = self.index.lock().unwrap().clone();
        let mut songs = Vec::new();

        for (id, entry) in &index {
            let path = self.downloads_dir.join(&entry.filename);
            if !path.exists() {
                continue;
            }
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

            let (name, singer) = self.parse_filename(stem, *id);

            songs.push(SongInfo {
                id: *id,
                name,
                singer,
                artist_id: 0,
                album: String::new(),
                album_id: 0,
                pic_url: String::new(),
                duration: entry.duration,
                copyright: ncm_api::SongCopyright::Unknown,
            });
        }

        songs
    }

    pub async fn list_cached_songs_async(&self) -> Vec<SongInfo> {
        let index = self.index.lock().unwrap().clone();
        let downloads_dir = self.downloads_dir.clone();
        let template = self.template.clone();

        tokio::task::spawn_blocking(move || {
            let mut songs = Vec::new();

            for (id, entry) in &index {
                let path = downloads_dir.join(&entry.filename);
                if !path.exists() {
                    continue;
                }
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

                let (name, singer) = Self::parse_filename_static(stem, *id, &template);

                songs.push(SongInfo {
                    id: *id,
                    name,
                    singer,
                    artist_id: 0,
                    album: String::new(),
                    album_id: 0,
                    pic_url: String::new(),
                    duration: entry.duration,
                    copyright: ncm_api::SongCopyright::Unknown,
                });
            }

            songs
        })
        .await
        .unwrap_or_default()
    }

    /// Parse a cached filename stem into (name, singer) using the template.
    ///
    /// Extracts literal separators from the template, then splits the stem
    /// from the right to separate the last field (singer) from the rest (name).
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

    pub fn save_content_cache(&self, api: &str, content: ContentState) {
        if let Err(e) = fs::create_dir_all(&self.content_dir) {
            log::warn!("Failed to create content cache dir: {e}");
            return;
        }
        let cached_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let entry = ContentCacheEntry {
            data: content,
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
