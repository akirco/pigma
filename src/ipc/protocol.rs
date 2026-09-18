//! JSON wire protocol shared by the CLI client and the TUI server.

use serde::{Deserialize, Serialize};

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
    /// [`SearchEntry`](crate::ipc::SearchEntry); results are registered
    /// in-process so a returned id can later be played with
    /// `pigma msg play <id>`.
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
            MsgAction::SwitchList { endpoint, playlist } => {
                IpcEvent::SwitchList { endpoint, playlist }
            }
        }
    }
}
