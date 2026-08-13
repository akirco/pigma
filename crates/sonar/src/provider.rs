use crate::error::Result;
use crate::model::{PlayUrlResult, Quality, SearchQuery, SearchResult, SonarSource, Song};
use async_trait::async_trait;
use reqwest::Client;

/// Default provider priorities (see [`SonarProvider::priority`]). Lower values
/// rank higher; the finder sorts providers by priority descending.
pub(crate) const PRIORITY_KUGOU: u8 = 10;
pub(crate) const PRIORITY_KUWO: u8 = 20;
pub(crate) const PRIORITY_YOUTUBE: u8 = 30;
pub(crate) const PRIORITY_BILIVIDEO: u8 = 40;

/// Build a `reqwest::Client` with the given user agent and an optional HTTP
/// proxy (empty `proxy_url` = direct connection). Fallible so callers can
/// surface an invalid proxy URL or a failed builder instead of panicking.
pub(crate) fn build_client(proxy_url: &str, user_agent: &str) -> Result<Client> {
    let mut builder = Client::builder().user_agent(user_agent);
    if !proxy_url.is_empty() {
        builder = builder.proxy(reqwest::Proxy::all(proxy_url)?);
    }
    Ok(builder.build()?)
}

/// A search backend that resolves songs (and optionally lyrics) from a single
/// third-party source. Implementors are constructed by [`crate::search::SonarFinder`]
/// and queried concurrently; see the trait methods for the per-provider contract.
#[async_trait]
pub trait SonarProvider: Send + Sync {
    /// Which [`SonarSource`] this provider represents.
    fn source(&self) -> SonarSource;

    /// Search the provider for songs matching `query`. Implementations should
    /// return an empty `songs` list (not an error) when nothing matches so the
    /// finder can fall back to other providers.
    async fn search(&self, query: &SearchQuery) -> Result<SearchResult>;

    /// Resolve a playable audio URL for `song` at `quality` (when the provider
    /// honours quality). Returns [`crate::error::SonarError::NoPlayUrl`] when the
    /// song cannot be played (e.g. VIP / copyright restricted).
    async fn get_play_url(&self, song: &Song, quality: Option<Quality>) -> Result<PlayUrlResult>;

    /// Fetch LRC lyrics for a song, if the provider offers them. Returns
    /// `Ok(None)` when no lyrics are available. The default implementation
    /// returns `None`; providers with lyrics override this.
    async fn get_lyrics(&self, song: &Song) -> Result<Option<String>> {
        let _ = song;
        Ok(None)
    }

    /// Whether this provider should be included in the active finder. The
    /// default is `true`; providers can disable themselves (e.g. on a missing
    /// runtime dependency) by returning `false`.
    fn enabled(&self) -> bool {
        true
    }

    /// Search/fallback priority (see the `PRIORITY_*` constants). Lower ranks
    /// higher; the finder sorts providers by priority descending so ties in
    /// match score break toward the preferred source.
    fn priority(&self) -> u8 {
        0
    }
}

pub mod bilivideo;
pub mod kugou;
pub mod kuwo;
pub mod youtube;
