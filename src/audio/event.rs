//! Events emitted by the audio player task.

use std::time::Duration;

/// Player-side events; the app maps these to its own `PlaybackEvent` channel.
#[derive(Debug, Clone)]
pub(crate) enum PlayerEvent {
    Progress {
        position: Duration,
        total: Option<Duration>,
    },
    Finished,
    Error(String),
    /// Output device failure (`Some`) or recovery (`None`).
    DeviceError(Option<String>),
}
