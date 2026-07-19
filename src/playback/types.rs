use std::sync::Arc;
use std::time::Duration;

use ncm_api::SongInfo;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlayMode {
    Sequential,
    RepeatOne,
    RepeatAll,
    Shuffle,
    Heartbeat { playlist_id: u64 },
}

impl PlayMode {
    pub fn display_name(&self) -> &str {
        match self {
            PlayMode::Sequential => "顺序",
            PlayMode::RepeatOne => "单曲循环",
            PlayMode::RepeatAll => "列表循环",
            PlayMode::Shuffle => "随机",
            PlayMode::Heartbeat { .. } => "心动",
        }
    }

    pub fn short_name(&self) -> &str {
        match self {
            PlayMode::Sequential => "SEQ",
            PlayMode::RepeatOne => "ONE",
            PlayMode::RepeatAll => "ALL",
            PlayMode::Shuffle => "SHF",
            PlayMode::Heartbeat { .. } => "HRT",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LyricLine {
    pub time: Duration,
    pub text: String,
}

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

pub fn parse_lyric_lines(raw: &[String]) -> Vec<LyricLine> {
    let mut lines: Vec<LyricLine> = raw
        .iter()
        .filter_map(|line| {
            let rest = line.strip_prefix('[')?;
            let close = rest.find(']')?;
            let ts = &rest[..close];
            let text = rest[close + 1..].trim().to_string();
            if text.is_empty() {
                return None;
            }
            let parts: Vec<&str> = ts.split(':').collect();
            if parts.len() < 2 {
                return None;
            }
            let mins: f64 = parts[0].parse().ok()?;
            let secs: f64 = parts[1].parse().ok()?;
            let time = Duration::from_secs_f64(mins * 60.0 + secs);
            Some(LyricLine { time, text })
        })
        .collect();
    // Only sort if not already sorted (LRC files are typically pre-sorted)
    if lines.windows(2).any(|w| w[0].time > w[1].time) {
        lines.sort_by_key(|l| l.time);
    }
    lines
}
