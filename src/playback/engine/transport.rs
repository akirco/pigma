//! Transport-level controls for [`PlaybackEngine`]: seeking, volume and play
//! mode cycling.

use std::time::Duration;

use super::{
    PlaybackEngine,
    mode::{self, PlayMode},
};

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

    pub fn set_volume(&mut self, volume: f64) {
        self.state.volume = volume;
        self.controller.set_volume(volume as f32);
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
