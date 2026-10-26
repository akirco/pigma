//! CLI entry: `pigma status` / `pigma msg` subcommands plus the argument
//! parser. The TUI itself runs when no subcommand is given.
//!
//! Subcommand handlers live in the `status` / `msg` / `completions` submodules;
//! this module keeps the clap definitions and the `run_cli` dispatcher.

mod completions;
mod msg;
mod status;

pub use msg::msg;
pub use status::status;

use completions::completions;

use std::path::PathBuf;

use clap::{
    Parser, Subcommand, ValueHint,
    builder::{Styles, styling::AnsiColor},
};

use crate::{app::App, config::Config, ipc, logger::init_logger};

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Yellow.on_default().bold())
    .usage(AnsiColor::Yellow.on_default().bold())
    .literal(AnsiColor::Cyan.on_default().bold())
    .placeholder(AnsiColor::Cyan.on_default());

#[derive(Debug, Parser)]
#[command(
    name = "pigma",
    version,
    disable_version_flag = true,
    about = "A netease cloud music client",
    long_about = "A netease cloud music client.\n\nNo subcommand launches the TUI; `status` / `msg` query or control a running instance.",
    args_conflicts_with_subcommands = true,
    styles = STYLES
)]
pub struct Cli {
    /// Print version and exit.
    #[arg(short = 'v', long = "version", action = clap::ArgAction::Version)]
    pub version: Option<bool>,
    /// Run headless, loading an endpoint (default `liked`). Suffix `:N` picks
    /// the N-th playlist of a list endpoint, e.g. `toplist:3`.
    #[arg(
        long,
        short = 'd',
        value_name = "ENDPOINT[:N]",
        num_args = 0..=1,
        default_missing_value = "liked"
    )]
    pub daemon: Option<String>,
    /// Load the N-th playlist of the endpoint (1-based).
    #[arg(long, value_name = "INDEX", requires = "daemon", hide = true, value_hint = ValueHint::Other)]
    pub playlist: Option<usize>,
    /// IPC socket path.
    #[arg(long, value_name = "SOCKET")]
    pub socket: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Show the running instance's status.
    Status {
        /// Output template: {name} {artist} {album} {duration} {position}
        /// {volume} {speed} {speed_badge} {status} {mode} {id} {liked}. Defaults
        /// to config.
        #[arg(long, default_value = "", value_hint = ValueHint::Other)]
        template: String,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
        /// List the playback queue (`>` marks the current song).
        #[arg(short = 'L', long)]
        list: bool,
        /// IPC socket path.
        #[arg(long, value_name = "SOCKET")]
        socket: Option<PathBuf>,
    },
    /// Control the running instance.
    Msg {
        /// Playback action. Possible values are listed below.
        action: MsgActionArg,
        /// Value for `play` (a song id), `search` (a keyword), `volume`
        /// (`75`/`+5`/`-5`), `speed` (`1.5`/`+0.1`), `pitch` (`on`/`off`/`toggle`),
        /// the endpoint for `switch-list`, or omitted.
        #[arg(allow_hyphen_values = true, value_hint = ValueHint::Other)]
        value: Option<String>,
        /// Playlist index for `switch-list` (1-based).
        #[arg(long, value_name = "INDEX", value_hint = ValueHint::Other)]
        playlist: Option<usize>,
        /// IPC socket path.
        #[arg(long, value_name = "SOCKET")]
        socket: Option<PathBuf>,
    },
    /// Generate a shell completion script and print it to stdout.
    Completions {
        /// Target shell: bash | zsh | fish | elvish | powershell.
        #[arg(value_parser = ["bash", "zsh", "fish", "elvish", "powershell"])]
        shell: String,
    },
}

/// `pigma msg` action selector. The `#[value(name)]`/`#[value(alias)]` names are
/// what the shell-completion script offers (and what `parse_msg_action` accepts).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum MsgActionArg {
    /// Go to the previous song.
    #[value(name = "previous", alias = "prev")]
    Previous,
    /// Go to the next song.
    Next,
    /// Pause playback.
    Pause,
    /// Play / resume, or play a specific song id in the queue.
    Play,
    /// Toggle play/pause (start when stopped, resume when paused).
    #[value(
        name = "toggle_play",
        alias = "play_pause",
        alias = "toggle-play",
        alias = "play-pause"
    )]
    TogglePlay,
    /// Switch the queue to another endpoint (optionally `--playlist N`).
    #[value(name = "switch-list", alias = "switch")]
    SwitchList,
    /// Set (absolute 0-100) or adjust (`+5`/`-5`) the volume.
    Volume,
    /// Set (absolute, e.g. `1.5`) or adjust (`+0.1`/`-0.1`) the playback speed.
    Speed,
    /// Toggle how speed is applied: `on` keeps the pitch (badge `*`), `off`
    /// resamples, omitted/`toggle` flips it.
    #[value(name = "pitch", alias = "pitch-lock")]
    Pitch,
    /// Cycle the playback mode.
    Mode,
    /// Like the current song.
    Like,
    /// Dislike the current song.
    Dislike,
    /// Toggle like on the current song.
    #[value(name = "toggle_like", alias = "unlike", alias = "toggle")]
    ToggleLike,
    /// Search songs across NCM + sonar sources.
    Search,
}

/// Split a `-d` value into its endpoint and optional 1-based playlist index.
/// Accepts both `endpoint` and `endpoint:N` (e.g. `toplist:3`). The legacy
/// global `--playlist` fallback stays for backward compatibility.
fn parse_daemon_endpoint(
    value: &str,
    playlist: Option<usize>,
) -> eyre::Result<(String, Option<usize>)> {
    if let Some((ep, idx)) = value.rsplit_once(':')
        && !ep.is_empty()
        && let Ok(n) = idx.parse::<usize>()
        && n > 0
    {
        return Ok((ep.to_string(), Some(n)));
    }
    if playlist.is_some() {
        return Ok((value.to_string(), playlist));
    }
    // A trailing `:N` that failed to parse is a user error, not an endpoint.
    if value.contains(':') {
        eyre::bail!("invalid playlist index in `{value}` (expected `ENDPOINT:N` with N > 0)");
    }
    Ok((value.to_string(), None))
}

pub async fn run_cli(mut cli: Cli) -> eyre::Result<Option<App>> {
    let socket = match &cli.command {
        Some(Command::Status {
            socket: cmd_socket, ..
        })
        | Some(Command::Msg {
            socket: cmd_socket, ..
        }) => match cmd_socket {
            Some(s) => Some(s.clone()),
            None => cli.socket.take(),
        },
        _ => cli.socket.take(),
    };
    ipc::set_socket_path(socket);

    match &cli.command {
        Some(Command::Status {
            template,
            json,
            list,
            ..
        }) => {
            status(template, *json, *list).await?;
            return Ok(None);
        }
        Some(Command::Msg {
            action,
            value,
            playlist,
            ..
        }) => {
            msg(*action, value.as_deref(), *playlist).await?;
            return Ok(None);
        }
        Some(Command::Completions { shell }) => {
            completions(shell)?;
            return Ok(None);
        }
        None => {}
    }

    let config = Config::load();
    init_logger(&config)?;

    if let Some(endpoint) = cli.daemon {
        let (endpoint, playlist) = parse_daemon_endpoint(&endpoint, cli.playlist)?;
        App::new(config, false)?
            .run_headless(&endpoint, playlist)
            .await?;
        return Ok(None);
    }

    Ok(Some(App::new(config, true)?))
}
