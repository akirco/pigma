use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SAVE_INTERVAL: Duration = Duration::from_secs(10);

/// 生成 52 位十六进制设备指纹（与官方客户端 `generateDeviceId` 一致）
fn generate_device_id() -> String {
    const HEX: &[u8] = b"0123456789ABCDEF";
    let mut s = String::with_capacity(52);
    for _ in 0..52 {
        s.push(HEX[(rand::random::<u8>() % 16) as usize] as char);
    }
    s
}

/// 登录后服务器下发的持久化 Cookie + 客户端生成的会话标识
pub struct CookieStore {
    /// 持久化 cookies（来自 set-cookie，序列化到磁盘）
    cookies: HashMap<String, String>,
    /// 会话级随机标识（每次启动重新生成，不持久化）
    session: SessionCookies,
    /// 磁盘路径
    path: PathBuf,
    /// 已提取的 CSRF token
    csrf: String,
    /// 上次写盘时间
    last_save: Instant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Persisted {
    cookies: HashMap<String, String>,
}

struct SessionCookies {
    wn_mc_id: String,
    ntes_nnid: String,
    ntes_nuid: String,
    nmtid: String,
}

impl SessionCookies {
    fn new() -> Self {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let now = now_ms.to_string();
        let nuid = format!("{:x}{:x}", rand::random::<u64>(), rand::random::<u64>());
        Self {
            wn_mc_id: format!(
                "{:02x}{:02x}{:02x}.{}",
                rand::random::<u8>(),
                rand::random::<u8>(),
                rand::random::<u8>(),
                now,
            ),
            ntes_nnid: format!("{},{}", nuid, now),
            ntes_nuid: nuid,
            nmtid: format!("{:x}", rand::random::<u64>()),
        }
    }
}

impl CookieStore {
    pub fn new(path: PathBuf) -> Self {
        let persisted: Persisted = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(Persisted {
                cookies: HashMap::new(),
            });

        let mut cookies = persisted.cookies;
        // 设备指纹：持久化，跨重启保持一致（降低风控触发概率）
        if !cookies.contains_key("deviceId") {
            cookies.insert("deviceId".to_string(), generate_device_id());
        }

        let csrf = cookies.get("__csrf").cloned().unwrap_or_default();

        Self {
            cookies,
            session: SessionCookies::new(),
            path,
            csrf,
            last_save: Instant::now(),
        }
    }

    /// 当前设备指纹（52 位十六进制）
    pub fn device_id(&self) -> &str {
        self.cookies
            .get("deviceId")
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    /// 构建完整的 Cookie header
    pub fn build_cookie_header(&self, is_eapi: bool) -> String {
        let (os, appver, osver) = if is_eapi {
            ("iphone", "9.0.90", "16.2")
        } else {
            ("pc", "2.7.1.198277", "10")
        };

        let mut parts: Vec<String> = self
            .cookies
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();

        // 追加客户端生成的会话标识
        parts.push(format!("os={}", os));
        parts.push(format!("appver={}", appver));
        parts.push(format!("osver={}", osver));
        parts.push("WEVNSM=1.0.0".to_string());
        parts.push(format!("WNMCID={}", self.session.wn_mc_id));
        parts.push(format!("_ntes_nnid={}", self.session.ntes_nnid));
        parts.push(format!("_ntes_nuid={}", self.session.ntes_nuid));
        parts.push(format!("NMTID={}", self.session.nmtid));
        parts.push("__remember_me=true".to_string());
        parts.push("channel=".to_string());

        parts.join("; ")
    }

    /// 从响应头中提取 Set-Cookie
    pub fn update_from_response(&mut self, headers: &reqwest::header::HeaderMap) {
        let mut changed = false;

        for set_cookie in headers.get_all("set-cookie").iter() {
            let Ok(val) = set_cookie.to_str() else {
                continue;
            };
            let Some(cookie_part) = val.split(';').next() else {
                continue;
            };
            let Some((name, value)) = cookie_part.split_once('=') else {
                continue;
            };
            let name = name.trim().to_string();
            let value = value.trim().to_string();

            // 追踪 __csrf
            if name == "__csrf" && !value.is_empty() {
                self.csrf = value.clone();
            }

            if self.cookies.get(&name) != Some(&value) {
                changed = true;
                self.cookies.insert(name, value);
            }
        }

        if changed {
            self.flush_if_stale();
        }
    }

    /// CSRF token
    pub fn csrf_token(&self) -> &str {
        &self.csrf
    }

    /// 是否已登录（依据关键 cookie 是否存在）
    pub fn is_logged_in(&self) -> bool {
        self.cookies.contains_key("MUSIC_U") || self.cookies.contains_key("__csrf")
    }

    /// 移除指定 cookie
    pub fn remove(&mut self, name: &str) {
        self.cookies.remove(name);
    }

    /// 强制写盘
    pub fn flush(&mut self) {
        let persisted = Persisted {
            cookies: self.cookies.clone(),
        };
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&persisted) {
            if let Err(e) = std::fs::write(&self.path, &json) {
                log::warn!("failed to write cookie file {:?}: {}", self.path, e);
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ =
                    std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600));
            }
        }
        self.last_save = Instant::now();
    }

    fn flush_if_stale(&mut self) {
        if self.last_save.elapsed() >= SAVE_INTERVAL {
            self.flush();
        }
    }
}

