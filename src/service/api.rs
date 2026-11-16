//! `ApiService` definition, shared helpers, and thin NCM passthroughs.

use std::{collections::HashMap, path::Path, sync::Arc};

use super::content_cache::ContentCache;
use crate::{
    cache::CacheManager,
    state::{ContentState, PaginationInfo},
};

/// Centralized API service that handles endpoint resolution, caching, and error mapping.
///
/// Callers are still responsible for `tokio::spawn` + `send_event`.
#[derive(Clone)]
pub struct ApiService {
    pub(super) client: Arc<ncm_api::NcmClient>,
    pub(super) cache: Arc<CacheManager>,
    pub(super) content_cache: Arc<ContentCache>,
    pub(super) playlist_track_ids: Arc<std::sync::Mutex<HashMap<u64, Vec<u64>>>>,
}

impl ApiService {
    pub fn new(
        client: Arc<ncm_api::NcmClient>,
        cache: Arc<CacheManager>,
        content_cache: Arc<ContentCache>,
    ) -> Self {
        Self {
            client,
            cache,
            content_cache,
            playlist_track_ids: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    pub fn client(&self) -> &Arc<ncm_api::NcmClient> {
        &self.client
    }

    pub fn cache(&self) -> &Arc<CacheManager> {
        &self.cache
    }

    /// Content-list cache (navigation payloads); separate from the media cache
    /// because it stores view-model `ContentState` values.
    pub fn content_cache(&self) -> &Arc<ContentCache> {
        &self.content_cache
    }
}

/// Thin passthroughs to the NCM client: they only forward arguments, so they
/// live next to `ApiService` instead of getting a module each.
impl ApiService {
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

/// Find the "我喜欢的音乐" liked playlist in a playlist list: exact match first,
/// then tolerate renamed titles (e.g. "柚子白纸喜欢的音乐").
pub(super) fn find_liked_playlist(lists: &[ncm_api::SongList]) -> Option<u64> {
    lists
        .iter()
        .find(|l| l.name == "我喜欢的音乐")
        .or_else(|| lists.iter().find(|l| l.name.ends_with("喜欢的音乐")))
        .map(|l| l.id)
}

/// Map a client result into a `(ContentState, None)` pair: success maps the
/// value through `map`, failure becomes `ContentState::Error`. Keeps the
/// `Ok -> state / Err -> Error` boilerplate in one place.
pub(super) fn content_result<T>(
    result: Result<T, ncm_api::NcmError>,
    map: impl FnOnce(T) -> ContentState,
) -> (ContentState, Option<PaginationInfo>) {
    match result {
        Ok(v) => (map(v), None),
        Err(e) => (ContentState::Error(e.to_string()), None),
    }
}

/// Share the API's `SongInfo` list as `Arc`s so `ContentState::Songs` and the
/// playback queue share ownership, avoiding deep-cloning the whole list between content and queue.
pub(super) fn arc_songs(songs: Vec<ncm_api::SongInfo>) -> Vec<Arc<ncm_api::SongInfo>> {
    songs.into_iter().map(Arc::new).collect()
}
