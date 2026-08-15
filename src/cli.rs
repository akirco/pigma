//! CLI entry: `pigma status` / `pigma msg` / `pigma list` subcommands plus the
//! argument parser. The TUI itself runs when no subcommand is given.

use clap::builder::Styles;
use clap::builder::styling::AnsiColor;
use clap::{Parser, Subcommand};

use std::path::PathBuf;
use std::sync::Arc;

use crate::config::Config;
use crate::ipc::{self, MsgAction, StatusSnapshot};

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
    long_about = "A netease cloud music client.\n\nNo subcommand launches the TUI; `status` / `msg` / `list` query or control a running instance.",
    args_conflicts_with_subcommands = true,
    styles = STYLES
)]
pub struct Cli {
    /// Print version and exit.
    #[arg(short = 'v', long = "version", action = clap::ArgAction::Version)]
    pub version: Option<bool>,
    /// Run headless, loading an endpoint (default `__liked__`).
    #[arg(
        long,
        short = 'd',
        value_name = "ENDPOINT",
        num_args = 0..=1,
        default_missing_value = "__liked__"
    )]
    pub daemon: Option<String>,
    /// Load the N-th playlist of the endpoint (1-based).
    #[arg(long, value_name = "INDEX")]
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
        /// {volume} {status} {mode} {id} {liked}. Defaults to config.
        #[arg(long, default_value = "")]
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
    /// List an endpoint's playlists (or songs) with 1-based indexes.
    List {
        /// API endpoint: `toplist`, `top_song_list`, `__liked__`, etc.
        endpoint: String,
    },
    /// Control the running instance.
    Msg {
        /// Action: previous | next | pause | play | volume | mode | like |
        /// dislike | toggle_like | switch-list.
        action: String,
        /// Value for `volume` (`75`/`+5`/`-5`) or the endpoint for `switch-list`.
        #[arg(allow_hyphen_values = true)]
        value: Option<String>,
        /// Playlist index for `switch-list` (1-based).
        #[arg(long, value_name = "INDEX")]
        playlist: Option<usize>,
        /// IPC socket path.
        #[arg(long, value_name = "SOCKET")]
        socket: Option<PathBuf>,
    },
}

