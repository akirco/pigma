//! Equalizer page: ten horizontal sliders with dB on the x-axis and frequency
//! on the y-axis.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{BlockStyle, block::CornerBlock};
use crate::config::{BAND_LABELS, EqualizerConfig, MAX_GAIN_DB, MIN_GAIN_DB};

pub(super) fn draw(
    f: &mut Frame,
    eq: &EqualizerConfig,
    selected: usize,
    bs: &BlockStyle<'_>,
    area: Rect,
) {
    let selected = selected.min(BAND_LABELS.len() - 1);
    let title = format!(
        " EQUALIZER  [{}]   {} Hz  {:+.1} dB ",
        if eq.is_flat() { "OFF" } else { "ON" },
        BAND_LABELS[selected],
        eq.bands[selected]
    );
    let block = CornerBlock::from_color(bs, bs.colors.bg).title(&title, bs.colors);
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 11 || inner.width < 32 {
        f.render_widget(Paragraph::new("窗口太小，无法显示均衡器"), inner);
        return;
    }

    // label (3, right-aligned) + space (1)
    let bar_width = (inner.width as usize).saturating_sub(4).max(9);
    let center = bar_width / 2;

    // Each band needs a row, and each gap between bands needs one too. Show as
    // many bands as fit; the window follows the selection.
    let visible = (inner.height as usize / 2).clamp(1, BAND_LABELS.len());
    let start = selected
        .saturating_sub(visible / 2)
        .min(BAND_LABELS.len() - visible);
    let top = (inner.height as usize).saturating_sub(visible * 2) / 2;

    let mut lines = Vec::with_capacity(visible * 2 + 1);
    lines.extend(std::iter::repeat_n(Line::from(""), top));
    lines.push(Line::from(Span::styled(
        format!("{:4}{}", "", db_axis(bar_width)),
        Style::default().fg(bs.colors.muted),
    )));

    for (offset, band) in (start..start + visible).enumerate() {
        let gain = eq.bands[band];
        let is_selected = band == selected;
        let style = if is_selected {
            Style::default()
                .fg(bs.colors.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(bs.colors.text)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{:>3} ", BAND_LABELS[band]), style),
            Span::styled(slider(bar_width, center, gain), style),
        ]));
        if offset + 1 < visible {
            // The guide continues the `─┼─` row colour: accent next to the
            // selected band, normal text elsewhere.
            let guide_style = if band == selected || band + 1 == selected {
                Style::default()
                    .fg(bs.colors.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(bs.colors.text)
            };
            lines.push(Line::from(Span::styled(
                format!("{:4}{}", "", center_guide(bar_width, center)),
                guide_style,
            )));
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}

/// Horizontal dB ruler, aligned with the slider track.
fn db_axis(width: usize) -> String {
    let mut cells = vec![' '; width];
    let ticks: &[(f32, &str)] = if width >= 40 {
        &[
            (MIN_GAIN_DB, "-12"),
            (-6.0, "-6"),
            (0.0, "0"),
            (6.0, "+6"),
            (MAX_GAIN_DB, "+12"),
        ]
    } else {
        &[(MIN_GAIN_DB, "-12"), (0.0, "0"), (MAX_GAIN_DB, "+12")]
    };
    for &(db, label) in ticks {
        let ratio = (db - MIN_GAIN_DB) / (MAX_GAIN_DB - MIN_GAIN_DB);
        let pos = (ratio * (width.saturating_sub(1)) as f32).round() as usize;
        if width < label.len() {
            continue;
        }
        let start = pos.saturating_sub(label.len() / 2).min(width - label.len());
        for (offset, ch) in label.chars().enumerate() {
            cells[start + offset] = ch;
        }
    }
    cells.into_iter().collect()
}

/// Vertical spacer between two band rows: a single `│` on the 0 dB column,
/// so the `┼` markers form one continuous guide.
fn center_guide(width: usize, center: usize) -> String {
    let mut cells = vec![' '; width];
    if let Some(cell) = cells.get_mut(center) {
        *cell = '│';
    }
    cells.into_iter().collect()
}

fn position(width: usize, gain: f32) -> usize {
    let ratio = ((gain - MIN_GAIN_DB) / (MAX_GAIN_DB - MIN_GAIN_DB)).clamp(0.0, 1.0);
    (ratio * (width.saturating_sub(1)) as f32).round() as usize
}

fn slider(width: usize, center: usize, gain: f32) -> String {
    let pos = position(width, gain);
    let mut cells = vec!['─'; width];
    let (lo, hi) = if pos < center {
        (pos, center)
    } else {
        (center, pos)
    };
    for cell in cells.iter_mut().take(hi + 1).skip(lo) {
        *cell = '━';
    }
    if let Some(cell) = cells.get_mut(center) {
        *cell = '┼';
    }
    if let Some(cell) = cells.get_mut(pos) {
        *cell = '●';
    }
    cells.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::{center_guide, db_axis, slider};

    #[test]
    fn every_row_keeps_the_track_width() {
        for width in [9usize, 20, 60] {
            assert_eq!(slider(width, width / 2, 3.0).chars().count(), width);
            assert_eq!(center_guide(width, width / 2).chars().count(), width);
            assert_eq!(db_axis(width).chars().count(), width);
        }
    }

    #[test]
    fn spacer_is_a_single_center_bar() {
        let guide = center_guide(21, 10);
        assert_eq!(guide.chars().filter(|c| *c == '│').count(), 1);
        assert_eq!(guide.chars().nth(10), Some('│'));
    }
}
