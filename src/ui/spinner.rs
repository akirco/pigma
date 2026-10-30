use ratatui::{buffer::Buffer, layout::Rect, style::Style, widgets::Widget};

const BLOCKS_FRAMES: &[&str] = &[
    "▰▱▱▱▱▱▱",
    "▰▰▱▱▱▱▱",
    "▰▰▰▱▱▱▱",
    "▰▰▰▰▱▱▱",
    "▰▰▰▰▰▱▱",
    "▰▰▰▰▰▰▱",
    "▰▰▰▰▰▰▰",
    "▰▱▱▱▱▱▱",
];

const SPEED: u64 = 3;

pub struct Spinner {
    tick: u64,
    filled_color: Style,
    empty_color: Style,
}

impl Spinner {
    pub(super) fn new(tick: u64) -> Self {
        Self {
            tick,
            filled_color: Style::default(),
            empty_color: Style::default(),
        }
    }

    pub(super) fn active_color(mut self, style: Style) -> Self {
        self.filled_color = style;
        self
    }

    pub(super) fn inactive_color(mut self, style: Style) -> Self {
        self.empty_color = style;
        self
    }
}

impl Widget for Spinner {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // A zero-height area can still carry an out-of-buffer y (an empty rect
        // sitting on the buffer's bottom edge), so skip it before indexing.
        if area.is_empty() {
            return;
        }
        let frame_idx = (self.tick / SPEED) as usize % BLOCKS_FRAMES.len();
        let frame = BLOCKS_FRAMES[frame_idx];

        for (i, ch) in frame.chars().enumerate() {
            let x = area.x + i as u16;
            if x >= area.right() {
                break;
            }
            let style = if ch == '▰' {
                self.filled_color
            } else {
                self.empty_color
            };
            buf[(x, area.y)].set_char(ch);
            buf[(x, area.y)].set_style(style);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    /// A zero-height spinner area can sit on the buffer's bottom edge
    /// (y == buffer height, e.g. a squeezed player bar's inner rect). The
    /// x-bounds guard alone doesn't help there: `buf[(x, y)]` would still
    /// index one row past the buffer, so empty areas must be skipped.
    #[test]
    fn empty_area_at_buffer_edge_does_not_panic() {
        let mut term = Terminal::new(TestBackend::new(64, 11)).unwrap();
        term.draw(|f| {
            f.render_widget(Spinner::new(0), Rect::new(0, 11, 3, 0));
        })
        .unwrap();
    }

    /// The non-empty branch: the guard must fall through and draw the frame
    /// for the current tick.
    #[test]
    fn non_empty_area_renders_the_current_frame() {
        let mut term = Terminal::new(TestBackend::new(64, 11)).unwrap();
        term.draw(|f| {
            f.render_widget(Spinner::new(0), Rect::new(0, 0, 3, 1));
        })
        .unwrap();
        let buf = term.backend().buffer();
        assert_eq!(buf[(0, 0)].symbol(), "▰");
        assert_eq!(buf[(1, 0)].symbol(), "▱");
        assert_eq!(buf[(2, 0)].symbol(), "▱");
    }
}
