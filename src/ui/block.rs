use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Padding, Widget},
};

use super::{BlockStyle, styled_text};
use crate::config::{BorderStyle, GradientPreset, Theme};

pub(super) struct StyledBlock<'a> {
    block: Block<'a>,
    tl_color: Color,
    tr_color: Color,
    bl_color: Color,
    br_color: Color,
    h_size: u16,
    v_size: u16,
    follow_corner_color: bool,
    border_gradient: Option<GradientPreset>,
    border_gradient_speed: f64,
    border_type: BorderType,
    tick: u64,
    no_border: bool,
    has_title: bool,
    horizontal_padding: u16,
    transparent_border: bool,
}

impl<'a> StyledBlock<'a> {
    pub(super) fn new(block: Block<'a>) -> Self {
        Self {
            block,
            tl_color: Color::White,
            tr_color: Color::White,
            bl_color: Color::White,
            br_color: Color::White,
            h_size: 1,
            v_size: 1,
            follow_corner_color: false,
            border_gradient: None,
            border_gradient_speed: 0.0,
            border_type: BorderType::Plain,
            tick: 0,
            no_border: false,
            has_title: false,
            horizontal_padding: 0,
            transparent_border: false,
        }
    }

    pub(super) fn corner_color(mut self, color: Color) -> Self {
        self.tl_color = color;
        self.tr_color = color;
        self.bl_color = color;
        self.br_color = color;
        self
    }

    pub(super) fn corner_sizes(mut self, horizontal: u16, vertical: u16) -> Self {
        self.h_size = horizontal;
        self.v_size = vertical;
        self
    }

    fn follow_corner_color(mut self, follow: bool) -> Self {
        self.follow_corner_color = follow;
        self
    }

    fn border_gradient(mut self, preset: Option<GradientPreset>) -> Self {
        self.border_gradient = preset;
        self
    }

    fn border_gradient_speed(mut self, speed: f64) -> Self {
        self.border_gradient_speed = speed;
        self
    }

    fn border_type(mut self, border_type: BorderType) -> Self {
        self.border_type = border_type;
        self
    }

    fn tick(mut self, tick: u64) -> Self {
        self.tick = tick;
        self
    }

    fn transparent_border(mut self, transparent: bool) -> Self {
        self.transparent_border = transparent;
        self
    }

    pub(super) fn from_color(style: &'a BlockStyle<'a>, no_border_bg: Color) -> Self {
        let border_color = style.colors.border;
        let border_type = border_type(style.border.style);
        let (block, horizontal_padding, no_border) = if style.border.enabled {
            (
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(border_type)
                    .border_style(Style::default().fg(border_color))
                    .title_style(Style::default().fg(style.colors.muted)),
                0,
                false,
            )
        } else {
            let padding = Padding::new(1, 1, 1, 0);
            (
                Block::default()
                    .borders(Borders::NONE)
                    .border_style(Style::default().fg(border_color))
                    .style(Style::default().bg(no_border_bg))
                    .title_style(Style::default().fg(style.colors.muted))
                    .padding(padding),
                1,
                true,
            )
        };
        Self::new(block)
            .corner_color(style.colors.accent)
            .corner_sizes(2, 1)
            .follow_corner_color(style.border.follow_corner_color)
            .border_gradient(style.border.border_gradient)
            .border_gradient_speed(style.border.border_gradient_speed)
            .border_type(border_type)
            .tick(style.tick)
            .transparent_border(border_color == Color::Reset)
            .set_borderless(horizontal_padding, no_border)
    }

    pub(super) fn set_borderless(mut self, horizontal_padding: u16, no_border: bool) -> Self {
        self.horizontal_padding = horizontal_padding;
        self.no_border = no_border;
        self
    }

    pub(super) fn title(mut self, title: &'a str, colors: &'a Theme) -> Self {
        let title_line = ratatui::text::Line::from(styled_text::parse_styled(title, colors));
        self.block = self
            .block
            .title(title_line)
            .title_style(Style::default().fg(colors.muted));
        if self.no_border {
            let h = self.horizontal_padding;
            self.block = self.block.padding(Padding::new(h, h, 0, 0));
        }
        self.has_title = true;
        self
    }

    pub(super) fn block_padding(mut self, padding: ratatui::widgets::Padding) -> Self {
        if self.no_border && !self.has_title && padding.top == 0 {
            self.block = self.block.padding(Padding { top: 1, ..padding });
        } else {
            self.block = self.block.padding(padding);
        }
        self
    }

