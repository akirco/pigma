use super::NcmClient;
use crate::{error::NcmError, model::*};
use serde_json::Value;

impl NcmClient {
    // ===== Home / Recommendations / Toplists =====

    /// Get personal FM songs
    pub async fn personal_fm(&self) -> Result<Vec<SongInfo>, NcmError> {
        let result = self.request_weapi("/weapi/v1/radio/get", &[]).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_info_array(&value, &["data"], SongContext::Rmd)
            .map_err(|e| NcmError::parse(e, &value))
    }

    /// Get the list of all toplists
    pub async fn toplist(&self) -> Result<Vec<TopList>, NcmError> {
        let result = self.request_weapi("/api/toplist", &[]).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_toplist(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// Get toplist songs (same as `song_list_detail`)
    ///
    /// * `list_id` — toplist ID (e.g. the Cloud Music Top Chart `19723756`)
    pub async fn top_songs(&self, list_id: u64) -> Result<PlayListDetail, NcmError> {
        self.song_list_detail(list_id).await
    }

    /// Get the home page carousel banners
    pub async fn banners(&self) -> Result<Vec<BannersInfo>, NcmError> {
        let params = vec![("clientType", "pc")];
        let result = self.request_weapi("/weapi/v2/banner/get", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_banners(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// Get the app home page section info (returns raw JSON)
    pub async fn homepage(&self) -> Result<String, NcmError> {
        let params = vec![("refresh", "false"), ("cursor", "null")];
        self.request_weapi("/api/homepage/block/page", &params)
            .await
    }

    /// Heartbeat mode / smart playback
    ///
    /// * `song_id` — ID of the currently playing song
    /// * `playlist_id` — playlist ID
    pub async fn playmode_intelligence_list(
        &self,
        song_id: u64,
        playlist_id: u64,
    ) -> Result<Vec<SongInfo>, NcmError> {
        let sid_str = song_id.to_string();
        let pid_str = playlist_id.to_string();
        let params = vec![
            ("songId", sid_str.as_str()),
            ("type", "fromPlayOne"),
            ("playlistId", pid_str.as_str()),
            ("startMusicId", sid_str.as_str()),
            ("count", "1"),
        ];
        let result = self
            .request_weapi("/weapi/playmode/intelligence/list", &params)
            .await?;
        log::debug!(
            "intelligence list response: song_id={}, playlist_id={}, response={}",
            song_id,
            playlist_id,
            &result[..result.len().min(2000)]
        );
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        let songs = parse_intelligence_songs(&value).map_err(|e| NcmError::parse(e, &value))?;
        log::debug!("intelligence list parsed: {} songs", songs.len());
        Ok(songs)
    }
}
