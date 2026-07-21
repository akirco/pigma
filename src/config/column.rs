use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// Lightweight discriminant for `ContentState`, used by `ColumnsConfig::for_content`
/// so that the config layer does not depend on runtime state types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentType {
    Songs,
    SongLists,
    TopLists,
    HotSearch,
    Singers,
    Empty,
}

/// Defines a single table column for config-driven rendering.
///
/// `field` is the serde field name (e.g. "name", "singer", "duration").
/// Missing fields render as `"—"` with a one-time `log::warn`.
///
/// TOML format (inline tables):
/// ```toml
/// columns = [
///     { header = "歌单", field = "name",   min_width = 20 },
///     { header = "作者", field = "author", width = 16 },
/// ]
/// ```
pub struct ColumnDef {
    pub header: String,
    pub field: String,
    pub width: Option<u16>,
    pub min_width: Option<u16>,
    pub ratio: Option<(u32, u32)>,
}

impl Serialize for ColumnDef {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let n = 3
            + usize::from(self.width.is_some())
            + usize::from(self.min_width.is_some())
            + usize::from(self.ratio.is_some());
        let mut map = serializer.serialize_map(Some(n))?;
        map.serialize_entry("header", &self.header)?;
        map.serialize_entry("field", &self.field)?;
        if let Some(v) = self.width {
            map.serialize_entry("width", &v)?;
        }
        if let Some(v) = self.min_width {
            map.serialize_entry("min_width", &v)?;
        }
        if let Some(v) = &self.ratio {
            map.serialize_entry("ratio", v)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for ColumnDef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::{self, MapAccess, Visitor};
        use std::fmt;

        struct ColumnDefVisitor;

        impl<'de> Visitor<'de> for ColumnDefVisitor {
            type Value = ColumnDef;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a column definition map")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut header = None;
                let mut field = None;
                let mut width = None;
                let mut min_width = None;
                let mut ratio = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "header" => header = Some(map.next_value()?),
                        "field" => field = Some(map.next_value()?),
                        "width" => width = Some(map.next_value()?),
                        "min_width" => min_width = Some(map.next_value()?),
                        "ratio" => ratio = Some(map.next_value()?),
                        _ => {
                            let _ = map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }

                let header = header.ok_or_else(|| de::Error::missing_field("header"))?;
                let field = field.ok_or_else(|| de::Error::missing_field("field"))?;

                Ok(ColumnDef {
                    header,
                    field,
                    width,
                    min_width,
                    ratio,
                })
            }
        }

        deserializer.deserialize_map(ColumnDefVisitor)
    }
}

impl std::fmt::Debug for ColumnDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ColumnDef")
            .field("header", &self.header)
            .field("field", &self.field)
            .field("width", &self.width)
            .field("min_width", &self.min_width)
            .field("ratio", &self.ratio)
            .finish()
    }
}

impl Clone for ColumnDef {
    fn clone(&self) -> Self {
        Self {
            header: self.header.clone(),
            field: self.field.clone(),
            width: self.width,
            min_width: self.min_width,
            ratio: self.ratio,
        }
    }
}

impl ColumnDef {
    pub fn to_constraint(&self) -> ratatui::layout::Constraint {
        if let Some(n) = self.width {
            ratatui::layout::Constraint::Length(n)
        } else if let Some(n) = self.min_width {
            ratatui::layout::Constraint::Min(n)
        } else if let Some((a, b)) = self.ratio {
            ratatui::layout::Constraint::Ratio(a, b)
        } else {
            ratatui::layout::Constraint::Min(12)
        }
    }
}

pub fn default_song_columns() -> Vec<ColumnDef> {
    vec![
        ColumnDef {
            header: "TITLE".into(),
            field: "name".into(),
            width: None,
            min_width: Some(18),
            ratio: None,
        },
        ColumnDef {
            header: "ARTIST".into(),
            field: "singer".into(),
            width: Some(16),
            min_width: None,
            ratio: None,
        },
        ColumnDef {
            header: "ALBUM".into(),
            field: "album".into(),
            width: None,
            min_width: Some(12),
            ratio: None,
        },
        ColumnDef {
            header: "DURATION".into(),
            field: "duration".into(),
            width: Some(9),
            min_width: None,
            ratio: None,
        },
    ]
}

