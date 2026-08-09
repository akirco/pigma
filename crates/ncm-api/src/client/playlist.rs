use super::NcmClient;
use crate::{error::NcmError, model::*};
use serde_json::Value;

impl NcmClient {
    // ===== 歌单 =====

    /// 获取歌单详情（含歌曲列表）
    ///
    /// 网易云单次请求最多返回 1000 首，超过的歌单按 `offset` 分页拉全。
    ///
    /// * `id` — 歌单 ID
    pub async fn song_list_detail(&self, id: u64) -> Result<PlayListDetail, NcmError> {
        const PAGE: u32 = 1000;
        const MAX_OFFSET: u32 = 200_000;
        let id_str = id.to_string();

        let mut all_songs: Vec<SongInfo> = Vec::new();
        let mut meta: Option<PlayListDetail> = None;
        let mut offset: u32 = 0;

        loop {
            let offset_str = offset.to_string();
            let params = vec![
                ("id", id_str.as_str()),
                ("offset", offset_str.as_str()),
                ("total", if offset == 0 { "true" } else { "false" }),
                ("limit", "1000"),
                ("n", "1000"),
            ];
            let result = self
                .request_weapi("/weapi/v6/playlist/detail", &params)
                .await?;
            let value: Value = serde_json::from_str(&result)?;
            Self::check_api_code(&value)?;

            let page = parse_playlist_detail(&value).map_err(|e| NcmError::parse(e, &value))?;
            let count = page.songs.len();
            if meta.is_none() {
                meta = Some(page.clone());
            }
            all_songs.extend(page.songs);

            if count < PAGE as usize {
                break;
            }
            offset += PAGE;
            if offset > MAX_OFFSET {
                break;
            }
        }

        let mut detail = meta.ok_or_else(|| NcmError::Session("歌单详情为空".into()))?;
        detail.songs = all_songs;
        Ok(detail)
    }

