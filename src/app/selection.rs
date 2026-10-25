//! Selection state transitions for the content table and the playback queue.
//!
//! These are the actions the key/mouse handlers trigger; they live here (next
//! to the `App` they mutate) rather than in `input`, which only maps events.

use super::App;
use crate::{
    event::{NavigationEvent, PlaybackEvent},
    state::ContentState,
};

impl App {
    fn content_item_count(&self) -> usize {
        self.state.navigation.content.len()
    }

    pub(crate) fn content_select_prev(&mut self) {
        let count = self.content_item_count();
        if count == 0 {
            return;
        }
        let sel = &mut self.state.navigation.content_selected;
        *sel = (*sel + count - 1) % count;
        self.state.navigation.table_state.select(Some(*sel));
        self.check_load_more(count);
    }

    pub(crate) fn content_select_next(&mut self) {
        let count = self.content_item_count();
        if count == 0 {
            return;
        }
        let sel = &mut self.state.navigation.content_selected;
        *sel = (*sel + 1) % count;
        self.state.navigation.table_state.select(Some(*sel));
        self.check_load_more(count);
    }

    pub(crate) fn content_select_first(&mut self) {
        if self.content_item_count() == 0 {
            return;
        }
        self.state.navigation.content_selected = 0;
        self.state.navigation.table_state.select(Some(0));
    }

    pub(crate) fn content_select_last(&mut self) {
        let count = self.content_item_count();
        if count == 0 {
            return;
        }
        self.state.navigation.content_selected = count - 1;
        self.state.navigation.table_state.select(Some(count - 1));
    }

    /// Ask for the next page when the selection approaches the loaded tail.
    fn check_load_more(&mut self, count: usize) {
        if let Some(ref mut pg) = self.state.navigation.pagination {
            let sel = self.state.navigation.content_selected;
            if !pg.loading && pg.has_more && count >= 10 && sel >= count.saturating_sub(10) {
                pg.loading = true;
                self.state.events.send(NavigationEvent::LoadMore);
            }
        }
    }

    pub(crate) fn playlist_select_prev(&mut self) {
        let len = self.playback.queue_len();
        let sel = &mut self.state.navigation.playlist_selected;
        if len > 0 {
            *sel = (*sel + len - 1) % len;
        }
    }

    pub(crate) fn playlist_select_next(&mut self) {
        let len = self.playback.queue_len();
        let sel = &mut self.state.navigation.playlist_selected;
        if len > 0 {
            *sel = (*sel + 1) % len;
        }
    }

    pub(crate) fn playlist_select_first(&mut self) {
        if self.playback.queue_len() > 0 {
            self.state.navigation.playlist_selected = 0;
        }
    }

    pub(crate) fn playlist_select_last(&mut self) {
        let len = self.playback.queue_len();
        if len > 0 {
            self.state.navigation.playlist_selected = len - 1;
        }
    }

    pub(crate) fn playlist_play_selected(&mut self) {
        let idx = self.state.navigation.playlist_selected;
        self.playback.play_index(idx);
    }

    /// Enter on a row: open the selected playlist/singer/keyword or play the
    /// selected song.
    pub(crate) fn enter_selected_row(&mut self) {
        let sel = self.state.navigation.content_selected;
        match self.state.navigation.content.as_ref() {
            ContentState::SongLists(lists) => {
                if let Some(list) = lists.get(sel) {
                    self.state.events.send(NavigationEvent::PlaylistSelect {
                        id: list.id,
                        name: Some(list.name.clone()),
                    });
                }
            }
            ContentState::TopLists(lists) => {
                if let Some(list) = lists.get(sel) {
                    self.state.events.send(NavigationEvent::PlaylistSelect {
                        id: list.id,
                        name: Some(list.name.clone()),
                    });
                }
            }
            ContentState::Songs(songs) => {
                if let Some(song) = songs.get(sel) {
                    self.state.events.send(PlaybackEvent::SongPlay(song.id));
                }
            }
            ContentState::Singers(_) => {
                self.state.events.send(NavigationEvent::CellAction(sel, 0));
            }
            ContentState::HotSearch(keywords) => {
                if let Some(kw) = keywords.0.get(sel) {
                    self.state
                        .events
                        .send(NavigationEvent::SearchSong(kw.clone()));
                }
            }
            _ => {}
        }
    }

    /// Enter on a cell: dispatch the column-specific cell action.
    pub(crate) fn enter_selected_cell(&mut self) {
        let sel = self.state.navigation.content_selected;
        let col = self.state.navigation.content_column_selected;
        self.state
            .events
            .send(NavigationEvent::CellAction(sel, col));
    }
}
