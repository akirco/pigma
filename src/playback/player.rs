use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::Source;
use rodio::mixer::Mixer;
use tokio::sync::mpsc;

#[cfg(target_os = "linux")]
use super::engine::mem_rss_kb;
#[cfg(target_os = "linux")]
use rodio::cpal::traits::{DeviceTrait, HostTrait};

use crate::event::{Event, PlaybackEvent};

pub trait AudioReader: Read + Seek + Send + Sync {}
impl<T: Read + Seek + Send + Sync> AudioReader for T {}

pub type AudioInput = SharedReader;

/// A reader wrapper that can be shared between the player and the seek controller.
/// On seek, the controller locks the shared reader, seeks to the new position,
/// then creates a new decoder + player from the seeked position.
#[derive(Clone)]
pub struct SharedReader(pub Arc<Mutex<Box<dyn AudioReader + 'static>>>);

impl std::fmt::Debug for SharedReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedReader").finish_non_exhaustive()
    }
}

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
    Switch(AudioInput, Option<Duration>),
    SeekTo(Duration),
    Pause,
    Resume,
    Stop,
    SetVolume(f32),
}

/// Run the audio player as a persistent blocking task. The task is spawned once
/// and stays alive across songs — new sources are fed via ControlCmd::Switch.
/// The `mixer` is obtained from a long-lived `MixerDeviceSink` managed by the
/// controller, so the audio device is opened only once across songs.
pub fn run(
    initial_reader: SharedReader,
    initial_seek_time: Option<Duration>,
    initial_volume: f32,
    event_tx: mpsc::UnboundedSender<Event>,
    control_rx: std::sync::mpsc::Receiver<ControlCmd>,
    mixer: Mixer,
) {
    tokio::task::spawn_blocking(move || {
        let progress_interval = Duration::from_millis(200);
        let mut reader = initial_reader;
        let mut total_duration: Option<Duration> = None;
        let mut seek_offset: Duration = Duration::default();
        let mut volume = initial_volume;

        macro_rules! start_playback {
            ($seek_time:expr) => {{
                let input = reader.clone();
                match rodio::Decoder::new(input) {
                    Ok(d) => {
                        total_duration = d.total_duration();
                        let seek_time = $seek_time;
                        let (source, offset): (Box<dyn Source<Item = f32> + Send>, Duration) =
                            if let Some(t) = seek_time {
                                let mut d = d;
                                if d.try_seek(t).is_err() {
                                    log::warn!("try_seek failed for {t:?}, starting from 0");
                                    (Box::new(d), Duration::default())
                                } else {
                                    (Box::new(d), t)
                                }
                            } else {
                                (Box::new(d), Duration::default())
                            };
                        seek_offset = offset;
                        let p = rodio::Player::connect_new(&mixer);
                        p.set_volume(volume);
                        p.append(source);
                        Some(p)
                    }
                    Err(e) => {
                        let _ = event_tx.send(PlaybackEvent::Error(format!("decode: {e}")).into());
                        None
                    }
                }
            }};
        }

        // Initial playback
        let mut player: Option<rodio::Player> = start_playback!(initial_seek_time);

        loop {
            match control_rx.recv_timeout(progress_interval) {
                Ok(cmd) => match cmd {
                    ControlCmd::Switch(input, seek_time) => {
                        if let Some(ref p) = player {
                            p.stop();
                        }
                        drop(player.take());
                        reader = input;
                        player = start_playback!(seek_time);
                    }
                    ControlCmd::SeekTo(seek_time) => {
                        if player.is_none() {
                            continue;
                        }
                        if let Some(ref p) = player {
                            p.stop();
                        }
                        drop(player.take());
                        let _ = reader.0.lock().map(|mut r| r.seek(SeekFrom::Start(0)));
                        player = start_playback!(Some(seek_time));
                    }
                    ControlCmd::Stop => {
                        if let Some(ref p) = player {
                            p.stop();
                        }
                        drop(player.take());
                        reader = SharedReader(Arc::new(Mutex::new(Box::new(std::io::empty()))));
                        total_duration = None;
                        seek_offset = Duration::default();
                        #[cfg(target_os = "linux")]
                        log::info!("[HEAP] after ControlCmd::Stop: {} kB", mem_rss_kb());
                    }
                    ControlCmd::Pause => {
                        if let Some(ref p) = player {
                            p.pause();
                        }
                    }
                    ControlCmd::Resume => {
                        if let Some(ref p) = player {
                            p.play();
                        }
                    }
                    ControlCmd::SetVolume(v) => {
                        volume = v;
                        if let Some(ref p) = player {
                            p.set_volume(v);
                        }
                    }
                },
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }

            if let Some(ref p) = player {
                if p.empty() && !p.is_paused() {
                    #[cfg(target_os = "linux")]
                    log::info!(
                        "[HEAP] song finished (playback complete): {} kB",
                        mem_rss_kb()
                    );
                    let _ = event_tx.send(PlaybackEvent::Finished.into());
                    p.stop();
                    drop(player.take());
                    total_duration = None;
                    seek_offset = Duration::default();
                    #[cfg(target_os = "linux")]
                    log::info!("[HEAP] after player drop on finish: {} kB", mem_rss_kb());
                    continue;
                }

                if !p.is_paused() {
                    let pos = p.get_pos() + seek_offset;
                    let _ = event_tx.send(
                        PlaybackEvent::Progress {
                            position: pos,
                            total: total_duration,
                        }
                        .into(),
                    );
                }
            }
        }
    });
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

/// Open the audio device while suppressing ALSA stderr noise (Linux only).
/// The returned sink should be kept alive across songs so the device is
/// opened only once.
pub fn create_sink() -> Result<rodio::MixerDeviceSink, rodio::DeviceSinkError> {
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

            for name in ["pipewire", "pulse"] {
                if let Some(device) = list
                    .iter()
                    .find(|d| d.id().map(|id| id.1.as_str() == name).unwrap_or(false))
                {
                    log::info!("opening audio device: {}", name);
                    if let Ok(sink) = rodio::DeviceSinkBuilder::from_device(device.clone())
                        .map(|b| b.with_buffer_size(rodio::cpal::BufferSize::Fixed(8192)))
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
