use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Cell, Row, Table, TableState},
};

use crate::theme::Theme;
use crate::types::{ColumnDef, TableMode};
use crate::ui::{calc_scroll_offset, render_scrollbar};

pub fn render_table(
    f: &mut Frame,
    headers: &[ColumnDef],
    rows: &[Vec<String>],
    table_state: &mut TableState,
    table_mode: TableMode,
    colors: &Theme,
    area: Rect,
) {
    if rows.is_empty() || headers.is_empty() {
        return;
    }

    let [table_area, scrollbar_area] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(1)]).areas(area);

    let header_cells: Vec<Cell> = headers
        .iter()
        .map(|h| Cell::from(h.header.as_str()).style(Style::default().fg(colors.muted)))
        .collect();
    let header = Row::new(header_cells)
        .style(Style::default().add_modifier(Modifier::UNDERLINED))
        .height(1);

    let widths: Vec<Constraint> = headers.iter().map(|h| h.to_constraint()).collect();

    let table_rows: Vec<Row> = rows
        .iter()
        .map(|cells| {
            let styled_cells: Vec<Cell> = cells
                .iter()
                .map(|value| {
                    let color = colors.muted;
                    if value == "—" {
                        Cell::from(value.as_str()).style(Style::default().fg(colors.error))
                    } else {
                        Cell::from(value.as_str()).style(Style::default().fg(color))
                    }
                })
                .collect();
            Row::new(styled_cells).height(1)
        })
        .collect();

    let selected = table_state.selected();
    let sel = selected.unwrap_or(0);
    let visible_height = table_area.height.saturating_sub(1) as usize;
    let offset = calc_scroll_offset(sel, visible_height, rows.len());
    *table_state.offset_mut() = offset;

    match table_mode {
        TableMode::Row => {
            let row_style = Style::default()
                .fg(colors.bg)
                .bg(colors.accent)
                .add_modifier(Modifier::BOLD);

            let table = Table::new(table_rows, widths)
                .header(header)
                .column_spacing(2)
                .row_highlight_style(row_style)
                .highlight_symbol("");

            f.render_stateful_widget(table, table_area, table_state);
        }
        TableMode::Cell => {
            let cell_highlight = Style::default()
                .fg(colors.bg)
                .bg(colors.accent)
                .add_modifier(Modifier::BOLD);

            let table = Table::new(table_rows, widths)
                .header(header)
                .column_spacing(2)
                .cell_highlight_style(cell_highlight);

            f.render_stateful_widget(table, table_area, table_state);
        }
    }

    render_scrollbar(f, rows.len(), sel, scrollbar_area);
}

/// Build row data from a string map and column definitions.
pub fn build_rows(
    maps: &[std::collections::HashMap<String, String>],
    headers: &[ColumnDef],
) -> Vec<Vec<String>> {
    let mut warned = std::collections::HashSet::new();

    maps.iter()
        .map(|map| {
            headers
                .iter()
                .map(|col| {
                    map.get(&col.field)
                        .cloned()
                        .or_else(|| {
                            if !warned.contains(&col.field) {
                                log::warn!("Missing field: \"{}\" — showing \"—\"", col.field);
                                warned.insert(col.field.clone());
                            }
                            None
                        })
                        .unwrap_or_else(|| "—".to_string())
                })
                .collect()
        })
        .collect()
}