    /// Override the border line colour (used by selected bordered cards).
    pub(super) fn border_color(mut self, color: Color) -> Self {
        self.block = self.block.border_style(Style::default().fg(color));
        self
    }

    /// Set padding exactly, without the borderless-mode top row added by
    /// [`Self::block_padding`]. Content cards use it so the cover starts at the
    /// top edge and the card does not gain a blank row.
    pub(super) fn content_padding(mut self, padding: ratatui::widgets::Padding) -> Self {
        self.block = self.block.padding(padding);
        self
    }

    pub(super) fn inner(&self, area: Rect) -> Rect {
        self.block.inner(area)
    }
}

impl<'a> Widget for StyledBlock<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (tl, tr, bl, br) = (self.tl_color, self.tr_color, self.bl_color, self.br_color);

        self.block.render(area, buf);

        if area.width < 2 || area.height < 2 {
            return;
        }

        let top = area.top();
        let bottom = area.bottom() - 1;
        let left = area.left();
        let right = area.right() - 1;

        let max_h = self.h_size.min(area.width / 2);
        let max_v = self.v_size.min(area.height / 2);

        // transparent border: blank the horizontal/vertical line segments so
        // only the corner characters (painted with corner_color below) show.
        if self.transparent_border {
            let border_set = self.border_type.to_border_set();
            for x in (left + max_h)..=(right - max_h) {
                Self::blank_if_border_line(buf, (x, top), &border_set);
                Self::blank_if_border_line(buf, (x, bottom), &border_set);
            }
            for y in (top + max_v)..=(bottom - max_v) {
                Self::blank_if_border_line(buf, (left, y), &border_set);
                Self::blank_if_border_line(buf, (right, y), &border_set);
            }
        }

        // corner pixels
        for i in 0..max_h {
            if let Some(cell) = buf.cell_mut((left + i, top)) {
                cell.fg = tl;
            }
            if let Some(cell) = buf.cell_mut((right - i, top)) {
                cell.fg = tr;
            }
            if let Some(cell) = buf.cell_mut((left + i, bottom)) {
                cell.fg = bl;
            }
            if let Some(cell) = buf.cell_mut((right - i, bottom)) {
                cell.fg = br;
            }
        }

        for i in 0..max_v {
            if let Some(cell) = buf.cell_mut((left, top + i)) {
                cell.fg = tl;
            }
            if let Some(cell) = buf.cell_mut((right, top + i)) {
                cell.fg = tr;
            }
            if let Some(cell) = buf.cell_mut((left, bottom - i)) {
                cell.fg = bl;
            }
            if let Some(cell) = buf.cell_mut((right, bottom - i)) {
                cell.fg = br;
            }
        }

        // border gradient: takes precedence over follow_corner_color
        if let Some(preset) = self.border_gradient {
            let h_span = right.saturating_sub(left);
            let v_span = bottom.saturating_sub(top);
            let offset = self.tick as f32 * self.border_gradient_speed as f32;

            // top edge: left → right
            for x in left..=right {
                let base = if h_span == 0 {
                    0.0
                } else {
                    (x - left) as f32 / h_span as f32
                };
                let t = (base + offset).rem_euclid(1.0);
                let [r, g, b] = preset.color(t);
                if let Some(cell) = buf.cell_mut((x, top)) {
                    cell.fg = Color::Rgb(r, g, b);
                }
            }
            // bottom edge: right → left (reversed for clockwise scroll)
            for x in left..=right {
                let base = if h_span == 0 {
                    0.0
                } else {
                    (right - x) as f32 / h_span as f32
                };
                let t = (base + offset).rem_euclid(1.0);
                let [r, g, b] = preset.color(t);
                if let Some(cell) = buf.cell_mut((x, bottom)) {
                    cell.fg = Color::Rgb(r, g, b);
                }
            }
            // left edge: top → bottom
            for y in top..=bottom {
                let base = if v_span == 0 {
                    0.0
                } else {
                    (y - top) as f32 / v_span as f32
                };
                let t = (base + offset).rem_euclid(1.0);
                let [r, g, b] = preset.color(t);
                if let Some(cell) = buf.cell_mut((left, y)) {
                    cell.fg = Color::Rgb(r, g, b);
                }
            }
            // right edge: bottom → top (reversed for clockwise scroll)
            for y in top..=bottom {
                let base = if v_span == 0 {
                    0.0
                } else {
                    (bottom - y) as f32 / v_span as f32
                };
                let t = (base + offset).rem_euclid(1.0);
                let [r, g, b] = preset.color(t);
                if let Some(cell) = buf.cell_mut((right, y)) {
                    cell.fg = Color::Rgb(r, g, b);
                }
            }
        } else if self.follow_corner_color {
            // follow_corner_color: also paint the horizontal and vertical borders with the corner color
            for x in (left + max_h)..=(right - max_h) {
                if let Some(cell) = buf.cell_mut((x, top)) {
                    cell.fg = tl;
                }
                if let Some(cell) = buf.cell_mut((x, bottom)) {
                    cell.fg = bl;
                }
            }
            for y in (top + max_v)..=(bottom - max_v) {
                if let Some(cell) = buf.cell_mut((left, y)) {
                    cell.fg = tl;
                }
                if let Some(cell) = buf.cell_mut((right, y)) {
                    cell.fg = tr;
                }
            }
        }
    }
}

