//! Two-column content cards: cover on the left, title/subtitle on the right.
//!
//! This is the sibling of `table`: `content` picks one of the two renderers
//! per frame. Bordered mode wraps each card in a `StyledBlock`; borderless
//! mode uses the same block for its background.

use std::borrow::Cow;

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Padding, Paragraph, StatefulWidget, Widget},
};
use ratatui_image::{Resize, StatefulImage};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::{BlockStyle, block::StyledBlock, scrollbar::render_scrollbar};
use crate::{
    cache::cover::{ContentCovers, SINGER_COVER_MAX_PX},
    config::ContentType,
    icons,
    lock::lock_recover,
    state::ContentState,
};

/// Card grid used for the current content type.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Grid {
    /// Two columns: cover on the left, title/subtitle on the right.
    Row,
    /// Portrait cards: a square image on top with the name centred underneath,
    /// up to [`PORTRAIT_COLUMNS`] per row.
    Portrait,
}

const ROW_COLUMNS: usize = 2;
const ROW_COLUMN_GAP: u16 = 2;
/// Row cards keep the same odd content height bordered and borderless, so the
/// right-end marker has a real middle row to sit on.
const ROW_CONTENT_HEIGHT: u16 = 3;
const MAX_COVER_WIDTH: u16 = 12;
const MIN_TEXT_WIDTH: u16 = 6;
const MIN_COLUMN_WIDTH: u16 = 14;

/// Portrait grids trade card count for image size: at most five per row.
const PORTRAIT_COLUMNS: usize = 5;
const PORTRAIT_GAP: u16 = 2;
const PORTRAIT_MIN_WIDTH: u16 = 10;
/// Blank row between the card top and the image.
const PORTRAIT_TOP_PAD: u16 = 1;
/// One row for the centred name.
const PORTRAIT_NAME_ROWS: u16 = 1;
/// Blank row under the name, matching the top padding.
const PORTRAIT_BOTTOM_PAD: u16 = 1;

impl Grid {
    fn for_content(content: &ContentState) -> Self {
        if content.content_type() == ContentType::Singers {
            Self::Portrait
        } else {
            Self::Row
        }
    }
}

/// Geometry of one card row for the active grid.
#[derive(Clone, Copy)]
struct Metrics {
    columns: usize,
    width: u16,
    height: u16,
    gap: u16,
}

impl Metrics {
    fn stride(self, bs: &BlockStyle<'_>) -> u16 {
        self.height + row_gap(bs)
    }
}

fn border_rows(bs: &BlockStyle<'_>) -> u16 {
    if bs.border.enabled { 2 } else { 0 }
}

fn metrics(
    grid: Grid,
    list_width: u16,
    bs: &BlockStyle<'_>,
    font_size: ratatui_image::FontSize,
) -> Metrics {
    match grid {
        Grid::Row => {
            let width = list_width.saturating_sub(ROW_COLUMN_GAP * (ROW_COLUMNS as u16 - 1))
                / ROW_COLUMNS as u16;
            Metrics {
                columns: ROW_COLUMNS,
                width,
                height: ROW_CONTENT_HEIGHT + border_rows(bs),
                gap: ROW_COLUMN_GAP,
            }
        }
        Grid::Portrait => {
            let columns = portrait_columns(list_width);
            let width = (list_width.saturating_sub(PORTRAIT_GAP * (columns as u16 - 1))
                / columns as u16)
                .max(1);
            let inner_width = width.saturating_sub(border_rows(bs));
            let image_width = portrait_width(font_size, inner_width);
            let content = PORTRAIT_TOP_PAD
                + portrait_height(font_size, image_width)
                + PORTRAIT_NAME_ROWS
                + PORTRAIT_BOTTOM_PAD;
            Metrics {
                columns,
                width,
                height: content + border_rows(bs),
                gap: PORTRAIT_GAP,
            }
        }
    }
}

