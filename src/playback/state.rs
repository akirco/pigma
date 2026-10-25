use std::{sync::Arc, time::Duration};

use ncm_api::SongInfo;

use super::{cover::CoverState, mode::PlayMode};

use super::lyrics::LyricLine;

/// Lower bound of the playback speed factor (keyboard `[`).
pub const MIN_SPEED: f64 = 0.5;
/// Upper bound of the playback speed factor (keyboard `]`).
pub const MAX_SPEED: f64 = 2.0;
/// Speed step applied by the `[` / `]` keys.
pub const SPEED_STEP: f64 = 0.1;

#[derive(Debug, Clone)]
pub struct PlaybackState {
    pub progress: f64,
    pub volume: f64,
    /// Playback speed factor (1.0 = normal), adjustable with `[` / `]`.
    pub speed: f64,
    /// Whether a speed change keeps the pitch (WSOLA stretch; the badge gains a
    /// leading `*`) or shifts it along with the tempo (resampling; no `*`).
    /// Toggled with `t`.
    pub pitch_locked: bool,
    pub paused: bool,
    pub playing: bool,
    pub seeking: bool,
    pub current_song: Option<Arc<SongInfo>>,
    pub error: Option<String>,
    pub lyrics: Option<Vec<LyricLine>>,
    pub translated_lyrics: Option<Vec<LyricLine>>,
    pub mode: PlayMode,
    pub cached: bool,
    pub liked: bool,
    pub cover: CoverState,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            progress: 0.0,
            volume: 0.65,
            speed: 1.0,
            pitch_locked: true,
            paused: false,
            playing: false,
            seeking: false,
            current_song: None,
            error: None,
            lyrics: None,
            translated_lyrics: None,
            mode: PlayMode::Sequential,
            cached: false,
            liked: false,
            cover: CoverState::default(),
        }
    }
}

/// Clamp a playback speed factor into [`MIN_SPEED`]..=[`MAX_SPEED`].
pub fn clamp_speed(speed: f64) -> f64 {
    if !speed.is_finite() {
        return 1.0;
    }
    speed.clamp(MIN_SPEED, MAX_SPEED)
}

impl PlaybackState {
    /// The speed as shown on the player bar: `*1.2x` when the pitch is kept,
    /// `1.2x` when the speed change also shifts the pitch. Always rendered,
    /// including the default `1.0x`, so the player bar does not leave a blank
    /// slot.
    pub fn speed_badge(&self) -> String {
        let star = if self.pitch_locked { "*" } else { "" };
        format!("{star}{:.1}x", self.speed)
    }

    /// Round `speed` into the supported range, snapping to the nearest
    /// tenth of [`SPEED_STEP`] so repeated `[`/`]` presses never drift off 0.1.
    pub fn set_speed(&mut self, speed: f64) {
        self.speed = (clamp_speed(speed) * 10.0).round() / 10.0;
    }

    pub(super) fn on_started(&mut self) {
        self.error = None;
        self.paused = false;
        self.playing = true;
        self.lyrics = None;
        self.translated_lyrics = None;
    }

    pub(super) fn on_progress(&mut self, position: Duration, total: Option<Duration>) {
        self.seeking = false;
        let total_secs = match total {
            Some(t) => t.as_secs_f64(),
            None => self
                .current_song
                .as_ref()
                .map(|s| s.duration as f64 / 1000.0)
                .unwrap_or(0.0),
        };
        if total_secs > 0.0 {
            self.progress = (position.as_secs_f64() / total_secs).clamp(0.0, 1.0);
        }
    }

    /// Resets progress. Returns `true` if the caller should advance to the next song.
    pub(super) fn on_finished(&mut self) -> bool {
        self.progress = 0.0;
        // When the track ends but nothing advances it (e.g. an empty queue winding down),
        // exit the seeking poll loop as a safety net.
        self.seeking = false;
        self.playing
    }

    pub(super) fn clear_after_stopped(&mut self) {
        self.current_song = None;
        self.error = None;
        self.paused = false;
    }

    pub(super) fn on_error(&mut self, err: String) {
        log::error!("Playback error: {}", err);
        // buffer underrun/overrun is transient — rodio recovers automatically
        if err.contains("buffer underrun") || err.contains("overrun") {
            return;
        }
        self.error = Some(err);
    }

    pub(super) fn on_lyrics_loaded(
        &mut self,
        song_id: u64,
        lyrics: Vec<LyricLine>,
        translated_lyrics: Vec<LyricLine>,
    ) {
        if let Some(song) = &self.current_song
            && song.id == song_id
        {
            self.lyrics = Some(lyrics);
            self.translated_lyrics = Some(translated_lyrics);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_speed_is_normal() {
        assert_eq!(PlaybackState::default().speed, 1.0);
    }

    #[test]
    fn set_speed_snaps_to_tenths_without_drift() {
        let mut s = PlaybackState::default();
        // 11 presses of `]` must land exactly on the upper bound, not 2.099999.
        for _ in 0..11 {
            s.set_speed(s.speed + SPEED_STEP);
        }
        assert_eq!(s.speed, MAX_SPEED);
        for _ in 0..25 {
            s.set_speed(s.speed - SPEED_STEP);
        }
        assert_eq!(s.speed, MIN_SPEED);
    }

    #[test]
    fn set_speed_clamps_invalid_input() {
        let mut s = PlaybackState::default();
        s.set_speed(9.9);
        assert_eq!(s.speed, MAX_SPEED);
        s.set_speed(0.0);
        assert_eq!(s.speed, MIN_SPEED);
        s.set_speed(f64::NAN);
        assert_eq!(s.speed, 1.0);
    }
}
