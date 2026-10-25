//! `pigma msg` subcommand: map the CLI action + value onto the IPC wire
//! protocol's `MsgAction` and send it to the running instance.

use super::MsgActionArg;
use crate::ipc::{self, MsgAction};

/// `pigma msg` handler. `search` is a request/response command (the daemon
/// returns matching songs and registers them for a later `pigma msg play <id>`);
/// everything else is a fire-and-forget control action.
pub async fn msg(
    action: MsgActionArg,
    value: Option<&str>,
    playlist: Option<usize>,
) -> eyre::Result<()> {
    match action {
        MsgActionArg::Search => {
            let keyword = value.ok_or_else(|| {
                eyre::eyre!("search requires a keyword (e.g. `pigma msg search 周杰伦`)")
            })?;
            let results = ipc::search_songs(keyword).await?;
            if results.is_empty() {
                println!("没有找到与「{keyword}」相关的歌曲");
                return Ok(());
            }
            for entry in &results {
                println!(
                    "{:<10} {:<20} {} - {}",
                    entry.source, entry.id, entry.name, entry.singer
                );
            }
            Ok(())
        }
        other => {
            let action = parse_msg_action(other, value, playlist)?;
            ipc::send_msg(action).await?;
            Ok(())
        }
    }
}

fn parse_msg_action(
    action: MsgActionArg,
    value: Option<&str>,
    playlist: Option<usize>,
) -> eyre::Result<MsgAction> {
    match action {
        MsgActionArg::Previous => Ok(MsgAction::Previous),
        MsgActionArg::Next => Ok(MsgAction::Next),
        MsgActionArg::Pause => Ok(MsgAction::Pause),
        MsgActionArg::Play => {
            let song_id = match value {
                None => None,
                Some(v) => Some(v.parse().map_err(|_| {
                    eyre::eyre!("invalid song id `{v}` (expected a number, or omit to play/resume)")
                })?),
            };
            Ok(MsgAction::Play { song_id })
        }
        MsgActionArg::TogglePlay => Ok(MsgAction::TogglePlay),
        MsgActionArg::SwitchList => {
            let endpoint = value.ok_or_else(|| {
                eyre::eyre!(
                    "switch-list requires an endpoint (e.g. `pigma msg switch-list toplist`)"
                )
            })?;
            Ok(MsgAction::SwitchList {
                endpoint: endpoint.to_string(),
                playlist,
            })
        }
        MsgActionArg::Volume => {
            let value = value
                .ok_or_else(|| eyre::eyre!("volume requires a value like `75`, `+5` or `-5`"))?;
            parse_volume(value)
        }
        MsgActionArg::Speed => {
            let value = value.ok_or_else(|| {
                eyre::eyre!("speed requires a value like `1.5`, `+0.1` or `-0.1`")
            })?;
            parse_speed(value)
        }
        MsgActionArg::Pitch => Ok(MsgAction::Pitch {
            locked: parse_pitch(value)?,
        }),
        MsgActionArg::Mode => Ok(MsgAction::Mode),
        MsgActionArg::Like => Ok(MsgAction::Like),
        MsgActionArg::Dislike => Ok(MsgAction::Dislike),
        MsgActionArg::ToggleLike => Ok(MsgAction::ToggleLike),
        // Handled above in `msg` before parsing.
        MsgActionArg::Search => unreachable!(),
    }
}

/// Parse a volume value. A leading `+`/`-` is a delta (percent) applied via
/// `adjust_volume`, matching the TUI's `+`/`-` keys; a bare number is an
/// absolute volume percent.
fn parse_volume(value: &str) -> eyre::Result<MsgAction> {
    let err = || eyre::eyre!("invalid volume `{value}`");
    if let Some(delta) = value.strip_prefix('+').or_else(|| value.strip_prefix('-')) {
        let number: f64 = delta.parse().map_err(|_| err())?;
        let signed = if value.starts_with('-') {
            -number
        } else {
            number
        };
        Ok(MsgAction::Volume {
            delta: Some(signed / 100.0),
            absolute: None,
        })
    } else {
        let number: f64 = value.parse().map_err(|_| err())?;
        if !(0.0..=100.0).contains(&number) {
            return Err(err());
        }
        Ok(MsgAction::Volume {
            delta: None,
            absolute: Some(number / 100.0),
        })
    }
}

