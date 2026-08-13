use super::NcmClient;
use crate::{error::NcmError, model::*};
use serde_json::Value;

impl NcmClient {
    // ===== Search =====

    /// Search for songs
    ///
    /// * `keyword` — search keyword
    /// * `offset` — offset
    /// * `limit` — count
    pub async fn search_song(
        &self,
        keyword: &str,
        offset: u16,
        limit: u16,
    ) -> Result<SearchResult, NcmError> {
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("s", keyword),
            ("type", "1"),
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
        ];
        let result = self.request_weapi("/weapi/search/get", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;

        let total = value["result"]["songCount"].as_u64().unwrap_or(0) as u32;
        let songs = parse_song_info_array(&value, &["result", "songs"], SongContext::Search)
            .map_err(|e| NcmError::parse(e, &value))?;

        Ok(SearchResult { songs, total })
    }

    /// Search for playlists
    ///
    /// * `keyword` — search keyword
    /// * `offset` — offset
    /// * `limit` — count
    pub async fn search_songlist(
        &self,
        keyword: &str,
        offset: u16,
        limit: u16,
    ) -> Result<Vec<SongList>, NcmError> {
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("s", keyword),
            ("type", "1000"),
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
        ];
        let result = self.request_weapi("/weapi/search/get", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_list(&value, &["result", "playlists"]).map_err(|e| NcmError::parse(e, &value))
    }

    /// Search for artists
    ///
    /// * `keyword` — search keyword
    /// * `offset` — offset
    /// * `limit` — count
    pub async fn search_singer(
        &self,
        keyword: &str,
        offset: u16,
        limit: u16,
    ) -> Result<Vec<SingerInfo>, NcmError> {
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("s", keyword),
            ("type", "100"),
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
        ];
        let result = self.request_weapi("/weapi/search/get", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_singer_info(&value, &["result", "artists"]).map_err(|e| NcmError::parse(e, &value))
    }

    /// Search for albums
    ///
    /// * `keyword` — search keyword
    /// * `offset` — offset
    /// * `limit` — count
    pub async fn search_album(
        &self,
        keyword: &str,
        offset: u16,
        limit: u16,
    ) -> Result<Vec<SongList>, NcmError> {
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("s", keyword),
            ("type", "10"),
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
        ];
        let result = self.request_weapi("/weapi/search/get", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_list(&value, &["result", "albums"]).map_err(|e| NcmError::parse(e, &value))
    }

    /// Get the hot search list
    pub async fn search_hot(&self) -> Result<Vec<HotSearchItem>, NcmError> {
        let result = self.request_weapi("/api/hotsearchlist/get", &[]).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_hot_search(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// Search lyrics
    ///
    /// * `keyword` — search keyword
    /// * `offset` — offset
    /// * `limit` — count
    pub async fn search_lyrics(
        &self,
        keyword: &str,
        offset: u16,
        limit: u16,
    ) -> Result<Vec<SongInfo>, NcmError> {
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("s", keyword),
            ("type", "1006"),
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
        ];
        let result = self.request_weapi("/weapi/search/get", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_info_array(&value, &["result", "songs"], SongContext::Search)
            .map_err(|e| NcmError::parse(e, &value))
    }
}
