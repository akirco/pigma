//! On-disk cache manager (`CacheManager`) and the audio/content/covers/lyrics/index
//! cache submodules.

pub mod audio;
pub mod content;
pub mod covers;
pub mod index;
pub mod lyrics;

use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64},
    },
};

use index::CacheIndex;

use crate::state::{ContentState, PaginationInfo};

/// Default maximum cache size in bytes (2 GB).
const DEFAULT_MAX_CACHE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Atomically write `bytes` to `path`, creating its parent directory if needed.
///
/// A unique temp file is written in the same directory, `fsync`ed, then renamed
/// over the target. `rename` is atomic within a filesystem, so a reader — or a
/// process crash mid-write — never observes a truncated file: the previous
/// contents stay intact until the swap. The data is synced before the rename so
/// a power loss cannot leave the target pointing at a zero-length temp file; at
/// worst the rename is lost and the old contents remain. Every cache file
/// (content/lyrics/covers/index) goes through here so none of them can be left
/// half-written.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;

    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("cache");
    let tmp = dir.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        rand::random::<u32>()
    ));

    let result = (|| {
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

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
    /// Set by every in-memory mutation of the index; a pending writer drains it
    /// so a burst of mutations coalesces into few disk writes.
    index_dirty: Arc<AtomicBool>,
    /// Guard ensuring at most one index-write loop is in flight.
    index_write_pending: Arc<AtomicBool>,
    /// Content-cache payloads awaiting a coalesced write, keyed by API. A paged
    /// load replaces the entry for its API instead of re-serializing and writing
    /// the whole list per page, so a burst of pages collapses into one write of
    /// the final state; entries for different APIs never displace each other.
    content_pending: Arc<Mutex<HashMap<String, (ContentState, PaginationInfo)>>>,
    /// Guard ensuring at most one content-cache writer drains `content_pending`.
    content_writer: Arc<AtomicBool>,
    /// Serializes the take-and-write step of the content-cache writer against
    /// [`CacheManager::flush_content_cache`] and `remove_content_cache`, so an
    /// in-flight writer can never land an older payload after a newer one, nor
    /// resurrect a file that was just removed.
    content_write_lock: Arc<Mutex<()>>,
}

#[cfg(test)]
mod tests {
    use super::write_atomic;

    #[test]
    fn write_atomic_creates_dir_and_replaces_without_leftovers() {
        let dir = std::env::temp_dir().join(format!("pigma_cache_test_{}", rand::random::<u64>()));
        let path = dir.join("nested").join("entry.json");

        write_atomic(&path, b"first").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"first");

        // A second write replaces the contents (not appends / truncates).
        write_atomic(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");

        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {leftovers:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
