use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Cell, Paragraph, Row, Table};

use super::create_block;
use crate::playback::PlaybackEngine;
use crate::theme::Theme;
use crate::ui::{calc_scroll_offset, render_scrollbar};
use crate::utils::format_duration;

pub fn draw_queue_table(
    f: &mut Frame,
    playback: &PlaybackEngine,
    selected: usize,
    colors: &Theme,
    bordered: bool,
    border_rounded: bool,
    title_template: &str,
    area: Rect,
) {
    let count = playback.queue.songs.len();
    let title = format!(" {} ", crate::ui::render_title(title_template, "", count));
    let block = create_block(&title, colors, bordered, border_rounded, false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    if playback.queue.is_empty() {
        let empty = Paragraph::new("\u{64ad}\u{653e}\u{5217}\u{8868}\u{4e3a}\u{7a7a}")
            .style(Style::default().fg(colors.muted))
            .alignment(Alignment::Center);
        f.render_widget(empty, inner);
        return;
    }

    let [table_area, scrollbar_area] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(1)]).areas(inner);

    let queue_len = playback.queue.len();
    let visible = table_area.height.saturating_sub(1) as usize;
    let sel = selected.min(queue_len.saturating_sub(1));
    let offset = calc_scroll_offset(sel, visible, queue_len);

    let header = Row::new(vec![
        Cell::from("#").style(Style::default().fg(colors.muted)),
        Cell::from("TITLE").style(Style::default().fg(colors.muted)),
        Cell::from("ARTIST").style(Style::default().fg(colors.muted)),
        Cell::from("DURATION").style(Style::default().fg(colors.muted)),
    ])
    .style(Style::default().add_modifier(Modifier::UNDERLINED))
    .height(1);

    let current_idx = playback.queue.current_index;

    let rows: Vec<Row> = playback
        .queue
        .songs
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
                    .fg(colors.text)
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

            let dur = format_duration(song.duration);

            Row::new(vec![
                Cell::from(num).style(Style::default().fg(colors.muted)),
                Cell::from(song.name.as_str()).style(Style::default().fg(colors.muted)),
                Cell::from(song.singer.as_str()).style(Style::default().fg(colors.muted)),
                Cell::from(dur).style(Style::default().fg(colors.muted)),
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
