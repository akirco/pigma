use crate::crypto::{invalidate_wbi_keys, wbi_sign};
use crate::error::{Result, SonarError};
use crate::model::{
    PlayUrlResult, Quality, SearchQuery, SearchResult, SonarSource, Song, SongMeta, make_song_id,
};
use crate::provider::SonarProvider;
use crate::provider::{PRIORITY_BILIVIDEO, build_client};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::{
    Client,
    header::{COOKIE, HeaderMap, HeaderValue, REFERER, SET_COOKIE},
};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

static HTML_TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]+>").expect("valid regex"));

fn clean_title(title: &str) -> String {
    HTML_TAG_RE.replace_all(title, "").into_owned()
}

/// Bilibili video provider. Searches via the WBI-signed web search API and
/// extracts the audio track from the DASH manifest of a video.
#[derive(Debug)]
pub struct BiliVideoProvider {
    client: Client,
    cookies: Arc<Mutex<String>>,
}

impl BiliVideoProvider {
    /// Build a provider with no proxy (cookies are fetched lazily on first search).
    pub fn new() -> Result<Self> {
        Self::with_proxy("")
    }

    /// Build a provider, routing requests through `proxy_url` (empty = direct).
    pub fn with_proxy(proxy_url: &str) -> Result<Self> {
        let client = build_client(
            proxy_url,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        )?;
        Ok(Self {
            client,
            cookies: Arc::new(Mutex::new(String::new())),
        })
    }

    async fn fetch_cookies(&self) -> Result<()> {
        let resp = self.client.get("https://www.bilibili.com").send().await?;
        let cookies: Vec<String> = resp
            .headers()
            .get_all(SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .filter_map(|value| value.split(';').next())
            .filter(|c| !c.trim().is_empty())
            .map(|c| c.trim().to_string())
            .collect();
        if !cookies.is_empty() {
            let mut lock = self.cookies.lock().await;
            *lock = cookies.join("; ");
        }
        Ok(())
    }

    async fn signed_request(&self, path: &str, params: Vec<(String, String)>) -> Result<Value> {
        // `params` is cloned for signing so the original (without the injected
        // `wts`) survives across retries.
        for attempt in 0..2u32 {
            let query = wbi_sign(&self.client, &mut params.clone()).await?;
            let url = format!("https://api.bilibili.com{}?{}", path, query);

            let mut headers = HeaderMap::new();
            let cookies = self.cookies.lock().await;
            if !cookies.is_empty() {
                headers.insert(COOKIE, HeaderValue::from_str(&cookies)?);
            }
            headers.insert(
                REFERER,
                HeaderValue::from_str("https://search.bilibili.com")?,
            );

            let resp = self.client.get(&url).headers(headers).send().await?;
            let json: Value = resp.json().await?;

            let code = json["code"].as_i64().unwrap_or(-999);
            if code == 0 {
                return Ok(json);
            }

            let message = json["message"]
                .as_str()
                .unwrap_or("Unknown error")
                .to_string();

            // `-403` (stale/missing WBI signature) and `-412` (risk control)
            // are often transient: invalidate the cached keys, refresh cookies,
            // back off briefly and re-sign once.
            if attempt == 0 && matches!(code, -403 | -412) {
                log::warn!(
                    "bilivideo {} rejected (code={code}, {message}); refreshing keys/cookies and retrying",
                    path
                );
                invalidate_wbi_keys();
                self.fetch_cookies().await?;
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                continue;
            }

            return Err(SonarError::Provider {
                provider: "bilivideo".into(),
                message,
            });
        }
        unreachable!("loop always returns within two iterations")
    }
}

impl Default for BiliVideoProvider {
    fn default() -> Self {
        Self::new().expect("direct bilibili client")
    }
}

#[async_trait]
impl SonarProvider for BiliVideoProvider {
    fn source(&self) -> SonarSource {
        SonarSource::BiliVideo
    }

    async fn search(&self, query: &SearchQuery) -> Result<SearchResult> {
        let cookies = self.cookies.lock().await;
        if cookies.is_empty() {
            drop(cookies);
            self.fetch_cookies().await?;
        }

        let json = self
            .signed_request(
                "/x/web-interface/wbi/search/type",
                vec![
                    ("search_type".to_string(), "video".to_string()),
                    ("keyword".to_string(), query.keyword.clone()),
                    (
                        "page_size".to_string(),
                        query.page_size.unwrap_or(20).to_string(),
                    ),
                ],
            )
            .await?;

        let results = json["data"]["result"]
            .as_array()
            .ok_or(SonarError::InvalidResponse("Missing result array".into()))?;

        let songs: Vec<Song> = results
            .iter()
            .filter_map(|item| {
                let bvid = item["bvid"].as_str()?.to_string();
                let title = clean_title(item["title"].as_str().unwrap_or(""));
                let author = item["author"].as_str().unwrap_or("").to_string();
                let typename = item["typename"].as_str().unwrap_or("").to_string();

                let duration = item["duration"]
                    .as_str()
                    .and_then(crate::util::parse_duration_str)
                    .unwrap_or(0)
                    * 1000;
                let pic_url = item["pic"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(|s| {
                        if s.starts_with("//") {
                            format!("https:{s}")
                        } else {
                            s.to_string()
                        }
                    })
                    .unwrap_or_default();

                Some(Song {
                    id: make_song_id(SonarSource::BiliVideo, &bvid),
                    source_id: bvid,
                    name: title,
                    singer: author,
                    album: typename,
                    duration,
                    source: SonarSource::BiliVideo,
                    pic_url,
                    meta: SongMeta::default(),
                })
            })
            .collect();

        Ok(SearchResult {
            total: None,
            songs,
            source: SonarSource::BiliVideo,
            query: query.clone(),
        })
    }

    async fn get_play_url(&self, song: &Song, _quality: Option<Quality>) -> Result<PlayUrlResult> {
        let bvid = &song.source_id;

        let view_json = self
            .signed_request(
                "/x/web-interface/wbi/view",
                vec![("bvid".to_string(), bvid.clone())],
            )
            .await?;

        let cid = view_json["data"]["cid"]
            .as_u64()
            .ok_or(SonarError::InvalidResponse("Missing cid".into()))?;

        let play_json = self
            .signed_request(
                "/x/player/wbi/playurl",
                vec![
                    ("bvid".to_string(), bvid.clone()),
                    ("cid".to_string(), cid.to_string()),
                    ("fnval".to_string(), "16".to_string()),
                    ("platform".to_string(), "pc".to_string()),
                ],
            )
            .await?;

        let audio_url = play_json["data"]["dash"]["audio"][0]["base_url"]
            .as_str()
            .ok_or(SonarError::NoPlayUrl)?;

        Ok(PlayUrlResult {
            url: audio_url.to_string(),
            quality: Quality::High,
            size: None,
            bitrate: None,
        })
    }

    fn priority(&self) -> u8 {
        PRIORITY_BILIVIDEO
    }
}
