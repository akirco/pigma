//! Audio playback subsystem: re-exports the `PlaybackEngine` plus the queue/scan/
//! lyrics/cover state and play-mode types used across the app.

mod controller;
mod cover;
mod engine;
mod event;
mod lyrics;
mod scan;
mod source;
mod state;
mod stream_client;

pub(crate) use crate::queue::{NCM_SEARCH_QUEUE_KEY, PlayMode, THIRD_PARTY_QUEUE_KEY, mode_icon};
pub use cover::CoverState;
pub use engine::PlaybackEngine;
pub use event::PlaybackEvent;
pub use lyrics::{LyricLine, parse_lyric_lines};
pub use scan::scan_local_music;
pub use state::{MAX_SPEED, MIN_SPEED, PlaybackState, SPEED_STEP};
