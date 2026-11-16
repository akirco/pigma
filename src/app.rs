//! Main application state (`App`) and the wiring of views, events, navigation,
//! search, login and theming for the pigma TUI.

mod actions;
mod builder;
mod content;
mod context;
pub(crate) mod cover;
mod equalizer;
mod event;
mod headless;
mod login;
mod lyrics;
mod navigation;
mod run;
mod search;
mod selection;
mod splash;
mod table;

pub use context::App;
// Backwards-compatible re-exports: the IPC server used to live under `app`
// before it became a top-level module.
pub use crate::ipc::{IpcServerGuard, start_server};
