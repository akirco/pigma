//! Shared application state: the active `Page`, the navigation/search/login/help
//! sub-state, and `PaginationInfo` for lazy-loaded content.

pub mod command;
pub mod content;
pub mod help;
pub mod login;
pub mod navigation;
pub mod search;
pub mod splash;

pub use command::*;
pub use content::*;
pub use help::*;
pub use login::*;
pub use navigation::*;
pub use search::*;
pub use splash::*;

use std::time::Instant;

use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};

use crate::config::BorderConfig;
use crate::event::EventHandler;

/// Top-level screens the TUI can be on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Splash,
    Main,
    Lyrics,
    Playlist,
    Login,
}

/// Pagination state for a lazily-loaded content view (e.g. a playlist or
/// search results page). Drives "load more" and the loading indicator.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaginationInfo {
    /// API/endpoint key this pagination belongs to.
    pub api: String,
    /// Current offset of loaded items.
    pub offset: u32,
    /// Page size requested.
    pub limit: u32,
    /// Whether more items are available from the API.
    pub has_more: bool,
    /// Total item count reported by the API (0 when unknown).
    pub total: u64,
    /// Whether a load is currently in flight.
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

pub struct State {
    pub running: bool,
    pub events: EventHandler,
    pub border: BorderConfig,
    pub splash: SplashState,
    pub login: LoginState,
    pub navigation: NavigationState,
    pub command_panel: CommandPanel,
    pub help: HelpState,
    pub offline: bool,
    pub tick: u64,
    pub last_tick: Instant,
    pub toast_msg: String,
    pub toast_time: Option<Instant>,
    /// Layout rect of the player bar, cached by the draw pass (`ui::draw`) and
    /// consumed by mouse input to hit-test volume scrolling on the player bar.
    pub playerbar_area: Rect,
}
