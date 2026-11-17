//! Playback-start bookkeeping and headless/IPC endpoint loading.

use ncm_api::SongList;

use super::super::{App, event::send_event};
use crate::{
    event::NavigationEvent,
    lock::lock_recover,
    service::{ApiEndpoint, ApiService},
    state::ContentState,
};

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
        *lock_recover(&self.cover.protocol) = None;
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
        let uid = self.state.navigation.user.as_ref().map(|u| u.uid);
        let (content, playlist_id) = resolve_endpoint(
            &self.service,
            api_str,
            uid,
            self.config.search_limit,
            playlist_index,
        )
        .await;
        self.apply_endpoint_resolution(api_str, content, playlist_id)
    }

    /// Start resolving `endpoint` in the background and emit
    /// [`NavigationEvent::EndpointResolved`] when it is ready. Used by IPC so a
    /// `switch-list` request cannot block the render loop on network I/O.
    pub(crate) fn spawn_endpoint_load(&self, endpoint: String, playlist_index: Option<usize>) {
        let service = self.service.clone();
        let uid = self.state.navigation.user.as_ref().map(|u| u.uid);
        let limit = self.config.search_limit;
        let sender = self.events.sender();
        tokio::spawn(async move {
            let (content, playlist_id) =
                resolve_endpoint(&service, &endpoint, uid, limit, playlist_index).await;
            send_event(
                &sender,
                NavigationEvent::EndpointResolved {
                    endpoint,
                    content,
                    playlist_id,
                }
                .into(),
            );
        });
    }

    /// Apply an asynchronously resolved endpoint to the playback queue.
    pub(crate) fn apply_endpoint_resolution(
        &mut self,
        api_str: &str,
        content: ContentState,
        playlist_id: Option<u64>,
    ) -> bool {
        if let Some(id) = playlist_id {
            self.playback.set_playlist_id(id);
        }
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
}

/// Resolve an endpoint (and optionally one playlist inside a list endpoint)
/// without mutating `App`, so it can run on a background task.
async fn resolve_endpoint(
    service: &ApiService,
    api_str: &str,
    uid: Option<u64>,
    limit: u16,
    playlist_index: Option<usize>,
) -> (ContentState, Option<u64>) {
    let Some(api) = ApiEndpoint::parse(api_str) else {
        return (
            ContentState::Error(format!("未知 endpoint: {api_str}")),
            None,
        );
    };
    let content = service.resolve_endpoint_content(api, uid, limit).await;

    // Playlist/toplist endpoints resolve to a *list* of playlists; pick one
    // (default first, or `--playlist N`) and load its songs.
    match content {
        ContentState::SongLists(lists) => {
            resolve_endpoint_list(service, api, lists, playlist_index, limit).await
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
            resolve_endpoint_list(service, api, lists, playlist_index, limit).await
        }
        other => (other, None),
    }
}

/// Resolve a playlist-list (or toplist) to the songs of the selected
/// playlist: `--playlist N` picks the 1-based `N`-th entry (default 1st).
async fn resolve_endpoint_list(
    service: &ApiService,
    api: ApiEndpoint,
    lists: Vec<SongList>,
    playlist_index: Option<usize>,
    limit: u16,
) -> (ContentState, Option<u64>) {
    if lists.is_empty() {
        return (ContentState::Empty, None);
    }
    let index = playlist_index.unwrap_or(1).max(1) - 1;
    let Some(list) = lists.get(index) else {
        return (
            ContentState::Error(format!(
                "playlist index {} out of range (1..={})",
                index + 1,
                lists.len()
            )),
            None,
        );
    };
    let playlist_id = list.id;
    let content = match api {
        ApiEndpoint::SavedAlbums => service.load_album(playlist_id).await,
        ApiEndpoint::UserRadioSublist => {
            let (state, _, _) = service.load_playlist_detail(playlist_id, true, limit).await;
            state
        }
        _ => {
            let (state, _, _) = service
                .load_playlist_detail(playlist_id, false, limit)
                .await;
            state
        }
    };
    (content, Some(playlist_id))
}
