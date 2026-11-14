//! Playback queue domain: the song list, play modes and on-disk persistence.

mod list;
pub(crate) mod mode;
mod storage;

pub(crate) use list::PlaylistQueue;
pub(crate) use mode::{PlayMode, mode_icon};
pub(crate) use storage::PlaylistStorage;

/// Fixed queue key (display name) shared by every third-party (sonar) search
/// queue; all such songs are stored in the single `thirdparty_source.json`.
pub(crate) const THIRD_PARTY_QUEUE_KEY: &str = "第三方搜索";

/// Fixed queue key (display name) shared by every NCM search queue; all such
/// songs are stored in the single `ncm_search.json`.
pub(crate) const NCM_SEARCH_QUEUE_KEY: &str = "官方搜索";
