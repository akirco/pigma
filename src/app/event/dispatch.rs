//! Application event loop: pulls events from the runtime channel and routes
//! each one to its family handler (playback, navigation, command, IPC).

use std::time::Duration;

use crossterm::event::Event as CrosstermEvent;
use tokio::time::sleep;

use super::super::App;
use crate::{
    event::{AppEvent, AuthEvent, CommandEvent, Event, EventSender, NavigationEvent, SplashEvent},
    input,
    state::CommandAction,
};

/// Send an app event on the event loop channels, logging a dropped-receiver
/// failure instead of panicking. Shared by the spawn-and-notify pipelines.
pub(crate) fn send_event(tx: &EventSender, event: Event) {
    if let Err(e) = tx.send(event) {
        log::error!("Failed to send event: {e}");
    }
}

impl App {
    /// Upper bound for events folded into one frame. Control events are
    /// unbounded, so draining without a limit would let a backlog postpone the
    /// next render indefinitely; 32 keeps input latency bounded.
    const MAX_EVENTS_PER_FRAME: usize = 32;

    pub(crate) async fn handle_events(&mut self) -> eyre::Result<()> {
        // Seeking and page transitions need periodic redraws even when no
        // terminal/playback event arrives.
        if self.playback.state.seeking || self.page_transition.is_some() {
            tokio::select! {
                biased;
                result = self.events.next() => {
                    self.dispatch_event(result?).await?;
                }
                _ = sleep(Duration::from_millis(16)) => {}
            }
        } else {
            let event = self.events.next().await?;
            self.dispatch_event(event).await?;
        }
        let mut drained = 0;
        while drained < Self::MAX_EVENTS_PER_FRAME && self.state.running {
            let Some(event) = self.events.try_recv() else {
                break;
            };
            self.dispatch_event(event).await?;
            drained += 1;
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
                AppEvent::Ipc(e) => self.handle_ipc_event(e),
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
                    self.toast(format!("{} 加载失败: {e}", crate::icons::ERROR_MARK));
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
            NavigationEvent::Navigate(page) => self.start_page_transition(page),
            NavigationEvent::SearchActivated => self.handle_search_activate(),
            NavigationEvent::SearchDeactivated => self.handle_search_deactivate(),
            NavigationEvent::ContentRestore => self.handle_content_restore(),
            NavigationEvent::CellAction(row, col) => {
                if let Err(e) = self.handle_cell_action(row, col) {
                    log::error!("CellAction error: {e}");
                    self.toast(format!("{} 操作失败: {e}", crate::icons::ERROR_MARK));
                }
            }
            NavigationEvent::LoadMore => self.handle_load_more(),
            NavigationEvent::LoadMoreFailed => {
                if let Some(ref mut pg) = self.state.navigation.pagination {
                    pg.loading = false;
                }
            }
            NavigationEvent::UploadCachedSong(row) => self.handle_upload_cached_song(row),
            NavigationEvent::QueueFilterExit => self.handle_queue_filter_exit(),
            NavigationEvent::QueueFilterChanged => self.handle_queue_filter_changed(),
            NavigationEvent::EndpointResolved {
                endpoint,
                content,
                playlist_id,
            } => {
                let loaded = self.apply_endpoint_resolution(&endpoint, content, playlist_id);
                self.toast(if loaded {
                    format!("已切换到: {endpoint}")
                } else {
                    format!("切换失败: {endpoint}")
                });
            }
        }
    }

    fn handle_command_event(&mut self, event: CommandEvent) {
        match event {
            CommandEvent::Panel(action) => self.handle_command_panel(action),
            CommandEvent::ToggleBordered => {
                self.execute_command(CommandAction::ToggleBordered);
            }
        }
    }
}
