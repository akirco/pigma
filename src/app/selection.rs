//! Selection state transitions for the content table and the playback queue.
//!
//! These are the actions the key/mouse handlers trigger; they live here (next
//! to the `App` they mutate) rather than in `input`, which only maps events.

use std::time::{Duration, Instant};

use ratatui::layout::Rect;

use super::App;
use crate::{
    event::{NavigationEvent, PlaybackEvent},
    state::ContentState,
};

/// Two presses on the same cell closer together than this count as a
/// double-click; terminals report presses, not clicks.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

fn is_double_click(last: Option<(Instant, u16, u16)>, col: u16, row: u16) -> bool {
    last.is_some_and(|(at, x, y)| at.elapsed() < DOUBLE_CLICK && (x, y) == (col, row))
}

/// Song index under the pointer in the queue table. `body` includes the header
/// row at its top, and `offset` is the first visible song.
fn queue_index(body: Rect, offset: usize, len: usize, col: u16, row: u16) -> Option<usize> {
    if len == 0 || !body.contains((col, row).into()) {
        return None;
    }
    let line = (row - body.y) as usize;
    if line == 0 {
        return None;
    }
    let index = offset + line - 1;
    (index < len).then_some(index)
}

impl App {
    fn content_item_count(&self) -> usize {
        self.state.navigation.content.len()
    }

    /// Up/down move through the content in reading order: one item at a time
    /// in a card grid (left, right, left, right…), wrapping around only in the
    /// single-column table. A grid stops at its first/last card because there
    /// is no "row after the last one" to clamp onto.
    /// `check_load_more` doubles as the tail trigger for paged endpoints.
    pub(crate) fn content_select_prev(&mut self) {
        let count = self.content_item_count();
        if count == 0 {
            return;
        }
        let cols = self.state.navigation.content_columns.max(1);
        let sel = &mut self.state.navigation.content_selected;
        *sel = if cols > 1 {
            sel.saturating_sub(1)
        } else {
            (*sel + count - 1) % count
        };
        self.state.navigation.table_state.select(Some(*sel));
        self.check_load_more(count);
    }

    pub(crate) fn content_select_next(&mut self) {
        let count = self.content_item_count();
        if count == 0 {
            return;
        }
        let cols = self.state.navigation.content_columns.max(1);
        let sel = &mut self.state.navigation.content_selected;
        *sel = if cols > 1 {
            (*sel + 1).min(count - 1)
        } else {
            (*sel + 1) % count
        };
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
        self.check_load_more(count);
    }

    /// Ask for the next page when the selection approaches the loaded tail.
    fn check_load_more(&mut self, count: usize) {
        if let Some(ref mut pg) = self.state.navigation.pagination {
            let sel = self.state.navigation.content_selected;
            if !pg.loading && pg.has_more && sel >= count.saturating_sub(10) {
                pg.loading = true;
                self.events.send(NavigationEvent::LoadMore);
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

    /// A second press on the same cell within [`DOUBLE_CLICK`] activates the
    /// card under the pointer, the same action as Enter. The pointer must have
    /// been over a card in the last frame.
    pub(crate) fn content_double_click(&mut self, col: u16, row: u16) {
        if !is_double_click(self.state.last_click, col, row) {
            self.state.last_click = Some((Instant::now(), col, row));
            return;
        }
        self.state.last_click = None;
        let Some(index) = self.state.navigation.content_hovered else {
            return;
        };
        self.state.navigation.content_selected = index;
        self.state.navigation.table_state.select(Some(index));
        self.enter_selected_row();
    }

    pub(crate) fn playlist_play_selected(&mut self) {
        let idx = self.state.navigation.playlist_selected;
        self.playback.play_index(idx);
    }

    /// A second press on a queue row plays that song.
    pub(crate) fn queue_double_click(&mut self, col: u16, row: u16) {
        if !is_double_click(self.state.last_click, col, row) {
            self.state.last_click = Some((Instant::now(), col, row));
            return;
        }
        self.state.last_click = None;
        let nav = &self.state.navigation;
        let Some(index) = queue_index(
            nav.queue_body,
            nav.queue_offset,
            self.playback.queue_len(),
            col,
            row,
        ) else {
            return;
        };
        self.state.navigation.playlist_selected = index;
        self.playback.play_index(index);
    }

    /// Enter on a row: open the selected playlist/singer/keyword or play the
    /// selected song.
    pub(crate) fn enter_selected_row(&mut self) {
        let sel = self.state.navigation.content_selected;
        match self.state.navigation.content.as_ref() {
            ContentState::SongLists(lists) => {
                if let Some(list) = lists.get(sel) {
                    self.events.send(NavigationEvent::PlaylistSelect {
                        id: list.id,
                        name: Some(list.name.clone()),
                    });
                }
            }
            ContentState::TopLists(lists) => {
                if let Some(list) = lists.get(sel) {
                    self.events.send(NavigationEvent::PlaylistSelect {
                        id: list.id,
                        name: Some(list.name.clone()),
                    });
                }
            }
            ContentState::Songs(songs) => {
                if let Some(song) = songs.get(sel) {
                    self.events.send(PlaybackEvent::SongPlay(song.id));
                }
            }
            ContentState::Singers(_) => {
                self.events.send(NavigationEvent::CellAction(sel, 0));
            }
            ContentState::HotSearch(keywords) => {
                if let Some(kw) = keywords.0.get(sel) {
                    self.events.send(NavigationEvent::SearchSong(kw.clone()));
                }
            }
            _ => {}
        }
    }

    /// Enter on a cell: dispatch the column-specific cell action.
    pub(crate) fn enter_selected_cell(&mut self) {
        let sel = self.state.navigation.content_selected;
        let col = self.state.navigation.content_column_selected;
        self.events.send(NavigationEvent::CellAction(sel, col));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_click_needs_the_same_cell_within_the_window() {
        let recent = Some((Instant::now() - Duration::from_millis(120), 3, 4));
        assert!(is_double_click(recent, 3, 4));
        assert!(
            !is_double_click(recent, 3, 5),
            "a different cell is not a double-click"
        );

        let stale = Some((Instant::now() - Duration::from_millis(600), 3, 4));
        assert!(
            !is_double_click(stale, 3, 4),
            "clicks too far apart are separate"
        );
        assert!(!is_double_click(None, 3, 4));
    }

    #[test]
    fn queue_index_skips_the_header_and_applies_the_offset() {
        let body = Rect::new(2, 5, 40, 6); // header + four song rows
        assert_eq!(queue_index(body, 0, 10, 2, 5), None, "header row");
        assert_eq!(queue_index(body, 0, 10, 2, 6), Some(0));
        assert_eq!(queue_index(body, 0, 10, 2, 10), Some(4));
        assert_eq!(queue_index(body, 3, 10, 2, 6), Some(3), "scrolled offset");
        assert_eq!(queue_index(body, 0, 10, 1, 6), None, "left of the table");
        assert_eq!(queue_index(body, 0, 10, 2, 11), None, "below the table");
        assert_eq!(queue_index(body, 0, 0, 2, 6), None, "empty queue");
    }
}
