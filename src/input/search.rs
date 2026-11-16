use crossterm::event::{KeyCode, KeyEvent};

use crate::{app::App, event::NavigationEvent, state::ContentState};

pub(super) fn handle_search_key(app: &mut App, key_event: KeyEvent) -> bool {
    match key_event.code {
        KeyCode::Esc => {
            app.events.send(NavigationEvent::SearchDeactivated);
            return true;
        }
        KeyCode::Enter => {
            if app.state.navigation.search.filter_queue_only {
                // Restoring the queue and moving the playhead is state work; the
                // handler owns it so this stays a key-to-intent mapping.
                app.events.send(NavigationEvent::QueueFilterExit);
            } else {
                let keyword = app.state.navigation.search.input.value.clone();
                if !keyword.is_empty() {
                    app.state.navigation.search.active = false;
                    app.events.send(NavigationEvent::SearchSong(keyword));
                } else if let ContentState::HotSearch(keywords) =
                    app.state.navigation.content.as_ref()
                {
                    let sel = app.state.navigation.content_selected;
                    if let Some(kw) = keywords.0.get(sel) {
                        app.state.navigation.search.active = false;
                        app.events.send(NavigationEvent::SearchSong(kw.clone()));
                    }
                }
            }
            return true;
        }
        KeyCode::Left => {
            app.state.navigation.search.input.move_left();
            return true;
        }
        KeyCode::Right => {
            app.state.navigation.search.input.move_right();
            return true;
        }
        KeyCode::Backspace => {
            app.state.navigation.search.input.delete_char();
            if app.state.navigation.search.filter_queue_only {
                app.events.send(NavigationEvent::QueueFilterChanged);
            }
            return true;
        }
        KeyCode::Tab => {
            app.state.navigation.search.cycle_provider(true);
            return true;
        }
        KeyCode::BackTab => {
            app.state.navigation.search.cycle_provider(false);
            return true;
        }
        KeyCode::Char(c) => {
            app.state.navigation.search.input.enter_char(c);
            if app.state.navigation.search.filter_queue_only {
                app.events.send(NavigationEvent::QueueFilterChanged);
            }
            return true;
        }
        _ => {}
    }
    false
}
