//! Shared application state: the active `Page` and the top-level `State` that
//! bundles the navigation/search/login/help sub-state.

pub mod command;
pub mod content;
pub mod help;
pub mod login;
pub mod navigation;
pub mod search;
pub mod splash;

use std::time::Instant;

pub use command::*;
pub use content::*;
pub use help::*;
pub use login::*;
pub use navigation::*;
pub use search::*;
pub use splash::*;

// --- Private Internal Imports ---
use crate::{config::BorderConfig, event::EventHandler};

/// Top-level screens the TUI can be on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Splash,
    Main,
    Lyrics,
    Playlist,
    Login,
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
}
