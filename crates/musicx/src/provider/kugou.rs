use crate::crypto::kugou_md5_key;
use crate::error::{MusicError, Result};
use crate::model::{
    Album, Artist, MusicSource, PlayUrlResult, Quality, SearchQuery, SearchResult, Song,
};
use crate::provider::MusicProvider;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

#[derive(Debug)]
pub struct KugouProvider {
    client: Client,
    enable_flac: bool,
}

impl KugouProvider {
    pub fn new(enable_flac: bool) -> Self {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Linux; Android 10) AppleWebKit/537.36")
            .build()
            .expect("Failed to create HTTP client");
        Self {
            client,
            enable_flac,
        }
    }

    fn pick_hash(&self, song: &Value) -> Option<String> {
        if self.enable_flac {
            song.get("sqhash")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    song.get("320hash")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                })
                .or_else(|| {
                    song.get("hash")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                })
                .map(|s| s.to_string())
        } else {
            song.get("320hash")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    song.get("hash")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                })
                .map(|s| s.to_string())
        }
    }

    fn pick_quality(&self, song: &Value) -> Quality {
        if self.enable_flac {
            if song
                .get("sqhash")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .is_some()
            {
                Quality::Lossless
            } else if song
                .get("320hash")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .is_some()
            {
                Quality::High
            } else {
                Quality::Standard
            }
        } else {
            if song
                .get("320hash")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .is_some()
            {
                Quality::High
            } else {
                Quality::Standard
            }
        }
    }

    /// Ordered candidate hashes to try, best quality first (mirrors the
    /// reference `['sqhash', 'hqhash', 'hash']` fallback chain).
    fn candidate_hashes(&self, raw: &Value) -> Vec<(String, Quality)> {
        let get = |key: &str| {
            raw.get(key)
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from)
        };

        let mut candidates = Vec::new();
        if self.enable_flac
            && let Some(h) = get("sqhash")
        {
            candidates.push((h, Quality::Lossless));
        }
        if let Some(h) = get("320hash") {
            candidates.push((h, Quality::High));
        }
        if let Some(h) = get("hash") {
            candidates.push((h, Quality::Standard));
        }
        candidates
    }

    async fn request_track_url(&self, hash: &str, song: &Song) -> Result<PlayUrlResult> {
        let album_id = song
            .raw_data
            .get("album_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let key = kugou_md5_key(hash);

        let url = "http://trackercdn.kugou.com/i/v2/";
        let resp = self
            .client
            .get(url)
            .query(&[
                ("key", key.as_str()),
                ("hash", hash),
                ("appid", "1005"),
                ("pid", "2"),
                ("cmd", "25"),
                ("behavior", "play"),
                ("album_id", album_id),
            ])
            .send()
            .await?;

        let json: Value = resp.json().await?;

        let play_url = json["url"][0]
            .as_str()
            .ok_or(MusicError::NoPlayUrl)?
            .to_string();
        let bitrate = json["bitrate"].as_u64().map(|v| v as u32);
        let size = json["size"].as_u64();

        let final_quality = match bitrate {
            Some(b) if b >= 800 => Quality::Lossless,
            Some(b) if b >= 300 => Quality::High,
            _ => Quality::Standard,
        };

        Ok(PlayUrlResult {
            url: play_url,
            quality: final_quality,
            size,
            bitrate,
        })
    }
}

#[async_trait]
impl MusicProvider for KugouProvider {
    fn source(&self) -> MusicSource {
        MusicSource::Kugou
    }

    async fn search(&self, query: &SearchQuery) -> Result<SearchResult> {
        let url = "http://mobilecdn.kugou.com/api/v3/search/song";
        let resp = self
            .client
            .get(url)
            .query(&[
                ("keyword", query.keyword.as_str()),
                ("page", query.page.unwrap_or(1).to_string().as_str()),
                (
                    "pagesize",
                    query.page_size.unwrap_or(10).to_string().as_str(),
                ),
            ])
            .send()
            .await?;

        let json: Value = resp.json().await?;

        if json["status"].as_i64() != Some(1) {
            return Err(MusicError::Provider {
                provider: "kugou".into(),
                message: json["error_msg"].as_str().unwrap_or("Unknown error").into(),
            });
        }

        let info = json["data"]["info"]
            .as_array()
            .ok_or(MusicError::InvalidResponse("Missing info array".into()))?;

        let songs: Vec<Song> = info
            .iter()
            .filter_map(|item| {
                let hash = self.pick_hash(item)?;
                let name = item["songname"].as_str()?.to_string();
                let singer = item["singername"].as_str().unwrap_or("").to_string();
                let album_id = item["album_id"].as_str().unwrap_or("").to_string();
                let album_name = item["album_name"].as_str().unwrap_or("").to_string();
                let duration = item["duration"].as_u64().unwrap_or(0) * 1000;

                Some(Song {
                    id: hash.clone(),
                    name,
                    artists: vec![Artist {
                        id: "".to_string(),
                        name: singer,
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
                    source: MusicSource::Kugou,
                    quality: Some(self.pick_quality(item)),
                    url: None,
                    raw_data: item.clone(),
                })
            })
            .collect();

        Ok(SearchResult {
            total: json["data"]["total"].as_u64().map(|v| v as u32),
            songs,
            source: MusicSource::Kugou,
            query: query.clone(),
        })
    }

    async fn get_play_url(&self, song: &Song, _quality: Option<Quality>) -> Result<PlayUrlResult> {
        for (hash, _) in self.candidate_hashes(&song.raw_data) {
            if let Ok(result) = self.request_track_url(&hash, song).await {
                return Ok(result);
            }
        }
        Err(MusicError::NoPlayUrl)
    }

    fn priority(&self) -> u8 {
        10
    }
}

#[cfg(test)]
mod tests {
    use super::KugouProvider;
    use crate::model::Quality;
    use serde_json::json;

    fn ensure_crypto() {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });
    }

    #[test]
    fn candidate_hashes_fall_back_in_order() {
        ensure_crypto();
        let raw = json!({
            "sqhash": "sq", "320hash": "hq", "hash": "base",
            "songname": "x", "singername": "y", "album_id": "1",
        });

        let with_flac = KugouProvider::new(true);
        let got = with_flac.candidate_hashes(&raw);
        assert_eq!(
            got,
            vec![
                ("sq".to_string(), Quality::Lossless),
                ("hq".to_string(), Quality::High),
                ("base".to_string(), Quality::Standard),
            ]
        );

        let without_flac = KugouProvider::new(false);
        let got = without_flac.candidate_hashes(&raw);
        assert_eq!(
            got,
            vec![
                ("hq".to_string(), Quality::High),
                ("base".to_string(), Quality::Standard),
            ]
        );
    }

    #[test]
    fn candidate_hashes_skips_missing_hashes() {
        ensure_crypto();
        let raw = json!({"hash": "base"});
        let provider = KugouProvider::new(true);
        let got = provider.candidate_hashes(&raw);
        assert_eq!(got, vec![("base".to_string(), Quality::Standard)]);
    }
}
