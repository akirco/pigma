use std::sync::Arc;
use std::time::Duration;

use ncm_api::SongInfo;

use super::cover::CoverState;
use super::lyrics::LyricLine;
use super::mode::PlayMode;

#[derive(Debug, Clone)]
pub struct PlaybackState {
    pub progress: f64,
    pub volume: f64,
    pub paused: bool,
    pub playing: bool,
    pub seeking: bool,
    pub current_song: Option<Arc<SongInfo>>,
    pub error: Option<String>,
    pub lyrics: Option<Vec<LyricLine>>,
    pub translated_lyrics: Option<Vec<LyricLine>>,
    pub mode: PlayMode,
    pub cached: bool,
    pub cover: CoverState,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            progress: 0.0,
            volume: 0.65,
            paused: false,
            playing: false,
            seeking: false,
            current_song: None,
            error: None,
            lyrics: None,
            translated_lyrics: None,
            mode: PlayMode::Sequential,
            cached: false,
            cover: CoverState::default(),
        }
    }
}

impl PlaybackState {
    pub fn on_started(&mut self) {
        self.error = None;
        self.paused = false;
        self.playing = true;
        self.lyrics = None;
        self.translated_lyrics = None;
    }

    pub fn on_progress(&mut self, position: Duration, total: Option<Duration>) {
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
    pub fn on_finished(&mut self) -> bool {
        self.progress = 0.0;
        self.playing
    }

    pub fn clear_after_stopped(&mut self) {
        self.current_song = None;
        self.error = None;
        self.paused = false;
    }

    pub fn on_error(&mut self, err: String) {
        log::error!("Playback error: {}", err);
        // buffer underrun/overrun is transient — rodio recovers automatically
        if err.contains("buffer underrun") || err.contains("overrun") {
            return;
        }
        self.error = Some(err);
    }

    pub fn on_lyrics_loaded(
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
