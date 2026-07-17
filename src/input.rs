use crate::event::AppEvent;
use crate::state::{App, LoginField, LoginMethod, Page};
use crate::types::{CommandPanelAction, ContentState, TableMode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEventKind};

pub fn handle_key_events(app: &mut App, key_event: KeyEvent) -> color_eyre::Result<()> {
    if app.state.navigation.page == Page::Splash {
        match key_event.code {
            KeyCode::Esc | KeyCode::Char('q') => app.state.events.send(AppEvent::Quit),
            KeyCode::Char('c' | 'C') if key_event.modifiers == KeyModifiers::CONTROL => {
                app.state.events.send(AppEvent::Quit)
            }
            _ => {
                app.state.splash.boot_complete = true;
                app.state.navigation.page = Page::Login;
            }
        }
        return Ok(());
    }

    if app.state.command_panel.open {
        match key_event.code {
            KeyCode::Esc => {
                app.state
                    .events
                    .send(AppEvent::CommandPanel(CommandPanelAction::Close));
            }
            KeyCode::Up => {
                app.state
                    .events
                    .send(AppEvent::CommandPanel(CommandPanelAction::Previous));
            }
            KeyCode::Down => {
                app.state
                    .events
                    .send(AppEvent::CommandPanel(CommandPanelAction::Next));
            }
            KeyCode::Enter => {
                app.state
                    .events
                    .send(AppEvent::CommandPanel(CommandPanelAction::Select));
            }
            _ => {}
        }
        return Ok(());
    }

    if app.state.navigation.page == Page::Login {
        if key_event.modifiers == KeyModifiers::CONTROL
            && matches!(key_event.code, KeyCode::Char('c' | 'C'))
        {
            app.state.events.send(AppEvent::Quit);
            return Ok(());
        }
        if key_event.modifiers == KeyModifiers::CONTROL
            && matches!(key_event.code, KeyCode::Char('p' | 'P'))
        {
            app.state
                .events
                .send(AppEvent::CommandPanel(CommandPanelAction::Open));
            return Ok(());
        }

        let login = &mut app.state.navigation.login;

        match key_event.code {
            KeyCode::Tab => {
                login.focus = match login.focus {
                    LoginField::Method => LoginField::Username,
                    LoginField::Username => LoginField::Password,
                    LoginField::Password => LoginField::Method,
                };
            }
            KeyCode::BackTab => {
                login.focus = match login.focus {
                    LoginField::Method => LoginField::Password,
                    LoginField::Username => LoginField::Method,
                    LoginField::Password => LoginField::Username,
                };
            }
            KeyCode::Left => {
                if login.focus == LoginField::Method {
                    login.selected_method =
                        LoginMethod::from_index((login.selected_method.index() + 2) % 3);
                } else if login.focus == LoginField::Username {
                    login.username.move_left();
                } else if login.focus == LoginField::Password {
                    login.password.move_left();
                }
            }
            KeyCode::Right => {
                if login.focus == LoginField::Method {
                    login.selected_method =
                        LoginMethod::from_index((login.selected_method.index() + 1) % 3);
                } else if login.focus == LoginField::Username {
                    login.username.move_right();
                } else if login.focus == LoginField::Password {
                    login.password.move_right();
                }
            }
            KeyCode::Char(c) => {
                if login.focus == LoginField::Username {
                    login.username.enter_char(c);
                } else if login.focus == LoginField::Password {
                    login.password.enter_char(c);
                }
            }
            KeyCode::Backspace => {
                if login.focus == LoginField::Username {
                    login.username.delete_char();
                } else if login.focus == LoginField::Password {
                    login.password.delete_char();
                }
            }
            KeyCode::Enter => {
                app.state.events.send(AppEvent::Login);
            }
            KeyCode::Esc => app.state.events.send(AppEvent::Quit),
            _ => {}
        }
        return Ok(());
    }

    if app.state.navigation.search.active {
        match key_event.code {
            KeyCode::Esc => {
                app.state.events.send(AppEvent::SearchDeactivated);
                return Ok(());
            }
            KeyCode::Enter => {
                if app.state.navigation.search.filter_queue_only {
                    let sel = app.state.navigation.playlist_selected;
                    let song_id = app.playback.queue.songs.get(sel).map(|s| s.id);
                    app.playback.queue.songs = app
                        .state
                        .navigation
                        .search
                        .unfiltered_songs
                        .take()
                        .unwrap_or_default();
                    app.state.navigation.search.active = false;
                    app.state.navigation.search.filter_queue_only = false;
                    app.state.navigation.search.unfiltered_songs = None;
                    app.state.navigation.playlist_selected = 0;
                    if let Some(id) = song_id
                        && let Some(pos) = app.playback.queue.songs.iter().position(|s| s.id == id)
                    {
                        app.playback.play_index(pos);
                    }
                } else {
                    let keyword = app.state.navigation.search.input.value.clone();
                    if !keyword.is_empty() {
                        app.state.navigation.search.active = false;
                        app.state.events.send(AppEvent::SearchSong(keyword));
                    } else if let ContentState::HotSearch(keywords) = &app.state.navigation.content
                    {
                        let sel = app.state.navigation.content_selected;
                        if let Some(kw) = keywords.get(sel) {
                            app.state.navigation.search.active = false;
                            app.state.events.send(AppEvent::SearchSong(kw.clone()));
                        }
                    }
                }
                return Ok(());
            }
            KeyCode::Left => {
                app.state.navigation.search.input.move_left();
                return Ok(());
            }
            KeyCode::Right => {
                app.state.navigation.search.input.move_right();
                return Ok(());
            }
            KeyCode::Backspace => {
                app.state.navigation.search.input.delete_char();
                if app.state.navigation.search.filter_queue_only {
                    apply_filter_queue_only(app);
                }
                return Ok(());
            }
            KeyCode::Char(c) => {
                app.state.navigation.search.input.enter_char(c);
                if app.state.navigation.search.filter_queue_only {
                    apply_filter_queue_only(app);
                }
                return Ok(());
            }
            _ => {}
        }
    }

    match key_event.code {
        KeyCode::Esc => {
            app.state.events.send(AppEvent::ContentRestore);
        }
        KeyCode::Char('q') => app.state.events.send(AppEvent::Quit),
        KeyCode::Char('c' | 'C') if key_event.modifiers == KeyModifiers::CONTROL => {
            app.state.events.send(AppEvent::Quit)
        }
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
        KeyCode::Char('p' | 'P') if key_event.modifiers == KeyModifiers::CONTROL => {
            app.state
                .events
                .send(AppEvent::CommandPanel(CommandPanelAction::Open));
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
                        app.playback.queue.current_index.unwrap_or(0);
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
                app.state.navigation.search.unfiltered_songs =
                    Some(app.playback.queue.songs.clone());
                app.state.navigation.search.active = true;
                app.state.navigation.search.input = crate::ui::text_input::TextInput::new();
            } else {
                app.state.events.send(AppEvent::SearchActivated);
            }
        }
        KeyCode::Char('b' | 'B') => {
            app.state.events.send(AppEvent::ToggleBordered);
        }
        KeyCode::Char(' ') => {
            app.playback.toggle_pause();
        }
        KeyCode::Char('r') => {
            app.playback.cycle_mode();
        }
        _ => {}
    }
    Ok(())
}

