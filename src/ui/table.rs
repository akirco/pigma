use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Cell, Row, Table},
};

use crate::theme::Theme;
use crate::types::ColumnDef;
use crate::ui::{calc_scroll_offset, render_scrollbar};

/// Generic table renderer driven by `ColumnDef` headers and string rows.
///
/// - Missing field values in any row should be `"—"` (handled by caller before passing).
/// - Extra columns beyond the defined headers are ignored.
pub fn render_table(
    f: &mut Frame,
    headers: &[ColumnDef],
    rows: &[Vec<String>],
    selected: Option<usize>,
    colors: &Theme,
    area: Rect,
) {
    if rows.is_empty() || headers.is_empty() {
        return;
    }

    let [table_area, scrollbar_area] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(1)]).areas(area);

    let sel = selected.unwrap_or(0);
    let visible_height = table_area.height.saturating_sub(1) as usize;
    let offset = calc_scroll_offset(sel, visible_height, rows.len());

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
        .enumerate()
        .skip(offset)
        .take(visible_height)
        .map(|(i, cells)| {
            let is_selected = Some(i) == selected;
            let row_style = if is_selected {
                Style::default()
                    .fg(colors.bg)
                    .bg(colors.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let styled_cells: Vec<Cell> = cells
                .iter()
                .enumerate()
                .map(|(col_idx, value)| {
                    let color = if col_idx == 0 {
                        colors.text
                    } else {
                        colors.muted
                    };
                    // First column gets text color, rest get muted
                    // Special case: missing field markers get error color
                    if value == "—" {
                        Cell::from(value.as_str()).style(Style::default().fg(colors.error))
                    } else {
                        Cell::from(value.as_str()).style(Style::default().fg(color))
                    }
                })
                .collect();

            Row::new(styled_cells).height(1).style(row_style)
        })
        .collect();

    let table = Table::new(table_rows, widths)
        .header(header)
        .column_spacing(2);

    f.render_widget(table, table_area);

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
