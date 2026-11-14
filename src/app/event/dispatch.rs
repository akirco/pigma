//! Application event loop: pulls events from the runtime channel and routes
//! each one to its family handler (playback, navigation, command, IPC).

use std::time::Duration;

use crossterm::event::Event as CrosstermEvent;
use tokio::{sync::mpsc::UnboundedSender, time::sleep};

use super::super::App;
use crate::{
    event::{AppEvent, AuthEvent, CommandEvent, Event, NavigationEvent, SplashEvent},
    input,
};

/// Send an app event on the event loop channel, logging a dropped-receiver
/// failure instead of panicking. Shared by the spawn-and-notify pipelines.
pub(crate) fn send_event(tx: &UnboundedSender<Event>, event: Event) {
    if tx.send(event).is_err() {
        log::error!("Failed to send event: receiver dropped");
    }
}

impl App {
    pub(crate) async fn handle_events(&mut self) -> eyre::Result<()> {
        if self.playback.state.seeking {
            tokio::select! {
                biased;
                result = self.events.next() => {
                    self.dispatch_event(result?).await?;
                }
                _ = sleep(Duration::from_millis(32)) => {}
            }
        } else {
            let event = self.events.next().await?;
            self.dispatch_event(event).await?;
        }
        Ok(())
    }

    async fn dispatch_event(&mut self, event: Event) -> eyre::Result<()> {
        match event {
            Event::Crossterm(event) => match event {
                CrosstermEvent::Key(key) if key.kind == crossterm::event::KeyEventKind::Press => {
                    input::handle_key_events(self, key)?
                }
                CrosstermEvent::Mouse(mouse) => {
                    input::handle_mouse_event(self, mouse.kind, mouse.column, mouse.row);
                }
                _ => {}
            },
            Event::App(app_event) => match app_event {
                AppEvent::Quit => self.quit(),
                AppEvent::Redraw => {}
                AppEvent::Splash(e) => self.handle_splash_event(e),
                AppEvent::Auth(e) => self.handle_auth_event(e),
                AppEvent::Playback(e) => self.handle_playback_event(e),
                AppEvent::Navigation(e) => self.handle_navigation_event(e),
                AppEvent::Command(e) => self.handle_command_event(e),
                AppEvent::Toast(msg) => self.toast(msg),
                AppEvent::Ipc(e) => self.handle_ipc_event(e).await,
            },
        }
        Ok(())
    }

    fn handle_splash_event(&mut self, event: SplashEvent) {
        match event {
            SplashEvent::Tick { progress, log } => self.handle_splash_tick(progress, log),
            SplashEvent::SetOffline => self.state.offline = true,
        }
    }

    fn handle_auth_event(&mut self, event: AuthEvent) {
        match event {
            AuthEvent::Login => self.handle_login(),
            AuthEvent::Success(info) => self.handle_login_success(info),
            AuthEvent::Error(e) => self.handle_login_error(e),
            AuthEvent::QRCreated { url, key } => self.handle_qr_created(url, key),
            AuthEvent::QRStatus(text) => self.handle_qr_status(text),
        }
    }

    fn handle_navigation_event(&mut self, event: NavigationEvent) {
        match event {
            NavigationEvent::NavSelect(api_str) => {
                if let Err(e) = self.handle_nav_select(api_str, false) {
                    log::error!("NavSelect error: {e}");
                }
            }
            NavigationEvent::ContentLoaded {
                content,
                generation,
            } => self.handle_content_loaded(content, generation),
            NavigationEvent::ContentLoadedPaged {
                content,
                pagination,
                generation,
            } => {
                self.handle_content_loaded_paged(content, pagination, generation);
            }
            NavigationEvent::PlaylistSelect { id, name } => self.handle_playlist_select(id, name),
            NavigationEvent::BreadcrumbSet { name, generation } => {
                self.handle_breadcrumb(name, generation);
            }
            NavigationEvent::SearchSong(keyword) => self.handle_search_song(keyword),
            NavigationEvent::Navigate(page) => self.state.navigation.page = page,
            NavigationEvent::SearchActivated => self.handle_search_activate(),
            NavigationEvent::SearchDeactivated => self.handle_search_deactivate(),
            NavigationEvent::ContentRestore => self.handle_content_restore(),
            NavigationEvent::CellAction(row, col) => {
                if let Err(e) = self.handle_cell_action(row, col) {
                    log::error!("CellAction error: {e}");
                }
            }
            NavigationEvent::LoadMore => self.handle_load_more(),
            NavigationEvent::LoadMoreFailed => {
                if let Some(ref mut pg) = self.state.navigation.pagination {
                    pg.loading = false;
                }
            }
            NavigationEvent::UploadCachedSong(row) => self.handle_upload_cached_song(row),
        }
    }

    fn handle_command_event(&mut self, event: CommandEvent) {
        match event {
            CommandEvent::Panel(action) => self.handle_command_panel(action),
            CommandEvent::ToggleBordered => self.state.border.enabled = !self.state.border.enabled,
        }
    }
}