fn navigate_nav_up(app: &mut App) {
    let nav = &mut app.state.navigation.nav;
    if nav.sections.is_empty() {
        return;
    }
    let focus = nav.focus_section;
    let len = nav.sections[focus].items.len();
    if len == 0 {
        return;
    }
    let current = nav.section_states[focus].selected().unwrap_or(0);
    if current > 0 {
        nav.section_states[focus].select(Some(current - 1));
    } else {
        let section_count = nav.sections.len();
        let prev_section = (focus + section_count - 1) % section_count;
        if nav.sections[prev_section].items.is_empty() {
            return;
        }
        nav.section_states[focus].select(Some(0));
        nav.focus_section = prev_section;
        let prev_len = nav.sections[prev_section].items.len();
        nav.section_states[prev_section].select(Some(prev_len - 1));
    }
    emit_nav_select(app);
}

fn navigate_nav_down(app: &mut App) {
    let nav = &mut app.state.navigation.nav;
    if nav.sections.is_empty() {
        return;
    }
    let focus = nav.focus_section;
    let len = nav.sections[focus].items.len();
    if len == 0 {
        return;
    }
    let current = nav.section_states[focus].selected().unwrap_or(0);
    if current + 1 < len {
        nav.section_states[focus].select(Some(current + 1));
    } else {
        let next_section = (focus + 1) % nav.sections.len();
        if nav.sections[next_section].items.is_empty() {
            return;
        }
        nav.section_states[focus].select(Some(0));
        nav.focus_section = next_section;
        nav.section_states[next_section].select(Some(0));
    }
    emit_nav_select(app);
}

fn content_item_count(app: &App) -> usize {
    app.state.navigation.content.len()
}

fn content_select_prev(app: &mut App) {
    let count = content_item_count(app);
    if count == 0 {
        return;
    }
    let sel = &mut app.state.navigation.content_selected;
    *sel = (*sel + count - 1) % count;
    app.state.navigation.table_state.select(Some(*sel));
}

fn content_select_next(app: &mut App) {
    let count = content_item_count(app);
    if count == 0 {
        return;
    }
    let sel = &mut app.state.navigation.content_selected;
    *sel = (*sel + 1) % count;
    app.state.navigation.table_state.select(Some(*sel));
}

fn emit_nav_select(app: &mut App) {
    let nav = &app.state.navigation.nav;
    if nav.sections.is_empty() {
        return;
    }
    let focus = nav.focus_section;
    if let Some(selected) = nav.section_states[focus].selected()
        && let Some(api) = nav.sections[focus]
            .items
            .get(selected)
            .and_then(|i| i.api.as_ref())
    {
        app.state.events.send(AppEvent::NavSelect(api.clone()));
    }
}

fn playlist_select_prev(app: &mut App) {
    let len = app.playback.queue_len();
    let sel = &mut app.state.navigation.playlist_selected;
    if len > 0 {
        *sel = (*sel + len - 1) % len;
    }
}

