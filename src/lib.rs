//! pigma library crate: the netease cloud music TUI and its CLI helpers.
//!
//! The modules are exposed so the binary entry point (`src/main.rs`) and the
//! CLI subcommands (`src/cli.rs`) can share the app logic without duplicating
//! it. `main.rs` keeps only process-level side effects (terminal init, argument
//! dispatch); everything else lives here.

pub mod app;
mod bootstrap;
pub mod cache;
pub mod cli;
pub mod config;
pub mod event;
pub(crate) mod icons;
pub mod input;
pub mod ipc;
pub(crate) mod lock;
pub mod logger;
pub mod lyrics;
pub(crate) mod memory;
mod persist;
pub mod playback;
pub mod service;
pub mod state;
pub mod text_input;
pub(crate) mod time;
pub mod ui;

pub use bootstrap::run;
