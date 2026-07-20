use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use stream_download::storage::StorageProvider;

use crate::state::ContentState;

#[derive(Serialize, Deserialize)]
struct ContentCacheEntry {
    data: ContentState,
    cached_at: u64,
}

#[derive(Clone)]
pub struct CacheManager {
    cache_dir: PathBuf,
    lyrics_dir: PathBuf,
    content_dir: PathBuf,
}

impl CacheManager {
    pub fn with_cache_dir(cache_dir: PathBuf) -> Self {
        let lyrics_dir = cache_dir.join("lyrics");
        let content_dir = cache_dir.join("content");
        Self {
            cache_dir,
            lyrics_dir,
            content_dir,
        }
    }

    pub fn cache_path(&self, id: u64, ext: &str) -> PathBuf {
        self.cache_dir.join(format!("{}.{}", id, ext))
    }

    pub fn is_cached(&self, id: u64, ext: &str) -> bool {
        self.cache_path(id, ext).exists()
    }

    pub fn ensure_dir(&self) -> io::Result<()> {
        fs::create_dir_all(&self.cache_dir)
    }

    pub fn open_cached(&self, id: u64, ext: &str) -> io::Result<File> {
        File::open(self.cache_path(id, ext))
    }

    pub fn create_provider(&self, id: u64, ext: &str) -> io::Result<CacheFileProvider> {
        self.ensure_dir()?;
        Ok(CacheFileProvider {
            path: self.cache_path(id, ext),
        })
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

    pub fn save_content_cache(&self, api: &str, content: ContentState) {
        if let Err(e) = fs::create_dir_all(&self.content_dir) {
            log::warn!("Failed to create content cache dir: {e}");
            return;
        }
        let cached_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let entry = ContentCacheEntry { data: content, cached_at };
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
