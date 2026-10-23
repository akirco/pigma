use std::{fs, path::PathBuf};

use super::CacheManager;

impl CacheManager {
    fn lyrics_path(&self, id: u64) -> PathBuf {
        self.lyrics_dir.join(format!("{}.json", id))
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
        match serde_json::to_string(lyrics) {
            Ok(json) => {
                if let Err(e) = super::write_atomic(&self.lyrics_path(id), json.as_bytes()) {
                    log::warn!("Failed to write lyrics cache for {id}: {e}");
                }
            }
            Err(e) => {
                log::warn!("Failed to serialize lyrics cache for {id}: {e}");
            }
        }
    }
}
