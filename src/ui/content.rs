use std::cell::RefCell;
use std::collections::HashMap;

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, TableState},
};

use super::table;
use crate::config::ColumnsConfig;
use crate::field::ToFieldMap;
use crate::config::Theme;
use crate::config::ColumnDef;
use crate::state::{ContentState, TableMode};

fn compute_rows(content: &ContentState, columns: &[ColumnDef]) -> Vec<Vec<String>> {
    match content {
        ContentState::Songs(songs) => {
            let maps: Vec<HashMap<String, String>> =
                songs.iter().map(ToFieldMap::to_field_map).collect();
            table::build_rows(&maps, columns)
        }
        ContentState::SongLists(lists) => {
            let maps: Vec<HashMap<String, String>> =
                lists.iter().map(ToFieldMap::to_field_map).collect();
            table::build_rows(&maps, columns)
        }
        ContentState::TopLists(lists) => {
            let maps: Vec<HashMap<String, String>> =
                lists.iter().map(ToFieldMap::to_field_map).collect();
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
        ContentState::Singers(singers) => {
            let maps: Vec<HashMap<String, String>> =
                singers.iter().map(ToFieldMap::to_field_map).collect();
            table::build_rows(&maps, columns)
        }
        _ => vec![],
    }
}

pub fn render_content(
    f: &mut Frame,
    content: &ContentState,
    columns: &ColumnsConfig,
    api: Option<&str>,
    cache: &RefCell<Option<Vec<Vec<String>>>>,
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
                table_state,
                table_mode,
                colors,
                area,
            );
        }
    }
}
