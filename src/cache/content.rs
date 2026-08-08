use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::state::{ContentState, PaginationInfo};

use super::CacheManager;

#[derive(Serialize, Deserialize)]
struct ContentCacheEntry {
    data: ContentState,
    #[serde(default)]
    pagination: Option<PaginationInfo>,
    cached_at: u64,
}

#[derive(Serialize)]
struct ContentCacheEntryRef<'a> {
    data: &'a ContentState,
    pagination: Option<&'a PaginationInfo>,
    cached_at: u64,
}

impl CacheManager {
    fn content_path(&self, api: &str) -> PathBuf {
        self.content_dir.join(format!("{}.json", api))
    }

    pub async fn load_content_cache_async(
        &self,
        api: &str,
        ttl_secs: u64,
    ) -> Option<(ContentState, Option<PaginationInfo>)> {
        let path = self.content_path(api);
        tokio::task::spawn_blocking(move || {
            let data = fs::read_to_string(path).ok()?;
            let entry: ContentCacheEntry = serde_json::from_str(&data).ok()?;
            let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
            if now - entry.cached_at > ttl_secs {
                return None;
            }
            Some((entry.data, entry.pagination))
        })
        .await
        .ok()
        .flatten()
    }

    pub fn save_content_cache(
        &self,
        api: &str,
        content: &ContentState,
        pagination: Option<&PaginationInfo>,
    ) {
        if let Err(e) = fs::create_dir_all(&self.content_dir) {
            log::warn!("Failed to create content cache dir: {e}");
            return;
        }
        let cached_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let entry = ContentCacheEntryRef {
            data: content,
            pagination,
            cached_at,
        };
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
