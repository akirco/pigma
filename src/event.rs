//! Event model and the channel that bridges async workers and the main render
//! loop (`Event`, `AppEvent`, `AuthEvent`, `PlaybackEvent`,
//! `NavigationEvent`, ...).

mod handler;
mod model;

pub use handler::{EventHandler, EventSender};
pub use model::*;
