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
    event::{AppEvent, CommandPanelAction, Event},
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
                                if sender
                                    .send(Event::App(AppEvent::LoginSuccess(info)))
                                    .is_err()
                                {
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
            send_event(&sender, Event::App(AppEvent::NavSelect(api)));
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
                e => self.handle_app_event(e)?,
            },
        }
        Ok(())
    }

    fn handle_app_event(&mut self, event: AppEvent) -> color_eyre::Result<()> {
        match event {
            AppEvent::Quit => self.quit(),
            AppEvent::SplashTick { progress, log } => self.handle_splash_tick(progress, log),
            AppEvent::Login => self.handle_login(),
            AppEvent::LoginSuccess(info) => self.handle_login_success(info),
            AppEvent::LoginError(e) => self.handle_login_error(e),
            AppEvent::CaptchaSent => self.handle_captcha_sent(),
            AppEvent::QRCreated { url, key } => self.handle_qr_created(url, key),
            AppEvent::QRStatus(text) => self.handle_qr_status(text),
            AppEvent::NavSelect(api_str) => self.handle_nav_select(api_str)?,
            AppEvent::BreadcrumbSet(name) => self.handle_breadcrumb(name),
            AppEvent::ContentLoaded(content) => self.handle_content_loaded(content),
            AppEvent::PlaylistSelect { id, name } => self.handle_playlist_select(id, name),
            AppEvent::SongPlay(id) => self.handle_song_play(id),
            AppEvent::PlaybackStarted => self.handle_playback_started(),
            AppEvent::PlaybackProgress { position, total } => {
                self.playback.on_playback_progress(position, total);
            }
            AppEvent::PlaybackFinished => {
                self.playback.finish_and_snapshot();
                self.report_pending_play();
            }
            AppEvent::PlaybackError(e) => {
                self.playback.on_playback_error(e);
                self.report_pending_play();
            }
            AppEvent::LyricsLoaded {
                song_id,
                lyrics,
                translated_lyrics,
            } => self
                .playback
                .on_lyrics_loaded(song_id, lyrics, translated_lyrics),
            AppEvent::HeartbeatSong(song) => {
                self.playback.play_heartbeat_song(song);
                self.report_pending_play();
            }
            AppEvent::HeartbeatFallback => {
                self.playback.on_heartbeat_fallback();
                self.report_pending_play();
            }
            AppEvent::SearchSong(keyword) => self.handle_search_song(keyword),
            AppEvent::LocalMusicLoaded(songs) => {
                self.state.local_music = crate::state::ContentState::Songs(songs);
            }
            AppEvent::SetOffline => self.state.offline = true,
            AppEvent::Navigate(page) => self.state.navigation.page = page,
            AppEvent::CommandPanel(action) => self.handle_command_panel(action),
            AppEvent::SearchActivated => self.handle_search_activate(),
            AppEvent::SearchDeactivated => self.handle_search_deactivate(),
            AppEvent::ToggleBordered => self.state.bordered = !self.state.bordered,
            AppEvent::ExecuteCommand(action) => self.execute_command(action),
            AppEvent::ContentRestore => self.handle_content_restore(),
            AppEvent::CellAction(row, col) => self.handle_cell_action(row, col)?,
            AppEvent::LoadMore => self.handle_load_more(),
            AppEvent::ContentLoadedPaged { content, pagination } => {
                self.handle_content_loaded_paged(content, pagination)
            }
        }
        Ok(())
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
