use crate::event::{CommandEvent, NavigationEvent, PlaybackEvent};
use crate::state::{App, ContentState, Page, TableMode};
use crossterm::event::{KeyCode, KeyEvent, MouseEventKind};
use std::sync::Arc;

use super::content::{
    cell_enter_action, content_select_first, content_select_last, content_select_next,
    content_select_prev, playlist_play_selected, playlist_select_first, playlist_select_last,
    playlist_select_next, playlist_select_prev, row_enter_action,
};
use super::navigation::{navigate_nav_down, navigate_nav_up};
use super::table::{cell_select_next_column, cell_select_prev_column, toggle_table_mode};

pub(super) fn handle_main_key(app: &mut App, key_event: KeyEvent) -> color_eyre::Result<()> {
    // Ctrl+C and Ctrl+P are handled globally in input.rs
    match key_event.code {
        KeyCode::Esc => {
            app.state.events.send(NavigationEvent::ContentRestore);
        }
        KeyCode::Char('q') => app.state.events.send(crate::event::AppEvent::Quit),
        KeyCode::Tab if app.state.navigation.page == Page::Playlist => {
            if let Some(key) = app.playback.switch_queue(true) {
                app.state.navigation.playlist_selected =
                    app.playback.queue_current_index().unwrap_or(0);
                app.toast(format!("▣ 队列: {key}"));
            }
        }
        KeyCode::BackTab if app.state.navigation.page == Page::Playlist => {
            if let Some(key) = app.playback.switch_queue(false) {
                app.state.navigation.playlist_selected =
                    app.playback.queue_current_index().unwrap_or(0);
                app.toast(format!("▣ 队列: {key}"));
            }
        }
        KeyCode::Tab => navigate_nav_down(app),
        KeyCode::BackTab => navigate_nav_up(app),
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            if app.state.navigation.page == Page::Playlist {
                playlist_select_prev(app);
            } else {
                content_select_prev(app);
            }
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            if app.state.navigation.page == Page::Playlist {
                playlist_select_next(app);
            } else {
                content_select_next(app);
            }
        }
        KeyCode::Char('g') => {
            if app.state.navigation.page == Page::Playlist {
                playlist_select_first(app);
            } else {
                content_select_first(app);
            }
        }
        KeyCode::Char('G') => {
            if app.state.navigation.page == Page::Playlist {
                playlist_select_last(app);
            } else {
                content_select_last(app);
            }
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
            app.state.events.send(NavigationEvent::Navigate(next));
        }
        KeyCode::Char('p' | 'P') => {
            app.playback.prev();
        }
        KeyCode::Char('n' | 'N') => {
            app.playback.next();
        }
        KeyCode::Char('c' | 'C') => {
            toggle_table_mode(app);
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
            app.state.events.send(NavigationEvent::Navigate(next));
        }
        KeyCode::Char('/') => {
            if app.state.navigation.page == Page::Playlist {
                app.state.navigation.search.filter_queue_only = true;
                let songs = app.playback.queue_songs();
                app.state.navigation.search.unfiltered_songs = Some(songs.to_vec());
                app.state.navigation.search.active = true;
                app.state.navigation.search.input = crate::text_input::TextInput::new();
            } else if app.state.navigation.page == Page::Lyrics {
                return Ok(());
            } else {
                app.state.events.send(NavigationEvent::SearchActivated);
            }
        }
        KeyCode::Char('b' | 'B') => {
            app.state.events.send(CommandEvent::ToggleBordered);
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
        KeyCode::Char('S') => {
            if let Some(song) = app.playback.current_song() {
                app.state.events.send(PlaybackEvent::LikeSong(song.id));
                app.toast(format!("♥  {}", song.name));
            }
        }
        KeyCode::Char('s') => {
            if let ContentState::Songs(songs) = app.state.navigation.content.as_ref() {
                let sel = app.state.navigation.content_selected;
                if let Some(song) = songs.get(sel) {
                    app.state.events.send(PlaybackEvent::LikeSong(song.id));
                    app.toast(format!("♥  {}", song.name));
                }
            }
        }
        KeyCode::Char('a' | 'A') => {
            let song =/* if app.state.navigation.page == Page::Playlist {
                app.playback
                    .song_at(app.state.navigation.playlist_selected)
                    .cloned()
            } else */ if let ContentState::Songs(songs) = app.state.navigation.content.as_ref() {
                songs
                    .get(app.state.navigation.content_selected)
                    .cloned()
                    .map(Arc::new)
            } else {
                None
            };
            if let Some(song) = song {
                app.playback.add_next(song.clone());
                app.toast(format!("⏭  下一首: {}", song.name));
            }
        }
        KeyCode::Char('d' | 'D') => {
            if is_daily_recommend(app)
                && let ContentState::Songs(songs) = app.state.navigation.content.as_ref()
            {
                let sel = app.state.navigation.content_selected;
                if let Some(song) = songs.get(sel) {
                    app.state.events.send(PlaybackEvent::DislikeSong(song.id));
                    app.toast(format!("✕  {}", song.name));
                }
            }
        }
        KeyCode::Char('u' | 'U') if is_download_view(app) || is_local_music_view(app) => {
            let sel = app.state.navigation.content_selected;
            app.state
                .events
                .send(NavigationEvent::UploadCachedSong(sel));
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn handle_main_mouse(app: &mut App, kind: MouseEventKind, col: u16, row: u16) {
    // Volume scroll: if mouse is over playerbar area
    let area = app.state.playerbar_area;
    if row >= area.y && row < area.y + area.height && col >= area.x && col < area.x + area.width {
        let vol = app.playback.state.volume;
        match kind {
            MouseEventKind::ScrollUp => {
                let new = (vol + 0.05).clamp(0.0, 1.0);
                app.playback.set_volume(new);
                app.toast(format!("   {:.0}%", new * 100.0));
            }
            MouseEventKind::ScrollDown => {
                let new = (vol - 0.05).clamp(0.0, 1.0);
                app.playback.set_volume(new);
                app.toast(format!("   {:.0}%", new * 100.0));
            }
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
}

fn current_api(app: &App) -> Option<&str> {
    app.state
        .navigation
        .nav
        .sections
        .get(app.state.navigation.nav.focus_section)
        .and_then(|s| {
            let idx = app
                .state
                .navigation
                .nav
                .section_states
                .get(app.state.navigation.nav.focus_section)?
                .selected()?;
            s.items.get(idx)
        })
        .and_then(|item| item.api.as_deref())
}

fn is_daily_recommend(app: &App) -> bool {
    current_api(app) == Some("recommend_songs")
}

fn is_download_view(app: &App) -> bool {
    current_api(app) == Some("__download__")
}

fn is_local_music_view(app: &App) -> bool {
    current_api(app) == Some("__local_music__")
}
