pub(crate) mod content;
pub(crate) mod login;
pub(crate) mod navigation;
pub(crate) mod search;
pub(crate) mod splash;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossterm::event::Event as CrosstermEvent;
use ratatui::layout::Rect;
use ratatui::widgets::TableState;
use ratatui::{DefaultTerminal, Frame};
use ratatui_image::picker::Picker;
use reqwest::Client;
use sonar::{SonarFinder, Song};
use tokio::time::sleep;

use crate::cache::CacheManager;
use crate::config::{Config, ProxyTarget, Theme, ThemeRegistry, theme_fallback};
use crate::event::EventHandler;
use crate::event::{
    AppEvent, AuthEvent, CommandEvent, CommandPanelAction, Event, NavigationEvent, PlaybackEvent,
    SplashEvent,
};
use crate::input;
use crate::playback::PlaybackEngine;
use crate::service::ApiService;
use crate::state::{
    CommandAction, CommandItem, CommandPanel, ContentState, HelpState, LoginState, NavState,
    NavigationState, Page, SearchProvider, SearchState, SplashState, State, TableMode,
};
use crate::ui;
use crate::utils::pigma_cache_dir;
use crate::utils::terminal::{ImageProtocol, best_image_protocol};

pub(crate) use splash::send_event;

/// Main application state and entry point for the pigma TUI.
pub struct App {
    pub config: Config,
    pub state: State,
    pub playback: PlaybackEngine,
    pub theme_registry: ThemeRegistry,
    pub service: ApiService,
    pub picker: Picker,
    /// Blocking HTTP client for cover downloads (honours the proxy config).
    pub cover_http: Client,
    /// Shared sonar finder used for per-provider search and playback fallback.
    pub finder: Arc<SonarFinder>,
    /// Original sonar songs for search results, keyed by synthetic song id.
    pub sonar_songs: Arc<Mutex<HashMap<u64, Arc<Song>>>>,
}

