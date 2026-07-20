use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Color, Style},
    widgets::Widget,
};

struct ShortBraille {
    ratio: f64,
    width: u16,
    start_color: Color,
    end_color: Color,
    empty_style: Style,
}

impl ShortBraille {
    fn new(ratio: f64, width: u16) -> Self {
        Self {
            ratio: ratio.clamp(0.0, 1.0),
            width: width.max(1),
            start_color: Color::Red,
            end_color: Color::Green,
            empty_style: Style::default().fg(Color::DarkGray),
        }
    }

    fn start_color(mut self, color: Color) -> Self {
        self.start_color = color;
        self
    }

    fn end_color(mut self, color: Color) -> Self {
        self.end_color = color;
        self
    }

    fn empty_style(mut self, style: Style) -> Self {
        self.empty_style = style;
        self
    }
}

impl Widget for ShortBraille {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 {
            return;
        }

        let total_dots = self.width as usize * 4;
        let filled_dots = (self.ratio * total_dots as f64).round() as usize;

        let y = area.y;
        let start_x = area.x + (area.width.saturating_sub(self.width)) / 2;

        const DOTS: [(u8, u8); 4] = [
            (3, 0x04), // 点3
            (6, 0x20), // 点6
            (2, 0x02), // 点2
            (5, 0x10), // 点5
        ];

        for col in 0..self.width as usize {
            let x = start_x + col as u16;
            if x >= area.right() {
                break;
            }

            let dots_before = col * 4;
            let remaining = filled_dots.saturating_sub(dots_before);
            let dots_in_this_char = remaining.min(4);

            let mut bits: u8 = 0;
            for i in 0..dots_in_this_char {
                bits |= DOTS[i].1;
            }

            let braille_char = char::from_u32(0x2800 + bits as u32).unwrap_or(' ');

            let col_ratio = (col as f64 + 0.5) / self.width as f64;
            let style = if dots_in_this_char > 0 {
                Style::default().fg(lerp_color(self.start_color, self.end_color, col_ratio))
            } else {
                self.empty_style
            };

            if let Some(cell) = buf.cell_mut(Position::new(x, y)) {
                cell.set_char(braille_char).set_style(style);
            }
        }
    }
}

fn lerp_color(a: Color, b: Color, t: f64) -> Color {
    let t = t.clamp(0.0, 1.0);
    let (ar, ag, ab) = color_rgb(a);
    let (br, bg, bb) = color_rgb(b);
    let r = (ar as f64 + (br as f64 - ar as f64) * t).round() as u8;
    let g = (ag as f64 + (bg as f64 - ag as f64) * t).round() as u8;
    let b = (ab as f64 + (bb as f64 - ab as f64) * t).round() as u8;
    Color::Rgb(r, g, b)
}

fn color_rgb(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Black => (0, 0, 0),
        Color::Red => (255, 0, 0),
        Color::Green => (0, 255, 0),
        Color::Yellow => (255, 255, 0),
        Color::Blue => (0, 0, 255),
        Color::Magenta => (255, 0, 255),
        Color::Cyan => (0, 255, 255),
        Color::White => (255, 255, 255),
        Color::DarkGray => (85, 85, 85),
        _ => (170, 170, 170),
    }
}

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend, layout::Position as Pos};
use std::{io, time::Duration};

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut progress: f64 = 0.0;

    loop {
        terminal.draw(|f| {
            let area = f.area();
            let gauge = ShortBraille::new(progress, 30)
                .start_color(Color::Red)
                .end_color(Color::Green)
                .empty_style(Style::default().fg(Color::DarkGray));

            let y = area.height / 2;
            let rect = ratatui::layout::Rect::new(area.x, y, area.width, 1);
            f.render_widget(gauge, rect);

            let label = format!(" {:.1}% ", progress * 100.0);
            let label_x = area.x + (area.width.saturating_sub(label.len() as u16)) / 2;
            let label_y = y + 1;
            for (i, ch) in label.chars().enumerate() {
                if let Some(cell) = f
                    .buffer_mut()
                    .cell_mut(Pos::new(label_x + i as u16, label_y))
                {
                    cell.set_char(ch)
                        .set_style(Style::default().fg(Color::White));
                }
            }
        })?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }
        progress = (progress + 0.005) % 1.0;
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
