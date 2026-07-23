use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{LineGauge, Padding, Paragraph},
};

use super::gradient_line_gauge::GradientLineGauge;
use super::spinner::Spinner;

use super::BlockStyle;
use super::create_block;
use crate::config::{PlayerbarConfig, Theme};
use crate::playback::types::PlayMode;
use crate::state::PlaybackState;

fn fmt_secs_into(total_secs: f64, out: &mut String) {
    let m = (total_secs as u64) / 60;
    let s = (total_secs as u64) % 60;
    use std::fmt::Write;
    let _ = write!(out, "{}:{:02}", m, s);
}

fn mode_icon(mode: &PlayMode) -> (&str, &str) {
    match mode {
        PlayMode::Sequential => ("\u{F049E}", "顺序"),
        PlayMode::RepeatOne => ("\u{F0458}", "单曲"),
        PlayMode::RepeatAll => ("\u{F0577}", "列表"),
        PlayMode::Shuffle => ("\u{F049F}", "随机"),
        PlayMode::Heartbeat { .. } => ("\u{F0430}", "心动"),
    }
}

pub fn draw(
    f: &mut Frame,
    player: &PlaybackState,
    tick: u64,
    bs: &BlockStyle<'_>,
    pb_config: &PlayerbarConfig,
    area: Rect,
) {
    let colors = bs.colors;
    let block = create_block("", bs, false).block_padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if let Some(err) = &player.error {
        let text = format!(" \u{26a0}  {}", err);
        f.render_widget(
            Paragraph::new(text).style(Style::default().fg(colors.error)),
            inner,
        );
        return;
    }

    let cols = Layout::horizontal([
        Constraint::Length(32),
        Constraint::Min(25),
        Constraint::Length(12),
        Constraint::Length(3),
    ])
    .split(inner);

    draw_left(f, player, colors, cols[0]);

    let mid = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(cols[1]);
    draw_controls(f, player, colors, mid[1]);
    draw_gauge(f, player, colors, pb_config, mid[2]);

    if player.seeking {
        f.render_widget(
            Spinner::new(tick)
                .active_color(Style::default().fg(colors.accent))
                .inactive_color(Style::default().fg(colors.surface)),
            cols[2],
        );
    }

    let (icon, _) = mode_icon(&player.mode);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            icon,
            Style::default().fg(colors.accent),
        )))
        .alignment(Alignment::Center),
        cols[3],
    );
}

fn draw_left(f: &mut Frame, player: &PlaybackState, colors: &Theme, area: Rect) {
    if let Some(song) = &player.current_song {
        let info_lines = vec![
            Line::from(vec![
                Span::styled(" \u{266a} ", Style::default().fg(colors.accent)),
                Span::styled(
                    &song.name,
                    Style::default()
                        .fg(colors.text)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(format!("   {} ◈  {}", song.singer, song.album))
                .style(Style::default().fg(colors.muted)),
        ];
        f.render_widget(Paragraph::new(info_lines), area);
    } else {
        let idle = Line::from(Span::styled("未在播放", Style::default().fg(colors.muted)));
        f.render_widget(Paragraph::new(idle), area);
    }
}

fn draw_controls(f: &mut Frame, player: &PlaybackState, colors: &Theme, area: Rect) {
    let play_icon = if player.paused || !player.playing {
        "\u{25b6}"
    } else {
        "\u{23f8}"
    };
    let controls = Line::from(vec![
        Span::raw("       "),
        Span::styled("\u{23ee}", Style::default().fg(colors.muted)),
        Span::raw("   "),
        Span::styled(
            play_icon,
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("   "),
        Span::styled("\u{23ed}", Style::default().fg(colors.muted)),
        Span::raw("       "),
    ])
    .alignment(Alignment::Center);
    f.render_widget(Paragraph::new(controls), area);
}

fn draw_gauge(
    f: &mut Frame,
    player: &PlaybackState,
    colors: &Theme,
    pb: &PlayerbarConfig,
    area: Rect,
) {
    let Some(song) = &player.current_song else {
        return;
    };

    let dur_secs = song.duration as f64 / 1000.0;
    let cur_secs = player.progress * dur_secs;
    let mut time_buf = String::with_capacity(16);
    fmt_secs_into(cur_secs, &mut time_buf);
    time_buf.push_str(" / ");
    fmt_secs_into(dur_secs, &mut time_buf);
    let time_str = time_buf;

    if pb.gradient_enabled {
        let unfilled_color = if player.cached {
            pb.unfilled_color_cached.as_str()
        } else {
            pb.unfilled_color.as_str()
        };
        let gauge = GradientLineGauge::new(&pb.gradient_preset)
            .ratio(player.progress.clamp(0.0, 1.0))
            .label(Line::from(Span::styled(
                time_str,
                Style::default().fg(colors.text),
            )))
            .filled_symbol(&pb.filled_symbol)
            .unfilled_symbol(&pb.unfilled_symbol)
            .unfilled_style(Style::default().fg(colors.field_color(unfilled_color)));
        f.render_widget(gauge, area);
    } else {
        let unfilled_color = if player.cached {
            pb.unfilled_color_cached.as_str()
        } else {
            pb.unfilled_color.as_str()
        };
        let gauge = LineGauge::default()
            .filled_symbol(&pb.filled_symbol)
            .unfilled_symbol(&pb.unfilled_symbol)
            .filled_style(Style::default().fg(colors.field_color(&pb.filled_color)))
            .unfilled_style(Style::default().fg(colors.field_color(unfilled_color)))
            .ratio(player.progress.clamp(0.0, 1.0))
            .label(Span::styled(time_str, Style::default().fg(colors.text)));
        f.render_widget(gauge, area);
    }
}
