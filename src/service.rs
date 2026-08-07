use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::api::ApiEndpoint;
use crate::cache::CacheManager;
use crate::state::{ContentState, PaginationInfo};

/// Centralized API service that handles endpoint resolution, caching, and error mapping.
///
/// Callers are still responsible for `tokio::spawn` + `send_event`.
#[derive(Clone)]
pub struct ApiService {
    client: Arc<ncm_api::NcmClient>,
    cache: CacheManager,
    /// 歌单的完整 trackIds 缓存（按歌单 id），用于歌单内歌曲的惰性分页
    /// （先取全量 id，再按页切片走 `songs_detail`，避免 >1000 首一次拉完）。
    playlist_track_ids: Arc<std::sync::Mutex<HashMap<u64, Vec<u64>>>>,
}

impl ApiService {
    pub fn new(client: Arc<ncm_api::NcmClient>, cache: CacheManager) -> Self {
        Self {
            client,
            cache,
            playlist_track_ids: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    pub fn client(&self) -> &Arc<ncm_api::NcmClient> {
        &self.client
    }

    pub fn cache(&self) -> &CacheManager {
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
        match api {
            ApiEndpoint::RecommendSongs => match self.client.recommend_songs().await {
                Ok(songs) => (ContentState::Songs(songs), None),
                Err(e) => (ContentState::Error(e.to_string()), None),
            },
            ApiEndpoint::RecommendResource => match self.client.recommend_resource().await {
                Ok(lists) => (ContentState::SongLists(lists), None),
                Err(e) => (ContentState::Error(e.to_string()), None),
            },
            ApiEndpoint::Toplist => match self.client.toplist().await {
                Ok(lists) => (ContentState::TopLists(lists), None),
                Err(e) => (ContentState::Error(e.to_string()), None),
            },
            ApiEndpoint::TopSongList => {
                match self.client.top_song_list("全部", "hot", 0, limit).await {
                    Ok(lists) => (ContentState::SongLists(lists), None),
                    Err(e) => (ContentState::Error(e.to_string()), None),
                }
            }
            ApiEndpoint::UserRadioSublist => match self.client.user_radio_sublist(0, limit).await {
                Ok(lists) => (ContentState::SongLists(lists), None),
                Err(e) => (ContentState::Error(e.to_string()), None),
            },
            ApiEndpoint::UserCloudDisk => {
                match self.client.user_cloud_disk(0, limit as u32).await {
                    Ok(result) => (
                        ContentState::Songs(result.songs),
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
            ApiEndpoint::Recent => match self.client.recent_songs(limit).await {
                Ok(songs) => (ContentState::Songs(songs), None),
                Err(e) => (ContentState::Error(e.to_string()), None),
            },
            ApiEndpoint::UserSongList => match uid {
                Some(uid) => match self.client.user_song_list(uid, 0, limit).await {
                    Ok(lists) => (ContentState::SongLists(lists), None),
                    Err(e) => (ContentState::Error(e.to_string()), None),
                },
                None => (ContentState::Error("未登录".into()), None),
            },
            ApiEndpoint::UserCreatedSongList => match uid {
                Some(uid) => match self.client.user_created_playlist(uid, 0, limit).await {
                    Ok(lists) => (ContentState::SongLists(lists), None),
                    Err(e) => (ContentState::Error(e.to_string()), None),
                },
                None => (ContentState::Error("未登录".into()), None),
            },
            ApiEndpoint::UserSubscribedSongList => match uid {
                Some(uid) => match self.client.user_collected_playlist(uid, 0, limit).await {
                    Ok(lists) => (ContentState::SongLists(lists), None),
                    Err(e) => (ContentState::Error(e.to_string()), None),
                },
                None => (ContentState::Error("未登录".into()), None),
            },
            ApiEndpoint::SavedAlbums => match self.client.album_sublist(0, limit).await {
                Ok(albums) => (ContentState::SongLists(albums), None),
                Err(e) => (ContentState::Error(e.to_string()), None),
            },
            ApiEndpoint::Search => match self.client.search_hot().await {
                Ok(items) => (
                    ContentState::HotSearch(crate::state::HotSearchKeywords(
                        items.into_iter().map(|h| h.keyword).collect(),
                    )),
                    None,
                ),
                Err(e) => (ContentState::Error(e.to_string()), None),
            },
            ApiEndpoint::TopSingers => match self.client.top_artists(0, limit).await {
                Ok(singers) => (ContentState::Singers(singers), None),
                Err(e) => (ContentState::Error(e.to_string()), None),
            },
            ApiEndpoint::Download | ApiEndpoint::LocalMusic | ApiEndpoint::LikedSongs => {
                unreachable!("handled separately by caller")
            }
        }
    }

    /// Load liked songs and the user's liked playlist ID (for heartbeat mode).
    pub async fn load_liked_songs(&self, uid: u64, limit: u16) -> (ContentState, Option<u64>) {
        match self.client.liked_songs(uid).await {
            Ok(songs) => {
                let playlist_id = self
                    .client
                    .user_song_list(uid, 0, limit)
                    .await
                    .ok()
                    .and_then(|lists| {
                        lists
                            .iter()
                            .find(|l| l.name == "我喜欢的音乐")
                            .map(|l| l.id)
                    });
                (ContentState::Songs(songs), playlist_id)
            }
            Err(e) => (ContentState::Error(e.to_string()), None),
        }
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

    /// Search songs by keyword.
    pub async fn search_songs(&self, keyword: &str, limit: u16) -> ContentState {
        match self.client.search_song(keyword, 0, limit).await {
            Ok(result) => ContentState::Songs(result.songs),
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
                Ok(songs) => (ContentState::Songs(songs), None, None),
                Err(e) => (ContentState::Error(e.to_string()), None, None),
            }
        } else {
            match self.client.playlist_detail(id).await {
                Ok((detail, track_ids)) => {
                    // 缓存全量 trackIds，供后续惰性分页（LoadMore）切片使用。
                    if let Ok(mut guard) = self.playlist_track_ids.lock() {
                        guard.insert(id, track_ids.clone());
                    }
                    let total = track_ids.len() as u64;
                    let limit = limit as u32;
                    let songs = match self.client.playlist_songs(&track_ids, 0, limit).await {
                        Ok(s) => s,
                        Err(e) => return (ContentState::Error(e.to_string()), None, None),
                    };
                    let pagination = PaginationInfo {
                        api: format!("playlist:{id}"),
                        offset: 0,
                        limit,
                        has_more: total > limit as u64,
                        total,
                        loading: false,
                    };
                    (
                        ContentState::Songs(songs),
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
            Ok(detail) => ContentState::Songs(detail.songs),
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
            Ok(songs) => ContentState::Songs(songs),
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

    // ── Song actions ─────────────────────────────────────────────────────

    pub async fn like_song(&self, song_id: u64, like: bool) -> Result<(), ncm_api::NcmError> {
        self.client.like(song_id, like).await.map(|_| ())
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

    /// 分页加载更多。仅云盘与「歌单内歌曲」走此路径：
    /// - 云盘：`offset/limit` 直接翻服务端；
    /// - 歌单内歌曲：用首屏缓存的 `trackIds` 切片 + `songs_detail` 取本页（惰性分页）。
    pub async fn load_more(
        &self,
        api: &str,
        offset: u32,
        limit: u32,
    ) -> Option<(ContentState, PaginationInfo)> {
        // 仅云盘与「歌单内歌曲」支持分页。
        // 歌单内歌曲：用首屏缓存的 trackIds 切片 + songs_detail 取本页。
        if let Some(id_str) = api.strip_prefix("playlist:") {
            let id: u64 = id_str.parse().ok()?;
            let track_ids = self.playlist_track_ids.lock().ok()?.get(&id).cloned()?;
            let total = track_ids.len() as u64;
            let start = (offset as usize).min(track_ids.len());
            let end = (start + limit as usize).min(track_ids.len());
            let page = track_ids[start..end].to_vec();
            let songs = if page.is_empty() {
                Vec::new()
            } else {
                self.client.songs_detail(&page).await.ok()?
            };
            let has_more = end < track_ids.len();
            return Some((
                ContentState::Songs(songs),
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
                ContentState::Songs(r.songs),
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
