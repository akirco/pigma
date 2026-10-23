//! Endpoint content resolution and pagination: navigation endpoints, playlist
//! detail, album/artist pages, and `load_more` for cloud disk + playlist tracks.

use super::{ApiEndpoint, ApiService, arc_songs, content_result};
use crate::{
    state::{ContentState, HotSearchKeywords, PaginationInfo},
    utils::lock_recover,
};

impl ApiService {
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

    /// Resolve an endpoint into a displayable [`ContentState`], handling the local
    /// sources (`Download` / `LocalMusic`) and `LikedSongs` that [`Self::resolve_content`]
    /// deliberately leaves to its caller. Shared by `App::load_endpoint` (which then
    /// loads the resolved songs into the queue) and the TUI's content page, so the
    /// endpoint→content mapping lives in exactly one place.
    pub async fn resolve_endpoint_content(
        &self,
        api: ApiEndpoint,
        uid: Option<u64>,
        limit: u16,
    ) -> ContentState {
        if api == ApiEndpoint::LikedSongs {
            return match uid {
                Some(uid) => self.load_liked_songs(uid, limit).await.0,
                None => ContentState::Error("未登录".into()),
            };
        }
        match api {
            ApiEndpoint::Download => {
                let songs = self.cache().list_cached_songs_async().await;
                ContentState::Songs(songs.into_iter().map(std::sync::Arc::new).collect())
            }
            ApiEndpoint::LocalMusic => self.load_local_music().await,
            _ => self.resolve_content(api, uid, limit).await.0,
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
                    lock_recover(&self.playlist_track_ids).insert(id, track_ids);
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
                let guard = lock_recover(&self.playlist_track_ids);
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
