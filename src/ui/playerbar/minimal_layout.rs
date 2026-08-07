use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::config::PlayerbarConfig;
use crate::config::Theme;
use crate::state::PlaybackState;

use super::LayoutArea;
use super::Playerbar;
use super::widgets;

pub struct MinimalLayout;

impl Playerbar for MinimalLayout {
    fn layout(&self, area: Rect, _config: &PlayerbarConfig, _is_sixel: bool) -> LayoutArea {
        let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(area);

        let cols = Layout::horizontal([
            Constraint::Percentage(12),
            Constraint::Length(18),
            Constraint::Length(6),
            Constraint::Min(0),
            Constraint::Length(6),
            Constraint::Length(3),
        ])
        .spacing(2)
        .split(rows[1]);

        LayoutArea {
            song_info: cols[0],
            controls: cols[1],
            progress_time_left: cols[2],
            gauge: cols[3],
            progress_time_right: cols[4],
            mode_icon: cols[5],
            ..Default::default()
        }
    }

    fn render(
        &self,
        f: &mut Frame,
        player: &PlaybackState,
        colors: &Theme,
        _tick: u64,
        config: &PlayerbarConfig,
        layout: &LayoutArea,
    ) {
        draw_song_info_inline(f, player, colors, layout.song_info);
        widgets::draw_controls(f, player, colors, layout.controls, true);
        widgets::draw_gauge_bar(f, player, colors, config, layout.gauge);
        widgets::draw_current_time(f, player, colors, layout.progress_time_left);
        widgets::draw_total_time(f, player, colors, layout.progress_time_right);

        if config.visible.mode_icon {
            widgets::draw_mode_icon(f, player, colors, layout.mode_icon);
        }
    }
}

fn draw_song_info_inline(f: &mut Frame, player: &PlaybackState, colors: &Theme, area: Rect) {
    if let Some(song) = &player.current_song {
        let info = Line::from(vec![
            Span::styled(" ♪ ", Style::default().fg(colors.accent)),
            Span::styled(
                &song.name,
                Style::default()
                    .fg(colors.text)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
            Span::styled(" - ", Style::default().fg(colors.muted)),
            Span::styled(&song.singer, Style::default().fg(colors.muted)),
        ]);
        f.render_widget(Paragraph::new(info), area);
    } else {
        let idle = Line::from(Span::styled("未在播放", Style::default().fg(colors.muted)));
        f.render_widget(Paragraph::new(idle), area);
    }
}
