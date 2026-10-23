//! Liked songs and the in-memory playlist `trackIds` used for lazy pagination.

use std::collections::HashSet;

use super::{ApiService, arc_songs, find_liked_playlist};
use crate::{
    state::{ContentState, PaginationInfo},
    utils::lock_recover,
};

impl ApiService {
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

        // `liked_song_ids` is authoritative. Playlist detail can lag after an unlike and
        // must not normally drive this list, otherwise removed songs remain visible.
        let track_ids = match self.client.liked_song_ids(uid).await {
            Ok(ids) => ids,
            Err(liked_error) => match self.client.playlist_detail(id).await {
                Ok((_detail, ids)) => {
                    log::warn!(
                        "liked_song_ids failed ({liked_error}); falling back to playlist detail"
                    );
                    ids
                }
                Err(_) => {
                    return (ContentState::Error(liked_error.to_string()), None, Some(id));
                }
            },
        };
        let total = track_ids.len() as u64;
        let page_limit = limit as u32;
        let songs = match self.client.playlist_songs(&track_ids, 0, page_limit).await {
            Ok(songs) => songs,
            Err(e) => return (ContentState::Error(e.to_string()), None, Some(id)),
        };
        // Cache the authoritative IDs for later lazy pagination (LoadMore) slicing.
        lock_recover(&self.playlist_track_ids).insert(id, track_ids);

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

    /// Remove a track from a cached playlist ID list after an optimistic local mutation.
    pub fn remove_cached_playlist_track(&self, playlist_id: u64, song_id: u64) {
        if let Some(track_ids) = lock_recover(&self.playlist_track_ids).get_mut(&playlist_id) {
            track_ids.retain(|id| *id != song_id);
        }
    }

    /// Insert a track at the front of a cached playlist ID list after an optimistic
    /// local mutation, keeping `load_more` slicing aligned with the visible list.
    pub fn insert_cached_playlist_track(&self, playlist_id: u64, song_id: u64) {
        if let Some(track_ids) = lock_recover(&self.playlist_track_ids).get_mut(&playlist_id) {
            track_ids.retain(|id| *id != song_id);
            track_ids.insert(0, song_id);
        }
    }

    /// Ensure the playlist's `trackIds` are cached in memory (for `load_more` lazy pagination slicing).
    ///
    /// When content is restored from the disk cache, `playlist_track_ids` has not
    /// been populated yet, so top it up before scrolling to load more.
    /// Returns the track count (callers only care about the number), avoiding cloning the entire `trackIds` list.
    pub async fn ensure_playlist_track_ids(&self, id: u64) -> Option<usize> {
        if lock_recover(&self.playlist_track_ids).contains_key(&id) {
            return None;
        }
        let (_, track_ids) = self.client.playlist_detail(id).await.ok()?;
        let count = track_ids.len();
        lock_recover(&self.playlist_track_ids).insert(id, track_ids);
        Some(count)
    }

    pub async fn like_song(&self, song_id: u64, like: bool) -> Result<(), ncm_api::NcmError> {
        self.client.like(song_id, like).await.map(|_| ())
    }

    /// Fetch the song ID set of the "我喜欢的音乐" playlist to maintain the local liked state.
    pub async fn load_liked_song_ids(&self, uid: u64) -> Result<HashSet<u64>, ncm_api::NcmError> {
        let ids = self.client.liked_song_ids(uid).await?;
        Ok(ids.into_iter().collect())
    }

    pub async fn dislike_song(&self, song_id: u64) -> Result<(), ncm_api::NcmError> {
        self.client
            .recommend_song_dislike(song_id)
            .await
            .map(|_| ())
    }
}
