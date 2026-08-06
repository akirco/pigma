pub(crate) mod content;
pub(crate) mod login;
pub(crate) mod navigation;
pub(crate) mod search;
pub(crate) mod splash;

use std::time::Duration;

use crossterm::event::Event as CrosstermEvent;
use ratatui::{DefaultTerminal, Frame};
use tokio::time::sleep;

pub use crate::state::App;

use crate::{
    event::{
        AppEvent, AuthEvent, CommandEvent, CommandPanelAction, Event, NavigationEvent,
        PlaybackEvent, SplashEvent,
    },
    input,
    state::Page,
};

pub(crate) use splash::send_event;

impl App {
    pub async fn run(mut self, mut terminal: DefaultTerminal) -> color_eyre::Result<()> {
        self.start_splash_boot();
        while self.state.running {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events().await?;

            if self.state.splash.boot_complete && self.state.navigation.page == Page::Splash {
                if self.state.offline {
                    self.navigate_to_local();
                } else if self.service.client().is_logged_in() {
                    self.navigate_to_main();
                    let service = self.service.clone();
                    let sender = self.state.events.sender();
                    tokio::spawn(async move {
                        match service.login_status().await {
                            Ok(info) => {
                                if sender.send(AuthEvent::Success(info).into()).is_err() {
                                    log::error!("Failed to send LoginSuccess: receiver dropped");
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to get login status: {e}");
                            }
                        }
                    });
                } else {
                    self.state.navigation.page = Page::Login;
                }
            }
        }
        Ok(())
    }

    fn navigate_to_local(&mut self) {
        self.state.navigation.page = Page::Main;
        self.state.navigation.nav.focus_section = 1;
        if let Some(s) = self.state.navigation.nav.sections.get(1)
            && let Some(i) = s.items.iter().position(|item| item.name == "本地音乐")
        {
            self.state.navigation.nav.section_states[1].select(Some(i));
        }
        self.state.navigation.nav.subtitle = Some("本地音乐".into());
        let sender = self.state.events.sender();
        send_event(
            &sender,
            NavigationEvent::NavSelect("__local_music__".into()).into(),
        );
        self.state.navigation.content_selected = 0;
    }

    pub(crate) fn navigate_to_main(&mut self) {
        self.state.navigation.page = Page::Main;

        let api = self
            .state
            .navigation
            .nav
            .sections
            .first()
            .and_then(|s| s.items.first())
            .and_then(|i| i.api.clone());
        if let Some(api) = api {
            let sender = self.state.events.sender();
            send_event(&sender, NavigationEvent::NavSelect(api).into());
        }
    }

    async fn handle_events(&mut self) -> color_eyre::Result<()> {
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
            PlaybackEvent::SetPlaylistId(id) => self.playback.set_playlist_id(id),
            PlaybackEvent::LikeSong(id) => {
                let service = self.service.clone();
                tokio::spawn(async move {
                    let _ = service.like_song(id, true).await;
                });
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
        }
    }

    fn handle_navigation_event(&mut self, event: NavigationEvent) {
        match event {
            NavigationEvent::NavSelect(api_str) => {
                if let Err(e) = self.handle_nav_select(api_str) {
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

    fn draw(&mut self, frame: &mut Frame) {
        crate::ui::draw(frame, self);
    }
}
