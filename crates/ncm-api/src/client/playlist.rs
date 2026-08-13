use super::NcmClient;
use crate::{error::NcmError, model::*};
use serde_json::Value;

impl NcmClient {
    // ===== Playlist =====

    /// Get playlist details (including the song list)
    ///
    /// A single NetEase Cloud request returns at most 1000 songs; larger playlists are
    /// fetched page by page via `offset`.
    ///
    /// * `id` — playlist ID
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

    /// Get the playlist metadata plus the full `trackIds` (for lazy pagination).
    ///
    /// Unlike `song_list_detail`, this makes only one `/weapi/v6/playlist/detail` request to
    /// get all `trackIds`; the songs themselves are fetched page by page by
    /// [`NcmClient::playlist_songs`] via `songs_detail`, so playlists with >1000 songs are
    /// never fetched all at once (matching the official `playlist_track_all` approach).
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

    /// Slice `trackIds` to fetch this page's songs (via [`NcmClient::songs_detail`]).
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

    /// Get the IDs of my liked songs (without song details, suitable for lightweight sync of large playlists).
    ///
    /// * `uid` — user ID
    pub async fn liked_song_ids(&self, uid: u64) -> Result<Vec<u64>, NcmError> {
        let uid_str = uid.to_string();
        let result = self
            .request_weapi("/api/song/like/get", &[("uid", &uid_str)])
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_id_list(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// Get my liked songs
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

    /// Get a user's playlist list
    ///
    /// * `uid` — user ID
    /// * `offset` — offset
    /// * `limit` — count
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

    /// Get the playlists created by a user
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

    /// Get the playlists collected by a user
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

    /// Get the list of song IDs liked by a user
    ///
    /// * `uid` — user ID
    pub async fn user_song_id_list(&self, uid: u64) -> Result<Vec<u64>, NcmError> {
        let uid_str = uid.to_string();
        let params = vec![("uid", uid_str.as_str())];
        let result = self.request_weapi("/weapi/song/like/get", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_id_list(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// Get the albums collected by a user
    ///
    /// * `offset` — offset
    /// * `limit` — count
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

    /// Like/unlike a playlist
    ///
    /// * `id` — playlist ID
    /// * `like` — `true` to like, `false` to unlike
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

    /// Get playlist dynamic info (play/like/comment counts)
    ///
    /// * `id` — playlist ID
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

    /// Get the daily recommended playlists
    pub async fn recommend_resource(&self) -> Result<Vec<SongList>, NcmError> {
        let result = self
            .request_weapi("/weapi/v1/discovery/recommend/resource", &[])
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_list(&value, &["recommend"]).map_err(|e| NcmError::parse(e, &value))
    }

    /// Get the daily recommended songs
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

    /// Get popular playlists (category browsing)
    ///
    /// * `cat` — category (e.g. `"全部"`, `"华语"`, `"流行"`)
    /// * `order` — sort order: `"hot"` or `"new"`
    /// * `offset` — offset
    /// * `limit` — count
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

    /// Get high-quality playlists
    ///
    /// * `cat` — category (e.g. `"全部"`, `"华语"`, `"流行"`)
    /// * `lasttime` — pagination parameter, the `updateTime` of the last playlist on the previous page
    /// * `limit` — count
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