/// Parse a playback speed value. A leading `+`/`-` is a delta factor applied by
/// `adjust_speed` (matching the TUI's `[`/`]` keys, e.g. `+0.1`); a bare number
/// is an absolute factor (e.g. `1.5`). The daemon clamps both into the
/// supported range.
fn parse_speed(value: &str) -> eyre::Result<MsgAction> {
    let err = || eyre::eyre!("invalid speed `{value}` (expected e.g. `1.5`, `+0.1`)");
    if let Some(delta) = value.strip_prefix('+').or_else(|| value.strip_prefix('-')) {
        let number: f64 = delta.parse().map_err(|_| err())?;
        let signed = if value.starts_with('-') {
            -number
        } else {
            number
        };
        Ok(MsgAction::Speed {
            delta: Some(signed),
            absolute: None,
        })
    } else {
        let number: f64 = value.parse().map_err(|_| err())?;
        if number <= 0.0 {
            return Err(err());
        }
        Ok(MsgAction::Speed {
            delta: None,
            absolute: Some(number),
        })
    }
}

/// Parse the `pitch` value: `on`/`true`/`1` locks the pitch (WSOLA stretch, the
/// badge gains `*`), `off`/`false`/`0` resamples; `toggle` or no value asks the
/// daemon to flip it (`None`).
fn parse_pitch(value: Option<&str>) -> eyre::Result<Option<bool>> {
    match value.unwrap_or("toggle") {
        "on" | "true" | "1" | "lock" => Ok(Some(true)),
        "off" | "false" | "0" | "unlock" => Ok(Some(false)),
        "toggle" | "flip" => Ok(None),
        other => Err(eyre::eyre!(
            "invalid pitch `{other}` (expected `on`, `off` or `toggle`)"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_volume_absolute() {
        let action = parse_volume("75").unwrap();
        assert_eq!(
            action,
            MsgAction::Volume {
                delta: None,
                absolute: Some(0.75),
            }
        );
    }

    #[test]
    fn parse_volume_delta() {
        let plus = parse_volume("+5").unwrap();
        assert_eq!(
            plus,
            MsgAction::Volume {
                delta: Some(0.05),
                absolute: None,
            }
        );
        let minus = parse_volume("-10").unwrap();
        assert_eq!(
            minus,
            MsgAction::Volume {
                delta: Some(-0.10),
                absolute: None,
            }
        );
    }

    #[test]
    fn parse_volume_out_of_range() {
        assert!(parse_volume("150").is_err());
        assert!(parse_volume("abc").is_err());
    }

    #[test]
    fn parse_speed_absolute_and_delta() {
        assert_eq!(
            parse_speed("1.5").unwrap(),
            MsgAction::Speed {
                delta: None,
                absolute: Some(1.5),
            }
        );
        assert_eq!(
            parse_speed("+0.1").unwrap(),
            MsgAction::Speed {
                delta: Some(0.1),
                absolute: None,
            }
        );
        assert_eq!(
            parse_speed("-0.2").unwrap(),
            MsgAction::Speed {
                delta: Some(-0.2),
                absolute: None,
            }
        );
        assert!(parse_speed("0").is_err());
        assert!(parse_speed("abc").is_err());
    }

    #[test]
    fn parse_pitch_values() {
        assert_eq!(
            parse_msg_action(MsgActionArg::Pitch, Some("on"), None).unwrap(),
            MsgAction::Pitch { locked: Some(true) }
        );
        assert_eq!(
            parse_msg_action(MsgActionArg::Pitch, Some("off"), None).unwrap(),
            MsgAction::Pitch {
                locked: Some(false)
            }
        );
        assert_eq!(
            parse_msg_action(MsgActionArg::Pitch, None, None).unwrap(),
            MsgAction::Pitch { locked: None }
        );
        assert!(parse_msg_action(MsgActionArg::Pitch, Some("sideways"), None).is_err());
    }

    #[test]
    fn parse_msg_play_with_optional_id() {
        let plain = parse_msg_action(MsgActionArg::Play, None, None).unwrap();
        assert_eq!(plain, MsgAction::Play { song_id: None });

        let with_id = parse_msg_action(MsgActionArg::Play, Some("187186"), None).unwrap();
        assert_eq!(
            with_id,
            MsgAction::Play {
                song_id: Some(187186)
            }
        );

        assert!(parse_msg_action(MsgActionArg::Play, Some("abc"), None).is_err());
        assert_eq!(
            parse_msg_action(MsgActionArg::TogglePlay, None, None).unwrap(),
            MsgAction::TogglePlay
        );
    }

    #[test]
    fn msg_switch_list_and_aliases() {
        assert_eq!(
            parse_msg_action(MsgActionArg::SwitchList, Some("toplist"), None).unwrap(),
            MsgAction::SwitchList {
                endpoint: "toplist".into(),
                playlist: None,
            }
        );
        assert_eq!(
            parse_msg_action(MsgActionArg::ToggleLike, None, None).unwrap(),
            MsgAction::ToggleLike
        );
        assert_eq!(
            parse_msg_action(MsgActionArg::Previous, None, None).unwrap(),
            MsgAction::Previous
        );
    }
}