impl<'a> StyledBlock<'a> {
    /// Blank a cell if it currently holds a glyph from the active border set,
    /// so titles (non-border glyphs) survive transparent borders.
    fn blank_if_border_line(
        buf: &mut Buffer,
        pos: (u16, u16),
        border_set: &ratatui::symbols::border::Set<'_>,
    ) {
        let Some(cell) = buf.cell_mut(pos) else {
            return;
        };
        let is_border = [
            border_set.top_left,
            border_set.top_right,
            border_set.bottom_left,
            border_set.bottom_right,
            border_set.vertical_left,
            border_set.vertical_right,
            border_set.horizontal_top,
            border_set.horizontal_bottom,
        ]
        .contains(&cell.symbol());
        if is_border {
            cell.set_symbol(" ");
        }
    }
}

fn border_type(style: BorderStyle) -> BorderType {
    match style {
        BorderStyle::Plain => BorderType::Plain,
        BorderStyle::Rounded => BorderType::Rounded,
        BorderStyle::Double => BorderType::Double,
        BorderStyle::Thick => BorderType::Thick,
        BorderStyle::LightDoubleDashed => BorderType::LightDoubleDashed,
        BorderStyle::HeavyDoubleDashed => BorderType::HeavyDoubleDashed,
        BorderStyle::LightTripleDashed => BorderType::LightTripleDashed,
        BorderStyle::HeavyTripleDashed => BorderType::HeavyTripleDashed,
        BorderStyle::LightQuadrupleDashed => BorderType::LightQuadrupleDashed,
        BorderStyle::HeavyQuadrupleDashed => BorderType::HeavyQuadrupleDashed,
        BorderStyle::QuadrantInside => BorderType::QuadrantInside,
        BorderStyle::QuadrantOutside => BorderType::QuadrantOutside,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, style::Color};

    use super::*;
    use crate::config::{BorderConfig, BorderStyle};

    const STYLES: [BorderStyle; 12] = [
        BorderStyle::Plain,
        BorderStyle::Rounded,
        BorderStyle::Double,
        BorderStyle::Thick,
        BorderStyle::LightDoubleDashed,
        BorderStyle::HeavyDoubleDashed,
        BorderStyle::LightTripleDashed,
        BorderStyle::HeavyTripleDashed,
        BorderStyle::LightQuadrupleDashed,
        BorderStyle::HeavyQuadrupleDashed,
        BorderStyle::QuadrantInside,
        BorderStyle::QuadrantOutside,
    ];

    /// Every border character set must render inside a small terminal, both
    /// normally and with a transparent border (the path that blanks edge
    /// glyphs and must know the active set).
    #[test]
    fn every_border_style_renders() {
        let mut colors = Theme::default();
        for transparent in [false, true] {
            colors.border = if transparent {
                Color::Reset
            } else {
                Color::White
            };
            for style in STYLES {
                let border = BorderConfig {
                    style,
                    ..BorderConfig::default()
                };
                let bs = BlockStyle {
                    colors: &colors,
                    border: &border,
                    tick: 0,
                };
                let mut terminal = Terminal::new(TestBackend::new(20, 5)).expect("terminal");
                terminal
                    .draw(|frame| {
                        frame.render_widget(StyledBlock::from_color(&bs, colors.bg), frame.area());
                    })
                    .expect("render border");
            }
        }
    }
}
