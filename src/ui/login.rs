use qrcode::{QrCode, render::unicode::Dense1x2};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Padding, Paragraph},
};

use super::BlockStyle;
use super::block::CornerBlock;
use crate::config::Theme;
use crate::layout::LoginLayout;
use crate::state::LoginState;

pub fn draw(f: &mut Frame, login: &LoginState, bs: &BlockStyle<'_>, layout: &LoginLayout) {
    let colors = bs.colors;
    render_status(f, colors, layout.status);
    render_box(f, login, colors, bs.border.enabled, layout.login_box);
}

fn render_status(f: &mut Frame, colors: &Theme, area: Rect) {
    let line = Line::from(vec![
        Span::styled(
            "● ",
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
        Span::styled("ONLINE // RTT 36ms", Style::default().fg(colors.muted)),
    ]);
    f.render_widget(Paragraph::new(line).alignment(Alignment::Right), area);
}

fn render_box(f: &mut Frame, login: &LoginState, colors: &Theme, enabled: bool, area: Rect) {
    let box_width = area.width.saturating_sub(10).min(64);
    let box_x = area.x + (area.width.saturating_sub(box_width)) / 2;

    let content_rows: u16 = 30;
    let box_height = (8 + content_rows).min(area.height);
    let box_y = area.y + (area.height.saturating_sub(box_height)) / 2;

    let block = if enabled {
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors.border))
            .title(
                Line::from(vec![
                    Span::styled(" ► ", Style::default().fg(colors.accent)),
                    Span::styled(
                        "AUTHENTICATION REQUIRED",
                        Style::default()
                            .fg(colors.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                ])
                .alignment(Alignment::Left),
            )
            .title_style(Style::default().fg(colors.border))
            .padding(Padding::horizontal(1))
    } else {
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(colors.border))
            .title(
                Line::from(vec![
                    Span::styled(" ► ", Style::default().fg(colors.accent)),
                    Span::styled(
                        "AUTHENTICATION REQUIRED",
                        Style::default()
                            .fg(colors.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                ])
                .alignment(Alignment::Left),
            )
            .title_style(Style::default().fg(colors.border))
    };

    let block = CornerBlock::new(block)
        .corner_color(colors.border)
        .corner_sizes(2, 1);

    let box_area = Rect {
        x: box_x,
        y: box_y,
        width: box_width,
        height: box_height,
    };
    let inner = block.inner(box_area);
    f.render_widget(block, box_area);

    render_inner(f, login, colors, inner);
}

fn render_inner(f: &mut Frame, login: &LoginState, colors: &Theme, area: Rect) {
    let [content_area, err_area, btn_area, footer_area] = Layout::vertical([
        Constraint::Min(14),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    render_qr_content(f, login, colors, content_area);

    if let Some(err) = &login.error {
        let err_line = Line::from(Span::styled(
            format!(" ✗ {}", err),
            Style::default().fg(colors.error),
        ));
        f.render_widget(
            Paragraph::new(err_line).alignment(Alignment::Center),
            err_area,
        );
    }

    if login.loading {
        let loading_line = Line::from(Span::styled(
            " ◌ CREATING QR CODE ...",
            Style::default().fg(colors.muted),
        ));
        f.render_widget(
            Paragraph::new(loading_line).alignment(Alignment::Center),
            btn_area,
        );
    } else {
        render_button(f, colors, btn_area);
    }
    render_footer(f, colors, footer_area);
}

fn render_qr_content(f: &mut Frame, login: &LoginState, colors: &Theme, area: Rect) {
    if login.qr_url.is_empty() {
        let msg = Line::from(Span::styled(
            "  Press ENTER to generate QR code  ",
            Style::default()
                .fg(colors.muted)
                .add_modifier(Modifier::SLOW_BLINK),
        ));
        f.render_widget(Paragraph::new(msg).alignment(Alignment::Center), area);
        return;
    }

    let code = match QrCode::new(login.qr_url.as_bytes()) {
        Ok(code) => code,
        Err(_) => {
            let msg = Line::from(Span::styled(
                "  Failed to generate QR code  ",
                Style::default().fg(colors.error),
            ));
            f.render_widget(Paragraph::new(msg).alignment(Alignment::Center), area);
            return;
        }
    };
    let qr_str = code.render::<Dense1x2>().quiet_zone(false).build();
    let mut lines: Vec<Line> = qr_str
        .lines()
        .map(|l| {
            Line::from(Span::styled(
                l.to_string(),
                Style::default().fg(colors.accent),
            ))
        })
        .collect();

    let hint = if login.qr_status_text.is_empty() {
        "Scan with Netease Cloud Music App"
    } else {
        &login.qr_status_text
    };
    lines.push(Line::from(Span::styled(
        hint,
        Style::default().fg(colors.muted),
    )));

    f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

fn render_button(f: &mut Frame, colors: &Theme, area: Rect) {
    let text = "► GENERATE QR CODE";
    let inner = area.width as usize;
    let pad_left = (inner.saturating_sub(text.len())) / 2;
    let pad_right = inner.saturating_sub(text.len()).saturating_sub(pad_left);

    let line = Line::from(vec![Span::styled(
        format!(
            "{:pad_left$}{}{:pad_right$}",
            "",
            text,
            "",
            pad_left = pad_left,
            pad_right = pad_right
        ),
        Style::default()
            .fg(colors.bg)
            .bg(colors.accent)
            .add_modifier(Modifier::BOLD),
    )]);
    f.render_widget(Paragraph::new(line), area);
}

fn render_footer(f: &mut Frame, colors: &Theme, area: Rect) {
    let line = Line::from(vec![
        Span::styled("ENTER login", Style::default().fg(colors.muted)),
        Span::raw("   "),
        Span::styled("ESC exit", Style::default().fg(colors.muted)),
    ]);
    f.render_widget(Paragraph::new(line).alignment(Alignment::Center), area);
}
