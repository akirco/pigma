use super::{cookie::CookieStore, encrypt, error::NcmError, model::*};
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const BASE_URL: &str = "https://music.163.com";
const EAPI_BASE: &str = "https://interface.music.163.com";

struct RequestCookies {
    csrf: String,
    cookie_header: String,
}

const UA_LIST: &[&str] = &[
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
];

/// `NcmClient` 构造器
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
    /// Cookie 持久化文件路径（默认 `~/.config/pigma/cookies.json`）
    pub fn cookie_path(mut self, path: PathBuf) -> Self {
        self.cookie_path = Some(path);
        self
    }

    /// 请求超时时间（默认 30s）
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = duration;
        self
    }

    /// HTTP 代理（支持 http / https / socks5）
    pub fn proxy(mut self, proxy: &str) -> Self {
        self.proxy = Some(proxy.to_string());
        self
    }

    /// 自定义 User-Agent（默认随机选一个）
    pub fn user_agent(mut self, ua: &str) -> Self {
        self.user_agent = Some(ua.to_string());
        self
    }

    /// 构建 `NcmClient`
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

        let no_proxy_http = reqwest::Client::builder()
            .timeout(self.timeout)
            .no_proxy()
            .http1_only()
            .build()
            .map_err(|e| NcmError::Session(format!("failed to build no_proxy HTTP client: {e}")))?;

        let ua = self.user_agent.unwrap_or_else(random_ua);

        Ok(NcmClient {
            http,
            no_proxy_http,
            ua,
            store: Arc::new(Mutex::new(CookieStore::new(cookie_path))),
        })
    }
}

fn default_cookie_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pigma")
        .join("cookies.json")
}

fn random_ua() -> String {
    let i: usize = rand::random_range(0..UA_LIST.len());
    UA_LIST[i].to_string()
}

/// 网易云音乐 API 客户端
pub struct NcmClient {
    http: Client,
    no_proxy_http: Client,
    ua: String,
    store: Arc<Mutex<CookieStore>>,
}

impl NcmClient {
    /// 获取构造器
    pub fn builder() -> NcmClientBuilder {
        NcmClientBuilder::default()
    }

    /// 创建默认配置的客户端
    pub fn new() -> Result<Self, NcmError> {
        Self::builder().build()
    }

    /// 手动触发 cookie 写盘（进程退出前调用）
    pub fn flush_cookies(&self) {
        if let Ok(mut store) = self.store.lock() {
            store.flush();
        }
    }

    /// 检查是否已登录（通过 `MUSIC_U` 或 `__csrf` cookie 判断）
    pub fn is_logged_in(&self) -> bool {
        self.store.lock().map(|s| s.is_logged_in()).unwrap_or(false)
    }

    /// 获取内部 CookieStore（可用于注入/读取 cookie）
    pub fn cookie_store(&self) -> &Arc<Mutex<CookieStore>> {
        &self.store
    }

    /// 安全地锁住 CookieStore，传播 poison 错误
    fn with_store<F, T>(&self, f: F) -> Result<T, NcmError>
    where
        F: FnOnce(&mut CookieStore) -> T,
    {
        self.store
            .lock()
            .map(|mut g| f(&mut g))
            .map_err(|_| NcmError::Session("cookie store lock poisoned".into()))
    }

    /// 单次上锁获取 csrf_token + cookie_header
    fn prepare_request(&self, is_eapi: bool) -> Result<RequestCookies, NcmError> {
        self.with_store(|store| RequestCookies {
            csrf: store.csrf_token().to_string(),
            cookie_header: store.build_cookie_header(is_eapi),
        })
    }

    /// 通用 HTTP POST 请求（weapi/eapi 共用）
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
            let headers = resp.headers().clone();
            self.with_store(|store| store.update_from_response(&headers))?;
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

    // ===== 内部请求 =====

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

