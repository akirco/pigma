use crate::{config::Config, logger::init_logger, state::App};
use crossterm::execute;
use std::io::stdout;

pub mod app;
pub mod cache;
pub mod config;
pub mod event;
pub mod field;
pub mod input;
pub mod layout;
pub mod logger;
pub mod playback;
pub mod state;
pub mod theme;
pub mod types;
pub mod ui;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let config = Config::load();
    init_logger(&config)?;
    let terminal = ratatui::init();
    let _ = execute!(stdout(), crossterm::event::EnableMouseCapture);
    let result = App::new(config)?.run(terminal).await;
    let _ = execute!(stdout(), crossterm::event::DisableMouseCapture);
    ratatui::restore();
    result
}
