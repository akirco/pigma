use crate::error::Result;
use crate::model::{PlayUrlResult, Quality, SearchQuery, SearchResult, SonarSource, Song};
use async_trait::async_trait;

#[async_trait]
pub trait SonarProvider: Send + Sync {
    fn source(&self) -> SonarSource;

    async fn search(&self, query: &SearchQuery) -> Result<SearchResult>;

    async fn get_play_url(&self, song: &Song, quality: Option<Quality>) -> Result<PlayUrlResult>;

    /// Fetch LRC lyrics for a song, if the provider offers them. Returns
    /// `Ok(None)` when no lyrics are available.
    async fn get_lyrics(&self, song: &Song) -> Result<Option<String>> {
        let _ = song;
        Ok(None)
    }

    fn enabled(&self) -> bool {
        true
    }

    fn priority(&self) -> u8 {
        0
    }
}

pub mod bilivideo;
pub mod kugou;
pub mod kuwo;
pub mod youtube;
