//! Content renderer: per-content-type table or two-column cover cards (cover left,
//! title/subtitle right), with the table as a fallback when rows do not fit.

use std::{borrow::Cow, ops::Range};

use ncm_api::{SingerInfo, SongInfo, SongList, TopList};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, TableState},
};

use super::{
    BlockStyle, cards, scrollbar::calc_scroll_offset, skeleton::Skeleton, styled_text, table,
};
use crate::{
    cache::cover::ContentCovers,
    config::{ColumnDef, ColumnsConfig, ContentConfig, ContentStyle, Theme},
    state::{ContentState, TableMode},
    time::format_duration,
};

const MISSING: &str = "—";

thread_local! {
    static WARNED_FIELDS: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// Warn once per process for an unknown column field. Rendering runs every
/// frame on the main thread, so a per-call `HashSet` would re-log on every
/// frame; the `thread_local` keeps it deduplicated across frames.
fn warn_missing_field(field: &str) {
    WARNED_FIELDS.with(|warned| {
        let mut warned = warned.borrow_mut();
        if !warned.contains(field) {
            log::warn!("Missing field: \"{field}\" — showing \"{MISSING}\"");
            warned.insert(field.to_string());
        }
    });
}

/// Look up a field value for a table row by its column field name.
/// Returns `None` for unknown fields (rendered as "—").
fn song_field<'a>(song: &'a SongInfo, field: &str) -> Option<Cow<'a, str>> {
    match field {
        "name" => Some(Cow::Borrowed(&song.name)),
        "singer" => Some(Cow::Borrowed(&song.singer)),
        "album" => Some(Cow::Borrowed(&song.album)),
        "duration" => Some(Cow::Owned(format_duration(song.duration))),
        "id" => Some(Cow::Owned(song.id.to_string())),
        _ => None,
    }
}

fn songlist_field<'a>(list: &'a SongList, field: &str) -> Option<Cow<'a, str>> {
    match field {
        "name" => Some(Cow::Borrowed(&list.name)),
        "author" => Some(Cow::Borrowed(&list.author)),
        "id" => Some(Cow::Owned(list.id.to_string())),
        _ => None,
    }
}

fn toplist_field<'a>(list: &'a TopList, field: &str) -> Option<Cow<'a, str>> {
    match field {
        "name" => Some(Cow::Borrowed(&list.name)),
        "description" => Some(Cow::Borrowed(&list.description)),
        "id" => Some(Cow::Owned(list.id.to_string())),
        _ => None,
    }
}

fn singer_field<'a>(singer: &'a SingerInfo, field: &str) -> Option<Cow<'a, str>> {
    match field {
        "name" => Some(Cow::Borrowed(&singer.name)),
        "id" => Some(Cow::Owned(singer.id.to_string())),
        _ => None,
    }
}

/// Build table rows directly from a slice of items, borrowing each field into
/// `Cell` instead of materializing a `String` per cell. Borrowed fields (e.g.
/// `&song.name`) stay borrowed; only derived fields (`duration`, `id`) allocate.
fn build_rows<'a, I>(
    items: &'a [I],
    columns: &'a [ColumnDef],
    colors: &'a Theme,
    lookup: impl Fn(&'a I, &str) -> Option<Cow<'a, str>>,
) -> Vec<Row<'a>> {
    items
        .iter()
        .map(|item| {
            Row::new(columns.iter().map(|col| match lookup(item, &col.field) {
                Some(value) => Cell::from(value).style(Style::default().fg(colors.muted)),
                None => {
                    warn_missing_field(&col.field);
                    Cell::from(MISSING).style(Style::default().fg(colors.error))
                }
            }))
            .height(1)
        })
        .collect()
}

fn build_content_rows<'a>(
    content: &'a ContentState,
    columns: &'a [ColumnDef],
    colors: &'a Theme,
    range: Range<usize>,
) -> Vec<Row<'a>> {
    match content {
        ContentState::Songs(songs) => build_rows(&songs[range], columns, colors, |song, field| {
            song_field(song, field)
        }),
        ContentState::SongLists(lists) => {
            build_rows(&lists[range], columns, colors, songlist_field)
        }
        ContentState::TopLists(lists) => build_rows(&lists[range], columns, colors, toplist_field),
        ContentState::HotSearch(keywords) => {
            build_rows(&keywords.0[range], columns, colors, |kw, field| {
                if field == "keyword" {
                    Some(Cow::Borrowed(kw.as_str()))
                } else {
                    None
                }
            })
        }
        ContentState::Singers(singers) => {
            build_rows(&singers[range], columns, colors, singer_field)
        }
        _ => vec![],
    }
}

