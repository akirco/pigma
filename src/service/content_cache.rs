//! Content-list cache: endpoint payloads plus their pagination state.
//!
//! Kept outside `cache` because it depends on the UI-facing `ContentState`;
//! the media cache must not know about view-model types.

use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use serde::{Deserialize, Serialize};

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
/// Split out from [`ContentCache::save_content_cache`] so a caller that holds
/// the content through a shared `Arc` can serialize on the calling thread and
/// move only the resulting bytes to the blocking disk writer. Holding the `Arc`
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

/// Cache for navigation endpoint payloads (`ContentState`) keyed by account and
/// API string; pagination metadata is stored alongside.
#[derive(Clone)]
pub struct ContentCache {
    content_dir: PathBuf,
    /// Payloads awaiting a coalesced write, keyed by API. A paged load replaces
    /// the entry for its API instead of re-serializing and writing the whole
    /// list per page, so a burst of pages collapses into one write of the final
    /// state; entries for different APIs never displace each other.
    pending: Arc<Mutex<HashMap<String, (ContentState, PaginationInfo)>>>,
    /// Guard ensuring at most one content-cache writer drains `pending`.
    writer: Arc<AtomicBool>,
    /// Serializes the take-and-write step of the writer against
    /// [`ContentCache::flush_content_cache`] and `remove_content_cache`, so an
    /// in-flight writer can never land an older payload after a newer one, nor
    /// resurrect a file that was just removed.
    write_lock: Arc<Mutex<()>>,
}

impl ContentCache {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            content_dir: base_dir.join("content"),
            pending: Arc::new(Mutex::new(HashMap::new())),
            writer: Arc::new(AtomicBool::new(false)),
            write_lock: Arc::new(Mutex::new(())),
        }
    }

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
                let _writing = lock_recover(&self.write_lock);
                lock_recover(&self.pending).remove(api);
                self.write_content_cache_locked(api, &json);
            }
            Err(e) => log::warn!("Failed to serialize content cache for {api}: {e}"),
        }
    }

    /// [`Self::save_content_cache`] for callers that already hold `write_lock`.
    fn write_content_cache_locked(&self, api: &str, json: &str) {
        if let Err(e) = crate::store::write_atomic(&self.content_path(api), json.as_bytes()) {
            log::warn!("Failed to write content cache for {api}: {e}");
        }
    }

    /// Queue a content-cache write, coalescing bursts (paged loads).
    ///
    /// A paged load re-serializes the whole (merged) list after every page;
    /// writing each one straight to disk would be one full-list write per page.
    /// Here the newest payload for an API replaces any not-yet-written one and a
    /// single background writer drains the queue.
    pub fn queue_content_cache(
        &self,
        api: &str,
        content: ContentState,
        pagination: PaginationInfo,
    ) {
        lock_recover(&self.pending).insert(api.to_string(), (content, pagination));
        if self.writer.swap(true, Ordering::SeqCst) {
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
            let wrote = {
                let _writing = lock_recover(&self.write_lock);
                self.drain_pending_locked()
            };
            if wrote {
                continue;
            }
            self.writer.store(false, Ordering::SeqCst);
            if !lock_recover(&self.pending).is_empty() && !self.writer.swap(true, Ordering::SeqCst)
            {
                continue;
            }
            break;
        }
    }

    /// Write every queued payload and report whether the queue had anything.
    /// The caller must hold `write_lock`.
    fn drain_pending_locked(&self) -> bool {
        let entries: Vec<_> = lock_recover(&self.pending).drain().collect();
        for (api, (content, pagination)) in &entries {
            match content_cache_json(content, Some(pagination)) {
                Ok(json) => self.write_content_cache_locked(api, &json),
                Err(e) => log::warn!("Failed to serialize content cache for {api}: {e}"),
            }
        }
        !entries.is_empty()
    }

    /// Write every pending payload synchronously. Used on the quit path, where
    /// the detached coalescing writer might not run before the runtime is torn
    /// down.
    pub fn flush_content_cache(&self) {
        let _writing = lock_recover(&self.write_lock);
        self.drain_pending_locked();
    }

    pub fn remove_content_cache(&self, api: &str) {
        // Serialize with the writer: drop any queued payload for this API and
        // delete the file only once an in-flight write for it has finished, so
        // neither the writer nor a stale queue entry can recreate the file.
        let _writing = lock_recover(&self.write_lock);
        lock_recover(&self.pending).remove(api);
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

    use super::ContentCache;
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
        let cache = ContentCache::new(dir.clone());
        let api = "shutdown_flush_test";
        let pagination = PaginationInfo {
            api: api.to_string(),
            ..PaginationInfo::default()
        };

        lock_recover(&cache.pending)
            .insert(api.to_string(), (ContentState::Songs(vec![]), pagination));
        cache.writer.store(true, Ordering::SeqCst);

        cache.flush_content_cache();

        let path = cache.content_path(api);
        assert!(
            path.exists(),
            "pending content cache should be flushed to disk"
        );
        let json = fs::read_to_string(path).unwrap();
        assert!(json.contains("\"data\""), "{json}");
        assert!(
            lock_recover(&cache.pending).is_empty(),
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
        let cache = ContentCache::new(dir.clone());
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
            lock_recover(&cache.pending).is_empty(),
            "the writer should drain the queue"
        );

        let _ = fs::remove_dir_all(dir);
    }
}
