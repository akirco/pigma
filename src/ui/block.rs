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
}

impl<'a> CornerBlock<'a> {
    pub fn new(block: Block<'a>) -> Self {
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
        }
    }

    pub fn corner_color(mut self, color: Color) -> Self {
        self.tl_color = color;
        self.tr_color = color;
        self.bl_color = color;
        self.br_color = color;
        self
    }

    pub fn corner_sizes(mut self, horizontal: u16, vertical: u16) -> Self {
        self.h_size = horizontal;
        self.v_size = vertical;
        self
    }

    pub fn follow_corner_color(mut self, follow: bool) -> Self {
        self.follow_corner_color = follow;
        self
    }

    pub fn border_gradient(mut self, preset: Option<GradientPreset>) -> Self {
        self.border_gradient = preset;
        self
    }

    pub fn border_gradient_speed(mut self, speed: f64) -> Self {
        self.border_gradient_speed = speed;
        self
    }

    pub fn tick(mut self, tick: u64) -> Self {
        self.tick = tick;
        self
    }

    pub fn block_padding(mut self, padding: ratatui::widgets::Padding) -> Self {
        self.block = self.block.padding(padding);
        self
    }

    pub fn inner(&self, area: Rect) -> Rect {
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

        // border gradient: 优先于 follow_corner_color
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
            // follow_corner_color: 将横竖边框也染成 corner 色
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

// create block fn

use ratatui::style::Style;
use ratatui::widgets::{BorderType, Borders, Padding};

use crate::config::{BorderConfig, Theme};

use super::styled_text;

pub struct BlockStyle<'a> {
    pub colors: &'a Theme,
    pub border: &'a BorderConfig,
    pub tick: u64,
}

pub(crate) fn create_block<'a>(
    title: &'a str,
    style: &'a BlockStyle<'a>,
    _focused: bool,
) -> CornerBlock<'a> {
    create_block_bg(title, style, _focused, style.colors.bg)
}

pub(crate) fn create_block_surfaced<'a>(
    title: &'a str,
    style: &'a BlockStyle<'a>,
    _focused: bool,
) -> CornerBlock<'a> {
    create_block_bg(title, style, _focused, style.colors.surface)
}

fn create_block_bg<'a>(
    title: &'a str,
    style: &'a BlockStyle<'a>,
    _focused: bool,
    no_border_bg: Color,
) -> CornerBlock<'a> {
    let border_color = style.colors.border;
    let border_type = if style.border.rounded {
        BorderType::Rounded
    } else {
        BorderType::Plain
    };
    let title_line = ratatui::text::Line::from(styled_text::parse_styled(title, style.colors));
    let block = if style.border.enabled {
        Block::default()
            .borders(Borders::ALL)
            .border_type(border_type)
            .border_style(Style::default().fg(border_color))
            .title(title_line)
            .title_style(Style::default().fg(style.colors.muted))
    } else {
        Block::default()
            .borders(Borders::NONE)
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(no_border_bg))
            .title(title_line)
            .title_style(Style::default().fg(style.colors.muted))
            .padding(Padding::horizontal(1))
    };
    CornerBlock::new(block)
        .corner_color(style.colors.accent)
        .corner_sizes(2, 1)
        .follow_corner_color(style.border.follow_corner_color)
        .border_gradient(style.border.border_gradient)
        .border_gradient_speed(style.border.border_gradient_speed)
        .tick(style.tick)
}