fn playlist_select_next(app: &mut App) {
    let len = app.playback.queue_len();
    let sel = &mut app.state.navigation.playlist_selected;
    if len > 0 {
        *sel = (*sel + 1) % len;
    }
}

fn playlist_play_selected(app: &mut App) {
    let idx = app.state.navigation.playlist_selected;
    app.playback.play_index(idx);
}

fn apply_filter_queue_only(app: &mut App) {
    let keyword = app.state.navigation.search.input.value.to_lowercase();
    if let Some(full) = &app.state.navigation.search.unfiltered_songs {
        if keyword.is_empty() {
            app.playback.queue.songs.clone_from(full);
        } else {
            let filtered: Vec<_> = full
                .iter()
                .filter(|s| {
                    s.name.to_lowercase().contains(&keyword)
                        || s.singer.to_lowercase().contains(&keyword)
                })
                .cloned()
                .collect();
            app.playback.queue.songs = filtered;
        }
        app.state.navigation.playlist_selected = 0;
    }
}

fn toggle_table_mode(app: &mut App) {
    let nav = &mut app.state.navigation;
    match nav.table_mode {
        TableMode::Row => {
            nav.table_mode = TableMode::Cell;
            let columns = app.config.columns.for_content(&nav.content, None).to_vec();
            let col = next_selectable_column(0, &columns, &nav.content);
            if let Some(c) = col {
                nav.content_column_selected = c;
                nav.table_state.select_first_column();
                for _ in 0..c {
                    nav.table_state.select_next_column();
                }
            } else {
                nav.table_mode = TableMode::Row;
            }
        }
        TableMode::Cell => {
            nav.table_mode = TableMode::Row;
            nav.content_column_selected = 0;
            nav.table_state.select_first();
        }
    }
}

fn is_selectable_field(content: &ContentState, field: &str) -> bool {
    matches!(
        (content, field),
        (ContentState::Songs(_), "album" | "singer") | (ContentState::Singers(_), "name")
    )
}

fn next_selectable_column(
    from: usize,
    columns: &[crate::types::ColumnDef],
    content: &ContentState,
) -> Option<usize> {
    let n = columns.len();
    for i in 0..n {
        let idx = (from + i) % n;
        if is_selectable_field(content, &columns[idx].field) {
            return Some(idx);
        }
    }
    None
}

fn cell_select_prev_column(app: &mut App) {
    let nav = &mut app.state.navigation;
    let columns = app.config.columns.for_content(&nav.content, None);
    let n = columns.len();
    if n == 0 {
        return;
    }
    let current = nav.content_column_selected;
    for i in 1..=n {
        let idx = (current + n - i) % n;
        if is_selectable_field(&nav.content, &columns[idx].field) {
            nav.content_column_selected = idx;
            nav.table_state.select_first_column();
            for _ in 0..idx {
                nav.table_state.select_next_column();
            }
            return;
        }
    }
}

fn cell_select_next_column(app: &mut App) {
    let nav = &mut app.state.navigation;
    let columns = app.config.columns.for_content(&nav.content, None);
    let n = columns.len();
    if n == 0 {
        return;
    }
    let current = nav.content_column_selected;
    for i in 1..=n {
        let idx = (current + i) % n;
        if is_selectable_field(&nav.content, &columns[idx].field) {
            nav.content_column_selected = idx;
            nav.table_state.select_first_column();
            for _ in 0..idx {
                nav.table_state.select_next_column();
            }
            return;
        }
    }
}

fn row_enter_action(app: &mut App) {
    let sel = app.state.navigation.content_selected;
    match &app.state.navigation.content {
        ContentState::SongLists(lists) => {
            if let Some(list) = lists.get(sel) {
                app.state.events.send(AppEvent::PlaylistSelect(list.id));
            }
        }
        ContentState::TopLists(lists) => {
            if let Some(list) = lists.get(sel) {
                app.state.events.send(AppEvent::PlaylistSelect(list.id));
            }
        }
        ContentState::Songs(songs) => {
            if let Some(song) = songs.get(sel) {
                app.state.events.send(AppEvent::SongPlay(song.id));
            }
        }
        ContentState::Singers(_) => {
            app.state.events.send(AppEvent::CellAction(sel, 0));
        }
        ContentState::HotSearch(keywords) => {
            if let Some(kw) = keywords.get(sel) {
                app.state.events.send(AppEvent::SearchSong(kw.clone()));
            }
        }
        _ => {}
    }
}

fn cell_enter_action(app: &mut App) {
    let sel = app.state.navigation.content_selected;
    let col = app.state.navigation.content_column_selected;
    app.state.events.send(AppEvent::CellAction(sel, col));
}

pub fn handle_mouse_event(app: &mut App, kind: MouseEventKind) {
    if app.state.command_panel.open {
        match kind {
            MouseEventKind::ScrollUp => {
                app.state
                    .events
                    .send(AppEvent::CommandPanel(CommandPanelAction::Previous));
            }
            MouseEventKind::ScrollDown => {
                app.state
                    .events
                    .send(AppEvent::CommandPanel(CommandPanelAction::Next));
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
