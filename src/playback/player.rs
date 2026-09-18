use std::{
    io::{Read, Seek, SeekFrom},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use rodio::{
    Source,
    cpal::traits::{DeviceTrait, HostTrait},
};
use tokio::sync::mpsc;

use crate::event::{Event, PlaybackEvent};
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use crate::utils::process::mem_rss_kb;

/// Progress ticks (~200ms each) the position may stay frozen while playing
/// before we assume the audio stream is dead (e.g. Bluetooth device removed on
/// macOS sometimes dies without firing the cpal error callback) and rebuild it.
/// Network stalls also freeze the position, but those are accompanied by recent
/// buffer-underrun errors, so they are excluded via [`UNDERRUN_FRESH_MS`].
const STALL_TICKS: u32 = 25;
/// A buffer-underrun error younger than this (ms) means the freeze is a data
/// feed problem (slow network), not a dead device — suppress the watchdog.
const UNDERRUN_FRESH_MS: u64 = 10_000;
/// Poll the system default output device name every N progress ticks (~1s).
const FOLLOW_POLL_TICKS: u32 = 5;
/// Rebuild to follow the default device only after the change is stable for
/// this many consecutive polls (~2s), avoiding flaps during BT handshakes.
const FOLLOW_DIFF_THRESHOLD: u32 = 2;

/// Shared between the stream error callback and the player loop: set when a
/// fatal stream error indicates the output device/stream must be rebuilt.
struct DeviceHealth {
    /// Fatal stream error seen — rebuild the sink as soon as possible.
    device_lost: AtomicBool,
    /// Milliseconds (relative to `epoch`) of the last underrun, used to tell
    /// network stalls apart from device death. `u64::MAX` = none.
    last_underrun_ms: AtomicU64,
    /// Shared instant the millisecond timestamps are relative to.
    epoch: Instant,
}

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
///
/// The audio device sink is owned by this task (created lazily on first
/// playback) so it can be rebuilt when the output device goes away: Bluetooth
/// headsets being disconnected/reconnected invalidate the old stream, and the
/// only fix is to re-enumerate devices and open the current default again.
pub(super) fn run(
    initial_reader: SharedReader,
    initial_seek_time: Option<Duration>,
    initial_volume: f32,
    event_tx: mpsc::UnboundedSender<Event>,
    control_rx: std::sync::mpsc::Receiver<ControlCmd>,
) {
    tokio::task::spawn_blocking(move || {
        let progress_interval = Duration::from_millis(200);
        let mut reader = initial_reader;
        let mut total_duration: Option<Duration> = None;
        let mut seek_offset: Duration = Duration::default();
        let mut volume = initial_volume;
        // Owned by this task; recreated whenever the output device is lost.
        let mut sink: Option<rodio::MixerDeviceSink> = None;
        let health = Arc::new(DeviceHealth {
            device_lost: AtomicBool::new(false),
            last_underrun_ms: AtomicU64::new(u64::MAX),
            epoch: Instant::now(),
        });
        // Watchdog state: position frozen while playing => dead stream.
        let mut last_pos = Duration::default();
        let mut stall_ticks: u32 = 0;
        let mut player: Option<rodio::Player> = None;
        // Default-device following state (see FOLLOW_POLL_TICKS).
        let mut sink_dev_id: Option<String> = None;
        let mut follow_poll: u32 = 0;
        let mut follow_diff: u32 = 0;

        macro_rules! ensure_sink {
            () => {{
                if sink.is_none()
                    && let Ok(mut s) = create_sink(health.clone())
                {
                    s.log_on_drop(false);
                    sink = Some(s);
                    sink_dev_id = current_default_id();
                }
                sink.is_some()
            }};
        }

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
                        let p = rodio::Player::connect_new(
                            &sink.as_ref().expect("sink ensured").mixer().clone(),
                        );
                        p.set_volume(volume);
                        p.append(source);
                        last_pos = Duration::default();
                        stall_ticks = 0;
                        Some(p)
                    }
                    Err(e) => {
                        let _ = event_tx.send(PlaybackEvent::Error(format!("decode: {e}")).into());
                        None
                    }
                }
            }};
        }

        // Drop the current player/sink and reopen the (current default) audio
        // device. If `resume_at`/`was_paused` are provided, playback continues
        // from there on the fresh stream.
        macro_rules! rebuild_sink {
            ($resume_at:expr, $was_paused:expr) => {{
                let resume_at: Option<Duration> = $resume_at;
                let was_paused: bool = $was_paused;
                drop(player.take());
                drop(sink.take());
                match create_sink(health.clone()) {
                    Ok(mut s) => {
                        s.log_on_drop(false);
                        sink = Some(s);
                        sink_dev_id = current_default_id();
                        health.device_lost.store(false, Ordering::Relaxed);
                        stall_ticks = 0;
                        log::info!(
                            "音频设备已重建{}",
                            resume_at
                                .map(|t| format!("，从 {t:?} 继续"))
                                .unwrap_or_default()
                        );
                        if let Some(t) = resume_at {
                            let _ = reader.0.lock().map(|mut r| r.seek(SeekFrom::Start(0)));
                            player = start_playback!(Some(t));
                        }
                        if was_paused && let Some(ref p) = player {
                            p.pause();
                        }
                    }
                    Err(e) => {
                        // Keep device_lost set so the loop retries next tick
                        // (e.g. device still reconnecting).
                        log::warn!("重建音频设备失败，稍后重试: {e}");
                    }
                }
            }};
        }

        // Lazy-init the audio sink before first playback.
        if ensure_sink!() {
            // Initial playback.
            player = start_playback!(initial_seek_time);
        }

        loop {
            match control_rx.recv_timeout(progress_interval) {
                Ok(cmd) => match cmd {
                    ControlCmd::Switch(input, seek_time) => {
                        if let Some(ref p) = player {
                            p.stop();
                        }
                        drop(player.take());
                        reader = input;
                        if ensure_sink!() {
                            player = start_playback!(seek_time);
                        } else {
                            player = None;
                        }
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
                        if !ensure_sink!() {
                            player = None;
                            continue;
                        }
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
                        #[cfg(all(target_os = "linux", target_env = "gnu"))]
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

            // Fatal stream error (device removed etc.) — rebuild and resume at
            // the last known position.
            if health.device_lost.load(Ordering::Relaxed) {
                let resume_at = player.as_ref().map(|p| p.get_pos() + seek_offset);
                let was_paused = player.as_ref().is_some_and(|p| p.is_paused());
                rebuild_sink!(resume_at, was_paused);
                continue;
            }

            // Follow system default output device changes: after a Bluetooth
            // headset reconnects the OS makes it default again, and without
            // this the stream would keep playing on the fallback (speakers)
            // forever. Rebuild only when the change is stable, and keep
            // playing at the current position on the new device.
            follow_poll += 1;
            if follow_poll >= FOLLOW_POLL_TICKS {
                follow_poll = 0;
                if sink.is_some()
                    && let Some(id) = current_default_id()
                {
                    if Some(&id) != sink_dev_id.as_ref() {
                        follow_diff += 1;
                    } else {
                        follow_diff = 0;
                    }
                    if follow_diff >= FOLLOW_DIFF_THRESHOLD {
                        log::info!(
                            "默认输出设备变化（{} -> {id}），切换音频设备",
                            sink_dev_id.as_deref().unwrap_or("?")
                        );
                        follow_diff = 0;
                        let resume_at = player.as_ref().map(|p| p.get_pos() + seek_offset);
                        let was_paused = player.as_ref().is_some_and(|p| p.is_paused());
                        rebuild_sink!(resume_at, was_paused);
                        continue;
                    }
                }
            }

            if let Some(ref p) = player {
                if p.empty() && !p.is_paused() {
                    #[cfg(all(target_os = "linux", target_env = "gnu"))]
                    log::info!(
                        "[HEAP] song finished (playback complete): {} kB",
                        mem_rss_kb()
                    );
                    let _ = event_tx.send(PlaybackEvent::Finished.into());
                    p.stop();
                    drop(player.take());
                    total_duration = None;
                    seek_offset = Duration::default();
                    #[cfg(all(target_os = "linux", target_env = "gnu"))]
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

                    // Watchdog: position frozen for STALL_TICKS while playing
                    // means the stream died silently (macOS sometimes removes
                    // the output device without firing the error callback).
                    // Skip when underruns were seen recently — that freeze is
                    // just the network lagging behind.
                    if !p.empty() {
                        let raw_pos = p.get_pos();
                        if raw_pos == last_pos {
                            stall_ticks += 1;
                        } else {
                            stall_ticks = 0;
                            last_pos = raw_pos;
                        }
                        let underrun_ms = health.last_underrun_ms.load(Ordering::Relaxed);
                        let now_ms = underrun_now(&health.epoch);
                        let recent_underrun =
                            now_ms.saturating_sub(underrun_ms) < UNDERRUN_FRESH_MS;
                        if stall_ticks >= STALL_TICKS && !recent_underrun {
                            log::warn!(
                                "播放位置持续冻结且无网络欠载，疑似输出设备失效，重建音频流"
                            );
                            rebuild_sink!(Some(pos), false);
                        }
                    }
                }
            }
        }
    });
}

