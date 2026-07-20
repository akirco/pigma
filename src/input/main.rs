use crate::event::AppEvent;
use crate::state::{App, Page, TableMode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEventKind};

use super::content::{
    cell_enter_action, content_select_next, content_select_prev, playlist_play_selected,
    playlist_select_next, playlist_select_prev, row_enter_action,
};
use super::navigation::{navigate_nav_down, navigate_nav_up};
use super::table::{cell_select_next_column, cell_select_prev_column, toggle_table_mode};

pub(super) fn handle_main_key(app: &mut App, key_event: KeyEvent) -> color_eyre::Result<()> {
    // Ctrl+C and Ctrl+P are handled globally in input.rs
    match key_event.code {
        KeyCode::Esc => {
            app.state.events.send(AppEvent::ContentRestore);
        }
        KeyCode::Char('q') => app.state.events.send(AppEvent::Quit),
        KeyCode::Tab => navigate_nav_down(app),
        KeyCode::BackTab => navigate_nav_up(app),
        KeyCode::Up => {
            if app.state.navigation.page == Page::Playlist {
                playlist_select_prev(app);
            } else {
                content_select_prev(app);
            }
        }
        KeyCode::Down => {
            log::info!(
                "KEY_DOWN page={:?} selected={}",
                app.state.navigation.page,
                app.state.navigation.playlist_selected
            );
            if app.state.navigation.page == Page::Playlist {
                playlist_select_next(app);
            } else {
                content_select_next(app);
            }
        }
        KeyCode::Left if key_event.modifiers == KeyModifiers::SHIFT => {
            app.playback.prev();
        }
        KeyCode::Right if key_event.modifiers == KeyModifiers::SHIFT => {
            app.playback.next();
        }
        KeyCode::Enter => {
            if app.state.navigation.page == Page::Playlist {
                playlist_play_selected(app);
            } else if app.state.navigation.table_mode == TableMode::Cell {
                cell_enter_action(app);
            } else {
                row_enter_action(app);
            }
        }
        KeyCode::Left => {
            if app.state.navigation.table_mode == TableMode::Cell
                && app.state.navigation.page != Page::Playlist
            {
                cell_select_prev_column(app);
            } else if app.playback.current_song().is_some() {
                let interval = app.config.seek_interval_secs as f64;
                app.playback.seek_relative(-interval);
            }
        }
        KeyCode::Right => {
            if app.state.navigation.table_mode == TableMode::Cell
                && app.state.navigation.page != Page::Playlist
            {
                cell_select_next_column(app);
            } else if app.playback.current_song().is_some() {
                let interval = app.config.seek_interval_secs as f64;
                app.playback.seek_relative(interval);
            }
        }
        KeyCode::Char('l' | 'L') => {
            let next = match app.state.navigation.page {
                Page::Main => Page::Lyrics,
                Page::Lyrics => Page::Main,
                Page::Playlist => Page::Main,
                Page::Login => Page::Main,
                Page::Splash => Page::Splash,
            };
            app.state.events.send(AppEvent::Navigate(next));
        }
        KeyCode::Char('p' | 'P') => {
            if app.state.navigation.page != Page::Playlist {
                toggle_table_mode(app);
            } else {
                app.playback.prev();
            }
        }
        KeyCode::Char('f' | 'F') => {
            let next = match app.state.navigation.page {
                Page::Main => {
                    app.state.navigation.playlist_selected =
                        app.playback.queue_current_index().unwrap_or(0);
                    Page::Playlist
                }
                Page::Playlist => Page::Main,
                Page::Lyrics => Page::Main,
                Page::Login => Page::Main,
                Page::Splash => Page::Splash,
            };
            app.state.events.send(AppEvent::Navigate(next));
        }
        KeyCode::Char('/') => {
            if app.state.navigation.page == Page::Playlist {
                app.state.navigation.search.filter_queue_only = true;
                let songs = app.playback.queue_songs();
                app.state.navigation.search.unfiltered_songs = Some(songs.to_vec());
                app.state.navigation.search.unfiltered_songs_lower = Some(
                    songs
                        .iter()
                        .map(|s| (s.name.to_lowercase(), s.singer.to_lowercase()))
                        .collect(),
                );
                app.state.navigation.search.active = true;
                app.state.navigation.search.input = crate::text_input::TextInput::new();
            } else {
                app.state.events.send(AppEvent::SearchActivated);
            }
        }
        KeyCode::Char('b' | 'B') => {
            app.state.events.send(AppEvent::ToggleBordered);
        }
        KeyCode::Char(' ') => {
            let was_paused = app.playback.state.paused;
            app.playback.toggle_pause();
            if let Some(song) = app.playback.current_song() {
                if was_paused {
                    app.toast(format!("▶  {}", song.name));
                } else {
                    app.toast(format!("⏸  {}", song.name));
                }
            }
        }
        KeyCode::Char('m') => {
            app.playback.cycle_mode();
        }
        _ => {}
    }
    app.report_pending_play();
    Ok(())
}

pub(super) fn handle_main_mouse(app: &mut App, kind: MouseEventKind) {
    match app.state.navigation.page {
        Page::Lyrics => {
            if kind == MouseEventKind::ScrollUp {
                app.playback.seek_relative(-5.0);
            } else if kind == MouseEventKind::ScrollDown {
                app.playback.seek_relative(5.0);
            }
        }
        Page::Main => {
            if kind == MouseEventKind::ScrollUp {
                content_select_prev(app);
            } else if kind == MouseEventKind::ScrollDown {
                content_select_next(app);
            }
        }
        Page::Playlist => {
            if kind == MouseEventKind::ScrollUp {
                playlist_select_prev(app);
            } else if kind == MouseEventKind::ScrollDown {
                playlist_select_next(app);
            }
        }
        _ => {}
    }
    app.report_pending_play();
}
