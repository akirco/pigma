use crate::{app::App, config::Config, logger::init_logger};
use crossterm::execute;
use std::io::stdout;

mod api;
mod app;
mod cache;
mod config;
mod event;
mod input;
mod layout;
mod logger;
mod playback;
mod service;
mod state;
mod text_input;
mod ui;
mod utils;

// #[global_allocator]
// static ALLOC: dhat::Alloc = dhat::Alloc;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    // let _dhat = dhat::Profiler::new_heap();
    let _ = rustls::crypto::ring::default_provider().install_default();
    color_eyre::install()?;
    let config = Config::load();
    init_logger(&config)?;
    let terminal = ratatui::init();
    execute!(stdout(), crossterm::event::EnableMouseCapture)?;
    let result = App::new(config)?.run(terminal).await;
    execute!(stdout(), crossterm::event::DisableMouseCapture)?;
    ratatui::restore();
    result
}
