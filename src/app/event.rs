use std::time::Duration;

use crossterm::event::Event as CrosstermEvent;
use tokio::time::sleep;

use crate::event::{
    AppEvent, AuthEvent, CommandEvent, CommandPanelAction, Event, NavigationEvent, PlaybackEvent,
    SplashEvent,
};
use crate::input;
use crate::state::CommandAction;

use super::App;

impl App {
    pub(super) async fn handle_events(&mut self) -> color_eyre::Result<()> {
        if self.playback.state.seeking {
            tokio::select! {
                biased;
                result = self.state.events.next() => {
                    self.dispatch_event(result?)?;
                }
                _ = sleep(Duration::from_millis(32)) => {}
            }
        } else {
            let event = self.state.events.next().await?;
            self.dispatch_event(event)?;
        }
        Ok(())
    }

    fn dispatch_event(&mut self, event: Event) -> color_eyre::Result<()> {
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
                AppEvent::Splash(e) => self.handle_splash_event(e),
                AppEvent::Auth(e) => self.handle_auth_event(e),
                AppEvent::Playback(e) => self.handle_playback_event(e),
                AppEvent::Navigation(e) => self.handle_navigation_event(e),
                AppEvent::Command(e) => self.handle_command_event(e),
                AppEvent::Toast(msg) => self.toast(msg),
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

    fn handle_playback_event(&mut self, event: PlaybackEvent) {
        match event {
            PlaybackEvent::SongPlay(id) => self.handle_song_play(id),
            PlaybackEvent::Started => self.handle_playback_started(),
            PlaybackEvent::Progress { position, total } => {
                self.playback.on_playback_progress(position, total);
            }
            PlaybackEvent::Finished => {
                self.playback.finish_and_snapshot();
            }
            PlaybackEvent::Error(e) => {
                self.playback.on_playback_error(e);
            }
            PlaybackEvent::LyricsLoaded {
                song_id,
                lyrics,
                translated_lyrics,
            } => self
                .playback
                .on_lyrics_loaded(song_id, lyrics, translated_lyrics),
            PlaybackEvent::HeartbeatSong(song) => {
                self.playback.play_heartbeat_song(song);
            }
            PlaybackEvent::HeartbeatFallback => {
                self.playback.on_heartbeat_fallback();
            }
            PlaybackEvent::SetPlaylistId(id) => {
                // 内容（重新）加载后，之前的"全量已入队"标记失效。
                self.queued_playlists.remove(&id);
                self.playback.set_playlist_id(id);
            }
            PlaybackEvent::LikeSong(id, like) => {
                // 立即更新本地集合并刷新图标（无论云端结果如何，与现有行为一致）。
                if let Ok(mut guard) = self.liked_ids.lock() {
                    if like {
                        guard.insert(id);
                    } else {
                        guard.remove(&id);
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
                let service = self.service.clone();
                tokio::spawn(async move {
                    let _ = service.like_song(id, like).await;
                });
            }
            PlaybackEvent::LikedUpdated => {
                self.playback.update_liked_status();
            }
            PlaybackEvent::DislikeSong(id) => {
                let service = self.service.clone();
                tokio::spawn(async move {
                    match service.dislike_song(id).await {
                        Ok(_) => {}
                        Err(e) => log::warn!("Dislike failed: {e}"),
                    }
                });
            }
            PlaybackEvent::Cached(song_id) => {
                if self
                    .playback
                    .state
                    .current_song
                    .as_ref()
                    .is_some_and(|s| s.id == song_id)
                {
                    self.playback.state.cached = true;
                }
            }
            PlaybackEvent::QueueAppend { key, songs } => {
                self.playback.append_songs_to_key(&key, songs);
            }
            PlaybackEvent::QueueLoadDone { playlist_id } => {
                self.queued_playlists.insert(playlist_id);
            }
        }
    }

    fn handle_navigation_event(&mut self, event: NavigationEvent) {
        match event {
            NavigationEvent::NavSelect(api_str) => {
                if let Err(e) = self.handle_nav_select(api_str, false) {
                    log::error!("NavSelect error: {e}");
                }
            }
            NavigationEvent::ContentLoaded(content) => self.handle_content_loaded(content),
            NavigationEvent::ContentLoadedPaged {
                content,
                pagination,
                generation,
            } => {
                self.handle_content_loaded_paged(content, pagination, generation);
            }
            NavigationEvent::PlaylistSelect { id, name } => self.handle_playlist_select(id, name),
            NavigationEvent::BreadcrumbSet(name) => self.handle_breadcrumb(name),
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

    fn handle_command_panel(&mut self, action: CommandPanelAction) {
        let panel = &mut self.state.command_panel;
        match action {
            CommandPanelAction::Open => {
                panel.open = true;
                panel.selected = 0;
            }
            CommandPanelAction::Close => panel.back(),
            CommandPanelAction::Previous => {
                if let Some(items) = panel.current_items() {
                    let len = items.len();
                    panel.selected = (panel.selected + len - 1) % len;
                }
            }
            CommandPanelAction::Next => {
                if let Some(items) = panel.current_items() {
                    let len = items.len();
                    panel.selected = (panel.selected + 1) % len;
                }
            }
            CommandPanelAction::Select => {
                let action = panel.enter();
                if action.is_some() {
                    panel.open = false;
                }
                if let Some(action) = action {
                    self.execute_command(action);
                }
            }
        }
    }

    fn execute_command(&mut self, action: CommandAction) {
        match action {
            CommandAction::ToggleBordered => {
                self.state.border.enabled = !self.state.border.enabled;
                self.toast(format!(
                    "BORDER MODE: {}",
                    if self.state.border.enabled {
                        "ON"
                    } else {
                        "OFF"
                    }
                ));
            }
            CommandAction::ToggleSaveOnPlay => {
                let enabled = !self.config.cache.save_on_play;
                self.config.cache.save_on_play = enabled;
                self.playback.set_save_on_play(enabled);
                self.config.save();
                self.toast(format!("边听边存: {}", if enabled { "ON" } else { "OFF" }));
            }
            CommandAction::SwitchTheme(name) => {
                let msg = format!("THEME: {name}");
                self.config.default_theme = name;
                self.config.save();
                self.toast(msg);
            }
        }
    }
}
