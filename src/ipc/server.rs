//! IPC server: the listener the running TUI hosts for `pigma status` /
//! `pigma msg` clients.

#[cfg(unix)]
use std::fs;
use std::sync::{Arc, Mutex};

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{broadcast, mpsc},
};

use crate::event::{AppEvent, Event};

use super::{
    path::resolve_socket_path,
    protocol::{IpcEvent, IpcRequest},
    snapshot::{QueueSnapshot, StatusSnapshot},
};

/// The stream accepted by the server (Unix socket on unix, named pipe on
/// Windows).
#[cfg(unix)]
type AcceptedStream = tokio::net::UnixStream;
#[cfg(windows)]
type AcceptedStream = tokio::net::windows::named_pipe::NamedPipeServer;

/// Platform-specific listener for the IPC server. On Windows a named pipe is
/// re-created for every connection, so `next()` holds the pipe name rather than
/// a persistent handle.
enum IpcListener {
    #[cfg(unix)]
    Unix(tokio::net::UnixListener),
    #[cfg(windows)]
    Pipe { name: String },
}

/// Bind the listener, clearing any stale file left by a previous run on Unix.
/// Returns `None` when another pigma instance already holds the endpoint.
impl IpcListener {
    fn bind() -> Option<Self> {
        let path = resolve_socket_path();
        #[cfg(unix)]
        {
            if let Some(dir) = path.parent() {
                let _ = fs::create_dir_all(dir);
            }
            match tokio::net::UnixListener::bind(&path) {
                Ok(listener) => Some(Self::Unix(listener)),
                Err(_) => {
                    // Either a live instance owns the socket or it is stale.
                    // A non-blocking connect probe tells us which: if we can
                    // connect, another instance is running and we must not
                    // steal the socket.
                    if std::os::unix::net::UnixStream::connect(&path).is_ok() {
                        log::warn!(
                            "ipc: another pigma instance already owns {}",
                            path.display()
                        );
                        return None;
                    }
                    let _ = fs::remove_file(&path);
                    tokio::net::UnixListener::bind(&path).ok().map(Self::Unix)
                }
            }
        }
        #[cfg(windows)]
        {
            let name = path.to_string_lossy().into_owned();
            match tokio::net::windows::named_pipe::ServerOptions::new().create(&name) {
                Ok(_) => Some(Self::Pipe { name }),
                Err(e) => {
                    // Windows releases the pipe name when the owning process
                    // exits, so a failed bind always means a live instance.
                    log::warn!("ipc: another pigma instance already owns the pipe {name}: {e}");
                    None
                }
            }
        }
    }

    /// Wait for the next incoming connection, returning the accepted stream.
    async fn next(&mut self) -> Option<AcceptedStream> {
        match self {
            #[cfg(unix)]
            Self::Unix(l) => l.accept().await.ok().map(|(stream, _)| stream),
            #[cfg(windows)]
            Self::Pipe { name } => {
                // A fresh server instance per connection; after a client
                // attaches, that instance becomes the connection stream.
                let server = tokio::net::windows::named_pipe::ServerOptions::new()
                    .create(name)
                    .ok()?;
                server.connect().await.ok()?;
                Some(server)
            }
        }
    }
}

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
    searcher: Arc<crate::app::SearchEngine>,
) -> IpcServerGuard {
    let listener = match IpcListener::bind() {
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
    #[cfg_attr(windows, allow(dead_code))]
    remove_on_drop: bool,
}

impl IpcServerGuard {
    fn new(remove_on_drop: bool) -> Self {
        Self { remove_on_drop }
    }
}

impl Drop for IpcServerGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        if self.remove_on_drop {
            let _ = fs::remove_file(resolve_socket_path());
        }
    }
}

/// Remove the Unix socket file unconditionally (used on shutdown paths where
/// the guard may already be dropped). No-op on Windows.
pub fn remove_socket() {
    #[cfg(unix)]
    let _ = fs::remove_file(resolve_socket_path());
}

async fn handle_connection<S>(
    stream: S,
    snapshot: Arc<Mutex<StatusSnapshot>>,
    queue: Arc<Mutex<QueueSnapshot>>,
    status_tx: broadcast::Sender<StatusSnapshot>,
    event_tx: mpsc::UnboundedSender<Event>,
    searcher: Arc<crate::app::SearchEngine>,
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
                let guard = snapshot.lock().unwrap();
                serde_json::to_string(&*guard).unwrap_or_default()
            };
            let _ = write_reply(&mut stream, &reply).await;
        }
        IpcRequest::List => {
            let reply = {
                let guard = queue.lock().unwrap();
                serde_json::to_string(&*guard).unwrap_or_default()
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
            let results = searcher.search(&keyword).await;
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
    let mut framed = reply.to_string();
    framed.push('\n');
    stream.write_all(framed.as_bytes()).await
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
    let initial = snapshot.lock().unwrap().clone();
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
                let current = snapshot.lock().unwrap().clone();
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
