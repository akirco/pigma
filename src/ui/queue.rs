use std::borrow::Cow;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::{
    BlockStyle, block::StyledBlock, navigation::keep_visible, scrollbar::calc_scroll_offset, table,
    title::render_title,
};
use crate::{
    config::Theme, playback::PlaybackEngine, state::QueueTabHit, time::format_duration_into,
};

fn tab_style(colors: &Theme, selected: bool, playing: bool) -> Style {
    if !selected && !playing {
        return Style::default().fg(colors.muted);
    }
    let mut modifier = Modifier::BOLD;
    if playing {
        modifier |= Modifier::SLOW_BLINK;
    }
    Style::default()
        .fg(colors.bg)
        .bg(colors.accent)
        .add_modifier(Modifier::BOLD)
}

/// Geometry of the last queue frame: the tab scroll offset and the table body
/// with the first visible song, so mouse clicks can be mapped to rows and tabs.
pub(super) struct QueueLayout {
    pub tab_scroll: u16,
    /// Tab bar row, zero-sized when the queue has a single tab.
    pub tab_area: Rect,
    /// Clickable tabs: line columns and the tab index they select.
    pub tab_hits: Vec<QueueTabHit>,
    pub body: Rect,
    pub offset: usize,
}

pub(super) fn draw_queue_table(
    f: &mut Frame,
    playback: &PlaybackEngine,
    selected: usize,
    bs: &BlockStyle<'_>,
    title_template: &str,
    tab_scroll: u16,
    area: Rect,
) -> QueueLayout {
    let mut tab_scroll = tab_scroll;
    let colors = bs.colors;
    let count = playback.queue_len();
    let key = playback.queue_key();
    let title = if key.is_empty() {
        render_title(title_template, "", count, 0)
    } else {
        render_title(title_template, key, count, 0)
    };
    let block = StyledBlock::from_color(bs, bs.colors.bg).title(&title, bs.colors);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut tab_area = Rect::default();
    let mut tab_hits = Vec::with_capacity(playback.queue_keys().len());
    let body = if playback.queue_keys().is_empty() {
        inner
    } else {
        let keys = playback.queue_keys();
        let selected_tab = keys.iter().position(|k| k == key);
        let playing_tab = keys.iter().position(|k| k == playback.playing_queue_key());
        let [tabs_area, body] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
        tab_area = tabs_area;

        let mut line = Line::default();
        let mut total = 0usize;
        let mut selected_start = None;
        let mut selected_width = 0;

        for (i, k) in keys.iter().enumerate() {
            if i > 0 {
                line.push_span(Span::styled("  ", Style::default()));
                total += 2;
            }
            let start = total;
            let label = clip_long_text(k, 24);
            let label_w = UnicodeWidthStr::width(label.as_ref());
            // One tab is " name " plus the trailing spacing cell.
            tab_hits.push(QueueTabHit {
                columns: start..start + label_w + 3,
                index: i,
            });
            let is_selected = Some(i) == selected_tab;
            let is_playing = Some(i) == playing_tab;
            if is_selected {
                selected_start = Some(total + 1);
                selected_width = label_w + 2;
            }
            total += 1;
            let style = tab_style(colors, is_selected, is_playing);
            line.push_span(Span::styled(" ", style));
            line.push_span(Span::styled(label, style));
            line.push_span(Span::styled(" ", style));
            total += label_w + 1;
            line.push_span(Span::styled(" ", Style::default()));
            total += 1;
        }

        let viewport = tabs_area.width as usize;
        if let Some(start) = selected_start {
            let max_scroll = total.saturating_sub(viewport);
            tab_scroll = keep_visible(
                tab_scroll as usize,
                start,
                selected_width,
                viewport,
                max_scroll,
            ) as u16;
        } else {
            tab_scroll = 0;
        }

        f.render_widget(Paragraph::new(line).scroll((0, tab_scroll)), tabs_area);
        body
    };
    if playback.queue_len() == 0 {
        let empty = Paragraph::new("播放列表为空")
            .style(Style::default().fg(colors.muted))
            .alignment(Alignment::Center);
        f.render_widget(empty, body);
        return QueueLayout {
            tab_scroll,
            tab_area,
            tab_hits,
            body,
            offset: 0,
        };
    }

    let queue_len = playback.queue_len();
    let visible = body.height.saturating_sub(1) as usize;
    let sel = selected.min(queue_len.saturating_sub(1));
    let offset = calc_scroll_offset(sel, visible, queue_len);

    let header = table::header_row(["#", "TITLE", "ARTIST", "DURATION"].map(Line::from), colors);

    let current_idx = playback.queue_current_index();

    let rows: Vec<Row> = playback
        .queue_songs()
        .iter()
        .enumerate()
        .skip(offset)
        .take(visible)
        .map(|(i, song)| {
            let is_playing = Some(i) == current_idx;
            let is_selected = i == sel;

            let prefix = if is_playing {
                crate::icons::PLAY_INDICATOR
            } else {
                " "
            };
            let num = format!("{}{:02}", prefix, i + 1);

            let row_style = if is_playing {
                Style::default()
                    .fg(colors.surface)
                    .bg(colors.accent)
                    .add_modifier(Modifier::SLOW_BLINK)
            } else if is_selected {
                Style::default()
                    .fg(colors.bg)
                    .bg(colors.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let mut dur_buf = String::with_capacity(8);
            format_duration_into(song.duration, &mut dur_buf);

            Row::new(vec![
                Cell::from(num).style(Style::default().fg(colors.muted)),
                Cell::from(song.name.as_str()).style(Style::default().fg(colors.muted)),
                Cell::from(song.singer.as_str()).style(Style::default().fg(colors.muted)),
                Cell::from(dur_buf).style(Style::default().fg(colors.muted)),
            ])
            .height(1)
            .style(row_style)
        })
        .collect();

    let widths = vec![
        Constraint::Length(5),
        Constraint::Min(20),
        Constraint::Length(16),
        Constraint::Length(9),
    ];

    table::render_table(
        f,
        table::TableView {
            header,
            rows,
            widths,
            state: None,
            highlight: None,
            total: queue_len,
            selected: sel,
        },
        colors,
        body,
    );

    QueueLayout {
        tab_scroll,
        tab_area,
        tab_hits,
        body,
        offset,
    }
}

/// Cap a tab label to `max_cells` display cells, appending an ellipsis when
/// truncated, so a single long playlist name can't push the others off screen.
fn clip_long_text(s: &str, max_cells: usize) -> Cow<'_, str> {
    if UnicodeWidthStr::width(s) <= max_cells {
        return Cow::Borrowed(s);
    }
    let mut out = String::new();
    let mut cells = 0;
    for ch in s.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        if cells + cw > max_cells.saturating_sub(1) {
            break;
        }
        out.push(ch);
        cells += cw;
    }
    out.push('…');
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use unicode_width::UnicodeWidthStr;

    use super::clip_long_text;

    #[test]
    fn clip_keeps_short_labels() {
        assert_eq!(clip_long_text("我喜欢的音乐", 24), "我喜欢的音乐");
        assert_eq!(clip_long_text("short", 24), "short");
    }

    #[test]
    fn clip_truncates_wide_labels() {
        let clipped = clip_long_text("一个特别特别特别长的歌单名字", 8);
        assert!(clipped.ends_with('…'));
        assert!(UnicodeWidthStr::width(clipped.as_ref()) <= 8);
    }
}
