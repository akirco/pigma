//! IPC between the running TUI and the `pigma status` / `pigma msg` CLI
//! commands.
//!
//! The TUI binds a listener and accepts one-line JSON requests:
//!
//! - `{"cmd":"status"}` → the server replies with a serialized `StatusSnapshot`.
//! - `{"cmd":"subscribe"}` → the server streams each snapshot change as a JSON
//!   line until the connection closes (event push for waybar / other clients).
//! - `{"cmd":"msg","action":...}` → the server forwards an `IpcEvent` into the
//!   app's event channel and replies `{"ok":true}`.
//!
//! Transport is platform-specific: a Unix domain socket at
//! `~/.cache/pigma/pigma.sock` on Linux/macOS, and a named pipe `\\.\pipe\pigma`
//! on Windows. The endpoint is user-scoped so no authentication is needed.

#[cfg(unix)]
use std::fs;
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use color_eyre::eyre::{OptionExt, WrapErr};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{broadcast, mpsc},
};

#[cfg(unix)]
use crate::utils::pigma_cache_dir;
use crate::{
    event::{AppEvent, Event},
    playback::PlayMode,
};

/// Socket file name inside `pigma_cache_dir()` (Unix only).
pub const SOCKET_FILE: &str = "pigma.sock";

/// Default named-pipe name on Windows.
#[cfg(windows)]
const PIPE_NAME: &str = r"\\.\pipe\pigma";

/// Request sent from the CLI to the running TUI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum IpcRequest {
    Status,
    /// Return the current playback queue (`pigma status -L`).
    List,
    /// Keep the connection open and stream each `StatusSnapshot` change as a
    /// JSON line. An initial snapshot is sent immediately on connect.
    Subscribe,
    /// Search songs across NCM and the enabled sonar providers
    /// (`pigma msg search <keyword>`). The server replies with a JSON array of
    /// [`SearchEntry`]; results are registered in-process so a returned id can
    /// later be played with `pigma msg play <id>`.
    Search {
        keyword: String,
    },
    Msg {
        action: MsgAction,
    },
}

/// A playback control action for `pigma msg`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum MsgAction {
    Previous,
    Next,
    Pause,
    /// Resume when paused, start when stopped. With `song_id` set, jump to that
    /// song in the active queue and play it (`pigma msg play <id>`).
    Play {
        song_id: Option<u64>,
    },
    /// Play/pause toggle (the TUI spacebar semantics: start when stopped,
    /// resume when paused, pause when playing).
    TogglePlay,
    /// Exactly one of `delta` / `absolute` is set:
    /// - `delta`: fraction of 0..=1 to add/subtract (mirrors the TUI's `+`/`-`).
    /// - `absolute`: target fraction of 0..=1.
    Volume {
        delta: Option<f64>,
        absolute: Option<f64>,
    },
    Mode,
    Like,
    Dislike,
    ToggleLike,
    Stop,
    SeekRelative {
        seconds: f64,
    },
    SeekAbsolute {
        seconds: f64,
    },
    SetLoopStatus {
        status: String,
    },
    SetShuffle {
        shuffle: bool,
    },
    /// Dynamically switch the daemon's queue to another endpoint. `endpoint` is
    /// an API endpoint name (e.g. `toplist`, `liked`); `playlist` optionally
    /// picks the 1-based playlist within list-type endpoints.
    SwitchList {
        endpoint: String,
        playlist: Option<usize>,
    },
}

/// Runtime event dispatched to the app loop for a `msg` action.
#[derive(Debug, Clone)]
pub enum IpcEvent {
    Previous,
    Next,
    Pause,
    Play {
        song_id: Option<u64>,
    },
    TogglePlay,
    Volume {
        delta: Option<f64>,
        absolute: Option<f64>,
    },
    Mode,
    Like,
    Dislike,
    ToggleLike,
    Stop,
    SeekRelative {
        seconds: f64,
    },
    SeekAbsolute {
        seconds: f64,
    },
    SetLoopStatus {
        status: String,
    },
    SetShuffle {
        shuffle: bool,
    },
    SwitchList {
        endpoint: String,
        playlist: Option<usize>,
    },
}

