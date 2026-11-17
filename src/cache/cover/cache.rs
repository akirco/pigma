//! Bounded cache for content-card cover thumbnails.
//!
//! Thumbnails are kept for the session, but browsing many lists must not grow
//! RSS without bound. The cache tracks the last access per URL and evicts the
//! least recently used entry once it reaches its capacity.

use std::collections::HashMap;

use ratatui_image::protocol::StatefulProtocol;

/// Enough for several screens of covers. Thumbnails are capped at 96×96, so
/// the cache stays at a few MiB instead of growing with every visited list.
const DEFAULT_CAPACITY: usize = 128;

struct Entry {
    protocol: StatefulProtocol,
    last_used: u64,
}

pub(crate) struct CoverCache {
    entries: HashMap<String, Entry>,
    /// Monotonic access counter; avoids moving entries around on every read.
    clock: u64,
    capacity: usize,
}

impl Default for CoverCache {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            clock: 0,
            capacity: DEFAULT_CAPACITY,
        }
    }
}

impl CoverCache {
    pub(crate) fn contains_key(&self, url: &str) -> bool {
        self.entries.contains_key(url)
    }

    pub(crate) fn get_mut(&mut self, url: &str) -> Option<&mut StatefulProtocol> {
        self.clock = self.clock.wrapping_add(1);
        let now = self.clock;
        let entry = self.entries.get_mut(url)?;
        entry.last_used = now;
        Some(&mut entry.protocol)
    }

    pub(crate) fn insert(&mut self, url: String, protocol: StatefulProtocol) {
        if !self.entries.contains_key(&url) && self.entries.len() >= self.capacity {
            self.evict_lru();
        }
        self.clock = self.clock.wrapping_add(1);
        self.entries.insert(
            url,
            Entry {
                protocol,
                last_used: self.clock,
            },
        );
    }

    fn evict_lru(&mut self) {
        let Some(oldest) = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(url, _)| url.clone())
        else {
            return;
        };
        self.entries.remove(&oldest);
    }
}

#[cfg(test)]
mod tests {
    use image::DynamicImage;
    use ratatui_image::{
        FontSize,
        protocol::{StatefulProtocol, StatefulProtocolType, halfblocks::Halfblocks},
    };

    use super::*;

    fn protocol() -> StatefulProtocol {
        StatefulProtocol::new(
            DynamicImage::new_rgba8(1, 1),
            FontSize::new(10, 20),
            None,
            StatefulProtocolType::Halfblocks(Halfblocks::default()),
        )
    }

    fn cache_with_capacity(capacity: usize) -> CoverCache {
        CoverCache {
            capacity,
            ..CoverCache::default()
        }
    }

    #[test]
    fn evicts_the_least_recently_used_entry() {
        let mut cache = cache_with_capacity(2);
        cache.insert("a".into(), protocol());
        cache.insert("b".into(), protocol());

        // Touch "a" so "b" becomes the least recently used.
        assert!(cache.get_mut("a").is_some());
        cache.insert("c".into(), protocol());

        assert!(cache.contains_key("a"));
        assert!(!cache.contains_key("b"));
        assert!(cache.contains_key("c"));
        assert_eq!(cache.entries.len(), 2);
    }

    #[test]
    fn replacing_an_existing_url_does_not_evict_another() {
        let mut cache = cache_with_capacity(1);
        cache.insert("a".into(), protocol());
        cache.insert("a".into(), protocol());

        assert!(cache.contains_key("a"));
        assert_eq!(cache.entries.len(), 1);
    }

    #[test]
    fn capacity_is_bounded() {
        let mut cache = cache_with_capacity(DEFAULT_CAPACITY);
        for index in 0..=DEFAULT_CAPACITY {
            cache.insert(format!("url-{index}"), protocol());
        }

        assert_eq!(cache.entries.len(), DEFAULT_CAPACITY);
    }
}
