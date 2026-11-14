//! Audio processing and output primitives: WSOLA stretching, the equalizer,
//! and the persistent rodio player task.

mod equalizer;
pub mod equalizer_config;
mod event;
mod player;
mod stretch;

pub(crate) use equalizer::EqHandle;
pub use equalizer_config::*;
pub(crate) use event::PlayerEvent;
pub(crate) use player::{AudioInput, AudioReader, ControlCmd, SharedReader, run};
pub(crate) use stretch::SpeedMode;
