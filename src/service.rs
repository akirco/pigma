//! Centralized data-loading service (`ApiService`): resolves NetEase Cloud Music
//! API endpoints (see [`ApiEndpoint`]), applies caching, and maps
//! errors to events.
//!
//! The methods are grouped by concern in submodules:
//! - `content`: navigation endpoints, playlist/album/artist pages, `load_more`
//! - `liked`: liked songs and the lazy playlist `trackIds` cache
//! - `lyrics`: NetEase + sonar lyrics with cache integration
//! - `library`: local music, search, auth, song URLs, cloud upload
//! - `search`: shared TUI/IPC search execution and result registries

mod content;
mod library;
mod liked;
mod lyrics;
mod search;

use std::{collections::HashMap, sync::Arc};

use crate::{
    cache::CacheManager,
    state::{ContentState, PaginationInfo},
};

pub use search::{SearchEngine, SearchHit, SearchResults};
pub(crate) use search::{search_ncm, search_sonar};

/// Navigation/content endpoints backed by the NetEase Cloud Music API.
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
    /// Canonical string key used by the nav config and the content cache.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RecommendSongs => "recommend_songs",
            Self::RecommendResource => "recommend_resource",
            Self::Toplist => "toplist",
            Self::TopSongList => "top_song_list",
            Self::UserRadioSublist => "user_radio_sublist",
            Self::UserCloudDisk => "user_cloud_disk",
            Self::LikedSongs => "liked",
            Self::UserSongList => "user_song_list",
            Self::UserCreatedSongList => "user_created_song_list",
            Self::UserSubscribedSongList => "user_subscribed_song_list",
            Self::SavedAlbums => "album_sublist",
            Self::Download => "download",
            Self::LocalMusic => "local_music",
            Self::Recent => "recent",
            Self::Search => "search",
            Self::TopSingers => "top_singers",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "recommend_songs" => Self::RecommendSongs,
            "recommend_resource" => Self::RecommendResource,
            "toplist" => Self::Toplist,
            "top_song_list" => Self::TopSongList,
            "user_radio_sublist" => Self::UserRadioSublist,
            "user_cloud_disk" => Self::UserCloudDisk,
            "liked" | "__liked__" => Self::LikedSongs,
            "user_song_list" => Self::UserSongList,
            "user_created_song_list" => Self::UserCreatedSongList,
            "user_subscribed_song_list" => Self::UserSubscribedSongList,
            "album_sublist" => Self::SavedAlbums,
            "download" | "__download__" => Self::Download,
            "local_music" | "__local_music__" => Self::LocalMusic,
            "recent" | "__recent__" => Self::Recent,
            "search" => Self::Search,
            "top_singers" => Self::TopSingers,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ApiEndpoint;

    const ALL: &[ApiEndpoint] = &[
        ApiEndpoint::RecommendSongs,
        ApiEndpoint::RecommendResource,
        ApiEndpoint::Toplist,
        ApiEndpoint::TopSongList,
        ApiEndpoint::UserRadioSublist,
        ApiEndpoint::UserCloudDisk,
        ApiEndpoint::LikedSongs,
        ApiEndpoint::UserSongList,
        ApiEndpoint::UserCreatedSongList,
        ApiEndpoint::UserSubscribedSongList,
        ApiEndpoint::SavedAlbums,
        ApiEndpoint::Download,
        ApiEndpoint::LocalMusic,
        ApiEndpoint::Recent,
        ApiEndpoint::Search,
        ApiEndpoint::TopSingers,
    ];

    #[test]
    fn parse_round_trips_the_canonical_key() {
        for endpoint in ALL {
            assert_eq!(ApiEndpoint::parse(endpoint.as_str()), Some(*endpoint));
        }
    }

    #[test]
    fn parse_accepts_the_legacy_aliases() {
        assert_eq!(
            ApiEndpoint::parse("__liked__"),
            Some(ApiEndpoint::LikedSongs)
        );
        assert_eq!(
            ApiEndpoint::parse("__download__"),
            Some(ApiEndpoint::Download)
        );
        assert_eq!(
            ApiEndpoint::parse("__local_music__"),
            Some(ApiEndpoint::LocalMusic)
        );
        assert_eq!(ApiEndpoint::parse("__recent__"), Some(ApiEndpoint::Recent));
        assert_eq!(ApiEndpoint::parse("nope"), None);
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
