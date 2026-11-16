//! Parsed lyric lines shared between the playback engine, the service layer
//! and the lyrics renderer.

use std::time::Duration;

/// A single parsed lyric line.
#[derive(Debug, Clone)]
pub struct LyricLine {
    pub time: Duration,
    pub text: String,
}

pub fn parse_lyric_lines(raw: &[String]) -> Vec<LyricLine> {
    let mut lines: Vec<LyricLine> = raw
        .iter()
        .filter_map(|line| {
            let rest = line.strip_prefix('[')?;
            let close = rest.find(']')?;
            let ts = &rest[..close];
            let text = rest[close + 1..].trim().to_string();
            if text.is_empty() {
                return None;
            }
            let parts: Vec<&str> = ts.split(':').collect();
            if parts.len() < 2 {
                return None;
            }
            let mins: f64 = parts[0].parse().ok()?;
            let secs: f64 = parts[1].parse().ok()?;
            // Lyrics come from a remote API: reject negative, non-finite and
            // overflowing timestamps instead of panicking in `from_secs_f64`.
            let time = Duration::try_from_secs_f64(mins * 60.0 + secs).ok()?;
            Some(LyricLine { time, text })
        })
        .collect();
    // Only sort if not already sorted (LRC files are typically pre-sorted)
    if lines.windows(2).any(|w| w[0].time > w[1].time) {
        lines.sort_by_key(|l| l.time);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(raw: &[&str]) -> Vec<LyricLine> {
        parse_lyric_lines(&raw.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn parses_normal_lrc_and_sorts_out_of_order_lines() {
        let parsed = lines(&["[00:05.00]later", "[00:01.50]first"]);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].text, "first");
        assert_eq!(parsed[0].time, Duration::from_millis(1_500));
    }

    #[test]
    fn malformed_timestamps_are_skipped_instead_of_panicking() {
        let parsed = lines(&[
            "[-1:00]negative",
            "[999999999999999999999:00]overflow",
            "[nan:00]not-a-number",
            "[00:01]ok",
        ]);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].text, "ok");
    }
}
