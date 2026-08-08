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
