//! Shared table chrome: header, column spacing, selection highlight, scrollbar.
//!
//! Both the content view (`content`) and the playback queue (`queue`) render
//! tables. They differ in where rows come from and whether selection is drawn
//! through a `TableState`, but the frame around the rows is the same.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Cell, Row, Table, TableState},
};

use super::scrollbar::render_scrollbar;
use crate::config::Theme;

/// Which part of the table the selection highlight covers.
///
/// `None` on [`TableView::highlight`] means the caller already styled the
/// visible rows (the queue highlights the playing row itself).
#[derive(Clone, Copy)]
pub(super) enum Highlight {
    Row,
    Cell,
}

/// Everything `render_table` needs; the state is optional so callers that
/// pre-scroll their rows can render statelessly.
pub(super) struct TableView<'a, 's> {
    pub header: Row<'a>,
    pub rows: Vec<Row<'a>>,
    pub widths: Vec<Constraint>,
    pub state: Option<&'s mut TableState>,
    pub highlight: Option<Highlight>,
    pub total: usize,
    pub selected: usize,
}

/// Build the standard header row: each cell muted, the row bold and one line
/// tall. Callers choose the labels and how they are styled internally.
pub(super) fn header_row<'a>(
    labels: impl IntoIterator<Item = Line<'a>>,
    colors: &Theme,
) -> Row<'a> {
    Row::new(
        labels
            .into_iter()
            .map(|line| Cell::from(line).style(Style::default().fg(colors.muted))),
    )
    .style(Style::default().add_modifier(Modifier::BOLD))
    .height(1)
}

fn highlight_style(colors: &Theme) -> Style {
    Style::default()
        .fg(colors.bg)
        .bg(colors.accent)
        .add_modifier(Modifier::BOLD)
}

pub(super) fn render_table(f: &mut Frame, view: TableView<'_, '_>, colors: &Theme, area: Rect) {
    let TableView {
        header,
        rows,
        widths,
        state,
        highlight,
        total,
        selected,
    } = view;

    if total == 0 || widths.is_empty() {
        return;
    }

    let [table_area, scrollbar_area] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(1)]).areas(area);

    let table = Table::new(rows, widths).header(header).column_spacing(2);

    match (state, highlight) {
        (Some(state), Some(Highlight::Row)) => {
            let table = table
                .row_highlight_style(highlight_style(colors))
                .highlight_symbol("");
            f.render_stateful_widget(table, table_area, state);
        }
        (Some(state), Some(Highlight::Cell)) => {
            let table = table.cell_highlight_style(highlight_style(colors));
            f.render_stateful_widget(table, table_area, state);
        }
        // Stateless: rows already carry their own styles.
        _ => f.render_widget(table, table_area),
    }

    render_scrollbar(f, total, selected, scrollbar_area, colors.muted);
}

#[cfg(test)]
mod tests {
    use ratatui::{
        Terminal,
        backend::TestBackend,
        buffer::Buffer,
        style::{Modifier, Style},
        widgets::{Cell, Row, TableState},
    };

    use super::*;

    fn draw(view: TableView<'_, '_>, theme: &Theme, width: u16, height: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|f| render_table(f, view, theme, Rect::new(0, 0, width, height)))
            .unwrap();
        terminal.backend().buffer().clone()
    }

    #[test]
    fn renders_header_and_stateful_row_highlight() {
        let theme = Theme::default();
        let header = header_row([Line::from("NAME")], &theme);
        let rows = vec![Row::new(vec![Cell::from("song")])];
        let mut state = TableState::default();
        state.select(Some(0));

        let buffer = draw(
            TableView {
                header,
                rows,
                widths: vec![Constraint::Length(10)],
                state: Some(&mut state),
                highlight: Some(Highlight::Row),
                total: 1,
                selected: 0,
            },
            &theme,
            20,
            4,
        );

        assert_eq!(buffer[(0, 0)].symbol(), "N");
        assert_eq!(buffer[(0, 0)].style().fg, Some(theme.muted));
        assert!(buffer[(0, 0)].style().add_modifier.contains(Modifier::BOLD));
        assert_eq!(buffer[(0, 1)].style().bg, Some(theme.accent));
    }

    #[test]
    fn renders_cell_highlight() {
        let theme = Theme::default();
        let header = header_row([Line::from("#"), Line::from("NAME")], &theme);
        let rows = vec![Row::new(vec![Cell::from("1"), Cell::from("song")])];
        let mut state = TableState::default();
        state.select(Some(0));
        state.select_first_column();

        let buffer = draw(
            TableView {
                header,
                rows,
                widths: vec![Constraint::Length(3), Constraint::Length(10)],
                state: Some(&mut state),
                highlight: Some(Highlight::Cell),
                total: 1,
                selected: 0,
            },
            &theme,
            20,
            4,
        );

        assert_eq!(buffer[(0, 1)].style().bg, Some(theme.accent));
        assert_ne!(buffer[(4, 1)].style().bg, Some(theme.accent));
    }

    #[test]
    fn stateless_view_keeps_caller_row_style() {
        let theme = Theme::default();
        let header = header_row([Line::from("NAME")], &theme);
        let rows = vec![
            Row::new(vec![Cell::from("playing")])
                .style(Style::default().fg(theme.surface).bg(theme.accent)),
        ];

        let buffer = draw(
            TableView {
                header,
                rows,
                widths: vec![Constraint::Length(10)],
                state: None,
                highlight: None,
                total: 1,
                selected: 0,
            },
            &theme,
            20,
            4,
        );

        assert_eq!(buffer[(0, 1)].symbol(), "p");
        assert_eq!(buffer[(0, 1)].style().fg, Some(theme.surface));
        assert_eq!(buffer[(0, 1)].style().bg, Some(theme.accent));
    }

    #[test]
    fn empty_table_is_not_drawn() {
        let theme = Theme::default();
        let buffer = draw(
            TableView {
                header: Row::default(),
                rows: vec![],
                widths: vec![],
                state: None,
                highlight: None,
                total: 0,
                selected: 0,
            },
            &theme,
            10,
            3,
        );

        assert_eq!(buffer[(0, 0)].symbol(), " ");
    }
}
