//! Table selection mode: row vs. cell, and moving between selectable columns.

use super::App;
use crate::{
    config::ColumnDef,
    state::{ContentState, TableMode},
};

fn is_selectable_field(content: &ContentState, field: &str) -> bool {
    matches!(
        (content, field),
        (ContentState::Songs(_), "album" | "singer") | (ContentState::Singers(_), "name")
    )
}

fn next_selectable_column(
    from: usize,
    columns: &[ColumnDef],
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

impl App {
    pub(crate) fn toggle_table_mode(&mut self) {
        let nav = &mut self.state.navigation;
        match nav.table_mode {
            TableMode::Row => {
                nav.table_mode = TableMode::Cell;
                let columns = self
                    .config
                    .columns
                    .for_content(nav.content.content_type(), None)
                    .to_vec();
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

    pub(crate) fn cell_select_prev_column(&mut self) {
        let nav = &mut self.state.navigation;
        let columns = self
            .config
            .columns
            .for_content(nav.content.content_type(), None);
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

    pub(crate) fn cell_select_next_column(&mut self) {
        let nav = &mut self.state.navigation;
        let columns = self
            .config
            .columns
            .for_content(nav.content.content_type(), None);
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
}