/// `pigma status` handler.
pub async fn status(template: &str, json: bool, list: bool) -> color_eyre::Result<()> {
    let config = Config::load();
    if list {
        let queue = ipc::fetch_queue().await?;
        if json {
            println!("{}", serde_json::to_string_pretty(&queue)?);
        } else {
            let current = queue.current_index;
            for (i, song) in queue.songs.iter().enumerate() {
                let marker = if Some(i) == current { ">" } else { " " };
                println!(
                    "{marker} {:<3} {:<32} {:<24} {}",
                    i + 1,
                    song.name,
                    song.singer,
                    format_duration(song.duration_ms)
                );
            }
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

/// `pigma list` handler: resolve an endpoint without a running daemon and print
/// the playlists (or songs) it yields, numbered by their 1-based index.
pub async fn list(endpoint: &str) -> color_eyre::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let config = Config::load();

    let cookie_path = crate::utils::pigma_config_dir().join("cookies.json");
    let mut api_builder = ncm_api::NcmClient::builder().cookie_path(cookie_path);
    let ncm_proxy = match config.proxy_target {
        crate::config::ProxyTarget::Reversed | crate::config::ProxyTarget::Both => {
            config.proxy.as_str()
        }
        _ => "",
    };
    if !ncm_proxy.is_empty() {
        api_builder = api_builder.proxy(ncm_proxy);
    }
    let api = Arc::new(api_builder.build()?);

    let cache_dir = {
        let path = std::path::Path::new(&config.cache.cache_dir);
        if path.is_absolute() {
            std::path::PathBuf::from(&config.cache.cache_dir)
        } else {
            crate::utils::pigma_cache_dir().join(&config.cache.cache_dir)
        }
    };
    let cache = Arc::new(crate::cache::CacheManager::new(
        cache_dir,
        crate::utils::pigma_cache_dir(),
        config.cache.cache_template.clone(),
    ));
    let service = crate::service::ApiService::new(api.clone(), cache);

    let uid = if api.is_logged_in() {
        api.login_status().await.ok().map(|info| info.uid)
    } else {
        None
    };

    let api_ep = crate::service::ApiEndpoint::parse(endpoint).unwrap_or_else(|| {
        eprintln!("未知端点: {endpoint}");
        std::process::exit(1);
    });

    let (content, _) = service
        .resolve_content(api_ep, uid, config.search_limit)
        .await;

    match content {
        crate::state::ContentState::SongLists(lists) => {
            for (i, list) in lists.iter().enumerate() {
                println!("{:>3}. {}", i + 1, list.name);
            }
        }
        crate::state::ContentState::TopLists(lists) => {
            for (i, list) in lists.iter().enumerate() {
                println!("{:>3}. {}", i + 1, list.name);
            }
        }
        crate::state::ContentState::Songs(songs) => {
            for (i, song) in songs.iter().enumerate() {
                println!("{:>3}. {} - {}", i + 1, song.name, song.singer);
            }
        }
        crate::state::ContentState::Error(e) => {
            eprintln!("{endpoint}: {e}");
        }
        _ => eprintln!("{endpoint}: 无可列出的内容"),
    }
    Ok(())
}

/// `pigma msg` handler.
pub async fn msg(
    action: &str,
    value: Option<&str>,
    playlist: Option<usize>,
) -> color_eyre::Result<()> {
    let action = parse_msg_action(action, value, playlist)?;
    ipc::send_msg(action).await?;
    Ok(())
}

fn parse_msg_action(
    action: &str,
    value: Option<&str>,
    playlist: Option<usize>,
) -> color_eyre::Result<MsgAction> {
    match action {
        "previous" | "prev" => Ok(MsgAction::Previous),
        "next" => Ok(MsgAction::Next),
        "pause" => Ok(MsgAction::Pause),
        "play" => Ok(MsgAction::Play),
        "switch-list" | "list" | "switch" => {
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
        "volume" => {
            let value = value.ok_or_else(|| {
                color_eyre::eyre::eyre!("volume requires a value like `75`, `+5` or `-5`")
            })?;
            parse_volume(value)
        }
        "mode" => Ok(MsgAction::Mode),
        "like" => Ok(MsgAction::Like),
        "dislike" => Ok(MsgAction::Dislike),
        "toggle_like" | "unlike" | "toggle" => Ok(MsgAction::ToggleLike),
        other => Err(color_eyre::eyre::eyre!(
            "unknown action `{other}` (expected previous/next/pause/play/switch-list/volume/mode/like/dislike/toggle_like)"
        )),
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

/// Replace `{token}` placeholders in a plain-status template.
fn format_status(template: &str, s: &StatusSnapshot) -> String {
    let duration = format_duration(s.duration_ms);
    let current = format_duration(s.position_ms);
    let volume = (s.volume * 100.0).round() as u64;
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
        .replace("{status}", status)
        .replace("{mode}", &s.mode)
        .replace("{id}", &s.id.to_string())
        .replace("{liked}", if s.liked { "true" } else { "false" })
}

fn format_duration(ms: u64) -> String {
    let total_secs = ms / 1000;
    format!("{}:{:02}", total_secs / 60, total_secs % 60)
}

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
            playing: true,
            paused: false,
            mode: "sequential".into(),
            liked: true,
        }
    }

    #[test]
    fn format_status_plain() {
        let out = format_status(
            "{current}/{duration} {artist} {name} {volume}% {status}",
            &snapshot(),
        );
        assert_eq!(out, "1:05/2:05 Example Artist Example Song 75% playing");
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
}