impl From<MsgAction> for IpcEvent {
    fn from(action: MsgAction) -> Self {
        match action {
            MsgAction::Previous => IpcEvent::Previous,
            MsgAction::Next => IpcEvent::Next,
            MsgAction::Pause => IpcEvent::Pause,
            MsgAction::Play { song_id } => IpcEvent::Play { song_id },
            MsgAction::TogglePlay => IpcEvent::TogglePlay,
            MsgAction::Volume { delta, absolute } => IpcEvent::Volume { delta, absolute },
            MsgAction::Mode => IpcEvent::Mode,
            MsgAction::Like => IpcEvent::Like,
            MsgAction::Dislike => IpcEvent::Dislike,
            MsgAction::ToggleLike => IpcEvent::ToggleLike,
            MsgAction::Stop => IpcEvent::Stop,
            MsgAction::SeekRelative { seconds } => IpcEvent::SeekRelative { seconds },
            MsgAction::SeekAbsolute { seconds } => IpcEvent::SeekAbsolute { seconds },
            MsgAction::SetLoopStatus { status } => IpcEvent::SetLoopStatus { status },
            MsgAction::SetShuffle { shuffle } => IpcEvent::SetShuffle { shuffle },
            MsgAction::SwitchList { endpoint, playlist } => {
                IpcEvent::SwitchList { endpoint, playlist }
            }
        }
    }
}

/// Live playback state snapshot served to `pigma status`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatusSnapshot {
    pub id: u64,
    pub name: String,
    pub artist: String,
    pub album: String,
    /// Total length in milliseconds (0 when nothing is loaded).
    pub duration_ms: u64,
    /// Playback position in milliseconds.
    pub position_ms: u64,
    /// Volume as a fraction of 0..=1.
    pub volume: f64,
    pub playing: bool,
    pub paused: bool,
    /// Stable play-mode key: `sequential` / `repeat_one` / `repeat_all` /
    /// `shuffle` / `heartbeat`.
    pub mode: String,
    pub liked: bool,
    pub art_url: String,
    pub lyrics: String,
}

impl StatusSnapshot {
    pub fn from_playback(state: &crate::playback::PlaybackState) -> Self {
        let song = state.current_song.as_ref();
        let duration_ms = song.map(|s| s.duration).unwrap_or(0);
        let position_ms = song
            .map(|s| (state.progress * s.duration as f64) as u64)
            .unwrap_or(0);
        Self {
            id: song.map(|s| s.id).unwrap_or(0),
            name: song.map(|s| s.name.clone()).unwrap_or_default(),
            artist: song.map(|s| s.singer.clone()).unwrap_or_default(),
            album: song.map(|s| s.album.clone()).unwrap_or_default(),
            duration_ms,
            position_ms,
            volume: state.volume,
            playing: state.playing,
            paused: state.paused,
            mode: mode_key(&state.mode).to_string(),
            liked: state.liked,
            art_url: song.map(|s| s.pic_url.clone()).unwrap_or_default(),
            lyrics: state
                .lyrics
                .as_ref()
                .map(|lines| {
                    lines
                        .iter()
                        .map(|line| line.text.as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default(),
        }
    }

    /// Whether `self` differs from `other` in any field that is *not* the
    /// playback position. The app loop compares snapshots with this before
    /// deciding to broadcast, so a running track does not spam subscribers on
    /// every progress tick — position refreshes are instead throttled by time.
    pub fn meaningfully_differs(&self, other: &Self) -> bool {
        self.id != other.id
            || self.name != other.name
            || self.artist != other.artist
            || self.album != other.album
            || self.duration_ms != other.duration_ms
            || self.volume != other.volume
            || self.playing != other.playing
            || self.paused != other.paused
            || self.mode != other.mode
            || self.liked != other.liked
            || self.art_url != other.art_url
            || self.lyrics != other.lyrics
    }
}

fn mode_key(mode: &PlayMode) -> &'static str {
    match mode {
        PlayMode::Sequential => "sequential",
        PlayMode::RepeatOne => "repeat_one",
        PlayMode::RepeatAll => "repeat_all",
        PlayMode::Shuffle => "shuffle",
        PlayMode::Heartbeat { .. } => "heartbeat",
    }
}

/// A single entry in the playback queue, served to `pigma status -L`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueEntry {
    pub id: u64,
    pub name: String,
    pub singer: String,
    pub album: String,
    pub duration_ms: u64,
}

impl QueueEntry {
    pub fn from_song(song: &ncm_api::SongInfo) -> Self {
        Self {
            id: song.id,
            name: song.name.clone(),
            singer: song.singer.clone(),
            album: song.album.clone(),
            duration_ms: song.duration,
        }
    }
}

/// Full queue listing served to `pigma status -L`: the current song's queue
/// index (0-based, `None` when nothing is queued) plus the songs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueueSnapshot {
    pub current_index: Option<usize>,
    pub songs: Vec<QueueEntry>,
}

