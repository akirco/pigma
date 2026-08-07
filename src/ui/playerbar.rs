mod default_layout;
mod minimal_layout;
mod modern_layout;
pub mod widgets;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Padding, Paragraph};

use crate::config::LayoutType;
use crate::config::PlayerbarConfig;
use crate::config::Theme;
use crate::state::PlaybackState;

use super::BlockStyle;
use super::create_block;

#[derive(Debug, Clone, Default)]
pub struct LayoutArea {
    pub progress_time_left: Rect,
    pub progress_bar: Rect,
    pub progress_time_right: Rect,
    pub song_info: Rect,
    pub song_detail: Rect,
    pub cover: Rect,
    pub controls: Rect,
    pub gauge: Rect,
    pub spinner: Rect,
    pub mode_icon: Rect,
    pub volume: Rect,
}

pub trait Playerbar {
    /// Build the concrete sub-areas from the already-inner area.
    fn layout(&self, area: Rect, config: &PlayerbarConfig, is_sixel: bool) -> LayoutArea;

    fn render(
        &self,
        f: &mut Frame,
        player: &PlaybackState,
        colors: &Theme,
        tick: u64,
        config: &PlayerbarConfig,
        layout: &LayoutArea,
    );

    #[allow(clippy::too_many_arguments)]
    fn draw(
        &self,
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
        f.render_widget(block, area);

        if let Some(err) = &player.error {
            f.render_widget(
                Paragraph::new(format!(" ⚠  {}", err)).style(Style::default().fg(colors.error)),
                inner,
            );
            return;
        }

        let layout = self.layout(inner, config, is_sixel);
        self.render(f, player, colors, tick, config, &layout);
    }
}

pub fn draw(
    f: &mut Frame,
    player: &PlaybackState,
    tick: u64,
    bs: &BlockStyle<'_>,
    config: &PlayerbarConfig,
    area: Rect,
    is_sixel: bool,
) {
    let layout: &dyn Playerbar = match config.layout {
        LayoutType::Default => &default_layout::DefaultLayout,
        LayoutType::Modern => &modern_layout::ModernLayout,
        LayoutType::Minimal => &minimal_layout::MinimalLayout,
    };
    layout.draw(f, player, tick, bs, config, area, is_sixel);
}
