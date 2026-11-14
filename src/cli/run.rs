//! CLI dispatch: runs the selected subcommand, or builds the `App` that the
//! TUI/headless entry point will run.

use super::{Cli, Command, completions, msg, status};
use crate::{app::App, config::Config, ipc, logger::init_logger};

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
            completions::completions(shell)?;
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
