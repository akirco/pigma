use super::NcmClient;
use crate::{error::NcmError, model::*};
use serde_json::Value;

impl NcmClient {
    // ===== 云盘 =====

    /// 获取用户最近播放歌曲
    pub async fn recent_songs(&self, limit: u16) -> Result<Vec<SongInfo>, NcmError> {
        let limit_str = limit.to_string();
        let result = self
            .request_weapi("/api/play-record/song/list", &[("limit", &limit_str)])
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        let array = value["data"]["list"]
            .as_array()
            .ok_or_else(|| NcmError::parse(String::from("list not found"), &value))?;
        let mut songs = Vec::new();
        for v in array {
            let song_data = &v["data"];
            if !song_data.is_null() {
                songs.push(
                    parse_song_info(song_data, SongContext::Usl)
                        .map_err(|e| NcmError::parse(e, &value))?,
                );
            }
        }
        Ok(songs)
    }

    /// 获取用户云盘歌曲
    pub async fn user_cloud_disk(
        &self,
        offset: u32,
        limit: u32,
    ) -> Result<CloudDiskResult, NcmError> {
        let offset_s = offset.to_string();
        let limit_s = limit.to_string();
        let params = vec![("offset", offset_s.as_str()), ("limit", limit_s.as_str())];
        let result = self.request_weapi("/weapi/v1/cloud/get", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_cloud_disk_songs(&value).map_err(|e| NcmError::parse(e, &value))
    }

    // ===== 云盘上传 =====

    /// 上传本地音频文件到网易云音乐云盘（完整流程，对齐 NeteaseCloudMusicApi/cloud.js）
    ///
    /// 1. `/api/cloud/upload/check` 检查是否需要上传
    /// 2. 解析音频标签拿到歌名/歌手/专辑
    /// 3. `/api/nos/token/alloc` 申请 NOS 上传凭证
    /// 4. 若 `needUpload`，把原始字节上传到 NOS（`wanproxy.127.net`）
    /// 5. `/api/upload/cloud/info/v2` 提交元数据
    /// 6. `/api/cloud/pub/v2` 发布到云盘
    pub async fn upload_song(&self, path: &std::path::Path) -> Result<CloudUploadResult, NcmError> {
        self.upload_song_with_meta(path, "", "", "").await
    }

    /// 同上，但允许传入元数据 hint（从缓存索引获取的 name/singer/album），标签解析失败时作为 fallback。
    pub async fn upload_song_with_meta(
        &self,
        path: &std::path::Path,
        song_hint: &str,
        album_hint: &str,
        artist_hint: &str,
    ) -> Result<CloudUploadResult, NcmError> {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| NcmError::Session("invalid file path".into()))?
            .to_string();

        let ext = file_name
            .rsplit('.')
            .next()
            .filter(|_| file_name.contains('.'))
            .unwrap_or("mp3")
            .to_string();
        let mime = match ext.as_str() {
            "flac" => "audio/flac",
            "wav" => "audio/wav",
            "m4a" => "audio/mp4",
            "ogg" => "audio/ogg",
            _ => "audio/mpeg",
        };

        // 流式计算 MD5 与文件大小（不把整文件读入内存）
        let (md5, size) = stream_file_digest(path)?;
        let bitrate = 999000u32;

        // 步骤 1：上传前检查（参数对齐参考实现，保留 JSON 类型）
        let check = self
            .request_eapi_value(
                "/api/cloud/upload/check",
                serde_json::json!({
                    "bitrate": bitrate,
                    "ext": "",
                    "length": size,
                    "md5": md5,
                    "songId": "0",
                    "version": 1,
                }),
            )
            .await?;
        let check_value: Value = serde_json::from_str(&check)?;
        Self::check_api_code(&check_value)?;
        let need_upload = check_value
            .get("needUpload")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let check_song_id: String = check_value
            .get("songId")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .or_else(|| {
                check_value
                    .get("songId")
                    .and_then(|v| v.as_u64().map(|n| n.to_string()))
            })
            .unwrap_or_else(|| "0".to_string());

        // 步骤 2：解析音频元数据（用独立文件句柄，按需 seek 读取标签，不占满内存）
        let meta_file = std::fs::File::open(path)
            .map_err(|e| NcmError::Session(format!("failed to open {}: {e}", path.display())))?;
        let (song_name, album, artist) = {
            let (s, a, ar) = parse_audio_meta(meta_file, mime);
            (
                if s.is_empty() && !song_hint.is_empty() {
                    song_hint.to_string()
                } else {
                    s
                },
                if a.is_empty() && !album_hint.is_empty() {
                    album_hint.to_string()
                } else {
                    a
                },
                if ar.is_empty() && !artist_hint.is_empty() {
                    artist_hint.to_string()
                } else {
                    ar
                },
            )
        };

        // 文件名归一化（对齐参考实现）
        let raw_name = file_name
            .trim_end_matches(&format!(".{ext}"))
            .replace(' ', "")
            .replace('.', "_");

        // 步骤 3：申请 NOS token
        let token_res = self
            .request_eapi_value(
                "/api/nos/token/alloc",
                serde_json::json!({
                    "bucket": "",
                    "ext": ext,
                    "filename": raw_name,
                    "local": false,
                    "nos_product": 3,
                    "type": "audio",
                    "md5": md5,
                }),
            )
            .await?;
        let token_value: Value = serde_json::from_str(&token_res)?;
        Self::check_api_code(&token_value)?;
        let token_result = token_value
            .get("result")
            .ok_or_else(|| NcmError::parse("nos token alloc: result not found", &token_value))?;
        let resource_id = token_result
            .get("resourceId")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        // 步骤 4：上传原始字节到 NOS（仅当 needUpload 需要上传时），对齐 uploadPlugin
        if need_upload {
            let upload_token = self
                .request_eapi_value(
                    "/api/nos/token/alloc",
                    serde_json::json!({
                        "bucket": "jd-musicrep-privatecloud-audio-public",
                        "ext": ext,
                        "filename": raw_name,
                        "local": false,
                        "nos_product": 3,
                        "type": "audio",
                        "md5": md5,
                    }),
                )
                .await?;
            let upload_token_value: Value = serde_json::from_str(&upload_token)?;
            Self::check_api_code(&upload_token_value)?;
            let upload_result = upload_token_value.get("result").ok_or_else(|| {
                NcmError::parse("upload token alloc: result not found", &upload_token_value)
            })?;
            let upload_object_key = upload_result
                .get("objectKey")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    NcmError::parse("upload token alloc: objectKey missing", &upload_token_value)
                })?
                .replace('/', "%2F");
            let upload_nos_token = upload_result
                .get("token")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    NcmError::parse("upload token alloc: token missing", &upload_token_value)
                })?
                .to_string();

            self.upload_to_nos(
                path,
                &upload_object_key,
                &upload_nos_token,
                &md5,
                size,
                &mime,
            )
            .await?;
        }

        // 步骤 5：提交云盘信息（元数据为空时 fallback 到文件名，对齐参考实现）
        let info = self
            .request_eapi_value(
                "/api/upload/cloud/info/v2",
                serde_json::json!({
                    "md5": md5,
                    "songid": check_song_id,
                    "filename": file_name,
                    "song": if song_name.is_empty() { &raw_name } else { &song_name },
                    "album": if album.is_empty() { &raw_name } else { &album },
                    "artist": if artist.is_empty() { "未知" } else { &artist },
                    "bitrate": bitrate,
                    "resourceId": resource_id,
                }),
            )
            .await?;
        let info_value: Value = serde_json::from_str(&info)?;
        let info_code = info_value.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
        if info_code != 200 && info_code != 400 {
            return Err(NcmError::api(info_value));
        }
        let info_song_id = info_value
            .get("songId")
            .and_then(|v| v.as_u64())
            .or_else(|| {
                info_value
                    .get("songId")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
            })
            .unwrap_or(0);

        // 步骤 6：发布到云盘
        let pub_res = self
            .request_eapi_value(
                "/api/cloud/pub/v2",
                serde_json::json!({ "songid": info_song_id }),
            )
            .await?;
        let pub_value: Value = serde_json::from_str(&pub_res)?;
        Self::check_upload_code(&pub_value)?;

        // 合并 step1 + step6 响应（对齐参考实现返回）
        let mut merged = check_value.clone();
        if let Some(obj) = merged.as_object_mut()
            && let Some(p) = pub_value.as_object()
        {
            for (k, v) in p {
                obj.insert(k.clone(), v.clone());
            }
        }
        parse_cloud_upload(&merged).map_err(|e| NcmError::parse(e, &merged))
    }

    /// 把音频文件流式上传到网易云 NOS 对象存储（以文件作为请求体，不占满内存）
    async fn upload_to_nos(
        &self,
        path: &std::path::Path,
        object_key: &str,
        nos_token: &str,
        md5: &str,
        size: u64,
        mime: &str,
    ) -> Result<(), NcmError> {
        const BUCKET: &str = "jd-musicrep-privatecloud-audio-public";

        // 1. 获取上传节点
        let lbs_url = format!("https://wanproxy.127.net/lbs?version=1.0&bucketname={BUCKET}");
        let lbs: Value = self
            .http
            .get(&lbs_url)
            .header("User-Agent", &self.ua)
            .send()
            .await?
            .json()
            .await?;

        let upload_host = lbs
            .get("upload")
            .and_then(|u| u.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .ok_or_else(|| NcmError::Session(format!("nos lbs returned no upload node: {lbs}")))?
            .to_string();

        // 2. 以文件作为请求体流式上传（reqwest 自动按文件大小设置 Content-Length）
        let file = tokio::fs::File::open(path)
            .await
            .map_err(|e| NcmError::Session(format!("failed to open {}: {e}", path.display())))?;
        let url = format!(
            "{}/{}/{}?offset=0&complete=true&version=1.0",
            upload_host, BUCKET, object_key
        );
        let resp = self
            .http
            .post(&url)
            .header("x-nos-token", nos_token)
            .header("Content-MD5", md5)
            .header("Content-Type", mime)
            .header("Content-Length", size.to_string())
            .body(reqwest::Body::from(file))
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(NcmError::Session(format!(
                "nos upload failed: status={status}, body={body}"
            )));
        }
        log::debug!("nos upload success: status={status}");
        Ok(())
    }
}

