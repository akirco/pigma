//! Equalizer page keys: band selection, gain adjustment and reset.

use crossterm::event::{KeyCode, KeyEvent};

use crate::app::App;

pub(super) fn handle_equalizer_key(app: &mut App, key_event: KeyEvent) {
    if super::common::handle_playback_key(app, key_event)
        || super::common::handle_page_key(app, key_event)
    {
        return;
    }
    match key_event.code {
        KeyCode::Esc => app.close_equalizer(),
        KeyCode::Up | KeyCode::Char('k' | 'K') => app.eq_select_prev(),
        KeyCode::Down | KeyCode::Char('j' | 'J') => app.eq_select_next(),
        KeyCode::Left => app.eq_adjust(-0.5),
        KeyCode::Right => app.eq_adjust(0.5),
        KeyCode::Char('r' | 'R') => app.reset_equalizer(),
        _ => {}
    }
}
