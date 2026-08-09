use super::NcmClient;
use crate::{error::NcmError, model::*};
use serde_json::Value;

impl NcmClient {
    // ===== 专辑 =====

    /// 获取专辑详情
    ///
    /// * `album_id` — 专辑 ID
    pub async fn album(&self, album_id: u64) -> Result<AlbumDetail, NcmError> {
        let path = format!("/weapi/v1/album/{}", album_id);
        let result = self.request_weapi(&path, &[]).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_album_detail(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取新碟上架
    ///
    /// * `area` — 区域：`ALL`/`ZH`/`EA`/`KR`/`JP`
    /// * `offset` — 偏移量
    /// * `limit` — 数量
    pub async fn new_albums(
        &self,
        area: &str,
        offset: u16,
        limit: u16,
    ) -> Result<Vec<SongList>, NcmError> {
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("area", area),
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
            ("total", "true"),
        ];
        let result = self.request_weapi("/weapi/album/new", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_list(&value, &["albums"]).map_err(|e| NcmError::parse(e, &value))
    }

    /// 收藏/取消收藏专辑
    ///
    /// * `id` — 专辑 ID
    /// * `like` — `true` 收藏，`false` 取消
    pub async fn album_like(&self, id: u64, like: bool) -> Result<Msg, NcmError> {
        let path = if like {
            "/api/album/sub"
        } else {
            "/api/album/unsub"
        };
        let id_str = id.to_string();
        let params = vec![("id", id_str.as_str())];
        let result = self.request_weapi(path, &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        parse_msg(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取专辑动态信息（收藏/评论数）
    ///
    /// * `id` — 专辑 ID
    pub async fn album_detail_dynamic(&self, id: u64) -> Result<AlbumDetailDynamic, NcmError> {
        let id_str = id.to_string();
        let params = vec![("id", id_str.as_str())];
        let result = self
            .request_weapi("/weapi/album/detail/dynamic", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_album_detail_dynamic(&value).map_err(|e| NcmError::parse(e, &value))
    }
}
