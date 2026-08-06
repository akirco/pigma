use crate::crypto::kugou_md5_key;
use crate::error::{SonarError, Result};
use crate::model::{
    SonarSource, PlayUrlResult, Quality, SearchQuery, SearchResult, Song, SongMeta, make_song_id,
};
use crate::provider::SonarProvider;
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
        Self::with_proxy(enable_flac, "")
    }

    pub fn with_proxy(enable_flac: bool, proxy_url: &str) -> Self {
        let mut builder = Client::builder()
            .user_agent("Mozilla/5.0 (Linux; Android 10) AppleWebKit/537.36");
        if !proxy_url.is_empty() {
            builder = builder.proxy(reqwest::Proxy::all(proxy_url).expect("invalid proxy url"));
        }
        let client = builder.build().expect("Failed to create HTTP client");
        Self {
            client,
            enable_flac,
        }
    }

    /// The base `hash` is required for every candidate; higher-quality hashes
    /// live in [`SongMeta`] and are tried first when enabled.
    fn base_hash(&self, song: &Value) -> Option<String> {
        song.get("hash")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    }

    fn build_meta(&self, song: &Value) -> SongMeta {
        SongMeta {
            high_hash: song
                .get("320hash")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
            lossless_hash: song
                .get("sqhash")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
            album_id: song.get("album_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        }
    }

    /// Ordered candidate hashes to try, best quality first (mirrors the
    /// reference `['sqhash', 'hqhash', 'hash']` fallback chain).
    fn candidate_hashes(&self, song: &Song) -> Vec<(String, Quality)> {
        let mut candidates = Vec::new();
        if self.enable_flac
            && let Some(h) = &song.meta.lossless_hash
        {
            candidates.push((h.clone(), Quality::Lossless));
        }
        if let Some(h) = &song.meta.high_hash {
            candidates.push((h.clone(), Quality::High));
        }
        candidates.push((song.source_id.clone(), Quality::Standard));
        candidates
    }

    /// Best hash available for lyrics lookups (does not gate playback).
    fn pick_hash<'a>(&self, song: &'a Song) -> Option<&'a str> {
        if self.enable_flac {
            song.meta
                .lossless_hash
                .as_deref()
                .or(song.meta.high_hash.as_deref())
                .or(Some(song.source_id.as_str()))
        } else {
            song.meta.high_hash.as_deref().or(Some(song.source_id.as_str()))
        }
    }

    async fn request_track_url(&self, hash: &str, song: &Song) -> Result<PlayUrlResult> {
        let key = kugou_md5_key(hash);

        let url = "http://trackercdn.kugou.com/i/v2/";
        let query = [
            ("key", key.as_str()),
            ("hash", hash),
            ("appid", "1005"),
            ("pid", "2"),
            ("cmd", "25"),
            ("behavior", "play"),
        ];
        let mut request = self.client.get(url).query(&query);
        if !song.meta.album_id.is_empty() {
            request = request.query(&[("album_id", &*song.meta.album_id)]);
        }
        let resp = request.send().await?;

        let json: Value = resp.json().await?;

        let play_url = json["url"][0]
            .as_str()
            .ok_or(SonarError::NoPlayUrl)?
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
impl SonarProvider for KugouProvider {
    fn source(&self) -> SonarSource {
        SonarSource::Kugou
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
                    query.page_size.unwrap_or(20).to_string().as_str(),
                ),
            ])
            .send()
            .await?;

        let json: Value = resp.json().await?;

        if json["status"].as_i64() != Some(1) {
            return Err(SonarError::Provider {
                provider: "kugou".into(),
                message: json["error_msg"].as_str().unwrap_or("Unknown error").into(),
            });
        }

        let info = json["data"]["info"]
            .as_array()
            .ok_or(SonarError::InvalidResponse("Missing info array".into()))?;

        let songs: Vec<Song> = info
            .iter()
            .filter_map(|item| {
                let hash = self.base_hash(item)?;
                let name = item["songname"].as_str()?.to_string();
                let singer = item["singername"].as_str().unwrap_or("").to_string();
                let album_name = item["album_name"].as_str().unwrap_or("").to_string();
                let duration = item["duration"].as_u64().unwrap_or(0) * 1000;

                Some(Song {
                    id: make_song_id(SonarSource::Kugou, &hash),
                    source_id: hash,
                    name,
                    singer,
                    album: album_name,
                    duration,
                    source: SonarSource::Kugou,
                    pic_url: String::new(),
                    meta: self.build_meta(item),
                })
            })
            .collect();

        Ok(SearchResult {
            total: json["data"]["total"].as_u64().map(|v| v as u32),
            songs,
            source: SonarSource::Kugou,
            query: query.clone(),
        })
    }

    async fn get_play_url(&self, song: &Song, _quality: Option<Quality>) -> Result<PlayUrlResult> {
        for (hash, _) in self.candidate_hashes(song) {
            if let Ok(result) = self.request_track_url(&hash, song).await {
                return Ok(result);
            }
        }
        Err(SonarError::NoPlayUrl)
    }

    fn priority(&self) -> u8 {
        10
    }

    async fn get_lyrics(&self, song: &Song) -> Result<Option<String>> {
        let hash = match self.pick_hash(song) {
            Some(h) => h,
            None => return Ok(None),
        };
        let keyword = format!("{} {}", song.name, song.singer);

        // Step 1: locate the lyric record.
        let search_url = "http://lyrics.kugou.com/search";
        let resp = self
            .client
            .get(search_url)
            .query(&[
                ("ver", "1"),
                ("man", "yes"),
                ("client", "pc"),
                ("keyword", keyword.as_str()),
                ("hash", hash),
            ])
            .send()
            .await?;
        let json: Value = resp.json().await?;
        let candidate = json["candidates"]
            .as_array()
            .and_then(|arr| arr.first())
            .ok_or(SonarError::NoLyrics)?;
        let id = candidate["id"].as_str().ok_or(SonarError::NoLyrics)?;
        let accesskey = candidate["accesskey"].as_str().ok_or(SonarError::NoLyrics)?;

        // Step 2: download the LRC (base64-encoded in `content`).
        let download_url = "http://lyrics.kugou.com/download";
        let resp = self
            .client
            .get(download_url)
            .query(&[
                ("ver", "1"),
                ("client", "pc"),
                ("id", id),
                ("accesskey", accesskey),
                ("fmt", "lrc"),
            ])
            .send()
            .await?;
        let json: Value = resp.json().await?;
        let content = json["content"].as_str().ok_or(SonarError::NoLyrics)?;
        if content.is_empty() {
            return Ok(None);
        }
        let lrc = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, content)
            .map_err(|_| SonarError::InvalidResponse("kugou lyric base64 decode failed".into()))?;
        Ok(Some(String::from_utf8_lossy(&lrc).into_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::KugouProvider;
    use crate::model::{SonarSource, Quality, Song, SongMeta};
    use serde_json::json;

    fn ensure_crypto() {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });
    }

    fn song_with_meta(meta: SongMeta) -> Song {
        Song {
            id: 0,
            source_id: "base".to_string(),
            name: "x".to_string(),
            singer: "y".to_string(),
            album: String::new(),
            duration: 0,
            source: SonarSource::Kugou,
            pic_url: String::new(),
            meta,
        }
    }

    #[test]
    fn candidate_hashes_fall_back_in_order() {
        ensure_crypto();
        let meta = SongMeta {
            high_hash: Some("hq".to_string()),
            lossless_hash: Some("sq".to_string()),
            album_id: "1".to_string(),
        };
        let song = song_with_meta(meta);

        let with_flac = KugouProvider::new(true);
        let got = with_flac.candidate_hashes(&song);
        assert_eq!(
            got,
            vec![
                ("sq".to_string(), Quality::Lossless),
                ("hq".to_string(), Quality::High),
                ("base".to_string(), Quality::Standard),
            ]
        );

        let without_flac = KugouProvider::new(false);
        let got = without_flac.candidate_hashes(&song);
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
        let song = song_with_meta(SongMeta::default());
        let provider = KugouProvider::new(true);
        let got = provider.candidate_hashes(&song);
        assert_eq!(got, vec![("base".to_string(), Quality::Standard)]);
    }

    #[test]
    fn build_meta_extracts_hashes_and_album() {
        ensure_crypto();
        let raw = json!({
            "hash": "base",
            "320hash": "hq",
            "sqhash": "sq",
            "album_id": "999",
        });
        let provider = KugouProvider::new(true);
        let meta = provider.build_meta(&raw);
        assert_eq!(meta.high_hash.as_deref(), Some("hq"));
        assert_eq!(meta.lossless_hash.as_deref(), Some("sq"));
        assert_eq!(meta.album_id, "999");
    }
}