    async fn request_eapi(&self, path: &str, params: &[(&str, &str)]) -> Result<String, NcmError> {
        let cookies = self.prepare_request(true)?;

        let mut map: HashMap<&str, &str> = params.iter().copied().collect();
        map.insert("csrf_token", &cookies.csrf);

        let mut data = serde_json::json!(map);

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let buildver: String = now_ms.to_string().chars().take(10).collect();
        let request_id = format!("{}_{:04}", now_ms, rand::random::<u16>() % 1000);

        if let serde_json::Value::Object(ref mut map_obj) = data {
            map_obj.insert(
                "header".to_string(),
                serde_json::json!({
                    "osver": "16.2",
                    "deviceId": "",
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
        }

        let params_json = data.to_string();
        let body = encrypt::eapi(path, &params_json);

        let eapi_path = path.replacen("/api", "/eapi", 1);
        let url = format!("{}{}", EAPI_BASE, eapi_path);

        let client = &self.no_proxy_http;

        let resp = client
            .post(&url)
            .header("User-Agent", &self.ua)
            .header("Accept", "*/*")
            .header("Accept-Language", "en-US,en;q=0.5")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Host", "interface.music.163.com")
            .header("Referer", "https://music.163.com")
            .header("Cookie", &cookies.cookie_header)
            .body(body)
            .send()
            .await?;

        {
            let headers = resp.headers().clone();
            self.with_store(|store| store.update_from_response(&headers))?;
        }

        let text = resp.text().await?;
        Ok(text)
    }

    fn check_api_code(value: &Value) -> Result<(), NcmError> {
        let code = value.get("code").and_then(|c| c.as_i64()).unwrap_or(0) as i32;
        if code != 200 {
            return Err(NcmError::api(value.clone()));
        }
        Ok(())
    }

    // ===== 认证 =====

    /// 登录（自动识别手机号/邮箱）
    ///
    /// * `username` — 手机号（11 位数字）或邮箱
    /// * `password` — 密码（明文）
    pub async fn login(&self, username: &str, password: &str) -> Result<LoginInfo, NcmError> {
        let (path, params);
        if username.len() == 11 && username.parse::<u64>().is_ok() {
            path = "/weapi/login/cellphone";
            params = vec![
                ("phone", username),
                ("password", password),
                ("rememberLogin", "true"),
            ];
        } else {
            path = "/weapi/login";
            params = vec![
                ("username", username),
                ("password", password),
                ("rememberLogin", "true"),
                (
                    "clientToken",
                    "1_jVUMqWEPke0/1/Vu56xCmJpo5vP1grjn_SOVVDzOc78w8OKLVZ2JH7IfkjSXqgfmh",
                ),
            ];
        }
        let result = self.request_weapi(path, &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        parse_login_info(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// 手机验证码登录
    ///
    /// * `ctcode` — 国家码（如 `86`）
    /// * `phone` — 手机号
    /// * `captcha` — 验证码
    pub async fn login_cellphone(
        &self,
        ctcode: &str,
        phone: &str,
        captcha: &str,
    ) -> Result<LoginInfo, NcmError> {
        let params = vec![
            ("phone", phone),
            ("countrycode", ctcode),
            ("captcha", captcha),
            ("rememberLogin", "true"),
        ];
        let result = self
            .request_weapi("/weapi/login/cellphone", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        parse_login_info(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// 发送短信验证码
    ///
    /// * `ctcode` — 国家码（如 `86`）
    /// * `phone` — 手机号
    pub async fn captcha(&self, ctcode: &str, phone: &str) -> Result<(), NcmError> {
        let params = vec![("cellphone", phone), ("ctcode", ctcode)];
        let result = self
            .request_weapi("/weapi/sms/captcha/sent", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)
    }

    /// 创建登录二维码，返回 (二维码 URL, unikey)
    pub async fn login_qr_create(&self) -> Result<(String, String), NcmError> {
        let params = vec![("type", "1")];
        let result = self
            .request_weapi("/weapi/login/qrcode/unikey", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        let unikey = parse_unikey(&value).map_err(|e| NcmError::parse(e, &value))?;
        let qr_url = format!("https://music.163.com/login?codekey={}", &unikey);
        Ok((qr_url, unikey))
    }

    /// 轮询二维码登录状态
    ///
    /// * `key` — 由 `login_qr_create` 返回的 unikey
    pub async fn login_qr_check(&self, key: &str) -> Result<Msg, NcmError> {
        let params = vec![("type", "1"), ("key", key)];
        let result = self
            .request_weapi("/weapi/login/qrcode/client/login", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        parse_msg(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取当前登录状态
    pub async fn login_status(&self) -> Result<LoginInfo, NcmError> {
        let result = self.request_weapi("/api/nuser/account/get", &[]).await?;
        let value: Value = serde_json::from_str(&result)?;
        parse_login_info(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// 退出登录
    pub async fn logout(&self) -> Result<Msg, NcmError> {
        let result = self.request_weapi("/weapi/logout", &[]).await?;
        let value: Value = serde_json::from_str(&result)?;
        parse_msg(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取歌曲详情
    ///
    /// * `ids` — 歌曲 ID 列表
    pub async fn songs_detail(&self, ids: &[u64]) -> Result<Vec<SongInfo>, NcmError> {
        let c: String = ids
            .iter()
            .map(|id| format!(r#"{{"id":"{}"}}"#, id))
            .collect::<Vec<_>>()
            .join(",");
        let c = format!("[{}]", c);
        let params = vec![("c", c.as_str())];
        let result = self.request_weapi("/weapi/v3/song/detail", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_info_array(&value, &["songs"], SongContext::Usl).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取歌曲播放 URL（基于码率）
    ///
    /// * `ids` — 歌曲 ID 列表
    /// * `br` — 码率：`128000` / `192000` / `320000` / `999000` / `1900000`
    pub async fn songs_url(&self, ids: &[u64], br: &str) -> Result<Vec<SongUrl>, NcmError> {
        let ids_json = serde_json::to_string(ids).map_err(|e| NcmError::Crypto(e.to_string()))?;
        let params = vec![("ids", ids_json.as_str()), ("br", br)];
        let result = self
            .request_eapi("/api/song/enhance/player/url", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_url(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取歌曲播放 URL（基于音质等级）
    ///
    /// * `ids` — 歌曲 ID 列表
    /// * `level` — 音质等级，见 [`SongQuality`]
    pub async fn songs_url_v1(
        &self,
        ids: &[u64],
        level: SongQuality,
    ) -> Result<Vec<SongUrl>, NcmError> {
        let ids_json = serde_json::to_string(ids).map_err(|e| NcmError::Crypto(e.to_string()))?;
        let level_str = level.as_level();
        let encode_type = if level.is_lossy() { "aac" } else { "flac" };
        let mut params = vec![
            ("ids", ids_json.as_str()),
            ("level", level_str),
            ("encodeType", encode_type),
        ];
        if level == SongQuality::AudioVivid {
            params.push(("immerseType", "c51"));
        }
        let result = self
            .request_eapi("/api/song/enhance/player/url/v1", &params)
            .await?;
        let preview_300 = result.chars().take(500).collect::<String>();
        log::debug!("songs_url_v1 raw response (first 500): {:?}", preview_300);
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_url(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取歌词
    ///
    /// * `id` — 歌曲 ID
    pub async fn song_lyric(&self, id: u64) -> Result<Lyrics, NcmError> {
        let id_str = id.to_string();
        let params = vec![("id", id_str.as_str()), ("lv", "-1"), ("tv", "-1")];
        let result = self.request_weapi("/weapi/song/lyric", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_lyrics(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取歌单详情（含歌曲列表）
    ///
    /// * `id` — 歌单 ID
    pub async fn song_list_detail(&self, id: u64) -> Result<PlayListDetail, NcmError> {
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
        parse_playlist_detail(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取我喜欢的歌曲
    pub async fn liked_songs(&self, uid: u64) -> Result<Vec<SongInfo>, NcmError> {
        let uid_str = uid.to_string();
        let result = self
            .request_weapi("/api/song/like/get", &[("uid", &uid_str)])
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        let ids: Vec<u64> = parse_song_id_list(&value).map_err(|e| NcmError::parse(e, &value))?;
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
        let result = self.request_weapi("/api/user/playlist/create", &params).await?;
        log::debug!("user_created_playlist response: {}", &result[..result.len().min(500)]);
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
        let result = self.request_weapi("/api/user/playlist/collect", &params).await?;
        log::debug!("user_collected_playlist response: {}", &result[..result.len().min(500)]);
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

    /// 搜索单曲
    ///
    /// * `keyword` — 关键词
    /// * `offset` — 偏移量
    /// * `limit` — 数量
    pub async fn search_song(
        &self,
        keyword: &str,
        offset: u16,
        limit: u16,
    ) -> Result<SearchResult, NcmError> {
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("s", keyword),
            ("type", "1"),
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
        ];
        let result = self.request_weapi("/weapi/search/get", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;

        let total = value["result"]["songCount"].as_u64().unwrap_or(0) as u32;
        let songs = parse_song_info_array(&value, &["result", "songs"], SongContext::Search)
            .map_err(|e| NcmError::parse(e, &value))?;

        Ok(SearchResult { songs, total })
    }

    /// 搜索歌单
    ///
    /// * `keyword` — 关键词
    /// * `offset` — 偏移量
    /// * `limit` — 数量
    pub async fn search_songlist(
        &self,
        keyword: &str,
        offset: u16,
        limit: u16,
    ) -> Result<Vec<SongList>, NcmError> {
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("s", keyword),
            ("type", "1000"),
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
        ];
        let result = self.request_weapi("/weapi/search/get", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_list(&value, &["result", "playlists"]).map_err(|e| NcmError::parse(e, &value))
    }

    /// 搜索歌手
    ///
    /// * `keyword` — 关键词
    /// * `offset` — 偏移量
    /// * `limit` — 数量
    pub async fn search_singer(
        &self,
        keyword: &str,
        offset: u16,
        limit: u16,
    ) -> Result<Vec<SingerInfo>, NcmError> {
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("s", keyword),
            ("type", "100"),
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
        ];
        let result = self.request_weapi("/weapi/search/get", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_singer_info(&value, &["result", "artists"]).map_err(|e| NcmError::parse(e, &value))
    }

    /// 搜索专辑
    ///
    /// * `keyword` — 关键词
    /// * `offset` — 偏移量
    /// * `limit` — 数量
    pub async fn search_album(
        &self,
        keyword: &str,
        offset: u16,
        limit: u16,
    ) -> Result<Vec<SongList>, NcmError> {
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("s", keyword),
            ("type", "10"),
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
        ];
        let result = self.request_weapi("/weapi/search/get", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_list(&value, &["result", "albums"]).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取热搜榜
    pub async fn search_hot(&self) -> Result<Vec<HotSearchItem>, NcmError> {
        let result = self.request_weapi("/api/hotsearchlist/get", &[]).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_hot_search(&value).map_err(|e| NcmError::parse(e, &value))
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
        parse_song_info_array(&value, &["data", "dailySongs"], SongContext::Rmds).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取私人 FM 歌曲
    pub async fn personal_fm(&self) -> Result<Vec<SongInfo>, NcmError> {
        let result = self.request_weapi("/weapi/v1/radio/get", &[]).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_info_array(&value, &["data"], SongContext::Rmd).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取所有排行榜列表
    pub async fn toplist(&self) -> Result<Vec<TopList>, NcmError> {
        let result = self.request_weapi("/api/toplist", &[]).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_toplist(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取排行榜歌曲（等同于 `song_list_detail`）
    ///
    /// * `list_id` — 排行榜 ID（如云音乐飙升榜 `19723756`）
    pub async fn top_songs(&self, list_id: u64) -> Result<PlayListDetail, NcmError> {
        self.song_list_detail(list_id).await
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

    /// 获取首页轮播图
    pub async fn banners(&self) -> Result<Vec<BannersInfo>, NcmError> {
        let params = vec![("clientType", "pc")];
        let result = self.request_weapi("/weapi/v2/banner/get", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_banners(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取歌手热门歌曲
    ///
    /// * `id` — 歌手 ID
    pub async fn singer_songs(&self, id: u64) -> Result<Vec<SongInfo>, NcmError> {
        let path = format!("/weapi/v1/artist/{}", id);
        let result = self.request_weapi(&path, &[]).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_info_array(&value, &["hotSongs"], SongContext::Singer).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取歌手全部歌曲
    ///
    /// * `id` — 歌手 ID
    /// * `order` — `"hot"`（热门）或 `"time"`（时间）
    /// * `offset` — 偏移量
    /// * `limit` — 数量
    pub async fn singer_all_songs(
        &self,
        id: u64,
        order: &str,
        offset: u16,
        limit: u16,
    ) -> Result<Vec<SongInfo>, NcmError> {
        let id_str = id.to_string();
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("id", id_str.as_str()),
            ("private_cloud", "true"),
            ("work_type", "1"),
            ("order", order),
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
        ];
        let result = self
            .request_weapi("/weapi/v1/artist/songs", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_info_array(&value, &["songs"], SongContext::SingerSongs).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取热门歌手
    pub async fn top_artists(&self, offset: u16, limit: u16) -> Result<Vec<SingerInfo>, NcmError> {
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
            ("total", "true"),
        ];
        let result = self.request_weapi("/api/artist/top", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_singer_info(&value, &["artists"]).map_err(|e| NcmError::parse(e, &value))
    }

    /// 获取歌手榜（排行榜）
    ///
    /// * `r#type` — 榜单类型（1-华语, 2-欧美, 3-韩国, 4-日本）
    pub async fn toplist_artist(&self, r#type: u8) -> Result<Vec<SingerInfo>, NcmError> {
        let limit_str = 100u16.to_string();
        let offset_str = 0u16.to_string();
        let type_str = r#type.to_string();
        let params = vec![
            ("type", type_str.as_str()),
            ("limit", limit_str.as_str()),
            ("offset", offset_str.as_str()),
            ("total", "true"),
        ];
        let result = self.request_weapi("/api/toplist/artist", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        // Response: { code: 200, list: { artists: [...] } }
        let list = value.get("list").ok_or_else(|| NcmError::parse(String::from("list not found"), &value))?;
        parse_singer_info(list, &["artists"]).map_err(|e| NcmError::parse(e, &value))
    }

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

    /// 喜欢/取消喜欢歌曲
    ///
    /// * `song_id` — 歌曲 ID
    /// * `like` — `true` 喜欢，`false` 取消
    pub async fn like(&self, song_id: u64, like: bool) -> Result<Msg, NcmError> {
        let id_str = song_id.to_string();
        let like_str = if like { "true" } else { "false" };
        let params = vec![
            ("alg", "itembased"),
            ("trackId", id_str.as_str()),
            ("like", like_str),
            ("time", "25"),
        ];
        let result = self.request_weapi("/weapi/radio/like", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        parse_msg(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// FM 垃圾桶（不喜欢当前 FM 歌曲）
    ///
    /// * `song_id` — 歌曲 ID
    pub async fn fm_trash(&self, song_id: u64) -> Result<Msg, NcmError> {
        let id_str = song_id.to_string();
        let params = vec![("alg", "RT"), ("songId", id_str.as_str()), ("time", "25")];
        let result = self
            .request_weapi("/weapi/radio/trash/add", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        parse_msg(&value).map_err(|e| NcmError::parse(e, &value))
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

    // ===== 动态信息 =====

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

    // ===== 每日任务 =====

    /// 每日签到
    ///
    /// * `type` — `0`（PC）或 `1`（移动端）
    pub async fn daily_task(&self, r#type: &str) -> Result<Msg, NcmError> {
        let params = vec![("type", r#type)];
        let result = self
            .request_weapi("/weapi/point/dailyTask", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        parse_msg(&value).map_err(|e| NcmError::parse(e, &value))
    }

    // ===== 上报播放记录 =====

    /// 听歌打卡 — 上报歌曲播放记录
    ///
    /// * `song_id` — 歌曲 ID
    /// * `time_ms` — 播放时长（毫秒）
    /// * `source_id` — 来源歌单 ID（可选）
    pub async fn report_play(
        &self,
        song_id: u64,
        time_ms: u64,
        source_id: Option<u64>,
    ) -> Result<(), NcmError> {
        let log = serde_json::json!([{
            "action": "play",
            "json": {
                "id": song_id,
                "sourceId": source_id.unwrap_or(0),
                "time": time_ms,
                "type": "song",
                "end": "playend",
                "download": 0,
                "wifi": 0,
                "source": "list",
                "mainsite": 1,
                "content": "",
            }
        }]);
        let logs_str = log.to_string();
        let params = vec![("logs", logs_str.as_str())];
        let _ = self.request_weapi("/api/feedback/weblog", &params).await?;
        Ok(())
    }

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
                songs.push(parse_song_info(song_data, SongContext::Usl).map_err(|e| NcmError::parse(e, &value))?);
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
        let params = vec![
            ("offset", offset_s.as_str()),
            ("limit", limit_s.as_str()),
        ];
        let result = self.request_weapi("/weapi/v1/cloud/get", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_cloud_disk_songs(&value).map_err(|e| NcmError::parse(e, &value))
    }

    // ===== 搜索歌词 =====

    /// 搜索歌词
    ///
    /// * `keyword` — 关键词
    /// * `offset` — 偏移量
    /// * `limit` — 数量
    pub async fn search_lyrics(
        &self,
        keyword: &str,
        offset: u16,
        limit: u16,
    ) -> Result<Vec<SongInfo>, NcmError> {
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("s", keyword),
            ("type", "1006"),
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
        ];
        let result = self.request_weapi("/weapi/search/get", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_info_array(&value, &["result", "songs"], SongContext::Search).map_err(|e| NcmError::parse(e, &value))
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

    // ===== 首页 =====

    /// 获取 APP 首页板块信息（返回原始 JSON）
    pub async fn homepage(&self) -> Result<String, NcmError> {
        let params = vec![("refresh", "false"), ("cursor", "null")];
        self.request_weapi("/api/homepage/block/page", &params)
            .await
    }

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

    /// 心动模式 / 智能播放
    ///
    /// * `song_id` — 当前播放的歌曲 ID
    /// * `playlist_id` — 歌单 ID
    pub async fn playmode_intelligence_list(
        &self,
        song_id: u64,
        playlist_id: u64,
    ) -> Result<Vec<SongInfo>, NcmError> {
        let sid_str = song_id.to_string();
        let pid_str = playlist_id.to_string();
        let params = vec![
            ("songId", sid_str.as_str()),
            ("type", "fromPlayOne"),
            ("playlistId", pid_str.as_str()),
            ("startMusicId", sid_str.as_str()),
            ("count", "1"),
        ];
        let result = self
            .request_weapi("/weapi/playmode/intelligence/list", &params)
            .await?;
        log::debug!(
            "intelligence list response: song_id={}, playlist_id={}, response={}",
            song_id,
            playlist_id,
            &result[..result.len().min(2000)]
        );
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        let songs = parse_intelligence_songs(&value).map_err(|e| NcmError::parse(e, &value))?;
        log::debug!("intelligence list parsed: {} songs", songs.len());
        Ok(songs)
    }

    /// 从网络下载图片到本地
    ///
    /// * `url` — 图片 URL
    /// * `path` — 本地保存路径（含文件名）
    /// * `width` — 请求宽度
    /// * `height` — 请求高度
    pub async fn download_img(
        &self,
        url: &str,
        path: std::path::PathBuf,
        width: u16,
        height: u16,
    ) -> Result<(), NcmError> {
        if path.exists() {
            return Ok(());
        }
        let image_url = format!("{}?param={}y{}", url, width, height);
        let bytes = self.http.get(&image_url).send().await?.bytes().await?;
        let parent = path.parent().map(|p| p.to_path_buf());
        tokio::task::spawn_blocking(move || {
            if let Some(parent) = parent {
                if let Err(e) = std::fs::create_dir_all(&parent) {
                    log::warn!("failed to create image dir {:?}: {}", parent, e);
                }
            }
            std::fs::write(&path, &bytes)
        })
        .await
        .map_err(|e| NcmError::Session(format!("spawn_blocking failed: {e}")))?
        .map_err(|e| NcmError::Session(format!("failed to write image: {e}")))?;
        Ok(())
    }

    /// 从网络下载歌曲到本地
    ///
    /// * `url` — 歌曲 URL
    /// * `path` — 本地保存路径（含文件名）
    pub async fn download_song(&self, url: &str, path: std::path::PathBuf) -> Result<(), NcmError> {
        if path.exists() {
            return Ok(());
        }
        let bytes = self.http.get(url).send().await?.bytes().await?;
        let parent = path.parent().map(|p| p.to_path_buf());
        tokio::task::spawn_blocking(move || {
            if let Some(parent) = parent {
                if let Err(e) = std::fs::create_dir_all(&parent) {
                    log::warn!("failed to create song dir {:?}: {}", parent, e);
                }
            }
            std::fs::write(&path, &bytes)
        })
        .await
        .map_err(|e| NcmError::Session(format!("spawn_blocking failed: {e}")))?
        .map_err(|e| NcmError::Session(format!("failed to write song: {e}")))?;
        Ok(())
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

        // 步骤 1：上传前检查
        let check = self
            .request_weapi(
                "/api/cloud/upload/check",
                &[
                    ("bitrate", &bitrate.to_string()),
                    ("ext", ""),
                    ("length", &size.to_string()),
                    ("md5", &md5),
                    ("songId", "0"),
                    ("version", "1"),
                ],
            )
            .await?;
        let check_value: Value = serde_json::from_str(&check)?;
        Self::check_api_code(&check_value)?;
        let need_upload = check_value
            .get("needUpload")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let check_song_id = check_value
            .get("songId")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        // 步骤 2：解析音频元数据（用独立文件句柄，按需 seek 读取标签，不占满内存）
        let meta_file = std::fs::File::open(path)
            .map_err(|e| NcmError::Session(format!("failed to open {}: {e}", path.display())))?;
        let (song_name, album, artist) = parse_audio_meta(meta_file, mime);

        // 文件名归一化（对齐参考实现）
        let raw_name = file_name
            .trim_end_matches(&format!(".{ext}"))
            .replace(' ', "")
            .replace('.', "_");

        // 步骤 3：申请 NOS token
        let token_res = self
            .request_weapi(
                "/api/nos/token/alloc",
                &[
                    ("bucket", ""),
                    ("ext", &ext),
                    ("filename", &raw_name),
                    ("local", "false"),
                    ("nos_product", "3"),
                    ("type", "audio"),
                    ("md5", &md5),
                ],
            )
            .await?;
        let token_value: Value = serde_json::from_str(&token_res)?;
        Self::check_api_code(&token_value)?;
        let token_result = token_value
            .get("result")
            .ok_or_else(|| NcmError::parse("nos token alloc: result not found", &token_value))?;
        let object_key = token_result
            .get("objectKey")
            .and_then(|v| v.as_str())
            .ok_or_else(|| NcmError::parse("nos token alloc: objectKey missing", &token_value))?
            .replace('/', "%2F");
        let nos_token = token_result
            .get("token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| NcmError::parse("nos token alloc: token missing", &token_value))?
            .to_string();
        let resource_id = token_result
            .get("resourceId")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        // 步骤 4：上传原始字节到 NOS（以文件作为请求体，流式上传）
        if need_upload {
            self.upload_to_nos(path, &object_key, &nos_token, &md5, size)
                .await?;
        }

        // 步骤 5：提交云盘信息
        let info = self
            .request_weapi(
                "/api/upload/cloud/info/v2",
                &[
                    ("md5", &md5),
                    ("songid", &check_song_id.to_string()),
                    ("filename", &file_name),
                    ("song", &song_name),
                    ("album", &album),
                    ("artist", &artist),
                    ("bitrate", &bitrate.to_string()),
                    ("resourceId", &resource_id.to_string()),
                ],
            )
            .await?;
        let info_value: Value = serde_json::from_str(&info)?;
        Self::check_api_code(&info_value)?;
        let info_song_id = info_value
            .get("songId")
            .and_then(|v| v.as_u64())
            .or(Some(check_song_id))
            .unwrap_or(0);

        // 步骤 6：发布到云盘
        let pub_res = self
            .request_weapi("/api/cloud/pub/v2", &[("songid", &info_song_id.to_string())])
            .await?;
        let pub_value: Value = serde_json::from_str(&pub_res)?;
        Self::check_api_code(&pub_value)?;

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
            .ok_or_else(|| {
                NcmError::Session(format!("nos lbs returned no upload node: {lbs}"))
            })?
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
            .header("Content-Type", "audio/mpeg")
            .header("Content-Length", size.to_string())
            .body(reqwest::Body::from(file))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(NcmError::Session(format!(
                "nos upload failed: status={status}, body={body}"
            )));
        }
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
        .get_string(&ItemKey::TrackTitle)
        .unwrap_or("")
        .to_string();
    let album = tag
        .get_string(&ItemKey::AlbumTitle)
        .unwrap_or("")
        .to_string();
    let artist = tag
        .get_string(&ItemKey::TrackArtist)
        .or_else(|| tag.get_string(&ItemKey::AlbumArtist))
        .unwrap_or("")
        .to_string();
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
        let (song, album, artist) =
            parse_audio_meta(std::io::Cursor::new(b"not an audio file at all".to_vec()), "audio/mpeg");
        assert_eq!(song, "");
        assert_eq!(album, "");
        assert_eq!(artist, "");
    }

    #[test]
    fn test_upload_song_rejects_missing_path() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let client = NcmClient::new().unwrap();
        // 不存在的路径应返回 Session 错误而非 panic
        let err = rt.block_on(client.upload_song(std::path::Path::new("/nonexistent/file.mp3")));
        assert!(err.is_err());
    }
}
