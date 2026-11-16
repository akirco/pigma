use crossterm::event::{KeyCode, KeyEvent};

use crate::{app::App, event::AppEvent};

pub(super) fn handle_splash_key(app: &mut App, key_event: KeyEvent) {
    match key_event.code {
        KeyCode::Esc | KeyCode::Char('q') => app.events.send(AppEvent::Quit),
        _ => {}
    }
}
