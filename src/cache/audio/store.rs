//! Cache lookups, download finalization, listing and LRU eviction.

use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    sync::{Arc, atomic::Ordering},
};

use ncm_api::SongInfo;

use super::{
    super::{
        CacheManager,
        index::{CacheEntry, CacheIndex},
    },
    provider::{CacheFileProvider, replace_file},
};
use crate::time::unix_secs;

impl CacheManager {
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
        // Resolve the path under a single read lock, then drop the guard before
        // touching the filesystem. Re-entering the read lock (as `cache_path`
        // does) while a writer is queued could deadlock with `std::sync::RwLock`.
        let path = {
            let index = self.index.read().unwrap_or_else(|e| e.into_inner());
            index
                .get(&id)
                .map(|e| self.downloads_dir.join(&e.filename))?
        };
        if !path.exists() {
            return None;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        Some(super::naming::canonical_ext(ext))
    }

    fn ensure_dir(&self) -> io::Result<()> {
        fs::create_dir_all(&self.downloads_dir)
    }

    pub fn open_cached(&self, id: u64, ext: &str) -> io::Result<File> {
        let file = File::open(self.cache_path(id, ext))?;
        let now = unix_secs();
        if let Ok(mut index) = self.index.write()
            && let Some(entry) = index.get_mut(&id)
        {
            entry.accessed_at = now;
        }
        Ok(file)
    }

    /// Create the provider that streams a new download into a hidden `.part`
    /// file. The caller must hand the returned path to
    /// [`Self::mark_cached`] once the download completes, which atomically
    /// renames it into place. Writing to a temp path means a failed/aborted
    /// download can never truncate an existing good cache file, and the index
    /// only ever points at complete files.
    pub fn create_provider(&self, song: &SongInfo) -> io::Result<(CacheFileProvider, PathBuf)> {
        self.ensure_dir()?;
        let part_path = self.downloads_dir.join(format!(
            ".{}-{}.{}.part",
            song.id,
            std::process::id(),
            rand::random::<u32>()
        ));

        // Evict oldest entries if cache exceeds size limit
        self.evict();

        Ok((
            CacheFileProvider {
                part_path: part_path.clone(),
            },
            part_path,
        ))
    }

    /// Mark a song as successfully cached. Call this only after the download
    /// completes, so the index never contains entries for incomplete/failed
    /// downloads. Persists the index so a completed download survives an
    /// immediate quit.
    ///
    /// `part_path` is the temporary file the download wrote to; it is renamed
    /// over the final cache path here. Returns whether the entry was finalized
    /// (the caller should only report `Cached` when this is `true`).
    pub fn mark_cached(
        &self,
        song: &SongInfo,
        ext: &str,
        thirdparty: Option<sonar::Song>,
        part_path: Option<&Path>,
    ) -> bool {
        let now = unix_secs();
        let filename = self.resolve_filename(song, ext);
        let path = self.downloads_dir.join(&filename);

        // Read the previous entry/size *before* replacing the file: re-caching a
        // song (quality/template change) must subtract the old size from the
        // running total, and a template change leaves the old file orphaned.
        let previous = {
            let index = self.index.read().unwrap_or_else(|e| e.into_inner());
            index.get(&song.id).map(|entry| {
                let old_path = self.downloads_dir.join(&entry.filename);
                let bytes = fs::metadata(&old_path).map(|m| m.len()).unwrap_or(0);
                (entry.filename.clone(), bytes)
            })
        };

        if let Some(part) = part_path
            && let Err(e) = replace_file(part, &path)
        {
            // Keep the previous index entry: the old file is still intact and
            // the new download simply stays a `.part` until cleaned up.
            log::warn!(
                "Failed to finalize cache file {} -> {}: {e}",
                part.display(),
                path.display()
            );
            return false;
        }

        if let Some((old_filename, _)) = &previous
            && old_filename != &filename
        {
            let _ = fs::remove_file(self.downloads_dir.join(old_filename));
        }

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
                    thirdparty: thirdparty.map(Arc::new),
                },
            );
        self.cached_total_bytes
            .fetch_add(file_bytes, Ordering::Relaxed);
        if let Some((_, previous_bytes)) = previous {
            self.cached_total_bytes
                .fetch_sub(previous_bytes, Ordering::Relaxed);
        }
        self.save_index();
        true
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
                format!("{name} {}", crate::icons::UPLOADED_MARK)
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

    fn evict(&self) -> usize {
        let total_bytes = self.cached_total_bytes.load(Ordering::Relaxed);
        if total_bytes <= self.max_cache_bytes {
            return 0;
        }
        let need = total_bytes - self.max_cache_bytes;

        // Under the read lock, only decide the LRU candidate order (IDs +
        // filenames). All filesystem I/O happens after the lock is released so
        // concurrent readers are never blocked by disk operations.
        let entries: Vec<(u64, String)> = {
            let index = self.index.read().unwrap_or_else(|e| e.into_inner());
            let mut by_age: Vec<(u64, u64)> =
                index.iter().map(|(id, e)| (*id, e.accessed_at)).collect();
            by_age.sort_by_key(|e| e.1);
            by_age
                .into_iter()
                .filter_map(|(id, _)| index.get(&id).map(|e| (id, e.filename.clone())))
                .collect()
        };

        // Evict only enough oldest entries to get under the limit — not the
        // whole cache, which would force a full re-download of everything.
        let mut freed = 0u64;
        let mut victims: Vec<(u64, String)> = Vec::new();
        for (id, filename) in entries {
            if freed >= need {
                break;
            }
            let size = fs::metadata(self.downloads_dir.join(&filename))
                .map(|m| m.len())
                .unwrap_or(0);
            freed += size;
            victims.push((id, filename));
        }
        if victims.is_empty() {
            return 0;
        }

        {
            let mut index = self.index.write().unwrap_or_else(|e| e.into_inner());
            for (id, _) in &victims {
                index.remove(id);
            }
        }
        for (_, filename) in &victims {
            let _ = fs::remove_file(self.downloads_dir.join(filename));
        }
        self.cached_total_bytes.fetch_sub(freed, Ordering::Relaxed);
        log::info!("Evicted {} cached songs", victims.len());
        victims.len()
    }
}
