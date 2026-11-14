//! Persistent player loop: owns per-song state, runs progress/watchdog ticks
//! and follows changes to the system default output device.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use tokio::sync::mpsc;

use super::{
    super::{
        equalizer::EqHandle,
        stretch::{SpeedHandle, SpeedMode},
    },
    device::{DeviceHealth, current_default_id, underrun_now},
    reader::{AudioInput, SharedReader},
};
use crate::event::{Event, PlaybackEvent};

pub(crate) enum ControlCmd {
    Switch(AudioInput, Option<Duration>),
    SeekTo(Duration),
    Pause,
    Resume,
    Stop,
    SetVolume(f32),
    SetSpeed(f32),
    /// Keep the pitch (WSOLA stretch) or shift it (resampling).
    SetSpeedMode(SpeedMode),
}

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

/// Progress tick: how long `recv_timeout` waits before running the per-tick
/// progress/watchdog/device-follow checks.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);

/// Run the audio player as a persistent blocking task. The task is spawned once
/// and stays alive across songs — new sources are fed via `ControlCmd::Switch`.
///
/// The audio device sink is owned by the task (created lazily on first
/// playback) so it can be rebuilt when the output device goes away: Bluetooth
/// headsets being disconnected/reconnected invalidate the old stream, and the
/// only fix is to re-enumerate devices and open the current default again.
// The audio loop needs every engine handle and initial playback parameter.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    initial_reader: SharedReader,
    initial_seek_time: Option<Duration>,
    initial_volume: f32,
    initial_speed: f32,
    initial_speed_mode: SpeedMode,
    eq: EqHandle,
    event_tx: mpsc::UnboundedSender<Event>,
    control_rx: std::sync::mpsc::Receiver<ControlCmd>,
) {
    tokio::task::spawn_blocking(move || {
        let task = PlayerTask::new(
            initial_reader,
            initial_volume,
            initial_speed,
            initial_speed_mode,
            eq,
            event_tx,
        );
        task.run(control_rx, initial_seek_time);
    });
}

/// Mutable state owned by the persistent player thread. Keeping it in one place
/// lets the control/tick/device logic live in small methods instead of a single
/// 300-line loop body.
pub(super) struct PlayerTask {
    pub(super) reader: SharedReader,
    pub(super) total_duration: Option<Duration>,
    pub(super) seek_offset: Duration,
    pub(super) volume: f32,
    pub(super) speed: SpeedHandle,
    pub(super) eq: EqHandle,
    pub(super) sink: Option<rodio::MixerDeviceSink>,
    pub(super) health: Arc<DeviceHealth>,
    /// Watchdog state: position frozen while playing => dead stream.
    pub(super) last_pos: Duration,
    pub(super) stall_ticks: u32,
    pub(super) player: Option<rodio::Player>,
    /// Default-device following state (see [`FOLLOW_POLL_TICKS`]).
    pub(super) sink_dev_id: Option<String>,
    pub(super) follow_poll: u32,
    pub(super) follow_diff: u32,
    /// Whether a device failure was reported to the UI; throttles the 200ms
    /// rebuild retries so a missing device logs/toasts once, not five times a
    /// second.
    pub(super) device_error_reported: bool,
    /// Playback to restore once a sink can be opened. Kept across failed
    /// rebuild retries so a device that comes back still resumes the current
    /// song at the same position.
    pub(super) pending_resume: Option<(Option<Duration>, bool)>,
    pub(super) event_tx: mpsc::UnboundedSender<Event>,
}

impl PlayerTask {
    fn new(
        reader: SharedReader,
        volume: f32,
        initial_speed: f32,
        initial_speed_mode: SpeedMode,
        eq: EqHandle,
        event_tx: mpsc::UnboundedSender<Event>,
    ) -> Self {
        // Playback speed. rodio's `Player::set_speed` resamples (raising the
        // source's sample rate), which shifts the pitch by the same factor —
        // unusable for music. The speed therefore lives in a pitch-preserving
        // WSOLA stretcher (`Stretch`) wrapped around every source we append:
        // the sample rate stays put, only the tempo changes.
        let speed = SpeedHandle::new(initial_speed);
        speed.set_mode(initial_speed_mode);
        Self {
            reader,
            total_duration: None,
            seek_offset: Duration::default(),
            volume,
            speed,
            eq,
            sink: None,
            health: Arc::new(DeviceHealth {
                device_lost: AtomicBool::new(false),
                last_underrun_ms: AtomicU64::new(u64::MAX),
                epoch: Instant::now(),
            }),
            last_pos: Duration::default(),
            stall_ticks: 0,
            player: None,
            sink_dev_id: None,
            follow_poll: 0,
            follow_diff: 0,
            device_error_reported: false,
            pending_resume: None,
            event_tx,
        }
    }

