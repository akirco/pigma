//! Optimistic liked-state updates: keep the local set, the playerbar icon and
//! the open liked-songs list consistent before the cloud request settles.

use super::super::App;
use crate::{lock::lock_recover, service::ApiEndpoint, state::ContentState};

impl App {
    pub(super) fn handle_like_song(&mut self, id: u64, like: bool) {
        {
            let mut liked = lock_recover(&self.liked_ids);
            if like {
                liked.insert(id);
            } else {
                liked.remove(&id);
            }
        }
        if self
            .playback
            .state
            .current_song
            .as_ref()
            .is_some_and(|s| s.id == id)
        {
            self.playback.update_liked_status();
        }
        let is_liked_root = self.selected_endpoint() == Some(ApiEndpoint::LikedSongs)
            && self.state.navigation.history.is_empty();
        if !like && is_liked_root {
            // The active list and its cache are snapshots; keep them in sync with the
            // optimistic liked-state update instead of showing the removed song until reload.
            self.service
                .content_cache()
                .remove_content_cache(ApiEndpoint::LikedSongs.as_str());
            let playlist_id = self
                .state
                .navigation
                .pagination
                .as_ref()
                .and_then(|p| p.api.strip_prefix("playlist:"))
                .and_then(|id| id.parse::<u64>().ok());
            if self.state.navigation.remove_song(id)
                && let Some(playlist_id) = playlist_id
            {
                self.service.remove_cached_playlist_track(playlist_id, id);
            }
        } else if is_liked_root {
            // A like can target the current playing song (`S`) without it being the
            // selected row. Insert the newly liked song at the top of the open liked
            // list so the visible list reflects the change immediately.
            let already_present = matches!(
                self.state.navigation.content.as_ref(),
                ContentState::Songs(songs) if songs.iter().any(|song| song.id == id)
            );
            if !already_present
                && let Some(song) = self.playback.current_song().filter(|song| song.id == id)
            {
                let playlist_id = self
                    .state
                    .navigation
                    .pagination
                    .as_ref()
                    .and_then(|p| p.api.strip_prefix("playlist:"))
                    .and_then(|id| id.parse::<u64>().ok());
                if self.state.navigation.insert_song_at_top(song)
                    && let Some(playlist_id) = playlist_id
                {
                    self.service.insert_cached_playlist_track(playlist_id, id);
                }
            }
        }
        let service = self.service.clone();
        tokio::spawn(async move {
            if let Err(e) = service.like_song(id, like).await {
                log::warn!("Failed to update liked state for song {id}: {e}");
            }
        });
    }
}
