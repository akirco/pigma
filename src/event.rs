use std::time::Duration;

use crossterm::event::Event as CrosstermEvent;
use color_eyre::eyre::OptionExt;
use futures::{FutureExt, StreamExt};
use ncm_api::{LoginInfo, SongInfo};
use tokio::sync::mpsc;

use crate::playback::types::LyricLine;
use crate::state::{CommandAction, Page, SplashLogEntry};
use crate::state::ContentState;

#[derive(Clone, Debug)]
pub enum Event {
    Crossterm(CrosstermEvent),
    App(AppEvent),
}

#[derive(Clone, Debug)]
pub enum AppEvent {
    Quit,
    SplashTick {
        progress: f64,
        log: Option<SplashLogEntry>,
    },
    Login,
    LoginSuccess(LoginInfo),
    LoginError(String),
    CaptchaSent,
    QRCreated {
        url: String,
        key: String,
    },
    QRStatus(String),
    NavSelect(String),
    ContentLoaded(ContentState),
    PlaylistSelect { id: u64, name: Option<String> },
    BreadcrumbSet(String),
    SongPlay(u64),
    PlaybackStarted,
    PlaybackProgress {
        position: Duration,
        total: Option<Duration>,
    },
    PlaybackFinished,
    PlaybackError(String),
    LyricsLoaded {
        song_id: u64,
        lyrics: Vec<LyricLine>,
        translated_lyrics: Vec<LyricLine>,
    },
    HeartbeatSong(SongInfo),
    HeartbeatFallback,
    SearchSong(String),
    LocalMusicLoaded(Vec<SongInfo>),
    SetOffline,
    Navigate(Page),
    CommandPanel(CommandPanelAction),
    SearchActivated,
    SearchDeactivated,
    ToggleBordered,
    ExecuteCommand(CommandAction),
    ContentRestore,
    CellAction(usize, usize),
}

#[derive(Clone, Debug)]
pub enum CommandPanelAction {
    Open,
    Close,
    Previous,
    Next,
    Select,
}

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

    pub fn send(&mut self, app_event: AppEvent) {
        if let Err(e) = self.sender.send(Event::App(app_event)) {
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
