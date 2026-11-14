//! Two-column content cards: cover on the left, title/subtitle on the right.
//!
//! This is the sibling of `table`: `content` picks one of the two renderers
//! per frame. Bordered mode wraps each card in a `StyledBlock`; borderless
//! mode uses the same block for its background.

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Padding, Paragraph, StatefulWidget, Widget},
};
use ratatui_image::{Resize, StatefulImage};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::{BlockStyle, block::StyledBlock, scrollbar::render_scrollbar};
use crate::{app::cover::ContentCovers, icons, lock::lock_recover, state::ContentState};

const COLUMNS: usize = 2;
const COLUMN_GAP: u16 = 2;
const BORDERED_CONTENT_HEIGHT: u16 = 2;
const BORDERLESS_CONTENT_HEIGHT: u16 = 3;
const MAX_COVER_WIDTH: u16 = 12;
const MIN_TEXT_WIDTH: u16 = 6;
const MIN_COLUMN_WIDTH: u16 = 14;

fn content_height(bs: &BlockStyle<'_>) -> u16 {
    if bs.border.enabled {
        BORDERED_CONTENT_HEIGHT
    } else {
        BORDERLESS_CONTENT_HEIGHT
    }
}

fn row_height(bs: &BlockStyle<'_>) -> u16 {
    content_height(bs) + if bs.border.enabled { 2 } else { 0 }
}

/// Borderless cards are separated by one blank screen row; bordered cards
/// already have their own outline and stay adjacent.
fn row_gap(bs: &BlockStyle<'_>) -> u16 {
    if bs.border.enabled { 0 } else { 1 }
}

fn row_stride(bs: &BlockStyle<'_>) -> u16 {
    row_height(bs) + row_gap(bs)
}

/// Whether the content area has room for two usable card columns.
pub(super) fn fits(area: Rect, count: usize, bs: &BlockStyle<'_>) -> bool {
    count > 0
        && column_width(area.width.saturating_sub(1)) >= MIN_COLUMN_WIDTH
        && area.height >= row_height(bs)
}

fn column_width(list_width: u16) -> u16 {
    list_width.saturating_sub(COLUMN_GAP * (COLUMNS as u16 - 1)) / COLUMNS as u16
}

fn column_areas(list_area: Rect) -> [Rect; COLUMNS] {
    let areas = Layout::horizontal(vec![Constraint::Fill(1); COLUMNS])
        .spacing(COLUMN_GAP)
        .split(list_area);
    // `split` yields one rect per constraint, but avoid unwrapping here: a
    // zero-sized area must not be able to panic the render loop.
    let mut columns = [Rect::default(); COLUMNS];
    for (slot, area) in columns.iter_mut().zip(areas.iter()) {
        *slot = *area;
    }
    columns
}

trait Canvas {
    fn render<W: Widget>(&mut self, widget: W, area: Rect);
    fn render_stateful<W: StatefulWidget>(&mut self, widget: W, area: Rect, state: &mut W::State);
    fn fill(&mut self, area: Rect, style: Style);
}

impl Canvas for Frame<'_> {
    fn render<W: Widget>(&mut self, widget: W, area: Rect) {
        self.render_widget(widget, area);
    }

    fn render_stateful<W: StatefulWidget>(&mut self, widget: W, area: Rect, state: &mut W::State) {
        self.render_stateful_widget(widget, area, state);
    }

    fn fill(&mut self, area: Rect, style: Style) {
        self.render_widget(Block::default().style(style), area);
    }
}

impl Canvas for Buffer {
    fn render<W: Widget>(&mut self, widget: W, area: Rect) {
        widget.render(area, self);
    }

    fn render_stateful<W: StatefulWidget>(&mut self, widget: W, area: Rect, state: &mut W::State) {
        widget.render(area, self, state);
    }

    fn fill(&mut self, area: Rect, style: Style) {
        self.set_style(area, style);
    }
}

/// Immutable inputs shared by every card in one frame. Keeping them together
/// makes the read-only part of a render pass explicit and keeps `draw` /
/// `draw_card` under clippy's argument limit.
pub(super) struct CardView<'v, 's> {
    pub(super) content: &'v ContentState,
    pub(super) bs: &'v BlockStyle<'s>,
    pub(super) covers: &'v ContentCovers,
    pub(super) font_size: ratatui_image::FontSize,
}

