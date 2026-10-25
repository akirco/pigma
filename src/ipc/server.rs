//! IPC server: the listener the running TUI hosts for `pigma status` /
//! `pigma msg` clients.

use std::sync::{Arc, Mutex};

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{broadcast, mpsc},
};

use crate::{
    event::{AppEvent, Event, IpcEvent},
    service::SearchEngine,
    utils::lock_recover,
};

use super::{
    path::resolve_socket_path,
    protocol::IpcRequest,
    snapshot::{QueueSnapshot, SearchEntry, StatusSnapshot},
    transport::{self, Listener},
};

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
    tokio::spawn(async move {
        loop {
            match listener.next().await {
                Some(stream) => {
                    let snapshot = Arc::clone(&status_snapshot);
                    let queue = Arc::clone(&queue_snapshot);
                    let tx = event_tx.clone();
                    let status_tx = status_tx.clone();
                    let searcher = Arc::clone(&searcher);
                    tokio::spawn(async move {
                        handle_connection(stream, snapshot, queue, status_tx, tx, searcher).await;
                    });
                }
                None => {
                    log::error!("ipc: accept failed");
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
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
    let mut stream = BufReader::new(stream);
    let mut line = String::new();
    if stream.read_line(&mut line).await.is_err() {
        return;
    }
    let request: IpcRequest = match serde_json::from_str(&line) {
        Ok(req) => req,
        Err(e) => {
            log::debug!("ipc: invalid request: {e}");
            return;
        }
    };
    let mut stream = stream.into_inner();
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

/// Write a single JSON line (terminated by `\n`) to the client stream.
async fn write_reply<S>(mut stream: S, reply: &str) -> std::io::Result<()>
where
    S: tokio::io::AsyncWrite + Unpin,
{
    stream.write_all(reply.as_bytes()).await?;
    stream.write_all(b"\n").await
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
