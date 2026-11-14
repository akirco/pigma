//! Centralized data-loading service (`ApiService`): resolves NetEase Cloud Music
//! API endpoints (see [`ApiEndpoint`]), applies caching, and maps
//! errors to events.
//!
//! The methods are grouped by concern in submodules:
//! - `content`: navigation endpoints, playlist/album/artist pages, `load_more`
//! - `liked`: liked songs and the lazy playlist `trackIds` cache
//! - `lyrics`: NetEase + sonar lyrics with cache integration
//! - `library`: local music library (`~/Music` scan)
//! - `search`: NCM song search, shared TUI/IPC execution and result registries
//!
//! `api` holds `ApiService` itself, its thin NCM passthroughs (login, song
//! URLs, heartbeat, cloud upload) and the shared mapping helpers.

mod api;
mod content;
mod content_cache;
mod endpoint;
mod library;
mod liked;
mod lyrics;
mod search;

pub use api::ApiService;
pub use content_cache::ContentCache;
pub use endpoint::ApiEndpoint;
pub use search::{SearchEngine, SearchHit, SearchResults};
pub(crate) use search::{search_ncm, search_sonar};
