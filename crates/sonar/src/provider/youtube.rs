use crate::error::{Result, SonarError};
use crate::model::{
    PlayUrlResult, Quality, SearchQuery, SearchResult, SonarSource, Song, SongMeta, make_song_id,
};
use crate::provider::SonarProvider;
use crate::util::parse_duration_str;
use async_trait::async_trait;
use y7dl::Client;

/// YouTube audio source backed by `y7dl`.
///
/// Search uses YouTube's public `/results` page; play URLs are resolved
/// through the InnerTube `ANDROID_VR` client (no signing required).
pub struct YoutubeProvider {
    client: Client,
}

impl YoutubeProvider {
    pub fn new() -> Self {
        Self::with_proxy("")
    }

    pub fn with_proxy(proxy_url: &str) -> Self {
        let client = if proxy_url.is_empty() {
            Client::new()
        } else {
            Client::with_proxy(proxy_url)
        };
        Self { client }
    }
}

impl Default for YoutubeProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SonarProvider for YoutubeProvider {
    fn source(&self) -> SonarSource {
        SonarSource::Youtube
    }

    async fn search(&self, query: &SearchQuery) -> Result<SearchResult> {
        let results = self
            .client
            .search(&query.keyword, query.page_size.unwrap_or(30) as usize, None)
            .await
            .map_err(|e| SonarError::Provider {
                provider: "youtube".into(),
                message: e.to_string(),
            })?;

        let songs: Vec<Song> = results
            .into_iter()
            .map(|r| Song {
                id: make_song_id(SonarSource::Youtube, &r.video_id),
                source_id: r.video_id,
                name: r.title,
                singer: r.author,
                album: String::new(),
                duration: parse_duration_str(&r.duration).unwrap_or(0) * 1000,
                source: SonarSource::Youtube,
                pic_url: String::new(),
                meta: SongMeta::default(),
            })
            .collect();

        Ok(SearchResult {
            total: Some(songs.len() as u32),
            songs,
            source: SonarSource::Youtube,
            query: query.clone(),
        })
    }

    async fn get_play_url(&self, song: &Song, _quality: Option<Quality>) -> Result<PlayUrlResult> {
        let video =
            self.client
                .get_video(&song.source_id)
                .await
                .map_err(|e| SonarError::Provider {
                    provider: "youtube".into(),
                    message: e.to_string(),
                })?;

        let format = video
            .audio_formats()
            .into_iter()
            .filter(|f| {
                // Only keep formats rodio/symphonia can decode
                f.mime_type.starts_with("audio/mpeg")
                    || f.mime_type.starts_with("audio/mp4")
                    || f.mime_type.starts_with("audio/ogg")
                    || f.mime_type.starts_with("audio/flac")
                    || f.mime_type.starts_with("audio/wav")
            })
            .max_by_key(|f| f.bitrate.unwrap_or(0))
            .ok_or(SonarError::NoPlayUrl)?;

        let url =
            self.client
                .stream_url(&video, format)
                .await
                .map_err(|e| SonarError::Provider {
                    provider: "youtube".into(),
                    message: e.to_string(),
                })?;

        Ok(PlayUrlResult {
            url,
            quality: Quality::High,
            size: format.content_length(),
            bitrate: format.bitrate.map(|b| b as u32),
        })
    }

    fn priority(&self) -> u8 {
        30
    }
}