    /// 获取歌单元信息 + 完整 `trackIds`（用于惰性分页）。
    ///
    /// 与 `song_list_detail` 不同，这里只请求一次 `/weapi/v6/playlist/detail` 拿到
    /// `trackIds` 全量 id；歌曲本体由 [`NcmClient::playlist_songs`] 按页切片走
    /// `songs_detail` 获取，因此 >1000 首的歌单也不会一次拉完（对齐官方
    /// `playlist_track_all` 的做法）。
    pub async fn playlist_detail(&self, id: u64) -> Result<(PlayListDetail, Vec<u64>), NcmError> {
        let id_str = id.to_string();
        let params = vec![
            ("id", id_str.as_str()),
            ("offset", "0"),
            ("total", "true"),
            ("limit", "1000"),
            ("n", "1000"),
        ];
        let result = self
            .request_weapi("/weapi/v6/playlist/detail", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        let detail = parse_playlist_detail(&value).map_err(|e| NcmError::parse(e, &value))?;
        let track_ids = detail.track_ids.clone();
        Ok((detail, track_ids))
    }

    /// 按 `trackIds` 切片取本页歌曲（走 [`NcmClient::songs_detail`]）。
    pub async fn playlist_songs(
        &self,
        track_ids: &[u64],
        offset: u32,
        limit: u32,
    ) -> Result<Vec<SongInfo>, NcmError> {
        let offset = (offset as usize).min(track_ids.len());
        let end = (offset + limit as usize).min(track_ids.len());
        if offset >= end {
            return Ok(Vec::new());
        }
        self.songs_detail(&track_ids[offset..end]).await
    }

    /// 获取我喜欢的歌曲
    pub async fn liked_songs(&self, uid: u64) -> Result<Vec<SongInfo>, NcmError> {
        let uid_str = uid.to_string();
        let result = self
            .request_weapi("/api/song/like/get", &[("uid", &uid_str)])
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        let mut ids: Vec<u64> =
            parse_song_id_list(&value).map_err(|e| NcmError::parse(e, &value))?;
        ids.reverse();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.songs_detail(&ids).await
    }

    /// 获取用户歌单列表
    ///
    /// * `uid` — 用户 ID
    /// * `offset` — 偏移量
    /// * `limit` — 数量
    pub async fn user_song_list(
        &self,
        uid: u64,
        offset: u16,
        limit: u16,
    ) -> Result<Vec<SongList>, NcmError> {
        let uid_str = uid.to_string();
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("uid", uid_str.as_str()),
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
        ];
        let result = self.request_weapi("/weapi/user/playlist", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_list(&value, &["playlist"]).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取用户创建的歌单列表
    pub async fn user_created_playlist(
        &self,
        uid: u64,
        offset: u16,
        limit: u16,
    ) -> Result<Vec<SongList>, NcmError> {
        let uid_str = uid.to_string();
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("userId", uid_str.as_str()),
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
            ("isWebview", "true"),
            ("includeRedHeart", "true"),
            ("includeTop", "true"),
        ];
        let result = self
            .request_weapi("/api/user/playlist/create", &params)
            .await?;
        log::debug!(
            "user_created_playlist response: {}",
            &result[..result.len().min(500)]
        );
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_list(&value, &["data", "playlist"]).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取用户收藏的歌单列表
    pub async fn user_collected_playlist(
        &self,
        uid: u64,
        offset: u16,
        limit: u16,
    ) -> Result<Vec<SongList>, NcmError> {
        let uid_str = uid.to_string();
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("userId", uid_str.as_str()),
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
            ("isWebview", "true"),
            ("includeRedHeart", "true"),
            ("includeTop", "true"),
        ];
        let result = self
            .request_weapi("/api/user/playlist/collect", &params)
            .await?;
        log::debug!(
            "user_collected_playlist response: {}",
            &result[..result.len().min(500)]
        );
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_list(&value, &["data", "playlist"]).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取用户喜欢的歌曲 ID 列表
    ///
    /// * `uid` — 用户 ID
    pub async fn user_song_id_list(&self, uid: u64) -> Result<Vec<u64>, NcmError> {
        let uid_str = uid.to_string();
        let params = vec![("uid", uid_str.as_str())];
        let result = self.request_weapi("/weapi/song/like/get", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_id_list(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取用户收藏的专辑列表
    ///
    /// * `offset` — 偏移量
    /// * `limit` — 数量
    pub async fn album_sublist(&self, offset: u16, limit: u16) -> Result<Vec<SongList>, NcmError> {
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("total", "true"),
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
        ];
        let result = self.request_weapi("/weapi/album/sublist", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_list(&value, &["data"]).map_err(|e| NcmError::parse(e, &value))
    }

    /// 收藏/取消收藏歌单
    ///
    /// * `id` — 歌单 ID
    /// * `like` — `true` 收藏，`false` 取消
    pub async fn song_list_like(&self, id: u64, like: bool) -> Result<Msg, NcmError> {
        let path = if like {
            "/weapi/playlist/subscribe"
        } else {
            "/weapi/playlist/unsubscribe"
        };
        let id_str = id.to_string();
        let params = vec![("id", id_str.as_str())];
        let result = self.request_weapi(path, &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        parse_msg(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取歌单动态信息（播放/收藏/评论数）
    ///
    /// * `id` — 歌单 ID
    pub async fn songlist_detail_dynamic(
        &self,
        id: u64,
    ) -> Result<PlayListDetailDynamic, NcmError> {
        let id_str = id.to_string();
        let params = vec![("id", id_str.as_str())];
        let result = self
            .request_weapi("/weapi/playlist/detail/dynamic", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_playlist_detail_dynamic(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取每日推荐歌单
    pub async fn recommend_resource(&self) -> Result<Vec<SongList>, NcmError> {
        let result = self
            .request_weapi("/weapi/v1/discovery/recommend/resource", &[])
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_list(&value, &["recommend"]).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取每日推荐歌曲
    pub async fn recommend_songs(&self) -> Result<Vec<SongInfo>, NcmError> {
        let params = vec![("afresh", "false")];
        let result = self
            .request_weapi("/api/v3/discovery/recommend/songs", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_info_array(&value, &["data", "dailySongs"], SongContext::Rmds)
            .map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取热门歌单（分类浏览）
    ///
    /// * `cat` — 分类（如 `"全部"`、`"华语"`、`"流行"`）
    /// * `order` — 排序：`"hot"` 或 `"new"`
    /// * `offset` — 偏移量
    /// * `limit` — 数量
    pub async fn top_song_list(
        &self,
        cat: &str,
        order: &str,
        offset: u16,
        limit: u16,
    ) -> Result<Vec<SongList>, NcmError> {
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("cat", cat),
            ("order", order),
            ("total", "true"),
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
        ];
        let result = self.request_weapi("/weapi/playlist/list", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_list(&value, &["playlists"]).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取精品歌单
    ///
    /// * `cat` — 分类（如 `"全部"`、`"华语"`、`"流行"`）
    /// * `lasttime` — 分页参数，上一页最后一个歌单的 `updateTime`
    /// * `limit` — 数量
    pub async fn top_song_list_highquality(
        &self,
        cat: &str,
        lasttime: u64,
        limit: u16,
    ) -> Result<Vec<SongList>, NcmError> {
        let lasttime_str = lasttime.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("cat", cat),
            ("total", "true"),
            ("lasttime", lasttime_str.as_str()),
            ("limit", limit_str.as_str()),
        ];
        let result = self
            .request_weapi("/api/playlist/highquality/list", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_list(&value, &["playlists"]).map_err(|e| NcmError::parse(e, &value))
    }
}
