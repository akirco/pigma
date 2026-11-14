//! Per-content-type presentation style.
//!
//! The default is the original table; users can opt individual content types
//! into the two-column cover cards:
//!
//! ```toml
//! [content]
//! songs = "table"
//! playlists = "cards"
//! singers = "cards"
//! hot_search = "table"
//! ```

use serde::{Deserialize, Serialize};

use super::ContentType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentStyle {
    /// Original table with configured columns.
    #[default]
    Table,
    /// Two-column cover cards.
    Cards,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ContentConfig {
    pub songs: ContentStyle,
    /// Applies to playlists and toplists, which are both collections.
    #[serde(alias = "songlists")]
    pub playlists: ContentStyle,
    pub singers: ContentStyle,
    #[serde(alias = "hotsearch")]
    pub hot_search: ContentStyle,
}

impl ContentConfig {
    pub fn style_for(&self, content_type: ContentType) -> ContentStyle {
        match content_type {
            ContentType::Songs => self.songs,
            ContentType::SongLists | ContentType::TopLists => self.playlists,
            ContentType::Singers => self.singers,
            ContentType::HotSearch => self.hot_search,
            ContentType::Empty => ContentStyle::Table,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_table_for_every_content_type() {
        let config = ContentConfig::default();
        assert_eq!(config.style_for(ContentType::Songs), ContentStyle::Table);
        assert_eq!(
            config.style_for(ContentType::SongLists),
            ContentStyle::Table
        );
        assert_eq!(config.style_for(ContentType::TopLists), ContentStyle::Table);
        assert_eq!(config.style_for(ContentType::Singers), ContentStyle::Table);
        assert_eq!(
            config.style_for(ContentType::HotSearch),
            ContentStyle::Table
        );
    }

    #[test]
    fn playlists_cover_songlists_and_toplists() {
        let config = ContentConfig {
            playlists: ContentStyle::Cards,
            ..ContentConfig::default()
        };
        assert_eq!(
            config.style_for(ContentType::SongLists),
            ContentStyle::Cards
        );
        assert_eq!(config.style_for(ContentType::TopLists), ContentStyle::Cards);
        assert_eq!(config.style_for(ContentType::Songs), ContentStyle::Table);
    }
}
