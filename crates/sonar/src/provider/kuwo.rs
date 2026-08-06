use crate::error::{SonarError, Result};
use crate::model::{
    SonarSource, PlayUrlResult, Quality, SearchQuery, SearchResult, Song, SongMeta, make_song_id,
};
use crate::provider::SonarProvider;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

#[derive(Debug)]
pub struct KuwoProvider {
    client: Client,
}

impl KuwoProvider {
    pub fn new() -> Self {
        Self::with_proxy("")
    }

    pub fn with_proxy(proxy_url: &str) -> Self {
        let mut builder = Client::builder().user_agent("okhttp/3.10.0");
        if !proxy_url.is_empty() {
            builder = builder.proxy(reqwest::Proxy::all(proxy_url).expect("invalid proxy url"));
        }
        let client = builder.build().expect("Failed to create HTTP client");
        Self { client }
    }
}

impl Default for KuwoProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SonarProvider for KuwoProvider {
    fn source(&self) -> SonarSource {
        SonarSource::Kuwo
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
            .ok_or(SonarError::InvalidResponse("Missing abslist".into()))?;

        let songs: Vec<Song> = abslist
            .iter()
            .filter(|item| item["tpay"].as_str() == Some("0"))
            .filter_map(|item| {
                let music_rid = item["MUSICRID"].as_str()?;
                let id = music_rid.split('_').next_back()?.to_string();
                let name = item["SONGNAME"].as_str()?.to_string();
                let artist_name = item["ARTIST"].as_str().unwrap_or("").to_string();
                let album_name = item["ALBUM"].as_str().unwrap_or("").to_string();
                let duration = item["DURATION"]
                    .as_str()
                    .and_then(|s| s.parse().ok())
                    .or_else(|| item["DURATION"].as_u64())
                    .unwrap_or(0)
                    * 1000;
                let pic_url = item["web_albumpic_short"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(|s| format!("http://img3.kuwo.cn/star/albumcover/{s}"))
                    .unwrap_or_default();

                Some(Song {
                    id: make_song_id(SonarSource::Kuwo, &id),
                    source_id: id,
                    name,
                    singer: artist_name,
                    album: album_name,
                    duration,
                    source: SonarSource::Kuwo,
                    pic_url,
                    meta: SongMeta::default(),
                })
            })
            .collect();

        Ok(SearchResult {
            total: None,
            songs,
            source: SonarSource::Kuwo,
            query: query.clone(),
        })
    }

    async fn get_play_url(&self, song: &Song, _quality: Option<Quality>) -> Result<PlayUrlResult> {
        // The legacy mobi.kuwo.cn/mobi.s endpoint (kwDES + `jiakong_vh` package)
        // now returns a fixed placeholder audio for every request, so use the
        // anti.s endpoint instead, which serves real MP3 URLs for free songs.
        // It only supports mp3 (no flac).
        let url = format!(
            "http://antiserver.kuwo.cn/anti.s?type=convert_url&format=mp3&response=url&rid=MUSIC_{}",
            song.source_id
        );
        let resp = self.client.get(&url).send().await?;
        let body = resp.text().await?;
        let url = body.trim().to_string();

        if url.is_empty() || !url.starts_with("http") {
            return Err(SonarError::NoPlayUrl);
        }
        if is_placeholder_url(&url) {
            return Err(SonarError::NoPlayUrl);
        }

        Ok(PlayUrlResult {
            url,
            quality: Quality::Standard,
            size: None,
            bitrate: None,
        })
    }

    fn priority(&self) -> u8 {
        20
    }

    async fn get_lyrics(&self, song: &Song) -> Result<Option<String>> {
        let url = format!(
            "http://player.kuwo.cn/webmusic/st/getNewMuiseByRid?rid=MUSIC_{}&type=musicname",
            song.source_id
        );
        let resp = self.client.get(&url).send().await?;
        let body = resp.text().await?;

        let lyric = extract_kuwo_lyric(&body).map(|l| l.trim().to_string());
        Ok(lyric.filter(|l| !l.is_empty()))
    }
}

/// Pull the `<lyric>` element out of the kuwo lyrics XML response (it may be
/// wrapped in CDATA).
fn extract_kuwo_lyric(xml: &str) -> Option<String> {
    let start = xml.find("<lyric>")?;
    let inner = &xml[start + "<lyric>".len()..];
    let end = inner.find("</lyric>")?;
    let raw = &inner[..end];
    let text = raw
        .strip_prefix("<![CDATA[")
        .and_then(|rest| rest.strip_suffix("]]>"))
        .unwrap_or(raw);
    (!text.trim().is_empty()).then(|| text.to_string())
}

/// Kuwo serves a short "this resource cannot be played" jingle (a fixed file)
/// instead of a real song when it is unavailable (VIP / copyright). Reject such
/// URLs so the caller can fall back to the next source.
fn is_placeholder_url(url: &str) -> bool {
    url.contains("/1325645003.mp3") || url.contains("/588957081.mp3")
}

#[cfg(test)]
mod tests {
    use super::is_placeholder_url;

    #[test]
    fn detects_placeholder_urls() {
        assert!(is_placeholder_url(
            "http://kw-bj.kuwo.cn/abc/lx/resource/n3/49/43/1325645003.mp3?bitrate$6"
        ));
        assert!(is_placeholder_url(
            "http://nf.sycdn.kuwo.cn/abc/resource/n1/69/32/588957081.mp3"
        ));
    }

    #[test]
    fn accepts_real_urls() {
        assert!(!is_placeholder_url(
            "http://lw.sycdn.kuwo.cn/abc/resource/30106/trackmedia/M500001UJiey3Uv8zq.mp3"
        ));
    }
}
