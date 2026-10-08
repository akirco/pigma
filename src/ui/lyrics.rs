use std::{cell::Cell, time::Duration};

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Padding, Paragraph},
};

use super::{BlockStyle, block::CornerBlock};
use crate::{
    config::Theme,
    playback::{LyricLine, PlaybackState},
    utils::GradientPreset,
};

thread_local! {
    static LAST_CUR: Cell<usize> = const { Cell::new(0) };
}

/// Find the current lyric index — incremental forward scan, O(1) amortized.
fn find_current_line(lyrics: &[LyricLine], cur_ms: f64) -> usize {
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

pub(super) fn draw(
    f: &mut Frame,
    player: &PlaybackState,
    bs: &BlockStyle<'_>,
    gradient: GradientPreset,
    title: &str,
    area: Rect,
) {
    let colors = bs.colors;
    let block = CornerBlock::from_color(bs, bs.colors.bg).title(title, bs.colors);
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
            && let Some(tl) = translation_for(translated_lyrics, l.time)
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

/// Look up the translation belonging to the original line at `time`.
///
/// NCM delivers the translation as its own LRC stream (`tlyric`). Both streams
/// carry the same timestamps for translated lines, but they drop different
/// ones: `lyric` keeps the timestamped credits and instrumental markers that
/// `tlyric` omits, while `tlyric` may cover lines the original splits. Their
/// lengths therefore differ, and pairing them by index shifts every translation
/// by however many lines were dropped. Pair them on the timestamp instead.
///
/// `translated` must be sorted by time, which `parse_lyric_lines` guarantees.
fn translation_for(translated: &[LyricLine], time: Duration) -> Option<&LyricLine> {
    let next = translated.partition_point(|line| line.time < time);
    translated.get(next).filter(|line| line.time == time)
}

fn render_current_line<'a>(
    text: &'a str,
    cur_ms: f64,
    line_ms: f64,
    next_ms: Option<f64>,
    colors: &Theme,
    gradient: GradientPreset,
) -> Line<'a> {
    let seg_dur = next_ms.map(|n| (n - line_ms).max(1.0)).unwrap_or(4000.0);
    let seg_progress = ((cur_ms - line_ms) / seg_dur).clamp(0.0, 1.0);
    let total = text.chars().count();
    let split_at = (total as f64 * seg_progress).floor() as usize;

    let mut line = Line::default();
    for (byte_start, ch) in text.char_indices() {
        let j = line.spans.len();
        let byte_end = byte_start + ch.len_utf8();
        let ch_str = &text[byte_start..byte_end];
        let s = if j < split_at {
            let t = j as f64 / split_at.max(1) as f64;
            let [r, g, b] = gradient.color(t as f32);
            Span::styled(ch_str, Style::default().fg(Color::Rgb(r, g, b)))
        } else if j == split_at {
            Span::styled(
                ch_str,
                Style::default()
                    .fg(Color::White)
                    .bg(colors.accent)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            let t = (j - split_at) as f64 / (total - split_at).max(1) as f64;
            let [r, g, b] = gradient.color(1.0 - t as f32);
            Span::styled(ch_str, Style::default().fg(Color::Rgb(r, g, b)))
        };
        line.push_span(s);
    }

    line.alignment(Alignment::Center)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ncm_api::{SongCopyright, SongInfo};
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::{config::BorderConfig, playback::parse_lyric_lines};

    /// Credit lines carry timestamps in `lyric` but have no counterpart in
    /// `tlyric`, so the two streams are not index-aligned. Shape taken from the
    /// real response for song 2755332551 ("DAMIDAMI").
    const LYRIC: &[&str] = &[
        "[00:00.00] 制作人 : Sihan",
        "[00:00.66] 作曲 : Sihan",
        "[00:01.32] 编曲 : Sihan",
        "[00:07.26] Moon's up high, shining wide.",
        "[00:08.98] Counting sheep on my bed.",
        "[00:11.44] Who's still awake? Tossing and turning",
    ];
    const TLYRIC: &[&str] = &[
        "[00:07.26] 明月光，夜夜亮",
        "[00:08.98] 让我来看看：有谁睡得不香？",
        "[00:11.44] 把月光拉长，盖在你身上",
    ];

    fn lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|s| (*s).to_string()).collect()
    }

    fn player() -> PlaybackState {
        let song = SongInfo {
            id: 2755332551,
            name: "DAMIDAMI".to_string(),
            singer: "Sihan".to_string(),
            artist_id: 0,
            album: "绝区零-DAMIDAMI".to_string(),
            album_id: 0,
            pic_url: String::new(),
            duration: 191_000,
            copyright: SongCopyright::Free,
        };
        let mut player = PlaybackState {
            current_song: Some(Arc::new(song)),
            lyrics: Some(parse_lyric_lines(&lines(LYRIC))),
            translated_lyrics: Some(parse_lyric_lines(&lines(TLYRIC))),
            ..PlaybackState::default()
        };
        // 7.5s in, "Moon's up high, shining wide." is the current line.
        player.progress = 7.5 / 191.0;
        player
    }

    fn render(player: &PlaybackState) -> Vec<String> {
        let theme = Theme::default();
        let border = BorderConfig::default();
        let block_style = BlockStyle {
            colors: &theme,
            border: &border,
            tick: 0,
        };
        let mut terminal = Terminal::new(TestBackend::new(60, 14)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                draw(
                    frame,
                    player,
                    &block_style,
                    GradientPreset::default(),
                    "LYRICS",
                    area,
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let area = buffer.area;
        (area.y..area.y + area.height)
            .map(|y| {
                (area.x..area.x + area.width)
                    .map(|x| buffer.cell((x, y)).map_or(" ", |cell| cell.symbol()))
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    fn row_with(screen: &[String], needle: &str) -> usize {
        screen
            .iter()
            .position(|row| squash(row).contains(&squash(needle)))
            .unwrap_or_else(|| panic!("{needle:?} was not rendered:\n{}", screen.join("\n")))
    }

    /// Drop whitespace: the test backend pads the cell after every wide
    /// character, so CJK text comes back as "明 月 光".
    fn squash(text: &str) -> String {
        text.chars().filter(|c| !c.is_whitespace()).collect()
    }

    fn timed(millis: u64, text: &str) -> LyricLine {
        LyricLine {
            time: Duration::from_millis(millis),
            text: text.to_string(),
        }
    }

    /// Deterministic xorshift64 so the randomized cases stay reproducible.
    fn next_rand(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    #[test]
    fn translation_lookup_matches_timestamp() {
        let translated = vec![timed(1_000, "一"), timed(2_000, "二")];
        let at =
            |ms| translation_for(&translated, Duration::from_millis(ms)).map(|l| l.text.as_str());

        assert_eq!(at(2_000), Some("二"));
        assert_eq!(at(1_000), Some("一"));
        // A line the translation stream does not cover stays untranslated.
        assert_eq!(at(1_500), None);
        assert_eq!(at(3_000), None);
    }

    #[test]
    fn extra_translation_lines_do_not_shift_the_lookup() {
        // A translation the original stream never mentions must not displace
        // the pairs around it.
        let translated = vec![
            timed(400, "多出来的"),
            timed(2_000, "二"),
            timed(3_000, "三"),
        ];

        assert_eq!(
            translation_for(&translated, Duration::from_millis(2_000)).map(|l| l.text.as_str()),
            Some("二")
        );
        assert_eq!(
            translation_for(&translated, Duration::from_millis(3_000)).map(|l| l.text.as_str()),
            Some("三")
        );
        // Gaps stay untranslated instead of borrowing a neighbour.
        assert!(translation_for(&translated, Duration::from_millis(1_200)).is_none());
    }

    #[test]
    fn duplicate_timestamps_resolve_deterministically() {
        let translated = vec![timed(1_000, "先"), timed(1_000, "后")];

        assert_eq!(
            translation_for(&translated, Duration::from_millis(1_000)).map(|l| l.text.as_str()),
            Some("先")
        );
    }

    #[test]
    fn streams_are_sorted_before_pairing() {
        let parsed = parse_lyric_lines(&lines(&["[00:02.00]b", "[00:01.00]a", "[00:03.00]c"]));

        assert!(parsed.windows(2).all(|w| w[0].time <= w[1].time));
        // `partition_point` relies on that order.
        assert_eq!(
            translation_for(&parsed, Duration::from_secs(2)).map(|l| l.text.as_str()),
            Some("b")
        );
    }

    #[test]
    fn empty_translation_stream_pairs_nothing() {
        let translated: Vec<LyricLine> = Vec::new();

        assert!(translation_for(&translated, Duration::ZERO).is_none());
        assert!(translation_for(&translated, Duration::from_secs(42)).is_none());
    }

    #[test]
    fn pairing_matches_a_linear_scan_on_random_streams() {
        let mut state = 0x2545_F491_4F6C_DD1D_u64;

        for round in 0..64 {
            let mut translated: Vec<LyricLine> = (0..400)
                .map(|_| {
                    let ms = next_rand(&mut state) % 4_000;
                    timed(ms, &format!("t{ms}"))
                })
                .collect();
            translated.sort_by_key(|line| line.time);

            for _ in 0..400 {
                let ms = next_rand(&mut state) % 5_000;
                let time = Duration::from_millis(ms);
                let expected = translated.iter().find(|line| line.time == time);

                assert_eq!(
                    translation_for(&translated, time).map(|line| line.text.as_str()),
                    expected.map(|line| line.text.as_str()),
                    "round {round}, lookup at {ms}ms"
                );
            }
        }
    }

    #[test]
    fn pairing_scales_to_large_streams() {
        let translated: Vec<LyricLine> = (0..20_000_u64)
            .map(|i| timed(i * 10 + 5, &i.to_string()))
            .collect();

        for i in (0..20_000_u64).step_by(97) {
            let expected = i.to_string();
            assert_eq!(
                translation_for(&translated, Duration::from_millis(i * 10 + 5))
                    .map(|l| l.text.as_str()),
                Some(expected.as_str())
            );
        }
        assert!(translation_for(&translated, Duration::from_millis(1)).is_none());
        assert!(translation_for(&translated, Duration::from_millis(200_000)).is_none());
    }

    #[test]
    fn translation_is_paired_by_timestamp_not_index() {
        let screen = render(&player());

        let original = row_with(&screen, "Moon's up high");
        assert!(
            squash(&screen[original + 1]).contains("明月光，夜夜亮"),
            "translation did not follow its original line:\n{}",
            screen.join("\n")
        );

        // Index pairing would hand the first translation to the first credit line.
        let credit = row_with(&screen, "制作人");
        assert!(
            !squash(&screen[credit + 1]).contains("明月光"),
            "credit line stole the first translation:\n{}",
            screen.join("\n")
        );
    }

    #[test]
    fn untranslated_lines_render_without_a_translation_row() {
        let screen = render(&player());
        let credit = row_with(&screen, "制作人");

        assert!(
            squash(&screen[credit + 1]).contains("作曲"),
            "unexpected row under an untranslated line:\n{}",
            screen.join("\n")
        );
    }
}
