use std::cell::RefCell;
use std::collections::HashMap;

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::table;
use crate::config::ColumnsConfig;
use crate::field::to_map;
use crate::theme::Theme;
use crate::types::{ColumnDef, ContentState};

fn compute_rows(content: &ContentState, columns: &[ColumnDef]) -> Vec<Vec<String>> {
    match content {
        ContentState::Songs(songs) => {
            let mut maps: Vec<HashMap<String, String>> = songs.iter().map(to_map).collect();
            format_duration_fields(&mut maps);
            table::build_rows(&maps, columns)
        }
        ContentState::SongLists(lists) => {
            let maps: Vec<HashMap<String, String>> = lists.iter().map(to_map).collect();
            table::build_rows(&maps, columns)
        }
        ContentState::TopLists(lists) => {
            let maps: Vec<HashMap<String, String>> = lists.iter().map(to_map).collect();
            table::build_rows(&maps, columns)
        }
        ContentState::HotSearch(keywords) => {
            let maps: Vec<HashMap<String, String>> = keywords
                .iter()
                .map(|kw| {
                    let mut m = HashMap::new();
                    m.insert("keyword".into(), kw.clone());
                    m
                })
                .collect();
            table::build_rows(&maps, columns)
        }
        _ => vec![],
    }
}

/// Convert raw millisecond duration values to "MM:SS" format.
fn format_duration_fields(maps: &mut [HashMap<String, String>]) {
    for map in maps.iter_mut() {
        if let Some(v) = map.get("duration")
            && let Ok(ms) = v.parse::<u64>()
        {
            let total_secs = ms / 1000;
            let mins = total_secs / 60;
            let secs = total_secs % 60;
            map.insert("duration".into(), format!("{:02}:{:02}", mins, secs));
        }
    }
}

pub fn render_content(
    f: &mut Frame,
    content: &ContentState,
    columns: &ColumnsConfig,
    api: Option<&str>,
    cache: &RefCell<Option<Vec<Vec<String>>>>,
    colors: &Theme,
    selected: Option<usize>,
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
            let cols = columns.for_content(content, api);
            if cache.borrow().is_none() {
                let rows = compute_rows(content, cols);
                *cache.borrow_mut() = Some(rows);
            }
            let rows = cache.borrow();
            table::render_table(
                f,
                cols,
                rows.as_deref().unwrap_or(&[]),
                selected,
                colors,
                area,
            );
        }
    }
}
