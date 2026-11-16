use crossterm::event::{KeyCode, KeyEvent};

use crate::{
    app::App,
    event::{AuthEvent, NavigationEvent},
    state::Page,
};

pub(super) fn handle_login_key(app: &mut App, key_event: KeyEvent) -> bool {
    // Ctrl+C and Ctrl+P are handled globally in input.rs
    match key_event.code {
        KeyCode::Enter => {
            app.events.send(AuthEvent::Login);
        }
        KeyCode::Esc => app.events.send(NavigationEvent::Navigate(Page::Main)),
        _ => {}
    }
    true
}
