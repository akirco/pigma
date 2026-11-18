use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, atomic::AtomicU64},
    time::{Duration, Instant},
};

use ratatui::{layout::Rect, widgets::TableState};
use ratatui_image::picker::Picker;
use reqwest::Client;
use sonar::SonarFinder;

use super::{App, search::SearchHost};
use crate::{
    cache::CacheManager,
    config::{Config, ProxyTarget, ThemeRegistry},
    event::EventHandler,
    ipc::IpcState,
    persist::{expand_tilde, pigma_cache_dir, pigma_config_dir},
    playback::PlaybackEngine,
    service::{ApiService, ContentCache, SearchEngine, SearchRegistry, SearchResults, SonarSongs},
    state::{
        CommandAction, CommandItem, CommandPanel, ContentState, HelpState, LoginState, NavState,
        NavigationState, Page, SearchProvider, SearchState, SplashState, State, TableMode,
    },
    ui::terminal::{ImageProtocol, best_image_protocol},
};

/// Used to decide whether a proxy is enabled based on `ProxyTarget`.
pub(super) enum ProxyKind {
    /// Non-YouTube services (NetEase Cloud, sonar search, covers, streaming), proxied under `Reversed`/`Both`.
    NonYoutube,
    /// YouTube services, proxied under `Normal`/`Both`.
    Youtube,
}

/// Bound connection setup and each read. Deliberately no total request timeout:
/// audio streams legitimately download for the length of the track.
const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const HTTP_READ_TIMEOUT: Duration = Duration::from_secs(30);

impl App {
    /// `NonYoutube` is proxied under `Reversed`/`Both`; `Youtube` under `Normal`/`Both`.
    pub(super) fn proxy_for(config: &Config, kind: ProxyKind) -> &str {
        let proxy = config.proxy.as_str();
        if proxy.is_empty() {
            return "";
        }
        let active = match kind {
            ProxyKind::NonYoutube => {
                matches!(
                    config.proxy_target,
                    ProxyTarget::Reversed | ProxyTarget::Both
                )
            }
            ProxyKind::Youtube => {
                matches!(config.proxy_target, ProxyTarget::Normal | ProxyTarget::Both)
            }
        };
        if active { proxy } else { "" }
    }

    /// Build the command panel (theme-switching submenu + border/save-on-play toggles).
    pub(super) fn build_command_panel(theme_registry: &ThemeRegistry) -> CommandPanel {
        let theme_children: Vec<CommandItem> = theme_registry
            .all_names()
            .into_iter()
            .map(|name| {
                let name = name.to_string();
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
                name: "Show / Hide Border".into(),
                action: CommandAction::ToggleBordered,
            },
            CommandItem::Action {
                name: "Toggle Save on Play".into(),
                action: CommandAction::ToggleSaveOnPlay,
            },
            CommandItem::Action {
                name: "Cycle Nav Position".into(),
                action: CommandAction::CycleNavPosition,
            },
        ];

        let mut command_panel = CommandPanel::new();
        command_panel.levels = vec![commands];
        command_panel
    }

    /// Build the sonar finder per config, applying the search/YouTube proxy.
    pub(super) fn build_finder(
        config: &Config,
        search_proxy: &str,
        youtube_proxy: &str,
    ) -> eyre::Result<Arc<SonarFinder>> {
        let mut sources: Vec<sonar::SonarSource> = Vec::new();
        for source in &config.source_fallback.providers {
            if !sources.contains(source) {
                sources.push(*source);
            }
        }
        let search_config = sonar::SearchConfig::new()
            .with_providers(sources)
            .with_timeout(config.source_fallback.timeout_ms)
            .with_search_proxy(search_proxy.to_string())
            .with_youtube_proxy(youtube_proxy.to_string());
        let finder = sonar::SonarFinder::new(search_config).map_err(eyre::Report::msg)?;
        Ok(Arc::new(finder))
    }

    /// Build the image picker for the current terminal and select the best image protocol (Kitty/Sixel).
    pub(super) fn build_picker() -> Picker {
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
        picker
    }

    /// Build a blocking HTTP client for the given proxy address; an empty address connects directly.
    pub(super) fn build_http_client(proxy: &str) -> eyre::Result<Client> {
        let mut builder = Client::builder()
            .connect_timeout(HTTP_CONNECT_TIMEOUT)
            .read_timeout(HTTP_READ_TIMEOUT);
        if !proxy.is_empty() {
            builder = builder.proxy(reqwest::Proxy::all(proxy).map_err(eyre::Report::msg)?);
        }
        builder.build().map_err(eyre::Report::msg)
    }
}

