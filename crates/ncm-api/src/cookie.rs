use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const SAVE_INTERVAL: Duration = Duration::from_secs(10);

/// Generate a 52-character hexadecimal device fingerprint (matches the official client's
/// `generateDeviceId`)
fn generate_device_id() -> String {
    const HEX: &[u8] = b"0123456789ABCDEF";
    let mut s = String::with_capacity(52);
    for _ in 0..52 {
        s.push(HEX[(rand::random::<u8>() % 16) as usize] as char);
    }
    s
}

/// The persistent Cookie issued by the server after login plus the client-generated session
/// identifiers
pub struct CookieStore {
    /// Persistent cookies (from set-cookie, serialized to disk)
    cookies: HashMap<String, String>,
    /// Cookie names that were already on disk when this store was created.
    /// Used by [`Self::flush`] to write back only what this instance actually
    /// changed, so a second concurrently-running instance cannot clobber
    /// cookies it did not fetch.
    loaded: HashSet<String>,
    /// Cookie names this instance has set or removed since creation.
    dirty: HashSet<String>,
    /// Session-level random identifiers (regenerated at each startup, not persisted)
    session: SessionCookies,
    /// Disk path
    path: PathBuf,
    /// The extracted CSRF token
    csrf: String,
    /// Time of the last write to disk
    last_save: Instant,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
        let persisted = read_persisted(&path).unwrap_or_default();

        let loaded = persisted.cookies.keys().cloned().collect();
        let mut cookies = persisted.cookies;
        // Device fingerprint: persisted and kept consistent across restarts (reduces the chance of triggering risk control)
        if !cookies.contains_key("deviceId") {
            cookies.insert("deviceId".to_string(), generate_device_id());
        }

        let csrf = cookies.get("__csrf").cloned().unwrap_or_default();

        Self {
            cookies,
            loaded,
            dirty: HashSet::new(),
            session: SessionCookies::new(),
            path,
            csrf,
            last_save: Instant::now(),
        }
    }

    /// The current device fingerprint (52 hexadecimal characters)
    pub fn device_id(&self) -> &str {
        self.cookies
            .get("deviceId")
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    /// Build the full Cookie header
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

        // Append the client-generated session identifiers
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

    /// Extract Set-Cookie from the response headers
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

            // Track __csrf
            if name == "__csrf" && !value.is_empty() {
                self.csrf.clone_from(&value);
            }

            if self.cookies.get(&name) != Some(&value) {
                changed = true;
                self.dirty.insert(name.clone());
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

    /// Whether the user is logged in (based on whether the key cookie is present)
    pub fn is_logged_in(&self) -> bool {
        self.cookies.contains_key("MUSIC_U") || self.cookies.contains_key("__csrf")
    }

    /// Remove the specified cookie
    pub fn remove(&mut self, name: &str) {
        self.cookies.remove(name);
        self.dirty.insert(name.to_string());
    }

    /// Force a write to disk.
    ///
    /// The current on-disk state is re-read and only this instance's changes
    /// are applied on top of it, then the result is written atomically (temp
    /// file + rename). This prevents a second concurrently-running instance
    /// from clobbering cookies it did not fetch, and stops a concurrent reader
    /// from ever observing a truncated/empty file.
    pub fn flush(&mut self) {
        let merged = self.merged_for_disk();
        match write_persisted(&self.path, &merged) {
            Ok(()) => {
                self.loaded = merged.keys().cloned().collect();
                self.dirty.clear();
            }
            Err(e) => log::warn!("failed to write cookie file {:?}: {}", self.path, e),
        }
        self.last_save = Instant::now();
    }

    /// Re-read the file and overlay only this instance's changes. A persisted
    /// `deviceId` always wins over a locally generated one, so a store that
    /// failed to load (e.g. started mid-write) cannot replace the fingerprint.
    fn merged_for_disk(&self) -> HashMap<String, String> {
        let Some(disk) = read_persisted(&self.path) else {
            // No readable file (missing or corrupt): fall back to our full
            // in-memory state, which also restores cookies if the file was
            // deleted underneath us.
            return self.cookies.clone();
        };

        let mut merged = disk.cookies;
        let disk_has_device_id = merged.contains_key("deviceId");

        for (name, value) in &self.cookies {
            if name == "deviceId" && disk_has_device_id {
                continue;
            }
            if self.dirty.contains(name) || !self.loaded.contains(name) {
                merged.insert(name.clone(), value.clone());
            }
        }

        // Explicit removals win over anything still on disk.
        for name in &self.dirty {
            if !self.cookies.contains_key(name) {
                merged.remove(name);
            }
        }

        merged
    }

    fn flush_if_stale(&mut self) {
        if self.last_save.elapsed() >= SAVE_INTERVAL {
            self.flush();
        }
    }
}

/// Read and parse the persisted cookie file, returning `None` when it is
/// missing or cannot be parsed.
fn read_persisted(path: &Path) -> Option<Persisted> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

/// Atomically write the persisted cookies to `path`.
///
/// A unique temp file is written in the same directory (mode `0600` on Unix)
/// and then renamed over the target — `rename` is atomic, so a concurrent
/// reader always sees either the old or the new complete file.
fn write_persisted(path: &Path, cookies: &HashMap<String, String>) -> std::io::Result<()> {
    let persisted = Persisted {
        cookies: cookies.clone(),
    };
    let json = serde_json::to_string_pretty(&persisted)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("cookies.json");
    let tmp = dir.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        rand::random::<u32>()
    ));

    if let Err(e) = write_temp_file(&tmp, json.as_bytes()) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

