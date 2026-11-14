//! The `App` object itself: owned state and the small accessors/actions shared by
//! the TUI, headless mode and the IPC server.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, atomic::AtomicU64},
    time::Instant,
};

use ratatui::layout::Rect;
use ratatui_image::picker::Picker;
use reqwest::Client;

use super::search::SearchHost;
use crate::{
    config::{Config, ThemeRegistry},
    event::EventHandler,
    ipc::IpcState,
    playback::PlaybackEngine,
    service::ApiService,
    state::State,
};

pub struct App {
    pub(crate) config: Config,
    pub(crate) events: EventHandler,
    pub(crate) state: State,
    pub(crate) playback: PlaybackEngine,
    pub(crate) theme_registry: ThemeRegistry,
    pub(super) service: ApiService,
    pub(super) picker: Picker,
    /// Async HTTP client for cover downloads (honours the proxy config).
    pub(super) cover_http: Client,
    /// Thumbnails for the content cards, keyed by cover URL.
    pub(super) content_covers: super::cover::ContentCovers,
    /// Cover URLs already requested, so background fetches are not duplicated.
    pub(super) cover_requests: Arc<Mutex<HashSet<String>>>,
    /// Failed cover URLs wait before being retried.
    pub(super) cover_retry_after: Arc<Mutex<HashMap<String, Instant>>>,
    /// Search subsystem: sonar finder/registry, IPC result registry and engine.
    pub(super) search: SearchHost,
    /// Song ID set of the user's "我喜欢的音乐" playlist, sharing the same `Arc` as `PlaybackEngine`.
    pub(super) liked_ids: Arc<Mutex<HashSet<u64>>>,
    /// Playlists whose full tracks have already been merged into the playback queue for lazy pagination, avoiding repeated Enter presses refetching/truncating the queue.
    pub(super) queued_playlists: HashSet<u64>,
    /// IPC snapshot subsystem: status/queue snapshots, broadcast fan-out and
    /// their freshness bookkeeping (see `ipc::state`).
    pub(super) ipc: IpcState,
    /// Playerbar hit-test rect of the last draw, used by mouse input handling.
    pub(super) playerbar_area: Rect,
    /// Guards the startup splash branch: `login_status` is requested exactly
    /// once, so later events (IPC, mouse...) cannot spawn duplicate requests
    /// that would double-toast and re-fetch liked IDs.
    pub(super) login_status_requested: bool,
    /// True while the startup `login_status` request is in flight. The splash
    /// stays up until its result arrives, so the default tab is loaded with the
    /// UID already available instead of racing a "not logged in" error.
    pub(super) awaiting_login_status: bool,
    /// Invalidates late QR-login polling responses when the user starts a new
    /// login or the flow finishes.
    pub(super) qr_generation: Arc<AtomicU64>,
}

impl App {
    pub fn is_logged_in(&self) -> bool {
        self.service.client().is_logged_in()
    }

    /// Image protocol of the cover-rendering picker, so the UI layer can
    /// degrade gracefully on terminals without Sixel.
    pub fn picker_protocol(&self) -> ratatui_image::picker::ProtocolType {
        self.picker.protocol_type()
    }

    /// Terminal cell size, used to keep a square cover square in cells.
    pub(crate) fn picker_font_size(&self) -> ratatui_image::FontSize {
        self.picker.font_size()
    }

    /// Shared thumbnail map for the content cards.
    pub(crate) fn content_covers(&self) -> super::cover::ContentCovers {
        Arc::clone(&self.content_covers)
    }

    /// Playerbar hit-test rect recorded by the last draw (`ui::draw`).
    pub fn playerbar_area(&self) -> Rect {
        self.playerbar_area
    }

    /// Record the playerbar hit-test rect for the frame just drawn.
    pub(crate) fn set_playerbar_area(&mut self, area: Rect) {
        self.playerbar_area = area;
    }

    pub fn quit(&mut self) {
        self.save_equalizer();
        self.playback.save_session();
        self.service.client().flush_cookies();
        // Persist any coalesced content-cache write that has not drained yet;
        // the detached blocking writer may not run before the runtime exits.
        self.service.content_cache().flush_content_cache();
        self.state.running = false;
    }

    pub fn toast(&mut self, msg: String) {
        self.state.toast_msg = msg;
        self.state.toast_time = Some(Instant::now());
    }
}
