//! Application entry point: parses CLI arguments, dispatches `status`/`msg`
//! subcommands, and otherwise initializes the terminal and launches the main
//! `App` loop until quit.

use std::io::{Write, stdout};

use clap::{Parser, error::ErrorKind};
use crossterm::{
    cursor,
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    style::ResetColor,
};
use pigma::cli::{Cli, run_cli};

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut output = stdout();
        // Disable mouse reporting even when the app unwinds from a panic. The
        // ratatui panic hook restores raw mode and the alternate screen, but it
        // does not know that Pigma enabled mouse capture separately.
        let _ = execute!(output, DisableMouseCapture, ResetColor, cursor::Show);
        ratatui::restore();
        let _ = write!(output, "\r\n");
        let _ = output.flush();
    }
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cli = Cli::parse();

    // CLI subcommands (`status`/`msg`/`completions`) are one-shot queries; when
    // they fail, print a single clean line instead of the color_eyre trace.
    let app = match run_cli(cli).await {
        Ok(app) => app,
        Err(err) => {
            let msg = err
                .chain()
                .next()
                .map(ToString::to_string)
                .unwrap_or_default();
            clap::Error::raw(ErrorKind::Io, msg)
                .print()
                .expect("failed to write error");
            eprintln!();
            std::process::exit(1);
        }
    };
    let Some(app) = app else {
        return Ok(());
    };

    color_eyre::install()?;
    let terminal = ratatui::init();
    let _terminal_guard = TerminalGuard;
    execute!(stdout(), EnableMouseCapture)?;
    let result = app.run(terminal).await;
    result
}
