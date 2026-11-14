//! Application bootstrap: process-level side effects (logger flush, terminal
//! guard, panic hook) and dispatch between CLI subcommands, the TUI and the
//! headless daemon.

use std::io::{Write, stdout};

use crate::{
    cli::{Cli, run_cli},
    logger::flush_logger,
};
use clap::{Parser, error::ErrorKind};
use crossterm::{
    cursor,
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    style::ResetColor,
};

struct TerminalGuard;

/// Mirror [`TerminalGuard`]'s cleanup from a panic hook.
///
/// `ratatui::init` already installs a hook that restores raw mode and the
/// alternate screen, but Pigma enables mouse capture separately. In release
/// builds (`panic = "abort"`) `Drop` never runs, so the hook is the only place
/// that can turn mouse reporting off.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let mut output = stdout();
        let _ = execute!(output, DisableMouseCapture, ResetColor, cursor::Show);
        let _ = output.flush();
        // The logger is buffered; keep the last lines leading up to the panic.
        flush_logger();
        // Ratatui's hook (the previous one) restores raw mode / alternate screen
        // and then prints the standard panic message.
        previous(info);
    }));
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut output = stdout();
        // Clean exits and unwinding panics: disable mouse reporting. A custom
        // panic hook (`install_panic_hook`) covers `panic = "abort"`, where this
        // `Drop` never runs.
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
        // all in release builds (`panic = "abort"`); `install_panic_hook` does
        // the same cleanup there.
        flush_logger();
    }
}

/// Run pigma: dispatch CLI subcommands, or launch the TUI / headless daemon.
///
/// `main` only installs the async runtime and delegates here, keeping the
/// binary crate free of application wiring.
pub async fn run() -> eyre::Result<()> {
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
            let _ = clap::Error::raw(ErrorKind::Io, msg).print();
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
            install_panic_hook();
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
