//! Main application state (`App`) and the wiring of views, events, navigation,
//! search, login and theming for the pigma TUI.

mod builder;
mod content;
mod cover;
mod event;
mod headless;
mod login;
mod lyrics;
mod navigation;
mod search;
mod selection;
mod splash;
mod table;
mod theme;

use search::SearchHost;

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Instant,
};

use ratatui::{DefaultTerminal, Frame, layout::Rect, widgets::TableState};
use ratatui_image::picker::Picker;
use reqwest::Client;

use crate::{
    cache::CacheManager,
    config::{Config, ThemeRegistry},
    event::{AuthEvent, EventHandler},
    ipc::IpcState,
    playback::PlaybackEngine,
    service::{ApiService, SearchEngine, SearchResults},
    state::{
        ContentState, HelpState, LoginState, NavState, NavigationState, Page, SearchProvider,
        SearchState, SplashState, State, TableMode,
    },
    ui,
    utils::{path::expand_tilde, pigma_cache_dir, pigma_config_dir},
};

/// Main application state and entry point for the pigma TUI.
///
/// Field visibility contract: only `config`, `state`, `playback` and
/// `theme_registry` are `pub(crate)` (the input/UI layers read them heavily).
/// Everything else is private to the `app` module — its submodules access the
/// fields directly, while code outside `app` (e.g. `input`, `ui`) goes through
/// the accessor methods (`is_logged_in`, `picker_protocol`, `playerbar_area`).
pub struct App {
    pub(crate) config: Config,
    pub(crate) state: State,
    pub(crate) playback: PlaybackEngine,
    pub(crate) theme_registry: ThemeRegistry,
    service: ApiService,
    picker: Picker,
    /// Async HTTP client for cover downloads (honours the proxy config).
    cover_http: Client,
    /// Search subsystem: sonar finder/registry, IPC result registry and engine.
    search: SearchHost,
    /// Song ID set of the user's "我喜欢的音乐" playlist, sharing the same `Arc` as `PlaybackEngine`.
    liked_ids: Arc<Mutex<HashSet<u64>>>,
    /// Playlists whose full tracks have already been merged into the playback queue for lazy pagination, avoiding repeated Enter presses refetching/truncating the queue.
    queued_playlists: HashSet<u64>,
    /// IPC snapshot subsystem: status/queue snapshots, broadcast fan-out and
    /// their freshness bookkeeping (see `ipc::state`).
    ipc: IpcState,
    /// Playerbar hit-test rect of the last draw, used by mouse input handling.
    playerbar_area: Rect,
    /// Guards the startup splash branch: `login_status` is requested exactly
    /// once, so later events (IPC, mouse...) cannot spawn duplicate requests
    /// that would double-toast and re-fetch liked IDs.
    login_status_requested: bool,
}

