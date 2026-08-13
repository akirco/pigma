//! Event model and the `EventHandler` channel that bridges async workers and the
//! main render loop (`Event`, `AppEvent`, `AuthEvent`, `PlaybackEvent`,
//! `NavigationEvent`, ...).

use std::sync::Arc;
use std::time::Duration;

use color_eyre::eyre::OptionExt;
use crossterm::event::Event as CrosstermEvent;
use futures::{FutureExt, StreamExt};
use ncm_api::{LoginInfo, SongInfo};
use tokio::sync::mpsc;

use crate::playback::LyricLine;
use crate::state::{ContentState, Page, PaginationInfo, SplashLogEntry};

#[derive(Clone, Debug)]
pub enum Event {
    Crossterm(CrosstermEvent),
    App(AppEvent),
}

#[derive(Clone, Debug)]
pub enum AppEvent {
    Quit,
    Splash(SplashEvent),
    Auth(AuthEvent),
    Playback(PlaybackEvent),
    Navigation(NavigationEvent),
    Command(CommandEvent),
    Toast(String),
}

#[derive(Clone, Debug)]
pub enum SplashEvent {
    Tick {
        progress: f64,
        log: Option<SplashLogEntry>,
    },
    SetOffline,
}

#[derive(Clone, Debug)]
pub enum AuthEvent {
    Login,
    Success(LoginInfo),
    Error(String),
    QRCreated { url: String, key: String },
    QRStatus(String),
}

#[derive(Clone, Debug)]
pub enum PlaybackEvent {
    SongPlay(u64),
    Started,
    Progress {
        position: Duration,
        total: Option<Duration>,
    },
    Finished,
    Error(String),
    LyricsLoaded {
        song_id: u64,
        lyrics: Vec<LyricLine>,
        translated_lyrics: Vec<LyricLine>,
    },
    HeartbeatSong(SongInfo),
    HeartbeatFallback,
    SetPlaylistId(u64),
    LikeSong(u64, bool),
    DislikeSong(u64),
    Cached(u64),
    /// The cloud "我喜欢的音乐" list has been fetched/updated; the main thread refreshes `PlaybackState.liked` from it.
    LikedUpdated,
    /// Append lazily-paged songs to the queue identified by `key` (background
    /// full-list load after Enter on a playlist page). Songs are shared `Arc`s
    /// so appending never deep-clones the page.
    QueueAppend {
        key: String,
        songs: Vec<Arc<SongInfo>>,
    },
    /// The full song list for `playlist_id` has been queued; subsequent Enter
    /// presses on the same playlist can jump within the queue without reloading.
    QueueLoadDone {
        playlist_id: u64,
    },
}

#[derive(Clone, Debug)]
pub enum NavigationEvent {
    NavSelect(String),
    ContentLoaded(ContentState),
    ContentLoadedPaged {
        content: ContentState,
        pagination: PaginationInfo,
        generation: u64,
    },
    PlaylistSelect {
        id: u64,
        name: Option<String>,
    },
    BreadcrumbSet(String),
    SearchSong(String),
    Navigate(Page),
    SearchActivated,
    SearchDeactivated,
    ContentRestore,
    CellAction(usize, usize),
    LoadMore,
    /// A paged load failed; clears the in-flight flag so it can be retried.
    LoadMoreFailed,
    UploadCachedSong(usize),
}

#[derive(Clone, Debug)]
pub enum CommandEvent {
    Panel(CommandPanelAction),
    ToggleBordered,
}

#[derive(Clone, Debug)]
pub enum CommandPanelAction {
    Open,
    Close,
    Previous,
    Next,
    Select,
}

/// Sub-event enums convert directly into `Event` (callers need this path when using `.into()`).
/// `From<$sub> for AppEvent` is no longer generated because all callers go straight
/// `.into()` → `Event` without the `AppEvent` intermediate step. `From<AppEvent> for Event`
/// is kept separately for cases that construct an `AppEvent` variant directly, such as `AppEvent::Quit`.
macro_rules! impl_from_sub_event {
    ($variant:ident, $sub:ty) => {
        impl From<$sub> for Event {
            fn from(e: $sub) -> Self {
                Event::App(AppEvent::$variant(e))
            }
        }
    };
}

impl_from_sub_event!(Splash, SplashEvent);
impl_from_sub_event!(Auth, AuthEvent);
impl_from_sub_event!(Playback, PlaybackEvent);
impl_from_sub_event!(Navigation, NavigationEvent);
impl_from_sub_event!(Command, CommandEvent);

impl From<AppEvent> for Event {
    fn from(e: AppEvent) -> Self {
        Event::App(e)
    }
}

/// Bridges crossterm terminal events into an internal mpsc event channel.
pub struct EventHandler {
    sender: mpsc::UnboundedSender<Event>,
    receiver: mpsc::UnboundedReceiver<Event>,
}

impl Default for EventHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl EventHandler {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        let actor = EventTask::new(sender.clone());
        tokio::spawn(async { actor.run().await });
        Self { sender, receiver }
    }

    pub async fn next(&mut self) -> color_eyre::Result<Event> {
        self.receiver
            .recv()
            .await
            .ok_or_eyre("Failed to receive event")
    }

    pub fn send<E: Into<Event>>(&mut self, event: E) {
        if let Err(e) = self.sender.send(event.into()) {
            log::error!("Failed to send event: {}", e);
        }
    }

    pub fn sender(&self) -> mpsc::UnboundedSender<Event> {
        self.sender.clone()
    }
}

struct EventTask {
    sender: mpsc::UnboundedSender<Event>,
}

impl EventTask {
    fn new(sender: mpsc::UnboundedSender<Event>) -> Self {
        Self { sender }
    }

    async fn run(self) -> color_eyre::Result<()> {
        let mut reader = crossterm::event::EventStream::new();
        loop {
            let crossterm_event = reader.next().fuse();

            tokio::select! {
              _ = self.sender.closed() => {
                break;
              }
              Some(Ok(evt)) = crossterm_event => {
                self.send(Event::Crossterm(evt));
              }
            };
        }
        Ok(())
    }

    fn send(&self, event: Event) {
        if self.sender.send(event).is_err() {
            log::error!("EventTask: failed to send event: receiver dropped");
        }
    }
}
