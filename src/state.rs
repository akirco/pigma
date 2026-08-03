pub mod command;
pub mod content;
pub mod login;
pub mod navigation;
pub mod splash;

pub use command::*;
pub use content::*;
pub use login::*;
pub use navigation::*;
pub use splash::*;

use std::cell::RefCell;
use std::sync::{Arc, OnceLock};

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

pub use crate::playback::{EnginePlayMode, EngineState as PlaybackState};

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

#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub active: bool,
    pub input: TextInput,
    pub filter_queue_only: bool,
    pub unfiltered_songs: Option<Vec<Arc<ncm_api::SongInfo>>>,
}

pub struct State {
    pub running: bool,
    pub events: EventHandler,
    pub border: BorderConfig,
    pub splash: SplashState,
    pub navigation: NavigationState,
    pub command_panel: CommandPanel,
    pub offline: bool,
    pub tick: u64,
    pub last_tick: std::time::Instant,
    pub toast_msg: String,
    pub toast_time: Option<std::time::Instant>,
    pub playerbar_area: Rect,
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
            .iter()
            .map(|name| CommandItem::Action {
                name: name.clone(),
                action: CommandAction::SwitchTheme(name.clone()),
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

        let (ncm_proxy, yt_proxy) = if config.proxy.is_empty() {
            (String::new(), String::new())
        } else {
            match config.proxy_target {
                crate::config::ProxyTarget::Ncm => (config.proxy.clone(), String::new()),
                crate::config::ProxyTarget::Yt => (String::new(), config.proxy.clone()),
                crate::config::ProxyTarget::Both => (config.proxy.clone(), config.proxy.clone()),
            }
        };

        let api = if ncm_proxy.is_empty() {
            Arc::new(ncm_api::NcmClient::new()?)
        } else {
            Arc::new(ncm_api::NcmClient::builder().proxy(&ncm_proxy).build()?)
        };

        let nav_config = config.navigation.clone();
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
        let proxy = yt_proxy;

        let finder = {
            let mut sources: Vec<musicx::MusicSource> = Vec::new();
            for name in &config.source_fallback.providers {
                let source = match name.as_str() {
                    "kuwo" => musicx::MusicSource::Kuwo,
                    "kugou" => musicx::MusicSource::Kugou,
                    "bilivideo" => musicx::MusicSource::BiliVideo,
                    "youtube" => musicx::MusicSource::Youtube,
                    _ => continue,
                };
                if !sources.contains(&source) {
                    sources.push(source);
                }
            }
            let search_config = musicx::SearchConfig::new()
                .with_providers(sources)
                .with_timeout(config.source_fallback.timeout_ms)
                .with_proxy(proxy.clone());
            musicx::MusicFinder::new(search_config)
        };

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

        let musicx_enabled = config.source_fallback.enabled;
        Ok(Self {
            config,
            service: service.clone(),
            playback: PlaybackEngine::new(
                tx,
                service,
                cache,
                quality,
                proxy,
                finder,
                musicx_enabled,
            ),
            state: State {
                running: true,
                events,
                border,
                splash: SplashState::default(),
                navigation: NavigationState {
                    page: Page::Splash,
                    login: LoginState::default(),
                    user: None,
                    nav: NavState::from_config(&nav_config),
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
                    content_rows_cache: RefCell::new(None),
                    title_cache: RefCell::new(None),
                },
                command_panel,
                offline: false,
                tick: 0,
                last_tick: std::time::Instant::now(),
                toast_msg: String::new(),
                toast_time: None,
                playerbar_area: Rect::default(),
            },
            theme_registry,
            picker,
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
                self.config.default_theme = name.clone();
                self.config.save();
                self.toast(format!("THEME: {}", name));
            }
        }
    }

    pub fn toast(&mut self, msg: String) {
        self.state.toast_msg = msg;
        self.state.toast_time = Some(std::time::Instant::now());
    }
}
