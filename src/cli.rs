//! CLI entry: `pigma status` / `pigma msg` subcommands plus the argument
//! parser. The TUI itself runs when no subcommand is given.

use std::path::PathBuf;

use clap::{
    Parser, Subcommand, ValueHint,
    builder::{Styles, styling::AnsiColor},
};

use crate::{
    app::App,
    cli,
    config::Config,
    ipc::{self, MsgAction, StatusSnapshot},
    logger::init_logger,
    utils::format_duration,
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

/* -------------------------------------------------------------------------- */
/*                               command actions                              */
/* -------------------------------------------------------------------------- */

/// `pigma status` handler.
pub async fn status(template: &str, json: bool, list: bool) -> color_eyre::Result<()> {
    let config = Config::load();
    if list {
        let queue = ipc::fetch_queue().await?;
        if json {
            println!("{}", serde_json::to_string_pretty(&queue)?);
        } else {
            print!("{}", render_queue(&queue));
        }
        return Ok(());
    }
    let snapshot = ipc::fetch_status().await?;
    if json || config.cli_status_format == "json" {
        println!("{}", serde_json::to_string_pretty(&snapshot)?);
    } else {
        let template = if template.is_empty() {
            &config.cli_status_template
        } else {
            template
        };
        println!("{}", format_status(template, &snapshot));
    }
    Ok(())
}

/// Render the queue as plain text: `>` marks the current song, and each row is
/// `index  title  artist  duration  id`. The trailing song id lets the output
/// feed straight into `pigma msg play <id>`.
fn render_queue(queue: &ipc::QueueSnapshot) -> String {
    let current = queue.current_index;
    let mut out = String::new();
    for (i, song) in queue.songs.iter().enumerate() {
        let marker = if Some(i) == current { ">" } else { " " };
        use std::fmt::Write;
        let _ = writeln!(
            out,
            "{marker} {:<3} {:<32} {:<24} {:<8} {}",
            i + 1,
            song.name,
            song.singer,
            format_duration(song.duration_ms),
            song.id
        );
    }
    out
}

/// `pigma msg` handler. `search` is a request/response command (the daemon
/// returns matching songs and registers them for a later `pigma msg play <id>`);
/// everything else is a fire-and-forget control action.
pub async fn msg(
    action: MsgActionArg,
    value: Option<&str>,
    playlist: Option<usize>,
) -> color_eyre::Result<()> {
    match action {
        MsgActionArg::Search => {
            let keyword = value.ok_or_else(|| {
                color_eyre::eyre::eyre!(
                    "search requires a keyword (e.g. `pigma msg search 周杰伦`)"
                )
            })?;
            let results = ipc::search_songs(keyword).await?;
            if results.is_empty() {
                println!("没有找到与「{keyword}」相关的歌曲");
                return Ok(());
            }
            for entry in &results {
                println!(
                    "{:<10} {:<20} {} - {}",
                    entry.source, entry.id, entry.name, entry.singer
                );
            }
            Ok(())
        }
        other => {
            let action = parse_msg_action(other, value, playlist)?;
            ipc::send_msg(action).await?;
            Ok(())
        }
    }
}

fn parse_msg_action(
    action: MsgActionArg,
    value: Option<&str>,
    playlist: Option<usize>,
) -> color_eyre::Result<MsgAction> {
    match action {
        MsgActionArg::Previous => Ok(MsgAction::Previous),
        MsgActionArg::Next => Ok(MsgAction::Next),
        MsgActionArg::Pause => Ok(MsgAction::Pause),
        MsgActionArg::Play => {
            let song_id = match value {
                None => None,
                Some(v) => Some(v.parse().map_err(|_| {
                    color_eyre::eyre::eyre!(
                        "invalid song id `{v}` (expected a number, or omit to play/resume)"
                    )
                })?),
            };
            Ok(MsgAction::Play { song_id })
        }
        MsgActionArg::TogglePlay => Ok(MsgAction::TogglePlay),
        MsgActionArg::SwitchList => {
            let endpoint = value.ok_or_else(|| {
                color_eyre::eyre::eyre!(
                    "switch-list requires an endpoint (e.g. `pigma msg switch-list toplist`)"
                )
            })?;
            Ok(MsgAction::SwitchList {
                endpoint: endpoint.to_string(),
                playlist,
            })
        }
        MsgActionArg::Volume => {
            let value = value.ok_or_else(|| {
                color_eyre::eyre::eyre!("volume requires a value like `75`, `+5` or `-5`")
            })?;
            parse_volume(value)
        }
        MsgActionArg::Speed => {
            let value = value.ok_or_else(|| {
                color_eyre::eyre::eyre!("speed requires a value like `1.5`, `+0.1` or `-0.1`")
            })?;
            parse_speed(value)
        }
        MsgActionArg::Pitch => Ok(MsgAction::Pitch {
            locked: parse_pitch(value)?,
        }),
        MsgActionArg::Mode => Ok(MsgAction::Mode),
        MsgActionArg::Like => Ok(MsgAction::Like),
        MsgActionArg::Dislike => Ok(MsgAction::Dislike),
        MsgActionArg::ToggleLike => Ok(MsgAction::ToggleLike),
        // Handled above in `msg` before parsing.
        MsgActionArg::Search => unreachable!(),
    }
}

/// Parse a volume value. A leading `+`/`-` is a delta (percent) applied via
/// `adjust_volume`, matching the TUI's `+`/`-` keys; a bare number is an
/// absolute volume percent.
fn parse_volume(value: &str) -> color_eyre::Result<MsgAction> {
    let err = || color_eyre::eyre::eyre!("invalid volume `{value}`");
    if let Some(delta) = value.strip_prefix('+').or_else(|| value.strip_prefix('-')) {
        let number: f64 = delta.parse().map_err(|_| err())?;
        let signed = if value.starts_with('-') {
            -number
        } else {
            number
        };
        Ok(MsgAction::Volume {
            delta: Some(signed / 100.0),
            absolute: None,
        })
    } else {
        let number: f64 = value.parse().map_err(|_| err())?;
        if !(0.0..=100.0).contains(&number) {
            return Err(err());
        }
        Ok(MsgAction::Volume {
            delta: None,
            absolute: Some(number / 100.0),
        })
    }
}

/// Parse a playback speed value. A leading `+`/`-` is a delta factor applied by
/// `adjust_speed` (matching the TUI's `[`/`]` keys, e.g. `+0.1`); a bare number
/// is an absolute factor (e.g. `1.5`). The daemon clamps both into the
/// supported range.
fn parse_speed(value: &str) -> color_eyre::Result<MsgAction> {
    let err = || color_eyre::eyre::eyre!("invalid speed `{value}` (expected e.g. `1.5`, `+0.1`)");
    if let Some(delta) = value.strip_prefix('+').or_else(|| value.strip_prefix('-')) {
        let number: f64 = delta.parse().map_err(|_| err())?;
        let signed = if value.starts_with('-') {
            -number
        } else {
            number
        };
        Ok(MsgAction::Speed {
            delta: Some(signed),
            absolute: None,
        })
    } else {
        let number: f64 = value.parse().map_err(|_| err())?;
        if number <= 0.0 {
            return Err(err());
        }
        Ok(MsgAction::Speed {
            delta: None,
            absolute: Some(number),
        })
    }
}

/// Parse the `pitch` value: `on`/`true`/`1` locks the pitch (WSOLA stretch, the
/// badge gains `*`), `off`/`false`/`0` resamples; `toggle` or no value asks the
/// daemon to flip it (`None`).
fn parse_pitch(value: Option<&str>) -> color_eyre::Result<Option<bool>> {
    match value.unwrap_or("toggle") {
        "on" | "true" | "1" | "lock" => Ok(Some(true)),
        "off" | "false" | "0" | "unlock" => Ok(Some(false)),
        "toggle" | "flip" => Ok(None),
        other => Err(color_eyre::eyre::eyre!(
            "invalid pitch `{other}` (expected `on`, `off` or `toggle`)"
        )),
    }
}

/// Replace `{token}` placeholders in a plain-status template.
fn format_status(template: &str, s: &StatusSnapshot) -> String {
    let duration = format_duration(s.duration_ms);
    let current = format_duration(s.position_ms);
    let volume = (s.volume * 100.0).round() as u64;
    let speed = format!("{:.1}", s.speed);
    // What the player bar shows: `*1.2x` when the speed change keeps the pitch,
    // `1.2x` when it resamples, and nothing at all at normal speed.
    let speed_badge = if (s.speed - 1.0).abs() < f64::EPSILON {
        format!("{:.1}x", s.speed)
    } else if s.pitch_locked {
        format!("*{:.1}x", s.speed)
    } else {
        format!("{:.1}x", s.speed)
    };
    let status = if !s.playing {
        "stopped"
    } else if s.paused {
        "paused"
    } else {
        "playing"
    };
    template
        .replace("{current}", &current)
        .replace("{position}", &current)
        .replace("{duration}", &duration)
        .replace("{artist}", &s.artist)
        .replace("{name}", &s.name)
        .replace("{album}", &s.album)
        .replace("{volume}", &volume.to_string())
        .replace("{speed}", &speed)
        .replace("{speed_badge}", &speed_badge)
        .replace("{status}", status)
        .replace("{mode}", &s.mode)
        .replace("{id}", &s.id.to_string())
        .replace("{liked}", if s.liked { "true" } else { "false" })
}

/// `pigma completions <shell>` handler: print a shell completion script for the
/// `pigma` CLI to stdout.
fn completions(shell: &str) -> color_eyre::Result<()> {
    let mut cmd = <Cli as clap::CommandFactory>::command();
    let mut out = std::io::stdout();
    match shell {
        "bash" => clap_complete::generate(clap_complete::shells::Bash, &mut cmd, "pigma", &mut out),
        "zsh" => clap_complete::generate(clap_complete::shells::Zsh, &mut cmd, "pigma", &mut out),
        "fish" => clap_complete::generate(clap_complete::shells::Fish, &mut cmd, "pigma", &mut out),
        "elvish" => {
            clap_complete::generate(clap_complete::shells::Elvish, &mut cmd, "pigma", &mut out)
        }
        "powershell" => clap_complete::generate(
            clap_complete::shells::PowerShell,
            &mut cmd,
            "pigma",
            &mut out,
        ),
        other => {
            color_eyre::eyre::bail!(
                "unsupported shell `{other}` (expected bash/zsh/fish/elvish/powershell)"
            )
        }
    }
    Ok(())
}

/// Split a `-d` value into its endpoint and optional 1-based playlist index.
/// Accepts both `endpoint` and `endpoint:N` (e.g. `toplist:3`). The legacy
/// global `--playlist` fallback stays for backward compatibility.
fn parse_daemon_endpoint(
    value: &str,
    playlist: Option<usize>,
) -> color_eyre::Result<(String, Option<usize>)> {
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
        color_eyre::eyre::bail!(
            "invalid playlist index in `{value}` (expected `ENDPOINT:N` with N > 0)"
        );
    }
    Ok((value.to_string(), None))
}

