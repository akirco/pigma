use std::{borrow::Cow, ops::Range};

use ncm_api::{SingerInfo, SongInfo, SongList, TopList};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, TableState},
};

use super::{BlockStyle, scrollbar::calc_scroll_offset, skeleton::Skeleton, table};
use crate::{
    config::{ColumnDef, ColumnsConfig, Theme},
    state::{ContentState, TableMode},
    time::format_duration,
};

const MISSING: &str = "—";

thread_local! {
    static WARNED_FIELDS: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// Warn once per process for an unknown column field. Rendering runs every
/// frame on the main thread, so a per-call `HashSet` would re-log on every
/// frame; the `thread_local` keeps it deduplicated across frames.
fn warn_missing_field(field: &str) {
    WARNED_FIELDS.with(|warned| {
        let mut warned = warned.borrow_mut();
        if !warned.contains(field) {
            log::warn!("Missing field: \"{field}\" — showing \"{MISSING}\"");
            warned.insert(field.to_string());
        }
    });
}

/// Look up a field value for a table row by its column field name.
/// Returns `None` for unknown fields (rendered as "—").
fn song_field<'a>(song: &'a SongInfo, field: &str) -> Option<Cow<'a, str>> {
    match field {
        "name" => Some(Cow::Borrowed(&song.name)),
        "singer" => Some(Cow::Borrowed(&song.singer)),
        "album" => Some(Cow::Borrowed(&song.album)),
        "duration" => Some(Cow::Owned(format_duration(song.duration))),
        "id" => Some(Cow::Owned(song.id.to_string())),
        _ => None,
    }
}

fn songlist_field<'a>(list: &'a SongList, field: &str) -> Option<Cow<'a, str>> {
    match field {
        "name" => Some(Cow::Borrowed(&list.name)),
        "author" => Some(Cow::Borrowed(&list.author)),
        "id" => Some(Cow::Owned(list.id.to_string())),
        _ => None,
    }
}

fn toplist_field<'a>(list: &'a TopList, field: &str) -> Option<Cow<'a, str>> {
    match field {
        "name" => Some(Cow::Borrowed(&list.name)),
        "description" => Some(Cow::Borrowed(&list.description)),
        "id" => Some(Cow::Owned(list.id.to_string())),
        _ => None,
    }
}

fn singer_field<'a>(singer: &'a SingerInfo, field: &str) -> Option<Cow<'a, str>> {
    match field {
        "name" => Some(Cow::Borrowed(&singer.name)),
        "id" => Some(Cow::Owned(singer.id.to_string())),
        _ => None,
    }
}

/// Build table rows directly from a slice of items, borrowing each field into
/// `Cell` instead of materializing a `String` per cell. Borrowed fields (e.g.
/// `&song.name`) stay borrowed; only derived fields (`duration`, `id`) allocate.
fn build_rows<'a, I>(
    items: &'a [I],
    columns: &'a [ColumnDef],
    colors: &'a Theme,
    lookup: impl Fn(&'a I, &str) -> Option<Cow<'a, str>>,
) -> Vec<Row<'a>> {
    items
        .iter()
        .map(|item| {
            Row::new(columns.iter().map(|col| match lookup(item, &col.field) {
                Some(value) => Cell::from(value).style(Style::default().fg(colors.muted)),
                None => {
                    warn_missing_field(&col.field);
                    Cell::from(MISSING).style(Style::default().fg(colors.error))
                }
            }))
            .height(1)
        })
        .collect()
}

fn build_content_rows<'a>(
    content: &'a ContentState,
    columns: &'a [ColumnDef],
    colors: &'a Theme,
    range: Range<usize>,
) -> Vec<Row<'a>> {
    match content {
        ContentState::Songs(songs) => build_rows(&songs[range], columns, colors, |song, field| {
            song_field(song, field)
        }),
        ContentState::SongLists(lists) => {
            build_rows(&lists[range], columns, colors, songlist_field)
        }
        ContentState::TopLists(lists) => build_rows(&lists[range], columns, colors, toplist_field),
        ContentState::HotSearch(keywords) => {
            build_rows(&keywords.0[range], columns, colors, |kw, field| {
                if field == "keyword" {
                    Some(Cow::Borrowed(kw.as_str()))
                } else {
                    None
                }
            })
        }
        ContentState::Singers(singers) => {
            build_rows(&singers[range], columns, colors, singer_field)
        }
        _ => vec![],
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_content(
    f: &mut Frame,
    content: &ContentState,
    columns: &ColumnsConfig,
    api: Option<&str>,
    bs: &BlockStyle<'_>,
    table_state: &mut TableState,
    content_selected: usize,
    table_mode: TableMode,
    area: Rect,
) {
    let colors = bs.colors;
    match content {
        ContentState::Empty => {
            let text = Line::from(Span::styled("", Style::default().fg(colors.muted)));
            f.render_widget(Paragraph::new(text), area);
        }
        ContentState::Loading => {
            f.render_widget(Skeleton::new().bg(colors.bg).surface(colors.surface), area);
        }
        ContentState::Error(e) => {
            let text = Line::from(Span::styled(
                format!("错误: {e}"),
                Style::default().fg(colors.error),
            ));
            f.render_widget(Paragraph::new(text), area);
        }
        _ => {
            let cols = columns.for_content(content.content_type(), api);
            let total = content.len();
            let sel = content_selected.min(total.saturating_sub(1));
            // Only materialize the visible window of rows (header takes one row)
            // instead of rebuilding the whole list every frame.
            let visible = area.height.saturating_sub(1).max(1) as usize;
            let offset = calc_scroll_offset(sel, visible, total);
            let end = (offset + visible).min(total);
            let rows = build_content_rows(content, cols, colors, offset..end);
            // The window is pre-scrolled, so selection and offset are relative to
            // it; ratatui recomputes the offset from these during render.
            table_state.select(Some(sel.saturating_sub(offset)));
            *table_state.offset_mut() = 0;
            table::render_table(
                f,
                cols,
                rows,
                table_state,
                table_mode,
                colors,
                area,
                total,
                sel,
            );
        }
    }
}