/// 从音频文件（seekable reader）解析基础标签（歌名/专辑/歌手）。解析失败返回空串。
fn parse_audio_meta<R: std::io::Read + std::io::Seek>(
    reader: R,
    mime: &str,
) -> (String, String, String) {
    use lofty::file::TaggedFileExt;
    use lofty::tag::ItemKey;
    let file_type = match mime {
        "audio/flac" => Some(lofty::file::FileType::Flac),
        "audio/wav" => Some(lofty::file::FileType::Wav),
        "audio/mp4" => Some(lofty::file::FileType::Mp4),
        "audio/ogg" => Some(lofty::file::FileType::Vorbis),
        _ => Some(lofty::file::FileType::Mpeg),
    };
    let mut probe = lofty::probe::Probe::new(reader)
        .options(lofty::config::ParseOptions::new().read_tags(true));
    if let Some(ft) = file_type {
        probe = probe.set_file_type(ft);
    }
    let guessed = match probe.guess_file_type() {
        Ok(g) => g,
        Err(_) => return (String::new(), String::new(), String::new()),
    };
    let parsed = match guessed.read() {
        Ok(t) => t,
        Err(_) => return (String::new(), String::new(), String::new()),
    };
    let tag = match parsed.tags().first() {
        Some(t) => t,
        None => return (String::new(), String::new(), String::new()),
    };
    let song = tag
        .get_string(ItemKey::TrackTitle)
        .unwrap_or("")
        .to_string();
    let album = tag
        .get_string(ItemKey::AlbumTitle)
        .unwrap_or("")
        .to_string();
    let artist = tag
        .get_string(ItemKey::TrackArtist)
        .or_else(|| tag.get_string(ItemKey::AlbumArtist))
        .unwrap_or("")
        .to_string();

    // 用文件内容回退：从文件名解析（对齐 js 参考实现）
    (song, album, artist)
}

