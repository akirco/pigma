use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Color,
    widgets::{Block, Widget},
};

use crate::utils::GradientPreset;

pub struct CornerBlock<'a> {
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
    tick: u64,
    no_border: bool,
    has_title: bool,
    horizontal_padding: u16,
}

impl<'a> CornerBlock<'a> {
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
            tick: 0,
            no_border: false,
            has_title: false,
            horizontal_padding: 0,
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

    fn tick(mut self, tick: u64) -> Self {
        self.tick = tick;
        self
    }

    pub(super) fn from_color(style: &'a BlockStyle<'a>, no_border_bg: Color) -> Self {
        let border_color = style.colors.border;
        let border_type = if style.border.rounded {
            BorderType::Rounded
        } else {
            BorderType::Plain
        };
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
            .tick(style.tick)
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

    pub(super) fn inner(&self, area: Rect) -> Rect {
        self.block.inner(area)
    }
}

impl<'a> Widget for CornerBlock<'a> {
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

// create block builder

use ratatui::style::Style;
use ratatui::widgets::{BorderType, Borders, Padding};

use crate::config::{BorderConfig, Theme};

use super::styled_text;

pub struct BlockStyle<'a> {
    pub colors: &'a Theme,
    pub border: &'a BorderConfig,
    pub tick: u64,
}