impl App {
    /// `with_terminal` selects the interactive TUI event source (crossterm);
    /// pass `false` for headless daemon mode.
    pub fn new(config: Config, with_terminal: bool) -> eyre::Result<Self> {
        let border = config.border.clone();

        let events = EventHandler::new(with_terminal);
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
            let expanded = expand_tilde(&config.cache.cache_dir);
            if expanded.is_absolute() {
                expanded
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
        };
        state.navigation.search.providers = search_providers;
        let search_results: SearchResults = Arc::new(Mutex::new(HashMap::new()));
        let searcher = Arc::new(SearchEngine::new(
            service.clone(),
            Arc::clone(&finder),
            Arc::clone(&sonar_songs),
            Arc::clone(&search_results),
            config.search_limit as usize,
            state.navigation.search.providers.clone(),
        ));
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
            search: SearchHost {
                finder,
                sonar_songs,
                results: search_results,
                engine: searcher,
            },
            liked_ids,
            queued_playlists: HashSet::new(),
            ipc: IpcState::new(),
            playerbar_area: Rect::default(),
            login_status_requested: false,
        })
    }

    /* -------------------------------------------------------------------------- */
    /*                      shared helpers (TUI + headless)                        */
    /* -------------------------------------------------------------------------- */

    /// Whether the NetEase client holds a valid login (session-cookie state;
    /// gates the input layer's login-page shortcut).
    pub fn is_logged_in(&self) -> bool {
        self.service.client().is_logged_in()
    }

    /// Image protocol of the cover-rendering picker, so the UI layer can
    /// degrade gracefully on terminals without Sixel.
    pub fn picker_protocol(&self) -> ratatui_image::picker::ProtocolType {
        self.picker.protocol_type()
    }

    /// Playerbar hit-test rect recorded by the last draw (`ui::draw`).
    pub fn playerbar_area(&self) -> Rect {
        self.playerbar_area
    }

    /// Record the playerbar hit-test rect for the frame just drawn.
    pub(crate) fn set_playerbar_area(&mut self, area: Rect) {
        self.playerbar_area = area;
    }

    pub fn quit(&mut self) {
        self.playback.save_session();
        self.service.client().flush_cookies();
        // Persist any coalesced content-cache write that has not drained yet;
        // the detached blocking writer may not run before the runtime exits.
        self.service.cache().flush_content_cache();
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

    /// Adjust the playback speed by `delta` (steps of 0.1), clamped to the
    /// supported range and surfaced as a toast. Keyboard `[`/`]`.
    pub fn adjust_speed(&mut self, delta: f64) {
        self.playback.adjust_speed(delta);
        self.speed_toast();
    }

    /// Report the current speed the way the player bar shows it: `*1.2x` when
    /// the pitch is kept, `1.2x` when the speed change shifts it.
    pub fn speed_toast(&mut self) {
        let badge = self.playback.state.speed_badge();
        self.toast(format!("⚡ 速度 {badge}"));
    }

    /// Report the current speed/pitch mode: keeping the pitch (WSOLA stretch)
    /// vs. letting the speed shift it (resampling). A bare `*1.0x` badge does
    /// not tell the two apart at normal speed, so spell the mode out.
    pub fn pitch_toast(&mut self) {
        let mode = if self.playback.state.pitch_locked {
            "⚡ 变速不变调（保持音高）"
        } else {
            "⚡ 变速也变调（音高跟随速度）"
        };
        self.toast(mode.into());
    }

    /// Flip between pitch-keeping (WSOLA stretch) and pitch-shifting
    /// (resampling) speed. Keyboard `t`.
    pub fn toggle_pitch_locked(&mut self) {
        self.playback.toggle_pitch_locked();
        self.pitch_toast();
    }

    /* -------------------------------------------------------------------------- */
    /*                                  TUI mode                                   */
    /* -------------------------------------------------------------------------- */

    /// Cycle the navigation bar position (left → right → top → bottom) at
    /// runtime and persist the new value so it survives restarts. Keyboard
    /// `p`/`shift+p` binding.
    pub fn cycle_nav_position(&mut self) {
        self.config.navigation_position = self.config.navigation_position.cycle();
        self.config.save();
        let pos = self.config.navigation_position;
        self.toast(format!("◧ 导航栏位置: {}", pos.label()));
    }

    /// Interactive terminal entry point (`pigma` without subcommands). Draws
    /// the UI, serves the IPC socket, and pumps the event loop until quit.
    pub async fn run(mut self, mut terminal: DefaultTerminal) -> eyre::Result<()> {
        self.start_splash_boot();
        let _ipc_guard = crate::ipc::start_server(
            Arc::clone(&self.ipc.status),
            Arc::clone(&self.ipc.queue),
            self.ipc.status_tx.clone(),
            self.state.events.sender(),
            Arc::clone(&self.search.engine),
        );
        while self.state.running {
            self.ipc.update(&self.playback);
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
                } else if self.service.client().is_logged_in() && !self.login_status_requested {
                    // Wait for the authenticated user info before entering the
                    // default tab. Login-gated endpoints need the UID returned by
                    // this request, so loading the tab first can produce a false
                    // "未登录" error during startup.
                    self.login_status_requested = true;
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
                                if sender.send(AuthEvent::Error(e.to_string()).into()).is_err() {
                                    log::error!("Failed to send LoginError: receiver dropped");
                                }
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
