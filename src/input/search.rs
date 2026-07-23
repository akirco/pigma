use crate::event::NavigationEvent;
use crate::state::App;
use crate::state::ContentState;
use crossterm::event::{KeyCode, KeyEvent};

pub(super) fn handle_search_key(app: &mut App, key_event: KeyEvent) -> bool {
    match key_event.code {
        KeyCode::Esc => {
            app.state.events.send(NavigationEvent::SearchDeactivated);
            return true;
        }
        KeyCode::Enter => {
            if app.state.navigation.search.filter_queue_only {
                let sel = app.state.navigation.playlist_selected;
                let song_id = app.playback.song_at(sel).map(|s| s.id);
                let restored = app
                    .state
                    .navigation
                    .search
                    .unfiltered_songs
                    .take()
                    .unwrap_or_default();
                app.playback.set_queue_songs(restored);
                app.state.navigation.search.active = false;
                app.state.navigation.search.filter_queue_only = false;
                app.state.navigation.search.unfiltered_songs = None;
                app.state.navigation.search.unfiltered_songs_lower = None;
                app.state.navigation.playlist_selected = 0;
                if let Some(id) = song_id
                    && let Some(pos) = app.playback.queue_songs().iter().position(|s| s.id == id)
                {
                    app.playback.play_index(pos);
                }
            } else {
                let keyword = app.state.navigation.search.input.value.clone();
                if !keyword.is_empty() {
                    app.state.navigation.search.active = false;
                    app.state.events.send(NavigationEvent::SearchSong(keyword));
                } else if let ContentState::HotSearch(keywords) =
                    app.state.navigation.content.as_ref()
                {
                    let sel = app.state.navigation.content_selected;
                    if let Some(kw) = keywords.get(sel) {
                        app.state.navigation.search.active = false;
                        app.state
                            .events
                            .send(NavigationEvent::SearchSong(kw.clone()));
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
                apply_filter_queue_only(app);
            }
            return true;
        }
        KeyCode::Char(c) => {
            app.state.navigation.search.input.enter_char(c);
            if app.state.navigation.search.filter_queue_only {
                apply_filter_queue_only(app);
            }
            return true;
        }
        _ => {}
    }
    false
}

pub(super) fn apply_filter_queue_only(app: &mut App) {
    let keyword = app.state.navigation.search.input.value.to_lowercase();
    if let (Some(full), Some(lower)) = (
        &app.state.navigation.search.unfiltered_songs,
        &app.state.navigation.search.unfiltered_songs_lower,
    ) {
        if keyword.is_empty() {
            let all: Vec<usize> = (0..full.len()).collect();
            app.playback.set_queue_indices(full, &all);
        } else {
            let indices: Vec<usize> = lower
                .iter()
                .enumerate()
                .filter(|(_i, (ln, ls))| ln.contains(&keyword) || ls.contains(&keyword))
                .map(|(i, _)| i)
                .collect();
            app.playback.set_queue_indices(full, &indices);
        }
        app.state.navigation.playlist_selected = 0;
    }
}