impl App {
    pub fn new(mut config: Config, with_terminal: bool) -> eyre::Result<Self> {
        let border = config.border.clone();

        let events = EventHandler::new(with_terminal);
        let tx = events.sender();

        let theme_registry = ThemeRegistry::new(config.themes.clone());
        let command_panel = Self::build_command_panel(&theme_registry);

        // `normal` (domestic default): only YouTube goes through the proxy;
        // `reversed` (overseas): everything except YouTube; `both`: everything.
        let ncm_proxy = Self::proxy_for(&config, ProxyKind::NonYoutube);
        let search_proxy = Self::proxy_for(&config, ProxyKind::NonYoutube);
        let youtube_proxy = Self::proxy_for(&config, ProxyKind::Youtube);
        let stream_proxy = search_proxy;

        let cookie_path = pigma_config_dir().join("cookies.json");
        let mut api_builder = ncm_api::NcmClient::builder().cookie_path(cookie_path);
        if !ncm_proxy.is_empty() {
            api_builder = api_builder.proxy(ncm_proxy);
        }
        let api = Arc::new(api_builder.build()?);

        let quality = config.cache.quality;
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
        let content_cache = Arc::new(ContentCache::new(base_dir.clone()));

        let service = ApiService::new(api, cache.clone(), content_cache);

        let picker = Self::build_picker();

        let stream_client = Self::build_http_client(stream_proxy)?;
        let cover_http = Self::build_http_client(search_proxy)?;

        let sonar_enabled = config.source_fallback.enabled;
        let sonar_songs: SonarSongs = Arc::new(Mutex::new(SearchRegistry::default()));
        let liked_ids: Arc<Mutex<HashSet<u64>>> = Arc::new(Mutex::new(HashSet::new()));
        let mut state = State {
            running: true,
            border,
            splash: SplashState::default(),
            eq_selected: 0,
            login: LoginState::default(),
            navigation: NavigationState {
                page: Page::Splash,
                user: None,
                nav: NavState::from_config(&config.navigation),
                content: Arc::new(ContentState::Empty),
                history: Vec::new(),
                content_selected: 0,
                content_column_selected: 0,
                content_columns: 1,
                content_scroll: 0,
                content_visible: 0..0,
                content_hovered: None,
                table_mode: TableMode::Row,
                table_state: TableState::default(),
                playlist_selected: 0,
                queue_tab_scroll_x: 0,
                queue_body: Rect::default(),
                queue_offset: 0,
                queue_tab_area: Rect::default(),
                queue_tab_hits: Vec::new(),
                search: SearchState::default(),
                pagination: None,
                generation: 0,
                content_is_search: false,
                title_cache: RefCell::new(None),
            },
            command_panel,
            help: HelpState::default(),
            offline: false,
            mouse: None,
            last_click: None,
            tick: 0,
            last_tick: Instant::now(),
            toast_msg: String::new(),
            toast_time: None,
        };
        state.navigation.search.providers = search_providers;
        let search_results: SearchResults = Arc::new(Mutex::new(SearchRegistry::default()));
        let searcher = Arc::new(SearchEngine::new(
            service.clone(),
            Arc::clone(&finder),
            Arc::clone(&sonar_songs),
            Arc::clone(&search_results),
            config.search_limit as usize,
            state.navigation.search.providers.clone(),
        ));
        let mut playback = PlaybackEngine::new(
            tx,
            service.clone(),
            cache,
            base_dir,
            quality,
            save_on_play,
            stream_client,
            Arc::clone(&finder),
            sonar_enabled,
            Arc::clone(&sonar_songs),
            Arc::clone(&liked_ids),
        );
        playback.set_equalizer(config.equalizer.sanitized());
        config.equalizer = playback.equalizer();
        Ok(Self {
            config,
            events,
            service,
            playback,
            state,
            theme_registry,
            picker,
            cover_http,
            content_covers: crate::ui::cover::ContentCovers::default(),
            cover: crate::ui::cover::CoverState::default(),
            cover_requests: Arc::new(Mutex::new(HashSet::new())),
            cover_retry_after: Arc::new(Mutex::new(HashMap::new())),
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
            playerbar_hits: Default::default(),
            page_transition: None,
            animate_pages: with_terminal,
            draw_stats: Default::default(),
            login_status_requested: false,
            awaiting_login_status: false,
            qr_generation: Arc::new(AtomicU64::new(0)),
        })
    }
}
