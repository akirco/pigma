use crate::event::NavigationEvent;
use crate::state::App;
use crate::state::ContentState;

pub(super) fn content_item_count(app: &App) -> usize {
    app.state.navigation.content.len()
}

pub(super) fn content_select_prev(app: &mut App) {
    let count = content_item_count(app);
    if count == 0 {
        return;
    }
    let sel = &mut app.state.navigation.content_selected;
    *sel = (*sel + count - 1) % count;
    app.state.navigation.table_state.select(Some(*sel));
    check_load_more(app, count);
}

pub(super) fn content_select_next(app: &mut App) {
    let count = content_item_count(app);
    if count == 0 {
        return;
    }
    let sel = &mut app.state.navigation.content_selected;
    *sel = (*sel + 1) % count;
    app.state.navigation.table_state.select(Some(*sel));
    check_load_more(app, count);
}

fn check_load_more(app: &mut App, count: usize) {
    if let Some(ref mut pg) = app.state.navigation.pagination {
        let sel = app.state.navigation.content_selected;
        if !pg.loading && pg.has_more && count >= 5 && sel >= count.saturating_sub(5) {
            pg.loading = true;
            app.state.events.send(NavigationEvent::LoadMore);
        }
    }
}

pub(super) fn playlist_select_prev(app: &mut App) {
    let len = app.playback.queue_len();
    let sel = &mut app.state.navigation.playlist_selected;
    if len > 0 {
        *sel = (*sel + len - 1) % len;
    }
}

pub(super) fn playlist_select_next(app: &mut App) {
    let len = app.playback.queue_len();
    let sel = &mut app.state.navigation.playlist_selected;
    if len > 0 {
        *sel = (*sel + 1) % len;
    }
}

pub(super) fn playlist_play_selected(app: &mut App) {
    let idx = app.state.navigation.playlist_selected;
    app.playback.play_index(idx);
}

pub(super) fn row_enter_action(app: &mut App) {
    let sel = app.state.navigation.content_selected;
    match app.state.navigation.content.as_ref() {
        ContentState::SongLists(lists) => {
            if let Some(list) = lists.get(sel) {
                app.state.events.send(NavigationEvent::PlaylistSelect {
                    id: list.id,
                    name: Some(list.name.clone()),
                });
            }
        }
        ContentState::TopLists(lists) => {
            if let Some(list) = lists.get(sel) {
                app.state.events.send(NavigationEvent::PlaylistSelect {
                    id: list.id,
                    name: Some(list.name.clone()),
                });
            }
        }
        ContentState::Songs(songs) => {
            if let Some(song) = songs.get(sel) {
                app.state
                    .events
                    .send(crate::event::PlaybackEvent::SongPlay(song.id));
            }
        }
        ContentState::Singers(_) => {
            app.state.events.send(NavigationEvent::CellAction(sel, 0));
        }
        ContentState::HotSearch(keywords) => {
            if let Some(kw) = keywords.get(sel) {
                app.state
                    .events
                    .send(NavigationEvent::SearchSong(kw.clone()));
            }
        }
        _ => {}
    }
}

pub(super) fn cell_enter_action(app: &mut App) {
    let sel = app.state.navigation.content_selected;
    let col = app.state.navigation.content_column_selected;
    app.state.events.send(NavigationEvent::CellAction(sel, col));
}