fn write_temp_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(path)?;
    std::io::Write::write_all(&mut file, bytes)?;
    file.sync_all()
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
        assert!(
            id.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase())
        );

        // The header carries the fingerprint
        let header = store.build_cookie_header(false);
        assert!(header.contains(&format!("deviceId={id}")));

        // Persisted so it stays consistent across restarts
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

    fn read_disk(path: &Path) -> HashMap<String, String> {
        read_persisted(path).map(|p| p.cookies).unwrap_or_default()
    }

    #[test]
    fn flush_does_not_clobber_another_instances_cookies() {
        let path = temp_cookie_path();

        // Instance A logs in and persists a session.
        let mut a = CookieStore::new(path.clone());
        a.cookies.insert("MUSIC_U".into(), "token".into());
        a.dirty.insert("MUSIC_U".into());
        a.flush();
        let device = a.device_id().to_string();

        // Instance B started while A was writing, so it loaded nothing: only a
        // freshly generated deviceId. On shutdown it must not wipe A's login.
        let mut b = CookieStore::new(path.clone());
        b.cookies.clear();
        b.cookies.insert("deviceId".into(), "B".into());
        b.loaded.clear();
        b.dirty.clear();
        b.flush();

        let disk = read_disk(&path);
        assert_eq!(
            disk.get("MUSIC_U").map(String::as_str),
            Some("token"),
            "instance B wiped instance A's login cookie"
        );
        assert_eq!(
            disk.get("deviceId").map(String::as_str),
            Some(device.as_str()),
            "a persisted deviceId must not be replaced by a generated one"
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn removal_is_persisted_without_dropping_unrelated_cookies() {
        let path = temp_cookie_path();
        let mut a = CookieStore::new(path.clone());
        a.cookies.insert("MUSIC_U".into(), "token".into());
        a.cookies.insert("__csrf".into(), "csrf".into());
        a.dirty.extend(["MUSIC_U".into(), "__csrf".into()]);
        a.flush();

        // A second instance logs out: only MUSIC_U is removed.
        let mut b = CookieStore::new(path.clone());
        b.remove("MUSIC_U");
        b.flush();

        let disk = read_disk(&path);
        assert!(!disk.contains_key("MUSIC_U"));
        assert_eq!(disk.get("__csrf").map(String::as_str), Some("csrf"));

        let _ = std::fs::remove_file(path);
    }
}
