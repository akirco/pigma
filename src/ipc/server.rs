//! IPC server: the listener the running TUI hosts for `pigma status` /
//! `pigma msg` clients.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    sync::{Semaphore, broadcast, mpsc},
};

use crate::{
    event::{AppEvent, Event, IpcEvent},
    lock::lock_recover,
    service::SearchEngine,
};

use super::{
    path::resolve_socket_path,
    protocol::IpcRequest,
    snapshot::{QueueSnapshot, SearchEntry, StatusSnapshot},
    transport::{self, Listener},
};

/// Maximum size of one request line. `AsyncBufReadExt::read_line` is unbounded;
/// without this cap a local client could grow the server's allocation forever
/// by never sending a newline.
const MAX_REQUEST_BYTES: usize = 64 * 1024;
/// How long a client may take to send its request line.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a single reply write may block on a slow client.
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// Upper bound on simultaneous connections (including long-lived
/// `subscribe` clients), so a local client cannot exhaust the runtime.
const MAX_CONNECTIONS: usize = 32;

/// Start the IPC server for the running TUI.
///
/// Spawns a background task that accepts connections, answering `status` and
/// `list` requests from `status_snapshot` / `queue_snapshot`, streaming
/// snapshot changes to `subscribe` clients via `status_tx`, answering `search`
/// requests with `searcher`, and forwarding `msg` requests as `IpcEvent`s into
/// `event_tx`. Returns a guard that removes the socket file on drop.
pub fn start_server(
    status_snapshot: Arc<Mutex<StatusSnapshot>>,
    queue_snapshot: Arc<Mutex<QueueSnapshot>>,
    status_tx: broadcast::Sender<StatusSnapshot>,
    event_tx: mpsc::UnboundedSender<Event>,
    searcher: Arc<SearchEngine>,
) -> IpcServerGuard {
    let listener = match Listener::bind() {
        Some(l) => l,
        None => return IpcServerGuard::new(false),
    };
    let path = resolve_socket_path();
    let mut listener = listener;
    let permits = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    tokio::spawn(async move {
        loop {
            match listener.next().await {
                Some(stream) => {
                    if !listener.is_trusted(&stream) {
                        log::warn!("ipc: rejected connection from another user");
                        continue;
                    }
                    // Wait for capacity before spawning: a burst of connections
                    // queues in the OS backlog instead of creating unbounded tasks.
                    let Ok(permit) = Arc::clone(&permits).acquire_owned().await else {
                        return;
                    };
                    let snapshot = Arc::clone(&status_snapshot);
                    let queue = Arc::clone(&queue_snapshot);
                    let tx = event_tx.clone();
                    let status_tx = status_tx.clone();
                    let searcher = Arc::clone(&searcher);
                    tokio::spawn(async move {
                        let _permit = permit;
                        handle_connection(stream, snapshot, queue, status_tx, tx, searcher).await;
                    });
                }
                None => {
                    log::error!("ipc: accept failed");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    });
    log::info!("ipc: listening on {}", path.display());
    IpcServerGuard::new(true)
}

/// Removes the Unix socket file on drop (clean shutdown of the TUI).
/// On Windows the OS releases the pipe name automatically, so nothing to do.
pub struct IpcServerGuard {
    remove_on_drop: bool,
}

impl IpcServerGuard {
    fn new(remove_on_drop: bool) -> Self {
        Self { remove_on_drop }
    }
}

impl Drop for IpcServerGuard {
    fn drop(&mut self) {
        if self.remove_on_drop {
            transport::remove_endpoint();
        }
    }
}

/// Remove the Unix socket file unconditionally (used on shutdown paths where
/// the guard may already be dropped). No-op on Windows.
pub fn remove_socket() {
    transport::remove_endpoint();
}

/// Read one newline-terminated request, refusing lines larger than
/// [`MAX_REQUEST_BYTES`]. `AsyncBufReadExt::read_line` is unbounded.
async fn read_request<R>(reader: &mut R) -> std::io::Result<Option<String>>
where
    R: AsyncBufRead + Unpin,
{
    let mut line = Vec::with_capacity(256);
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            // EOF with no trailing newline: accept the line if there is one.
            return if line.is_empty() {
                Ok(None)
            } else {
                String::from_utf8(line).map(Some).map_err(invalid_utf8)
            };
        }
        if let Some(pos) = available.iter().position(|&b| b == b'\n') {
            if line.len() + pos > MAX_REQUEST_BYTES {
                return Err(request_too_large());
            }
            line.extend_from_slice(&available[..pos]);
            reader.consume(pos + 1);
            break;
        }
        if line.len() + available.len() > MAX_REQUEST_BYTES {
            return Err(request_too_large());
        }
        line.extend_from_slice(available);
        let consumed = available.len();
        reader.consume(consumed);
    }
    String::from_utf8(line).map(Some).map_err(invalid_utf8)
}

fn request_too_large() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "ipc: request exceeds the maximum size",
    )
}

fn invalid_utf8(_: std::string::FromUtf8Error) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "ipc: request is not valid UTF-8",
    )
}