// The renderer forwards the content view's full input set; a struct would only relocate the noise.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_content(
    f: &mut Frame,
    content: &ContentState,
    columns: &ColumnsConfig,
    api: Option<&str>,
    bs: &BlockStyle<'_>,
    table_state: &mut TableState,
    content_selected: usize,
    table_mode: TableMode,
    content_config: &ContentConfig,
    current_scroll: usize,
    covers: &ContentCovers,
    font_size: ratatui_image::FontSize,
    live: cards::LiveInputs,
    area: Rect,
) -> super::ContentLayout {
    let colors = bs.colors;
    if content_config.style_for(content.content_type()) == ContentStyle::Cards
        && !matches!(
            content,
            ContentState::Empty | ContentState::Loading | ContentState::Error(_)
        )
        && cards::fits(area, content, bs, font_size)
        && let Some(layout) = cards::draw(
            f,
            cards::CardView {
                content,
                bs,
                covers,
                font_size,
            },
            live,
            content_selected,
            current_scroll,
            area,
        )
    {
        return layout;
    }
    // Cards declined this geometry (or this content does not use them): the
    // table is the documented fallback, so the content area is never blank.
    let single_column = || super::ContentLayout {
        columns: 1,
        scroll: 0,
        visible: 0..0,
        hovered: None,
    };
    match content {
        ContentState::Empty => {
            let text = Line::from(Span::styled("", Style::default().fg(colors.muted)));
            f.render_widget(Paragraph::new(text), area);
            single_column()
        }
        ContentState::Loading => {
            f.render_widget(Skeleton::new().bg(colors.bg).surface(colors.surface), area);
            single_column()
        }
        ContentState::Error(e) => {
            let text = Line::from(Span::styled(
                format!("错误: {e}"),
                Style::default().fg(colors.error),
            ));
            f.render_widget(Paragraph::new(text), area);
            single_column()
        }
        _ => {
            let cols = columns.for_content(content.content_type(), api);
            let total = content.len();
            // Defensive only: `clamp_selection()` keeps the stored index in
            // range. The guard protects the row slice below if a future write
            // site forgets to call it.
            let sel = content_selected.min(total.saturating_sub(1));
            // Only materialize the visible window of rows (header takes one row)
            // instead of rebuilding the whole list every frame.
            let visible = area.height.saturating_sub(1).max(1) as usize;
            let offset = calc_scroll_offset(sel, visible, total);
            let end = (offset + visible).min(total);
            let rows = build_content_rows(content, cols, colors, offset..end);
            let header = table::header_row(
                cols.iter()
                    .map(|col| Line::from(styled_text::parse_styled(&col.header, colors))),
                colors,
            );
            let widths = cols.iter().map(|col| col.to_constraint()).collect();
            // The window is pre-scrolled, so selection and offset are relative to
            // it; ratatui recomputes the offset from these during render.
            table_state.select(Some(sel.saturating_sub(offset)));
            *table_state.offset_mut() = 0;
            table::render_table(
                f,
                table::TableView {
                    header,
                    rows,
                    widths,
                    state: Some(table_state),
                    highlight: Some(match table_mode {
                        TableMode::Row => table::Highlight::Row,
                        TableMode::Cell => table::Highlight::Cell,
                    }),
                    total,
                    selected: sel,
                },
                colors,
                area,
            );
            // The selected row keeps the accent highlight; any other row under
            // the pointer gets the dimmer hover fill across the whole table
            // width (the last column belongs to the scrollbar).
            let hovered = live.mouse.and_then(|(x, y)| {
                if !area.contains((x, y).into()) || x >= area.x + area.width.saturating_sub(1) {
                    return None;
                }
                let line = (y - area.y) as usize;
                if line == 0 {
                    return None;
                }
                let index = offset + line - 1;
                (index < end).then_some(index)
            });
            if let Some(index) = hovered
                && index != sel
            {
                let row = area.y + 1 + (index - offset) as u16;
                f.buffer_mut().set_style(
                    Rect::new(area.x, row, area.width.saturating_sub(1), 1),
                    Style::default().fg(colors.text).bg(colors.hover()),
                );
            }
            super::ContentLayout {
                columns: 1,
                scroll: 0,
                visible: offset..end,
                hovered,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ncm_api::{SongCopyright, SongInfo};
    use ratatui::{Terminal, backend::TestBackend, widgets::TableState};

    use super::*;
    use crate::{
        cache::cover::ContentCovers,
        config::{BorderConfig, ColumnsConfig, ContentConfig, Theme},
    };

    fn song(id: u64) -> Arc<SongInfo> {
        Arc::new(SongInfo {
            id,
            name: format!("song-{id}"),
            singer: "Tester".into(),
            artist_id: 0,
            album: "Album".into(),
            album_id: 0,
            pic_url: String::new(),
            duration: 180_000,
            copyright: SongCopyright::Free,
        })
    }

    #[test]
    fn hovering_a_table_row_paints_the_hover_fill() {
        let theme = Theme::default();
        let border = BorderConfig::default();
        let bs = BlockStyle {
            colors: &theme,
            border: &border,
            tick: 0,
        };
        let content = ContentState::Songs(vec![song(1), song(2), song(3)]);
        let columns = ColumnsConfig::default();
        let content_config = ContentConfig::default();
        let covers = ContentCovers::default();
        let font_size = ratatui_image::FontSize {
            width: 8,
            height: 16,
        };
        let mut table_state = TableState::default();
        let mut layout = None;
        let mut term = Terminal::new(TestBackend::new(40, 6)).unwrap();
        let area = Rect::new(0, 0, 40, 6);
        term.draw(|f| {
            layout = Some(render_content(
                f,
                &content,
                &columns,
                None,
                &bs,
                &mut table_state,
                0,
                TableMode::Row,
                &content_config,
                0,
                &covers,
                font_size,
                super::cards::LiveInputs {
                    mouse: Some((2, 2)),
                    ..Default::default()
                },
                area,
            ));
        })
        .unwrap();

        assert_eq!(layout.unwrap().hovered, Some(1));
        let buffer = term.backend().buffer();
        assert_eq!(
            buffer[(1, 1)].style().bg,
            Some(theme.accent),
            "the selected row keeps the accent highlight"
        );
        assert_eq!(
            buffer[(1, 2)].style().bg,
            Some(theme.hover()),
            "the row under the pointer gets the hover fill"
        );
    }
}
