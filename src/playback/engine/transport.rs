//! Transport-level controls for [`PlaybackEngine`]: seeking, volume, playback
//! speed and play mode cycling.

use std::time::Duration;

use super::{
    PlaybackEngine,
    mode::{self, PlayMode},
};
use crate::{config::EqualizerConfig, playback::stretch::SpeedMode};

impl PlaybackEngine {
    pub fn seek_relative(&mut self, delta_secs: f64) {
        let duration = match self.queue.current_song() {
            Some(s) => s.duration,
            None => return,
        };
        let total_secs = duration as f64 / 1000.0;
        if total_secs <= 0.0 {
            return;
        }

        if !self.state.playing {
            let cur_secs = self.state.progress * total_secs;
            self.state.progress =
                ((cur_secs + delta_secs).clamp(0.0, total_secs) / total_secs).clamp(0.0, 1.0);
            return;
        }

        let cur_secs = self.state.progress * total_secs;
        let new_secs = (cur_secs + delta_secs).clamp(0.0, total_secs);
        self.state.progress = new_secs / total_secs;
        self.state.seeking = true;
        self.controller.seek_to(Duration::from_secs_f64(new_secs));
    }

    /// Replace the equalizer settings; the audio thread retunes its running
    /// filters without restarting the decoder.
    pub fn set_equalizer(&mut self, equalizer: EqualizerConfig) {
        let equalizer = equalizer.sanitized();
        self.state.equalizer = equalizer;
        self.controller.set_equalizer(equalizer);
    }

    pub fn equalizer(&self) -> EqualizerConfig {
        self.state.equalizer
    }

    pub fn set_volume(&mut self, volume: f64) {
        self.state.volume = volume;
        self.controller.set_volume(volume as f32);
    }

    /// Set the playback speed factor (1.0 = normal). The state clamps it to the
    /// supported range and snaps it to the nearest 0.1; the audio thread is only
    /// signalled when the value actually changed. Speed is applied by
    /// pitch-preserving time stretching, not resampling.
    pub fn set_speed(&mut self, speed: f64) {
        let before = self.state.speed;
        self.state.set_speed(speed);
        if self.state.speed != before {
            self.controller.set_speed(self.state.speed as f32);
        }
    }

    /// Choose how the speed is applied: `locked` keeps the pitch (WSOLA
    /// stretch), otherwise tempo and pitch move together (resampling). Always
    /// forwarded, even when unchanged, so a fresh audio thread inherits it.
    pub fn set_pitch_locked(&mut self, locked: bool) {
        self.state.pitch_locked = locked;
        self.controller.set_speed_mode(if locked {
            SpeedMode::PreservePitch
        } else {
            SpeedMode::ShiftPitch
        });
    }

    /// Flip between pitch-keeping and pitch-shifting speed. Returns the new
    /// `pitch_locked` flag.
    pub fn toggle_pitch_locked(&mut self) -> bool {
        self.set_pitch_locked(!self.state.pitch_locked);
        self.state.pitch_locked
    }

    /// Nudge the playback speed by `delta` (e.g. `±0.1`). Returns the new factor.
    pub fn adjust_speed(&mut self, delta: f64) -> f64 {
        let target = self.state.speed + delta;
        self.set_speed(target);
        self.state.speed
    }

    pub fn cycle_mode(&mut self) -> PlayMode {
        let next = match self.state.mode {
            PlayMode::Sequential => PlayMode::RepeatOne,
            PlayMode::RepeatOne => PlayMode::RepeatAll,
            PlayMode::RepeatAll => PlayMode::Shuffle,
            PlayMode::Shuffle => match self.playlist_id {
                Some(pid) if pid > 0 => PlayMode::Heartbeat { playlist_id: pid },
                _ => PlayMode::Sequential,
            },
            PlayMode::Heartbeat { .. } => PlayMode::Sequential,
        };
        self.set_mode(next);
        next
    }

    pub(in crate::playback) fn set_mode(&mut self, mode: PlayMode) {
        self.state.mode = mode;
        self.strategy =
            mode::create_strategy(&self.state.mode, self.queue.len(), self.queue.current_index);
    }
}
