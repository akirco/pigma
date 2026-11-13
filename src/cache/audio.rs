use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
    sync::{Arc, atomic::Ordering},
};

use ncm_api::SongInfo;
use stream_download::storage::StorageProvider;

use super::{
    CacheManager,
    index::{CacheEntry, CacheIndex},
};
use crate::utils::{sanitize_filename, unix_secs};

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
        // The song id is the only field guaranteed to be unique. Without it two
        // different songs with the same name/singer (live vs. studio, covers,
        // remixes) resolve to the same file, truncate each other and make the
        // cache index serve the wrong audio. Append it even when the user's
        // template omits `{id}`; old index entries keep their old filenames.
        if !self.template.contains("{id}") {
            use std::fmt::Write;
            let _ = write!(out, "-{}", song.id);
        }
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
        Some(crate::utils::audio::canonical_ext(ext))
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

        // `resolve_filename` appends `-<id>` when the template has no `{id}`;
        // strip it so the cached-songs list shows a clean name/singer pair.
        let stem = if template.contains("{id}") {
            stem
        } else {
            stem.strip_suffix(&format!("-{id}")).unwrap_or(stem)
        };

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

/// Atomically move a completed `.part` download over its final cache path.
/// `rename` replaces an existing destination on Unix; Windows refuses, so the
/// old file is removed first there.
fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(windows)]
    if to.exists() {
        fs::remove_file(to)?;
    }
    fs::rename(from, to)
}

pub struct CacheFileProvider {
    /// Hidden temp file the download actually writes to.
    part_path: PathBuf,
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
            .open(&self.part_path)?;
        let reader = File::open(&self.part_path)?;
        Ok((reader, writer))
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, sync::atomic::Ordering};

    use ncm_api::{SongCopyright, SongInfo};

    use super::CacheManager;

    fn song(id: u64) -> SongInfo {
        SongInfo {
            id,
            name: "夜曲".into(),
            singer: "周杰伦".into(),
            artist_id: 0,
            album: String::new(),
            album_id: 0,
            pic_url: String::new(),
            duration: 1_000,
            copyright: SongCopyright::Unknown,
        }
    }

    fn cache(template: &str) -> (CacheManager, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "pigma_cache_audio_test_{}_{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        let cache = CacheManager::new(dir.join("downloads"), dir.clone(), template.into());
        (cache, dir)
    }

    /// Two songs with identical name/singer must never share a cache file:
    /// the id suffix is the uniqueness guarantee.
    #[test]
    fn template_without_id_still_gets_a_unique_filename() {
        let (cache, dir) = cache("{name}-{singer}");
        let a = song(1);
        let b = song(2);

        assert_eq!(cache.resolve_filename(&a, "mp3"), "夜曲-周杰伦-1.mp3");
        assert_ne!(
            cache.resolve_filename(&a, "mp3"),
            cache.resolve_filename(&b, "mp3")
        );

        // The id suffix stays out of the displayed name/singer pair.
        let (name, singer) =
            CacheManager::parse_filename_static("夜曲-周杰伦-1", 1, "{name}-{singer}");
        assert_eq!(name, "夜曲");
        assert_eq!(singer, "周杰伦");

        let _ = fs::remove_dir_all(dir);
    }

    /// A completed download is moved from its hidden `.part` file to the final
    /// path only when `mark_cached` is called; the index then points at it.
    #[tokio::test]
    async fn mark_cached_finalizes_the_part_file() {
        let (cache, dir) = cache("{name}-{singer}");
        let song = song(42);
        let (_provider, part_path) = cache.create_provider(&song).expect("provider");
        fs::write(&part_path, b"audio").expect("write part");

        cache.mark_cached(&song, "mp3", None, Some(&part_path));

        let final_path = cache.cache_path(song.id, "mp3");
        assert!(final_path.exists(), "final cache file missing");
        assert_eq!(fs::read(&final_path).expect("read final"), b"audio");
        assert!(!part_path.exists(), "part file should be renamed away");
        assert_eq!(cache.find_cached_extension(song.id), Some("mp3"));

        cache.save_index_sync();
        let _ = fs::remove_dir_all(dir);
    }

    /// Removing a cached song must delete the file and subtract its size from
    /// the running total; otherwise eviction later frees space that is no
    /// longer occupied.
    #[tokio::test]
    async fn remove_cached_deletes_the_file_and_frees_its_bytes() {
        let (cache, dir) = cache("{id}");
        let song = song(7);
        let (_provider, part_path) = cache.create_provider(&song).expect("provider");
        fs::write(&part_path, b"12345").expect("write part");
        cache.mark_cached(&song, "mp3", None, Some(&part_path));
        assert_eq!(cache.cached_total_bytes.load(Ordering::Relaxed), 5);

        cache.remove_cached(song.id);

        assert_eq!(cache.cached_total_bytes.load(Ordering::Relaxed), 0);
        assert!(!cache.cache_path(song.id, "mp3").exists());
        assert_eq!(cache.find_cached_extension(song.id), None);

        cache.save_index_sync();
        let _ = fs::remove_dir_all(dir);
    }
}
