use reqwest::header::{HeaderMap, HeaderValue, REFERER, USER_AGENT};
use stream_download::http::{Client, RANGE_HEADER_KEY, format_range_header_bytes};

/// Prevents a bare reqwest client from being rejected by CDNs.
const BROWSER_UA: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:153.0) Gecko/20100101 Firefox/153.0";

/// A [`stream_download::http::Client`] that decorates every request with browser-style headers.
/// Bilibili's `upos-*` CDN hosts return 403 when the browser User-Agent and
/// `Referer: https://www.bilibili.com` are missing.
///
/// The wrapped [`reqwest::Client`] is injected externally so callers can control the proxy;
/// `create()` (required by the trait, no arguments) falls back to a bare client and is only
/// used by `StreamDownload::new`. Prefer [`HeadersClient::new`] together with
/// [`StreamDownload::from_stream`] instead.
#[derive(Clone)]
pub struct HeadersClient {
    inner: reqwest::Client,
}

impl HeadersClient {
    pub(super) fn new(inner: reqwest::Client) -> Self {
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
