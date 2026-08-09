use super::NcmClient;
use crate::{error::NcmError, model::*};
use serde_json::Value;

impl NcmClient {
    // ===== 歌手 =====

    /// 获取歌手热门歌曲
    ///
    /// * `id` — 歌手 ID
    pub async fn singer_songs(&self, id: u64) -> Result<Vec<SongInfo>, NcmError> {
        let path = format!("/weapi/v1/artist/{}", id);
        let result = self.request_weapi(&path, &[]).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_info_array(&value, &["hotSongs"], SongContext::Singer)
            .map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取歌手全部歌曲
    ///
    /// * `id` — 歌手 ID
    /// * `order` — `"hot"`（热门）或 `"time"`（时间）
    /// * `offset` — 偏移量
    /// * `limit` — 数量
    pub async fn singer_all_songs(
        &self,
        id: u64,
        order: &str,
        offset: u16,
        limit: u16,
    ) -> Result<Vec<SongInfo>, NcmError> {
        let id_str = id.to_string();
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("id", id_str.as_str()),
            ("private_cloud", "true"),
            ("work_type", "1"),
            ("order", order),
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
        ];
        let result = self
            .request_weapi("/weapi/v1/artist/songs", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_info_array(&value, &["songs"], SongContext::SingerSongs)
            .map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取热门歌手
    pub async fn top_artists(&self, offset: u16, limit: u16) -> Result<Vec<SingerInfo>, NcmError> {
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
            ("total", "true"),
        ];
        let result = self.request_weapi("/api/artist/top", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_singer_info(&value, &["artists"]).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取歌手榜（排行榜）
    ///
    /// * `r#type` — 榜单类型（1-华语, 2-欧美, 3-韩国, 4-日本）
    pub async fn toplist_artist(&self, r#type: u8) -> Result<Vec<SingerInfo>, NcmError> {
        let limit_str = 100u16.to_string();
        let offset_str = 0u16.to_string();
        let type_str = r#type.to_string();
        let params = vec![
            ("type", type_str.as_str()),
            ("limit", limit_str.as_str()),
            ("offset", offset_str.as_str()),
            ("total", "true"),
        ];
        let result = self.request_weapi("/api/toplist/artist", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        // Response: { code: 200, list: { artists: [...] } }
        let list = value
            .get("list")
            .ok_or_else(|| NcmError::parse(String::from("list not found"), &value))?;
        parse_singer_info(list, &["artists"]).map_err(|e| NcmError::parse(e, &value))
    }
}
