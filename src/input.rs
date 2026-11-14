//! Keyboard (and mouse) input handling: maps events onto `App` actions. The
//! state transitions themselves live on `App` (`app::{navigation, selection,
//! table}`); the per-view handlers here only decide which action to trigger.

mod command;
mod equalizer;
mod global;
mod help;
mod login;
mod main;
mod search;
mod splash;

mod dispatch;
pub use dispatch::{handle_key_events, handle_mouse_event};
