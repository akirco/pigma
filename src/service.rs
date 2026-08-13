//! Centralized data-loading service (`ApiService`): resolves NetEase Cloud Music
//! API endpoints (see [`ApiEndpoint`]), applies caching, and maps errors to events.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::cache::CacheManager;
use crate::playback::{LyricLine, parse_lyric_lines};
use crate::state::{ContentState, HotSearchKeywords, PaginationInfo};

/// Navigation/content endpoints backed by the NetEase Cloud Music API.
///
/// `parse` maps the string keys used in the navigation tree (and persisted
/// playlist ids like `__liked__`, `__download__`, `__local_music__`) onto a
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
            "__liked__" => Some(ApiEndpoint::LikedSongs),
            "user_song_list" => Some(ApiEndpoint::UserSongList),
            "user_created_song_list" => Some(ApiEndpoint::UserCreatedSongList),
            "user_subscribed_song_list" => Some(ApiEndpoint::UserSubscribedSongList),
            "album_sublist" => Some(ApiEndpoint::SavedAlbums),
            "__download__" => Some(ApiEndpoint::Download),
            "__local_music__" => Some(ApiEndpoint::LocalMusic),
            "__recent__" => Some(ApiEndpoint::Recent),
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

    /// Resolve a navigation endpoint into a `ContentState`.
    ///
    /// Handles the API call, maps the result to `ContentState`, and logs errors.
    /// Does NOT handle caching — callers check/save cache before/after this call.
    pub async fn resolve_content(
        &self,
        api: ApiEndpoint,
        uid: Option<u64>,
        limit: u16,
    ) -> (ContentState, Option<PaginationInfo>) {
        let requires_login = matches!(
            api,
            ApiEndpoint::RecommendSongs
                | ApiEndpoint::RecommendResource
                | ApiEndpoint::UserRadioSublist
                | ApiEndpoint::UserCloudDisk
                | ApiEndpoint::LikedSongs
                | ApiEndpoint::UserSongList
                | ApiEndpoint::UserCreatedSongList
                | ApiEndpoint::UserSubscribedSongList
                | ApiEndpoint::SavedAlbums
                | ApiEndpoint::Recent
        );
        if requires_login && !self.client.is_logged_in() {
            return (ContentState::Error("未登录".into()), None);
        }
        match api {
            ApiEndpoint::RecommendSongs => {
                content_result(self.client.recommend_songs().await, |songs| {
                    ContentState::Songs(arc_songs(songs))
                })
            }
            ApiEndpoint::RecommendResource => content_result(
                self.client.recommend_resource().await,
                ContentState::SongLists,
            ),
            ApiEndpoint::Toplist => {
                content_result(self.client.toplist().await, ContentState::TopLists)
            }
            ApiEndpoint::TopSongList => content_result(
                self.client.top_song_list("全部", "hot", 0, limit).await,
                ContentState::SongLists,
            ),
            ApiEndpoint::UserRadioSublist => content_result(
                self.client.user_radio_sublist(0, limit).await,
                ContentState::SongLists,
            ),
            ApiEndpoint::UserCloudDisk => {
                match self.client.user_cloud_disk(0, limit as u32).await {
                    Ok(result) => (
                        ContentState::Songs(arc_songs(result.songs)),
                        Some(PaginationInfo {
                            api: "user_cloud_disk".into(),
                            offset: 0,
                            limit: limit as u32,
                            has_more: result.has_more,
                            total: result.count,
                            loading: false,
                        }),
                    ),
                    Err(e) => (ContentState::Error(e.to_string()), None),
                }
            }
            ApiEndpoint::Recent => content_result(self.client.recent_songs(limit).await, |songs| {
                ContentState::Songs(arc_songs(songs))
            }),
            ApiEndpoint::UserSongList => match uid {
                Some(uid) => content_result(
                    self.client.user_song_list(uid, 0, limit).await,
                    ContentState::SongLists,
                ),
                None => (ContentState::Error("未登录".into()), None),
            },
            ApiEndpoint::UserCreatedSongList => match uid {
                Some(uid) => content_result(
                    self.client.user_created_playlist(uid, 0, limit).await,
                    ContentState::SongLists,
                ),
                None => (ContentState::Error("未登录".into()), None),
            },
            ApiEndpoint::UserSubscribedSongList => match uid {
                Some(uid) => content_result(
                    self.client.user_collected_playlist(uid, 0, limit).await,
                    ContentState::SongLists,
                ),
                None => (ContentState::Error("未登录".into()), None),
            },
            ApiEndpoint::SavedAlbums => content_result(
                self.client.album_sublist(0, limit).await,
                ContentState::SongLists,
            ),
            ApiEndpoint::Search => content_result(self.client.search_hot().await, |items| {
                ContentState::HotSearch(HotSearchKeywords(
                    items.into_iter().map(|h| h.keyword).collect(),
                ))
            }),
            ApiEndpoint::TopSingers => content_result(
                self.client.top_artists(0, limit).await,
                ContentState::Singers,
            ),
            ApiEndpoint::LikedSongs => (ContentState::Error("未登录".into()), None),
            ApiEndpoint::Download | ApiEndpoint::LocalMusic => {
                unreachable!("handled separately by caller")
            }
        }
    }

    /// Load liked songs through the user's created "我喜欢的音乐" playlist so the
    /// lazy `trackIds` pagination path is reused.
    ///
    /// Returns `(content, pagination, playlist ID)`:
    /// - Content holds only the first page of songs; later pages are lazily
    ///   sliced by [`Self::load_more`] using `trackIds`;
    /// - The playlist ID is used by heartbeat mode.
    pub async fn load_liked_songs(
        &self,
        uid: u64,
        limit: u16,
    ) -> (ContentState, Option<PaginationInfo>, Option<u64>) {
        // First look for the "我喜欢的音乐" (liked) playlist among the user's created playlists, tolerating renamed titles.
        let mut playlist_id = self
            .client
            .user_created_playlist(uid, 0, limit)
            .await
            .ok()
            .and_then(|lists| find_liked_playlist(&lists));

        // When the created-playlist API is unavailable or the playlist is not found there, fall back to the general playlist API and search again.
        if playlist_id.is_none() {
            playlist_id = self
                .client
                .user_song_list(uid, 0, limit)
                .await
                .ok()
                .and_then(|lists| find_liked_playlist(&lists));
        }

        let Some(id) = playlist_id else {
            // Still no liked playlist found: fall back to the legacy API (fetches everything at once, no pagination).
            return match self.client.liked_songs(uid).await {
                Ok(songs) => (ContentState::Songs(arc_songs(songs)), None, None),
                Err(e) => (ContentState::Error(e.to_string()), None, None),
            };
        };

        let (songs, total) = match self.client.playlist_detail(id).await {
            Ok((_detail, track_ids)) => {
                let total = track_ids.len() as u64;
                let page_limit = limit as u32;
                let songs = match self.client.playlist_songs(&track_ids, 0, page_limit).await {
                    Ok(s) => s,
                    Err(e) => return (ContentState::Error(e.to_string()), None, Some(id)),
                };
                // Cache the full trackIds for later lazy pagination (LoadMore) slicing.
                if let Ok(mut guard) = self.playlist_track_ids.lock() {
                    guard.insert(id, track_ids);
                }
                (songs, total)
            }
            Err(e) => return (ContentState::Error(e.to_string()), None, Some(id)),
        };

        let limit = limit as u32;
        let pagination = PaginationInfo {
            api: format!("playlist:{id}"),
            offset: 0,
            limit,
            has_more: total > limit as u64,
            total,
            loading: false,
        };
        (
            ContentState::Songs(arc_songs(songs)),
            Some(pagination),
            Some(id),
        )
    }

    /// Ensure the playlist's `trackIds` are cached in memory (for `load_more` lazy pagination slicing).
    ///
    /// When content is restored from the disk cache, `playlist_track_ids` has not
    /// been populated yet, so top it up before scrolling to load more.
    /// Returns the track count (callers only care about the number), avoiding cloning the entire `trackIds` list.
    pub async fn ensure_playlist_track_ids(&self, id: u64) -> Option<usize> {
        if self.playlist_track_ids.lock().ok()?.contains_key(&id) {
            return None;
        }
        let (_, track_ids) = self.client.playlist_detail(id).await.ok()?;
        let count = track_ids.len();
        if let Ok(mut guard) = self.playlist_track_ids.lock() {
            guard.insert(id, track_ids);
        }
        Some(count)
    }

    /// Load lyrics for a song, with cache integration.
    pub async fn load_lyrics(&self, song_id: u64) -> Option<ncm_api::Lyrics> {
        if let Some(cached) = self.cache.load_lyrics_cache_async(song_id).await {
            return Some(cached);
        }
        match self.client.song_lyric(song_id).await {
            Ok(lyrics) => {
                let cache = self.cache.clone();
                tokio::task::spawn_blocking(move || {
                    cache.save_lyrics_cache(song_id, &lyrics);
                    lyrics
                })
                .await
                .ok()
            }
            Err(e) => {
                log::error!("Failed to fetch lyrics for {song_id}: {e}");
                None
            }
        }
    }

    /// Load lyrics for a third-party (sonar) song, with cache integration.
    ///
    /// Serves cached lyrics first (keyed by song id, shared with the NCM path)
    /// so replays don't hit the provider again. On a miss, the original sonar
    /// song is resolved from `registry` (falling back to the disk cache) and the
    /// provider's lyrics are fetched and cached.
    ///
    /// Returns `(lyric, translated)` lines, or `None` when no usable lyrics
    /// exist. `registry` maps synthetic song ids back to the sonar `Song`.
    pub async fn load_sonar_lyrics(
        &self,
        song_id: u64,
        finder: Arc<sonar::SonarFinder>,
        registry: &std::sync::Mutex<HashMap<u64, Arc<sonar::Song>>>,
    ) -> Option<(Vec<LyricLine>, Vec<LyricLine>)> {
        // Serve cached lyrics first so replays don't hit the provider again.
        if let Some(cached) = self.cache.load_lyrics_cache_async(song_id).await {
            let lyric_lines = parse_lyric_lines(&cached.lyric);
            if !lyric_lines.is_empty() {
                let tlyric_lines = parse_lyric_lines(&cached.tlyric);
                return Some((lyric_lines, tlyric_lines));
            }
        }

        let msong = registry
            .lock()
            .ok()
            .and_then(|m| m.get(&song_id).cloned())
            .or_else(|| self.cache.thirdparty_song(song_id))?;
        let lrc = finder.get_lyrics_fallback(&msong).await?;
        let lines: Vec<String> = lrc.lines().map(|s| s.to_string()).collect();
        let lyric_lines = parse_lyric_lines(&lines);
        if lyric_lines.is_empty() {
            return None;
        }
        self.cache.save_lyrics_cache(
            song_id,
            &ncm_api::Lyrics {
                lyric: lines,
                tlyric: Vec::new(),
            },
        );
        Some((lyric_lines, Vec::new()))
    }

    /// Search songs by keyword.
    pub async fn search_songs(&self, keyword: &str, limit: u16) -> ContentState {
        match self.client.search_song(keyword, 0, limit).await {
            Ok(result) => ContentState::Songs(arc_songs(result.songs)),
            Err(e) => ContentState::Error(e.to_string()),
        }
    }

    /// Load playlist detail (regular or radio).
    pub async fn load_playlist_detail(
        &self,
        id: u64,
        is_radio: bool,
        limit: u16,
    ) -> (ContentState, Option<String>, Option<PaginationInfo>) {
        if is_radio {
            match self.client.radio_program(id, 0, 1000).await {
                Ok(songs) => (ContentState::Songs(arc_songs(songs)), None, None),
                Err(e) => (ContentState::Error(e.to_string()), None, None),
            }
        } else {
            match self.client.playlist_detail(id).await {
                Ok((detail, track_ids)) => {
                    let total = track_ids.len() as u64;
                    let limit = limit as u32;
                    let songs = match self.client.playlist_songs(&track_ids, 0, limit).await {
                        Ok(s) => s,
                        Err(e) => return (ContentState::Error(e.to_string()), None, None),
                    };
                    // Cache the full trackIds for later lazy pagination slicing.
                    if let Ok(mut guard) = self.playlist_track_ids.lock() {
                        guard.insert(id, track_ids);
                    }
                    let pagination = PaginationInfo {
                        api: format!("playlist:{id}"),
                        offset: 0,
                        limit,
                        has_more: total > limit as u64,
                        total,
                        loading: false,
                    };
                    (
                        ContentState::Songs(arc_songs(songs)),
                        Some(detail.name),
                        Some(pagination),
                    )
                }
                Err(e) => (ContentState::Error(e.to_string()), None, None),
            }
        }
    }

    /// Load album songs.
    pub async fn load_album(&self, album_id: u64) -> ContentState {
        match self.client.album(album_id).await {
            Ok(detail) => ContentState::Songs(arc_songs(detail.songs)),
            Err(e) => ContentState::Error(e.to_string()),
        }
    }

    /// Load artist songs.
    pub async fn load_artist_songs(&self, artist_id: u64, limit: u16) -> ContentState {
        match self
            .client
            .singer_all_songs(artist_id, "time", 0, limit)
            .await
        {
            Ok(songs) => ContentState::Songs(arc_songs(songs)),
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

    /* -------------------------------------------------------------------------- */
    /*                                Song actions                                */
    /* -------------------------------------------------------------------------- */

    pub async fn like_song(&self, song_id: u64, like: bool) -> Result<(), ncm_api::NcmError> {
        self.client.like(song_id, like).await.map(|_| ())
    }

    /// Fetch the song ID set of the "我喜欢的音乐" playlist to maintain the local liked state.
    pub async fn load_liked_song_ids(
        &self,
        uid: u64,
    ) -> Result<std::collections::HashSet<u64>, ncm_api::NcmError> {
        let ids = self.client.liked_song_ids(uid).await?;
        Ok(ids.into_iter().collect())
    }

    pub async fn dislike_song(&self, song_id: u64) -> Result<(), ncm_api::NcmError> {
        self.client
            .recommend_song_dislike(song_id)
            .await
            .map(|_| ())
    }

    // ── Song URLs ────────────────────────────────────────────────────────

    pub async fn fetch_song_urls(
        &self,
        ids: &[u64],
        level: ncm_api::SongQuality,
    ) -> Result<Vec<ncm_api::SongUrl>, ncm_api::NcmError> {
        self.client.songs_url_v1(ids, level).await
    }

    // ── Heartbeat ────────────────────────────────────────────────────────

    pub async fn heartbeat_songs(
        &self,
        song_id: u64,
        playlist_id: u64,
    ) -> Result<Vec<ncm_api::SongInfo>, ncm_api::NcmError> {
        self.client
            .playmode_intelligence_list(song_id, playlist_id)
            .await
    }

    // ── Cloud disk upload ────────────────────────────────────────────────

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

    /// Load more with pagination. Only the cloud disk and songs within a playlist take this path:
    /// - Cloud disk: `offset/limit` pages the server directly;
    /// - Songs in a playlist: slice the `trackIds` cached from the first page and fetch this page via `songs_detail` (lazy pagination).
    pub async fn load_more(
        &self,
        api: &str,
        offset: u32,
        limit: u32,
    ) -> Option<(ContentState, PaginationInfo)> {
        // Only the cloud disk and songs within a playlist support pagination.
        // Songs in a playlist: slice the trackIds cached from the first page and fetch this page via songs_detail.
        if let Some(id_str) = api.strip_prefix("playlist:") {
            let id: u64 = id_str.parse().ok()?;
            // Compute the page slice only while holding the lock, avoiding cloning the entire trackIds list on every page.
            let (page, total, has_more) = {
                let guard = self.playlist_track_ids.lock().ok()?;
                let ids = guard.get(&id)?;
                let total = ids.len() as u64;
                let start = (offset as usize).min(ids.len());
                let end = (start + limit as usize).min(ids.len());
                let page = ids[start..end].to_vec();
                let has_more = end < ids.len();
                (page, total, has_more)
            };
            let songs = if page.is_empty() {
                Vec::new()
            } else {
                self.client.songs_detail(&page).await.ok()?
            };
            return Some((
                ContentState::Songs(arc_songs(songs)),
                PaginationInfo {
                    api: api.to_string(),
                    offset,
                    limit,
                    has_more,
                    total,
                    loading: false,
                },
            ));
        }

        if api == "user_cloud_disk" {
            let r = self.client.user_cloud_disk(offset, limit).await.ok()?;
            return Some((
                ContentState::Songs(arc_songs(r.songs)),
                PaginationInfo {
                    api: api.to_string(),
                    offset,
                    limit,
                    has_more: r.has_more,
                    total: r.count,
                    loading: false,
                },
            ));
        }

        log::error!("load_more: unsupported api {api}");
        None
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