/// How many portrait cards fit across `list_width`, capped at
/// [`PORTRAIT_COLUMNS`] so the images stay large.
fn portrait_columns(list_width: u16) -> usize {
    let fit = (list_width + PORTRAIT_GAP) / (PORTRAIT_MIN_WIDTH + PORTRAIT_GAP);
    (fit.max(1) as usize).min(PORTRAIT_COLUMNS)
}

/// Portrait width in cells, capped so the decoded thumbnail still fills it.
/// `Resize::Fit` never upscales and `Resize::Scale` re-encodes on every frame,
/// so the cell area must not exceed the decoded image.
fn portrait_width(font_size: ratatui_image::FontSize, available: u16) -> u16 {
    let max = (SINGER_COVER_MAX_PX / font_size.width.max(1) as u32).max(1) as u16;
    available.min(max).max(1)
}

/// Image height in cells that keeps a `width`-wide portrait square in pixels.
fn portrait_height(font_size: ratatui_image::FontSize, width: u16) -> u16 {
    let ratio = font_size.height as f32 / font_size.width.max(1) as f32;
    ((width as f32 / ratio).round() as u16).clamp(3, 12)
}

/// Borderless cards are separated by one blank screen row; bordered cards
/// already have their own outline and stay adjacent.
fn row_gap(bs: &BlockStyle<'_>) -> u16 {
    if bs.border.enabled { 0 } else { 1 }
}

/// Whether the content area has room for at least one usable card.
pub(super) fn fits(
    area: Rect,
    content: &ContentState,
    bs: &BlockStyle<'_>,
    font_size: ratatui_image::FontSize,
) -> bool {
    if content.is_empty() {
        return false;
    }
    let grid = Grid::for_content(content);
    let m = metrics(grid, area.width.saturating_sub(1), bs, font_size);
    let width_ok = match grid {
        Grid::Row => m.width >= MIN_COLUMN_WIDTH,
        Grid::Portrait => m.width >= PORTRAIT_MIN_WIDTH,
    };
    width_ok && area.height >= m.height
}

/// `(x, width)` of every card column in one row.
fn card_columns(
    grid: Grid,
    list_area: Rect,
    bs: &BlockStyle<'_>,
    font_size: ratatui_image::FontSize,
) -> Vec<(u16, u16)> {
    let m = metrics(grid, list_area.width, bs, font_size);
    (0..m.columns)
        .map(|i| (list_area.x + i as u16 * (m.width + m.gap), m.width))
        .collect()
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

/// Per-frame values that come from live app state instead of the content list:
/// where the pointer is, which song plays, and the animation clock.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct LiveInputs {
    pub(super) mouse: Option<(u16, u16)>,
    pub(super) playing_song: Option<u64>,
    pub(super) playing: bool,
    pub(super) tick: u64,
}

/// What sits at the right end of a card, next to the truncated title.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Marker {
    /// Hovered card: a play triangle over the selection colour.
    Hover,
    /// Card of the song that is playing: an animated loading glyph.
    Playing,
    /// Card of the current song while paused. Borderless cards only: a
    /// bordered card shows no triangle unless the pointer is on it.
    Paused,
}

impl Marker {
    fn for_card(hovered: bool, current: bool, playing: bool, bordered: bool) -> Option<Self> {
        if hovered {
            Some(Self::Hover)
        } else if current && playing {
            Some(Self::Playing)
        } else if current && !bordered {
            Some(Self::Paused)
        } else {
            None
        }
    }

    /// Columns the marker needs, including one column of separation from the
    /// title.
    fn reserved(self) -> u16 {
        MARKER_WIDTH + 1
    }
}

/// Width of the marker block: a single glyph padded on both sides.
const MARKER_WIDTH: u16 = 3;

