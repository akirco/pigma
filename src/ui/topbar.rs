use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::BlockStyle;
use super::create_block;
use crate::config::Theme;
use crate::state::SearchState;
use ncm_api::LoginInfo;
use unicode_width::UnicodeWidthStr;

pub fn draw(
    f: &mut Frame,
    user: Option<&LoginInfo>,
    search: &SearchState,
    bs: &BlockStyle<'_>,
    area: Rect,
) {
    let colors = bs.colors;
    let block = create_block("", bs, false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    if search.active {
        render_search(f, search, colors, inner);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(38),
            Constraint::Percentage(22),
        ])
        .split(inner);

    let logo = Line::from(vec![
        Span::styled("▓ ", Style::default().fg(colors.accent)),
        Span::styled(
            "PIGMA",
            Style::default()
                .fg(colors.text)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    f.render_widget(Paragraph::new(logo), chunks[0]);

    let mut right_line = Line::default();

    if let Some(info) = user {
        right_line.push_span(Span::styled(
            &info.nickname,
            Style::default().fg(colors.text),
        ));
        right_line.push_span(Span::styled("  ", Style::default()));
        match info.vip_type {
            10 | 11 => {
                right_line.push_span(Span::styled(
                    "♛VIP",
                    Style::default()
                        .fg(colors.accent)
                        .add_modifier(Modifier::BOLD),
                ));
                right_line.push_span(Span::styled("  ", Style::default()));
            }
            _ => {}
        }
    }

    // right_line.push(Span::styled("v0.1.0", Style::default().fg(colors.muted)));

    let right_line = right_line.alignment(Alignment::Right);
    f.render_widget(Paragraph::new(right_line), chunks[2]);
}

fn render_search(f: &mut Frame, search: &SearchState, colors: &Theme, area: Rect) {
    let provider_width = {
        let name = search.provider.display_name();
        name.width() + 2
    };
    let chunks = if search.filter_queue_only {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(2), Constraint::Min(1)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(2),
                Constraint::Min(1),
                Constraint::Length(provider_width as u16),
            ])
            .split(area)
    };

    let icon = Line::from(Span::styled("\u{F002}", Style::default().fg(colors.accent)));
    f.render_widget(Paragraph::new(icon), chunks[0]);

    let value = &search.input.value;

    let placeholder = if search.filter_queue_only {
        " 过滤播放队列..."
    } else {
        " 搜索歌曲..."
    };

    let display = if value.is_empty() {
        Line::from(Span::styled(placeholder, Style::default().fg(colors.muted)))
    } else {
        Line::from(Span::styled(
            value.as_str(),
            Style::default().fg(colors.text),
        ))
    };

    f.render_widget(Paragraph::new(display), chunks[1]);
    search
        .input
        .show_cursor_at(f, chunks[1].x, chunks[1].y, search.active, false);

    if !search.filter_queue_only {
        let provider = Line::from(vec![
            Span::styled(" ", Style::default().fg(colors.text)),
            Span::styled(
                search.provider.display_name(),
                Style::default()
                    .fg(colors.accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .alignment(Alignment::Right);
        f.render_widget(Paragraph::new(provider), chunks[2]);
    }
}
