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
    pub fn new(tick: u64) -> Self {
        Self {
            tick,
            filled_color: Style::default(),
            empty_color: Style::default(),
        }
    }

    pub fn active_color(mut self, style: Style) -> Self {
        self.filled_color = style;
        self
    }

    pub fn inactive_color(mut self, style: Style) -> Self {
        self.empty_color = style;
        self
    }
}

impl Widget for Spinner {
    fn render(self, area: Rect, buf: &mut Buffer) {
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
