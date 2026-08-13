use crate::app::App;
use crate::event::AppEvent;
use crossterm::event::{KeyCode, KeyEvent};

pub(super) fn handle_splash_key(app: &mut App, key_event: KeyEvent) {
    match key_event.code {
        KeyCode::Esc | KeyCode::Char('q') => app.state.events.send(AppEvent::Quit),
        _ => {}
    }
}
