use super::NcmClient;
use crate::{error::NcmError, model::*};
use serde_json::Value;

impl NcmClient {
    // ===== 电台 =====

    /// 获取用户订阅的电台列表
    ///
    /// * `offset` — 偏移量
    /// * `limit` — 数量
    pub async fn user_radio_sublist(
        &self,
        offset: u16,
        limit: u16,
    ) -> Result<Vec<SongList>, NcmError> {
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("total", "true"),
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
        ];
        let result = self
            .request_weapi("/weapi/djradio/get/subed", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_list(&value, &["djRadios"]).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取电台节目列表
    ///
    /// * `rid` — 电台 ID
    /// * `offset` — 偏移量
    /// * `limit` — 数量
    pub async fn radio_program(
        &self,
        rid: u64,
        offset: u16,
        limit: u16,
    ) -> Result<Vec<SongInfo>, NcmError> {
        let id_str = rid.to_string();
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("radioId", id_str.as_str()),
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
            ("asc", "false"),
        ];
        let result = self
            .request_weapi("/weapi/dj/program/byradio", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_radio_programs(&value).map_err(|e| NcmError::parse(e, &value))
    }
}
