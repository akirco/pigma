//! `pigma status` subcommand: fetch a snapshot or the queue from the running
//! instance and render it as JSON, a `{token}` template, or a plain listing.

use crate::{config::Config, ipc, ipc::StatusSnapshot, utils::format_duration};

/// `pigma status` handler.
pub async fn status(template: &str, json: bool, list: bool) -> eyre::Result<()> {
    let config = Config::load();
    if list {
        let queue = ipc::fetch_queue().await?;
        if json {
            println!("{}", serde_json::to_string_pretty(&queue)?);
        } else {
            print!("{}", render_queue(&queue));
        }
        return Ok(());
    }
    let snapshot = ipc::fetch_status().await?;
    if json || config.cli_status_format == "json" {
        println!("{}", serde_json::to_string_pretty(&snapshot)?);
    } else {
        let template = if template.is_empty() {
            &config.cli_status_template
        } else {
            template
        };
        println!("{}", format_status(template, &snapshot));
    }
    Ok(())
}

/// Render the queue as plain text: `>` marks the current song, and each row is
/// `index  title  artist  duration  id`. The trailing song id lets the output
/// feed straight into `pigma msg play <id>`.
fn render_queue(queue: &ipc::QueueSnapshot) -> String {
    let current = queue.current_index;
    let mut out = String::new();
    for (i, song) in queue.songs.iter().enumerate() {
        let marker = if Some(i) == current { ">" } else { " " };
        use std::fmt::Write;
        let _ = writeln!(
            out,
            "{marker} {:<3} {:<32} {:<24} {:<8} {}",
            i + 1,
            song.name,
            song.singer,
            format_duration(song.duration_ms),
            song.id
        );
    }
    out
}

/// Replace `{token}` placeholders in a plain-status template.
fn format_status(template: &str, s: &StatusSnapshot) -> String {
    let duration = format_duration(s.duration_ms);
    let current = format_duration(s.position_ms);
    let volume = (s.volume * 100.0).round() as u64;
    let speed = format!("{:.1}", s.speed);
    // What the player bar shows: `*1.2x` when the speed change keeps the pitch,
    // `1.2x` when it resamples, and nothing at all at normal speed.
    let speed_badge = if (s.speed - 1.0).abs() < f64::EPSILON {
        format!("{:.1}x", s.speed)
    } else if s.pitch_locked {
        format!("*{:.1}x", s.speed)
    } else {
        format!("{:.1}x", s.speed)
    };
    let status = if !s.playing {
        "stopped"
    } else if s.paused {
        "paused"
    } else {
        "playing"
    };
    template
        .replace("{current}", &current)
        .replace("{position}", &current)
        .replace("{duration}", &duration)
        .replace("{artist}", &s.artist)
        .replace("{name}", &s.name)
        .replace("{album}", &s.album)
        .replace("{volume}", &volume.to_string())
        .replace("{speed}", &speed)
        .replace("{speed_badge}", &speed_badge)
        .replace("{status}", status)
        .replace("{mode}", &s.mode)
        .replace("{id}", &s.id.to_string())
        .replace("{liked}", if s.liked { "true" } else { "false" })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> StatusSnapshot {
        StatusSnapshot {
            id: 123,
            name: "Example Song".into(),
            artist: "Example Artist".into(),
            album: "Example Album".into(),
            duration_ms: 125000,
            position_ms: 65000,
            volume: 0.75,
            speed: 1.0,
            pitch_locked: true,
            playing: true,
            paused: false,
            mode: "sequential".into(),
            liked: true,
        }
    }

    #[test]
    fn format_status_plain() {
        let out = format_status(
            "{current}/{duration} {artist} {name} {volume}% {speed}x {status}",
            &snapshot(),
        );
        assert_eq!(
            out,
            "01:05/02:05 Example Artist Example Song 75% 1.0x playing"
        );
    }

    #[test]
    fn format_status_speed_badge_marks_pitch_lock() {
        let mut s = snapshot();
        s.speed = 1.2;
        s.pitch_locked = true;
        assert_eq!(format_status("{speed} {speed_badge}", &s), "1.2 *1.2x");
        s.pitch_locked = false;
        assert_eq!(format_status("{speed} {speed_badge}", &s), "1.2 1.2x");
        // Normal speed still renders a badge (no star) rather than an empty token.
        s.speed = 1.0;
        assert_eq!(format_status("{speed_badge}", &s), "1.0x");
        // {speed} is untouched by the new token.
        assert_eq!(format_status("{speed}", &s), "1.0");
    }

    #[test]
    fn format_status_unknown_tokens_left_alone() {
        let out = format_status("{name} {bogus}", &snapshot());
        assert_eq!(out, "Example Song {bogus}");
    }

    #[test]
    fn render_queue_marks_current_and_includes_id() {
        let queue = ipc::QueueSnapshot {
            key: "test".into(),
            current_index: Some(0),
            songs: vec![
                ipc::QueueEntry {
                    id: 111,
                    name: "Song A".into(),
                    singer: "Artist A".into(),
                    album: String::new(),
                    duration_ms: 125_000,
                },
                ipc::QueueEntry {
                    id: 222,
                    name: "Song B".into(),
                    singer: "Artist B".into(),
                    album: String::new(),
                    duration_ms: 65_000,
                },
            ],
        };
        let out = render_queue(&queue);
        let mut lines = out.lines();
        let first = lines.next().unwrap();
        assert!(first.starts_with("> 1"), "current song not marked: {first}");
        assert!(first.contains("111"), "song id missing: {first}");
        let second = lines.next().unwrap();
        assert!(
            second.starts_with("  2"),
            "non-current marker wrong: {second}"
        );
        assert!(second.contains("222"), "song id missing: {second}");
    }
}
