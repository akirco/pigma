use super::NcmClient;
use crate::{error::NcmError, model::*};
use serde_json::Value;

impl NcmClient {
    // ===== Song =====

    /// Get song details
    ///
    /// * `ids` — list of song IDs
    pub async fn songs_detail(&self, ids: &[u64]) -> Result<Vec<SongInfo>, NcmError> {
        let c: String = ids
            .iter()
            .map(|id| format!(r#"{{"id":"{}"}}"#, id))
            .collect::<Vec<_>>()
            .join(",");
        let c = format!("[{}]", c);
        let params = vec![("c", c.as_str())];
        let result = self.request_weapi("/weapi/v3/song/detail", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_info_array(&value, &["songs"], SongContext::Usl)
            .map_err(|e| NcmError::parse(e, &value))
    }

    /// Get song playback URLs (based on bitrate)
    ///
    /// * `ids` — list of song IDs
    /// * `br` — bitrate: `128000` / `192000` / `320000` / `999000` / `1900000`
    #[deprecated(note = "use `songs_url_v1` (quality-level based) instead")]
    pub async fn songs_url(&self, ids: &[u64], br: &str) -> Result<Vec<SongUrl>, NcmError> {
        let ids_json = serde_json::to_string(ids).map_err(|e| NcmError::Crypto(e.to_string()))?;
        let params = vec![("ids", ids_json.as_str()), ("br", br)];
        let result = self
            .request_eapi("/api/song/enhance/player/url", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_url(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// Get song playback URLs (based on quality level)
    ///
    /// * `ids` — list of song IDs
    /// * `level` — quality level, see [`SongQuality`]
    pub async fn songs_url_v1(
        &self,
        ids: &[u64],
        level: SongQuality,
    ) -> Result<Vec<SongUrl>, NcmError> {
        let ids_json = serde_json::to_string(ids).map_err(|e| NcmError::Crypto(e.to_string()))?;
        let level_str = level.as_level();
        let encode_type = if level.is_lossy() { "aac" } else { "flac" };
        let mut params = vec![
            ("ids", ids_json.as_str()),
            ("level", level_str),
            ("encodeType", encode_type),
        ];
        if level == SongQuality::AudioVivid {
            params.push(("immerseType", "c51"));
        }
        let result = self
            .request_eapi("/api/song/enhance/player/url/v1", &params)
            .await?;
        let preview_300 = result.chars().take(500).collect::<String>();
        log::debug!("songs_url_v1 raw response (first 500): {:?}", preview_300);
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_url(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// Get lyrics
    ///
    /// * `id` — song ID
    pub async fn song_lyric(&self, id: u64) -> Result<Lyrics, NcmError> {
        let id_str = id.to_string();
        let params = vec![("id", id_str.as_str()), ("lv", "-1"), ("tv", "-1")];
        let result = self.request_weapi("/weapi/song/lyric", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_lyrics(&value).map_err(|e| NcmError::parse(e, &value))
    }
}