pub(super) fn draw(
    f: &mut Frame,
    view: CardView<'_, '_>,
    selected: usize,
    content_columns: &mut usize,
    content_scroll: &mut usize,
    content_visible: &mut std::ops::Range<usize>,
    area: Rect,
) {
    let total = view.content.len();
    if total == 0 {
        return;
    }
    *content_columns = COLUMNS;

    let colors = view.bs.colors;
    let list_area = Rect {
        width: area.width.saturating_sub(1),
        ..area
    };
    let columns = column_areas(list_area);
    let width = columns[0].width;
    let height = row_height(view.bs);
    let stride = row_stride(view.bs);
    if height == 0 || stride == 0 || width < MIN_COLUMN_WIDTH {
        return;
    }

    let sel = selected.min(total - 1);
    let total_rows = total.div_ceil(COLUMNS);
    let viewport = list_area.height as usize;
    let content_height =
        total_rows * height as usize + total_rows.saturating_sub(1) * row_gap(view.bs) as usize;
    let max_scroll = content_height.saturating_sub(viewport);
    let selected_row = sel / COLUMNS;
    let selected_top = selected_row * stride as usize;
    let selected_bottom = selected_top + height as usize;
    let margin = 1.min(viewport / 4);

    // Scroll by screen rows, not by whole card rows, so cards slide in and
    // out of the viewport exactly like the reference implementation.
    if selected_top < *content_scroll + margin {
        *content_scroll = selected_top.saturating_sub(margin);
    } else if selected_bottom > *content_scroll + viewport.saturating_sub(margin) {
        *content_scroll = (selected_bottom + margin).saturating_sub(viewport);
    }
    *content_scroll = (*content_scroll).min(max_scroll);

    render_scrollbar(
        f,
        total_rows,
        selected_row,
        Rect {
            x: area.right().saturating_sub(1),
            width: 1,
            ..area
        },
        colors.muted,
    );

    let first_row = *content_scroll / stride as usize;
    let last_row = ((*content_scroll + viewport).div_ceil(stride as usize)).min(total_rows);
    *content_visible = (first_row * COLUMNS)..(last_row * COLUMNS).min(total);
    for row in first_row..last_row {
        let card_top = (row * stride as usize) as isize - *content_scroll as isize;
        let dest_y = list_area.y as isize + card_top;
        if dest_y >= list_area.bottom() as isize || dest_y + height as isize <= list_area.y as isize
        {
            continue;
        }
        for (col, column) in columns.iter().enumerate() {
            let index = row * COLUMNS + col;
            if index >= total {
                break;
            }
            let x = column.x;
            let card_width = column.width;
            if dest_y >= list_area.y as isize
                && dest_y + height as isize <= list_area.bottom() as isize
            {
                // Fully visible: render straight into the frame. Kitty and
                // Sixel protocols must not be copied through a scratch buffer.
                draw_card(
                    f,
                    Rect::new(x, dest_y as u16, card_width, height),
                    &view,
                    index,
                    index == sel,
                );
            } else {
                let mut card = Buffer::empty(Rect::new(0, 0, card_width, height));
                let card_area = card.area;
                let opaque = draw_card(&mut card, card_area, &view, index, index == sel);
                blit_card(f.buffer_mut(), &card, x, dest_y, list_area, opaque);
            }
        }
    }
}

/// Copy the visible part of a pre-rendered card into the frame. Rendering into
/// a temporary buffer first means the top/bottom border is clipped by the
/// viewport instead of being forced onto the viewport edge.
fn blit_card(
    frame: &mut Buffer,
    card: &Buffer,
    x: u16,
    dest_y: isize,
    viewport: Rect,
    opaque: bool,
) {
    let top = dest_y.max(viewport.y as isize);
    let bottom = (dest_y + card.area.height as isize).min(viewport.bottom() as isize);
    if bottom <= top {
        return;
    }
    let src_y = (top - dest_y) as u16;
    let dst_y = top as u16;
    let rows = (bottom - top) as u16;
    for dy in 0..rows {
        for dx in 0..card.area.width {
            let cell = &card[(dx, src_y + dy)];
            // Bordered cards are transparent: skip untouched cells so the
            // page background is not replaced by the temporary buffer.
            if !opaque && cell.symbol() == " " && cell.style() == Style::default() {
                continue;
            }
            frame[(x + dx, dst_y + dy)] = cell.clone();
        }
    }
}

