use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

/// Skeleton 骨架屏：表格内容加载时按行交替填充 `bg` 与 `surface` 背景色，
/// 模拟真实表格的行间隔效果。
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
