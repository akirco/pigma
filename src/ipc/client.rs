//! IPC client used by the `pigma status` / `pigma msg` CLI commands.

use std::path::Path;

use color_eyre::eyre::{OptionExt, WrapErr};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::{
    path::resolve_socket_path,
    protocol::{IpcRequest, MsgAction},
    snapshot::{QueueSnapshot, SearchEntry, StatusSnapshot},
};

/// The stream a client connects with (Unix socket on unix, named pipe on
/// Windows).
#[cfg(unix)]
type ClientStream = tokio::net::UnixStream;
#[cfg(windows)]
type ClientStream = tokio::net::windows::named_pipe::NamedPipeClient;

/// Connect to the running instance's listener endpoint.
async fn client_connect(path: &Path) -> std::io::Result<ClientStream> {
    #[cfg(unix)]
    {
        ClientStream::connect(path).await
    }
    #[cfg(windows)]
    {
        tokio::net::windows::named_pipe::ClientOptions::new().open(path.to_string_lossy().as_ref())
    }
}

/// Connect to the running TUI's listener, returning a descriptive error when no
/// instance is up.
async fn connect() -> color_eyre::Result<ClientStream> {
    let path = resolve_socket_path();
    client_connect(&path)
        .await
        .wrap_err("pigma is not running (start the TUI or `pigma -d`, or check --socket)")
}

/// Send a `status` request and return the live snapshot.
pub async fn fetch_status() -> color_eyre::Result<StatusSnapshot> {
    let mut stream = connect().await?;
    stream
        .write_all(br#"{"cmd":"status"}"#)
        .await
        .wrap_err("failed to send status request")?;
    stream.write_all(b"\n").await?;
    let mut buf = String::new();
    let mut reader = BufReader::new(stream);
    reader
        .read_line(&mut buf)
        .await
        .wrap_err("failed to read status response")?;
    serde_json::from_str(&buf).wrap_err("invalid status response")
}

/// Send a `list` request and return the live playback queue.
pub async fn fetch_queue() -> color_eyre::Result<QueueSnapshot> {
    let mut stream = connect().await?;
    stream
        .write_all(br#"{"cmd":"list"}"#)
        .await
        .wrap_err("failed to send list request")?;
    stream.write_all(b"\n").await?;
    let mut buf = String::new();
    let mut reader = BufReader::new(stream);
    reader
        .read_line(&mut buf)
        .await
        .wrap_err("failed to read list response")?;
    serde_json::from_str(&buf).wrap_err("invalid list response")
}

/// Subscribe to status updates (`{"cmd":"subscribe"}`). Sends the request and
/// returns a line reader over the open connection; every subsequent
/// `StatusSnapshot` change is delivered as one JSON line. The connection stays
/// open until the daemon quits or the stream is dropped.
pub async fn subscribe_status() -> color_eyre::Result<impl tokio::io::AsyncBufRead + Unpin> {
    let mut stream = connect().await?;
    stream
        .write_all(br#"{"cmd":"subscribe"}"#)
        .await
        .wrap_err("failed to send subscribe request")?;
    stream.write_all(b"\n").await?;
    Ok(BufReader::new(stream))
}

/// Send a `search` request (`pigma msg search <keyword>`) and return the
/// matching songs, tagged by source and registered in the daemon for a later
/// `pigma msg play <id>`.
pub async fn search_songs(keyword: &str) -> color_eyre::Result<Vec<SearchEntry>> {
    let mut stream = connect().await?;
    let request = serde_json::to_string(&IpcRequest::Search {
        keyword: keyword.to_string(),
    })
    .wrap_err("failed to serialize search request")?;
    stream
        .write_all(request.as_bytes())
        .await
        .wrap_err("failed to send search request")?;
    stream.write_all(b"\n").await?;
    let mut buf = String::new();
    let mut reader = BufReader::new(stream);
    reader
        .read_line(&mut buf)
        .await
        .wrap_err("failed to read search response")?;
    serde_json::from_str(&buf).wrap_err("invalid search response")
}

/// Send a `msg` action to the running TUI. Returns once the server confirms.
pub async fn send_msg(action: MsgAction) -> color_eyre::Result<()> {
    let mut stream = connect().await?;
    let request = serde_json::to_string(&IpcRequest::Msg { action })
        .wrap_err("failed to serialize msg request")?;
    stream
        .write_all(request.as_bytes())
        .await
        .wrap_err("failed to send msg request")?;
    stream.write_all(b"\n").await?;
    let mut buf = String::new();
    let mut reader = BufReader::new(stream);
    reader
        .read_line(&mut buf)
        .await
        .wrap_err("failed to read msg response")?;
    serde_json::from_str::<serde_json::Value>(&buf)
        .ok()
        .and_then(|v| v.get("ok").and_then(|b| b.as_bool()))
        .ok_or_eyre("invalid msg response")
        .map(|_| ())
}
