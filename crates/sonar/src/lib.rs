pub mod crypto;
pub mod error;
pub mod model;
pub mod provider;
pub mod search;
pub mod util;

pub use error::{SonarError, Result};
pub use model::{
    SonarSource, PlayUrlResult, Quality, SearchQuery, SearchResult, Song, SongMeta, make_song_id,
    is_sonar_song_id,
};
pub use provider::SonarProvider;
pub use search::{SonarFinder, SearchConfig, SearchMode, quick_search, quick_search_with_mode};
