use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    symbols::border,
    widgets::{Block, BorderType, Borders, Padding, Widget},
};

use super::{BlockStyle, styled_text};
use crate::config::{BorderChars, BorderStyle, GradientPreset, Theme};

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
    border_set: border::Set<'a>,
    tick: u64,
    no_border: bool,
    has_title: bool,
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
            border_set: border::PLAIN,
            tick: 0,
            no_border: false,
            has_title: false,
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

    fn with_border_set(mut self, border_set: border::Set<'a>) -> Self {
        self.border_set = border_set;
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
        let border_set = match style.border.style {
            BorderStyle::Custom => custom_border_set(style.border.chars.as_ref()),
            other => border_type(other).to_border_set(),
        };
        let (block, no_border) = if style.border.enabled {
            (
                Block::default()
                    .borders(Borders::ALL)
                    .border_set(border_set)
                    .border_style(Style::default().fg(border_color))
                    .title_style(Style::default().fg(style.colors.muted)),
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
                true,
            )
        };
        Self::new(block)
            .corner_color(style.colors.accent)
            .corner_sizes(2, 1)
            .follow_corner_color(style.border.follow_corner_color)
            .border_gradient(style.border.border_gradient)
            .border_gradient_speed(style.border.border_gradient_speed)
            .with_border_set(border_set)
            .tick(style.tick)
            .transparent_border(border_color == Color::Reset)
            .set_borderless(no_border)
    }

    pub(super) fn set_borderless(mut self, no_border: bool) -> Self {
        self.no_border = no_border;
        self
    }

    pub(super) fn title(mut self, title: &'a str, colors: &'a Theme) -> Self {
        // Borderless blocks have no title row. The space stays as the block's
        // top padding, so content does not shift when borders are toggled off.
        if self.no_border {
            return self;
        }
        let title_line = ratatui::text::Line::from(styled_text::parse_styled(title, colors));
        self.block = self
            .block
            .title(title_line)
            .title_style(Style::default().fg(colors.muted));
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
            let border_set = self.border_set;
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

/// Characters for `style = "custom"`, falling back to the plain set when the
/// section is absent. Missing slots were filled with plain glyphs by serde.
fn custom_border_set(chars: Option<&BorderChars>) -> border::Set<'_> {
    let Some(chars) = chars else {
        return border::PLAIN;
    };
    border::Set {
        top_left: &chars.top_left,
        top_right: &chars.top_right,
        bottom_left: &chars.bottom_left,
        bottom_right: &chars.bottom_right,
        vertical_left: &chars.vertical_left,
        vertical_right: &chars.vertical_right,
        horizontal_top: &chars.horizontal_top,
        horizontal_bottom: &chars.horizontal_bottom,
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
        // Custom characters are installed as a `border::Set`, not a type.
        BorderStyle::Custom => BorderType::Plain,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, style::Color};

    use super::*;
    use crate::config::{BorderChars, BorderConfig, BorderStyle};

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

    fn render_title(bordered: bool) -> ratatui::buffer::Buffer {
        let theme = Theme::default();
        let border = BorderConfig {
            enabled: bordered,
            ..BorderConfig::default()
        };
        let bs = BlockStyle {
            colors: &theme,
            border: &border,
            tick: 0,
        };
        let mut term = Terminal::new(TestBackend::new(20, 4)).unwrap();
        term.draw(|f| {
            let block = StyledBlock::from_color(&bs, theme.bg).title("Title", &theme);
            f.render_widget(block, ratatui::layout::Rect::new(0, 0, 20, 4));
        })
        .unwrap();
        term.backend().buffer().clone()
    }

    fn rendered_text(buffer: &ratatui::buffer::Buffer) -> String {
        let area = buffer.area;
        (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .map(|(x, y)| buffer[(x, y)].symbol().to_string())
            .collect()
    }

    #[test]
    fn custom_border_uses_configured_characters() {
        // A Reset border colour makes the block blank its own line segments,
        // which is not what this test checks.
        let theme = Theme {
            border: Color::White,
            ..Theme::default()
        };
        let border = BorderConfig {
            style: BorderStyle::Custom,
            chars: Some(BorderChars {
                top_left: "◆".into(),
                top_right: "◆".into(),
                bottom_left: "◆".into(),
                bottom_right: "◆".into(),
                horizontal_top: "╌".into(),
                horizontal_bottom: "╌".into(),
                vertical_left: "╎".into(),
                vertical_right: "╎".into(),
            }),
            ..BorderConfig::default()
        };
        let bs = BlockStyle {
            colors: &theme,
            border: &border,
            tick: 0,
        };
        let mut term = Terminal::new(TestBackend::new(12, 4)).unwrap();
        term.draw(|f| {
            f.render_widget(
                StyledBlock::from_color(&bs, theme.bg),
                ratatui::layout::Rect::new(0, 0, 12, 4),
            );
        })
        .unwrap();
        let text = rendered_text(term.backend().buffer());
        assert!(text.contains('◆'), "custom corner missing: {text}");
        assert!(text.contains('╌'), "custom horizontal missing: {text}");
        assert!(text.contains('╎'), "custom vertical missing: {text}");
    }

    #[test]
    fn borderless_blocks_drop_the_title() {
        assert!(
            !rendered_text(&render_title(false)).contains("Title"),
            "borderless blocks should not draw a title"
        );
        assert!(
            rendered_text(&render_title(true)).contains("Title"),
            "bordered blocks keep their title"
        );
    }
}
