use time::OffsetDateTime;
use time::format_description::FormatItem;
use time::macros::format_description;

/// `YYYY-MM-DD HH:MM:SS` in local timezone.
pub fn local_timestamp() -> String {
    let now = match time::OffsetDateTime::now_local() {
        Ok(t) => t,
        Err(_) => OffsetDateTime::now_utc(),
    };
    now.format(&TIMESTAMP_FMT)
        .unwrap_or_else(|_| String::from("0000-00-00 00:00:00"))
}

/// `HH:MM:SS` in local timezone (for UI clock display).
pub fn clock_time() -> String {
    local_timestamp()[11..].to_string()
}

const TIMESTAMP_FMT: &[FormatItem<'static>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

pub fn format_duration(ms: u64) -> String {
    let mut s = String::with_capacity(5);
    format_duration_into(ms, &mut s);
    s
}

/// Write a `MM:SS` (or `H:MM:SS`) duration into `out`, reusing its allocation.
/// Avoids a per-call `String` allocation on hot render paths.
pub fn format_duration_into(ms: u64, out: &mut String) {
    let total_secs = ms / 1000;
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    out.clear();
    if hours > 0 {
        use std::fmt::Write;
        let _ = write!(out, "{}:{:02}:{:02}", hours, mins, secs);
    } else {
        use std::fmt::Write;
        let _ = write!(out, "{:02}:{:02}", mins, secs);
    }
}

/// Parse a `MM:SS` / `HH:MM:SS` duration string into total seconds.
pub fn parse_duration_secs(s: &str) -> Option<u64> {
    let parts: Vec<&str> = s.split(':').collect();
    let mut secs: u64 = 0;
    for (i, p) in parts.iter().rev().enumerate() {
        let n: u64 = p.parse().ok()?;
        secs += n * 60u64.pow(i as u32);
    }
    Some(secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_is_hhmmss() {
        let c = clock_time();
        assert_eq!(c.len(), 8);
        assert_eq!(&c[2..3], ":");
        assert_eq!(&c[5..6], ":");
    }

    #[test]
    fn timestamp_len() {
        assert_eq!(local_timestamp().len(), 19);
    }

    #[test]
    fn parse_mmss() {
        assert_eq!(parse_duration_secs("3:30"), Some(210));
        assert_eq!(parse_duration_secs("1:03:30"), Some(3810));
    }
}
