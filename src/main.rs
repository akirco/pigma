//! Application entry point: parses CLI arguments, dispatches `status`/`msg`
//! subcommands, and otherwise initializes the terminal and launches the main
//! `App` loop until quit.

use std::io::stdout;

use clap::Parser;
use crossterm::execute;

use pigma::app::App;
use pigma::cli::{self, Cli, Command};
use pigma::config::Config;
use pigma::ipc;
use pigma::logger::init_logger;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    let cli = Cli::parse();
    let socket = match &cli.command {
        Some(Command::Status { socket, .. }) | Some(Command::Msg { socket, .. }) => {
            cli.socket.clone().or_else(|| socket.clone())
        }
        _ => cli.socket.clone(),
    };
    ipc::set_socket_path(socket);

    match cli.command {
        Some(Command::Status {
            template,
            json,
            list,
            ..
        }) => {
            return cli::status(&template, json, list).await;
        }
        Some(Command::Msg {
            action,
            value,
            playlist,
            ..
        }) => {
            return cli::msg(&action, value.as_deref(), playlist).await;
        }
        Some(Command::List { endpoint }) => {
            return cli::list(&endpoint).await;
        }
        None => {}
    }

    let _ = rustls::crypto::ring::default_provider().install_default();
    color_eyre::install()?;
    let config = Config::load();
    init_logger(&config)?;

    // Headless daemon mode
    if let Some(endpoint) = cli.daemon {
        return App::new(config, false)?
            .run_headless(&endpoint, cli.playlist)
            .await;
    }

    let terminal = ratatui::init();
    execute!(stdout(), crossterm::event::EnableMouseCapture)?;
    let result = App::new(config, true)?.run(terminal).await;
    execute!(stdout(), crossterm::event::DisableMouseCapture)?;
    ratatui::restore();
    result
}
