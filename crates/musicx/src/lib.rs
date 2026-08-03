pub mod crypto;
pub mod error;
pub mod model;
pub mod provider;
pub mod search;
pub mod util;

pub use error::{MusicError, Result};
pub use model::{
    Album, Artist, MusicSource, PlayUrlResult, Quality, SearchQuery, SearchResult, Song,
};
pub use provider::MusicProvider;
pub use search::{MusicFinder, SearchConfig, SearchMode, quick_search, quick_search_with_mode};
