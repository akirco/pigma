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

use ncm_api::SongList;
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
use crate::ipc::{IpcEvent, QueueSnapshot, StatusSnapshot};
use crate::playback::{PlaybackEngine, scan_local_music};
use crate::service::{ApiEndpoint, ApiService};
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
    /// Live playback snapshot served to `pigma status` over the IPC socket.
    pub status: Arc<Mutex<StatusSnapshot>>,
    /// Live playback queue served to `pigma status -L` over the IPC socket.
    pub queue: Arc<Mutex<QueueSnapshot>>,
}

impl App {
    /// `with_terminal` selects the interactive TUI event source (crossterm);
    /// pass `false` for headless daemon mode.
    pub fn new(config: Config, with_terminal: bool) -> color_eyre::Result<Self> {
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
            status: Arc::new(Mutex::new(StatusSnapshot::default())),
            queue: Arc::new(Mutex::new(QueueSnapshot::default())),
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

    /// Refresh the IPC status snapshot from the live playback state.
    fn update_status_snapshot(&self) {
        if let Ok(mut snapshot) = self.status.lock() {
            *snapshot = StatusSnapshot::from_playback(&self.playback.state);
        }
        if let Ok(mut queue) = self.queue.lock() {
            *queue = QueueSnapshot {
                current_index: self.playback.queue_current_index(),
                songs: self
                    .playback
                    .queue_songs()
                    .iter()
                    .map(|s| crate::ipc::QueueEntry::from_song(s))
                    .collect(),
            };
        }
    }

    /// Apply a control request received over the IPC socket (`pigma msg`).
    async fn handle_ipc_event(&mut self, event: IpcEvent) {
        match event {
            IpcEvent::Previous => self.playback.prev(),
            IpcEvent::Next => self.playback.next(),
            IpcEvent::Pause => {
                // Pause only pauses; when stopped it stays stopped (unlike the
                // TUI spacebar which toggles/start).
                if self.playback.state.playing && !self.playback.state.paused {
                    self.playback.toggle_pause();
                }
            }
            IpcEvent::Play => {
                // Resume when paused; start when stopped (if a song is queued).
                if self.playback.state.paused || !self.playback.state.playing {
                    self.playback.toggle_pause();
                }
            }
            IpcEvent::Volume { delta, absolute } => {
                if let Some(delta) = delta {
                    self.adjust_volume(delta);
                } else if let Some(volume) = absolute {
                    let volume = volume.clamp(0.0, 1.0);
                    self.playback.set_volume(volume);
                    self.toast(format!("   {:.0}%", volume * 100.0));
                }
            }
            IpcEvent::Mode => {
                let mode = self.playback.cycle_mode();
                let (_, label) = crate::playback::mode_icon(&mode);
                self.toast(format!("🔄 播放模式: {label}"));
            }
            IpcEvent::Like => {
                if let Some(song) = self.playback.current_song() {
                    self.state
                        .events
                        .send(crate::event::PlaybackEvent::LikeSong(song.id, true));
                }
            }
            IpcEvent::Dislike => {
                if let Some(song) = self.playback.current_song() {
                    self.state
                        .events
                        .send(crate::event::PlaybackEvent::DislikeSong(song.id));
                }
            }
            IpcEvent::ToggleLike => {
                if let Some(song) = self.playback.current_song() {
                    let like = !self.playback.state.liked;
                    self.state
                        .events
                        .send(crate::event::PlaybackEvent::LikeSong(song.id, like));
                }
            }
            IpcEvent::SwitchList { endpoint, playlist } => {
                let loaded = self.load_endpoint(&endpoint, playlist).await;
                self.toast(if loaded {
                    format!("🔀 已切换到: {endpoint}")
                } else {
                    format!("🔀 切换失败: {endpoint}")
                });
            }
        }
    }

    pub async fn run(mut self, mut terminal: DefaultTerminal) -> color_eyre::Result<()> {
        self.start_splash_boot();
        let _ipc_guard = crate::ipc::start_server(
            Arc::clone(&self.status),
            Arc::clone(&self.queue),
            self.state.events.sender(),
        );
        while self.state.running {
            self.update_status_snapshot();
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

    /// Headless daemon mode (`pigma --daemon <endpoint>`): no terminal is opened.
    /// Loads the endpoint as the initial list, starts playing it, and runs the
    /// IPC socket so `pigma status` / `pigma msg` can observe and control it.
    /// Stops on SIGINT/SIGTERM (saving the session).
    pub async fn run_headless(
        mut self,
        endpoint: &str,
        playlist_index: Option<usize>,
    ) -> color_eyre::Result<()> {
        self.state.navigation.page = Page::Main;
        let _ipc_guard = crate::ipc::start_server(
            Arc::clone(&self.status),
            Arc::clone(&self.queue),
            self.state.events.sender(),
        );

        // Resolve the user session from cookies so login-gated endpoints like
        // `__liked__` can obtain the uid even without an interactive QR login.
        if self.state.navigation.user.is_none() {
            match self.service.login_status().await {
                Ok(info) => {
                    let uid = info.uid;
                    self.state.navigation.user = Some(info);
                    // Preload the liked-song id set so player-bar like status and
                    // the waybar heart icon reflect reality in headless mode too.
                    match self.service.load_liked_song_ids(uid).await {
                        Ok(ids) => {
                            if let Ok(mut guard) = self.liked_ids.lock() {
                                *guard = ids;
                            }
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
            self.update_status_snapshot();
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

    /// Resolve an endpoint string into playable songs and load them into the
    /// queue without starting playback. Shared by the daemon bootstrap
    /// (`--daemon`) and the IPC `pigma msg switch-list` action. Returns whether
    /// songs were loaded.
    async fn load_endpoint(&mut self, api_str: &str, playlist_index: Option<usize>) -> bool {
        let api = ApiEndpoint::parse(api_str).unwrap_or(ApiEndpoint::RecommendSongs);
        let uid = self.state.navigation.user.as_ref().map(|u| u.uid);
        let content = if api == ApiEndpoint::LikedSongs
            && let Some(uid) = uid
        {
            let (state, _, _) = self
                .service
                .load_liked_songs(uid, self.config.search_limit)
                .await;
            state
        } else if api == ApiEndpoint::Download {
            let songs = self.service.cache().list_cached_songs_async().await;
            ContentState::Songs(songs.into_iter().map(std::sync::Arc::new).collect())
        } else if api == ApiEndpoint::LocalMusic {
            let music_dir = dirs::home_dir().unwrap_or_default().join("Music");
            let songs = tokio::task::spawn_blocking(move || scan_local_music(&music_dir))
                .await
                .unwrap_or_default();
            ContentState::Songs(songs.into_iter().map(std::sync::Arc::new).collect())
        } else {
            let (state, _) = self
                .service
                .resolve_content(api, uid, self.config.search_limit)
                .await;
            state
        };

        // Playlist/toplist endpoints resolve to a *list* of playlists; pick one
        // (default first, or `--playlist N`) and load its songs.
        let content = match content {
            ContentState::SongLists(lists) => {
                self.load_headless_list_songs(api, lists, playlist_index)
                    .await
            }
            ContentState::TopLists(lists) => {
                let lists: Vec<SongList> = lists
                    .into_iter()
                    .map(|t| SongList {
                        id: t.id,
                        name: t.name,
                        cover_img_url: t.cover,
                        author: String::new(),
                        subscribed: false,
                    })
                    .collect();
                self.load_headless_list_songs(api, lists, playlist_index)
                    .await
            }
            other => other,
        };

        match content {
            ContentState::Songs(songs) if !songs.is_empty() => {
                self.playback.load_songs(api_str, songs, 0);
                true
            }
            ContentState::Error(e) => {
                log::error!("headless: {api_str}: {e}");
                false
            }
            _ => {
                log::warn!("headless: {api_str} resolved to no playable songs");
                false
            }
        }
    }

    /// Resolve a playlist-list (or toplist) to the songs of the selected
    /// playlist: `--playlist N` picks the 1-based `N`-th entry (default 1st).
    async fn load_headless_list_songs(
        &mut self,
        api: ApiEndpoint,
        lists: Vec<SongList>,
        playlist_index: Option<usize>,
    ) -> ContentState {
        if lists.is_empty() {
            return ContentState::Empty;
        }
        let index = playlist_index.unwrap_or(1).max(1) - 1;
        let Some(list) = lists.get(index) else {
            return ContentState::Error(format!(
                "playlist index {} out of range (1..={})",
                index + 1,
                lists.len()
            ));
        };
        self.playback.set_playlist_id(list.id);
        let limit = self.config.search_limit;
        match api {
            ApiEndpoint::SavedAlbums => self.service.load_album(list.id).await,
            ApiEndpoint::UserRadioSublist => {
                let (state, _, _) = self
                    .service
                    .load_playlist_detail(list.id, true, limit)
                    .await;
                state
            }
            _ => {
                let (state, _, _) = self
                    .service
                    .load_playlist_detail(list.id, false, limit)
                    .await;
                state
            }
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
