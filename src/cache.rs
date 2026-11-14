//! On-disk media cache (`CacheManager`) and its audio/covers/lyrics/index
//! submodules. Navigation content caching lives in [`crate::content_cache`].

pub mod audio;
pub mod covers;
pub mod index;
pub mod lyrics;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, AtomicU64},
    },
};

use index::CacheIndex;

/// Default maximum cache size in bytes (2 GB).
const DEFAULT_MAX_CACHE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Remove hidden `.part` downloads left behind by an interrupted run.
///
/// Only files older than a day are removed: a second concurrently-running
/// pigma instance may legitimately still be downloading into one.
fn cleanup_stale_partials(dir: &Path) {
    const MAX_AGE: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !(name.starts_with('.') && name.ends_with(".part")) {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        if modified.elapsed().is_ok_and(|age| age > MAX_AGE) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Manages lyrics, covers and audio caches on disk.
#[derive(Clone)]
pub struct CacheManager {
    downloads_dir: PathBuf,
    lyrics_dir: PathBuf,
    covers_dir: PathBuf,
    template: String,
    index: Arc<RwLock<CacheIndex>>,
    max_cache_bytes: u64,
    cached_total_bytes: Arc<AtomicU64>,
    /// Set by every in-memory mutation of the index; a pending writer drains it
    /// so a burst of mutations coalesces into few disk writes.
    index_dirty: Arc<AtomicBool>,
    /// Guard ensuring at most one index-write loop is in flight.
    index_write_pending: Arc<AtomicBool>,
}
