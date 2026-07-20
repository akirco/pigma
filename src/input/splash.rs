use crate::event::AppEvent;
use crate::state::App;
use crossterm::event::{KeyCode, KeyEvent};

pub(super) fn handle_splash_key(app: &mut App, key_event: KeyEvent) {
    match key_event.code {
        KeyCode::Esc | KeyCode::Char('q') => app.state.events.send(AppEvent::Quit),
        _ => {
            app.state.splash.boot_complete = true;
            app.state.navigation.page = crate::state::Page::Login;
        }
    }
}
