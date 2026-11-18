//! On-disk cover cache: the original (or re-encoded small) JPEG per song id,
//! addressed by `song_id` and stored under the cache root's `covers/` directory.
//!
//! These are `CacheManager` methods rather than free functions because the
//! directory is part of the manager's layout.

use std::{fs, path::PathBuf};

use crate::cache::CacheManager;

impl CacheManager {
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
        if let Err(e) = crate::persist::write_atomic(&self.cover_path(song_id), data) {
            log::warn!("Failed to write cover cache for {song_id}: {e}");
        }
    }
}
