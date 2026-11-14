//! Fixed (search) queue ids and file names shared by the storage layer.

use super::super::queue::{NCM_SEARCH_QUEUE_KEY, THIRD_PARTY_QUEUE_KEY};

/// File name for the unified NCM search queue.
pub(crate) const NCM_SEARCH_FILE: &str = "ncm_search.json";
/// Stable id for the NCM search queue: a single shared file, no date/hash.
pub(super) const NCM_SEARCH_ID: &str = "q_ncm_search";

/// File name for the unified third-party (sonar) search queue.
pub(crate) const THIRD_PARTY_FILE: &str = "thirdparty_source.json";
/// Stable id for the third-party queue: a single shared file, no date/hash.
pub(super) const THIRD_PARTY_ID: &str = "q_thirdparty";

/// Map a fixed search-queue key to its stable `(id, file)` pair.
pub(super) fn special_queue(key: &str) -> Option<(&'static str, &'static str)> {
    match key {
        NCM_SEARCH_QUEUE_KEY => Some((NCM_SEARCH_ID, NCM_SEARCH_FILE)),
        THIRD_PARTY_QUEUE_KEY => Some((THIRD_PARTY_ID, THIRD_PARTY_FILE)),
        _ => None,
    }
}

/// Reverse lookup: a fixed queue `id` to its `(display name, file)` pair.
pub(super) fn special_queue_by_id(id: &str) -> Option<(&'static str, &'static str)> {
    match id {
        NCM_SEARCH_ID => Some((NCM_SEARCH_QUEUE_KEY, NCM_SEARCH_FILE)),
        THIRD_PARTY_ID => Some((THIRD_PARTY_QUEUE_KEY, THIRD_PARTY_FILE)),
        _ => None,
    }
}