/// Milliseconds since `epoch`, matching how `DeviceHealth.last_underrun_ms`
/// timestamps are written by the stream error callback.
fn underrun_now(epoch: &std::time::Instant) -> u64 {
    epoch.elapsed().as_millis() as u64
}

/// Identifier of the current system default output device, used to detect when
/// the OS (e.g. a Bluetooth reconnection) or the user changes it. `None` when
/// no default exists right now or the id cannot be queried — treated as "no
/// change" so transient states never trigger a rebuild.
fn current_default_id() -> Option<String> {
    rodio::cpal::default_host()
        .default_output_device()
        .and_then(|d| d.id().ok())
        .map(|id| id.to_string())
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

/// rodio's default error callback prints `eprintln!("audio stream error: {err}")` straight to
/// stderr, which pollutes the TUI render in crossterm raw mode (typically triggered by YouTube
/// streams downloading slower than playback, causing audio device buffer underruns). We replace
/// it with a callback that logs and updates [`DeviceHealth`]: transient underruns/overruns (which
/// rodio recovers from automatically) are only recorded; fatal errors (device removed,
/// stream invalidated, backend-specific device loss on WASAPI/CoreAudio/ALSA) set the
/// `device_lost` flag so the player loop rebuilds the sink.
fn stream_error_callback(
    health: Arc<DeviceHealth>,
) -> impl FnMut(rodio::cpal::StreamError) + Send + 'static + Clone {
    move |err| match err {
        rodio::cpal::StreamError::BufferUnderrun => {
            log::warn!("音频流缓冲下溢/溢出（网络可能跟不上播放速度），rodio 会自动恢复");
            health
                .last_underrun_ms
                .store(underrun_now(&health.epoch), Ordering::Relaxed);
        }
        // cpal documents both variants as "the stream must be rebuilt".
        rodio::cpal::StreamError::DeviceNotAvailable
        | rodio::cpal::StreamError::StreamInvalidated => {
            log::warn!("输出设备不可用（{err}），准备重建音频流");
            health.device_lost.store(true, Ordering::Relaxed);
        }
        other @ rodio::cpal::StreamError::BackendSpecific { .. } => {
            // Platform device-removal surfaces here: WASAPI DEVICE_INVALIDATED
            // (Windows), CoreAudio AudioUnit errors (macOS), ALSA snd_pcm
            // failures (Linux). Treat as fatal for the current stream.
            log::warn!("音频流后端错误: {other}");
            health.device_lost.store(true, Ordering::Relaxed);
        }
    }
}

/// Open the audio device while suppressing ALSA stderr noise (Linux only).
/// The returned sink should be kept alive across songs so the device is
/// opened only once — until it is lost and must be rebuilt.
fn create_sink(
    health: Arc<DeviceHealth>,
) -> Result<rodio::MixerDeviceSink, rodio::DeviceSinkError> {
    #[cfg(target_os = "linux")]
    {
        let _ = StderrGuard::new().map_err(|e| {
            log::warn!("Failed to create stderr guard: {e}");
            rodio::DeviceSinkError::NoDevice
        })?;
        open_sink_impl(health)
    }
    #[cfg(not(target_os = "linux"))]
    {
        open_sink_impl(health)
    }
}

/// Prefer PipeWire/PulseAudio ALSA devices so system volume/mute works.
/// Falls back to the default ALSA device if not available.
fn open_sink_impl(
    health: Arc<DeviceHealth>,
) -> Result<rodio::MixerDeviceSink, rodio::DeviceSinkError> {
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
                        .map(|b| b.with_error_callback(stream_error_callback(health.clone())))
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
    rodio::DeviceSinkBuilder::from_default_device()
        .map(|b| b.with_error_callback(stream_error_callback(health)))
        .and_then(|b| b.open_sink_or_fallback())
}
