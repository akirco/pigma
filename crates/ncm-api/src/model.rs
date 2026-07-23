use serde::{Deserialize, Serialize};
use serde_json::Value;

// --- 音质 ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SongQuality {
    #[default]
    Standard,
    Higher,
    Extreme,
    Lossless,
    HiRes,
    Surround,
    AudioVivid,
    Master,
}

impl SongQuality {
    pub fn as_level(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Higher => "higher",
            Self::Extreme => "exhigh",
            Self::Lossless => "lossless",
            Self::HiRes => "hires",
            Self::Surround => "jyeffect",
            Self::AudioVivid => "sky",
            Self::Master => "jymaster",
        }
    }

    pub fn from_level(level: &str) -> Option<Self> {
        match level {
            "standard" => Some(Self::Standard),
            "higher" => Some(Self::Higher),
            "exhigh" => Some(Self::Extreme),
            "lossless" => Some(Self::Lossless),
            "hires" => Some(Self::HiRes),
            "jyeffect" => Some(Self::Surround),
            "sky" => Some(Self::AudioVivid),
            "jymaster" => Some(Self::Master),
            _ => None,
        }
    }

    pub fn from_rate(rate: u32) -> Self {
        match rate {
            0..=128000 => Self::Standard,
            128001..=192000 => Self::Higher,
            192001..=320000 => Self::Extreme,
            320001..=999000 => Self::Lossless,
            999001..=1_900_000 => Self::HiRes,
            1_900_001..=2_695_683 => Self::Surround,
            2_695_684..=4_532_510 => Self::AudioVivid,
            _ => Self::Master,
        }
    }

    /// 是否是有损编码（aac）还是无损（flac）
    pub fn is_lossy(self) -> bool {
        matches!(self, Self::Standard | Self::Higher | Self::Extreme)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SongQualityState {
    pub available: Vec<SongQuality>,
    pub selected: Option<SongQuality>,
    pub actual: Option<SongQuality>,
}

// --- 核心数据模型 ---

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
    fn from_fee(fee: i32) -> Self {
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
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingerInfo {
    pub id: u64,
    pub name: String,
    pub pic_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lyrics {
    pub lyric: Vec<String>,
    pub tlyric: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginInfo {
    pub code: i32,
    pub uid: u64,
    pub nickname: String,
    pub avatar_url: String,
    pub vip_type: i32,
    pub msg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Msg {
    pub code: i32,
    pub msg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CloudUploadResult {
    pub song_id: u64,
    pub song_name: String,
    /// 服务端返回的原始合并响应，便于调试 / UI 取私有云字段
    pub raw: serde_json::Value,
}

pub(crate) fn parse_cloud_upload(value: &Value) -> Result<CloudUploadResult, String> {
    let song_id = value["songId"].as_u64().or_else(|| {
        value
            .get("privateCloud")
            .and_then(|p| p.get("songId"))
            .and_then(|v| v.as_u64())
    });
    let song_name = value["songName"]
        .as_str()
        .or_else(|| {
            value
                .get("privateCloud")
                .and_then(|p| p.get("songName"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("")
        .to_string();

    Ok(CloudUploadResult {
        song_id: song_id.unwrap_or(0),
        song_name,
        raw: value.clone(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudDiskResult {
    pub songs: Vec<SongInfo>,
    pub has_more: bool,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopList {
    pub id: u64,
    pub name: String,
    pub update: String,
    pub description: String,
    pub cover: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BannersInfo {
    pub pic: String,
    pub target_id: u64,
    pub target_type: TargetType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub songs: Vec<SongInfo>,
    pub total: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotSearchItem {
    pub keyword: String,
    pub icon_type: i32,
}

pub(crate) fn parse_hot_search(value: &Value) -> Result<Vec<HotSearchItem>, String> {
    let array = value["data"].as_array().ok_or("data not found")?;
    array
        .iter()
        .map(|v| {
            Ok(HotSearchItem {
                keyword: v["searchWord"].as_str().unwrap_or("").to_string(),
                icon_type: v["iconType"].as_i64().unwrap_or(0) as i32,
            })
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlayListDetailDynamic {
    pub subscribed: bool,
    pub booked_count: u64,
    pub play_count: u64,
    pub comment_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AlbumDetailDynamic {
    pub is_sub: bool,
    pub sub_count: u64,
    pub comment_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TargetType {
    Song,
    Album,
    Unknown,
}

impl From<i32> for TargetType {
    fn from(t: i32) -> Self {
        match t {
            1 => Self::Song,
            10 => Self::Album,
            _ => Self::Unknown,
        }
    }
}

// --- 解析上下文（编译期安全，替代字符串） ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SongContext {
    Usl,
    Rmd,
    Rmds,
    Search,
    Singer,
    SingerSongs,
}

// --- JSON 解析 ---

fn str_val(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string()
}

fn u64_val(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
}

fn first_artist_name(v: &Value) -> String {
    v.get("ar")
        .or_else(|| v.get("artists"))
        .and_then(|a| a.as_array())
        .and_then(|a| a.first())
        .and_then(|a| a.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("unknown")
        .to_string()
}

fn first_artist_id(v: &Value) -> u64 {
    v.get("ar")
        .or_else(|| v.get("artists"))
        .and_then(|a| a.as_array())
        .and_then(|a| a.first())
        .and_then(|a| a.get("id"))
        .and_then(|n| n.as_u64())
        .unwrap_or(0)
}

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
                .unwrap_or("")
                .to_string();
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

pub(crate) fn parse_login_info(value: &Value) -> Result<LoginInfo, String> {
    let code = value["code"].as_i64().unwrap_or(0) as i32;
    if code == 200 {
        Ok(LoginInfo {
            code,
            uid: value["profile"]["userId"].as_u64().unwrap_or(0),
            nickname: value["profile"]["nickname"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            avatar_url: value["profile"]["avatarUrl"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            vip_type: value["profile"]["vipType"].as_i64().unwrap_or(0) as i32,
            msg: String::new(),
        })
    } else {
        let msg = value["msg"].as_str().unwrap_or("unknown error").to_string();
        Err(msg)
    }
}

pub(crate) fn parse_msg(value: &Value) -> Result<Msg, String> {
    let code = value["code"].as_i64().unwrap_or(0) as i32;
    let msg = value
        .get("msg")
        .or_else(|| value.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok(Msg { code, msg })
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
            let free_trial = v
                .get("freeTrialInfo")
                .and_then(|t| t.as_object())
                .is_some();
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

pub(crate) fn parse_toplist(value: &Value) -> Result<Vec<TopList>, String> {
    let array = value["list"].as_array().ok_or("list not found")?;
    Ok(array
        .iter()
        .map(|v| TopList {
            id: v["id"].as_u64().unwrap_or(0),
            name: v["name"].as_str().unwrap_or("unknown").to_string(),
            update: v["updateFrequency"].as_str().unwrap_or("").to_string(),
            description: v["description"].as_str().unwrap_or("").to_string(),
            cover: v["coverImgUrl"].as_str().unwrap_or("").to_string(),
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
    })
}

pub(crate) fn parse_unikey(value: &Value) -> Result<String, String> {
    value["unikey"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "unikey not found".to_string())
}

pub(crate) fn parse_banners(value: &Value) -> Result<Vec<BannersInfo>, String> {
    let array = value["banners"].as_array().ok_or("banners not found")?;
    Ok(array
        .iter()
        .map(|v| BannersInfo {
            pic: v["imageUrl"].as_str().unwrap_or("").to_string(),
            target_id: v["targetId"].as_u64().unwrap_or(0),
            target_type: TargetType::from(v["targetType"].as_i64().unwrap_or(0) as i32),
        })
        .collect())
}

pub(crate) fn parse_singer_info(value: &Value, path: &[&str]) -> Result<Vec<SingerInfo>, String> {
    let array = value_get(value, path)
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("path {:?} not found", path))?;

    Ok(array
        .iter()
        .map(|v| SingerInfo {
            id: v["id"].as_u64().unwrap_or(0),
            name: v["name"].as_str().unwrap_or("unknown").to_string(),
            pic_url: {
                let url = v["img1v1Url"].as_str().unwrap_or("").to_string();
                if url.ends_with("5639395138885805.jpg") {
                    String::new()
                } else {
                    url
                }
            },
        })
        .collect())
}

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

pub(crate) fn parse_album_detail_dynamic(value: &Value) -> Result<AlbumDetailDynamic, String> {
    Ok(AlbumDetailDynamic {
        is_sub: value["isSub"].as_bool().unwrap_or(false),
        sub_count: u64_val(value, "subCount"),
        comment_count: u64_val(value, "commentCount"),
    })
}

pub(crate) fn parse_cloud_disk_songs(value: &Value) -> Result<CloudDiskResult, String> {
    let array = value["data"].as_array().ok_or("data not found")?;
    let songs = array
        .iter()
        .map(|v| -> Result<SongInfo, String> {
            let simple = &v["simpleSong"];
            Ok(SongInfo {
                id: v["songId"]
                    .as_u64()
                    .or_else(|| simple["id"].as_u64())
                    .unwrap_or(0),
                name: v["songName"].as_str().unwrap_or("unknown").to_string(),
                singer: v["artist"].as_str().unwrap_or("unknown").to_string(),
                artist_id: simple
                    .get("ar")
                    .and_then(|a| a.as_array())
                    .and_then(|a| a.first())
                    .and_then(|a| a.get("id"))
                    .and_then(|n| n.as_u64())
                    .unwrap_or(0),
                album: v["album"].as_str().unwrap_or("unknown").to_string(),
                album_id: 0,
                pic_url: simple
                    .get("al")
                    .and_then(|a| a.get("picUrl"))
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .to_string(),
                duration: simple["dt"].as_u64().unwrap_or(0),
                copyright: SongCopyright::Unknown,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let has_more = value.get("hasMore").and_then(|v| v.as_bool()).unwrap_or(false);
    let count = value.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
    Ok(CloudDiskResult {
        songs,
        has_more,
        count,
    })
}

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

// --- 工具 ---

fn value_get<'a>(v: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = v;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_song_quality_as_level_and_from_level() {
        for q in [
            SongQuality::Standard,
            SongQuality::Higher,
            SongQuality::Extreme,
            SongQuality::Lossless,
            SongQuality::HiRes,
            SongQuality::Surround,
            SongQuality::AudioVivid,
            SongQuality::Master,
        ] {
            let level = q.as_level();
            assert_eq!(SongQuality::from_level(level), Some(q));
        }
        assert_eq!(SongQuality::from_level("unknown"), None);
    }

    #[test]
    fn test_song_quality_from_rate() {
        assert_eq!(SongQuality::from_rate(0), SongQuality::Standard);
        assert_eq!(SongQuality::from_rate(128000), SongQuality::Standard);
        assert_eq!(SongQuality::from_rate(128001), SongQuality::Higher);
        assert_eq!(SongQuality::from_rate(192000), SongQuality::Higher);
        assert_eq!(SongQuality::from_rate(192001), SongQuality::Extreme);
        assert_eq!(SongQuality::from_rate(320000), SongQuality::Extreme);
        assert_eq!(SongQuality::from_rate(320001), SongQuality::Lossless);
        assert_eq!(SongQuality::from_rate(999000), SongQuality::Lossless);
        assert_eq!(SongQuality::from_rate(999001), SongQuality::HiRes);
    }

    #[test]
    fn test_song_quality_is_lossy() {
        assert!(SongQuality::Standard.is_lossy());
        assert!(SongQuality::Higher.is_lossy());
        assert!(SongQuality::Extreme.is_lossy());
        assert!(!SongQuality::Lossless.is_lossy());
        assert!(!SongQuality::HiRes.is_lossy());
        assert!(!SongQuality::Master.is_lossy());
    }

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
    fn test_parse_msg() {
        let v = json!({"code": 200, "msg": "success"});
        let msg = parse_msg(&v).unwrap();
        assert_eq!(msg.code, 200);
        assert_eq!(msg.msg, "success");

        let v2 = json!({"code": 500, "message": "error occurred"});
        let msg2 = parse_msg(&v2).unwrap();
        assert_eq!(msg2.code, 500);
        assert_eq!(msg2.msg, "error occurred");
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
    fn test_parse_login_info_success() {
        let v = json!({
            "code": 200,
            "profile": {
                "userId": 12345,
                "nickname": "test_user",
                "avatarUrl": "http://avatar.png",
                "vipType": 1
            }
        });
        let info = parse_login_info(&v).unwrap();
        assert_eq!(info.code, 200);
        assert_eq!(info.uid, 12345);
        assert_eq!(info.nickname, "test_user");
        assert_eq!(info.avatar_url, "http://avatar.png");
        assert_eq!(info.vip_type, 1);
    }

    #[test]
    fn test_parse_login_info_failure() {
        let v = json!({
            "code": 400,
            "msg": "login failed"
        });
        let err = parse_login_info(&v).unwrap_err();
        assert_eq!(err, "login failed");
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
    fn test_parse_toplist() {
        let v = json!({
            "list": [
                {
                    "id": 19723756,
                    "name": "飙升榜",
                    "updateFrequency": "每天更新",
                    "description": "desc",
                    "coverImgUrl": "http://cover.png"
                }
            ]
        });
        let lists = parse_toplist(&v).unwrap();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].id, 19723756);
        assert_eq!(lists[0].name, "飙升榜");
    }

    #[test]
    fn test_parse_banners() {
        let v = json!({
            "banners": [
                {"imageUrl": "http://banner.png", "targetId": 100, "targetType": 1},
                {"imageUrl": "http://banner2.png", "targetId": 200, "targetType": 10}
            ]
        });
        let banners = parse_banners(&v).unwrap();
        assert_eq!(banners.len(), 2);
        assert_eq!(banners[0].target_type, TargetType::Song);
        assert_eq!(banners[1].target_type, TargetType::Album);
    }

    #[test]
    fn test_target_type_from() {
        assert_eq!(TargetType::from(1), TargetType::Song);
        assert_eq!(TargetType::from(10), TargetType::Album);
        assert_eq!(TargetType::from(99), TargetType::Unknown);
    }

    #[test]
    fn test_parse_hot_search() {
        let v = json!({
            "data": [
                {"searchWord": "keyword1", "iconType": 1},
                {"searchWord": "keyword2", "iconType": 0}
            ]
        });
        let items = parse_hot_search(&v).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].keyword, "keyword1");
    }

    #[test]
    fn test_parse_unikey() {
        let v = json!({"unikey": "abc123"});
        assert_eq!(parse_unikey(&v).unwrap(), "abc123");

        let v2 = json!({});
        assert!(parse_unikey(&v2).is_err());
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
    fn test_parse_song_info_privilege() {
        let v = json!({
            "id": 1,
            "name": "Song",
            "privilege": {"st": -1, "fee": 0}
        });
        let song = parse_song_info(&v, SongContext::Usl).unwrap();
        assert_eq!(song.copyright, SongCopyright::Unavailable);
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
