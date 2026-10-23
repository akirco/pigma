//! Local library, search, authentication, song URLs and cloud upload.

use std::{path::Path, sync::Arc};

use super::{ApiService, arc_songs};
use crate::{playback::scan_local_music, state::ContentState};

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

    /// Search songs by keyword.
    pub async fn search_songs(&self, keyword: &str, limit: u16) -> ContentState {
        match self.client.search_song(keyword, 0, limit).await {
            Ok(result) => ContentState::Songs(arc_songs(result.songs)),
            Err(e) => ContentState::Error(e.to_string()),
        }
    }

    pub async fn login_qr_create(&self) -> Result<(String, String), ncm_api::NcmError> {
        self.client.login_qr_create().await
    }

    pub async fn login_qr_check(&self, key: &str) -> Result<ncm_api::Msg, ncm_api::NcmError> {
        self.client.login_qr_check(key).await
    }

    pub async fn login_status(&self) -> Result<ncm_api::LoginInfo, ncm_api::NcmError> {
        self.client.login_status().await
    }

    pub async fn fetch_song_urls(
        &self,
        ids: &[u64],
        level: ncm_api::SongQuality,
    ) -> Result<Vec<ncm_api::SongUrl>, ncm_api::NcmError> {
        self.client.songs_url_v1(ids, level).await
    }

    pub async fn heartbeat_songs(
        &self,
        song_id: u64,
        playlist_id: u64,
    ) -> Result<Vec<ncm_api::SongInfo>, ncm_api::NcmError> {
        self.client
            .playmode_intelligence_list(song_id, playlist_id)
            .await
    }

    pub async fn upload_song_with_meta(
        &self,
        path: &Path,
        song_hint: &str,
        album_hint: &str,
        artist_hint: &str,
    ) -> Result<ncm_api::CloudUploadResult, ncm_api::NcmError> {
        self.client
            .upload_song_with_meta(path, song_hint, album_hint, artist_hint)
            .await
    }
}
