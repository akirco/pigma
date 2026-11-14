//! Cross-cutting helpers: audio-file helpers, gradients, filesystem paths,
//! synchronization, terminal setup, and time formatting.

pub mod process;
pub mod sync;
pub mod terminal;
pub mod time;

pub use crate::theme::{GradientPreset, deserialize_optional, gradient_color};
pub use sync::lock_recover;
pub use time::{clock_time, format_duration, format_duration_into, local_timestamp, unix_secs};
