//! Audio-file cache: filename templates, lookup/eviction and the streaming
//! provider that writes downloads into a hidden part file.

mod naming;
mod provider;
mod store;

pub(crate) use naming::canonical_ext;
pub use provider::CacheFileProvider;

#[cfg(test)]
mod tests {
    //! Audio-cache tests: filename uniqueness, part-file finalization and size
    //! accounting on removal.

    use std::{fs, path::PathBuf, sync::atomic::Ordering};

    use ncm_api::{SongCopyright, SongInfo};

    use crate::cache::CacheManager;

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
