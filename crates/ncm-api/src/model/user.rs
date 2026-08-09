use super::song::{SongCopyright, SongInfo};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// --- 用户 / 云盘模型 ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginInfo {
    pub code: i32,
    pub uid: u64,
    pub nickname: String,
    pub avatar_url: String,
    pub vip_type: i32,
    pub msg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Msg {
    pub code: i32,
    pub msg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CloudUploadResult {
    pub song_id: u64,
    pub song_name: String,
    /// 服务端返回的原始合并响应，便于调试 / UI 取私有云字段
    pub raw: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudDiskResult {
    pub songs: Vec<SongInfo>,
    pub has_more: bool,
    pub count: u64,
}

// --- 用户 / 云盘解析 ---

pub(crate) fn parse_login_info(value: &Value) -> Result<LoginInfo, String> {
    let code = value["code"].as_i64().unwrap_or(0) as i32;
    if code == 200 {
        Ok(LoginInfo {
            code,
            uid: value["profile"]["userId"].as_u64().unwrap_or(0),
            nickname: value["profile"]["nickname"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            avatar_url: value["profile"]["avatarUrl"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            vip_type: value["profile"]["vipType"].as_i64().unwrap_or(0) as i32,
            msg: String::new(),
        })
    } else {
        let msg = value["msg"]
            .as_str()
            .map(str::to_string)
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| match code {
                501 => "账号或密码错误".to_string(),
                502 => "请切换登录方式或升级版本".to_string(),
                10004 => "当前登录存在安全风险，请稍后再试".to_string(),
                -462 => "需要完成安全验证（滑块/行为验证）".to_string(),
                301 => "登录已过期".to_string(),
                _ => format!("登录失败 (code={code})"),
            });
        Err(msg)
    }
}

pub(crate) fn parse_msg(value: &Value) -> Result<Msg, String> {
    let code = value["code"].as_i64().unwrap_or(0) as i32;
    let msg = value
        .get("msg")
        .or_else(|| value.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok(Msg { code, msg })
}

pub(crate) fn parse_unikey(value: &Value) -> Result<String, String> {
    value["unikey"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "unikey not found".to_string())
}

pub(crate) fn parse_cloud_upload(value: &Value) -> Result<CloudUploadResult, String> {
    let song_id = value["songId"].as_u64().or_else(|| {
        value
            .get("privateCloud")
            .and_then(|p| p.get("songId"))
            .and_then(|v| v.as_u64())
    });
    let song_name = value["songName"]
        .as_str()
        .or_else(|| {
            value
                .get("privateCloud")
                .and_then(|p| p.get("songName"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("")
        .to_string();

    Ok(CloudUploadResult {
        song_id: song_id.unwrap_or(0),
        song_name,
        raw: value.clone(),
    })
}

pub(crate) fn parse_cloud_disk_songs(value: &Value) -> Result<CloudDiskResult, String> {
    let array = value["data"].as_array().ok_or("data not found")?;
    let songs = array
        .iter()
        .map(|v| -> Result<SongInfo, String> {
            let simple = &v["simpleSong"];
            Ok(SongInfo {
                id: v["songId"]
                    .as_u64()
                    .or_else(|| simple["id"].as_u64())
                    .unwrap_or(0),
                name: v["songName"].as_str().unwrap_or("unknown").to_string(),
                singer: v["artist"].as_str().unwrap_or("unknown").to_string(),
                artist_id: simple
                    .get("ar")
                    .and_then(|a| a.as_array())
                    .and_then(|a| a.first())
                    .and_then(|a| a.get("id"))
                    .and_then(|n| n.as_u64())
                    .unwrap_or(0),
                album: v["album"].as_str().unwrap_or("unknown").to_string(),
                album_id: 0,
                pic_url: simple
                    .get("al")
                    .and_then(|a| a.get("picUrl"))
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .to_string(),
                duration: simple["dt"].as_u64().unwrap_or(0),
                copyright: SongCopyright::Unknown,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let has_more = value
        .get("hasMore")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let count = value.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
    Ok(CloudDiskResult {
        songs,
        has_more,
        count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_msg() {
        let v = json!({"code": 200, "msg": "success"});
        let msg = parse_msg(&v).unwrap();
        assert_eq!(msg.code, 200);
        assert_eq!(msg.msg, "success");

        let v2 = json!({"code": 500, "message": "error occurred"});
        let msg2 = parse_msg(&v2).unwrap();
        assert_eq!(msg2.code, 500);
        assert_eq!(msg2.msg, "error occurred");
    }

    #[test]
    fn test_parse_login_info_success() {
        let v = json!({
            "code": 200,
            "profile": {
                "userId": 12345,
                "nickname": "test_user",
                "avatarUrl": "http://avatar.png",
                "vipType": 1
            }
        });
        let info = parse_login_info(&v).unwrap();
        assert_eq!(info.code, 200);
        assert_eq!(info.uid, 12345);
        assert_eq!(info.nickname, "test_user");
        assert_eq!(info.avatar_url, "http://avatar.png");
        assert_eq!(info.vip_type, 1);
    }

    #[test]
    fn test_parse_login_info_failure() {
        let v = json!({
            "code": 400,
            "msg": "login failed"
        });
        let err = parse_login_info(&v).unwrap_err();
        assert_eq!(err, "login failed");
    }

    #[test]
    fn test_parse_unikey() {
        let v = json!({"unikey": "abc123"});
        assert_eq!(parse_unikey(&v).unwrap(), "abc123");

        let v2 = json!({});
        assert!(parse_unikey(&v2).is_err());
    }
}
