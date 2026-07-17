use crossterm::event::Event as CrosstermEvent;
use ncm_api::{LoginInfo, SingerInfo, SongInfo, SongList, TopList};
use serde::{Deserialize, Serialize};

use std::time::Duration;

use crate::state::{CommandAction, Page, PlaybackLyricLine as LyricLine, SplashLogEntry};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApiEndpoint {
    RecommendSongs,
    RecommendResource,
    Toplist,
    TopSongList,
    UserRadioSublist,
    UserCloudDisk,
    LikedSongs,
    UserSongList,
    Download,
    LocalMusic,
    Recent,
    Search,
    TopSingers,
}

impl ApiEndpoint {
    pub const ALL: &'static [ApiEndpoint] = &[
        ApiEndpoint::RecommendSongs,
        ApiEndpoint::RecommendResource,
        ApiEndpoint::Toplist,
        ApiEndpoint::TopSongList,
        ApiEndpoint::UserRadioSublist,
        ApiEndpoint::UserCloudDisk,
        ApiEndpoint::LikedSongs,
        ApiEndpoint::UserSongList,
        ApiEndpoint::Download,
        ApiEndpoint::LocalMusic,
        ApiEndpoint::Recent,
        ApiEndpoint::Search,
        ApiEndpoint::TopSingers,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            ApiEndpoint::RecommendSongs => "recommend_songs",
            ApiEndpoint::RecommendResource => "recommend_resource",
            ApiEndpoint::Toplist => "toplist",
            ApiEndpoint::TopSongList => "top_song_list",
            ApiEndpoint::UserRadioSublist => "user_radio_sublist",
            ApiEndpoint::UserCloudDisk => "user_cloud_disk",
            ApiEndpoint::LikedSongs => "__liked__",
            ApiEndpoint::UserSongList => "user_song_list",
            ApiEndpoint::Download => "__download__",
            ApiEndpoint::LocalMusic => "__local_music__",
            ApiEndpoint::Recent => "__recent__",
            ApiEndpoint::Search => "search",
            ApiEndpoint::TopSingers => "top_singers",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "recommend_songs" => Some(ApiEndpoint::RecommendSongs),
            "recommend_resource" => Some(ApiEndpoint::RecommendResource),
            "toplist" => Some(ApiEndpoint::Toplist),
            "top_song_list" => Some(ApiEndpoint::TopSongList),
            "user_radio_sublist" => Some(ApiEndpoint::UserRadioSublist),
            "user_cloud_disk" => Some(ApiEndpoint::UserCloudDisk),
            "__liked__" => Some(ApiEndpoint::LikedSongs),
            "user_song_list" => Some(ApiEndpoint::UserSongList),
            "__download__" => Some(ApiEndpoint::Download),
            "__local_music__" => Some(ApiEndpoint::LocalMusic),
            "__recent__" => Some(ApiEndpoint::Recent),
            "search" => Some(ApiEndpoint::Search),
            "top_singers" => Some(ApiEndpoint::TopSingers),
            _ => None,
        }
    }

    pub fn needs_login(&self) -> bool {
        matches!(self, ApiEndpoint::LikedSongs | ApiEndpoint::UserSongList)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ContentState {
    Empty,
    Loading,
    Error(String),
    Songs(Vec<SongInfo>),
    SongLists(Vec<SongList>),
    TopLists(Vec<TopList>),
    HotSearch(Vec<String>),
    Singers(Vec<SingerInfo>),
}

impl ContentState {
    pub fn len(&self) -> usize {
        match self {
            ContentState::Songs(s) => s.len(),
            ContentState::SongLists(l) => l.len(),
            ContentState::TopLists(l) => l.len(),
            ContentState::HotSearch(kw) => kw.len(),
            ContentState::Singers(s) => s.len(),
            _ => 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableMode {
    Row,
    Cell,
}

#[derive(Clone, Debug)]
pub enum Event {
    Crossterm(CrosstermEvent),
    App(AppEvent),
}

#[derive(Clone, Debug)]
pub enum AppEvent {
    Quit,
    SplashTick {
        progress: f64,
        log: Option<SplashLogEntry>,
    },
    Login,
    LoginSuccess(LoginInfo),
    LoginError(String),
    CaptchaSent,
    QRCreated {
        url: String,
        key: String,
    },
    QRStatus(String),
    NavSelect(String),
    ContentLoaded(ContentState),
    PlaylistSelect(u64),
    BreadcrumbSet(String),
    SongPlay(u64),
    PlaybackStarted,
    PlaybackProgress {
        position: Duration,
        total: Option<Duration>,
    },
    PlaybackFinished,
    PlaybackError(String),
    LyricsLoaded {
        song_id: u64,
        lyrics: Vec<LyricLine>,
        translated_lyrics: Vec<LyricLine>,
    },
    HeartbeatSong(SongInfo),
    HeartbeatFallback,
    SearchSong(String),
    LocalMusicLoaded(Vec<SongInfo>),
    SetOffline,
    Navigate(Page),
    CommandPanel(CommandPanelAction),
    SearchActivated,
    SearchDeactivated,
    ToggleBordered,
    ExecuteCommand(CommandAction),
    ContentRestore,
    CellAction(usize, usize),
}

#[derive(Clone, Debug)]
pub enum CommandPanelAction {
    Open,
    Close,
    Previous,
    Next,
    Select,
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
