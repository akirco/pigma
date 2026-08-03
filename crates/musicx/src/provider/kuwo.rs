use crate::crypto::{kuwo_build_query, kuwo_des_encrypt};
use crate::error::{MusicError, Result};
use crate::model::{
    Album, Artist, MusicSource, PlayUrlResult, Quality, SearchQuery, SearchResult, Song,
};
use crate::provider::MusicProvider;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

#[derive(Debug)]
pub struct KuwoProvider {
    client: Client,
    enable_flac: bool,
}

impl KuwoProvider {
    pub fn new(enable_flac: bool) -> Self {
        let client = Client::builder()
            .user_agent("okhttp/3.10.0")
            .build()
            .expect("Failed to create HTTP client");
        Self {
            client,
            enable_flac,
        }
    }
}

#[async_trait]
impl MusicProvider for KuwoProvider {
    fn source(&self) -> MusicSource {
        MusicSource::Kuwo
    }

    async fn search(&self, query: &SearchQuery) -> Result<SearchResult> {
        let url = "http://search.kuwo.cn/r.s";
        let resp = self
            .client
            .get(url)
            .query(&[
                ("correct", "1"),
                ("vipver", "1"),
                ("stype", "comprehensive"),
                ("encoding", "utf8"),
                ("rformat", "json"),
                ("mobi", "1"),
                ("show_copyright_off", "1"),
                ("searchapi", "6"),
                ("all", query.keyword.as_str()),
            ])
            .send()
            .await?;

        let json: Value = resp.json().await?;

        let abslist = json["content"][1]["musicpage"]["abslist"]
            .as_array()
            .ok_or(MusicError::InvalidResponse("Missing abslist".into()))?;

        let songs: Vec<Song> = abslist
            .iter()
            .filter_map(|item| {
                let music_rid = item["MUSICRID"].as_str()?;
                let id = music_rid.split('_').next_back()?.to_string();
                let name = item["SONGNAME"].as_str()?.to_string();
                let artist_name = item["ARTIST"].as_str().unwrap_or("").to_string();
                let artist_id = item["ARTISTID"].as_str().unwrap_or("").to_string();
                let album_id = item["ALBUMID"].as_str().unwrap_or("").to_string();
                let album_name = item["ALBUM"].as_str().unwrap_or("").to_string();
                let duration = item["DURATION"].as_u64().unwrap_or(0) * 1000;

                Some(Song {
                    id,
                    name,
                    artists: vec![Artist {
                        id: artist_id,
                        name: artist_name,
                    }],
                    album: if album_id.is_empty() {
                        None
                    } else {
                        Some(Album {
                            id: album_id,
                            name: album_name,
                        })
                    },
                    duration,
                    source: MusicSource::Kuwo,
                    quality: None,
                    url: None,
                    raw_data: item.clone(),
                })
            })
            .collect();

        Ok(SearchResult {
            total: None,
            songs,
            source: MusicSource::Kuwo,
            query: query.clone(),
        })
    }

    async fn get_play_url(&self, song: &Song, _quality: Option<Quality>) -> Result<PlayUrlResult> {
        let rid = &song.id;
        let format = if self.enable_flac { "flac|mp3" } else { "mp3" };

        let query = kuwo_build_query(rid, format);
        let encrypted = kuwo_des_encrypt(&query)?;

        let url = "http://mobi.kuwo.cn/mobi.s";
        let resp = self
            .client
            .get(url)
            .query(&[("f", "kuwo"), ("q", encrypted.as_str())])
            .send()
            .await?;

        let body = resp.text().await?;
        let parsed = parse_mobi_response(&body)
            .ok_or_else(|| MusicError::InvalidResponse("Missing url in mobi.s response".into()))?;

        Ok(PlayUrlResult {
            url: parsed.url,
            quality: parsed.quality,
            size: None,
            bitrate: None,
        })
    }

    fn priority(&self) -> u8 {
        20
    }
}

struct MobiResponse {
    url: String,
    quality: Quality,
}

/// Parse the text/plain `key=value` response returned by `mobi.kuwo.cn/mobi.s`.
///
/// Example body:
/// ```text
/// format=mp3
/// bitrate=6
/// url=http://kw-bj.kuwo.cn/...mp3?bitrate$6&format$mp3&source$...
/// sig=...
/// ```
fn parse_mobi_response(body: &str) -> Option<MobiResponse> {
    let mut url = None;
    let mut format = String::new();
    let mut bitrate = 0u32;

    for line in body.lines() {
        if let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "url" => url = Some(value.trim().to_string()),
                "format" => format = value.trim().to_string(),
                "bitrate" => bitrate = value.trim().parse().unwrap_or(0),
                _ => {}
            }
        }
    }

    let url = url?;
    if url.is_empty() {
        return None;
    }

    let quality = if format.contains("flac") || url.contains(".flac") {
        Quality::Lossless
    } else if bitrate >= 8 {
        Quality::High
    } else {
        Quality::Standard
    };

    Some(MobiResponse { url, quality })
}

#[cfg(test)]
mod tests {
    use super::parse_mobi_response;
    use crate::model::Quality;

    #[test]
    fn parses_mp3_response() {
        let body = "format=mp3\nbitrate=6\nurl=http://kw-bj.kuwo.cn/a.mp3?bitrate$6\nsig=1\nrid=2\ntype=1\n";
        let parsed = parse_mobi_response(body).expect("should parse");
        assert_eq!(parsed.url, "http://kw-bj.kuwo.cn/a.mp3?bitrate$6");
        assert_eq!(parsed.quality, Quality::Standard);
    }

    #[test]
    fn parses_high_quality_response() {
        let body = "format=mp3\nbitrate=8\nurl=http://kw-bj.kuwo.cn/a.mp3\n";
        let parsed = parse_mobi_response(body).expect("should parse");
        assert_eq!(parsed.quality, Quality::High);
    }

    #[test]
    fn parses_flac_response() {
        let body = "format=flac\nbitrate=1\nurl=http://kw-bj.kuwo.cn/a.flac\n";
        let parsed = parse_mobi_response(body).expect("should parse");
        assert_eq!(parsed.quality, Quality::Lossless);
    }

    #[test]
    fn rejects_response_without_url() {
        assert!(parse_mobi_response("format=mp3\nbitrate=6\n").is_none());
    }
}
