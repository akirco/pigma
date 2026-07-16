use std::cell::RefCell;
use std::sync::{Arc, OnceLock};

use ratatui::widgets::ListState;

pub use crate::config::{NavItemConfig as NavItem, NavSectionConfig as NavSection};
use ncm_api::SongInfo;

use crate::{
    config::{Config, NavConfig},
    event::EventHandler,
    theme::{Theme, ThemeRegistry},
    types::ContentState,
    ui::text_input::TextInput,
};
use ncm_api::{LoginInfo, NcmClient};

use crate::playback::PlaybackEngine;

pub use crate::playback::{
    EnginePlayMode, EngineState as PlaybackState, PlaybackLyricLine, parse_lyric_lines,
};

fn theme_fallback() -> &'static Theme {
    static FALLBACK: OnceLock<Theme> = OnceLock::new();
    FALLBACK.get_or_init(Theme::default)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Splash,
    Main,
    Lyrics,
    Playlist,
    Login,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Success,
    Info,
    Warning,
}

#[derive(Debug, Clone)]
pub struct SplashLogEntry {
    pub time: String,
    pub text: String,
    pub level: LogLevel,
}

pub struct SplashState {
    pub progress: f64,
    pub status: String,
    pub logs: Vec<SplashLogEntry>,
    pub boot_complete: bool,
}