/// A search hit served to `pigma msg search <keyword>`. `source` tags the
/// provider: `netease` for NetEase Cloud, otherwise the sonar provider name
/// (`kugou` / `kuwo` / `bilivideo` / `youtube`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchEntry {
    pub id: u64,
    pub name: String,
    pub singer: String,
    pub album: String,
    pub duration_ms: u64,
    pub source: String,
}

impl SearchEntry {
    pub fn from_song(song: &ncm_api::SongInfo, source: &str) -> Self {
        Self {
            id: song.id,
            name: song.name.clone(),
            singer: song.singer.clone(),
            album: song.album.clone(),
            duration_ms: song.duration,
            source: source.to_string(),
        }
    }
}

fn socket_path() -> PathBuf {
    #[cfg(unix)]
    {
        pigma_cache_dir().join(SOCKET_FILE)
    }
    #[cfg(windows)]
    {
        PathBuf::from(PIPE_NAME)
    }
}

thread_local! {
    static SOCKET_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Process-wide socket-path override (set once by the CLI's `--socket` flag).
/// The thread-local test override, when present, still takes precedence.
static SOCKET_GLOBAL: OnceLock<PathBuf> = OnceLock::new();

/// Override the socket path for this thread (used by integration tests, which
/// each bind their own socket so they can run in parallel). Safe because the
/// override is thread-local.
#[doc(hidden)]
pub fn set_socket_path_override(path: Option<PathBuf>) {
    SOCKET_OVERRIDE.with(|c| *c.borrow_mut() = path);
}

/// Override the socket path process-wide (used by the CLI `--socket` flag so a
/// daemon and the `status`/`msg` commands can address a non-default instance).
pub fn set_socket_path(path: Option<PathBuf>) {
    if let Some(p) = path {
        let _ = SOCKET_GLOBAL.set(p);
    }
}

/// Resolve the socket path: a thread-local override if set, otherwise the
/// process-wide override, otherwise the default location under `pigma_cache_dir()`.
fn resolve_socket_path() -> PathBuf {
    SOCKET_OVERRIDE
        .with(|c| c.borrow().clone())
        .unwrap_or_else(|| SOCKET_GLOBAL.get().cloned().unwrap_or_else(socket_path))
}

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

/// The stream accepted by the server (Unix socket on unix, named pipe on
/// Windows).
#[cfg(unix)]
type AcceptedStream = tokio::net::UnixStream;
#[cfg(windows)]
type AcceptedStream = tokio::net::windows::named_pipe::NamedPipeServer;

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
