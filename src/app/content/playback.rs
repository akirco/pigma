//! Playback-start bookkeeping and headless/IPC endpoint loading.

use ncm_api::SongList;

use super::super::App;
use crate::{lock::lock_recover, service::ApiEndpoint, state::ContentState};

impl App {
    pub(crate) fn handle_playback_started(&mut self) {
        self.playback.on_playback_started();

        let Some(song) = self.playback.current_song() else {
            return;
        };
        if let ContentState::Songs(songs) = self.state.navigation.content.as_ref()
            && let Some(pos) = songs.iter().position(|s| s.id == song.id)
        {
            self.state.navigation.content_selected = pos;
        }
        self.toast(format!("{}  {}", crate::icons::PLAY_INDICATOR, song.name));

        self.spawn_lyrics_load(song.id);

        // Clear the cover so a new song never shows the previous one's
        // cover while its own cover is loading (or missing).
        *lock_recover(&self.playback.state.cover.protocol) = None;
        self.spawn_cover_load(song.id, song.pic_url.clone());
    }

    /// Resolve an endpoint string into playable songs and load them into the
    /// queue without starting playback. Shared by the daemon bootstrap
    /// (`--daemon`) and the IPC `pigma msg switch-list` action. Returns whether
    /// songs were loaded.
    pub(crate) async fn load_endpoint(
        &mut self,
        api_str: &str,
        playlist_index: Option<usize>,
    ) -> bool {
        let api = ApiEndpoint::parse(api_str).unwrap_or(ApiEndpoint::RecommendSongs);
        let uid = self.state.navigation.user.as_ref().map(|u| u.uid);
        let content = self
            .service
            .resolve_endpoint_content(api, uid, self.config.search_limit)
            .await;

        // Playlist/toplist endpoints resolve to a *list* of playlists; pick one
        // (default first, or `--playlist N`) and load its songs.
        let content = match content {
            ContentState::SongLists(lists) => {
                self.load_endpoint_list_songs(api, lists, playlist_index)
                    .await
            }
            ContentState::TopLists(lists) => {
                let lists: Vec<SongList> = lists
                    .into_iter()
                    .map(|t| SongList {
                        id: t.id,
                        name: t.name,
                        cover_img_url: t.cover,
                        author: String::new(),
                        subscribed: false,
                    })
                    .collect();
                self.load_endpoint_list_songs(api, lists, playlist_index)
                    .await
            }
            other => other,
        };

        match content {
            ContentState::Songs(songs) if !songs.is_empty() => {
                // Use the nav display name (e.g. " 我喜欢的音乐") as the queue key
                // so the queue title matches the TUI, falling back
                // to the raw endpoint string (e.g. `liked`) when unknown.
                let key = self
                    .config
                    .navigation
                    .name_for_api(api_str)
                    .unwrap_or_else(|| api_str.to_string());
                self.playback.load_songs(&key, songs, 0);
                true
            }
            ContentState::Error(e) => {
                log::error!("endpoint {api_str}: {e}");
                false
            }
            _ => {
                log::warn!("endpoint {api_str} resolved to no playable songs");
                false
            }
        }
    }

    /// Resolve a playlist-list (or toplist) to the songs of the selected
    /// playlist: `--playlist N` picks the 1-based `N`-th entry (default 1st).
    async fn load_endpoint_list_songs(
        &mut self,
        api: ApiEndpoint,
        lists: Vec<SongList>,
        playlist_index: Option<usize>,
    ) -> ContentState {
        if lists.is_empty() {
            return ContentState::Empty;
        }
        let index = playlist_index.unwrap_or(1).max(1) - 1;
        let Some(list) = lists.get(index) else {
            return ContentState::Error(format!(
                "playlist index {} out of range (1..={})",
                index + 1,
                lists.len()
            ));
        };
        self.playback.set_playlist_id(list.id);
        let limit = self.config.search_limit;
        match api {
            ApiEndpoint::SavedAlbums => self.service.load_album(list.id).await,
            ApiEndpoint::UserRadioSublist => {
                let (state, _, _) = self
                    .service
                    .load_playlist_detail(list.id, true, limit)
                    .await;
                state
            }
            _ => {
                let (state, _, _) = self
                    .service
                    .load_playlist_detail(list.id, false, limit)
                    .await;
                state
            }
        }
    }
}
