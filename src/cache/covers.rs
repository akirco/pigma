use std::fs;
use std::path::PathBuf;

use super::CacheManager;

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
        if let Err(e) = fs::create_dir_all(&self.covers_dir) {
            log::warn!("Failed to create covers cache dir: {e}");
            return;
        }
        if let Err(e) = fs::write(self.cover_path(song_id), data) {
            log::warn!("Failed to write cover cache for {song_id}: {e}");
        }
    }
}