impl App {
    pub fn new(config: Config) -> color_eyre::Result<Self> {
        let border = config.border.clone();

        let events = EventHandler::new();
        let tx = events.sender();

        let theme_registry = ThemeRegistry::new(config.themes.clone());

        let theme_names: Vec<String> = theme_registry
            .all_names()
            .into_iter()
            .map(|s| s.to_string())
            .collect();

        let theme_children: Vec<CommandItem> = theme_names
            .into_iter()
            .map(|name| {
                let action = CommandAction::SwitchTheme(name.clone());
                CommandItem::Action { name, action }
            })
            .collect();

        let commands = vec![
            CommandItem::SubMenu {
                name: "Switch Theme".into(),
                children: theme_children,
            },
            CommandItem::Action {
                name: "Toggle Border Mode".into(),
                action: CommandAction::ToggleBordered,
            },
        ];

        let mut command_panel = CommandPanel::new();
        command_panel.levels = vec![commands];

        let proxy = config.proxy.as_str();
        let empty = proxy.is_empty();
        // `normal`（国内默认）：仅 YouTube 走代理；`reversed`（海外）：除 YouTube
        // 外全部走代理；`both`：全部走代理。
        let ncm_proxy = if !empty
            && matches!(
                config.proxy_target,
                ProxyTarget::Reversed | ProxyTarget::Both
            ) {
            proxy
        } else {
            ""
        };
        let search_proxy = if !empty
            && matches!(
                config.proxy_target,
                ProxyTarget::Reversed | ProxyTarget::Both
            ) {
            proxy
        } else {
            ""
        };
        let youtube_proxy =
            if !empty && matches!(config.proxy_target, ProxyTarget::Normal | ProxyTarget::Both) {
                proxy
            } else {
                ""
            };
        let stream_proxy = search_proxy;

        let api = if ncm_proxy.is_empty() {
            Arc::new(ncm_api::NcmClient::new()?)
        } else {
            Arc::new(ncm_api::NcmClient::builder().proxy(ncm_proxy).build()?)
        };

        let quality = ncm_api::SongQuality::from_level(&config.cache.quality)
            .unwrap_or(ncm_api::SongQuality::Higher);

        let cache_dir = {
            let path = std::path::Path::new(&config.cache.cache_dir);
            if path.is_absolute() {
                std::path::PathBuf::from(&config.cache.cache_dir)
            } else {
                pigma_cache_dir().join(&config.cache.cache_dir)
            }
        };
        let base_dir = pigma_cache_dir();

        let finder = Arc::new({
            let mut sources: Vec<sonar::SonarSource> = Vec::new();
            for name in &config.source_fallback.providers {
                let source = match name.as_str() {
                    "kuwo" => sonar::SonarSource::Kuwo,
                    "kugou" => sonar::SonarSource::Kugou,
                    "bilivideo" => sonar::SonarSource::BiliVideo,
                    "youtube" => sonar::SonarSource::Youtube,
                    _ => continue,
                };
                if !sources.contains(&source) {
                    sources.push(source);
                }
            }
            let search_config = sonar::SearchConfig::new()
                .with_providers(sources)
                .with_timeout(config.source_fallback.timeout_ms)
                .with_search_proxy(search_proxy.to_string())
                .with_youtube_proxy(youtube_proxy.to_string());
            sonar::SonarFinder::new(search_config)
        });

        // Search providers offered in the search bar: 网易云 always first,
        // followed by the configured sonar fallback sources.
        let mut search_providers = vec![SearchProvider::Ncm];
        for source in finder
            .sources()
            .iter()
            .map(|s| SearchProvider::from_sonar(*s))
        {
            if !search_providers.contains(&source) {
                search_providers.push(source);
            }
        }

        let cache = CacheManager::new(
            cache_dir,
            base_dir.clone(),
            config.cache.cache_template.clone(),
        );

        let service = ApiService::new(api.clone(), cache.clone());

        let mut picker = ratatui_image::picker::Picker::from_query_stdio()
            .unwrap_or_else(|_| ratatui_image::picker::Picker::halfblocks());

        match best_image_protocol() {
            Some(ImageProtocol::Kitty) => {
                log::debug!("ImageProtocol::Kitty");
                picker.set_protocol_type(ratatui_image::picker::ProtocolType::Kitty);
            }
            Some(ImageProtocol::Sixel) => {
                picker.set_protocol_type(ratatui_image::picker::ProtocolType::Sixel);
                log::debug!("ImageProtocol::Sixel");
            }
            None => {
                log::debug!("ImageProtocol::None");
            }
        }

        let stream_client = {
            let mut builder = reqwest::Client::builder();
            if !stream_proxy.is_empty() {
                builder = builder
                    .proxy(reqwest::Proxy::all(stream_proxy).map_err(color_eyre::Report::msg)?);
            }
            builder.build()?
        };

        let cover_http = {
            let mut builder = reqwest::Client::builder();
            if !search_proxy.is_empty() {
                builder = builder
                    .proxy(reqwest::Proxy::all(search_proxy).map_err(color_eyre::Report::msg)?);
            }
            builder.build()?
        };

        let sonar_enabled = config.source_fallback.enabled;
        let sonar_songs: Arc<Mutex<HashMap<u64, Arc<sonar::Song>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let mut state = State {
            running: true,
            events,
            border,
            splash: SplashState::default(),
            navigation: NavigationState {
                page: Page::Splash,
                login: LoginState::default(),
                user: None,
                nav: NavState::from_config(&config.navigation),
                content: Arc::new(ContentState::Empty),
                history: Vec::new(),
                content_selected: 0,
                content_column_selected: 0,
                table_mode: TableMode::Row,
                table_state: TableState::default(),
                playlist_selected: 0,
                search: SearchState::default(),
                pagination: None,
                generation: 0,
                content_is_search: false,
                title_cache: RefCell::new(None),
            },
            command_panel,
            help: HelpState::default(),
            offline: false,
            tick: 0,
            last_tick: Instant::now(),
            toast_msg: String::new(),
            toast_time: None,
            playerbar_area: Rect::default(),
            queue_tab_scroll_x: 0,
        };
        state.navigation.search.providers = search_providers;
        Ok(Self {
            config,
            service: service.clone(),
            playback: PlaybackEngine::new(
                tx,
                service,
                cache,
                base_dir,
                quality,
                stream_client,
                Arc::clone(&finder),
                sonar_enabled,
                Arc::clone(&sonar_songs),
            ),
            state,
            theme_registry,
            picker,
            cover_http,
            finder,
            sonar_songs,
        })
    }

    pub fn current_theme(&self) -> &Theme {
        self.theme_registry
            .get(&self.config.default_theme)
            .unwrap_or_else(|| {
                log::warn!(
                    "Theme '{}' not found, falling back to default",
                    self.config.default_theme
                );
                self.theme_registry.get("default").unwrap_or_else(|| {
                    log::error!("Default theme missing, using hardcoded fallback");
                    theme_fallback()
                })
            })
    }

    pub fn quit(&mut self) {
        self.playback.save_session();
        self.service.client().flush_cookies();
        self.state.running = false;
    }

    pub fn execute_command(&mut self, action: CommandAction) {
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
            CommandAction::SwitchTheme(name) => {
                let msg = format!("THEME: {name}");
                self.config.default_theme = name;
                self.config.save();
                self.toast(msg);
            }
        }
    }

    pub fn toast(&mut self, msg: String) {
        self.state.toast_msg = msg;
        self.state.toast_time = Some(Instant::now());
    }

    /// Breadcrumb key for the current page: the last breadcrumb level's
    /// subtitle, falling back to the focused nav item's name. Distinct pages
    /// get distinct playback queues.
    pub fn current_queue_key(&self) -> String {
        let nav = &self.state.navigation;
        if let Some(sub) = nav.nav.subtitle.clone().filter(|s| !s.trim().is_empty()) {
            return sub;
        }
        nav.nav
            .sections
            .get(nav.nav.focus_section)
            .and_then(|s| {
                nav.nav
                    .section_states
                    .get(nav.nav.focus_section)
                    .and_then(|st| st.selected())
                    .and_then(|i| s.items.get(i))
            })
            .map(|item| item.name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "默认队列".into())
    }
}

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

    fn draw(&mut self, frame: &mut Frame) {
        ui::draw(frame, self);
    }
}
