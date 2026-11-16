use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::config::Theme;

/// Draw the current toast. Expiry is decided once per frame by `App::begin_frame`
/// (which clears `toast_time`), so this only has to render.
pub(super) fn draw_toast(f: &mut Frame, msg: &str, colors: &Theme) {
    let area = f.area();
    let display_w = unicode_width::UnicodeWidthStr::width(msg);
    let w = (display_w as u16 + 6).min(area.width);
    let h = 3u16;
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + area.height.saturating_sub(10);

    let toast_area = Rect {
        x,
        y,
        width: w,
        height: h,
    };

    f.render_widget(Clear, toast_area);

    let block = Block::default()
        .borders(Borders::TOP)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colors.muted))
        .style(Style::default().bg(colors.surface));

    let p = Paragraph::new(format!(" {msg} "))
        .style(Style::default().fg(colors.text))
        .block(block)
        .alignment(Alignment::Center);
    f.render_widget(p, toast_area);
}
