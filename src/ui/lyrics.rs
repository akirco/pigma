use std::cell::Cell;

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Padding, Paragraph};

use super::create_block;
use crate::state::PlaybackState;
use crate::theme::Theme;

thread_local! {
    static LAST_CUR: Cell<usize> = const { Cell::new(0) };
}

/// Find the current lyric index — incremental forward scan, O(1) amortized.
fn find_current_line(lyrics: &[crate::state::PlaybackLyricLine], cur_ms: f64) -> usize {
    LAST_CUR.with(|last| {
        let mut cur = last.get();
        // Reset if lyrics changed (new song)
        if cur >= lyrics.len() {
            cur = 0;
        }
        // Advance forward from last position
        while cur + 1 < lyrics.len() && lyrics[cur + 1].time.as_millis() as f64 <= cur_ms {
            cur += 1;
        }
        // Only scan backward if we overshot (user seeked back)
        if cur > 0 && lyrics[cur].time.as_millis() as f64 > cur_ms {
            cur = lyrics
                .iter()
                .rposition(|l| l.time.as_millis() as f64 <= cur_ms)
                .unwrap_or(0);
        }
        last.set(cur);
        cur
    })
}

pub fn draw(
    f: &mut Frame,
    player: &PlaybackState,
    colors: &Theme,
    bordered: bool,
    border_rounded: bool,
    gradient: &str,
    title: &str,
    area: Rect,
) {
    let block = create_block(title, colors, bordered, border_rounded, false);
    let inner = block.inner(area);
    f.render_widget(block.block_padding(Padding::vertical(1)), area);

    let Some(song) = &player.current_song else {
        return;
    };

    let Some(lyrics) = &player.lyrics else {
        return;
    };

    if lyrics.is_empty() {
        let msg = Line::from("纯音乐，请欣赏")
            .style(Style::default().fg(colors.muted))
            .alignment(Alignment::Center);
        f.render_widget(Paragraph::new(msg), inner);
        return;
    }

    let dur_secs = song.duration as f64 / 1000.0;
    let cur_ms = player.progress * dur_secs * 1000.0;
    let cur = find_current_line(lyrics, cur_ms);

    let h = inner.height as usize;
    let has_translations = player
        .translated_lyrics
        .as_ref()
        .is_some_and(|t| !t.is_empty());
    let lines_per_lyric = if has_translations { 2 } else { 1 };
    let half = (h / lines_per_lyric) / 2;
    let start = cur.saturating_sub(half);
    let end = (start + h / lines_per_lyric).min(lyrics.len());

    let mut lines: Vec<Line> = Vec::new();
    for i in start..end {
        let l = &lyrics[i];
        let text = if l.text.is_empty() {
            "·"
        } else {
            l.text.as_str()
        };

        if i == cur {
            lines.push(render_current_line(
                text,
                cur_ms,
                l.time.as_millis() as f64,
                lyrics.get(i + 1).map(|n| n.time.as_millis() as f64),
                colors,
                gradient,
            ));
        } else {
            let d = i.abs_diff(cur);
            let style = if d <= 2 {
                Style::default().fg(Color::Rgb(136, 136, 136))
            } else {
                Style::default().fg(Color::Rgb(85, 85, 85))
            };
            lines.push(Line::from(text).style(style).alignment(Alignment::Center));
        }

        if let Some(translated_lyrics) = &player.translated_lyrics
            && let Some(tl) = translated_lyrics.get(i)
        {
            let t_style = if i == cur {
                Style::default()
                    .fg(Color::Rgb(180, 180, 180))
                    .add_modifier(Modifier::ITALIC)
            } else {
                let d = i.abs_diff(cur);
                if d <= 2 {
                    Style::default().fg(Color::Rgb(100, 100, 100))
                } else {
                    Style::default().fg(Color::Rgb(60, 60, 60))
                }
            };
            lines.push(
                Line::from(tl.text.as_str())
                    .style(t_style)
                    .alignment(Alignment::Center),
            );
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn render_current_line(
    text: &str,
    cur_ms: f64,
    line_ms: f64,
    next_ms: Option<f64>,
    colors: &Theme,
    gradient: &str,
) -> Line<'static> {
    let seg_dur = next_ms.map(|n| (n - line_ms).max(1.0)).unwrap_or(4000.0);
    let seg_progress = ((cur_ms - line_ms) / seg_dur).clamp(0.0, 1.0);
    let split_at = (text.chars().count() as f64 * seg_progress).floor() as usize;

    let chars: Vec<char> = text.chars().collect();
    let total = chars.len();

    let mut spans: Vec<Span> = Vec::with_capacity(total);

    for (j, &ch) in chars.iter().enumerate() {
        let s = if j < split_at {
            let t = j as f64 / split_at.max(1) as f64;
            let [r, g, b] = crate::utils::gradient_color(gradient, t as f32);
            Span::styled(ch.to_string(), Style::default().fg(Color::Rgb(r, g, b)))
        } else if j == split_at {
            Span::styled(
                ch.to_string(),
                Style::default()
                    .fg(Color::White)
                    .bg(colors.accent)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            let t = (j - split_at) as f64 / (total - split_at).max(1) as f64;
            let [r, g, b] = crate::utils::gradient_color(gradient, 1.0 - t as f32);
            Span::styled(ch.to_string(), Style::default().fg(Color::Rgb(r, g, b)))
        };
        spans.push(s);
    }

    Line::from(spans).alignment(Alignment::Center)
}
