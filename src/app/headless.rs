//! Headless daemon mode (`pigma --daemon <endpoint>`): no terminal is opened.
//! Loads the endpoint as the initial list, starts playing it, and runs the
//! IPC socket so `pigma status` / `pigma msg` can observe and control it.
//! Stops on SIGINT/SIGTERM (saving the session).

use std::sync::Arc;

use super::App;
use crate::{state::Page, utils::lock_recover};

impl App {
    /// Headless daemon mode: load the endpoint, start the IPC server, and pump
    /// the event loop until SIGINT/SIGTERM.
    pub async fn run_headless(
        mut self,
        endpoint: &str,
        playlist_index: Option<usize>,
    ) -> eyre::Result<()> {
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
                            *lock_recover(&self.liked_ids) = ids;
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
            self.ipc.update(&self.playback);
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
