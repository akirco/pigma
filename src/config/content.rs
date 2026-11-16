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

use serde::{Deserialize, Deserializer, Serialize};

use super::ContentType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentStyle {
    /// Original table with configured columns.
    #[default]
    Table,
    /// Two-column cover cards.
    Cards,
}

impl ContentStyle {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "table" => Some(Self::Table),
            "cards" => Some(Self::Cards),
            _ => None,
        }
    }
}

/// Unknown styles fall back to the table instead of failing the whole config.
impl<'de> Deserialize<'de> for ContentStyle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::from_name(&raw).unwrap_or_default())
    }
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
