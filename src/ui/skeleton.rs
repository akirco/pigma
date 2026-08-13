use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

/// Skeleton screen: while table content loads, alternate `bg` and `surface` background colors
/// row by row to mimic the row-spacing look of the real table.
pub struct Skeleton {
    bg: Color,
    surface: Color,
}

impl Skeleton {
    pub(super) fn new() -> Self {
        Self {
            bg: Color::Reset,
            surface: Color::Reset,
        }
    }

    pub(super) fn bg(mut self, color: Color) -> Self {
        self.bg = color;
        self
    }

    pub(super) fn surface(mut self, color: Color) -> Self {
        self.surface = color;
        self
    }
}

impl Default for Skeleton {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Skeleton {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        for (i, row) in area.rows().enumerate() {
            let style = Style::default().bg(if i % 2 == 0 { self.bg } else { self.surface });
            for x in row.left()..row.right() {
                if let Some(cell) = buf.cell_mut((x, row.y)) {
                    cell.reset();
                    cell.set_style(style);
                }
            }
        }
    }
}
