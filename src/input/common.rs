//! Keys shared by the main and equalizer pages: transport, volume, speed,
//! pitch, play mode, quit, and the `l` / `f` / `e` page switches. Page-specific
//! handlers call these first so they never duplicate the mappings.

use crossterm::event::{KeyCode, KeyEvent};

use crate::{
    app::App,
    event::{AppEvent, NavigationEvent},
    state::Page,
};

/// Returns `true` when the key was a global playback control.
pub(super) fn handle_playback_key(app: &mut App, key_event: KeyEvent) -> bool {
    match key_event.code {
        KeyCode::Char('q') => app.state.events.send(AppEvent::Quit),
        KeyCode::Char(' ') => app.toggle_playback(),
        KeyCode::Char('m' | 'M') => app.cycle_play_mode(),
        KeyCode::Char('t' | 'T') => app.toggle_pitch_locked(),
        KeyCode::Char('+' | '=') => app.adjust_volume(0.05),
        KeyCode::Char('-' | '_') => app.adjust_volume(-0.05),
        KeyCode::Char('[') => app.adjust_speed(-crate::playback::SPEED_STEP),
        KeyCode::Char(']') => app.adjust_speed(crate::playback::SPEED_STEP),
        KeyCode::Char('p' | 'P') => app.playback.prev(),
        KeyCode::Char('n' | 'N') => app.playback.next(),
        _ => return false,
    }
    true
}

/// Page switches shared by the main and equalizer pages (`l` lyrics, `f` queue,
/// `e` equalizer). Returns `true` when the key was consumed.
pub(super) fn handle_page_key(app: &mut App, key_event: KeyEvent) -> bool {
    match key_event.code {
        KeyCode::Char('l') => {
            if app.state.navigation.page == Page::Equalizer {
                app.save_equalizer();
            }
            let next = if app.state.navigation.page == Page::Lyrics {
                Page::Main
            } else {
                Page::Lyrics
            };
            app.state.events.send(NavigationEvent::Navigate(next));
        }
        KeyCode::Char('f' | 'F') => {
            if app.state.navigation.page == Page::Equalizer {
                app.save_equalizer();
            }
            let next = if app.state.navigation.page == Page::Playlist {
                Page::Main
            } else {
                // Entering the queue always starts on the playing song.
                app.state.navigation.playlist_selected =
                    app.playback.queue_current_index().unwrap_or(0);
                Page::Playlist
            };
            app.state.events.send(NavigationEvent::Navigate(next));
        }
        KeyCode::Char('e' | 'E') => {
            if app.state.navigation.page == Page::Equalizer {
                app.close_equalizer();
            } else {
                app.open_equalizer();
            }
        }
        _ => return false,
    }
    true
}
