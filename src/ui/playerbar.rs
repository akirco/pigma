mod build_layout;
mod default_layout;
mod minimal_layout;
mod modern_layout;
mod widgets;

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::config::LayoutType;
use crate::config::PlayerbarConfig;
use crate::state::PlaybackState;

use super::BlockStyle;

pub fn draw(
    f: &mut Frame,
    player: &PlaybackState,
    tick: u64,
    bs: &BlockStyle<'_>,
    config: &PlayerbarConfig,
    area: Rect,
    is_sixel: bool,
) {
    match config.layout {
        LayoutType::Default => default_layout::draw(f, player, tick, bs, config, area),
        LayoutType::Modern => modern_layout::draw(f, player, tick, bs, config, area, is_sixel),
        LayoutType::Minimal => minimal_layout::draw(f, player, tick, bs, config, area),
    }
}