/// Render the card grid and return the layout it used. `None` means this
/// geometry cannot host cards, so `content::render_content` falls back to the
/// table renderer instead of leaving an empty content area.
pub(super) fn draw(
    f: &mut Frame,
    view: CardView<'_, '_>,
    live: LiveInputs,
    selected: usize,
    current_scroll: usize,
    area: Rect,
) -> Option<super::ContentLayout> {
    let total = view.content.len();
    if total == 0 {
        return None;
    }

    let colors = view.bs.colors;
    let grid = Grid::for_content(view.content);
    let list_area = Rect {
        width: area.width.saturating_sub(1),
        ..area
    };
    let m = metrics(grid, list_area.width, view.bs, view.font_size);
    let columns = card_columns(grid, list_area, view.bs, view.font_size);
    let column_count = columns.len();
    let height = m.height;
    let stride = m.stride(view.bs);
    if column_count == 0 || height == 0 || stride == 0 {
        return None;
    }
    if grid == Grid::Row && columns[0].1 < MIN_COLUMN_WIDTH {
        return None;
    }

    // `content_selected` is clamped where content is mutated, so the renderer
    // and the input handlers always agree on which row is highlighted.
    let sel = selected;
    let total_rows = total.div_ceil(column_count);
    let viewport = list_area.height as usize;
    let content_height =
        total_rows * height as usize + total_rows.saturating_sub(1) * row_gap(view.bs) as usize;
    let max_scroll = content_height.saturating_sub(viewport);
    let selected_row = sel / column_count;
    let selected_top = selected_row * stride as usize;
    let selected_bottom = selected_top + height as usize;
    let margin = 1.min(viewport / 4);

    // Scroll by screen rows, not by whole card rows, so cards slide in and
    // out of the viewport exactly like the reference implementation.
    let mut scroll = current_scroll;
    if selected_top < scroll + margin {
        scroll = selected_top.saturating_sub(margin);
    } else if selected_bottom > scroll + viewport.saturating_sub(margin) {
        scroll = (selected_bottom + margin).saturating_sub(viewport);
    }
    scroll = scroll.min(max_scroll);

    render_scrollbar(
        f,
        total_rows,
        selected_row.min(total_rows.saturating_sub(1)),
        Rect {
            x: area.right().saturating_sub(1),
            width: 1,
            ..area
        },
        colors.muted,
    );

    let first_row = scroll / stride as usize;
    let last_row = ((scroll + viewport).div_ceil(stride as usize)).min(total_rows);
    let visible = (first_row * column_count)..(last_row * column_count).min(total);
    // One reusable buffer per column for cards clipped by the viewport edge.
    let mut scratch: Vec<Option<Buffer>> = (0..column_count).map(|_| None).collect();
    let mut hovered_item = None;
    for row in first_row..last_row {
        let card_top = (row * stride as usize) as isize - scroll as isize;
        let dest_y = list_area.y as isize + card_top;
        if dest_y >= list_area.bottom() as isize || dest_y + height as isize <= list_area.y as isize
        {
            continue;
        }
        for (col, &(x, card_width)) in columns.iter().enumerate() {
            let index = row * column_count + col;
            if index >= total {
                break;
            }
            let card_rect = Rect::new(x, dest_y.max(0) as u16, card_width, height);
            let hovered = dest_y >= 0
                && live
                    .mouse
                    .is_some_and(|(mx, my)| card_rect.contains((mx, my).into()));
            if hovered {
                hovered_item = Some(index);
            }
            if dest_y >= list_area.y as isize
                && dest_y + height as isize <= list_area.bottom() as isize
            {
                // Fully visible: render straight into the frame. Kitty and
                // Sixel protocols must not be copied through a scratch buffer.
                draw_any(
                    f,
                    card_rect,
                    &view,
                    grid,
                    live,
                    index,
                    index == sel,
                    hovered,
                );
            } else {
                let card = scratch_buffer(&mut scratch[col], card_width, height);
                let card_area = card.area;
                let opaque = draw_any(
                    card,
                    card_area,
                    &view,
                    grid,
                    live,
                    index,
                    index == sel,
                    hovered,
                );
                blit_card(f.buffer_mut(), card, x, dest_y, list_area, opaque);
            }
        }
    }

    Some(super::ContentLayout {
        columns: column_count,
        scroll,
        visible,
        hovered: hovered_item,
    })
}

