use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table};

use unicode_width::UnicodeWidthStr;

use super::BlockStyle;
use super::create_block;
use super::navigation::keep_visible;

use crate::config::Theme;
use crate::playback::PlaybackEngine;
use crate::ui::scrollbar::{calc_scroll_offset, render_scrollbar};
use crate::ui::title::render_title;
use crate::utils::format::clip_long_text;
use crate::utils::format_duration_into;

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

pub fn draw_queue_table(
    f: &mut Frame,
    playback: &PlaybackEngine,
    selected: usize,
    bs: &BlockStyle<'_>,
    title_template: &str,
    tab_scroll: &mut u16,
    area: Rect,
) {
    let colors = bs.colors;
    let count = playback.queue_len();
    let key = playback.queue_key();
    let title = if key.is_empty() {
        render_title(title_template, "", count, 0)
    } else {
        render_title(title_template, key, count, 0)
    };
    let block = create_block(&title, bs, false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let body = if playback.queue_keys().is_empty() {
        inner
    } else {
        let keys = playback.queue_keys().to_vec();
        let selected_tab = keys.iter().position(|k| k == key);
        let playing_tab = keys.iter().position(|k| k == playback.playing_queue_key());
        let [tabs_area, body] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);

        let mut line = Line::default();
        let mut total = 0usize;
        let mut selected_start = None;
        let mut selected_width = 0;

        for (i, k) in keys.iter().enumerate() {
            if i > 0 {
                line.push_span(Span::styled("  ", Style::default()));
                total += 2;
            }
            let label = clip_long_text(k, 24);
            let label_w = UnicodeWidthStr::width(label.as_str());
            let is_selected = Some(i) == selected_tab;
            let is_playing = Some(i) == playing_tab;
            if is_selected {
                selected_start = Some(total + 1);
                selected_width = label_w + 2;
            }
            total += 1;
            line.push_span(Span::styled(
                format!(" {} ", label),
                tab_style(colors, is_selected, is_playing),
            ));
            total += label_w + 1;
            line.push_span(Span::styled(" ", Style::default()));
            total += 1;
        }

        let viewport = tabs_area.width as usize;
        if let Some(start) = selected_start {
            let max_scroll = total.saturating_sub(viewport);
            *tab_scroll = keep_visible(
                *tab_scroll as usize,
                start,
                selected_width,
                viewport,
                max_scroll,
            ) as u16;
        } else {
            *tab_scroll = 0;
        }

        f.render_widget(Paragraph::new(line).scroll((0, *tab_scroll)), tabs_area);
        body
    };

    if playback.queue_len() == 0 {
        let empty = Paragraph::new("播放列表为空")
            .style(Style::default().fg(colors.muted))
            .alignment(Alignment::Center);
        f.render_widget(empty, body);
        return;
    }

    let [table_area, scrollbar_area] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(1)]).areas(body);

    let queue_len = playback.queue_len();
    let visible = table_area.height.saturating_sub(1) as usize;
    let sel = selected.min(queue_len.saturating_sub(1));
    let offset = calc_scroll_offset(sel, visible, queue_len);

    let header = Row::new(vec![
        Cell::from("#").style(Style::default().fg(colors.muted)),
        Cell::from("TITLE").style(Style::default().fg(colors.muted)),
        Cell::from("ARTIST").style(Style::default().fg(colors.muted)),
        Cell::from("DURATION").style(Style::default().fg(colors.muted)),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD))
    .height(1);

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

            let prefix = if is_playing { "\u{25b6}" } else { " " };
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

    let table = Table::new(
        rows,
        vec![
            Constraint::Length(5),
            Constraint::Min(20),
            Constraint::Length(16),
            Constraint::Length(9),
        ],
    )
    .header(header)
    .column_spacing(2);

    f.render_widget(table, table_area);

    render_scrollbar(f, queue_len, sel, scrollbar_area);
}
