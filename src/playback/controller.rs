use std::sync::mpsc;
use std::time::Duration;

use rodio::MixerDeviceSink;
use tokio::sync::mpsc as tokio_mpsc;

use super::player::{self, AudioInput, ControlCmd};
use crate::event::{Event, PlaybackEvent};

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
}

impl PlaybackHandle {
    pub(super) fn new(event_tx: tokio_mpsc::UnboundedSender<Event>) -> Self {
        let (cmd_tx, mut cmd_rx) = tokio_mpsc::unbounded_channel::<PlayerCmd>();

        tokio::spawn(async move {
            let mut control_tx: Option<mpsc::Sender<ControlCmd>> = None;
            let mut last_volume: f32 = 1.0;
            // Create the audio sink once and reuse it across songs,
            // so the audio device is opened only on the first playback.
            let mut sink: Option<MixerDeviceSink> = None;

            while let Some(cmd) = cmd_rx.recv().await {
                match cmd {
                    PlayerCmd::Play { input, seek_time } => {
                        // Lazy-init the audio sink on first playback.
                        if sink.is_none() {
                            match player::create_sink() {
                                Ok(mut s) => {
                                    s.log_on_drop(false);
                                    sink = Some(s);
                                }
                                Err(e) => {
                                    let _ = event_tx.send(
                                        PlaybackEvent::Error(format!("open audio device: {e}"))
                                            .into(),
                                    );
                                    continue;
                                }
                            }
                        }

                        if let Some(ref ctrl) = control_tx {
                            // Player task already running — switch to new source.
                            let _ = ctrl.send(ControlCmd::Switch(input, seek_time));
                        } else {
                            // First playback: spawn the persistent player task.
                            let (ctrl_tx, ctrl_rx) = mpsc::channel();
                            control_tx = Some(ctrl_tx);
                            let tx = event_tx.clone();
                            let mixer = sink.as_ref().expect("sink just created").mixer().clone();
                            player::run(input, seek_time, last_volume, tx, ctrl_rx, mixer);
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
                        // Keep sink alive so next play reuses the device.
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
}
