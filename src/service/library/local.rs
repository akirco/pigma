//! Local library loading: turns a blocking `~/Music` scan into the `Songs`
//! content state shared by the TUI local-music tab and headless endpoint loading.

use std::sync::Arc;

use super::{super::ApiService, scan::scan_local_music};
use crate::state::ContentState;

impl ApiService {
    /// Scan the user's Music directory off the async runtime and map it to a
    /// `Songs` content state. Shared by the TUI local-music tab (which adds
    /// content-cache read/write around it) and headless endpoint loading.
    pub async fn load_local_music(&self) -> ContentState {
        let music_dir = dirs::home_dir().unwrap_or_default().join("Music");
        let songs = tokio::task::spawn_blocking(move || scan_local_music(&music_dir))
            .await
            .unwrap_or_default();
        ContentState::Songs(songs.into_iter().map(Arc::new).collect())
    }
}
