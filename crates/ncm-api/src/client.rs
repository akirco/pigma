use super::{cookie::CookieStore, encrypt, error::NcmError};
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

mod album;
mod artist;
mod auth;
mod cloud;
mod download;
mod home;
mod interaction;
mod playlist;
mod radio;
mod search;
mod song;

const BASE_URL: &str = "https://music.163.com";
const EAPI_BASE: &str = "https://music.163.com";

struct RequestCookies {
    csrf: String,
    cookie_header: String,
    device_id: String,
}

const UA_LIST: &[&str] = &[
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
];

/// Builder for `NcmClient`
pub struct NcmClientBuilder {
    cookie_path: Option<PathBuf>,
    timeout: Duration,
    proxy: Option<String>,
    user_agent: Option<String>,
}

impl Default for NcmClientBuilder {
    fn default() -> Self {
        Self {
            cookie_path: None,
            timeout: Duration::from_secs(30),
            proxy: None,
            user_agent: None,
        }
    }
}

impl NcmClientBuilder {
    /// Path of the cookie persistence file (defaults to `cookies.json` in the current working
    /// directory; pigma passes it in explicitly)
    pub fn cookie_path(mut self, path: PathBuf) -> Self {
        self.cookie_path = Some(path);
        self
    }

    /// Request timeout (defaults to 30s)
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = duration;
        self
    }

    /// HTTP proxy (supports http / https / socks5)
    pub fn proxy(mut self, proxy: &str) -> Self {
        self.proxy = Some(proxy.to_string());
        self
    }

    /// Custom User-Agent (a random one is chosen by default)
    pub fn user_agent(mut self, ua: &str) -> Self {
        self.user_agent = Some(ua.to_string());
        self
    }

    /// Build `NcmClient`
    pub fn build(self) -> Result<NcmClient, NcmError> {
        let cookie_path = self.cookie_path.unwrap_or_else(default_cookie_path);

        let mut http_builder = Client::builder().timeout(self.timeout);

        if let Some(proxy_url) = &self.proxy {
            let proxy = reqwest::Proxy::all(proxy_url)
                .map_err(|e| NcmError::Session(format!("invalid proxy: {e}")))?;
            http_builder = http_builder.proxy(proxy);
        }

        let http = http_builder
            .build()
            .map_err(|e| NcmError::Session(format!("failed to build HTTP client: {e}")))?;

        let ua = self.user_agent.unwrap_or_else(random_ua);

        Ok(NcmClient {
            http,
            ua,
            store: Arc::new(Mutex::new(CookieStore::new(cookie_path))),
        })
    }
}

fn default_cookie_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("cookies.json")
}

fn random_ua() -> String {
    let i: usize = rand::random_range(0..UA_LIST.len());
    UA_LIST[i].to_string()
}

/// NetEase Cloud Music API client
pub struct NcmClient {
    http: Client,
    ua: String,
    store: Arc<Mutex<CookieStore>>,
}

impl NcmClient {
    /// Get the builder
    pub fn builder() -> NcmClientBuilder {
        NcmClientBuilder::default()
    }

    /// Create a client with default configuration
    pub fn new() -> Result<Self, NcmError> {
        Self::builder().build()
    }

    /// Manually trigger a cookie write to disk (call before the process exits)
    pub fn flush_cookies(&self) {
        if let Ok(mut store) = self.store.lock() {
            store.flush();
        }
    }

    /// Clear `MUSIC_U` (clears the anonymous session after a failed login so it is not
    /// misjudged as logged in)
    pub fn clear_music_u(&self) {
        if let Ok(mut store) = self.store.lock() {
            store.remove("MUSIC_U");
            store.flush();
        }
    }

    /// Check whether the user is logged in (determined via the `MUSIC_U` or `__csrf` cookie)
    pub fn is_logged_in(&self) -> bool {
        self.store.lock().map(|s| s.is_logged_in()).unwrap_or(false)
    }

    /// Get the internal CookieStore (can be used to inject/read cookies)
    pub fn cookie_store(&self) -> &Arc<Mutex<CookieStore>> {
        &self.store
    }

    /// Safely lock the CookieStore, propagating poison errors
    fn with_store<F, T>(&self, f: F) -> Result<T, NcmError>
    where
        F: FnOnce(&mut CookieStore) -> T,
    {
        self.store
            .lock()
            .map(|mut g| f(&mut g))
            .map_err(|_| NcmError::Session("cookie store lock poisoned".into()))
    }

    /// Lock once to obtain csrf_token + cookie_header
    fn prepare_request(&self, is_eapi: bool) -> Result<RequestCookies, NcmError> {
        self.with_store(|store| RequestCookies {
            csrf: store.csrf_token().to_string(),
            cookie_header: store.build_cookie_header(is_eapi),
            device_id: store.device_id().to_string(),
        })
    }

    /// Generic HTTP POST request (shared by weapi/eapi)
    async fn send_request(
        &self,
        url: String,
        body: String,
        host: &str,
        is_eapi: bool,
    ) -> Result<String, NcmError> {
        let cookies = self.prepare_request(is_eapi)?;

        let resp = self
            .http
            .post(&url)
            .header("User-Agent", &self.ua)
            .header("Accept", "*/*")
            .header("Accept-Language", "en-US,en;q=0.5")
            .header("Connection", "keep-alive")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Host", host)
            .header("Referer", "https://music.163.com")
            .header("Cookie", &cookies.cookie_header)
            .body(body)
            .send()
            .await?;

        let status = resp.status();
        {
            self.with_store(|store| store.update_from_response(resp.headers()))?;
        }

        let text = resp.text().await?;
        let preview_200 = text.chars().take(200).collect::<String>();
        log::debug!(
            "send_request status={}, body(len={}): {:?}",
            status,
            text.len(),
            preview_200
        );
        if !status.is_success() {
            let preview_500 = text.chars().take(500).collect::<String>();
            log::warn!(
                "send_request non-200: status={}, body={:?}",
                status,
                preview_500
            );
        }
        Ok(text)
    }

