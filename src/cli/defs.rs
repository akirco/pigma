//! Clap command-line definitions for `pigma` and its subcommands.

use std::path::PathBuf;

use clap::{
    Parser, Subcommand, ValueHint,
    builder::{Styles, styling::AnsiColor},
};

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
