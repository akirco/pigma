//! IPC client used by the `pigma status` / `pigma msg` CLI commands.

use std::time::Duration;

use eyre::{OptionExt, WrapErr};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::{
    path::resolve_socket_path,
    protocol::{IpcRequest, MsgAction},
    snapshot::{QueueSnapshot, SearchEntry, StatusSnapshot},
    transport,
};

/// Bound connection setup so a missing/accepting-but-stuck daemon cannot hang
/// the CLI forever.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Status/list/msg are cheap; a daemon that needs longer is likely wedged.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// `search` may hit NCM and every sonar provider before replying.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(60);

async fn with_timeout<F, T>(duration: Duration, what: &str, fut: F) -> eyre::Result<T>
where
    F: std::future::Future<Output = eyre::Result<T>>,
{
    tokio::time::timeout(duration, fut)
        .await
        .map_err(|_| eyre::eyre!("timed out while {what}"))?
}

/// Connect to the running TUI's listener, returning a descriptive error when no
/// instance is up.
async fn connect() -> eyre::Result<transport::ClientStream> {
    let path = resolve_socket_path();
    with_timeout(CONNECT_TIMEOUT, "connecting to the pigma instance", async {
        transport::connect_client(&path)
            .await
            .wrap_err("pigma is not running (start the TUI or `pigma -d`, or check --socket)")
    })
    .await
}

/// Send one newline-terminated request and return the first reply line.
async fn round_trip(request: &str, timeout: Duration) -> eyre::Result<String> {
    with_timeout(timeout, "waiting for an IPC reply", async {
        let mut stream = connect().await?;
        stream
            .write_all(request.as_bytes())
            .await
            .wrap_err("failed to send IPC request")?;
        stream
            .write_all(b"\n")
            .await
            .wrap_err("failed to finish IPC request")?;
        let mut reader = BufReader::new(stream);
        let mut buf = String::new();
        reader
            .read_line(&mut buf)
            .await
            .wrap_err("failed to read IPC response")?;
        if buf.is_empty() {
            return Err(eyre::eyre!("pigma closed the connection before replying"));
        }
        Ok(buf)
    })
    .await
}

/// Send a `status` request and return the live snapshot.
pub async fn fetch_status() -> eyre::Result<StatusSnapshot> {
    let buf = round_trip(r#"{"cmd":"status"}"#, REQUEST_TIMEOUT).await?;
    serde_json::from_str(&buf).wrap_err("invalid status response")
}

/// Send a `list` request and return the live playback queue.
pub async fn fetch_queue() -> eyre::Result<QueueSnapshot> {
    let buf = round_trip(r#"{"cmd":"list"}"#, REQUEST_TIMEOUT).await?;
    serde_json::from_str(&buf).wrap_err("invalid list response")
}

/// Subscribe to status updates (`{"cmd":"subscribe"}`). Sends the request and
/// returns a line reader over the open connection; every subsequent
/// `StatusSnapshot` change is delivered as one JSON line. The connection stays
/// open until the daemon quits or the stream is dropped.
pub async fn subscribe_status() -> eyre::Result<impl tokio::io::AsyncBufRead + Unpin> {
    let mut stream = connect().await?;
    with_timeout(REQUEST_TIMEOUT, "sending the subscribe request", async {
        stream
            .write_all(br#"{"cmd":"subscribe"}"#)
            .await
            .wrap_err("failed to send subscribe request")?;
        stream
            .write_all(b"\n")
            .await
            .wrap_err("failed to finish subscribe request")?;
        Ok(BufReader::new(stream))
    })
    .await
}

/// Send a `search` request (`pigma msg search <keyword>`) and return the
/// matching songs, tagged by source and registered in the daemon for a later
/// `pigma msg play <id>`.
pub async fn search_songs(keyword: &str) -> eyre::Result<Vec<SearchEntry>> {
    let request = serde_json::to_string(&IpcRequest::Search {
        keyword: keyword.to_string(),
    })
    .wrap_err("failed to serialize search request")?;
    let buf = round_trip(&request, SEARCH_TIMEOUT).await?;
    serde_json::from_str(&buf).wrap_err("invalid search response")
}

/// Send a `msg` action to the running TUI. Returns once the server confirms.
pub async fn send_msg(action: MsgAction) -> eyre::Result<()> {
    let request = serde_json::to_string(&IpcRequest::Msg { action })
        .wrap_err("failed to serialize msg request")?;
    let buf = round_trip(&request, REQUEST_TIMEOUT).await?;
    serde_json::from_str::<serde_json::Value>(&buf)
        .ok()
        .and_then(|v| v.get("ok").and_then(|b| b.as_bool()))
        .ok_or_eyre("invalid msg response")
        .map(|_| ())
}
