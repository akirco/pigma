use super::NcmClient;
use crate::{error::NcmError, model::*};
use serde_json::Value;

impl NcmClient {
    // ===== Interaction =====

    /// Like/unlike a song
    ///
    /// * `song_id` — song ID
    /// * `like` — `true` to like, `false` to unlike
    pub async fn like(&self, song_id: u64, like: bool) -> Result<Msg, NcmError> {
        let id_str = song_id.to_string();
        let like_str = if like { "true" } else { "false" };
        let params = vec![
            ("alg", "itembased"),
            ("trackId", id_str.as_str()),
            ("like", like_str),
            ("time", "3"),
        ];
        let result = self.request_weapi("/api/radio/like", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        parse_msg(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// FM trash (dislike the current FM song)
    ///
    /// * `song_id` — song ID
    pub async fn fm_trash(&self, song_id: u64) -> Result<Msg, NcmError> {
        let id_str = song_id.to_string();
        let params = vec![("alg", "RT"), ("songId", id_str.as_str()), ("time", "25")];
        let result = self
            .request_weapi("/weapi/radio/trash/add", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        parse_msg(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// Mark a daily recommended song as not interested
    ///
    /// * `song_id` — song ID
    pub async fn recommend_song_dislike(&self, song_id: u64) -> Result<SongInfo, NcmError> {
        let id_str = song_id.to_string();
        let params = vec![
            ("resId", id_str.as_str()),
            ("resType", "4"),
            ("sceneType", "1"),
        ];
        let result = self
            .request_weapi("/api/v2/discovery/recommend/dislike", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        let data = value
            .get("data")
            .ok_or_else(|| NcmError::parse("missing data", &value))?;
        parse_song_info(data, SongContext::Rmds).map_err(|e| NcmError::parse(e, &value))
    }

    /// Daily sign-in
    ///
    /// * `type` — `0` (PC) or `1` (mobile)
    pub async fn daily_task(&self, r#type: &str) -> Result<Msg, NcmError> {
        let params = vec![("type", r#type)];
        let result = self
            .request_weapi("/weapi/point/dailyTask", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        parse_msg(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// Listening check-in — report a song play record
    ///
    /// * `song_id` — song ID
    /// * `time_ms` — play duration (milliseconds)
    /// * `source_id` — source playlist ID (optional)
    pub async fn report_play(
        &self,
        song_id: u64,
        time_ms: u64,
        source_id: Option<u64>,
    ) -> Result<(), NcmError> {
        let log = serde_json::json!([{
            "action": "play",
            "json": {
                "id": song_id,
                "sourceId": source_id.unwrap_or(0),
                "time": time_ms,
                "type": "song",
                "end": "playend",
                "download": 0,
                "wifi": 0,
                "source": "list",
                "mainsite": 1,
                "content": "",
            }
        }]);
        let logs_str = log.to_string();
        let params = vec![("logs", logs_str.as_str())];
        let _ = self.request_weapi("/api/feedback/weblog", &params).await?;
        Ok(())
    }
}
