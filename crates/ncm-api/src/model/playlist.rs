use super::song::{SongInfo, parse_song_info, parse_song_info_array};
use super::{SongContext, u64_val, value_get};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// --- Playlist models ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SongList {
    pub id: u64,
    pub name: String,
    pub cover_img_url: String,
    pub author: String,
    pub subscribed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayListDetail {
    pub id: u64,
    pub name: String,
    pub cover_img_url: String,
    pub description: String,
    pub create_time: u64,
    pub track_update_time: u64,
    pub songs: Vec<SongInfo>,
    /// All song ids in the playlist (from `playlist.trackIds`), used for lazy pagination slicing.
    pub track_ids: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlayListDetailDynamic {
    pub subscribed: bool,
    pub booked_count: u64,
    pub play_count: u64,
    pub comment_count: u64,
}

// --- Playlist parsing ---

pub(crate) fn parse_song_list(value: &Value, path: &[&str]) -> Result<Vec<SongList>, String> {
    let array = value_get(value, path)
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("path {:?} not found", path))?;

    Ok(array
        .iter()
        .map(|v| SongList {
            id: v["id"].as_u64().unwrap_or(0),
            name: v["name"].as_str().unwrap_or("unknown").to_string(),
            cover_img_url: v
                .get("coverImgUrl")
                .or_else(|| v.get("picUrl"))
                .and_then(|u| u.as_str())
                .unwrap_or("")
                .to_string(),
            author: v
                .get("creator")
                .and_then(|c| c.get("nickname"))
                .or_else(|| v.get("artist").and_then(|a| a.get("name")))
                .or_else(|| v.get("dj").and_then(|d| d.get("nickname")))
                .or_else(|| {
                    v.get("artists")
                        .and_then(|a| a.as_array())
                        .and_then(|a| a.first())
                        .and_then(|a| a.get("name"))
                })
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
                .to_string(),
            subscribed: v["subscribed"].as_bool().unwrap_or(false),
        })
        .collect())
}

pub(crate) fn parse_playlist_detail(value: &Value) -> Result<PlayListDetail, String> {
    let playlist = &value["playlist"];
    let songs = parse_song_info_array(value, &["songs"], SongContext::Usl).or_else(|_| {
        let arr = playlist["tracks"]
            .as_array()
            .ok_or("no tracks in playlist")?;
        arr.iter()
            .map(|v| parse_song_info(v, SongContext::Usl))
            .collect()
    })?;

    Ok(PlayListDetail {
        id: playlist["id"].as_u64().unwrap_or(0),
        name: playlist["name"].as_str().unwrap_or("unknown").to_string(),
        cover_img_url: playlist["coverImgUrl"].as_str().unwrap_or("").to_string(),
        description: playlist["description"].as_str().unwrap_or("").to_string(),
        create_time: playlist["createTime"].as_u64().unwrap_or(0),
        track_update_time: playlist["trackUpdateTime"].as_u64().unwrap_or(0),
        songs,
        track_ids: playlist["trackIds"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v["id"].as_u64()).collect())
            .unwrap_or_default(),
    })
}

pub(crate) fn parse_playlist_detail_dynamic(
    value: &Value,
) -> Result<PlayListDetailDynamic, String> {
    Ok(PlayListDetailDynamic {
        subscribed: value["subscribed"].as_bool().unwrap_or(false),
        booked_count: u64_val(value, "bookedCount"),
        play_count: u64_val(value, "playCount"),
        comment_count: u64_val(value, "commentCount"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_song_list() {
        let v = json!({
            "playlist": [
                {
                    "id": 1,
                    "name": "List1",
                    "coverImgUrl": "http://cover1.png",
                    "creator": {"nickname": "user1"}
                },
                {
                    "id": 2,
                    "name": "List2",
                    "picUrl": "http://cover2.png",
                    "artist": {"name": "artist1"}
                }
            ]
        });
        let lists = parse_song_list(&v, &["playlist"]).unwrap();
        assert_eq!(lists.len(), 2);
        assert_eq!(lists[0].author, "user1");
        assert_eq!(lists[1].cover_img_url, "http://cover2.png");
        assert_eq!(lists[1].author, "artist1");
    }

    #[test]
    fn test_parse_playlist_detail() {
        let v = json!({
            "playlist": {
                "id": 100,
                "name": "My Playlist",
                "coverImgUrl": "http://cover.png",
                "description": "A playlist",
                "createTime": 1000000,
                "trackUpdateTime": 2000000
            },
            "songs": [
                {
                    "id": 1,
                    "name": "Song1",
                    "ar": [{"name": "Artist1"}],
                    "al": {"name": "Album1", "id": 10, "picUrl": "http://pic1.png"},
                    "dt": 300000
                }
            ]
        });
        let detail = parse_playlist_detail(&v).unwrap();
        assert_eq!(detail.id, 100);
        assert_eq!(detail.name, "My Playlist");
        assert_eq!(detail.songs.len(), 1);
    }

    #[test]
    fn test_parse_playlist_detail_dynamic() {
        let v = json!({
            "subscribed": true,
            "bookedCount": 100,
            "playCount": 5000,
            "commentCount": 50
        });
        let dynamic = parse_playlist_detail_dynamic(&v).unwrap();
        assert!(dynamic.subscribed);
        assert_eq!(dynamic.booked_count, 100);
    }
}
