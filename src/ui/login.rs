use qrcode::{QrCode, render::unicode::Dense1x2};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding, Paragraph, Tabs},
};

use super::block::CornerBlock;
use crate::layout::LoginLayout;
use crate::state::{LoginField, LoginMethod, LoginState};
use crate::theme::Theme;

pub fn draw(
    f: &mut Frame,
    login: &LoginState,
    colors: &Theme,
    bordered: bool,
    layout: &LoginLayout,
) {
    render_status(f, colors, layout.status);
    render_box(f, login, colors, bordered, layout.login_box);
}

fn render_status(f: &mut Frame, colors: &Theme, area: Rect) {
    let line = Line::from(vec![
        Span::styled("● ", Style::default().fg(colors.highlight)),
        Span::styled("ONLINE // RTT 36ms", Style::default().fg(colors.muted)),
    ]);
    f.render_widget(Paragraph::new(line).alignment(Alignment::Right), area);
}

fn render_box(f: &mut Frame, login: &LoginState, colors: &Theme, bordered: bool, area: Rect) {
    let box_width = area.width.saturating_sub(8).min(64);
    let box_x = area.x + (area.width.saturating_sub(box_width)) / 2;

    let content_rows: u16 = match login.selected_method {
        LoginMethod::QR => 30,
        _ => 6,
    };
    let box_height = (8 + content_rows).min(area.height);
    let box_y = area.y + (area.height.saturating_sub(box_height)) / 2;

    let block = if bordered {
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors.muted))
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
            .title_style(Style::default().fg(colors.muted))
            .padding(Padding::horizontal(1))
    } else {
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(colors.surface))
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
            .title_style(Style::default().fg(colors.muted))
    };

    let block = CornerBlock::new(block)
        .corner_color(colors.accent)
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
    let content_h = match login.selected_method {
        LoginMethod::QR => Constraint::Min(14),
        _ => Constraint::Length(6),
    };
    let [
        tabs_area,
        sep_area,
        content_area,
        err_area,
        btn_area,
        footer_area,
    ] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        content_h,
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    let tabs_focused = login.focus == LoginField::Method;

    render_tabs(f, login, colors, tabs_focused, tabs_area);

    let sep = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(colors.muted))
        .border_type(BorderType::Plain);
    f.render_widget(sep, sep_area);

    match login.selected_method {
        LoginMethod::QR => render_qr_content(f, login, colors, content_area),
        LoginMethod::Phone => render_phone_content(f, login, colors, content_area),
        LoginMethod::Email => render_email_content(f, login, colors, content_area),
    }

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
        let loading_text = match login.selected_method {
            LoginMethod::Email => " ◌ AUTHENTICATING ...",
            LoginMethod::Phone => {
                if login.captcha_sent {
                    " ◌ VERIFYING ..."
                } else {
                    " ◌ SENDING CODE ..."
                }
            }
            LoginMethod::QR => " ◌ CREATING QR CODE ...",
        };
        let loading_line = Line::from(Span::styled(
            loading_text,
            Style::default().fg(colors.muted),
        ));
        f.render_widget(
            Paragraph::new(loading_line).alignment(Alignment::Center),
            btn_area,
        );
    } else {
        render_button(f, login, colors, btn_area);
    }
    render_footer(f, colors, footer_area);
}

fn render_tabs(f: &mut Frame, login: &LoginState, colors: &Theme, focused: bool, area: Rect) {
    let titles = vec!["QR CODE", "PHONE", "EMAIL"];

    let highlight_style = if focused {
        Style::default()
            .fg(colors.bg)
            .bg(colors.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(colors.muted)
    };

    let divider_style = Style::default().fg(colors.muted);

    let tabs = Tabs::new(titles)
        .select(login.selected_method.index())
        .highlight_style(highlight_style)
        .divider(Span::styled("│", divider_style))
        .padding(" ", " ");

    f.render_widget(tabs, area);
}

fn build_label_line(
    label: &str,
    hint: &str,
    w: usize,
    _focused: bool,
    colors: &Theme,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(label.to_string(), Style::default().fg(colors.muted)),
        Span::raw(" ".repeat(w.saturating_sub(label.len() + hint.len()))),
        Span::styled(hint.to_string(), Style::default().fg(colors.surface)),
    ])
}

fn render_qr_content(f: &mut Frame, login: &LoginState, colors: &Theme, area: Rect) {
    if login.qr_url.is_empty() {
        let msg = Line::from(Span::styled(
            "  Press ENTER to generate QR code  ",
            Style::default().fg(colors.muted),
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

fn render_phone_content(f: &mut Frame, login: &LoginState, colors: &Theme, area: Rect) {
    let [phone_label, phone_input, code_label, code_input] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(2),
    ])
    .areas(area);

    let phone_focused = login.focus == LoginField::Username;
    let code_focused = login.focus == LoginField::Password;

    // Phone label
    let hint = "[+86]";
    let w = phone_label.width as usize;
    f.render_widget(
        Paragraph::new(build_label_line(
            "PHONE NUMBER",
            hint,
            w,
            phone_focused,
            colors,
        )),
        phone_label,
    );

    // Phone input
    login
        .username
        .render(f, phone_input, colors, phone_focused, false);

    // Code label
    let hint = "[6-DIGIT]";
    let w = code_label.width as usize;
    f.render_widget(
        Paragraph::new(build_label_line(
            "VERIFICATION CODE",
            hint,
            w,
            code_focused,
            colors,
        )),
        code_label,
    );

    // Code input
    login
        .password
        .render(f, code_input, colors, code_focused, false);
}

fn render_email_content(f: &mut Frame, login: &LoginState, colors: &Theme, area: Rect) {
    let [user_label, user_input, pass_label, pass_input] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(2),
    ])
    .areas(area);

    let user_focused = login.focus == LoginField::Username;
    let pass_focused = login.focus == LoginField::Password;

    // Username label
    let hint = "[REQUIRED]";
    let w = user_label.width as usize;
    f.render_widget(
        Paragraph::new(build_label_line(
            "USERNAME / EMAIL",
            hint,
            w,
            user_focused,
            colors,
        )),
        user_label,
    );

    // Username input
    login
        .username
        .render(f, user_input, colors, user_focused, false);

    // Password label
    let hint = "[ENCRYPTED · AES-256]";
    let w = pass_label.width as usize;
    f.render_widget(
        Paragraph::new(build_label_line("PASSWORD", hint, w, pass_focused, colors)),
        pass_label,
    );

    // Password input
    login
        .password
        .render(f, pass_input, colors, pass_focused, true);
}

fn render_button(f: &mut Frame, login: &LoginState, colors: &Theme, area: Rect) {
    let text = match login.selected_method {
        LoginMethod::QR => "► GENERATE QR CODE",
        LoginMethod::Phone => {
            if login.captcha_sent {
                "► VERIFY & LOGIN"
            } else {
                "► SEND VERIFICATION CODE"
            }
        }
        LoginMethod::Email => "► AUTHENTICATE & CONNECT",
    };
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
        Span::styled("← → select", Style::default().fg(colors.muted)),
        Span::raw("   "),
        Span::styled("TAB focus", Style::default().fg(colors.muted)),
        Span::raw("   "),
        Span::styled("ESC exit", Style::default().fg(colors.muted)),
        Span::raw("   "),
        Span::styled("ENTER login", Style::default().fg(colors.muted)),
    ]);
    f.render_widget(Paragraph::new(line).alignment(Alignment::Center), area);
}
