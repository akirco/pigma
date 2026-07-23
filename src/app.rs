pub(crate) mod content;
pub(crate) mod login;
pub(crate) mod navigation;
pub(crate) mod search;
pub(crate) mod splash;

use crossterm::event::Event as CrosstermEvent;
use ratatui::{DefaultTerminal, Frame};
use tokio::time::Duration;

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

            match tokio::time::timeout(Duration::from_millis(32), self.handle_events()).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(e),
                Err(_) => {}
            }

            if self.state.splash.boot_complete && self.state.navigation.page == Page::Splash {
                if self.state.offline {
                    self.navigate_to_local();
                } else if self.api.is_logged_in() {
                    self.navigate_to_main();
                    let api = self.api.clone();
                    let sender = self.state.events.sender();
                    tokio::spawn(async move {
                        match api.login_status().await {
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
        self.state
            .navigation
            .set_content(self.state.local_music.clone());
        self.state.navigation.nav.subtitle = Some("本地音乐".into());
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
        match self.state.events.next().await? {
            Event::Crossterm(event) => match event {
                CrosstermEvent::Key(key) if key.kind == crossterm::event::KeyEventKind::Press => {
                    input::handle_key_events(self, key)?
                }
                CrosstermEvent::Mouse(mouse) => {
                    input::handle_mouse_event(self, mouse.kind);
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
            },
        }
        Ok(())
    }

    fn handle_splash_event(&mut self, event: SplashEvent) {
        match event {
            SplashEvent::Tick { progress, log } => self.handle_splash_tick(progress, log),
            SplashEvent::LocalMusicLoaded(songs) => {
                self.state.local_music = crate::state::ContentState::Songs(songs);
            }
            SplashEvent::SetOffline => self.state.offline = true,
        }
    }

    fn handle_auth_event(&mut self, event: AuthEvent) {
        match event {
            AuthEvent::Login => self.handle_login(),
            AuthEvent::Success(info) => self.handle_login_success(info),
            AuthEvent::Error(e) => self.handle_login_error(e),
            AuthEvent::CaptchaSent => self.handle_captcha_sent(),
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
                self.report_pending_play();
            }
            PlaybackEvent::Error(e) => {
                self.playback.on_playback_error(e);
                self.report_pending_play();
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
                self.report_pending_play();
            }
            PlaybackEvent::HeartbeatFallback => {
                self.playback.on_heartbeat_fallback();
                self.report_pending_play();
            }
            PlaybackEvent::SetPlaylistId(id) => self.playback.set_playlist_id(id),
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
            } => {
                self.handle_content_loaded_paged(content, pagination);
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
        }
    }

    fn handle_command_event(&mut self, event: CommandEvent) {
        match event {
            CommandEvent::Panel(action) => self.handle_command_panel(action),
            CommandEvent::Execute(action) => self.execute_command(action),
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

    pub(crate) fn report_pending_play(&mut self) {
        if let Some((song_id, time_ms)) = self.playback.take_pending_report() {
            let api = self.api.clone();
            tokio::spawn(async move {
                log::info!("上报播放记录: song_id={song_id}, time_ms={time_ms}");
                if let Err(e) = api.report_play(song_id, time_ms, None).await {
                    log::error!("Failed to report play for {song_id}: {e}");
                }
            });
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        crate::ui::draw(frame, self);
    }
}
