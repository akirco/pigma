use std::time::Instant;

// --- Private Internal Imports ---
use super::{CommandPanel, HelpState, LoginState, NavigationState, SplashState};
use crate::config::BorderConfig;

/// Top-level screens the TUI can be on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Splash,
    Main,
    Lyrics,
    Playlist,
    Equalizer,
    Login,
}

pub struct State {
    pub running: bool,
    pub border: BorderConfig,
    pub splash: SplashState,
    pub eq_selected: usize,
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
