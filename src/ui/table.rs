use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Cell, Row, Table, TableState},
};

use crate::config::Theme;
use crate::state::TableMode;
use crate::ui::styled_text;
use crate::{config::ColumnDef, ui::scrollbar::render_scrollbar};

pub fn render_table(
    f: &mut Frame,
    headers: &[ColumnDef],
    rows: Vec<Row<'_>>,
    table_state: &mut TableState,
    table_mode: TableMode,
    colors: &Theme,
    area: Rect,
) {
    let row_count = rows.len();
    if row_count == 0 || headers.is_empty() {
        return;
    }

    let [table_area, scrollbar_area] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(1)]).areas(area);

    let header_cells: Vec<Cell> = headers
        .iter()
        .map(|h| {
            let spans = styled_text::parse_styled(&h.header, colors);
            Cell::from(Line::from(spans)).style(Style::default().fg(colors.muted))
        })
        .collect();
    let header = Row::new(header_cells)
        .style(Style::default().add_modifier(Modifier::BOLD))
        .height(1);

    let widths: Vec<Constraint> = headers.iter().map(|h| h.to_constraint()).collect();

    let sel = table_state.selected().unwrap_or(0);

    let table = Table::new(rows, widths).header(header).column_spacing(2);

    match table_mode {
        TableMode::Row => {
            let row_style = Style::default()
                .fg(colors.bg)
                .bg(colors.accent)
                .add_modifier(Modifier::BOLD);

            let table = table.row_highlight_style(row_style).highlight_symbol("");

            f.render_stateful_widget(table, table_area, table_state);
        }
        TableMode::Cell => {
            let cell_highlight = Style::default()
                .fg(colors.bg)
                .bg(colors.accent)
                .add_modifier(Modifier::BOLD);

            let table = table.cell_highlight_style(cell_highlight);

            f.render_stateful_widget(table, table_area, table_state);
        }
    }

    render_scrollbar(f, row_count, sel, scrollbar_area);
}