/// Dispatch to the layout-specific card renderer.
#[allow(clippy::too_many_arguments)]
fn draw_any<C: Canvas>(
    canvas: &mut C,
    rect: Rect,
    view: &CardView<'_, '_>,
    grid: Grid,
    live: LiveInputs,
    index: usize,
    selected: bool,
    hovered: bool,
) -> bool {
    match grid {
        Grid::Row => draw_card(canvas, rect, view, live, index, selected, hovered),
        Grid::Portrait => draw_portrait_card(canvas, rect, view, index, selected, hovered),
    }
}

/// Reuse one scratch buffer per column for cards clipped by the viewport edge.
/// Fully visible cards never go through this path.
fn scratch_buffer(slot: &mut Option<Buffer>, width: u16, height: u16) -> &mut Buffer {
    let area = Rect::new(0, 0, width, height);
    if let Some(buffer) = slot {
        if buffer.area == area {
            buffer.reset();
        } else {
            *buffer = Buffer::empty(area);
        }
        buffer
    } else {
        slot.insert(Buffer::empty(area))
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

/// Paint the card background and border, and return the content area.
fn draw_card_frame<C: Canvas>(
    canvas: &mut C,
    rect: Rect,
    view: &CardView<'_, '_>,
    selected: bool,
    hovered: bool,
    padding: Padding,
) -> (Rect, bool) {
    let colors = view.bs.colors;
    let fill = if view.bs.border.enabled {
        None
    } else if selected || hovered {
        // Keyboard selection and pointer hover share the blend; the play
        // marker draws on `bg` so it still stands out on top of it.
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
        // A gradient border takes precedence inside `StyledBlock::render`.
        block = block
            .corner_color(colors.accent)
            .border_color(colors.accent);
    }
    let block = block.content_padding(padding);
    let inner = block.inner(rect);
    canvas.render(block, rect);
    (inner, opaque)
}

/// Portrait card: a square image on top and the name centred underneath.
fn draw_portrait_card<C: Canvas>(
    canvas: &mut C,
    rect: Rect,
    view: &CardView<'_, '_>,
    index: usize,
    selected: bool,
    hovered: bool,
) -> bool {
    let colors = view.bs.colors;
    let (inner, opaque) = draw_card_frame(
        canvas,
        rect,
        view,
        selected,
        hovered,
        Padding::new(0, 0, 0, 0),
    );
    let Some((cover_url, title, _)) = item(view.content, index) else {
        return opaque;
    };
    if inner.width == 0 || inner.height == 0 {
        return opaque;
    }

    let avatar_width = portrait_width(view.font_size, inner.width);
    let avatar_height = portrait_height(view.font_size, avatar_width).min(
        inner
            .height
            .saturating_sub(PORTRAIT_TOP_PAD + PORTRAIT_NAME_ROWS + PORTRAIT_BOTTOM_PAD),
    );
    if avatar_height > 0 {
        // Centre the portrait; cards wider than the decoded thumbnail keep an
        // even margin instead of hugging the left edge.
        let x = inner.x + inner.width.saturating_sub(avatar_width) / 2;
        let avatar = Rect::new(x, inner.y + PORTRAIT_TOP_PAD, avatar_width, avatar_height);
        draw_cover(canvas, avatar, cover_url, view);
    }

    let name = fit(title, inner.width);
    let name_y = (inner.y + PORTRAIT_TOP_PAD + avatar_height)
        .min(inner.bottom().saturating_sub(PORTRAIT_NAME_ROWS));
    canvas.render(
        Paragraph::new(Line::from(Span::styled(
            name,
            Style::default()
                .fg(colors.text)
                .add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center),
        Rect::new(inner.x, name_y, inner.width, PORTRAIT_NAME_ROWS),
    );
    opaque
}

fn draw_card<C: Canvas>(
    canvas: &mut C,
    rect: Rect,
    view: &CardView<'_, '_>,
    live: LiveInputs,
    index: usize,
    selected: bool,
    hovered: bool,
) -> bool {
    let colors = view.bs.colors;
    let padding = if view.bs.border.enabled {
        Padding::horizontal(1)
    } else {
        // Borderless cards start the cover at the card edge; only the text
        // side needs padding.
        Padding::new(0, 1, 0, 0)
    };
    let (inner, opaque) = draw_card_frame(canvas, rect, view, selected, hovered, padding);

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
    let current = is_current_song(view.content, index, live.playing_song);
    let marker = Marker::for_card(hovered, current, live.playing, view.bs.border.enabled)
        .filter(|marker| text_width > marker.reserved());
    let name_width = text_width - marker.map_or(0, Marker::reserved);
    let text_y = inner.y + inner.height.saturating_sub(2) / 2;
    let title = fit(title, name_width);
    canvas.render(
        Paragraph::new(Line::from(Span::styled(
            title,
            Style::default()
                .fg(colors.text)
                .add_modifier(Modifier::BOLD),
        ))),
        Rect::new(text_x, text_y, name_width, 1),
    );
    if inner.height >= 2
        && let Some(subtitle) = subtitle
    {
        let subtitle = fit(subtitle, name_width);
        canvas.render(
            Paragraph::new(Line::from(Span::styled(
                subtitle,
                Style::default().fg(colors.muted),
            ))),
            Rect::new(text_x, text_y + 1, name_width, 1),
        );
    }
    if let Some(marker) = marker {
        // Centre on the card's content height, as a cover-centred card does:
        // the odd content height gives a real middle row.
        let marker_y = inner.y + inner.height / 2;
        draw_marker(
            canvas,
            Rect::new(inner.right() - MARKER_WIDTH, marker_y, MARKER_WIDTH, 1),
            marker,
            live,
            colors,
        );
    }
    opaque
}

/// Whether the card at `index` is the song playing now. Only song lists can
/// match: playlists, singers and hot searches are not playback items.
fn is_current_song(content: &ContentState, index: usize, playing: Option<u64>) -> bool {
    let Some(playing) = playing else {
        return false;
    };
    matches!(
        content,
        ContentState::Songs(songs)
            if songs.get(index).is_some_and(|song| song.id == playing)
    )
}

/// The marker at the right end of a card: a hovered card offers a play button,
/// the playing card shows the loading spinner, and a paused current song keeps
/// a static triangle.
fn draw_marker<C: Canvas>(
    canvas: &mut C,
    area: Rect,
    marker: Marker,
    live: LiveInputs,
    colors: &crate::config::Theme,
) {
    match marker {
        Marker::Hover => {
            // The block sits on the page background so it stays visible on the
            // selection-tinted card behind it.
            canvas.fill(area, Style::default().bg(colors.bg));
            canvas.render(
                Paragraph::new(icons::PLAY_INDICATOR)
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(colors.accent).bg(colors.bg)),
                area,
            );
        }
        Marker::Playing => {
            let frame = icons::LOADING_FRAMES[live.tick as usize % icons::LOADING_FRAMES.len()];
            canvas.render(
                Paragraph::new(frame)
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(colors.accent)),
                area,
            );
        }
        Marker::Paused => {
            canvas.render(
                Paragraph::new(icons::PLAY_INDICATOR)
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(colors.accent)),
                area,
            );
        }
    }
}

fn draw_cover<C: Canvas>(canvas: &mut C, area: Rect, url: &str, view: &CardView<'_, '_>) {
    if !url.is_empty()
        && let Some(protocol) = lock_recover(view.covers).get_mut(url)
    {
        // `Resize::Fit` reuses the encoded thumbnail while the area is stable;
        // `Resize::Scale` re-encodes on every frame and burns CPU.
        canvas.render_stateful(
            StatefulImage::new().resize(Resize::Fit(None)),
            area,
            protocol,
        );
        return;
    }
    canvas.fill(area, Style::default().bg(view.bs.colors.surface));
    // A one-line paragraph starts on the area's first row; centre the glyph in
    // a single cell both ways. Paragraph centring uses the glyph's unicode
    // width (2 for `♪` under the cjk table), which puts odd-width covers one
    // column off.
    let cell = Rect::new(
        area.x + area.width.saturating_sub(1) / 2,
        area.y + area.height.saturating_sub(1) / 2,
        1,
        1,
    );
    canvas.render(
        Paragraph::new(icons::MUSIC_NOTE).style(Style::default().fg(view.bs.colors.muted)),
        cell,
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
/// The common case (text already fits) borrows the input instead of allocating
/// a fresh `String` on every frame.
fn fit<'a>(text: &'a str, width: u16) -> Cow<'a, str> {
    let width = width as usize;
    if width == 0 {
        return Cow::Borrowed("");
    }
    if UnicodeWidthStr::width(text) <= width {
        return Cow::Borrowed(text);
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
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use std::{borrow::Cow, sync::Arc};

    use ncm_api::{SongCopyright, SongInfo};
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::config::{BorderConfig, GradientPreset, Theme};

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
        let covers = ContentCovers::default();
        let mut layout = None;
        let font_size = ratatui_image::FontSize {
            width: 8,
            height: 16,
        };
        let mut term = Terminal::new(TestBackend::new(60, 10)).unwrap();
        term.draw(|f| {
            layout = draw(
                f,
                CardView {
                    content: &content,
                    bs: &bs,
                    covers: &covers,
                    font_size,
                },
                LiveInputs::default(),
                0,
                0,
                Rect::new(0, 0, 60, 10),
            );
        })
        .unwrap();
        assert_eq!(layout.map(|l| l.columns), Some(2));
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
            theme.accent,
            "selected bordered card should use the accent colour for its border"
        );
        assert_ne!(
            buffer[(10, 1)].style().bg,
            Some(theme.accent),
            "bordered cards must not be filled with the accent colour"
        );
    }

    #[test]
    fn selected_bordered_card_keeps_gradient_border() {
        let theme = Theme::default();
        let border = BorderConfig {
            border_gradient: Some(GradientPreset::Turbo),
            ..BorderConfig::default()
        };
        let bs = BlockStyle {
            colors: &theme,
            border: &border,
            tick: 0,
        };
        let content = ContentState::Songs(vec![song(1), song(2)]);
        let covers = ContentCovers::default();
        let font_size = ratatui_image::FontSize {
            width: 8,
            height: 16,
        };
        let mut term = Terminal::new(TestBackend::new(60, 10)).unwrap();
        term.draw(|f| {
            let _ = draw(
                f,
                CardView {
                    content: &content,
                    bs: &bs,
                    covers: &covers,
                    font_size,
                },
                LiveInputs::default(),
                0,
                0,
                Rect::new(0, 0, 60, 10),
            );
        })
        .unwrap();
        let buffer = term.backend().buffer();
        let left = buffer[(0, 0)].fg;
        let next = buffer[(1, 0)].fg;
        assert_ne!(
            left, theme.accent,
            "gradient must override the selected solid border colour"
        );
        assert_ne!(left, next, "gradient should vary along the top edge");
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
        let covers = ContentCovers::default();
        let mut layout = None;
        let font_size = ratatui_image::FontSize {
            width: 8,
            height: 16,
        };
        let mut term = Terminal::new(TestBackend::new(60, 14)).unwrap();
        term.draw(|f| {
            layout = draw(
                f,
                CardView {
                    content: &content,
                    bs: &bs,
                    covers: &covers,
                    font_size,
                },
                LiveInputs::default(),
                10,
                0,
                Rect::new(0, 0, 60, 14),
            );
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

    const FONT: ratatui_image::FontSize = ratatui_image::FontSize {
        width: 8,
        height: 16,
    };

    #[test]
    fn narrow_area_falls_back() {
        let theme = Theme::default();
        let border = BorderConfig::default();
        let bs = BlockStyle {
            colors: &theme,
            border: &border,
            tick: 0,
        };
        let content = ContentState::Songs(vec![song(1), song(2)]);
        assert!(!fits(Rect::new(0, 0, 15, 10), &content, &bs, FONT));
    }

    #[test]
    fn singer_cards_are_taller_than_song_cards() {
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
        let songs = ContentState::Songs(vec![song(1), song(2)]);
        let singers = ContentState::Singers(vec![ncm_api::SingerInfo {
            id: 1,
            name: "Singer".into(),
            pic_url: "https://example.com/pic.jpg".into(),
        }]);
        let area = Rect::new(0, 0, 60, 4);
        assert!(
            fits(area, &songs, &bs, FONT),
            "a 3-row song card still fits"
        );
        assert!(
            !fits(area, &singers, &bs, FONT),
            "a cover card with a portrait needs more rows"
        );
    }

    #[test]
    fn singer_cards_put_the_name_under_the_portrait() {
        let content = ContentState::Singers(vec![ncm_api::SingerInfo {
            id: 1,
            name: "艾歌".into(),
            pic_url: String::new(),
        }]);
        let buffer = render_cards(&content, false, 0, LiveInputs::default());
        let pos_of = |needle: &str| {
            (0..buffer.area.height).find_map(|y| {
                (0..buffer.area.width)
                    .find(|&x| buffer[(x, y)].symbol() == needle)
                    .map(|x| (x, y))
            })
        };
        assert_eq!(
            pos_of("艾"),
            Some((3, 6)),
            "the name is centred on the row below the portrait"
        );
        assert_eq!(
            pos_of(icons::MUSIC_NOTE),
            Some((4, 3)),
            "the placeholder is centred inside the portrait"
        );
        assert_eq!(
            buffer[(0, 7)].style().bg,
            Some(Theme::default().selection()),
            "the row under the name is the card's bottom padding"
        );
    }

    #[test]
    fn fit_borrows_text_that_already_fits() {
        assert!(matches!(fit("short", 10), Cow::Borrowed("short")));
    }

    #[test]
    fn fit_allocates_only_when_truncating() {
        let fitted = fit("abcdef", 4);
        assert_eq!(fitted, "abc…");
        assert!(matches!(&fitted, Cow::Owned(_)));
    }
    fn render_cards(
        content: &ContentState,
        bordered: bool,
        selected: usize,
        live: LiveInputs,
    ) -> Buffer {
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
        let covers = ContentCovers::default();
        let font_size = ratatui_image::FontSize {
            width: 8,
            height: 16,
        };
        let mut term = Terminal::new(TestBackend::new(60, 10)).unwrap();
        term.draw(|f| {
            draw(
                f,
                CardView {
                    content,
                    bs: &bs,
                    covers: &covers,
                    font_size,
                },
                live,
                selected,
                0,
                Rect::new(0, 0, 60, 10),
            );
        })
        .unwrap();
        term.backend().buffer().clone()
    }

    /// First cell showing `glyph`, as `(x, y)`.
    fn find(buffer: &Buffer, glyph: &str) -> Option<(u16, u16)> {
        let area = buffer.area;
        (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .find(|&(x, y)| buffer[(x, y)].symbol() == glyph)
    }

    #[test]
    fn borderless_selection_and_hover_use_the_selection_blend() {
        let content = ContentState::Songs(vec![song(1), song(2)]);
        let theme = Theme::default();

        let idle = render_cards(&content, false, 0, LiveInputs::default());
        assert_eq!(
            idle[(20, 1)].style().bg,
            Some(theme.selection()),
            "the keyboard-selected card keeps the selection blend"
        );
        assert_eq!(
            idle[(40, 1)].style().bg,
            Some(theme.surface),
            "unselected cards fill the surface colour"
        );

        let hovering = render_cards(
            &content,
            false,
            0,
            LiveInputs {
                mouse: Some((40, 1)),
                ..LiveInputs::default()
            },
        );
        assert_eq!(
            hovering[(40, 1)].style().bg,
            Some(theme.selection()),
            "the card under the pointer also uses the selection blend"
        );
    }

    #[test]
    fn missing_cover_note_sits_on_the_middle_row() {
        let content =
            ContentState::HotSearch(crate::state::HotSearchKeywords(vec!["keyword".into()]));
        let buffer = render_cards(&content, false, 0, LiveInputs::default());
        let (_, y) = find(&buffer, icons::MUSIC_NOTE).expect("placeholder note");
        assert_eq!(
            y, 1,
            "the placeholder should be centred in a 3-row card, not hug the top"
        );
    }

    #[test]
    fn hovered_card_shows_a_filled_play_button() {
        let theme = Theme::default();
        let content = ContentState::Songs(vec![song(1), song(2)]);
        let live = LiveInputs {
            mouse: Some((5, 0)),
            ..LiveInputs::default()
        };
        let buffer = render_cards(&content, false, 0, live);
        let marker =
            find(&buffer, icons::PLAY_INDICATOR).expect("hovered card should show a play button");
        assert_eq!(buffer[marker].bg, theme.bg);
        assert_eq!(
            marker.1, 1,
            "a 3-row card should centre the marker on its middle row"
        );
    }

    #[test]
    fn playing_song_card_shows_the_loading_frame() {
        let theme = Theme::default();
        let content = ContentState::Songs(vec![song(1), song(2)]);
        let live = LiveInputs {
            playing_song: Some(1),
            playing: true,
            ..LiveInputs::default()
        };
        let buffer = render_cards(&content, false, 0, live);
        let frame = icons::LOADING_FRAMES[0];
        let glyphs: Vec<&str> = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .filter(|symbol| frame.contains(*symbol))
            .collect();
        assert_eq!(
            glyphs.concat(),
            frame,
            "only the playing card should show the loading frame"
        );
        for cell in buffer
            .content()
            .iter()
            .filter(|cell| frame.contains(cell.symbol()))
        {
            assert_eq!(cell.fg, theme.accent);
            assert_ne!(cell.bg, theme.accent);
        }
    }

    #[test]
    fn paused_current_song_shows_a_static_play_triangle() {
        let theme = Theme::default();
        let content = ContentState::Songs(vec![song(1), song(2)]);
        let live = LiveInputs {
            playing_song: Some(1),
            playing: false,
            ..LiveInputs::default()
        };
        let buffer = render_cards(&content, false, 0, live);
        let marker = find(&buffer, icons::PLAY_INDICATOR)
            .expect("borderless paused card keeps a play triangle");
        assert_eq!(buffer[marker].fg, theme.accent);
    }

    #[test]
    fn bordered_cards_show_the_triangle_only_while_hovered() {
        let content = ContentState::Songs(vec![song(1), song(2)]);
        let paused = LiveInputs {
            playing_song: Some(1),
            playing: false,
            ..LiveInputs::default()
        };
        let buffer = render_cards(&content, true, 0, paused);
        assert!(
            find(&buffer, icons::PLAY_INDICATOR).is_none(),
            "a bordered card must not show the triangle without the pointer"
        );

        let hovered = LiveInputs {
            mouse: Some((5, 0)),
            ..paused
        };
        let buffer = render_cards(&content, true, 0, hovered);
        let marker = find(&buffer, icons::PLAY_INDICATOR)
            .expect("hovering a bordered card shows the play triangle");
        assert_eq!(
            marker.1, 2,
            "the 5-row bordered card should centre the marker on its middle row"
        );
    }
}
