use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};

use crate::config::PlayerbarConfig;
use crate::config::Theme;
use crate::state::PlaybackState;

use super::LayoutArea;
use super::Playerbar;
use super::widgets;

pub struct DefaultLayout;

impl Playerbar for DefaultLayout {
    fn layout(&self, area: Rect, _config: &PlayerbarConfig, _is_sixel: bool) -> LayoutArea {
        let cols = Layout::horizontal([
            Constraint::Length(20),
            Constraint::Min(30),
            Constraint::Length(3),
            Constraint::Length(8),
        ])
        .split(area);

        let mid = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(cols[1]);

        let right = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .horizontal_margin(1)
        .split(cols[3]);

        LayoutArea {
            song_info: cols[0],
            controls: mid[0],
            gauge: mid[2],
            spinner: cols[2],
            mode_icon: right[0],
            volume: right[2],
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
        widgets::draw_song_info(f, player, colors, layout.song_info);
        widgets::draw_controls(f, player, colors, layout.controls, true);
        widgets::draw_gauge_with_label(f, player, colors, config, layout.gauge);

        if config.visible.mode_icon {
            widgets::draw_mode_icon(f, player, colors, layout.mode_icon);
        }

        if config.visible.volume && layout.volume.width > 0 {
            widgets::draw_volume(f, player, colors, layout.volume);
        }

        if player.seeking && config.visible.spinner {
            widgets::draw_spinner(f, _tick, colors, layout.spinner);
        }
    }
}
