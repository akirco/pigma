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
use pigma::{
    cli::{Cli, run_cli},
    logger::flush_logger,
};

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut output = stdout();
        // Disable mouse reporting even when the app unwinds from a panic. The
        // ratatui panic hook restores raw mode and the alternate screen, but it
        // does not know that Pigma enabled mouse capture separately.
        let _ = execute!(output, DisableMouseCapture, ResetColor, cursor::Show);
        // On the panic path the ratatui panic hook has already called restore(),
        // so only call it on clean exits to avoid restoring the terminal twice.
        if !std::thread::panicking() {
            ratatui::restore();
        }
        let _ = write!(output, "\r\n");
        let _ = output.flush();
        // The file logger is buffered; flush here so lines written just before a
        // panic still reach disk on the unwind path. This `Drop` does not run at
        // all in release builds (`panic = "abort"`); there only the immediate
        // flush of warn/error records survives a crash.
        flush_logger();
    }
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cli = Cli::parse();

    // CLI subcommands (`status`/`msg`/`completions`) are one-shot queries; when
    // they fail, print a single clean line instead of the eyre report.
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
            // `process::exit` skips destructors, and the logger is buffered, so
            // flush explicitly or the context lines leading up to the failure
            // are lost.
            flush_logger();
            std::process::exit(1);
        }
    };
    let result = match app {
        Some(app) => {
            let terminal = ratatui::init();
            let _terminal_guard = TerminalGuard;
            execute!(stdout(), EnableMouseCapture)?;
            app.run(terminal).await
        }
        // Headless daemon already ran `run_cli`; nothing left to drive here.
        None => Ok(()),
    };
    // The file logger is buffered. The panic/unwind path is covered by
    // `TerminalGuard`'s drop; this flush covers clean returns, including the
    // headless `None` branch that never builds a `TerminalGuard`. The logger is
    // only installed once `run_cli` gets past the one-shot
    // `status`/`msg`/`completions` branches.
    flush_logger();
    result
}
