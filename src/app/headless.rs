//! Headless daemon mode (`pigma --daemon <endpoint>`): no terminal is opened.
//! Loads the endpoint as the initial list, starts playing it, and runs the
//! IPC socket so `pigma status` / `pigma msg` can observe and control it.
//! Stops on SIGINT/SIGTERM (saving the session).

use std::sync::Arc;

use ncm_api::SongList;

use super::App;
use crate::{
    service::ApiEndpoint,
    state::{ContentState, Page},
};

impl App {
    /// Headless daemon mode: load the endpoint, start the IPC server, and pump
    /// the event loop until SIGINT/SIGTERM.
    pub async fn run_headless(
        mut self,
        endpoint: &str,
        playlist_index: Option<usize>,
    ) -> color_eyre::Result<()> {
        self.state.navigation.page = Page::Main;
        let _ipc_guard = crate::ipc::start_server(
            Arc::clone(&self.ipc.status),
            Arc::clone(&self.ipc.queue),
            self.ipc.status_tx.clone(),
            self.state.events.sender(),
            Arc::clone(&self.search.engine),
        );

        // Resolve the user session from cookies so login-gated endpoints like
        // `liked` can obtain the uid even without an interactive QR login.
        if self.state.navigation.user.is_none() {
            match self.service.login_status().await {
                Ok(info) => {
                    let uid = info.uid;
                    self.state.navigation.user = Some(info);
                    // Preload the liked-song id set so player-bar like status and
                    // the waybar heart icon reflect reality in headless mode too.
                    match self.service.load_liked_song_ids(uid).await {
                        Ok(ids) => {
                            if let Ok(mut guard) = self.liked_ids.lock() {
                                *guard = ids;
                            }
                        }
                        Err(e) => log::warn!("headless: failed to load liked song ids: {e}"),
                    }
                }
                Err(e) => log::warn!("headless: failed to resolve user session: {e}"),
            }
        }

        // Forward termination signals to the event loop so shutdown goes through
        // `App::quit` (session save + cookie flush) like a normal quit.
        let tx = self.state.events.sender();
        tokio::spawn(async move {
            wait_shutdown_signal().await;
            let _ = tx.send(crate::event::AppEvent::Quit.into());
        });

        self.bootstrap_headless(endpoint, playlist_index).await;
        while self.state.running {
            self.update_status_snapshot();
            self.handle_events().await?;
        }
        Ok(())
    }

    /// Resolve the `--daemon` endpoint and load its songs into the queue **without
    /// starting playback**; the user starts it via `pigma msg play` / toggle.
    async fn bootstrap_headless(&mut self, api_str: &str, playlist_index: Option<usize>) {
        let loaded = self.load_endpoint(api_str, playlist_index).await;
        if loaded {
            let name = self
                .playback
                .current_song()
                .map(|s| s.name.clone())
                .unwrap_or_default();
            log::info!("headless: loaded {} (paused, press play)", name);
        }
    }

    /// Resolve an endpoint string into playable songs and load them into the
    /// queue without starting playback. Shared by the daemon bootstrap
    /// (`--daemon`) and the IPC `pigma msg switch-list` action. Returns whether
    /// songs were loaded.
    pub(super) async fn load_endpoint(
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
                self.load_headless_list_songs(api, lists, playlist_index)
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
                self.load_headless_list_songs(api, lists, playlist_index)
                    .await
            }
            other => other,
        };

        match content {
            ContentState::Songs(songs) if !songs.is_empty() => {
                // Use the nav display name (e.g. " 我喜欢的音乐") as the queue key
                // so the daemon shows the same title the TUI would, falling back
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
                log::error!("headless: {api_str}: {e}");
                false
            }
            _ => {
                log::warn!("headless: {api_str} resolved to no playable songs");
                false
            }
        }
    }

    /// Resolve a playlist-list (or toplist) to the songs of the selected
    /// playlist: `--playlist N` picks the 1-based `N`-th entry (default 1st).
    async fn load_headless_list_songs(
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

#[cfg(unix)]
async fn wait_shutdown_signal() {
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("failed to install SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = sigterm.recv() => {}
    }
}

#[cfg(not(unix))]
async fn wait_shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
