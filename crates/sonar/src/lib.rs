//! Sonar: search audio across Kugou, Kuwo, Bilibili and YouTube without cookies.
//!
//! The public entry points are [`SonarFinder`] (multi-provider search + ranking)
//! and the [`quick_search`] / [`quick_search_with_mode`] helpers. Each source is
//! a [`SonarProvider`]; [`model`] holds the shared types ([`Song`], [`SearchQuery`],
//! [`SearchResult`], [`PlayUrlResult`]).

pub mod crypto;
pub mod error;
pub mod model;
pub mod provider;
pub mod search;
pub mod util;

pub use error::{Result, SonarError};
pub use model::{
    PlayUrlResult, Quality, SearchQuery, SearchResult, SonarSource, Song, SongMeta,
    is_sonar_song_id, make_song_id,
};
pub use provider::SonarProvider;
pub use search::{SearchConfig, SearchMode, SonarFinder, quick_search, quick_search_with_mode};
