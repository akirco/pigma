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

pub use crate::playback::PlaybackState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Splash,
    Main,
    Lyrics,
    Playlist,
    Login,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    pub last_tick: Instant,
    pub toast_msg: String,
    pub toast_time: Option<Instant>,
    pub playerbar_area: Rect,
    pub queue_tab_scroll_x: u16,
}