impl Drop for CookieStore {
    fn drop(&mut self) {
        self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_cookie_path() -> PathBuf {
        let dir = std::env::temp_dir().join("ncm_cookie_test");
        let _ = std::fs::create_dir_all(&dir);
        dir.join(format!("test_{}.json", rand::random::<u64>()))
    }

    #[test]
    fn test_new_store_creates_empty() {
        let path = temp_cookie_path();
        let store = CookieStore::new(path.clone());
        assert!(!store.is_logged_in());
        assert_eq!(store.csrf_token(), "");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_new_store_loads_existing() {
        let path = temp_cookie_path();
        let mut cookies = HashMap::new();
        cookies.insert("MUSIC_U".to_string(), "test_token".to_string());
        cookies.insert("__csrf".to_string(), "csrf123".to_string());
        let persisted = Persisted { cookies };
        let json = serde_json::to_string_pretty(&persisted).unwrap();
        std::fs::write(&path, &json).unwrap();

        let store = CookieStore::new(path.clone());
        assert!(store.is_logged_in());
        assert_eq!(store.csrf_token(), "csrf123");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_flush_persists_cookies() {
        let path = temp_cookie_path();
        let mut store = CookieStore::new(path.clone());
        store.cookies.insert("key".to_string(), "value".to_string());
        store.flush();

        let content = std::fs::read_to_string(&path).unwrap();
        let loaded: Persisted = serde_json::from_str(&content).unwrap();
        assert_eq!(loaded.cookies.get("key").unwrap(), "value");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_build_cookie_header_contains_session() {
        let path = temp_cookie_path();
        let store = CookieStore::new(path.clone());
        let header = store.build_cookie_header(false);
        assert!(header.contains("os=pc"));
        assert!(header.contains("WNMCID="));
        assert!(header.contains("_ntes_nnid="));
        assert!(header.contains("__remember_me=true"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_build_cookie_header_eapi() {
        let path = temp_cookie_path();
        let store = CookieStore::new(path.clone());
        let header = store.build_cookie_header(true);
        assert!(header.contains("os=iphone"));
        assert!(header.contains("appver=9.0.90"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_new_store_generates_device_id() {
        let path = temp_cookie_path();
        let store = CookieStore::new(path.clone());
        let id = store.device_id().to_string();
        assert_eq!(id.len(), 52);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase()));

        // header 里带上指纹
        let header = store.build_cookie_header(false);
        assert!(header.contains(&format!("deviceId={id}")));

        // 持久化后重启保持一致
        let mut store = store;
        store.flush();
        let store2 = CookieStore::new(path.clone());
        assert_eq!(store2.device_id(), id);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_is_logged_in_music_u() {
        let path = temp_cookie_path();
        let mut store = CookieStore::new(path.clone());
        store
            .cookies
            .insert("MUSIC_U".to_string(), "token".to_string());
        assert!(store.is_logged_in());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_is_logged_in_csrf() {
        let path = temp_cookie_path();
        let mut store = CookieStore::new(path.clone());
        store
            .cookies
            .insert("__csrf".to_string(), "csrf".to_string());
        assert!(store.is_logged_in());
        let _ = std::fs::remove_file(path);
    }
}
