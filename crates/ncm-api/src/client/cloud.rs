use super::NcmClient;
use crate::{error::NcmError, model::*};
use serde_json::Value;

impl NcmClient {
    // ===== Cloud disk =====

    /// Get the user's recently played songs
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

    /// Get songs from the user's cloud disk
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

    // ===== Cloud disk upload =====

    /// Upload a local audio file to the NetEase Cloud Music cloud disk (the full flow, aligned with NeteaseCloudMusicApi/cloud.js)
    ///
    /// 1. `/api/cloud/upload/check` checks whether an upload is needed
    /// 2. Parse the audio tags to get the title/artist/album
    /// 3. `/api/nos/token/alloc` requests the NOS upload credentials
    /// 4. If `needUpload`, upload the raw bytes to NOS (`wanproxy.127.net`)
    /// 5. `/api/upload/cloud/info/v2` submits the metadata
    /// 6. `/api/cloud/pub/v2` publishes to the cloud disk
    pub async fn upload_song(&self, path: &std::path::Path) -> Result<CloudUploadResult, NcmError> {
        self.upload_song_with_meta(path, "", "", "").await
    }

    /// Same as above, but allows metadata hints (name/singer/album fetched from the cache index)
    /// to be passed in, used as a fallback when tag parsing fails.
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

        // Compute the MD5 and file size in a streaming manner (without reading the whole file into memory)
        let (md5, size) = stream_file_digest(path)?;
        let bitrate = 999000u32;

        // Step 1: pre-upload check (parameters aligned with the reference implementation, preserving JSON types)
        let check_value: Value = self.check_cloud_upload(&md5, size, bitrate).await?;
        let need_upload = check_value
            .get("needUpload")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let check_song_id = parse_song_id(&check_value).unwrap_or_else(|| "0".to_string());

        // Step 2: parse the audio metadata (using a separate file handle, seeking to read the tags on demand, without filling memory)
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

        // Normalize the file name (aligned with the reference implementation)
        let raw_name = file_name
            .trim_end_matches(&format!(".{ext}"))
            .replace(' ', "")
            .replace('.', "_");

        // Step 3: request the NOS token
        let resource_id = self.alloc_nos_token("", &ext, &raw_name, &md5).await?;

        // Step 4: upload the raw bytes to NOS (only when needUpload requires it), aligned with uploadPlugin
        if need_upload {
            self.upload_file_to_nos(path, &ext, &raw_name, &md5, size, mime)
                .await?;
        }

        // Step 5: submit the cloud disk info (falls back to the file name when the metadata is empty, aligned with the reference implementation)
        let info_song_id = self
            .submit_cloud_info(
                &md5,
                &check_song_id,
                &file_name,
                &raw_name,
                &song_name,
                &album,
                &artist,
                bitrate,
                resource_id,
            )
            .await?;

        // Step 6: publish to the cloud disk
        let pub_value: Value = self.publish_cloud_song(info_song_id).await?;

        // Merge the step1 + step6 responses (aligned with the reference implementation's return)
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

    /// Step 1: `/api/cloud/upload/check`, returns the raw response Value.
    async fn check_cloud_upload(
        &self,
        md5: &str,
        size: u64,
        bitrate: u32,
    ) -> Result<Value, NcmError> {
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
        let value: Value = serde_json::from_str(&check)?;
        Self::check_api_code(&value)?;
        Ok(value)
    }

    /// Step 3: `/api/nos/token/alloc` requests the NOS credentials, returns `resourceId`.
    async fn alloc_nos_token(
        &self,
        bucket: &str,
        ext: &str,
        raw_name: &str,
        md5: &str,
    ) -> Result<u64, NcmError> {
        let token_res = self
            .request_eapi_value(
                "/api/nos/token/alloc",
                serde_json::json!({
                    "bucket": bucket,
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
        Ok(token_result
            .get("resourceId")
            .and_then(|v| v.as_u64())
            .unwrap_or(0))
    }

    /// Step 4: upload the raw bytes to NOS (using the public bucket's token).
    async fn upload_file_to_nos(
        &self,
        path: &std::path::Path,
        ext: &str,
        raw_name: &str,
        md5: &str,
        size: u64,
        mime: &str,
    ) -> Result<(), NcmError> {
        const BUCKET: &str = "jd-musicrep-privatecloud-audio-public";
        let upload_token = self
            .request_eapi_value(
                "/api/nos/token/alloc",
                serde_json::json!({
                    "bucket": BUCKET,
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

        self.upload_to_nos(path, &upload_object_key, &upload_nos_token, md5, size, mime)
            .await
    }

    /// Step 5: `/api/upload/cloud/info/v2` submits the metadata, returns the server-assigned `songId`.
    #[allow(clippy::too_many_arguments)]
    async fn submit_cloud_info(
        &self,
        md5: &str,
        song_id: &str,
        file_name: &str,
        raw_name: &str,
        song_name: &str,
        album: &str,
        artist: &str,
        bitrate: u32,
        resource_id: u64,
    ) -> Result<u64, NcmError> {
        let info = self
            .request_eapi_value(
                "/api/upload/cloud/info/v2",
                serde_json::json!({
                    "md5": md5,
                    "songid": song_id,
                    "filename": file_name,
                    "song": if song_name.is_empty() { raw_name } else { song_name },
                    "album": if album.is_empty() { raw_name } else { album },
                    "artist": if artist.is_empty() { "未知" } else { artist },
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
        Ok(info_value
            .get("songId")
            .and_then(|v| v.as_u64())
            .or_else(|| {
                info_value
                    .get("songId")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
            })
            .unwrap_or(0))
    }

    /// Step 6: `/api/cloud/pub/v2` publishes to the cloud disk, returns the response Value.
    async fn publish_cloud_song(&self, song_id: u64) -> Result<Value, NcmError> {
        let pub_res = self
            .request_eapi_value(
                "/api/cloud/pub/v2",
                serde_json::json!({ "songid": song_id }),
            )
            .await?;
        let pub_value: Value = serde_json::from_str(&pub_res)?;
        Self::check_upload_code(&pub_value)?;
        Ok(pub_value)
    }

    /// Stream-upload an audio file to the NetEase Cloud NOS object storage (using the file as
    /// the request body, without filling memory)
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

        // 1. Get the upload node
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

        // 2. Stream-upload using the file as the request body (reqwest automatically sets Content-Length from the file size)
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

/// Parse `songId` from the response (may be a string or a number, aligned with the reference implementation).
fn parse_song_id(value: &Value) -> Option<String> {
    value
        .get("songId")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .or_else(|| {
            value
                .get("songId")
                .and_then(|v| v.as_u64().map(|n| n.to_string()))
        })
}

/// Parse the basic tags (title/album/artist) from an audio file (seekable reader). Returns
/// empty strings when parsing fails.
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

    // Fall back on the values derived from the file (aligned with the JS reference implementation)
    (song, album, artist)
}

/// Read a file in a streaming manner: compute the MD5 in chunks and count the bytes (without
/// reading the whole file into memory).
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
        // Non-audio bytes should gracefully return empty strings instead of panicking
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
        // A non-existent path should return a Session error instead of panicking
        let err = rt.block_on(client.upload_song(std::path::Path::new("/nonexistent/file.mp3")));
        assert!(err.is_err());
    }
}