fn draw_card<C: Canvas>(
    canvas: &mut C,
    rect: Rect,
    view: &CardView<'_, '_>,
    index: usize,
    selected: bool,
) -> bool {
    let colors = view.bs.colors;
    let fill = if view.bs.border.enabled {
        None
    } else if selected {
        Some(colors.selection())
    } else {
        Some(colors.surface)
    };
    if let Some(bg) = fill {
        canvas.fill(rect, Style::default().bg(bg));
    }
    let opaque = fill.is_some();

    let mut block = StyledBlock::from_color(view.bs, fill.unwrap_or(colors.bg));
    if selected && view.bs.border.enabled {
        let selection = colors.selection();
        block = block.corner_color(selection).border_color(selection);
    }
    let padding = if view.bs.border.enabled {
        Padding::horizontal(1)
    } else {
        // Borderless cards start the cover at the card edge; only the text
        // side needs padding.
        Padding::new(0, 1, 0, 0)
    };
    let block = block.content_padding(padding);
    let inner = block.inner(rect);
    canvas.render(block, rect);

    let Some((cover_url, title, subtitle)) = item(view.content, index) else {
        return opaque;
    };
    if inner.width == 0 || inner.height == 0 {
        return opaque;
    }

    // The cover fills the card's content height. Terminal cells are roughly
    // twice as tall as wide, so a 1:1 image needs a width of 2 x height.
    let cover_height = inner.height;
    // A terminal cell is rarely exactly 2:1. Size the cover by the actual
    // font ratio so a square image fills the height without cropping its
    // subject; `Resize::Fit` then only scales it down.
    let font_ratio = view.font_size.height as f32 / view.font_size.width.max(1) as f32;
    let cover_width = ((cover_height as f32 * font_ratio).round() as u16)
        .min(MAX_COVER_WIDTH)
        .min(inner.width.saturating_sub(MIN_TEXT_WIDTH + 1))
        .max(1);
    let cover = Rect::new(inner.x, inner.y, cover_width, cover_height);
    draw_cover(canvas, cover, cover_url, view);

    let text_x = cover.right().saturating_add(1);
    let text_width = inner.right().saturating_sub(text_x);
    if text_width == 0 {
        return opaque;
    }
    let title = fit(title, text_width);
    let text_y = inner.y + inner.height.saturating_sub(2) / 2;
    canvas.render(
        Paragraph::new(Line::from(Span::styled(
            title,
            Style::default()
                .fg(colors.text)
                .add_modifier(Modifier::BOLD),
        ))),
        Rect::new(text_x, text_y, text_width, 1),
    );
    if inner.height >= 2
        && let Some(subtitle) = subtitle
    {
        let subtitle = fit(subtitle, text_width);
        canvas.render(
            Paragraph::new(Line::from(Span::styled(
                subtitle,
                Style::default().fg(colors.muted),
            ))),
            Rect::new(text_x, text_y + 1, text_width, 1),
        );
    }
    opaque
}

fn draw_cover<C: Canvas>(canvas: &mut C, area: Rect, url: &str, view: &CardView<'_, '_>) {
    if !url.is_empty()
        && let Some(protocol) = lock_recover(view.covers).get_mut(url)
    {
        canvas.render_stateful(
            StatefulImage::new().resize(Resize::Fit(None)),
            area,
            protocol,
        );
        return;
    }
    canvas.fill(area, Style::default().bg(view.bs.colors.surface));
    canvas.render(
        Paragraph::new(icons::MUSIC_NOTE)
            .alignment(Alignment::Center)
            .style(Style::default().fg(view.bs.colors.muted)),
        area,
    );
}

/// Cover URL, title and optional subtitle for one row.
fn item(content: &ContentState, index: usize) -> Option<(&str, &str, Option<&str>)> {
    match content {
        ContentState::Songs(songs) => songs.get(index).map(|song| {
            (
                song.pic_url.as_str(),
                song.name.as_str(),
                Some(song.singer.as_str()),
            )
        }),
        ContentState::SongLists(lists) => lists.get(index).map(|list| {
            (
                list.cover_img_url.as_str(),
                list.name.as_str(),
                Some(list.author.as_str()),
            )
        }),
        ContentState::TopLists(lists) => lists.get(index).map(|list| {
            (
                list.cover.as_str(),
                list.name.as_str(),
                Some(list.update.as_str()),
            )
        }),
        ContentState::Singers(singers) => singers
            .get(index)
            .map(|singer| (singer.pic_url.as_str(), singer.name.as_str(), None)),
        ContentState::HotSearch(keywords) => keywords
            .0
            .get(index)
            .map(|keyword| ("", keyword.as_str(), None)),
        _ => None,
    }
}

