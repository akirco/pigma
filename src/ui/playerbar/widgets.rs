use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{LineGauge, Paragraph},
};
use ratatui_image::{Resize, StatefulImage};

use crate::config::Theme;
use crate::state::PlaybackState;
use crate::ui::gradient_line_gauge::GradientLineGauge;
use crate::ui::spinner::Spinner;
use crate::utils::format_duration_into;
use crate::utils::time::format_duration;
use crate::{config::PlayerbarConfig, playback::mode_icon};

pub fn draw_song_info(f: &mut Frame, player: &PlaybackState, colors: &Theme, area: Rect) {
    if let Some(song) = &player.current_song {
        let info_lines = vec![
            Line::from(vec![
                Span::styled("\u{266a} ", Style::default().fg(colors.accent)),
                Span::styled(
                    &song.name,
                    Style::default()
                        .fg(colors.text)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("\u{25C8} ", Style::default().fg(colors.muted)),
                Span::styled(
                    &song.singer,
                    Style::default()
                        .fg(colors.muted)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
        ];
        f.render_widget(Paragraph::new(info_lines), area);
    } else {
        let idle = Line::from(Span::styled("未在播放", Style::default().fg(colors.muted)));
        f.render_widget(Paragraph::new(idle), area);
    }
}

pub fn draw_controls(
    f: &mut Frame,
    player: &PlaybackState,
    colors: &Theme,
    area: Rect,
    is_default: bool,
) {
    let play_icon = if player.paused || !player.playing {
        "\u{f040a}"
    } else {
        "\u{f03e4}"
    };
    let alignment = if is_default {
        Alignment::Center
    } else {
        Alignment::Left
    };
    let controls = Line::from(vec![
        Span::styled("\u{f049}", Style::default().fg(colors.muted)),
        Span::raw("   "),
        Span::styled(
            play_icon,
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("   "),
        Span::styled("\u{f050}", Style::default().fg(colors.muted)),
    ])
    .alignment(alignment);
    f.render_widget(Paragraph::new(controls), area);
}

pub fn draw_mode_icon(f: &mut Frame, player: &PlaybackState, colors: &Theme, area: Rect) {
    let (icon, _) = mode_icon(&player.mode);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            icon,
            Style::default().fg(colors.accent),
        )))
        .alignment(Alignment::Right),
        area,
    );
}

pub fn draw_spinner(f: &mut Frame, tick: u64, colors: &Theme, area: Rect) {
    f.render_widget(
        Spinner::new(tick)
            .active_color(Style::default().fg(colors.accent))
            .inactive_color(Style::default().fg(colors.surface)),
        area,
    );
}

pub fn draw_current_time(f: &mut Frame, player: &PlaybackState, colors: &Theme, area: Rect) {
    if let Some(song) = &player.current_song {
        let cur_ms = (player.progress * song.duration as f64) as u64;
        let mut buf = String::with_capacity(8);
        format_duration_into(cur_ms, &mut buf);
        f.render_widget(
            Paragraph::new(buf)
                .style(Style::default().fg(colors.text))
                .alignment(Alignment::Right),
            area,
        );
    }
}

pub fn draw_total_time(f: &mut Frame, player: &PlaybackState, colors: &Theme, area: Rect) {
    if let Some(song) = &player.current_song {
        let mut buf = String::with_capacity(8);
        format_duration_into(song.duration, &mut buf);
        f.render_widget(
            Paragraph::new(buf)
                .style(Style::default().fg(colors.text))
                .alignment(Alignment::Right),
            area,
        );
    }
}

pub fn draw_gauge_bar(
    f: &mut Frame,
    player: &PlaybackState,
    colors: &Theme,
    pb: &PlayerbarConfig,
    area: Rect,
) {
    if player.current_song.is_none() {
        return;
    }

    let unfilled_color = if player.cached {
        pb.unfilled_color_cached.as_str()
    } else {
        pb.unfilled_color.as_str()
    };

    let ratio = player.progress.clamp(0.0, 1.0);

    if let Some(preset) = pb.gradient_preset {
        let gauge = GradientLineGauge::new(preset)
            .ratio(ratio)
            .label(Line::from(""))
            .filled_symbol(&pb.filled_symbol)
            .unfilled_symbol(&pb.unfilled_symbol)
            .unfilled_style(Style::default().fg(colors.field_color(unfilled_color)));
        f.render_widget(gauge, area);
    } else {
        let gauge = LineGauge::default()
            .filled_symbol(&pb.filled_symbol)
            .unfilled_symbol(&pb.unfilled_symbol)
            .filled_style(Style::default().fg(colors.field_color(&pb.filled_color)))
            .unfilled_style(Style::default().fg(colors.field_color(unfilled_color)))
            .label("")
            .ratio(ratio);

        f.render_widget(gauge, area);
    }
}

pub fn draw_gauge_with_label(
    f: &mut Frame,
    player: &PlaybackState,
    colors: &Theme,
    pb: &PlayerbarConfig,
    area: Rect,
) {
    let time_buf = if let Some(song) = &player.current_song {
        let cur_ms = (player.progress * song.duration as f64) as u64;
        let mut buf = String::with_capacity(16);
        format_duration_into(cur_ms, &mut buf);
        buf.push_str(" / ");
        buf.push_str(&format_duration(song.duration));
        buf
    } else {
        "00:00 / 00:00".into()
    };
    let ratio = if player.current_song.is_some() {
        player.progress.clamp(0.0, 1.0)
    } else {
        0.0
    };

    let unfilled_color = if player.cached {
        pb.unfilled_color_cached.as_str()
    } else {
        pb.unfilled_color.as_str()
    };

    if let Some(preset) = pb.gradient_preset {
        let gauge = GradientLineGauge::new(preset)
            .ratio(ratio)
            .label(Line::from(Span::styled(
                time_buf,
                Style::default().fg(colors.text),
            )))
            .filled_symbol(&pb.filled_symbol)
            .unfilled_symbol(&pb.unfilled_symbol)
            .unfilled_style(Style::default().fg(colors.field_color(unfilled_color)));
        f.render_widget(gauge, area);
    } else {
        let gauge = LineGauge::default()
            .filled_symbol(&pb.filled_symbol)
            .unfilled_symbol(&pb.unfilled_symbol)
            .filled_style(Style::default().fg(colors.field_color(&pb.filled_color)))
            .unfilled_style(Style::default().fg(colors.field_color(unfilled_color)))
            .ratio(ratio)
            .label(Span::styled(time_buf, Style::default().fg(colors.text)));
        f.render_widget(gauge, area);
    }
}

pub fn draw_song_detail(f: &mut Frame, player: &PlaybackState, colors: &Theme, area: Rect) {
    if let Some(song) = &player.current_song {
        let detail = Line::from(vec![
            Span::styled("\u{25C8} ", Style::default().fg(colors.muted)),
            Span::styled(
                &song.singer,
                Style::default()
                    .fg(colors.muted)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .style(Style::default().fg(colors.muted));
        f.render_widget(Paragraph::new(detail), area);
    }
}
pub fn draw_volume(f: &mut Frame, player: &PlaybackState, colors: &Theme, area: Rect) {
    let icon = if player.volume <= 0.30 {
        ""
    } else if player.volume <= 0.60 {
        ""
    } else {
        ""
    };

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            icon,
            Style::default().fg(colors.accent),
        )))
        .alignment(Alignment::Right),
        area,
    );
}

pub fn draw_cover(f: &mut Frame, player: &PlaybackState, colors: &Theme, area: Rect) {
    if player.current_song.is_some() {
        // Try to render real cover image if available
        if let Ok(mut borrow) = player.cover.protocol.lock()
            && let Some(protocol) = borrow.as_mut()
        {
            let image = StatefulImage::new().resize(Resize::Fit(None));
            f.render_stateful_widget(image, area, protocol);
            return;
        }

        // Fallback to placeholder (no border)
        for y in 0..area.height {
            for x in 0..area.width {
                if let Some(cell) = f.buffer_mut().cell_mut((area.x + x, area.y + y)) {
                    cell.set_char('░');
                    cell.set_style(Style::default().fg(colors.surface));
                }
            }
        }

        let icon = "\u{266a}";
        let icon_x = area.x + area.width / 2;
        let icon_y = area.y + area.height / 2;
        if let Some(cell) = f.buffer_mut().cell_mut((icon_x, icon_y)) {
            cell.set_char(icon.chars().next().unwrap_or('♪'));
            cell.set_style(Style::default().fg(colors.accent));
        }
    }
}
