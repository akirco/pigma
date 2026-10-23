use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
};

use super::{LayoutArea, Playerbar, widgets};
use crate::{
    config::{PlayerbarConfig, Theme},
    playback::PlaybackState,
};

pub(super) struct DefaultLayout;

impl Playerbar for DefaultLayout {
    fn layout(&self, area: Rect, _config: &PlayerbarConfig, _is_sixel: bool) -> LayoutArea {
        // `⏮   ▶   ⏭` — one cell per glyph plus the two three-cell gaps.
        const CONTROLS_WIDTH: u16 = 9;
        const SONG_INFO_MIN: u16 = 20;
        const SPINNER_WIDTH: u16 = 3;
        const RIGHT_WIDTH: u16 = 8;

        // Keep the controls centred on the same middle span as the original
        // `[20, Min, 3, 8]` split, so widening the title never shifts them.
        let middle_start = area.x + SONG_INFO_MIN.min(area.width);
        let right_x = area.x + area.width.saturating_sub(RIGHT_WIDTH);
        let spinner_x = right_x.saturating_sub(SPINNER_WIDTH);
        let middle_width = spinner_x.saturating_sub(middle_start);
        let controls_x = middle_start + middle_width.saturating_sub(CONTROLS_WIDTH) / 2;

        // The middle span was mostly empty on the left of the centred controls;
        // hand that space to the title, stopping one column before the controls.
        let song_info_width = controls_x
            .saturating_sub(area.x + 1)
            .max(SONG_INFO_MIN.min(area.width));

        let song_info = Rect::new(area.x, area.y, song_info_width, 2.min(area.height));
        let controls = Rect::new(controls_x, area.y, CONTROLS_WIDTH, 1);
        let gauge = Rect::new(
            middle_start,
            area.y + area.height.saturating_sub(1),
            middle_width,
            1,
        );
        let spinner = Rect::new(spinner_x, area.y, SPINNER_WIDTH, area.height);

        let right = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .horizontal_margin(1)
        .split(Rect::new(right_x, area.y, RIGHT_WIDTH, area.height));

        LayoutArea {
            song_info,
            controls,
            gauge,
            spinner,
            mode_icon: right[0],
            speed: right[1],
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
        widgets::draw_speed(f, player, colors, layout.speed);

        if player.seeking && config.visible.spinner {
            widgets::draw_spinner(f, _tick, colors, layout.spinner);
        }
    }
}
