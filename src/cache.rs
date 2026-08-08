pub mod audio;
pub mod content;
pub mod covers;
pub mod index;
pub mod lyrics;

use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, RwLock};

use index::CacheIndex;

/// Default maximum cache size in bytes (2 GB).
const DEFAULT_MAX_CACHE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Manages content, lyrics, and audio caches on disk.
#[derive(Clone)]
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
