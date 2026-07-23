use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    symbols,
    text::Line,
    widgets::Widget,
};

use crate::utils::gradient_color;

pub struct GradientLineGauge<'a> {
    ratio: f64,
    label: Option<Line<'a>>,
    filled_symbol: String,
    unfilled_symbol: String,
    unfilled_style: Style,
    gradient_preset: String,
}

impl<'a> GradientLineGauge<'a> {
    pub fn new(preset: &str) -> Self {
        Self {
            ratio: 0.0,
            label: None,
            filled_symbol: symbols::line::THICK_HORIZONTAL.to_string(),
            unfilled_symbol: symbols::line::THICK_HORIZONTAL.to_string(),
            unfilled_style: Style::default().fg(Color::DarkGray),
            gradient_preset: preset.to_string(),
        }
    }

    pub fn ratio(mut self, r: f64) -> Self {
        self.ratio = r.clamp(0.0, 1.0);
        self
    }

    pub fn label(mut self, l: Line<'a>) -> Self {
        self.label = Some(l);
        self
    }

    pub fn filled_symbol(mut self, s: &str) -> Self {
        self.filled_symbol = s.to_string();
        self
    }

    pub fn unfilled_symbol(mut self, s: &str) -> Self {
        self.unfilled_symbol = s.to_string();
        self
    }

    pub fn unfilled_style(mut self, s: Style) -> Self {
        self.unfilled_style = s;
        self
    }
}

impl Widget for GradientLineGauge<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        let ratio = self.ratio;
        let default_label = Line::from(format!("{:3.0}%", ratio * 100.0));
        let label = self.label.as_ref().unwrap_or(&default_label);
        let (col, row) = buf.set_line(area.left(), area.top(), label, area.width);
        let start = col + 1;
        if start >= area.right() {
            return;
        }

        let bar_width = area.right().saturating_sub(start);
        let filled_len = (f64::from(bar_width) * ratio).floor() as u16;

        for i in 0..bar_width {
            let col_x = start + i;
            let (symbol, style) = if i < filled_len {
                let t = if filled_len == 0 {
                    0.0
                } else {
                    i as f64 / filled_len as f64
                };
                let [r, g, b] = gradient_color(&self.gradient_preset, t as f32);
                let style = Style::default().fg(Color::Rgb(r, g, b));
                (self.filled_symbol.as_str(), style)
            } else {
                (self.unfilled_symbol.as_str(), self.unfilled_style)
            };
            buf[(col_x, row)].set_symbol(symbol).set_style(style);
        }
    }
}