pub fn default_songlist_columns() -> Vec<ColumnDef> {
    vec![
        ColumnDef {
            header: "NAME".into(),
            field: "name".into(),
            width: None,
            min_width: Some(20),
            ratio: None,
        },
        ColumnDef {
            header: "AUTHOR".into(),
            field: "author".into(),
            width: Some(16),
            min_width: None,
            ratio: None,
        },
    ]
}

pub fn default_toplist_columns() -> Vec<ColumnDef> {
    vec![
        ColumnDef {
            header: "NAME".into(),
            field: "name".into(),
            width: Some(20),
            min_width: None,
            ratio: None,
        },
        ColumnDef {
            header: "DESCRIPTION".into(),
            field: "description".into(),
            width: None,
            min_width: Some(20),
            ratio: None,
        },
    ]
}

pub fn default_hotsearch_columns() -> Vec<ColumnDef> {
    vec![ColumnDef {
        header: "HOT SEARCH".into(),
        field: "keyword".into(),
        width: None,
        min_width: Some(1),
        ratio: None,
    }]
}

pub fn default_download_columns() -> Vec<ColumnDef> {
    vec![
        ColumnDef {
            header: "TITLE".into(),
            field: "name".into(),
            width: None,
            min_width: Some(18),
            ratio: None,
        },
        ColumnDef {
            header: "ARTIST".into(),
            field: "singer".into(),
            width: Some(16),
            min_width: None,
            ratio: None,
        },
    ]
}

pub fn default_singer_columns() -> Vec<ColumnDef> {
    vec![
        ColumnDef {
            header: "ARTIST".into(),
            field: "name".into(),
            width: None,
            min_width: Some(16),
            ratio: None,
        },
        ColumnDef {
            header: "ID".into(),
            field: "id".into(),
            width: Some(10),
            min_width: None,
            ratio: None,
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnsConfig {
    #[serde(default = "default_song_columns")]
    pub songs: Vec<ColumnDef>,
    #[serde(default = "default_songlist_columns")]
    pub songlist: Vec<ColumnDef>,
    /// Per-API overrides. Key is the API endpoint string (e.g. "toplist", "search").
    /// If set, these columns are used instead of the type-based defaults.
    #[serde(default)]
    pub overrides: HashMap<String, Vec<ColumnDef>>,
}

impl Default for ColumnsConfig {
    fn default() -> Self {
        let mut overrides = HashMap::new();
        overrides.insert("toplist".into(), default_toplist_columns());
        overrides.insert("hotsearch".into(), default_hotsearch_columns());
        overrides.insert("__download__".into(), default_download_columns());
        Self {
            songs: default_song_columns(),
            songlist: default_songlist_columns(),
            overrides,
        }
    }
}

impl ColumnsConfig {
    pub fn for_content(&self, content_type: ContentType, api: Option<&str>) -> &[ColumnDef] {
        match content_type {
            ContentType::Songs => {
                if let Some(api) = api
                    && let Some(cols) = self.overrides.get(api)
                {
                    return cols;
                }
                &self.songs
            }
            ContentType::SongLists | ContentType::TopLists => {
                if let Some(api) = api
                    && let Some(cols) = self.overrides.get(api)
                {
                    return cols;
                }
                &self.songlist
            }
            ContentType::HotSearch => {
                if let Some(api) = api
                    && let Some(cols) = self.overrides.get(api)
                {
                    return cols;
                }
                HOTSEARCH_FALLBACK.get_or_init(default_hotsearch_columns)
            }
            ContentType::Singers => {
                if let Some(api) = api
                    && let Some(cols) = self.overrides.get(api)
                {
                    return cols;
                }
                SINGER_FALLBACK.get_or_init(default_singer_columns)
            }
            _ => &[],
        }
    }
}

static HOTSEARCH_FALLBACK: OnceLock<Vec<ColumnDef>> = OnceLock::new();
static SINGER_FALLBACK: OnceLock<Vec<ColumnDef>> = OnceLock::new();
