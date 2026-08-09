use super::{
    SongContext, SongQuality, first_artist_id, first_artist_name, pic_url_from_id, str_val,
    u64_val, value_get,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// --- 歌曲模型 ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SongInfo {
    pub id: u64,
    pub name: String,
    pub singer: String,
    pub artist_id: u64,
    pub album: String,
    pub album_id: u64,
    pub pic_url: String,
    pub duration: u64,
    pub copyright: SongCopyright,
}

impl PartialEq for SongInfo {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SongCopyright {
    Free,
    VipOnly,
    Payment,
    VipOnlyHighRate,
    Unavailable,
    Unknown,
}

impl SongCopyright {
    pub(crate) fn from_fee(fee: i32) -> Self {
        match fee {
            0 => Self::Free,
            1 => Self::VipOnly,
            4 => Self::Payment,
            8 => Self::VipOnlyHighRate,
            _ => Self::Unknown,
        }
    }

    pub fn from_privilege(v: &Value) -> Self {
        let st = v["st"].as_i64().unwrap_or(0) as i32;
        let fee = v["fee"].as_i64().unwrap_or(-1) as i32;
        if st < 0 {
            Self::Unavailable
        } else {
            Self::from_fee(fee)
        }
    }

    pub fn playable(&self) -> bool {
        self != &Self::Unavailable
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SongUrl {
    pub id: u64,
    pub url: String,
    pub rate: u32,
    pub quality: SongQuality,
    pub free_trial: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lyrics {
    pub lyric: Vec<String>,
    pub tlyric: Vec<String>,
}

// --- 歌曲解析 ---

pub(crate) fn parse_song_info(v: &Value, context: SongContext) -> Result<SongInfo, String> {
    let name = str_val(v, "name");

    let (singer, artist_id) = match context {
        SongContext::Rmd | SongContext::Search => (
            v.get("artists")
                .and_then(|a| a.as_array())
                .and_then(|a| a.first())
                .and_then(|a| a.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
                .to_string(),
            v.get("artists")
                .and_then(|a| a.as_array())
                .and_then(|a| a.first())
                .and_then(|a| a.get("id"))
                .and_then(|n| n.as_u64())
                .unwrap_or(0),
        ),
        _ => (first_artist_name(v), first_artist_id(v)),
    };

    let (album, album_id, pic_url) = match context {
        SongContext::Rmd | SongContext::Search => {
            let album_name = v
                .get("album")
                .and_then(|a| a.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
                .to_string();
            let album_id = v
                .get("album")
                .and_then(|a| a.get("id"))
                .and_then(|i| i.as_u64())
                .unwrap_or(0);
            let pic_url = v
                .get("album")
                .and_then(|a| a.get("picUrl"))
                .and_then(|n| n.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                // Legacy search responses only carry `album.picId` (a numeric
                // id), not `picUrl` — rebuild the cover URL from it.
                .or_else(|| {
                    v.get("album")
                        .and_then(|a| a.get("picId"))
                        .and_then(|n| n.as_u64())
                        .map(pic_url_from_id)
                })
                .unwrap_or_default();
            (album_name, album_id, pic_url)
        }
        SongContext::Singer => {
            let album_name = v
                .get("al")
                .and_then(|a| a.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
                .to_string();
            let album_id = v
                .get("al")
                .and_then(|a| a.get("id"))
                .and_then(|i| i.as_u64())
                .unwrap_or(0);
            (album_name, album_id, String::new())
        }
        _ => {
            let album_name = v
                .get("al")
                .and_then(|a| a.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
                .to_string();
            let album_id = v
                .get("al")
                .and_then(|a| a.get("id"))
                .and_then(|i| i.as_u64())
                .unwrap_or(0);
            let pic_url = v
                .get("al")
                .and_then(|a| a.get("picUrl"))
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            (album_name, album_id, pic_url)
        }
    };

    let duration_field = match context {
        SongContext::Rmd | SongContext::Search => "duration",
        _ => "dt",
    };
    let duration = u64_val(v, duration_field);

    let copyright = v
        .get("privilege")
        .map(|p| {
            let st = p["st"].as_i64().unwrap_or(0) as i32;
            let fee = p["fee"].as_i64().unwrap_or(0) as i32;
            if st < 0 {
                SongCopyright::Unavailable
            } else {
                SongCopyright::from_fee(fee)
            }
        })
        .or_else(|| {
            let fee = v["fee"].as_i64().unwrap_or(-1) as i32;
            if fee >= 0 {
                Some(SongCopyright::from_fee(fee))
            } else {
                None
            }
        })
        .unwrap_or(SongCopyright::Free);

    Ok(SongInfo {
        id: u64_val(v, "id"),
        name,
        singer,
        artist_id,
        album,
        album_id,
        pic_url,
        duration,
        copyright,
    })
}

pub(crate) fn parse_song_info_array(
    value: &Value,
    path: &[&str],
    context: SongContext,
) -> Result<Vec<SongInfo>, String> {
    let array = value_get(value, path)
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("path {:?} not found or not array", path))?;

    array.iter().map(|v| parse_song_info(v, context)).collect()
}

pub(crate) fn parse_lyrics(value: &Value) -> Result<Lyrics, String> {
    let lrc = value["lrc"]["lyric"].as_str().unwrap_or("");
    let tlyric = value["tlyric"]["lyric"].as_str().unwrap_or("");

    let lyric: Vec<String> = lrc
        .split('\n')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    let tlyric: Vec<String> = tlyric
        .split('\n')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    Ok(Lyrics { lyric, tlyric })
}

pub(crate) fn parse_song_url(value: &Value) -> Result<Vec<SongUrl>, String> {
    let array = value["data"].as_array().ok_or("data not found")?;
    Ok(array
        .iter()
        .filter_map(|v| {
            let url = v["url"].as_str()?;
            if url.is_empty() {
                return None;
            }
            let rate = v["br"].as_u64().unwrap_or(0) as u32;
            let quality = v
                .get("level")
                .and_then(|l| l.as_str())
                .and_then(SongQuality::from_level)
                .unwrap_or_else(|| SongQuality::from_rate(rate));
            let free_trial = v.get("freeTrialInfo").and_then(|t| t.as_object()).is_some();
            Some(SongUrl {
                id: v["id"].as_u64().unwrap_or(0),
                url: url.to_string(),
                rate,
                quality,
                free_trial,
            })
        })
        .collect())
}

pub(crate) fn parse_song_id_list(value: &Value) -> Result<Vec<u64>, String> {
    let array = value["ids"].as_array().ok_or("ids not found")?;
    Ok(array.iter().filter_map(|v| v.as_u64()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_song_copyright_from_fee() {
        assert_eq!(SongCopyright::from_fee(0), SongCopyright::Free);
        assert_eq!(SongCopyright::from_fee(1), SongCopyright::VipOnly);
        assert_eq!(SongCopyright::from_fee(4), SongCopyright::Payment);
        assert_eq!(SongCopyright::from_fee(8), SongCopyright::VipOnlyHighRate);
        assert_eq!(SongCopyright::from_fee(99), SongCopyright::Unknown);
    }

    #[test]
    fn test_song_copyright_playable() {
        assert!(SongCopyright::Free.playable());
        assert!(SongCopyright::VipOnly.playable());
        assert!(!SongCopyright::Unavailable.playable());
    }

    #[test]
    fn test_parse_lyrics() {
        let v = json!({
            "lrc": {"lyric": "[00:00.00]line1\n[00:01.00]line2"},
            "tlyric": {"lyric": "[00:00.00]translated1"}
        });
        let lyrics = parse_lyrics(&v).unwrap();
        assert_eq!(lyrics.lyric.len(), 2);
        assert_eq!(lyrics.tlyric.len(), 1);
    }

    #[test]
    fn test_parse_lyrics_empty() {
        let v = json!({});
        let lyrics = parse_lyrics(&v).unwrap();
        assert!(lyrics.lyric.is_empty());
        assert!(lyrics.tlyric.is_empty());
    }

    #[test]
    fn test_parse_song_id_list() {
        let v = json!({"ids": [1, 2, 3]});
        let ids = parse_song_id_list(&v).unwrap();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn test_parse_song_id_list_missing() {
        let v = json!({});
        assert!(parse_song_id_list(&v).is_err());
    }

    #[test]
    fn test_parse_song_url_filters_empty() {
        let v = json!({
            "data": [
                {"id": 1, "url": "http://song.mp3", "br": 320000, "level": "exhigh"},
                {"id": 2, "url": null, "br": 128000},
                {"id": 3, "url": "", "br": 128000},
            ]
        });
        let urls = parse_song_url(&v).unwrap();
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].id, 1);
    }

    #[test]
    fn test_parse_song_info_usl_context() {
        let v = json!({
            "id": 123,
            "name": "Test Song",
            "ar": [{"name": "Artist"}],
            "al": {"name": "Album", "id": 456, "picUrl": "http://pic.png"},
            "dt": 300000,
            "fee": 0
        });
        let song = parse_song_info(&v, SongContext::Usl).unwrap();
        assert_eq!(song.id, 123);
        assert_eq!(song.name, "Test Song");
        assert_eq!(song.singer, "Artist");
        assert_eq!(song.album, "Album");
        assert_eq!(song.album_id, 456);
        assert_eq!(song.duration, 300000);
        assert_eq!(song.copyright, SongCopyright::Free);
    }

    #[test]
    fn test_parse_song_info_search_context() {
        let v = json!({
            "id": 789,
            "name": "Search Song",
            "artists": [{"name": "Singer"}],
            "album": {"name": "Search Album", "id": 101, "picUrl": "http://search.png"},
            "duration": 240000,
            "fee": 1
        });
        let song = parse_song_info(&v, SongContext::Search).unwrap();
        assert_eq!(song.id, 789);
        assert_eq!(song.singer, "Singer");
        assert_eq!(song.album, "Search Album");
        assert_eq!(song.duration, 240000);
        assert_eq!(song.copyright, SongCopyright::VipOnly);
    }

    #[test]
    fn test_parse_song_info_singer_context_empty_pic() {
        let v = json!({
            "id": 111,
            "name": "Singer Song",
            "ar": [{"name": "Singer"}],
            "al": {"name": "Album", "id": 222},
            "dt": 180000
        });
        let song = parse_song_info(&v, SongContext::Singer).unwrap();
        assert_eq!(song.pic_url, "");
    }

    #[test]
    fn test_parse_song_info_privilege() {
        let v = json!({
            "id": 1,
            "name": "Song",
            "privilege": {"st": -1, "fee": 0}
        });
        let song = parse_song_info(&v, SongContext::Usl).unwrap();
        assert_eq!(song.copyright, SongCopyright::Unavailable);
    }
}
