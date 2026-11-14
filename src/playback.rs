//! Audio playback subsystem: re-exports the `PlaybackEngine` plus the queue/cover
//! state and play-mode types used across the app.

mod controller;
mod cover;
mod engine;
mod equalizer;
pub(crate) mod memory;
mod mode;
mod player;
mod queue;
mod source;
mod state;
mod storage;
mod stream_client;
mod stretch;

pub use cover::CoverState;
// Backwards-compatible re-export: the event type now lives in `crate::event`.
pub use crate::event::PlaybackEvent;
pub use engine::PlaybackEngine;
pub use mode::{PlayMode, mode_icon};
pub use queue::{NCM_SEARCH_QUEUE_KEY, THIRD_PARTY_QUEUE_KEY};
pub use state::{MAX_SPEED, MIN_SPEED, PlaybackState, SPEED_STEP};
