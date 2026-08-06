pub mod command;
pub mod content;
pub mod help;
pub mod login;
pub mod navigation;
pub mod splash;

pub use command::*;
pub use content::*;
pub use help::*;
pub use login::*;
pub use navigation::*;
pub use splash::*;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use ncm_api::LoginInfo;
use ratatui::layout::Rect;
use ratatui::widgets::TableState;

use crate::{
    config::{BorderConfig, Config, Theme, ThemeRegistry},
    event::EventHandler,
    service::ApiService,
    text_input::TextInput,
};

use crate::playback::PlaybackEngine;

pub use crate::playback::PlaybackState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Splash,
    Main,
    Lyrics,
    Playlist,
    Login,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PaginationInfo {
    pub api: String,
    pub offset: u32,
    pub limit: u32,
    pub has_more: bool,
    pub total: u64,
    pub loading: bool,
}

impl Default for PaginationInfo {
    fn default() -> Self {
        Self {
            api: String::new(),
            offset: 0,
            limit: 50,
            has_more: false,
            total: 0,
            loading: false,
        }
    }
}

type TitleCache = (String, usize, Option<usize>, u64);

#[derive(Clone)]
pub struct BreadcrumbEntry {
    pub content: Arc<ContentState>,
    pub api: Option<String>,
    pub subtitle: Option<String>,
    pub content_selected: usize,
    pub content_column_selected: usize,
    pub table_mode: TableMode,
    pub table_state: TableState,
}

pub struct NavigationState {
    pub page: Page,
    pub login: LoginState,
    pub user: Option<LoginInfo>,
    pub nav: NavState,
    pub content: Arc<ContentState>,
    pub history: Vec<BreadcrumbEntry>,
    pub content_selected: usize,
    pub content_column_selected: usize,
    pub table_mode: TableMode,
    pub table_state: TableState,
    pub playlist_selected: usize,
    pub search: SearchState,
    pub pagination: Option<PaginationInfo>,
    pub generation: u64,
    /// True when the current `Songs` content is a search result (Enter plays
    /// only the selected song instead of appending the whole list to the queue).
    pub content_is_search: bool,
    /// Cached rendered rows to avoid per-frame serde serialization.
    /// Invalidated when `content` is replaced.
    pub content_rows_cache: RefCell<Option<Vec<Vec<String>>>>,
    /// Cached block title string, keyed by (focus_section, selected_index, generation).
    pub title_cache: RefCell<Option<TitleCache>>,
}

impl NavigationState {
    pub fn set_content(&mut self, content: ContentState) {
        self.content = Arc::new(content);
        self.content_selected = 0;
        self.content_column_selected = 0;
        self.table_mode = TableMode::Row;
        self.table_state = TableState::default();
        self.table_state.select_first();
        self.pagination = None;
        *self.content_rows_cache.borrow_mut() = None;
        *self.title_cache.borrow_mut() = None;
    }

    pub fn push_breadcrumb(&mut self) {
        let api = self
            .nav
            .section_states
            .get(self.nav.focus_section)
            .and_then(|st| st.selected())
            .and_then(|i| self.nav.sections[self.nav.focus_section].items.get(i))
            .and_then(|item| item.api.clone());
        self.history.push(BreadcrumbEntry {
            content: Arc::clone(&self.content),
            api,
            subtitle: self.nav.subtitle.clone(),
            content_selected: self.content_selected,
            content_column_selected: self.content_column_selected,
            table_mode: self.table_mode,
            table_state: self.table_state,
        });
    }

    pub fn pop_breadcrumb(&mut self) -> bool {
        if let Some(entry) = self.history.pop() {
            self.content = entry.content;
            self.content_selected = entry.content_selected;
            self.content_column_selected = entry.content_column_selected;
            self.table_mode = entry.table_mode;
            self.table_state = entry.table_state;
            self.nav.subtitle = entry.subtitle;
            if let Some(api) = &entry.api {
                self.nav.restore_focus_by_api(api);
            }
            *self.content_rows_cache.borrow_mut() = None;
            *self.title_cache.borrow_mut() = None;
            true
        } else {
            false
        }
    }

    pub fn clear_breadcrumb(&mut self) {
        self.history.clear();
    }
}

/// A search backend selectable in the search bar. `Ncm` is the default and
/// searches NetEase Cloud Music; the rest delegate to sonar providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchProvider {
    #[default]
    Ncm,
    Kugou,
    Kuwo,
    BiliVideo,
    Youtube,
}

