use crate::app::App;
use crossterm::event::{KeyCode, KeyEvent, MouseEventKind};

pub(super) fn handle_help_key(app: &mut App, key_event: KeyEvent) {
    match key_event.code {
        KeyCode::Esc | KeyCode::Char('q' | '?') => app.state.help.close(),
        KeyCode::Up | KeyCode::Char('k' | 'K') => app.state.help.scroll_up(),
        KeyCode::Down | KeyCode::Char('j' | 'J') => app.state.help.scroll_down(),
        _ => {}
    }
}

pub(super) fn handle_help_mouse(app: &mut App, kind: MouseEventKind) {
    match kind {
        MouseEventKind::ScrollUp => app.state.help.scroll_up(),
        MouseEventKind::ScrollDown => app.state.help.scroll_down(),
        _ => {}
    }
}