/// Truncate to `width` terminal cells, adding an ellipsis when text is cut.
fn fit(text: &str, width: u16) -> String {
    let width = width as usize;
    if width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width >= width {
            break;
        }
        out.push(ch);
        used += ch_width;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use ncm_api::{SongCopyright, SongInfo};
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::config::{BorderConfig, Theme};

    fn song(id: u64) -> Arc<SongInfo> {
        Arc::new(SongInfo {
            id,
            name: format!("Test Song {id}"),
            singer: "Tester".into(),
            artist_id: 2,
            album: "Album".into(),
            album_id: 3,
            pic_url: String::new(),
            duration: 180_000,
            copyright: SongCopyright::Free,
        })
    }

    #[test]
    fn card_rows_are_compact() {
        let theme = Theme::default();
        let border = BorderConfig::default();
        let bs = BlockStyle {
            colors: &theme,
            border: &border,
            tick: 0,
        };
        let content = ContentState::Songs(vec![song(1), song(2)]);
        let covers: ContentCovers = Arc::new(Mutex::new(HashMap::new()));
        let mut columns = 0;
        let mut scroll = 0;
        let font_size = ratatui_image::FontSize {
            width: 8,
            height: 16,
        };
        let mut visible = 0..0;
        let mut term = Terminal::new(TestBackend::new(60, 10)).unwrap();
        term.draw(|f| {
            draw(
                f,
                CardView {
                    content: &content,
                    bs: &bs,
                    covers: &covers,
                    font_size,
                },
                0,
                &mut columns,
                &mut scroll,
                &mut visible,
                Rect::new(0, 0, 60, 10),
            )
        })
        .unwrap();
        assert_eq!(columns, 2);
        let buffer = term.backend().buffer();
        let text: String = (0..buffer.area().height)
            .flat_map(|y| (0..buffer.area().width).map(move |x| (x, y)))
            .map(|(x, y)| buffer[(x, y)].symbol().to_string())
            .collect();
        assert!(text.contains("Test Song 1"), "{text}");
        assert!(
            text.contains('┌'),
            "bordered row should draw an outline: {text}"
        );
        assert_eq!(
            buffer[(0, 0)].fg,
            theme.selection(),
            "selected bordered card should use the selection colour for its border"
        );
        assert_ne!(
            buffer[(10, 1)].style().bg,
            Some(theme.selection()),
            "bordered cards must not be filled with the selection colour"
        );
    }

    #[test]
    fn selected_borderless_row_uses_the_selection_colour() {
        let theme = Theme::default();
        let border = BorderConfig {
            enabled: false,
            ..BorderConfig::default()
        };
        let bs = BlockStyle {
            colors: &theme,
            border: &border,
            tick: 0,
        };
        let content = ContentState::Songs(vec![song(1), song(2)]);
        let covers: ContentCovers = Arc::new(Mutex::new(HashMap::new()));
        let mut columns = 0;
        let mut scroll = 0;
        let font_size = ratatui_image::FontSize {
            width: 8,
            height: 16,
        };
        let mut visible = 0..0;
        let mut term = Terminal::new(TestBackend::new(60, 10)).unwrap();
        term.draw(|f| {
            draw(
                f,
                CardView {
                    content: &content,
                    bs: &bs,
                    covers: &covers,
                    font_size,
                },
                0,
                &mut columns,
                &mut scroll,
                &mut visible,
                Rect::new(0, 0, 60, 10),
            )
        })
        .unwrap();
        assert_eq!(columns, 2);
        let buffer = term.backend().buffer();
        assert_eq!(buffer[(20, 1)].style().bg, Some(theme.selection()));
    }

    #[test]
    fn scrolled_rows_show_later_items() {
        let theme = Theme::default();
        let border = BorderConfig::default();
        let bs = BlockStyle {
            colors: &theme,
            border: &border,
            tick: 0,
        };
        let content = ContentState::Songs((1..=20).map(song).collect());
        let covers: ContentCovers = Arc::new(Mutex::new(HashMap::new()));
        let mut columns = 0;
        let mut scroll = 0;
        let font_size = ratatui_image::FontSize {
            width: 8,
            height: 16,
        };
        let mut visible = 0..0;
        let mut term = Terminal::new(TestBackend::new(60, 14)).unwrap();
        term.draw(|f| {
            draw(
                f,
                CardView {
                    content: &content,
                    bs: &bs,
                    covers: &covers,
                    font_size,
                },
                10,
                &mut columns,
                &mut scroll,
                &mut visible,
                Rect::new(0, 0, 60, 14),
            )
        })
        .unwrap();
        let buffer = term.backend().buffer();
        let text: String = (0..buffer.area().height)
            .flat_map(|y| (0..buffer.area().width).map(move |x| (x, y)))
            .map(|(x, y)| buffer[(x, y)].symbol().to_string())
            .collect();
        assert!(
            text.contains("Test Song 11"),
            "missing scrolled item: {text}"
        );
    }

    #[test]
    fn narrow_area_falls_back() {
        let theme = Theme::default();
        let border = BorderConfig::default();
        let bs = BlockStyle {
            colors: &theme,
            border: &border,
            tick: 0,
        };
        assert!(!fits(Rect::new(0, 0, 15, 10), 2, &bs));
    }
}