impl SearchProvider {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Ncm => "netease",
            Self::Kugou => "kugou",
            Self::Kuwo => "kuwo",
            Self::BiliVideo => "bilivideo",
            Self::Youtube => "youtube",
        }
    }

    pub fn to_sonar(self) -> Option<sonar::SonarSource> {
        match self {
            Self::Ncm => None,
            Self::Kugou => Some(sonar::SonarSource::Kugou),
            Self::Kuwo => Some(sonar::SonarSource::Kuwo),
            Self::BiliVideo => Some(sonar::SonarSource::BiliVideo),
            Self::Youtube => Some(sonar::SonarSource::Youtube),
        }
    }

    pub fn from_sonar(source: sonar::SonarSource) -> Self {
        match source {
            sonar::SonarSource::Kugou => Self::Kugou,
            sonar::SonarSource::Kuwo => Self::Kuwo,
            sonar::SonarSource::BiliVideo => Self::BiliVideo,
            sonar::SonarSource::Youtube => Self::Youtube,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub active: bool,
    pub input: TextInput,
    pub filter_queue_only: bool,
    pub unfiltered_songs: Option<Vec<Arc<ncm_api::SongInfo>>>,
    /// Available providers in display order (网易云 first, then configured
    /// sonar sources). Populated from config at startup.
    pub providers: Vec<SearchProvider>,
    /// Currently selected search provider.
    pub provider: SearchProvider,
}

impl SearchState {
    pub fn cycle_provider(&mut self, forward: bool) {
        if self.providers.is_empty() {
            return;
        }
        let idx = self
            .providers
            .iter()
            .position(|p| *p == self.provider)
            .unwrap_or(0);
        let len = self.providers.len();
        self.provider = if forward {
            self.providers[(idx + 1) % len]
        } else {
            self.providers[(idx + len - 1) % len]
        };
    }
}

pub struct State {
    pub running: bool,
    pub events: EventHandler,
    pub border: BorderConfig,
    pub splash: SplashState,
    pub navigation: NavigationState,
    pub command_panel: CommandPanel,
    pub help: HelpState,
    pub offline: bool,
    pub tick: u64,
    pub last_tick: std::time::Instant,
    pub toast_msg: String,
    pub toast_time: Option<std::time::Instant>,
    pub playerbar_area: Rect,
    pub queue_tab_scroll_x: u16,
}

pub fn theme_fallback() -> &'static Theme {
    static FALLBACK: OnceLock<Theme> = OnceLock::new();
    FALLBACK.get_or_init(Theme::default)
}

/// Main application state and entry point for the pigma TUI.
pub struct App {
    pub config: Config,
    pub state: State,
    pub playback: PlaybackEngine,
    pub theme_registry: ThemeRegistry,
    pub service: ApiService,
    pub picker: ratatui_image::picker::Picker,
    /// Blocking HTTP client for cover downloads (honours the proxy config).
    pub cover_http: reqwest::Client,
    /// Shared sonar finder used for per-provider search and playback fallback.
    pub finder: Arc<sonar::SonarFinder>,
    /// Original sonar songs for search results, keyed by synthetic song id.
    pub sonar_songs: Arc<Mutex<HashMap<u64, Arc<sonar::Song>>>>,
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
        use crate::config::ProxyTarget;
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
                dirs::cache_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("pigma")
                    .join(&config.cache.cache_dir)
            }
        };
        let base_dir = dirs::cache_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("pigma");

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

        let cache = crate::cache::CacheManager::new(
            cache_dir,
            base_dir,
            config.cache.cache_template.clone(),
        );

        let service = ApiService::new(api.clone(), cache.clone());

        let mut picker = ratatui_image::picker::Picker::from_query_stdio()
            .unwrap_or_else(|_| ratatui_image::picker::Picker::halfblocks());

        match crate::utils::terminal::best_image_protocol() {
            Some(crate::utils::terminal::ImageProtocol::Kitty) => {
                picker.set_protocol_type(ratatui_image::picker::ProtocolType::Kitty);
            }
            Some(crate::utils::terminal::ImageProtocol::Sixel) => {
                picker.set_protocol_type(ratatui_image::picker::ProtocolType::Sixel);
            }
            None => {}
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
                content_rows_cache: RefCell::new(None),
                title_cache: RefCell::new(None),
            },
            command_panel,
            help: HelpState::default(),
            offline: false,
            tick: 0,
            last_tick: std::time::Instant::now(),
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
        self.state.toast_time = Some(std::time::Instant::now());
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
