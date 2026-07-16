use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::layout::SplashLayout;
use crate::state::{LogLevel, SplashState};
use crate::theme::Theme;

const LOGO: &[&str] = &[
    "█▀▀▀▄ ▀█▀ ▄▀▀▀▀ █▄ ▄█ ▄▀▀▀▄",
    "█▄▄▄▀  █  █ ▀▀█ █ ▀ █ █▄▄▄█",
    "█     ▄█▄ ▀▄▄▄▀ █   █ █   █",
];

pub fn draw(f: &mut Frame, splash: &SplashState, colors: &Theme, layout: &SplashLayout) {
    render_logo(f, colors, layout.logo);
    render_progress(f, splash, colors, layout.progress);
    render_logs(f, splash, colors, layout.logs);
    render_tag(f, colors, layout.tag);
}

fn render_logo(f: &mut Frame, colors: &Theme, area: Rect) {
    let [row0, row1, row2] = Layout::vertical([Constraint::Length(1); 3]).areas(area);
    let rows = [row0, row1, row2];
    for (i, line) in LOGO.iter().enumerate() {
        let spans = vec![Span::styled(
            line.to_string(),
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        )];
        f.render_widget(
            Paragraph::new(Line::from(spans)).alignment(Alignment::Center),
            rows[i],
        );
    }
}

fn render_progress(f: &mut Frame, splash: &SplashState, colors: &Theme, area: Rect) {
    let [bar_area, status_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);

    let bar_width = bar_area.width.saturating_sub(8).min(60) as usize;
    let filled = (bar_width as f64 * splash.progress) as usize;
    let empty = bar_width - filled;

    let mut spans: Vec<Span> = Vec::new();
    for _ in 0..filled {
        spans.push(Span::styled("▍", Style::default().fg(colors.accent)));
    }
    for _ in 0..empty {
        spans.push(Span::styled("▍", Style::default().fg(colors.surface)));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Center),
        bar_area,
    );

    let percent = (splash.progress * 100.0) as u32;
    let status_line = Line::from(vec![Span::styled(
        format!("{}  {}%", splash.status, percent),
        Style::default().fg(colors.muted),
    )]);
    f.render_widget(
        Paragraph::new(status_line).alignment(Alignment::Center),
        status_area,
    );
}

fn render_logs(f: &mut Frame, splash: &SplashState, colors: &Theme, area: Rect) {
    let max_logs = area.height as usize;
    let start = splash.logs.len().saturating_sub(max_logs);
    let visible: Vec<_> = splash.logs[start..].to_vec();

    if visible.is_empty() {
        return;
    }

    let tag_width = 7;
    let time_width = 10;
    let mut max_text_len = 0usize;
    for entry in &visible {
        max_text_len = max_text_len.max(entry.text.len());
    }
    let max_line_width = (time_width + tag_width + 1 + max_text_len) as u16;

    let [_, center, _] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(max_line_width),
        Constraint::Fill(1),
    ])
    .areas(area);

    for (y, entry) in (center.y..).zip(visible.iter()) {
        let row_area = Rect {
            x: center.x,
            y,
            width: center.width,
            height: 1,
        };

        let tag = match entry.level {
            LogLevel::Success => "[ OK ]",
            LogLevel::Info => "[INFO]",
            LogLevel::Warning => "[WARN]",
        };
        let tag_color = match entry.level {
            LogLevel::Success => colors.highlight,
            LogLevel::Info => colors.warning,
            LogLevel::Warning => colors.error,
        };

        let line = Line::from(vec![
            Span::styled(
                format!("[{}] ", entry.time),
                Style::default().fg(colors.muted),
            ),
            Span::styled(tag.to_string(), Style::default().fg(tag_color)),
            Span::raw(" "),
            Span::styled(entry.text.clone(), Style::default().fg(colors.muted)),
        ]);
        f.render_widget(Paragraph::new(line), row_area);
    }
}

fn render_tag(f: &mut Frame, colors: &Theme, area: Rect) {
    let line = Line::from(vec![
        Span::styled("NETEASE MUSIC TUI ", Style::default().fg(colors.muted)),
        Span::styled(
            "v0.1.0",
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  //  [ PRESS ANY KEY ]", Style::default().fg(colors.muted)),
    ]);
    f.render_widget(Paragraph::new(line).alignment(Alignment::Center), area);
}
