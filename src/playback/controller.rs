use std::io::{Seek, SeekFrom};
use std::sync::mpsc;
use std::time::Duration;

use tokio::sync::mpsc as tokio_mpsc;

use super::player::{self, AudioInput, ControlCmd, SharedReader};
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
}

impl PlaybackHandle {
    pub fn new(event_tx: tokio_mpsc::UnboundedSender<Event>) -> Self {
        let (cmd_tx, mut cmd_rx) = tokio_mpsc::unbounded_channel::<PlayerCmd>();

        tokio::spawn(async move {
            let mut control_tx: Option<mpsc::Sender<ControlCmd>> = None;
            let mut shared_reader: Option<SharedReader> = None;
            let mut old_done_rx: Option<tokio::sync::oneshot::Receiver<()>> = None;

            while let Some(cmd) = cmd_rx.recv().await {
                match cmd {
                    PlayerCmd::Play { input, seek_time } => {
                        if let Some(ctrl) = control_tx.take() {
                            let _ = ctrl.send(ControlCmd::Stop);
                        }
                        // Await old player completion to ensure its decoder,
                        // sink, and underlying StreamDownload (HTTP connection,
                        // buffers) are fully dropped before starting a new one.
                        if let Some(rx) = old_done_rx.take() {
                            let _ = rx.await;
                        }

                        shared_reader = Some(input.clone());

                        let (ctrl_tx, ctrl_rx) = mpsc::channel();
                        control_tx = Some(ctrl_tx);

                        let tx = event_tx.clone();
                        let done_rx = player::run(input, seek_time, tx, ctrl_rx).await;
                        old_done_rx = Some(done_rx);
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
                    PlayerCmd::SeekTo(seek_time) => {
                        if let Some(ctrl) = control_tx.take() {
                            let _ = ctrl.send(ControlCmd::Stop);
                        }
                        // Await old player completion before reusing the reader.
                        if let Some(rx) = old_done_rx.take() {
                            let _ = rx.await;
                        }

                        if let Some(ref reader) = shared_reader {
                            if let Ok(mut locked) = reader.0.lock() {
                                let _ = locked.seek(SeekFrom::Start(0));
                            }
                            let input = reader.clone();
                            let (ctrl_tx, ctrl_rx) = mpsc::channel();
                            control_tx = Some(ctrl_tx);

                            let tx = event_tx.clone();
                            let done_rx = player::run(input, Some(seek_time), tx, ctrl_rx).await;
                            old_done_rx = Some(done_rx);
                        }
                    }
                    PlayerCmd::Stop => {
                        if let Some(ctrl) = control_tx.take() {
                            let _ = ctrl.send(ControlCmd::Stop);
                        }
                        // Await old player completion so resources are freed
                        // immediately rather than lingering in the background.
                        if let Some(rx) = old_done_rx.take() {
                            let _ = rx.await;
                        }
                        shared_reader = None;
                    }
                    PlayerCmd::SetVolume(v) => {
                        if let Some(ref ctrl) = control_tx {
                            let _ = ctrl.send(ControlCmd::SetVolume(v));
                        }
                    }
                }
            }
        });

        Self { cmd_tx }
    }

    pub fn request(&self, input: AudioInput, seek_time: Option<Duration>) {
        if self
            .cmd_tx
            .send(PlayerCmd::Play { input, seek_time })
            .is_err()
        {
            log::error!("PlaybackHandle: failed to send Play command (channel closed)");
        }
    }

    pub fn pause(&self) {
        if self.cmd_tx.send(PlayerCmd::Pause).is_err() {
            log::error!("PlaybackHandle: failed to send Pause command (channel closed)");
        }
    }

    pub fn resume(&self) {
        if self.cmd_tx.send(PlayerCmd::Resume).is_err() {
            log::error!("PlaybackHandle: failed to send Resume command (channel closed)");
        }
    }

    pub fn seek_to(&self, seek_time: Duration) {
        if self.cmd_tx.send(PlayerCmd::SeekTo(seek_time)).is_err() {
            log::error!("PlaybackHandle: failed to send SeekTo command (channel closed)");
        }
    }

    pub fn stop(&self) {
        if self.cmd_tx.send(PlayerCmd::Stop).is_err() {
            log::error!("PlaybackHandle: failed to send Stop command (channel closed)");
        }
    }

    pub fn set_volume(&self, volume: f32) {
        if self.cmd_tx.send(PlayerCmd::SetVolume(volume)).is_err() {
            log::error!("PlaybackHandle: failed to send SetVolume command (channel closed)");
        }
    }
}
