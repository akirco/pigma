mod default_layout;
mod minimal_layout;
mod modern_layout;
mod widgets;

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    widgets::{Padding, Paragraph},
};

use super::{BlockStyle, block::CornerBlock};
use crate::{
    config::{LayoutType, PlayerbarConfig, Theme},
    playback::PlaybackState,
};

#[derive(Debug, Clone, Default)]
pub(super) struct LayoutArea {
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
    /// Playback-speed badge at the right end of the bar.
    pub speed: Rect,
}

pub(super) trait Playerbar {
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
        let block = CornerBlock::from_color(bs, bs.colors.bg).block_padding(Padding::horizontal(1));
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

pub(super) fn draw(
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ncm_api::{SongCopyright, SongInfo};
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::config::BorderConfig;

    const W: u16 = 90;
    const H: u16 = 6;

    fn state(speed: f64, pitch_locked: bool) -> PlaybackState {
        let song = Arc::new(SongInfo {
            id: 1,
            name: "Test Song".into(),
            singer: "Tester".into(),
            artist_id: 2,
            album: "Album".into(),
            album_id: 3,
            pic_url: String::new(),
            duration: 180_000,
            copyright: SongCopyright::Free,
        });
        PlaybackState {
            speed,
            pitch_locked,
            playing: true,
            current_song: Some(song),
            ..PlaybackState::default()
        }
    }

    /// Render one layout and return `(column, text)` for every speed badge
    /// drawn on the bar — the only place a digit is followed by `x`. The column
    /// is that of the badge's first cell.
    fn badges(layout: LayoutType, speed: f64, pitch_locked: bool) -> Vec<(u16, String)> {
        let colors = Theme::default();
        let border = BorderConfig::default();
        let bs = BlockStyle {
            colors: &colors,
            border: &border,
            tick: 0,
        };
        let config = PlayerbarConfig {
            layout,
            ..PlayerbarConfig::default()
        };
        let state = state(speed, pitch_locked);

        let mut term = Terminal::new(TestBackend::new(W, H)).unwrap();
        term.draw(|f| draw(f, &state, 0, &bs, &config, Rect::new(0, 0, W, H), false))
            .unwrap();

        let buf = term.backend().buffer().clone();
        let mut found = Vec::new();
        for y in 0..buf.area().height {
            let row: Vec<String> = (0..buf.area().width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect();
            let cells: Vec<&str> = row.iter().map(|s| s.as_str()).collect();
            for (i, sym) in cells.iter().enumerate().skip(1) {
                if *sym != "x"
                    || !cells[i - 1]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_digit())
                {
                    continue;
                }
                // Walk back over the digits, the decimal point and the star.
                let mut start = i - 1;
                while start > 0
                    && cells[start - 1]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_digit() || c == '.' || c == '*')
                {
                    start -= 1;
                }
                found.push((start as u16, cells[start..=i].concat()));
            }
        }
        found
    }

    const LAYOUTS: [LayoutType; 3] = [LayoutType::Default, LayoutType::Modern, LayoutType::Minimal];

    #[test]
    fn speed_badge_is_hidden_at_normal_speed() {
        for layout in LAYOUTS {
            for locked in [true, false] {
                assert!(
                    badges(layout.clone(), 1.0, locked).is_empty(),
                    "{layout:?} (pitch_locked {locked}) shows a badge at 1.0x"
                );
            }
        }
    }

    #[test]
    fn speed_badge_sits_at_the_right_end() {
        for layout in LAYOUTS {
            let found = badges(layout.clone(), 1.5, true);
            assert_eq!(found.len(), 1, "{layout:?}: expected exactly one badge");
            let (col, _) = &found[0];
            assert!(
                *col > W * 3 / 4,
                "{layout:?}: badge starts at column {col}, expected past the right quarter of a {W}-column bar"
            );
        }
    }

    #[test]
    fn badge_marks_pitch_lock_with_a_star() {
        for layout in LAYOUTS {
            let locked = badges(layout.clone(), 1.5, true);
            assert_eq!(
                locked[0].1, "*1.5x",
                "{layout:?}: pitch-keeping speed must be marked"
            );
            let shifted = badges(layout.clone(), 1.5, false);
            assert_eq!(
                shifted[0].1, "1.5x",
                "{layout:?}: resampled speed must carry no star"
            );
        }
    }
}
