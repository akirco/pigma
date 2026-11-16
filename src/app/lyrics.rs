//! Lyrics pipeline: loads lyrics for the playing song (NCM API or sonar
//! provider fallback) and emits `LyricsLoaded` for the playback state.

use super::{App, event::send_event};
use crate::{
    event::{EventSender, PlaybackEvent},
    lyrics::{LyricLine, parse_lyric_lines},
    state::Page,
};

impl App {
    /// Toggle between the main and lyrics pages with the shared page
    /// transition, used by the modern player-bar cover.
    pub(crate) fn toggle_lyrics_transition(&mut self) {
        if self.page_transition.is_some() {
            return;
        }
        let target = match self.state.navigation.page {
            Page::Main => Page::Lyrics,
            Page::Lyrics => Page::Main,
            _ => return,
        };
        if target == Page::Lyrics && self.playback.current_song().is_none() {
            return;
        }
        self.start_page_transition(target);
    }

    /// Spawn a background task that loads the lyrics for `song_id` (NCM or
    /// sonar path) and emits `LyricsLoaded` when found.
    pub(super) fn spawn_lyrics_load(&self, song_id: u64) {
        let service = self.service.clone();
        let sender = self.events.sender();
        if sonar::is_sonar_song_id(song_id) {
            let finder = self.search.finder.clone();
            let registry = self.search.sonar_songs.clone();
            tokio::spawn(async move {
                let Some((lyric_lines, tlyric_lines)) =
                    service.load_sonar_lyrics(song_id, finder, &registry).await
                else {
                    return;
                };
                send_lyrics(&sender, song_id, lyric_lines, tlyric_lines);
            });
        } else {
            tokio::spawn(async move {
                if let Some(lyrics) = service.load_lyrics(song_id).await {
                    let lyric_lines = parse_lyric_lines(&lyrics.lyric);
                    let tlyric_lines = parse_lyric_lines(&lyrics.tlyric);
                    send_lyrics(&sender, song_id, lyric_lines, tlyric_lines);
                }
            });
        }
    }
}

fn send_lyrics(
    sender: &EventSender,
    song_id: u64,
    lyrics: Vec<LyricLine>,
    translated_lyrics: Vec<LyricLine>,
) {
    send_event(
        sender,
        PlaybackEvent::LyricsLoaded {
            song_id,
            lyrics,
            translated_lyrics,
        }
        .into(),
    );
}
