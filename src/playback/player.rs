use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::Source;
use tokio::sync::mpsc;

use crate::types::{AppEvent, Event};

pub trait AudioReader: Read + Seek + Send + Sync {}
impl<T: Read + Seek + Send + Sync> AudioReader for T {}

pub enum Input {
    Reader(SharedReader),
}

/// A reader wrapper that can be shared between the player and the seek controller.
/// On seek, the controller locks the shared reader, seeks to the new position,
/// then creates a new decoder + player from the seeked position.
#[derive(Clone)]
pub struct SharedReader(pub Arc<Mutex<Box<dyn AudioReader + 'static>>>);

impl Read for SharedReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().read(buf)
    }
}

impl Seek for SharedReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.0.lock().unwrap().seek(pos)
    }
}

#[derive(Debug, Clone)]
pub enum ControlCmd {
    Pause,
    Resume,
    Stop,
    SetVolume(f32),
}

pub async fn run(
    input: Input,
    seek_time: Option<Duration>,
    event_tx: mpsc::UnboundedSender<Event>,
    control_rx: std::sync::mpsc::Receiver<ControlCmd>,
) {
    let Input::Reader(reader) = input;

    let decoder = match tokio::task::spawn_blocking(move || rodio::Decoder::new(reader)).await {
        Ok(Ok(d)) => d,
        Err(e) => {
            if event_tx
                .send(Event::App(AppEvent::PlaybackError(format!("decode: {e}"))))
                .is_err()
            {
                log::error!("Failed to send PlaybackError: receiver dropped");
            }
            return;
        }
        Ok(Err(e)) => {
            if event_tx
                .send(Event::App(AppEvent::PlaybackError(format!("decode: {e}"))))
                .is_err()
            {
                log::error!("Failed to send PlaybackError: receiver dropped");
            }
            return;
        }
    };

    let total_duration = decoder.total_duration();
    let (source, seek_offset): (Box<dyn Source<Item = f32> + Send>, Duration) =
        if let Some(t) = seek_time {
            let mut d = decoder;
            if d.try_seek(t).is_err() {
                log::warn!("try_seek failed for {t:?}, starting from 0");
                (Box::new(d), Duration::default())
            } else {
                (Box::new(d), t)
            }
        } else {
            (Box::new(decoder), Duration::default())
        };

    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();

    let progress_interval = Duration::from_millis(200);

    tokio::task::spawn_blocking(move || {
        let mut sink = match rodio::DeviceSinkBuilder::open_default_sink() {
            Ok(sink) => sink,
            Err(e) => {
                if event_tx
                    .send(Event::App(AppEvent::PlaybackError(format!(
                        "open audio device: {e}"
                    ))))
                    .is_err()
                {
                    log::error!("Failed to send PlaybackError: receiver dropped");
                }
                return;
            }
        };
        sink.log_on_drop(false);
        let player = rodio::Player::connect_new(sink.mixer());
        player.append(source);

        let _ = ready_tx.send(());

        loop {
            match control_rx.recv_timeout(progress_interval) {
                Ok(cmd) => match cmd {
                    ControlCmd::Pause => {
                        player.pause();
                        continue;
                    }
                    ControlCmd::Resume => {
                        player.play();
                        continue;
                    }
                    ControlCmd::Stop => {
                        player.stop();
                        return;
                    }
                    ControlCmd::SetVolume(v) => {
                        player.set_volume(v);
                        continue;
                    }
                },
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }

            if player.empty() && !player.is_paused() {
                if event_tx
                    .send(Event::App(AppEvent::PlaybackFinished))
                    .is_err()
                {
                    log::error!("Failed to send PlaybackFinished: receiver dropped");
                }
                break;
            }

            if !player.is_paused() {
                let pos = player.get_pos() + seek_offset;
                if event_tx
                    .send(Event::App(AppEvent::PlaybackProgress {
                        position: pos,
                        total: total_duration,
                    }))
                    .is_err()
                {
                    log::error!("Failed to send PlaybackProgress: receiver dropped");
                }
            }
        }
    });

    let _ = ready_rx.await;
}
