use super::song::{SongInfo, parse_song_info_array};
use super::{SongContext, u64_val};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// --- Album models ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumDetail {
    pub id: u64,
    pub name: String,
    pub pic_url: String,
    pub description: String,
    pub publish_time: u64,
    pub artist_id: u64,
    pub artist_name: String,
    pub artist_pic_url: String,
    pub songs: Vec<SongInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AlbumDetailDynamic {
    pub is_sub: bool,
    pub sub_count: u64,
    pub comment_count: u64,
}

// --- Album parsing ---

pub(crate) fn parse_album_detail(value: &Value) -> Result<AlbumDetail, String> {
    let album = &value["album"];
    let unk = "unknown".to_string();
    let album_name = album["name"].as_str().unwrap_or("unknown").to_string();
    let album_id = album["id"].as_u64().unwrap_or(0);
    let pic_url = album["picUrl"].as_str().unwrap_or("").to_string();

    let songs = parse_song_info_array(value, &["songs"], SongContext::Usl)?;

    Ok(AlbumDetail {
        id: album_id,
        name: album_name,
        pic_url,
        description: album["description"].as_str().unwrap_or("").to_string(),
        publish_time: album["publishTime"].as_u64().unwrap_or(0),
        artist_id: album["artist"]["id"].as_u64().unwrap_or(0),
        artist_name: album["artist"]["name"].as_str().unwrap_or(&unk).to_string(),
        artist_pic_url: album["artist"]["picUrl"].as_str().unwrap_or("").to_string(),
        songs,
    })
}

pub(crate) fn parse_album_detail_dynamic(value: &Value) -> Result<AlbumDetailDynamic, String> {
    Ok(AlbumDetailDynamic {
        is_sub: value["isSub"].as_bool().unwrap_or(false),
        sub_count: u64_val(value, "subCount"),
        comment_count: u64_val(value, "commentCount"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_album_detail() {
        let v = json!({
            "album": {
                "id": 200,
                "name": "Test Album",
                "picUrl": "http://album.png",
                "description": "Album desc",
                "publishTime": 1500000,
                "artist": {
                    "id": 300,
                    "name": "Artist",
                    "picUrl": "http://artist.png"
                }
            },
            "songs": [
                {
                    "id": 1,
                    "name": "Song1",
                    "ar": [{"name": "Artist"}],
                    "al": {"name": "Test Album", "id": 200, "picUrl": "http://album.png"},
                    "dt": 250000
                }
            ]
        });
        let detail = parse_album_detail(&v).unwrap();
        assert_eq!(detail.id, 200);
        assert_eq!(detail.artist_name, "Artist");
        assert_eq!(detail.songs.len(), 1);
    }

    #[test]
    fn test_parse_album_detail_dynamic() {
        let v = json!({
            "isSub": false,
            "subCount": 200,
            "commentCount": 30
        });
        let dynamic = parse_album_detail_dynamic(&v).unwrap();
        assert!(!dynamic.is_sub);
        assert_eq!(dynamic.sub_count, 200);
    }
}
