use crate::app::App;
use crate::event::{AppEvent, AuthEvent};
use crossterm::event::{KeyCode, KeyEvent};

pub(super) fn handle_login_key(app: &mut App, key_event: KeyEvent) -> bool {
    // Ctrl+C and Ctrl+P are handled globally in input.rs
    match key_event.code {
        KeyCode::Enter => {
            app.state.events.send(AuthEvent::Login);
        }
        KeyCode::Esc => app.state.events.send(AppEvent::Quit),
        _ => {}
    }
    true
}
