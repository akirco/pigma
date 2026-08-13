use super::song::{SongCopyright, SongInfo};
use serde_json::Value;

// --- Radio / smart-play parsing ---

pub(crate) fn parse_radio_programs(value: &Value) -> Result<Vec<SongInfo>, String> {
    let array = value["programs"].as_array().ok_or("programs not found")?;
    array
        .iter()
        .map(|v| {
            let main = &v["mainSong"];
            Ok(SongInfo {
                id: v["mainTrackId"]
                    .as_u64()
                    .or_else(|| main["id"].as_u64())
                    .unwrap_or(0),
                name: v["name"].as_str().unwrap_or("unknown").to_string(),
                singer: v["dj"]["nickname"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string(),
                artist_id: 0,
                album: v["radio"]["name"].as_str().unwrap_or("").to_string(),
                album_id: 0,
                pic_url: v["coverUrl"].as_str().unwrap_or("").to_string(),
                duration: v["duration"].as_u64().unwrap_or(0),
                copyright: SongCopyright::Unknown,
            })
        })
        .collect()
}

pub(crate) fn parse_intelligence_songs(value: &Value) -> Result<Vec<SongInfo>, String> {
    let array = value["data"].as_array().ok_or("data not found")?;
    array
        .iter()
        .map(|v| {
            let info = &v["songInfo"];
            let unk = "unknown".to_string();
            let singer = info
                .get("ar")
                .or_else(|| info.get("artists"))
                .and_then(|a| a.as_array())
                .and_then(|a| a.first())
                .and_then(|a| a.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or(&unk)
                .to_string();
            let artist_id = info
                .get("ar")
                .or_else(|| info.get("artists"))
                .and_then(|a| a.as_array())
                .and_then(|a| a.first())
                .and_then(|a| a.get("id"))
                .and_then(|n| n.as_u64())
                .unwrap_or(0);
            Ok(SongInfo {
                id: v["id"]
                    .as_u64()
                    .or_else(|| info["id"].as_u64())
                    .unwrap_or(0),
                name: info["name"].as_str().unwrap_or("unknown").to_string(),
                singer,
                artist_id,
                album: info
                    .get("al")
                    .or_else(|| info.get("album"))
                    .and_then(|a| a.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                album_id: info
                    .get("al")
                    .or_else(|| info.get("album"))
                    .and_then(|a| a.get("id"))
                    .and_then(|i| i.as_u64())
                    .unwrap_or(0),
                pic_url: info
                    .get("al")
                    .or_else(|| info.get("album"))
                    .and_then(|a| a.get("picUrl"))
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .to_string(),
                duration: info["dt"].as_u64().unwrap_or(0),
                copyright: SongCopyright::Unknown,
            })
        })
        .collect()
}
