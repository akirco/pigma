use std::sync::{Arc, Mutex};

use ratatui_image::protocol::StatefulProtocol;

use crate::utils::lock_recover;

/// Cover image state shared with async loaders. `protocol` holds the decoded
/// image; `song_id` records which song the cover belongs to so a slow loader
/// from a previously played song can't overwrite the current cover.
pub struct CoverState {
    pub protocol: Arc<Mutex<Option<StatefulProtocol>>>,
    pub song_id: Arc<Mutex<Option<u64>>>,
}

impl std::fmt::Debug for CoverState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoverState")
            .field("has_cover", &lock_recover(&self.protocol).is_some())
            .finish()
    }
}

impl Clone for CoverState {
    fn clone(&self) -> Self {
        Self {
            protocol: Arc::clone(&self.protocol),
            song_id: Arc::clone(&self.song_id),
        }
    }
}

impl Default for CoverState {
    fn default() -> Self {
        Self {
            protocol: Arc::new(Mutex::new(None)),
            song_id: Arc::new(Mutex::new(None)),
        }
    }
}
