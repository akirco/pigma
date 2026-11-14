//! Output-device plumbing: cpal sink creation, device-health flags and the
//! stream error callback that turns transport failures into rebuild requests.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Instant,
};

use rodio::cpal::traits::{DeviceTrait, HostTrait};

/// Shared between the stream error callback and the player loop: set when a
/// fatal stream error indicates the output device/stream must be rebuilt.
pub(super) struct DeviceHealth {
    /// Fatal stream error seen — rebuild the sink as soon as possible.
    pub(super) device_lost: AtomicBool,
    /// Milliseconds (relative to `epoch`) of the last underrun, used to tell
    /// network stalls apart from device death. `u64::MAX` = none.
    pub(super) last_underrun_ms: AtomicU64,
    /// Shared instant the millisecond timestamps are relative to.
    pub(super) epoch: Instant,
}

/// Milliseconds since `epoch`, matching how `DeviceHealth.last_underrun_ms`
/// timestamps are written by the stream error callback.
pub(super) fn underrun_now(epoch: &std::time::Instant) -> u64 {
    epoch.elapsed().as_millis() as u64
}

/// Identifier of the current system default output device, used to detect when
/// the OS (e.g. a Bluetooth reconnection) or the user changes it. `None` when
/// no default exists right now or the id cannot be queried — treated as "no
/// change" so transient states never trigger a rebuild.
pub(super) fn current_default_id() -> Option<String> {
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
pub(super) fn create_sink(
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