/// 流式读取文件：分块计算 MD5 并统计字节数（不把整文件读入内存）。
fn stream_file_digest(path: &std::path::Path) -> Result<(String, u64), NcmError> {
    use md5::Digest;
    use std::io::Read;

    let mut file = std::fs::File::open(path)
        .map_err(|e| NcmError::Session(format!("failed to open {}: {e}", path.display())))?;
    let mut hasher = md5::Md5::new();
    let mut buf = [0u8; 64 * 1024];
    let mut total: u64 = 0;
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| NcmError::Session(format!("failed to read {}: {e}", path.display())))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        total += n as u64;
    }
    let out = hasher.finalize();
    let mut s = String::with_capacity(out.len() * 2);
    for b in out {
        s.push_str(&format!("{:02x}", b));
    }
    Ok((s, total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_stream_file_digest_known_vector() {
        let dir = std::env::temp_dir().join("ncm_md5_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("abc.bin");
        std::fs::write(&path, b"abc").unwrap();
        let (md5, size) = stream_file_digest(&path).unwrap();
        // MD5("abc") == 900150983cd24fb0d6963f7d28e17f72
        assert_eq!(md5, "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(size, 3);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_parse_cloud_upload_song_id_and_name() {
        let v = json!({
            "songId": 123456,
            "songName": "My Song",
            "needUpload": true,
        });
        let r = parse_cloud_upload(&v).unwrap();
        assert_eq!(r.song_id, 123456);
        assert_eq!(r.song_name, "My Song");
    }

    #[test]
    fn test_parse_cloud_upload_from_private_cloud() {
        let v = json!({
            "privateCloud": { "songId": 999, "songName": "Hidden" },
        });
        let r = parse_cloud_upload(&v).unwrap();
        assert_eq!(r.song_id, 999);
        assert_eq!(r.song_name, "Hidden");
    }

    #[test]
    fn test_parse_audio_meta_graceful_on_garbage() {
        // 非音频字节应优雅返回空串，而非 panic
        let (song, album, artist) = parse_audio_meta(
            std::io::Cursor::new(b"not an audio file at all".to_vec()),
            "audio/mpeg",
        );
        assert_eq!(song, "");
        assert_eq!(album, "");
        assert_eq!(artist, "");
    }

    #[test]
    fn test_upload_song_rejects_missing_path() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let client = NcmClient::new().unwrap();
        // 不存在的路径应返回 Session 错误而非 panic
        let err = rt.block_on(client.upload_song(std::path::Path::new("/nonexistent/file.mp3")));
        assert!(err.is_err());
    }
}
