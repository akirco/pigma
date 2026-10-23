use std::{fs, path::PathBuf, sync::atomic::Ordering};

use serde::{Deserialize, Serialize};

use super::CacheManager;
use crate::{
    state::{ContentState, PaginationInfo},
    utils::{lock_recover, unix_secs},
};

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

/// Serialize a content-cache entry to JSON.
///
/// Split out from [`CacheManager::save_content_cache`] so a caller that holds the
/// content through a shared `Arc` can serialize on the calling thread and move
/// only the resulting bytes to the blocking disk writer. Holding the `Arc`
/// across `spawn_blocking` would make the next `Arc::make_mut` deep-clone the
/// whole content list.
fn content_cache_json(
    content: &ContentState,
    pagination: Option<&PaginationInfo>,
) -> Result<String, serde_json::Error> {
    let entry = ContentCacheEntryRef {
        data: content,
        pagination,
        cached_at: unix_secs(),
    };
    serde_json::to_string(&entry)
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
            let now = unix_secs();
            // A future timestamp (clock went backwards) is treated as expired:
            // `now - cached_at` would otherwise underflow and panic in debug
            // builds, or wrap to a huge value and be treated as stale anyway.
            if entry.cached_at > now || now - entry.cached_at > ttl_secs {
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
        match content_cache_json(content, pagination) {
            Ok(json) => {
                // A full replacement supersedes any queued paged payload for
                // the same API; dropping it also stops an older list from
                // landing after this write.
                let _writing = lock_recover(&self.content_write_lock);
                lock_recover(&self.content_pending).remove(api);
                self.write_content_cache_locked(api, &json);
            }
            Err(e) => log::warn!("Failed to serialize content cache for {api}: {e}"),
        }
    }

    /// [`Self::save_content_cache`] for callers that already hold
    /// `content_write_lock`.
    fn write_content_cache_locked(&self, api: &str, json: &str) {
        if let Err(e) = super::write_atomic(&self.content_path(api), json.as_bytes()) {
            log::warn!("Failed to write content cache for {api}: {e}");
        }
    }

    /// Queue a content-cache write, coalescing bursts (paged loads).
    ///
    /// A paged load re-serializes the whole (merged) list after every page;
    /// writing each one straight to disk would be one full-list write per page.
    /// Here the newest payload for an API replaces any not-yet-written one and a
    /// single background writer drains the queue — the same coalescing
    /// `index::save_index` uses. `content` is typically `ContentState::Songs(..)`,
    /// whose clone is a cheap `Vec<Arc<_>>` copy: no deep clone, and no shared
    /// `Arc` is kept alive, so the next `Arc::make_mut` is not forced into a
    /// deep clone.
    pub fn queue_content_cache(
        &self,
        api: &str,
        content: ContentState,
        pagination: PaginationInfo,
    ) {
        lock_recover(&self.content_pending).insert(api.to_string(), (content, pagination));
        if self.content_writer.swap(true, Ordering::SeqCst) {
            // A writer loop is already running and will drain the queue.
            return;
        }
        let cache = self.clone();
        tokio::task::spawn_blocking(move || cache.run_content_writer());
    }

    /// Background writer loop: drain the queue, then release the claim. Re-claims
    /// once if a payload landed between the empty check and clearing the flag, so
    /// it cannot be stranded without a writer.
    fn run_content_writer(&self) {
        loop {
            // Hold the write lock across `drain + write` so a shutdown flush (or
            // a removal) can never interleave an older payload after a newer one.
            // `content_pending` is always locked while the write lock is held —
            // except in `queue_content_cache`, which takes only the former — so
            // the lock order is consistent.
            let wrote = {
                let _writing = lock_recover(&self.content_write_lock);
                self.drain_pending_locked()
            };
            if wrote {
                continue;
            }
            self.content_writer.store(false, Ordering::SeqCst);
            if !lock_recover(&self.content_pending).is_empty()
                && !self.content_writer.swap(true, Ordering::SeqCst)
            {
                continue;
            }
            break;
        }
    }

    /// Write every queued payload and report whether the queue had anything.
    /// The caller must hold `content_write_lock`.
    fn drain_pending_locked(&self) -> bool {
        let entries: Vec<_> = lock_recover(&self.content_pending).drain().collect();
        for (api, (content, pagination)) in &entries {
            match content_cache_json(content, Some(pagination)) {
                Ok(json) => self.write_content_cache_locked(api, &json),
                Err(e) => log::warn!("Failed to serialize content cache for {api}: {e}"),
            }
        }
        !entries.is_empty()
    }

    /// Write every pending content-cache payload synchronously. Used on the quit
    /// path, where the detached coalescing writer might not run before the
    /// runtime is torn down.
    ///
    /// Acquiring `content_write_lock` waits out an in-flight write, so the
    /// newest payload is always the last one to reach disk. The writer flag is
    /// only a scheduling hint, not a lock: the queue itself is emptied here, and
    /// a writer that wakes up afterwards finds nothing to do.
    pub fn flush_content_cache(&self) {
        let _writing = lock_recover(&self.content_write_lock);
        self.drain_pending_locked();
    }

    pub fn remove_content_cache(&self, api: &str) {
        // Serialize with the writer: drop any queued payload for this API and
        // delete the file only once an in-flight write for it has finished, so
        // neither the writer nor a stale queue entry can recreate the file.
        let _writing = lock_recover(&self.content_write_lock);
        lock_recover(&self.content_pending).remove(api);
        match fs::remove_file(self.content_path(api)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => log::warn!("Failed to remove content cache for {api}: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::atomic::Ordering};

    use super::CacheManager;
    use crate::{
        state::{ContentState, PaginationInfo},
        utils::lock_recover,
    };

    #[test]
    fn flush_content_cache_writes_pending_payload_even_when_writer_is_busy() {
        let dir = std::env::temp_dir().join(format!(
            "pigma_content_flush_test_{}_{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        let cache = CacheManager::new(dir.join("downloads"), dir.clone(), "template".into());
        let api = "shutdown_flush_test";
        let pagination = PaginationInfo {
            api: api.to_string(),
            ..PaginationInfo::default()
        };

        lock_recover(&cache.content_pending).insert(
            api.to_string(),
            (ContentState::Songs(vec![]), pagination.clone()),
        );
        cache.content_writer.store(true, Ordering::SeqCst);

        cache.flush_content_cache();

        let path = cache.content_path(api);
        assert!(
            path.exists(),
            "pending content cache should be flushed to disk"
        );
        let json = fs::read_to_string(path).unwrap();
        assert!(json.contains("\"data\""), "{json}");
        assert!(
            lock_recover(&cache.content_pending).is_empty(),
            "flush should drain the queued payload"
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn queued_content_cache_is_written_by_the_background_writer() {
        let dir = std::env::temp_dir().join(format!(
            "pigma_content_queue_test_{}_{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        let cache = CacheManager::new(dir.join("downloads"), dir.clone(), "template".into());
        let api = "queued_write_test";
        let pagination = PaginationInfo {
            api: api.to_string(),
            ..PaginationInfo::default()
        };

        cache.queue_content_cache(api, ContentState::Songs(vec![]), pagination);

        let path = cache.content_path(api);
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        while !path.exists() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(
            path.exists(),
            "queued content cache should be written by the background writer"
        );
        assert!(
            lock_recover(&cache.content_pending).is_empty(),
            "the writer should drain the queue"
        );

        let _ = fs::remove_dir_all(dir);
    }
}
