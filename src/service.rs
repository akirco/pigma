//! Centralized data-loading service (`ApiService`): resolves NetEase Cloud Music
//! API endpoints (see [`ApiEndpoint`]), applies caching, and maps errors to events.
//!
//! The methods are grouped by concern in submodules:
//! - `content`: navigation endpoints, playlist/album/artist pages, `load_more`
//! - `liked`: liked songs and the lazy playlist `trackIds` cache
//! - `lyrics`: NetEase + sonar lyrics with cache integration
//! - `library`: local music, search, auth, song URLs, cloud upload

mod content;
mod library;
mod liked;
mod lyrics;

use std::{collections::HashMap, sync::Arc};

use crate::{
    cache::CacheManager,
    state::{ContentState, PaginationInfo},
};

/// Navigation/content endpoints backed by the NetEase Cloud Music API.
///
/// `parse` maps the string keys used in the navigation tree (and persisted
/// playlist ids like `liked`, `download`, `local_music`) onto a
/// concrete endpoint resolved by [`ApiService`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApiEndpoint {
    RecommendSongs,
    RecommendResource,
    Toplist,
    TopSongList,
    UserRadioSublist,
    UserCloudDisk,
    LikedSongs,
    UserSongList,
    UserCreatedSongList,
    UserSubscribedSongList,
    SavedAlbums,
    Download,
    LocalMusic,
    Recent,
    Search,
    TopSingers,
}

impl ApiEndpoint {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "recommend_songs" => Some(ApiEndpoint::RecommendSongs),
            "recommend_resource" => Some(ApiEndpoint::RecommendResource),
            "toplist" => Some(ApiEndpoint::Toplist),
            "top_song_list" => Some(ApiEndpoint::TopSongList),
            "user_radio_sublist" => Some(ApiEndpoint::UserRadioSublist),
            "user_cloud_disk" => Some(ApiEndpoint::UserCloudDisk),
            "liked" | "__liked__" => Some(ApiEndpoint::LikedSongs),
            "user_song_list" => Some(ApiEndpoint::UserSongList),
            "user_created_song_list" => Some(ApiEndpoint::UserCreatedSongList),
            "user_subscribed_song_list" => Some(ApiEndpoint::UserSubscribedSongList),
            "album_sublist" => Some(ApiEndpoint::SavedAlbums),
            "download" | "__download__" => Some(ApiEndpoint::Download),
            "local_music" | "__local_music__" => Some(ApiEndpoint::LocalMusic),
            "recent" | "__recent__" => Some(ApiEndpoint::Recent),
            "search" => Some(ApiEndpoint::Search),
            "top_singers" => Some(ApiEndpoint::TopSingers),
            _ => None,
        }
    }
}

/// Centralized API service that handles endpoint resolution, caching, and error mapping.
///
/// Callers are still responsible for `tokio::spawn` + `send_event`.
#[derive(Clone)]
pub struct ApiService {
    client: Arc<ncm_api::NcmClient>,
    cache: Arc<CacheManager>,
    playlist_track_ids: Arc<std::sync::Mutex<HashMap<u64, Vec<u64>>>>,
}

impl ApiService {
    pub fn new(client: Arc<ncm_api::NcmClient>, cache: Arc<CacheManager>) -> Self {
        Self {
            client,
            cache,
            playlist_track_ids: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    pub fn client(&self) -> &Arc<ncm_api::NcmClient> {
        &self.client
    }

    pub fn cache(&self) -> &Arc<CacheManager> {
        &self.cache
    }
}

/// Find the "我喜欢的音乐" liked playlist in a playlist list: exact match first,
/// then tolerate renamed titles (e.g. "柚子白纸喜欢的音乐").
fn find_liked_playlist(lists: &[ncm_api::SongList]) -> Option<u64> {
    lists
        .iter()
        .find(|l| l.name == "我喜欢的音乐")
        .or_else(|| lists.iter().find(|l| l.name.ends_with("喜欢的音乐")))
        .map(|l| l.id)
}

/// Map a client result into a `(ContentState, None)` pair: success maps the
/// value through `map`, failure becomes `ContentState::Error`. Keeps the
/// `Ok -> state / Err -> Error` boilerplate in one place.
fn content_result<T>(
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
fn arc_songs(songs: Vec<ncm_api::SongInfo>) -> Vec<Arc<ncm_api::SongInfo>> {
    songs.into_iter().map(Arc::new).collect()
}