async fn handle_connection<S>(
    stream: S,
    snapshot: Arc<Mutex<StatusSnapshot>>,
    queue: Arc<Mutex<QueueSnapshot>>,
    status_tx: broadcast::Sender<StatusSnapshot>,
    event_tx: mpsc::UnboundedSender<Event>,
    searcher: Arc<SearchEngine>,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let mut reader = BufReader::new(stream);
    let line = match tokio::time::timeout(REQUEST_TIMEOUT, read_request(&mut reader)).await {
        Ok(Ok(Some(line))) => line,
        Ok(Ok(None)) => return,
        Ok(Err(e)) => {
            log::debug!("ipc: failed to read request: {e}");
            return;
        }
        Err(_) => {
            log::debug!("ipc: timed out waiting for a request");
            return;
        }
    };
    let request: IpcRequest = match serde_json::from_str(&line) {
        Ok(req) => req,
        Err(e) => {
            log::debug!("ipc: invalid request: {e}");
            return;
        }
    };
    let mut stream = reader.into_inner();
    match request {
        IpcRequest::Status => {
            let reply = {
                // Clone under the lock, serialize outside it so a slow client
                // never blocks the app loop's `IpcState::update`.
                let snapshot = lock_recover(&snapshot).clone();
                serde_json::to_string(&snapshot).unwrap_or_default()
            };
            let _ = write_reply(&mut stream, &reply).await;
        }
        IpcRequest::List => {
            let reply = {
                let queue = lock_recover(&queue).clone();
                serde_json::to_string(&queue).unwrap_or_default()
            };
            let _ = write_reply(&mut stream, &reply).await;
        }
        IpcRequest::Msg { action } => {
            let event: IpcEvent = action.into();
            let sent = event_tx.send(Event::App(AppEvent::Ipc(event)));
            if sent.is_err() {
                log::error!("ipc: failed to forward msg event: receiver dropped");
            }
            let _ = write_reply(&mut stream, r#"{"ok":true}"#).await;
        }
        IpcRequest::Search { keyword } => {
            let results: Vec<SearchEntry> = searcher
                .search(&keyword)
                .await
                .into_iter()
                .map(|hit| SearchEntry::from_song(&hit.info, &hit.source))
                .collect();
            let reply = serde_json::to_string(&results).unwrap_or_default();
            let _ = write_reply(&mut stream, &reply).await;
        }
        IpcRequest::Subscribe => stream_updates(stream, snapshot, status_tx).await,
    }
}

/// Write a single JSON line (terminated by `\n`) to the client stream, bounded
/// so a stalled client cannot park a connection task forever.
async fn write_reply<S>(mut stream: S, reply: &str) -> std::io::Result<()>
where
    S: AsyncWrite + Unpin,
{
    tokio::time::timeout(WRITE_TIMEOUT, async move {
        stream.write_all(reply.as_bytes()).await?;
        stream.write_all(b"\n").await
    })
    .await
    .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "ipc: reply write timed out"))?
}

/// `subscribe` mode: send the current snapshot immediately, then stream every
/// broadcast update as a JSON line until the client disconnects or the app
/// shuts the channel down.
async fn stream_updates<S>(
    mut stream: S,
    snapshot: Arc<Mutex<StatusSnapshot>>,
    status_tx: broadcast::Sender<StatusSnapshot>,
) where
    S: tokio::io::AsyncWrite + Unpin,
{
    let mut rx = status_tx.subscribe();
    let initial = lock_recover(&snapshot).clone();
    let line = serde_json::to_string(&initial).unwrap_or_default();
    if write_reply(&mut stream, &line).await.is_err() {
        return;
    }
    loop {
        match rx.recv().await {
            Ok(s) => {
                let line = serde_json::to_string(&s).unwrap_or_default();
                if write_reply(&mut stream, &line).await.is_err() {
                    return;
                }
            }
            // A slow subscriber fell behind; resend the current snapshot so it
            // catches up instead of missing the intermediate state.
            Err(broadcast::error::RecvError::Lagged(_)) => {
                let current = lock_recover(&snapshot).clone();
                let line = serde_json::to_string(&current).unwrap_or_default();
                if write_reply(&mut stream, &line).await.is_err() {
                    return;
                }
            }
            // Sender dropped (app quitting) — close the stream.
            Err(_) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncWriteExt, BufReader};

    use super::{MAX_REQUEST_BYTES, read_request};

    #[tokio::test]
    async fn read_request_accepts_a_newline_terminated_line() {
        let (mut client, server) = tokio::io::duplex(256);
        client
            .write_all(b"{\"cmd\":\"status\"}\n")
            .await
            .expect("write request");

        let mut reader = BufReader::new(server);
        let line = read_request(&mut reader)
            .await
            .expect("read request")
            .expect("line");
        assert_eq!(line, r#"{"cmd":"status"}"#);
    }

    #[tokio::test]
    async fn read_request_rejects_overlong_lines() {
        let (mut client, server) = tokio::io::duplex(MAX_REQUEST_BYTES * 2);
        client
            .write_all(&vec![b'a'; MAX_REQUEST_BYTES + 1])
            .await
            .expect("write request");

        let mut reader = BufReader::new(server);
        let err = read_request(&mut reader)
            .await
            .expect_err("overlong request must be rejected");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }
}
