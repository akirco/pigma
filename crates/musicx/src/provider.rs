use crate::error::Result;
use crate::model::{MusicSource, PlayUrlResult, Quality, SearchQuery, SearchResult, Song};
use async_trait::async_trait;

#[async_trait]
pub trait MusicProvider: Send + Sync {
    fn source(&self) -> MusicSource;

    async fn search(&self, query: &SearchQuery) -> Result<SearchResult>;

    async fn get_play_url(&self, song: &Song, quality: Option<Quality>) -> Result<PlayUrlResult>;

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
