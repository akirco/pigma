use reqwest::header::{HeaderMap, HeaderValue, REFERER, USER_AGENT};
use stream_download::http::{Client, RANGE_HEADER_KEY, format_range_header_bytes};

/// 避免裸 reqwest 客户端被 CDN 拒绝请求。
const BROWSER_UA: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:153.0) Gecko/20100101 Firefox/153.0";

/// 一个 [`stream_download::http::Client`]，为每个请求装饰浏览器风格的头部。
/// Bilibili 的 `upos-*` CDN 主机在缺少浏览器 User-Agent 和
/// `Referer: https://www.bilibili.com` 时会返回 403。
///
/// 被封装的 [`reqwest::Client`] 由外部注入，以便调用者可以控制代理；
/// `create()`（trait 所要求的，无参数）回退到裸客户端，且仅由
/// `StreamDownload::new` 使用。推荐使用 [`HeadersClient::new`] 配合
/// [`StreamDownload::from_stream`] 代替。
#[derive(Clone)]
pub struct HeadersClient {
    inner: reqwest::Client,
}

impl HeadersClient {
    pub fn new(inner: reqwest::Client) -> Self {
        Self { inner }
    }

    fn request(&self, url: &reqwest::Url) -> reqwest::RequestBuilder {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(BROWSER_UA));
        if let Some(host) = url.host_str() {
            let is_bili = host.ends_with("bilivideo.com")
                || host.ends_with(".hdslb.com")
                || host.ends_with(".mountaintoys.cn")
                || host.contains("bilibili")
                || host.contains("bilivideo");
            if is_bili {
                headers.insert(
                    REFERER,
                    HeaderValue::from_static("https://www.bilibili.com"),
                );
            }
        }
        self.inner.get(url.clone()).headers(headers)
    }
}

impl Client for HeadersClient {
    type Url = reqwest::Url;
    type Response = reqwest::Response;
    type Error = reqwest::Error;
    type Headers = HeaderMap;

    fn create() -> Self {
        Self {
            inner: reqwest::Client::new(),
        }
    }

    async fn get(&self, url: &Self::Url) -> Result<Self::Response, Self::Error> {
        self.request(url).send().await
    }

    async fn get_range(
        &self,
        url: &Self::Url,
        start: u64,
        end: Option<u64>,
    ) -> Result<Self::Response, Self::Error> {
        self.request(url)
            .header(RANGE_HEADER_KEY, format_range_header_bytes(start, end))
            .send()
            .await
    }
}