    fn run(
        mut self,
        control_rx: std::sync::mpsc::Receiver<ControlCmd>,
        initial_seek_time: Option<Duration>,
    ) {
        // Lazy-init the audio sink before first playback.
        if self.ensure_sink() {
            self.player = self.start_playback(initial_seek_time);
        } else {
            // No device yet; remember what to start once the retry loop gets one.
            self.pending_resume = Some((initial_seek_time, false));
        }

        loop {
            match control_rx.recv_timeout(PROGRESS_INTERVAL) {
                Ok(cmd) => {
                    if self.handle_control(cmd) {
                        continue;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }

            // Fatal stream error (device removed etc.) — rebuild and resume at
            // the last known position.
            if self.health.device_lost.load(Ordering::Relaxed) {
                let resume_at = self.player.as_ref().map(|_| self.song_pos());
                let was_paused = self.player.as_ref().is_some_and(|p| p.is_paused());
                self.rebuild_sink(resume_at, was_paused);
                continue;
            }

            // Follow system default output device changes: after a Bluetooth
            // headset reconnects the OS makes it default again, and without
            // this the stream would keep playing on the fallback (speakers)
            // forever. Rebuild only when the change is stable, and keep
            // playing at the current position on the new device.
            if self.follow_default_device() {
                continue;
            }

            if self.handle_playback_tick() {
                continue;
            }
        }
    }

    /// Position within the recording: where the stretcher has read to, plus the
    /// offset the current source was seeked to.
    fn song_pos(&self) -> Duration {
        self.seek_offset + self.speed.position()
    }

    /// Report a missing/broken output device once. Rebuild retries happen every
    /// progress tick; without this guard each retry would log and toast.
    pub(super) fn report_device_error(&mut self, message: String) {
        if self.device_error_reported {
            return;
        }
        self.device_error_reported = true;
        log::warn!("{message}");
        let _ = self
            .event_tx
            .send(PlaybackEvent::DeviceError(Some(message)).into());
    }

    pub(super) fn clear_device_error(&mut self) {
        if !self.device_error_reported {
            return;
        }
        self.device_error_reported = false;
        let _ = self.event_tx.send(PlaybackEvent::DeviceError(None).into());
    }

    /// Poll the system default output device and rebuild the sink once a change
    /// has been stable for [`FOLLOW_DIFF_THRESHOLD`] polls. Returns `true` if a
    /// rebuild happened.
    fn follow_default_device(&mut self) -> bool {
        self.follow_poll += 1;
        if self.follow_poll < FOLLOW_POLL_TICKS {
            return false;
        }
        self.follow_poll = 0;
        if self.sink.is_none() {
            return false;
        }
        let Some(id) = current_default_id() else {
            return false;
        };
        if Some(&id) != self.sink_dev_id.as_ref() {
            self.follow_diff += 1;
        } else {
            self.follow_diff = 0;
        }
        if self.follow_diff < FOLLOW_DIFF_THRESHOLD {
            return false;
        }
        log::info!(
            "默认输出设备变化（{} -> {id}），切换音频设备",
            self.sink_dev_id.as_deref().unwrap_or("?")
        );
        self.follow_diff = 0;
        let resume_at = self.player.as_ref().map(|_| self.song_pos());
        let was_paused = self.player.as_ref().is_some_and(|p| p.is_paused());
        self.rebuild_sink(resume_at, was_paused);
        true
    }

    /// Emit progress and run the frozen-position watchdog. Returns `true` when
    /// playback finished (the caller then starts the next iteration).
    fn handle_playback_tick(&mut self) -> bool {
        if self.player.is_none() {
            return false;
        }
        let finished = self
            .player
            .as_ref()
            .is_some_and(|p| p.empty() && !p.is_paused());
        if finished {
            let _ = self.event_tx.send(PlaybackEvent::Finished.into());
            if let Some(p) = self.player.as_ref() {
                p.stop();
            }
            self.player = None;
            self.total_duration = None;
            self.seek_offset = Duration::default();
            return true;
        }

        if self.player.as_ref().is_some_and(|p| p.is_paused()) {
            return false;
        }

        let pos = self.song_pos();
        let _ = self.event_tx.send(
            PlaybackEvent::Progress {
                position: pos,
                total: self.total_duration,
            }
            .into(),
        );

        // Watchdog: position frozen for STALL_TICKS while playing means the
        // stream died silently (macOS sometimes removes the output device
        // without firing the error callback). Skip when underruns were seen
        // recently — that freeze is just the network lagging behind.
        let (empty, raw_pos) = self
            .player
            .as_ref()
            .map(|p| (p.empty(), p.get_pos()))
            .unwrap_or((true, Duration::default()));
        if empty {
            return false;
        }
        if raw_pos == self.last_pos {
            self.stall_ticks += 1;
        } else {
            self.stall_ticks = 0;
            self.last_pos = raw_pos;
        }
        let underrun_ms = self.health.last_underrun_ms.load(Ordering::Relaxed);
        let now_ms = underrun_now(&self.health.epoch);
        let recent_underrun = now_ms.saturating_sub(underrun_ms) < UNDERRUN_FRESH_MS;
        if self.stall_ticks >= STALL_TICKS && !recent_underrun {
            log::warn!("播放位置持续冻结且无网络欠载，疑似输出设备失效，重建音频流");
            self.rebuild_sink(Some(pos), false);
        }
        false
    }
}
