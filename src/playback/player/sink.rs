//! Output-sink lifecycle: opening the device, (re)starting the rodio player and
//! rebuilding the stream after device loss.

use std::{io::SeekFrom, sync::atomic::Ordering, time::Duration};

use rodio::Source;

use super::{
    super::{equalizer::EqSource, stretch::Stretch},
    device::{create_sink, current_default_id},
    task::PlayerTask,
};
use crate::{event::PlaybackEvent, lock::lock_recover};

impl PlayerTask {
    pub(super) fn ensure_sink(&mut self) -> bool {
        if self.sink.is_some() {
            return true;
        }
        match create_sink(self.health.clone()) {
            Ok(mut s) => {
                s.log_on_drop(false);
                self.sink = Some(s);
                self.sink_dev_id = current_default_id();
                self.health.device_lost.store(false, Ordering::Relaxed);
                self.clear_device_error();
                true
            }
            Err(e) => {
                // Make the main loop retry opening the device; the pending
                // playback state (if any) is retained by the caller.
                self.health.device_lost.store(true, Ordering::Relaxed);
                self.report_device_error(format!("音频设备不可用: {e}"));
                false
            }
        }
    }

    pub(super) fn start_playback(&mut self, seek_time: Option<Duration>) -> Option<rodio::Player> {
        match rodio::Decoder::new(self.reader.clone()) {
            Ok(d) => {
                let Some(mixer) = self.sink.as_ref().map(|sink| sink.mixer().clone()) else {
                    log::warn!("start_playback called without an audio sink");
                    return None;
                };
                self.total_duration = d.total_duration();
                let (stretched, offset): (Box<dyn Source<Item = f32> + Send>, Duration) =
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
                // Pitch-preserving speed control lives here rather than on the
                // Player, whose `set_speed` would resample.
                let source: Box<dyn Source<Item = f32> + Send> = Box::new(Stretch::new(
                    EqSource::new(stretched, self.eq.clone()),
                    self.speed.clone(),
                ));
                self.seek_offset = offset;
                let p = rodio::Player::connect_new(&mixer);
                p.set_volume(self.volume);
                p.append(source);
                self.last_pos = Duration::default();
                self.stall_ticks = 0;
                Some(p)
            }
            Err(e) => {
                let _ = self
                    .event_tx
                    .send(PlaybackEvent::Error(format!("decode: {e}")).into());
                None
            }
        }
    }

    /// Drop the current player/sink and reopen the (current default) audio
    /// device. If `resume_at`/`was_paused` are provided, playback continues from
    /// there on the fresh stream.
    pub(super) fn rebuild_sink(&mut self, resume_at: Option<Duration>, was_paused: bool) {
        // Retries arrive with `resume_at == None` once the failed attempt has
        // dropped the player; keep the first position so a device that comes
        // back still resumes the song instead of silently stopping.
        if resume_at.is_some() {
            self.pending_resume = Some((resume_at, was_paused));
        }
        self.player = None;
        self.sink = None;
        match create_sink(self.health.clone()) {
            Ok(mut s) => {
                s.log_on_drop(false);
                self.sink = Some(s);
                self.sink_dev_id = current_default_id();
                self.health.device_lost.store(false, Ordering::Relaxed);
                self.stall_ticks = 0;
                self.clear_device_error();
                let resume = self.pending_resume.take();
                log::info!(
                    "音频设备已重建{}",
                    resume
                        .as_ref()
                        .and_then(|(at, _)| *at)
                        .map(|t| format!("，从 {t:?} 继续"))
                        .unwrap_or_default()
                );
                if let Some((seek_time, was_paused)) = resume {
                    let _ = lock_recover(&self.reader.0).seek(SeekFrom::Start(0));
                    self.player = self.start_playback(seek_time);
                    if was_paused && let Some(ref p) = self.player {
                        p.pause();
                    }
                }
            }
            Err(e) => {
                // Keep device_lost set so the loop retries next tick
                // (e.g. device still reconnecting). The error is reported once;
                // progress-tick retries stay quiet.
                self.health.device_lost.store(true, Ordering::Relaxed);
                self.report_device_error(format!("音频设备不可用: {e}"));
            }
        }
    }
}