    // ===== Internal requests =====

    async fn request_weapi(&self, path: &str, params: &[(&str, &str)]) -> Result<String, NcmError> {
        let cookies = self.prepare_request(false)?;

        let mut map: HashMap<&str, &str> = params.iter().copied().collect();
        map.insert("csrf_token", &cookies.csrf);
        let params_json =
            serde_json::to_string(&map).map_err(|e| NcmError::Crypto(e.to_string()))?;

        let body = encrypt::weapi(&params_json);

        let path = path
            .strip_prefix("/api/")
            .map(|suffix| format!("/weapi/{}", suffix))
            .unwrap_or_else(|| path.to_string());

        let url = if path.contains('?') {
            format!("{}{}&csrf_token={}", BASE_URL, path, cookies.csrf)
        } else {
            format!("{}{}?csrf_token={}", BASE_URL, path, cookies.csrf)
        };

        self.send_request(url, body, "music.163.com", false).await
    }

    /// Send an eapi request (core implementation).
    ///
    /// The request body consists of encrypted form parameters. When `mobile_ua` is `true`,
    /// the NetEase Cloud iOS client identity is used (login/cloud-disk and similar APIs);
    /// otherwise the configured User-Agent is used together with
    /// `Host: interface.music.163.com` (playback-URL and similar APIs).
    /// Sub-methods only convert their own parameter shapes into `serde_json::Value` and call this method.
    async fn send_eapi(
        &self,
        path: &str,
        data: Value,
        mobile_ua: bool,
    ) -> Result<String, NcmError> {
        let cookies = self.prepare_request(true)?;

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let buildver: String = now_ms.to_string().chars().take(10).collect();
        let request_id = format!("{}_{:04}", now_ms, rand::random::<u16>() % 1000);

        let mut data = match data {
            serde_json::Value::Object(m) => serde_json::Value::Object(m),
            _ => {
                return Err(NcmError::Session(
                    "eapi request expects an object body".into(),
                ));
            }
        };
        let map = data
            .as_object_mut()
            .ok_or_else(|| NcmError::Session("eapi request expects an object body".into()))?;
        map.insert(
            "csrf_token".to_string(),
            serde_json::Value::String(cookies.csrf.clone()),
        );
        map.insert(
            "header".to_string(),
            serde_json::json!({
                "osver": "16.2",
                "deviceId": cookies.device_id,
                "os": "iPhone OS",
                "appver": "9.0.90",
                "versioncode": "140",
                "mobilename": "",
                "buildver": buildver,
                "resolution": "1920x1080",
                "__csrf": cookies.csrf,
                "channel": "",
                "requestId": request_id,
            }),
        );

        let params_json = data.to_string();
        let body = encrypt::eapi(path, &params_json);

        let eapi_path = path.replacen("/api", "/eapi", 1);
        let url = format!("{}{}", EAPI_BASE, eapi_path);

        let mut request = self
            .http
            .post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Cookie", &cookies.cookie_header);
        if mobile_ua {
            request = request.header(
                "User-Agent",
                "NeteaseMusic 9.0.90/5038 (iPhone; iOS 16.2; zh_CN)",
            );
        } else {
            request = request
                .header("User-Agent", &self.ua)
                .header("Accept", "*/*")
                .header("Accept-Language", "en-US,en;q=0.5")
                .header("Host", "interface.music.163.com")
                .header("Referer", "https://music.163.com");
        }
        let resp = request.body(body).send().await?;

        {
            self.with_store(|store| store.update_from_response(resp.headers()))?;
        }

        let status = resp.status();
        let text = resp.text().await?;
        let preview = text.chars().take(200).collect::<String>();
        log::debug!(
            "send_eapi path={path} status={status} body(len={}): {preview:?}",
            text.len(),
        );
        Ok(text)
    }

    /// eapi request whose parameters are a `serde_json::Value` preserving numbers/booleans (iOS client identity).
    async fn request_eapi_value(
        &self,
        path: &str,
        params: serde_json::Value,
    ) -> Result<String, NcmError> {
        self.send_eapi(path, params, true).await
    }

    /// eapi request whose parameters are string key-value pairs (converted to `serde_json::Value` internally).
    async fn request_eapi(&self, path: &str, params: &[(&str, &str)]) -> Result<String, NcmError> {
        let map: HashMap<&str, &str> = params.iter().copied().collect();
        self.send_eapi(path, serde_json::json!(map), false).await
    }

    fn check_api_code(value: &Value) -> Result<(), NcmError> {
        Self::check_codes(value, &[200])
    }

    /// Upload-specific status code check: accepts 200 and 400 (the reference implementation treats 400 as a special state and continues the flow).
    fn check_upload_code(value: &Value) -> Result<(), NcmError> {
        Self::check_codes(value, &[200, 400])
    }

    /// Check whether the `code` in the response is one of the allowed values; otherwise return an API error.
    fn check_codes(value: &Value, allowed: &[i32]) -> Result<(), NcmError> {
        let code = value.get("code").and_then(|c| c.as_i64()).unwrap_or(0) as i32;
        if !allowed.contains(&code) {
            return Err(NcmError::api(value.clone()));
        }
        Ok(())
    }
}
