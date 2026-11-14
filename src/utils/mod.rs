//! Cross-cutting helpers: audio-file helpers, gradients, filesystem paths,
//! synchronization, terminal setup, and time formatting.

pub mod process;
pub mod sync;
pub mod terminal;

pub use sync::lock_recover;
