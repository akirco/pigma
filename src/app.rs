//! Main application state (`App`) and the wiring of views, events, navigation,
//! search, login and theming for the pigma TUI.

mod builder;
mod content;
mod event;
mod login;
mod navigation;
mod search;
mod splash;
mod theme;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use ratatui::layout::Rect;
use ratatui::widgets::TableState;
use ratatui::{DefaultTerminal, Frame};
use ratatui_image::picker::Picker;
use reqwest::Client;
use sonar::{SonarFinder, Song};

use crate::cache::CacheManager;
use crate::config::{Config, ThemeRegistry};
use crate::event::AuthEvent;
use crate::event::EventHandler;
use crate::playback::PlaybackEngine;
use crate::service::ApiService;
use crate::state::{
    ContentState, HelpState, LoginState, NavState, NavigationState, Page, SearchProvider,
    SearchState, SplashState, State, TableMode,
};
use crate::ui;
use crate::utils::{pigma_cache_dir, pigma_config_dir};

use splash::send_event;

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
    /// Song ID set of the user's "我喜欢的音乐" playlist, sharing the same `Arc` as `PlaybackEngine`.
    pub liked_ids: Arc<Mutex<HashSet<u64>>>,
    /// Playlists whose full tracks have already been merged into the playback queue for lazy pagination, avoiding repeated Enter presses refetching/truncating the queue.
    queued_playlists: HashSet<u64>,
}

impl App {
    pub fn new(config: Config) -> color_eyre::Result<Self> {
        let border = config.border.clone();

        let events = EventHandler::new();
        let tx = events.sender();

        let theme_registry = ThemeRegistry::new(config.themes.clone());
        let command_panel = Self::build_command_panel(&theme_registry);

        // `normal` (domestic default): only YouTube goes through the proxy;
        // `reversed` (overseas): everything except YouTube; `both`: everything.
        let ncm_proxy = Self::proxy_for(&config, builder::ProxyKind::NonYoutube);
        let search_proxy = Self::proxy_for(&config, builder::ProxyKind::NonYoutube);
        let youtube_proxy = Self::proxy_for(&config, builder::ProxyKind::Youtube);
        let stream_proxy = search_proxy;

        let cookie_path = pigma_config_dir().join("cookies.json");
        let mut api_builder = ncm_api::NcmClient::builder().cookie_path(cookie_path);
        if !ncm_proxy.is_empty() {
            api_builder = api_builder.proxy(ncm_proxy);
        }
        let api = Arc::new(api_builder.build()?);

        let quality = ncm_api::SongQuality::from_level(&config.cache.quality)
            .unwrap_or(ncm_api::SongQuality::Higher);
        let save_on_play = config.cache.save_on_play;

        let cache_dir = {
            let path = std::path::Path::new(&config.cache.cache_dir);
            if path.is_absolute() {
                std::path::PathBuf::from(&config.cache.cache_dir)
            } else {
                pigma_cache_dir().join(&config.cache.cache_dir)
            }
        };
        let base_dir = pigma_cache_dir();

        let finder = Self::build_finder(&config, search_proxy, youtube_proxy)?;

        // Search providers offered in the search bar: NetEase Cloud always first,
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

        let cache = Arc::new(CacheManager::new(
            cache_dir,
            base_dir.clone(),
            config.cache.cache_template.clone(),
        ));

        let service = ApiService::new(api.clone(), cache.clone());

        let picker = Self::build_picker();

        let stream_client = Self::build_http_client(stream_proxy)?;
        let cover_http = Self::build_http_client(search_proxy)?;

        let sonar_enabled = config.source_fallback.enabled;
        let sonar_songs: Arc<Mutex<HashMap<u64, Arc<sonar::Song>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let liked_ids: Arc<Mutex<HashSet<u64>>> = Arc::new(Mutex::new(HashSet::new()));
        let mut state = State {
            running: true,
            events,
            border,
            splash: SplashState::default(),
            login: LoginState::default(),
            navigation: NavigationState {
                page: Page::Splash,
                user: None,
                nav: NavState::from_config(&config.navigation),
                content: Arc::new(ContentState::Empty),
                history: Vec::new(),
                content_selected: 0,
                content_column_selected: 0,
                table_mode: TableMode::Row,
                table_state: TableState::default(),
                playlist_selected: 0,
                queue_tab_scroll_x: 0,
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
                save_on_play,
                stream_client,
                Arc::clone(&finder),
                sonar_enabled,
                Arc::clone(&sonar_songs),
                Arc::clone(&liked_ids),
            ),
            state,
            theme_registry,
            picker,
            cover_http,
            finder,
            sonar_songs,
            liked_ids,
            queued_playlists: HashSet::new(),
        })
    }

    pub fn quit(&mut self) {
        self.playback.save_session();
        self.service.client().flush_cookies();
        self.state.running = false;
    }

    pub fn toast(&mut self, msg: String) {
        self.state.toast_msg = msg;
        self.state.toast_time = Some(Instant::now());
    }

    /// Adjust playback volume by `delta` (fraction of 0..=1), clamped to bounds
    /// and surfaced as a toast. Keyboard `+`/`-` mirrors the playerbar scroll.
    pub fn adjust_volume(&mut self, delta: f64) {
        let new = (self.playback.state.volume + delta).clamp(0.0, 1.0);
        self.playback.set_volume(new);
        self.toast(format!("   {:.0}%", new * 100.0));
    }

    /// Cycle the navigation bar position (left → right → top → bottom) at
    /// runtime and persist the new value so it survives restarts.
    pub fn cycle_nav_position(&mut self) {
        self.config.navigation_position = self.config.navigation_position.cycle();
        self.config.save();
        let pos = self.config.navigation_position;
        self.toast(format!("◧ 导航栏位置: {}", pos.label()));
    }

    pub async fn run(mut self, mut terminal: DefaultTerminal) -> color_eyre::Result<()> {
        self.start_splash_boot();
        while self.state.running {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events().await?;

            let splash_ready = self.state.splash.shown_at.elapsed().as_secs_f64()
                >= self.config.splash_duration_secs;
            if self.state.splash.boot_complete
                && splash_ready
                && self.state.navigation.page == Page::Splash
            {
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
                    self.navigate_to_main();
                }
            }
        }
        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        ui::draw(frame, self);
    }
}
