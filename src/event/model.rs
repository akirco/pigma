//! Event payload types exchanged between async workers and the app loop.

use std::{sync::Arc, time::Duration};

use crossterm::event::Event as CrosstermEvent;
use ncm_api::{LoginInfo, SongInfo};

use crate::{
    lyrics::LyricLine,
    state::{ContentState, Page, PaginationInfo, SplashLogEntry},
};

#[derive(Clone, Debug)]
pub enum Event {
    Crossterm(CrosstermEvent),
    App(AppEvent),
}

#[derive(Clone, Debug)]
pub enum AppEvent {
    Quit,
    /// Ask the event loop to redraw without changing state (e.g. a cover
    /// finished loading in the background).
    Redraw,
    Splash(SplashEvent),
    Auth(AuthEvent),
    Playback(PlaybackEvent),
    Navigation(NavigationEvent),
    Command(CommandEvent),
    Toast(String),
    /// Control request received over the IPC socket (`pigma msg`). The payload
    /// enum is defined here so the event channel does not depend on `ipc`;
    /// `ipc` converts its wire-level `MsgAction` via `From`.
    Ipc(IpcEvent),
}

/// Runtime playback-control request carried by [`AppEvent::Ipc`], mirrored
/// from the IPC wire protocol's `MsgAction` (see `ipc::protocol`).
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
    Speed {
        delta: Option<f64>,
        absolute: Option<f64>,
    },
    Pitch {
        locked: Option<bool>,
    },
    Mode,
    Like,
    Dislike,
    ToggleLike,
    SwitchList {
        endpoint: String,
        playlist: Option<usize>,
    },
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
    /// Output-device failure (`Some`) or recovery (`None`); never advances the
    /// queue, unlike a decode error.
    DeviceError(Option<String>),
    LyricsLoaded {
        song_id: u64,
        lyrics: Vec<LyricLine>,
        translated_lyrics: Vec<LyricLine>,
    },
    HeartbeatSong {
        request: u64,
        song: Arc<SongInfo>,
    },
    HeartbeatFallback {
        request: u64,
    },
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
    ContentLoaded {
        content: ContentState,
        /// Generation of the load that produced this result; handlers drop the
        /// event when it no longer matches the app's current generation.
        generation: u64,
    },
    ContentLoadedPaged {
        content: ContentState,
        pagination: PaginationInfo,
        generation: u64,
    },
    PlaylistSelect {
        id: u64,
        name: Option<String>,
    },
    BreadcrumbSet {
        name: String,
        generation: u64,
    },
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
    /// Leave the in-queue filter: restore the unfiltered queue and keep the
    /// song selected while filtered as the playhead.
    QueueFilterExit,
    /// The filter keyword changed; recompute which queue indices still match.
    QueueFilterChanged,
    /// An endpoint (`pigma msg switch-list`) finished resolving in the
    /// background; the main thread applies it to the queue.
    EndpointResolved {
        endpoint: String,
        content: ContentState,
        playlist_id: Option<u64>,
    },
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
