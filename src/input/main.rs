use crossterm::event::{KeyCode, KeyEvent, MouseEventKind};

use crate::{
    app::App,
    event::{AppEvent, CommandEvent, NavigationEvent, PlaybackEvent},
    playback::mode_icon,
    service::ApiEndpoint,
    state::{ContentState, Page, TableMode},
    text_input::TextInput,
};

pub(super) fn handle_main_key(app: &mut App, key_event: KeyEvent) -> eyre::Result<()> {
    match key_event.code {
        KeyCode::Esc => {
            app.state.events.send(NavigationEvent::ContentRestore);
        }
        KeyCode::Char('q') => app.state.events.send(AppEvent::Quit),
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
            } else if app.state.navigation.table_mode == TableMode::Cell {
                app.enter_selected_cell();
            } else {
                app.enter_selected_row();
            }
        }
        KeyCode::Left => {
            if app.state.navigation.table_mode == TableMode::Cell
                && app.state.navigation.page != Page::Playlist
            {
                app.cell_select_prev_column();
            } else if app.playback.current_song().is_some() {
                let interval = app.config.seek_interval_secs as f64;
                app.playback.seek_relative(-interval);
            }
        }
        KeyCode::Right => {
            if app.state.navigation.table_mode == TableMode::Cell
                && app.state.navigation.page != Page::Playlist
            {
                app.cell_select_next_column();
            } else if app.playback.current_song().is_some() {
                let interval = app.config.seek_interval_secs as f64;
                app.playback.seek_relative(interval);
            }
        }
        KeyCode::Char('l') => {
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
            app.toggle_table_mode();
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
                app.state.navigation.search.input = TextInput::new();
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
                    app.toast(format!("\u{f03e4}  {}", song.name));
                } else {
                    app.toast(format!("\u{f040a}  {}", song.name));
                }
            }
        }
        KeyCode::Char('m') => {
            let mode = app.playback.cycle_mode();
            let (icon, label) = mode_icon(&mode);
            app.toast(format!("{icon} 循环: {label}"));
        }
        KeyCode::Char('S') => {
            if let Some(song) = app.playback.current_song() {
                app.state
                    .events
                    .send(PlaybackEvent::LikeSong(song.id, true));
                app.toast(format!("♥  {}", song.name));
            }
        }
        KeyCode::Char('s') => {
            if let ContentState::Songs(songs) = app.state.navigation.content.as_ref() {
                let sel = app.state.navigation.content_selected;
                if let Some(song) = songs.get(sel) {
                    app.state
                        .events
                        .send(PlaybackEvent::LikeSong(song.id, true));
                    app.toast(format!("♥  {}", song.name));
                }
            }
        }
        KeyCode::Char('a' | 'A') => {
            let song = if let ContentState::Songs(songs) = app.state.navigation.content.as_ref() {
                songs.get(app.state.navigation.content_selected).cloned()
            } else {
                None
            };
            if let Some(song) = song {
                app.playback.add_next(song.clone());
                app.toast(format!("⏭  下一首: {}", song.name));
            }
        }
        KeyCode::Char('d') => {
            if app.selected_endpoint() == Some(ApiEndpoint::RecommendSongs)
                && let ContentState::Songs(songs) = app.state.navigation.content.as_ref()
            {
                let sel = app.state.navigation.content_selected;
                if let Some(song) = songs.get(sel) {
                    app.state.events.send(PlaybackEvent::DislikeSong(song.id));
                    app.toast(format!("✕  {}", song.name));
                }
            } else if let ContentState::Songs(songs) = app.state.navigation.content.as_ref() {
                let sel = app.state.navigation.content_selected;
                if let Some(song) = songs.get(sel) {
                    app.state
                        .events
                        .send(PlaybackEvent::LikeSong(song.id, false));
                    app.toast(format!("♡ 已取消喜欢: {}", song.name));
                }
            }
        }
        KeyCode::Char('D') => {
            if let Some(song) = app.playback.current_song() {
                app.state
                    .events
                    .send(PlaybackEvent::LikeSong(song.id, false));
                app.toast(format!("♡ 已取消喜欢: {}", song.name));
            }
        }
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
            app.state
                .events
                .send(NavigationEvent::UploadCachedSong(sel));
        }
        KeyCode::Char('+' | '=') => {
            app.adjust_volume(0.05);
        }
        KeyCode::Char('-' | '_') => {
            app.adjust_volume(-0.05);
        }
        KeyCode::Char('[') => {
            app.adjust_speed(-crate::playback::SPEED_STEP);
        }
        KeyCode::Char(']') => {
            app.adjust_speed(crate::playback::SPEED_STEP);
        }
        KeyCode::Char('t' | 'T') => {
            app.toggle_pitch_locked();
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
