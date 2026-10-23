use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
};

use super::{LayoutArea, Playerbar, widgets};
use crate::{
    config::{PlayerbarConfig, Theme},
    playback::PlaybackState,
};

pub(super) struct ModernLayout;

impl Playerbar for ModernLayout {
    fn layout(&self, area: Rect, config: &PlayerbarConfig, is_sixel: bool) -> LayoutArea {
        let cols = Layout::horizontal([
            if config.visible.cover {
                Constraint::Length(8)
            } else {
                Constraint::Length(0)
            },
            Constraint::Min(20),
        ])
        .spacing(1)
        .split(area);

        let cover_area = cols[0];

        let cover_height = (if is_sixel && area.height >= 5 { 4 } else { 3 }).min(area.height);
        let cover_area = Rect {
            y: area.y,
            height: cover_height,
            ..cover_area
        };

        let right_rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .horizontal_margin(1)
        .split(cols[1]);

        let progress_cols = Layout::horizontal([
            Constraint::Length(6),
            Constraint::Min(10),
            Constraint::Length(6),
        ])
        .split(right_rows[0]);

        // Middle: song_info(left) | spinner(right)
        let middle_cols = Layout::horizontal([Constraint::Min(10), Constraint::Length(8)])
            .flex(Flex::SpaceBetween)
            .split(right_rows[1]);

        // Bottom: song_detail(left) | controls(center) | mode(right)
        let bottom_cols = Layout::horizontal([
            Constraint::Length(15),
            Constraint::Length(20),
            Constraint::Length(11),
        ])
        .flex(Flex::SpaceBetween)
        .split(right_rows[2]);

        // Right end: volume | mode | speed badge, one blank column between each
        // so a full `*1.5x` badge cannot butt against the mode icon. Widths sum
        // to 9 + two 1-column gaps = the 11 columns reserved above.
        let vol_mode_cols = Layout::horizontal([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(5),
        ])
        .spacing(1)
        .split(bottom_cols[2]);

        LayoutArea {
            cover: cover_area,
            progress_time_left: progress_cols[0],
            progress_bar: progress_cols[1],
            progress_time_right: progress_cols[2],
            song_info: middle_cols[0],
            spinner: middle_cols[1],
            song_detail: bottom_cols[0],
            controls: bottom_cols[1],
            volume: vol_mode_cols[0],
            mode_icon: vol_mode_cols[1],
            speed: vol_mode_cols[2],
            ..Default::default()
        }
    }

    fn render(
        &self,
        f: &mut Frame,
        player: &PlaybackState,
        colors: &Theme,
        tick: u64,
        config: &PlayerbarConfig,
        layout: &LayoutArea,
    ) {
        if config.visible.cover && layout.cover.width > 0 {
            widgets::draw_cover(f, player, colors, layout.cover);
        }

        widgets::draw_current_time(f, player, colors, layout.progress_time_left);
        widgets::draw_gauge_bar(f, player, colors, config, layout.progress_bar);
        widgets::draw_total_time(f, player, colors, layout.progress_time_right);

        widgets::draw_song_info(f, player, colors, layout.song_info);
        if player.seeking && config.visible.spinner && layout.spinner.width > 0 {
            widgets::draw_spinner(f, tick, colors, layout.spinner);
        }

        widgets::draw_song_detail(f, player, colors, layout.song_detail);
        widgets::draw_controls(f, player, colors, layout.controls, false);
        if config.visible.volume && layout.volume.width > 0 {
            widgets::draw_volume(f, player, colors, layout.volume);
        }
        if config.visible.mode_icon {
            widgets::draw_mode_icon(f, player, colors, layout.mode_icon);
        }
        widgets::draw_speed(f, player, colors, layout.speed);
    }
}
