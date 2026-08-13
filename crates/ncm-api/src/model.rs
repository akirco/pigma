use serde::{Deserialize, Serialize};
use serde_json::Value;

mod album;
mod discovery;
mod playlist;
mod radio;
mod singer;
mod song;
mod user;

pub use album::{AlbumDetail, AlbumDetailDynamic};
pub use discovery::{BannersInfo, HotSearchItem, SearchResult, TargetType, TopList};
pub use playlist::{PlayListDetail, PlayListDetailDynamic, SongList};
pub use singer::SingerInfo;
pub use song::{Lyrics, SongCopyright, SongInfo, SongUrl};
pub use user::{CloudDiskResult, CloudUploadResult, LoginInfo, Msg};

pub(crate) use album::{parse_album_detail, parse_album_detail_dynamic};
pub(crate) use discovery::{parse_banners, parse_hot_search, parse_toplist};
pub(crate) use playlist::{parse_playlist_detail, parse_playlist_detail_dynamic, parse_song_list};
pub(crate) use radio::{parse_intelligence_songs, parse_radio_programs};
pub(crate) use singer::parse_singer_info;
pub(crate) use song::{
    parse_lyrics, parse_song_id_list, parse_song_info, parse_song_info_array, parse_song_url,
};
pub(crate) use user::{
    parse_cloud_disk_songs, parse_cloud_upload, parse_login_info, parse_msg, parse_unikey,
};

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

    /// Whether it is lossy encoding (aac) or lossless (flac)
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

// --- Parse context (compile-time safe, replacing string hints) ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SongContext {
    Usl,
    Rmd,
    Rmds,
    Search,
    Singer,
    SingerSongs,
}

// --- Shared parsing utilities ---

pub(crate) fn str_val(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string()
}

pub(crate) fn u64_val(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
}

pub(crate) fn first_artist_name(v: &Value) -> String {
    v.get("ar")
        .or_else(|| v.get("artists"))
        .and_then(|a| a.as_array())
        .and_then(|a| a.first())
        .and_then(|a| a.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("unknown")
        .to_string()
}

pub(crate) fn first_artist_id(v: &Value) -> u64 {
    v.get("ar")
        .or_else(|| v.get("artists"))
        .and_then(|a| a.as_array())
        .and_then(|a| a.first())
        .and_then(|a| a.get("id"))
        .and_then(|n| n.as_u64())
        .unwrap_or(0)
}

/// Build an NCM image URL from a numeric `picId`, used when a response carries
/// only the id (e.g. legacy `/weapi/search/get` results have `album.picId`
/// instead of `album.picUrl`). Mirrors the official client: XOR the id digits
/// with `3go8&$8*3*3h0k(2)2`, md5 the bytes, then url-safe base64 —
/// `https://p1.music.126.net/<hash>/<picId>.jpg`.
pub(crate) fn pic_url_from_id(pic_id: u64) -> String {
    use base64::Engine;
    use md5::Digest;
    const KEY: &[u8] = b"3go8&$8*3*3h0k(2)2";
    let digits = pic_id.to_string();
    let xored: Vec<u8> = digits
        .bytes()
        .enumerate()
        .map(|(i, c)| c ^ KEY[i % KEY.len()])
        .collect();
    let digest = md5::Md5::digest(&xored);
    let hash = base64::engine::general_purpose::URL_SAFE.encode(digest);
    format!("https://p1.music.126.net/{hash}/{pic_id}.jpg")
}

// --- Utilities ---

pub(crate) fn value_get<'a>(v: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = v;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_pic_url_from_id() {
        // Known pair from a real hot-search icon picId.
        assert_eq!(
            pic_url_from_id(109951163967994693),
            "https://p1.music.126.net/IBKnY_RCYTUAALcqWhAT6g==/109951163967994693.jpg"
        );
        assert_eq!(
            pic_url_from_id(109951171458803146),
            "https://p1.music.126.net/kBeIsIs1LuDB5aoj8PWdxw==/109951171458803146.jpg"
        );
    }
}
