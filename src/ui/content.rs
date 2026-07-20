use ncm_api::{SingerInfo, SongInfo, SongList, TopList};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, TableState},
};

use super::table;
use crate::config::ColumnDef;
use crate::config::ColumnsConfig;
use crate::config::Theme;
use crate::state::{ContentState, TableMode};

/// Look up a field value for a table row by its column field name.
/// Returns `None` for unknown fields (rendered as "—").
fn song_field(song: &SongInfo, field: &str) -> Option<String> {
    match field {
        "name" => Some(song.name.clone()),
        "singer" => Some(song.singer.clone()),
        "album" => Some(song.album.clone()),
        "duration" => Some(crate::utils::format_duration(song.duration)),
        "id" => Some(song.id.to_string()),
        _ => None,
    }
}

fn songlist_field(list: &SongList, field: &str) -> Option<String> {
    match field {
        "name" => Some(list.name.clone()),
        "author" => Some(list.author.clone()),
        "id" => Some(list.id.to_string()),
        _ => None,
    }
}

fn toplist_field(list: &TopList, field: &str) -> Option<String> {
    match field {
        "name" => Some(list.name.clone()),
        "description" => Some(list.description.clone()),
        "id" => Some(list.id.to_string()),
        _ => None,
    }
}

fn singer_field(singer: &SingerInfo, field: &str) -> Option<String> {
    match field {
        "name" => Some(singer.name.clone()),
        "id" => Some(singer.id.to_string()),
        _ => None,
    }
}

/// Build table rows directly from a slice of items, avoiding the intermediate
/// `HashMap` allocation that the old `compute_rows` path performed per row.
fn build_rows<I>(
    items: &[I],
    columns: &[ColumnDef],
    lookup: impl Fn(&I, &str) -> Option<String>,
) -> Vec<Vec<String>> {
    let mut warned = std::collections::HashSet::new();
    items
        .iter()
        .map(|item| {
            columns
                .iter()
                .map(|col| {
                    lookup(item, &col.field).unwrap_or_else(|| {
                        if !warned.contains(&col.field) {
                            log::warn!("Missing field: \"{}\" — showing \"—\"", col.field);
                            warned.insert(col.field.clone());
                        }
                        "—".to_string()
                    })
                })
                .collect()
        })
        .collect()
}

fn compute_rows(content: &ContentState, columns: &[ColumnDef]) -> Vec<Vec<String>> {
    match content {
        ContentState::Songs(songs) => build_rows(songs, columns, song_field),
        ContentState::SongLists(lists) => build_rows(lists, columns, songlist_field),
        ContentState::TopLists(lists) => build_rows(lists, columns, toplist_field),
        ContentState::HotSearch(keywords) => build_rows(keywords, columns, |kw, field| {
            if field == "keyword" {
                Some(kw.clone())
            } else {
                None
            }
        }),
        ContentState::Singers(singers) => build_rows(singers, columns, singer_field),
        _ => vec![],
    }
}

pub fn render_content(
    f: &mut Frame,
    content: &ContentState,
    columns: &ColumnsConfig,
    api: Option<&str>,
    cache: &std::cell::RefCell<Option<Vec<Vec<String>>>>,
    colors: &Theme,
    table_state: &mut TableState,
    table_mode: TableMode,
    area: Rect,
) {
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
            if cache.borrow().is_none() {
                let rows = compute_rows(content, cols);
                *cache.borrow_mut() = Some(rows);
            }
            let rows = cache.borrow();
            table::render_table(
                f,
                cols,
                rows.as_deref().unwrap_or(&[]),
                table_state,
                table_mode,
                colors,
                area,
            );
        }
    }
}
