use std::{sync::mpsc, time::Duration};

use tokio::sync::mpsc as tokio_mpsc;

use super::{
    player::{self, AudioInput, ControlCmd},
    stretch::SpeedMode,
};
use crate::event::Event;

#[derive(Clone)]
pub struct PlaybackHandle {
    cmd_tx: tokio_mpsc::UnboundedSender<PlayerCmd>,
}

enum PlayerCmd {
    Play {
        input: AudioInput,
        seek_time: Option<Duration>,
    },
    Pause,
    Resume,
    SeekTo(Duration),
    Stop,
    SetVolume(f32),
    SetSpeed(f32),
    SetSpeedMode(SpeedMode),
}

impl PlaybackHandle {
    pub(super) fn new(event_tx: tokio_mpsc::UnboundedSender<Event>) -> Self {
        let (cmd_tx, mut cmd_rx) = tokio_mpsc::unbounded_channel::<PlayerCmd>();

        tokio::spawn(async move {
            let mut control_tx: Option<mpsc::Sender<ControlCmd>> = None;
            let mut last_volume: f32 = 1.0;
            let mut last_speed: f32 = 1.0;
            let mut last_speed_mode: SpeedMode = SpeedMode::default();

            while let Some(cmd) = cmd_rx.recv().await {
                match cmd {
                    PlayerCmd::Play { input, seek_time } => {
                        if let Some(ref ctrl) = control_tx {
                            // Player task already running — switch to new source.
                            let _ = ctrl.send(ControlCmd::Switch(input, seek_time));
                        } else {
                            // First playback: spawn the persistent player task.
                            // It owns the audio device sink for its whole
                            // lifetime and rebuilds it if the output device is
                            // lost (e.g. Bluetooth disconnect/reconnect).
                            let (ctrl_tx, ctrl_rx) = mpsc::channel();
                            control_tx = Some(ctrl_tx);
                            let tx = event_tx.clone();
                            player::run(
                                input,
                                seek_time,
                                last_volume,
                                last_speed,
                                last_speed_mode,
                                tx,
                                ctrl_rx,
                            );
                        }
                    }
                    PlayerCmd::SeekTo(seek_time) => {
                        if let Some(ref ctrl) = control_tx {
                            let _ = ctrl.send(ControlCmd::SeekTo(seek_time));
                        }
                    }
                    PlayerCmd::Stop => {
                        if let Some(ref ctrl) = control_tx {
                            // Player task stays alive, just clears decoder.
                            let _ = ctrl.send(ControlCmd::Stop);
                        }
                        // Keep the player task alive so next play reuses it
                        // (and its audio device).
                    }
                    PlayerCmd::Pause => {
                        if let Some(ref ctrl) = control_tx {
                            let _ = ctrl.send(ControlCmd::Pause);
                        }
                    }
                    PlayerCmd::Resume => {
                        if let Some(ref ctrl) = control_tx {
                            let _ = ctrl.send(ControlCmd::Resume);
                        }
                    }
                    PlayerCmd::SetVolume(v) => {
                        last_volume = v;
                        if let Some(ref ctrl) = control_tx {
                            let _ = ctrl.send(ControlCmd::SetVolume(v));
                        }
                    }
                    PlayerCmd::SetSpeed(v) => {
                        last_speed = v;
                        if let Some(ref ctrl) = control_tx {
                            let _ = ctrl.send(ControlCmd::SetSpeed(v));
                        }
                    }
                    PlayerCmd::SetSpeedMode(mode) => {
                        last_speed_mode = mode;
                        if let Some(ref ctrl) = control_tx {
                            let _ = ctrl.send(ControlCmd::SetSpeedMode(mode));
                        }
                    }
                }
            }
        });

        Self { cmd_tx }
    }

    pub(super) fn request(&self, input: AudioInput, seek_time: Option<Duration>) {
        if self
            .cmd_tx
            .send(PlayerCmd::Play { input, seek_time })
            .is_err()
        {
            log::error!("PlaybackHandle: failed to send Play command (channel closed)");
        }
    }

    pub(super) fn pause(&self) {
        if self.cmd_tx.send(PlayerCmd::Pause).is_err() {
            log::error!("PlaybackHandle: failed to send Pause command (channel closed)");
        }
    }

    pub(super) fn resume(&self) {
        if self.cmd_tx.send(PlayerCmd::Resume).is_err() {
            log::error!("PlaybackHandle: failed to send Resume command (channel closed)");
        }
    }

    pub(super) fn seek_to(&self, seek_time: Duration) {
        if self.cmd_tx.send(PlayerCmd::SeekTo(seek_time)).is_err() {
            log::error!("PlaybackHandle: failed to send SeekTo command (channel closed)");
        }
    }

    pub(super) fn stop(&self) {
        if self.cmd_tx.send(PlayerCmd::Stop).is_err() {
            log::error!("PlaybackHandle: failed to send Stop command (channel closed)");
        }
    }

    pub(super) fn set_volume(&self, volume: f32) {
        if self.cmd_tx.send(PlayerCmd::SetVolume(volume)).is_err() {
            log::error!("PlaybackHandle: failed to send SetVolume command (channel closed)");
        }
    }

    pub(super) fn set_speed(&self, speed: f32) {
        if self.cmd_tx.send(PlayerCmd::SetSpeed(speed)).is_err() {
            log::error!("PlaybackHandle: failed to send SetSpeed command (channel closed)");
        }
    }

    pub(super) fn set_speed_mode(&self, mode: SpeedMode) {
        if self.cmd_tx.send(PlayerCmd::SetSpeedMode(mode)).is_err() {
            log::error!("PlaybackHandle: failed to send SetSpeedMode command (channel closed)");
        }
    }
}
