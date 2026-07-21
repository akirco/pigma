use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::Source;
use rodio::cpal::traits::{DeviceTrait, HostTrait};
use tokio::sync::mpsc;

use crate::event::{AppEvent, Event};

pub trait AudioReader: Read + Seek + Send + Sync {}
impl<T: Read + Seek + Send + Sync> AudioReader for T {}

pub type AudioInput = SharedReader;

/// A reader wrapper that can be shared between the player and the seek controller.
/// On seek, the controller locks the shared reader, seeks to the new position,
/// then creates a new decoder + player from the seeked position.
#[derive(Clone)]
pub struct SharedReader(pub Arc<Mutex<Box<dyn AudioReader + 'static>>>);

impl Read for SharedReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .map_err(|e| std::io::Error::other(format!("mutex poisoned: {e}")))?
            .read(buf)
    }
}

impl Seek for SharedReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.0
            .lock()
            .map_err(|e| std::io::Error::other(format!("mutex poisoned: {e}")))?
            .seek(pos)
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
    reader: SharedReader,
    seek_time: Option<Duration>,
    event_tx: mpsc::UnboundedSender<Event>,
    control_rx: std::sync::mpsc::Receiver<ControlCmd>,
) {
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
        let mut sink = match open_sink_silent() {
            Ok(s) => s,
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

/// RAII guard that redirects stderr to /dev/null while alive, restoring it on drop.
/// Used to suppress ALSA noise during audio device initialization.
#[cfg(target_os = "linux")]
struct StderrGuard {
    saved_fd: std::os::fd::RawFd,
}

#[cfg(target_os = "linux")]
impl StderrGuard {
    fn new() -> std::io::Result<Self> {
        use std::os::fd::AsRawFd;

        let stderr_fd = 2;
        let saved = unsafe { libc::dup(stderr_fd) };
        if saved < 0 {
            return Err(std::io::Error::last_os_error());
        }

        let dev_null = std::fs::File::open("/dev/null")?;
        let ret = unsafe { libc::dup2(dev_null.as_raw_fd(), stderr_fd) };
        if ret < 0 {
            unsafe { libc::close(saved) };
            return Err(std::io::Error::last_os_error());
        }

        Ok(Self { saved_fd: saved })
    }
}

#[cfg(target_os = "linux")]
impl Drop for StderrGuard {
    fn drop(&mut self) {
        let stderr_fd = 2;
        unsafe {
            libc::dup2(self.saved_fd, stderr_fd);
            libc::close(self.saved_fd);
        }
    }
}

/// Open rodio default sink while suppressing ALSA stderr noise.
fn open_sink_silent() -> Result<rodio::MixerDeviceSink, rodio::DeviceSinkError> {
    #[cfg(target_os = "linux")]
    {
        let _ = StderrGuard::new().map_err(|e| {
            log::warn!("Failed to create stderr guard: {e}");
            rodio::DeviceSinkError::NoDevice
        })?;
        open_sink_impl()
    }
    #[cfg(not(target_os = "linux"))]
    {
        open_sink_impl()
    }
}

/// Prefer PipeWire/PulseAudio ALSA devices so system volume/mute works.
/// Falls back to the default ALSA device if not available.
fn open_sink_impl() -> Result<rodio::MixerDeviceSink, rodio::DeviceSinkError> {
    #[cfg(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly",
    ))]
    {
        let host = rodio::cpal::default_host();
        if let Ok(devices) = host.devices() {
            let list: Vec<_> = devices.collect();

            // ! linux debug
            // for d in &list {
            //     if let Ok(id) = d.id() {
            //         log::debug!("cpal device: {}", id.1);
            //     }
            // }

            for name in ["pipewire", "pulse"] {
                if let Some(device) = list
                    .iter()
                    .find(|d| d.id().map(|id| id.1.as_str() == name).unwrap_or(false))
                {
                    log::info!("opening audio device: {}", name);
                    if let Ok(sink) = rodio::DeviceSinkBuilder::from_device(device.clone())
                        .and_then(|b| b.open_sink_or_fallback())
                    {
                        return Ok(sink);
                    }
                    log::warn!("failed to open {}, falling back", name);
                } else {
                    log::debug!("cpal device not found: {}", name);
                }
            }
        }
    }

    log::debug!("falling back to default audio device");
    rodio::DeviceSinkBuilder::open_default_sink()
}
