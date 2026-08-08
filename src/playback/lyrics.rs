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
            let time = Duration::from_secs_f64(mins * 60.0 + secs);
            Some(LyricLine { time, text })
        })
        .collect();
    // Only sort if not already sorted (LRC files are typically pre-sorted)
    if lines.windows(2).any(|w| w[0].time > w[1].time) {
        lines.sort_by_key(|l| l.time);
    }
    lines
}
