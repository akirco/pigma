use crossterm::event::{KeyCode, KeyEvent, MouseEventKind};

use crate::{
    app::App,
    event::{CommandEvent, NavigationEvent},
    service::ApiEndpoint,
    state::{Page, TableMode},
};

pub(super) fn handle_main_key(app: &mut App, key_event: KeyEvent) -> eyre::Result<()> {
    if super::global::handle_playback_key(app, key_event)
        || super::global::handle_page_key(app, key_event)
    {
        return Ok(());
    }
    match key_event.code {
        KeyCode::Esc => {
            app.events.send(NavigationEvent::ContentRestore);
        }
        KeyCode::Tab if app.state.navigation.page == Page::Playlist => app.switch_queue_page(true),
        KeyCode::BackTab if app.state.navigation.page == Page::Playlist => {
            app.switch_queue_page(false)
        }
        KeyCode::Tab => app.nav_select_next(),
        KeyCode::BackTab => app.nav_select_prev(),
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            if app.state.navigation.page == Page::Playlist {
                app.playlist_select_prev();
            } else {
                app.content_select_prev();
            }
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            if app.state.navigation.page == Page::Playlist {
                app.playlist_select_next();
            } else {
                app.content_select_next();
            }
        }
        KeyCode::Char('g') => {
            if app.state.navigation.page == Page::Playlist {
                app.playlist_select_first();
            } else {
                app.content_select_first();
            }
        }
        KeyCode::Char('G') => {
            if app.state.navigation.page == Page::Playlist {
                app.playlist_select_last();
            } else {
                app.content_select_last();
            }
        }
        KeyCode::Enter => {
            if app.state.navigation.page == Page::Playlist {
                app.playlist_play_selected();
            } else if app.state.navigation.table_mode == TableMode::Cell
                && app.state.navigation.content_columns == 1
            {
                app.enter_selected_cell();
            } else {
                app.enter_selected_row();
            }
        }
        KeyCode::Left => {
            if app.state.navigation.table_mode == TableMode::Cell
                && app.state.navigation.content_columns == 1
                && app.state.navigation.page != Page::Playlist
            {
                app.cell_select_prev_column();
            } else if app.state.navigation.page == Page::Main
                && app.state.navigation.content_columns > 1
            {
                app.content_select_left();
            } else if app.playback.current_song().is_some() {
                let interval = app.config.seek_interval_secs as f64;
                app.playback.seek_relative(-interval);
            }
        }
        KeyCode::Right => {
            if app.state.navigation.table_mode == TableMode::Cell
                && app.state.navigation.content_columns == 1
                && app.state.navigation.page != Page::Playlist
            {
                app.cell_select_next_column();
            } else if app.state.navigation.page == Page::Main
                && app.state.navigation.content_columns > 1
            {
                app.content_select_right();
            } else if app.playback.current_song().is_some() {
                let interval = app.config.seek_interval_secs as f64;
                app.playback.seek_relative(interval);
            }
        }
        KeyCode::Char('c' | 'C') => {
            app.toggle_table_mode();
        }
        KeyCode::Char('/') => {
            if app.state.navigation.page == Page::Playlist {
                app.begin_queue_filter();
            } else if app.state.navigation.page == Page::Lyrics {
                return Ok(());
            } else {
                app.events.send(NavigationEvent::SearchActivated);
            }
        }
        KeyCode::Char('b' | 'B') => {
            app.events.send(CommandEvent::ToggleBordered);
        }
        KeyCode::Char('S') => app.like_current_song(),
        KeyCode::Char('s') => app.like_selected_song(),
        KeyCode::Char('a' | 'A') => app.add_selected_next(),
        KeyCode::Char('d') => app.dislike_selected_song(),
        KeyCode::Char('D') => app.unlike_current_song(),
        KeyCode::Char('r' | 'R') => {
            if matches!(app.state.navigation.page, Page::Main | Page::Lyrics) {
                app.reload_current_nav();
            }
        }
        KeyCode::Char('u' | 'U')
            if matches!(
                app.selected_endpoint(),
                Some(ApiEndpoint::Download | ApiEndpoint::LocalMusic)
            ) =>
        {
            let sel = app.state.navigation.content_selected;
            app.events.send(NavigationEvent::UploadCachedSong(sel));
        }
        KeyCode::Char('z' | 'Z') => {
            app.cycle_nav_position();
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn handle_main_mouse(app: &mut App, kind: MouseEventKind, col: u16, row: u16) {
    // Volume scroll: if mouse is over playerbar area
    let area = app.playerbar_area();
    if row >= area.y && row < area.y + area.height && col >= area.x && col < area.x + area.width {
        match kind {
            MouseEventKind::ScrollUp => app.adjust_volume(0.05),
            MouseEventKind::ScrollDown => app.adjust_volume(-0.05),
            _ => {}
        }
        return;
    }

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
                app.content_select_prev();
            } else if kind == MouseEventKind::ScrollDown {
                app.content_select_next();
            }
        }
        Page::Playlist => {
            if kind == MouseEventKind::ScrollUp {
                app.playlist_select_prev();
            } else if kind == MouseEventKind::ScrollDown {
                app.playlist_select_next();
            }
        }
        _ => {}
    }
}
