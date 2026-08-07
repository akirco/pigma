use std::borrow::Cow;

use ncm_api::{SingerInfo, SongInfo, SongList, TopList};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, TableState},
};

use super::BlockStyle;
use super::table;
use crate::config::ColumnDef;
use crate::config::ColumnsConfig;
use crate::config::Theme;
use crate::state::{ContentState, TableMode};

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
        "duration" => Some(Cow::Owned(crate::utils::format_duration(song.duration))),
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
) -> Vec<Row<'a>> {
    match content {
        ContentState::Songs(songs) => build_rows(songs, columns, colors, song_field),
        ContentState::SongLists(lists) => build_rows(lists, columns, colors, songlist_field),
        ContentState::TopLists(lists) => build_rows(lists, columns, colors, toplist_field),
        ContentState::HotSearch(keywords) => {
            build_rows(&keywords.0, columns, colors, |kw, field| {
                if field == "keyword" {
                    Some(Cow::Borrowed(kw.as_str()))
                } else {
                    None
                }
            })
        }
        ContentState::Singers(singers) => build_rows(singers, columns, colors, singer_field),
        _ => vec![],
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render_content(
    f: &mut Frame,
    content: &ContentState,
    columns: &ColumnsConfig,
    api: Option<&str>,
    bs: &BlockStyle<'_>,
    table_state: &mut TableState,
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
            let text = Line::from(Span::styled("加载中...", Style::default().fg(colors.muted)));
            f.render_widget(Paragraph::new(text), area);
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
            let rows = build_content_rows(content, cols, colors);
            table::render_table(f, cols, rows, table_state, table_mode, colors, area);
        }
    }
}