pub async fn run_cli(mut cli: Cli) -> color_eyre::Result<Option<App>> {
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
            cli::status(template, *json, *list).await?;
            return Ok(None);
        }
        Some(Command::Msg {
            action,
            value,
            playlist,
            ..
        }) => {
            cli::msg(*action, value.as_deref(), *playlist).await?;
            return Ok(None);
        }
        Some(Command::Completions { shell }) => {
            cli::completions(shell)?;
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

/* -------------------------------------------------------------------------- */
/*                                   Testing                                  */
/* -------------------------------------------------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> StatusSnapshot {
        StatusSnapshot {
            id: 123,
            name: "Example Song".into(),
            artist: "Example Artist".into(),
            album: "Example Album".into(),
            duration_ms: 125000,
            position_ms: 65000,
            volume: 0.75,
            speed: 1.0,
            pitch_locked: true,
            playing: true,
            paused: false,
            mode: "sequential".into(),
            liked: true,
        }
    }

    #[test]
    fn format_status_plain() {
        let out = format_status(
            "{current}/{duration} {artist} {name} {volume}% {speed}x {status}",
            &snapshot(),
        );
        assert_eq!(
            out,
            "01:05/02:05 Example Artist Example Song 75% 1.0x playing"
        );
    }

    #[test]
    fn format_status_speed_badge_marks_pitch_lock() {
        let mut s = snapshot();
        s.speed = 1.2;
        s.pitch_locked = true;
        assert_eq!(format_status("{speed} {speed_badge}", &s), "1.2 *1.2x");
        s.pitch_locked = false;
        assert_eq!(format_status("{speed} {speed_badge}", &s), "1.2 1.2x");
        // Normal speed still renders a badge (no star) rather than an empty token.
        s.speed = 1.0;
        assert_eq!(format_status("{speed_badge}", &s), "1.0x");
        // {speed} is untouched by the new token.
        assert_eq!(format_status("{speed}", &s), "1.0");
    }

    #[test]
    fn format_status_unknown_tokens_left_alone() {
        let out = format_status("{name} {bogus}", &snapshot());
        assert_eq!(out, "Example Song {bogus}");
    }

    #[test]
    fn parse_volume_absolute() {
        let action = parse_volume("75").unwrap();
        assert_eq!(
            action,
            MsgAction::Volume {
                delta: None,
                absolute: Some(0.75),
            }
        );
    }

    #[test]
    fn parse_volume_delta() {
        let plus = parse_volume("+5").unwrap();
        assert_eq!(
            plus,
            MsgAction::Volume {
                delta: Some(0.05),
                absolute: None,
            }
        );
        let minus = parse_volume("-10").unwrap();
        assert_eq!(
            minus,
            MsgAction::Volume {
                delta: Some(-0.10),
                absolute: None,
            }
        );
    }

    #[test]
    fn parse_volume_out_of_range() {
        assert!(parse_volume("150").is_err());
        assert!(parse_volume("abc").is_err());
    }

    #[test]
    fn parse_speed_absolute_and_delta() {
        assert_eq!(
            parse_speed("1.5").unwrap(),
            MsgAction::Speed {
                delta: None,
                absolute: Some(1.5),
            }
        );
        assert_eq!(
            parse_speed("+0.1").unwrap(),
            MsgAction::Speed {
                delta: Some(0.1),
                absolute: None,
            }
        );
        assert_eq!(
            parse_speed("-0.2").unwrap(),
            MsgAction::Speed {
                delta: Some(-0.2),
                absolute: None,
            }
        );
        assert!(parse_speed("0").is_err());
        assert!(parse_speed("abc").is_err());
    }

    #[test]
    fn parse_pitch_values() {
        assert_eq!(
            parse_msg_action(MsgActionArg::Pitch, Some("on"), None).unwrap(),
            MsgAction::Pitch { locked: Some(true) }
        );
        assert_eq!(
            parse_msg_action(MsgActionArg::Pitch, Some("off"), None).unwrap(),
            MsgAction::Pitch {
                locked: Some(false)
            }
        );
        assert_eq!(
            parse_msg_action(MsgActionArg::Pitch, None, None).unwrap(),
            MsgAction::Pitch { locked: None }
        );
        assert!(parse_msg_action(MsgActionArg::Pitch, Some("sideways"), None).is_err());
    }

    #[test]
    fn parse_msg_play_with_optional_id() {
        let plain = parse_msg_action(MsgActionArg::Play, None, None).unwrap();
        assert_eq!(plain, MsgAction::Play { song_id: None });

        let with_id = parse_msg_action(MsgActionArg::Play, Some("187186"), None).unwrap();
        assert_eq!(
            with_id,
            MsgAction::Play {
                song_id: Some(187186)
            }
        );

        assert!(parse_msg_action(MsgActionArg::Play, Some("abc"), None).is_err());
        assert_eq!(
            parse_msg_action(MsgActionArg::TogglePlay, None, None).unwrap(),
            MsgAction::TogglePlay
        );
    }

    #[test]
    fn msg_switch_list_and_aliases() {
        assert_eq!(
            parse_msg_action(MsgActionArg::SwitchList, Some("toplist"), None).unwrap(),
            MsgAction::SwitchList {
                endpoint: "toplist".into(),
                playlist: None,
            }
        );
        assert_eq!(
            parse_msg_action(MsgActionArg::ToggleLike, None, None).unwrap(),
            MsgAction::ToggleLike
        );
        assert_eq!(
            parse_msg_action(MsgActionArg::Previous, None, None).unwrap(),
            MsgAction::Previous
        );
    }

    #[test]
    fn render_queue_marks_current_and_includes_id() {
        let queue = ipc::QueueSnapshot {
            key: "test".into(),
            current_index: Some(0),
            songs: vec![
                ipc::QueueEntry {
                    id: 111,
                    name: "Song A".into(),
                    singer: "Artist A".into(),
                    album: String::new(),
                    duration_ms: 125_000,
                },
                ipc::QueueEntry {
                    id: 222,
                    name: "Song B".into(),
                    singer: "Artist B".into(),
                    album: String::new(),
                    duration_ms: 65_000,
                },
            ],
        };
        let out = render_queue(&queue);
        let mut lines = out.lines();
        let first = lines.next().unwrap();
        assert!(first.starts_with("> 1"), "current song not marked: {first}");
        assert!(first.contains("111"), "song id missing: {first}");
        let second = lines.next().unwrap();
        assert!(
            second.starts_with("  2"),
            "non-current marker wrong: {second}"
        );
        assert!(second.contains("222"), "song id missing: {second}");
    }

    #[test]
    fn completions_prints_script() {
        // Capturing stdout is awkward; instead verify the subcommand parses and
        // that each supported shell name is accepted by the parser.
        let cmd = Cli::try_parse_from(["pigma", "completions", "bash"]).unwrap();
        assert!(matches!(
            cmd.command,
            Some(Command::Completions { shell }) if shell == "bash"
        ));
        assert!(Cli::try_parse_from(["pigma", "completions", "zsh"]).is_ok());
        assert!(Cli::try_parse_from(["pigma", "completions", "fish"]).is_ok());
        assert!(Cli::try_parse_from(["pigma", "completions", "powershell"]).is_ok());
        assert!(Cli::try_parse_from(["pigma", "completions", "nushell"]).is_err());
    }
}
