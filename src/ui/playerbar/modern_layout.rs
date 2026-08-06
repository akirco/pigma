use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Padding;

use crate::config::PlayerbarConfig;
use crate::state::PlaybackState;

use super::super::{BlockStyle, create_block};
use super::build_layout;
use super::widgets;

pub fn draw(
    f: &mut Frame,
    player: &PlaybackState,
    tick: u64,
    bs: &BlockStyle<'_>,
    config: &PlayerbarConfig,
    area: Rect,
    is_sixel: bool,
) {
    let colors = bs.colors;
    let block = create_block("", bs, false).block_padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block.block_padding(Padding::horizontal(1)), area);

    if let Some(err) = &player.error {
        use ratatui::widgets::Paragraph;
        let text = format!(" \u{26a0}  {}", err);
        f.render_widget(
            Paragraph::new(text).style(Style::default().fg(colors.error)),
            inner,
        );
        return;
    }

    let layout =
        build_layout::build_modern(inner, config.visible.cover, config.visible.volume, is_sixel);

    // Left: cover
    if config.visible.cover && layout.cover.width > 0 {
        widgets::draw_cover(f, player, colors, layout.cover);
    }

    // Right top: current time + progress bar + total time
    widgets::draw_current_time(f, player, colors, layout.progress_time_left);
    widgets::draw_gauge_bar(f, player, colors, config, layout.progress_bar);
    widgets::draw_total_time(f, player, colors, layout.progress_time_right);

    // Right middle: song name + spinner
    widgets::draw_song_info(f, player, colors, layout.song_info);
    if player.seeking && config.visible.spinner && layout.spinner.width > 0 {
        widgets::draw_spinner(f, tick, colors, layout.spinner);
    }

    // Right bottom: singer + album + controls + volume + mode
    widgets::draw_song_detail(f, player, colors, layout.song_detail);
    widgets::draw_controls(f, player, colors, layout.controls);
    if config.visible.volume && layout.volume.width > 0 {
        widgets::draw_volume(f, player, colors, layout.volume);
    }
    if config.visible.mode_icon {
        widgets::draw_mode_icon(f, player, colors, layout.mode_icon);
    }
}