impl Default for SplashState {
    fn default() -> Self {
        Self {
            progress: 0.0,
            status: "INITIALIZING SYSTEM...".to_string(),
            logs: Vec::new(),
            boot_complete: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandAction {
    ToggleBordered,
    SwitchTheme(String),
}

#[derive(Debug, Clone)]
pub enum CommandItem {
    Action {
        name: String,
        action: CommandAction,
    },
    SubMenu {
        name: String,
        children: Vec<CommandItem>,
    },
}

pub struct CommandPanel {
    pub open: bool,
    pub selected: usize,
    pub levels: Vec<Vec<CommandItem>>,
}

impl Default for CommandPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandPanel {
    pub fn new() -> Self {
        Self {
            open: false,
            selected: 0,
            levels: Vec::new(),
        }
    }

    pub fn current_items(&self) -> Option<&Vec<CommandItem>> {
        self.levels.last()
    }

    pub fn current_title(&self) -> &str {
        if self.levels.len() > 1 {
            "THEMES"
        } else {
            "COMMANDS"
        }
    }

    pub fn enter(&mut self) -> Option<CommandAction> {
        let item = self.current_items()?[self.selected].clone();
        match item {
            CommandItem::Action { action, .. } => Some(action),
            CommandItem::SubMenu { children, .. } => {
                self.selected = 0;
                self.levels.push(children);
                None
            }
        }
    }

    pub fn back(&mut self) {
        if self.levels.len() > 1 {
            self.levels.pop();
            self.selected = 0;
        } else {
            self.open = false;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginMethod {
    QR,
    Phone,
    Email,
}

impl LoginMethod {
    pub fn index(&self) -> usize {
        match self {
            LoginMethod::QR => 0,
            LoginMethod::Phone => 1,
            LoginMethod::Email => 2,
        }
    }

    pub fn from_index(i: usize) -> Self {
        match i {
            0 => LoginMethod::QR,
            1 => LoginMethod::Phone,
            _ => LoginMethod::Email,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginField {
    Username,
    Password,
    Method,
}

pub struct LoginState {
    pub selected_method: LoginMethod,
    pub username: TextInput,
    pub password: TextInput,
    pub focus: LoginField,
    pub loading: bool,
    pub error: Option<String>,
    pub captcha_sent: bool,
    pub qr_url: String,
    pub qr_key: String,
    pub qr_status_text: String,
}

impl Default for LoginState {
    fn default() -> Self {
        Self {
            selected_method: LoginMethod::Email,
            username: TextInput::new(),
            password: TextInput::new(),
            focus: LoginField::Method,
            loading: false,
            error: None,
            captcha_sent: false,
            qr_url: String::new(),
            qr_key: String::new(),
            qr_status_text: String::new(),
        }
    }
}

pub struct NavState {
    pub sections: Vec<NavSection>,
    pub section_states: Vec<ListState>,
    pub focus_section: usize,
    pub subtitle: Option<String>,
}

impl NavState {
    pub fn from_config(config: &NavConfig) -> Self {
        let sections: Vec<NavSection> = if config.sections.is_empty() {
            NavConfig::default().sections
        } else {
            config.sections.clone()
        };

        let section_states: Vec<ListState> = sections
            .iter()
            .map(|s| {
                let mut state = ListState::default();
                if !s.items.is_empty() {
                    state.select(Some(0));
                }
                state
            })
            .collect();

        Self {
            sections,
            section_states,
            focus_section: 0,
            subtitle: None,
        }
    }

    pub fn restore_focus_by_api(&mut self, api: &str) {
        for (s, section) in self.sections.iter().enumerate() {
            if let Some(i) = section
                .items
                .iter()
                .position(|item| item.api.as_deref() == Some(api))
            {
                self.focus_section = s;
                self.section_states[s].select(Some(i));
                break;
            }
        }
    }
}

pub struct NavigationState {
    pub page: Page,
    pub login: LoginState,
    pub user: Option<LoginInfo>,
    pub nav: NavState,
    pub content: ContentState,
    pub previous_content: Option<ContentState>,
    pub previous_api: Option<String>,
    pub content_selected: usize,
    pub playlist_selected: usize,
    pub search: SearchState,
    /// Cached rendered rows to avoid per-frame serde serialization.
    /// Invalidated when `content` is replaced.
    pub content_rows_cache: RefCell<Option<Vec<Vec<String>>>>,
}

impl NavigationState {
    pub fn set_content(&mut self, content: ContentState) {
        self.content = content;
        *self.content_rows_cache.borrow_mut() = None;
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub active: bool,
    pub input: TextInput,
    pub filter_queue_only: bool,
    pub unfiltered_songs: Option<Vec<SongInfo>>,
}

pub struct State {
    pub running: bool,
    pub events: EventHandler,
    pub bordered: bool,
    pub border_rounded: bool,
    pub current_color_name: String,
    pub splash: SplashState,
    pub navigation: NavigationState,
    pub command_panel: CommandPanel,
    pub offline: bool,
    pub local_music: ContentState,
    pub tick: u64,
    pub last_tick: std::time::Instant,
}

pub struct App {
    pub config: Config,
    pub state: State,
    pub playback: PlaybackEngine,
    pub theme_registry: ThemeRegistry,
    pub api: Arc<NcmClient>,
}

impl App {
    pub fn new(config: Config) -> color_eyre::Result<Self> {
        let current_color_name = config.default_theme.clone();
        let bordered = config.bordered;
        let border_rounded = config.border_rounded;

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

        let api = Arc::new(NcmClient::new()?);

        Ok(Self {
            config: config.clone(),
            api: api.clone(),
            playback: PlaybackEngine::new(tx, api.clone()),
            state: State {
                running: true,
                events,
                bordered,
                border_rounded,
                current_color_name,
                splash: SplashState::default(),
                navigation: NavigationState {
                    page: Page::Splash,
                    login: LoginState::default(),
                    user: None,
                    nav: NavState::from_config(&config.navigation),
                    content: ContentState::Empty,
                    previous_content: None,
                    previous_api: None,
                    content_selected: 0,
                    playlist_selected: 0,
                    search: SearchState::default(),
                    content_rows_cache: RefCell::new(None),
                },
                command_panel,
                offline: false,
                local_music: ContentState::Empty,
                tick: 0,
                last_tick: std::time::Instant::now(),
            },
            theme_registry,
        })
    }

    pub fn current_theme(&self) -> &Theme {
        self.theme_registry
            .get(&self.state.current_color_name)
            .unwrap_or_else(|| {
                log::warn!(
                    "Theme '{}' not found, falling back to default",
                    self.state.current_color_name
                );
                self.theme_registry.get("default").unwrap_or_else(|| {
                    log::error!("Default theme missing, using hardcoded fallback");
                    theme_fallback()
                })
            })
    }

    pub fn quit(&mut self) {
        self.playback.save_session();
        self.api.flush_cookies();
        self.state.running = false;
    }

    pub fn execute_command(&mut self, action: CommandAction) {
        match action {
            CommandAction::ToggleBordered => {
                self.state.bordered = !self.state.bordered;
            }
            CommandAction::SwitchTheme(name) => {
                self.state.current_color_name = name.clone();
                self.config.default_theme = name;
                self.config.save();
            }
        }
    }
}
