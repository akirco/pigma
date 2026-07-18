use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::sync::OnceLock;

use crate::{
    logger::Logger,
    theme::Theme,
    types::{
        ApiEndpoint, ColumnDef, ContentState, default_hotsearch_columns, default_singer_columns,
        default_song_columns, default_songlist_columns, default_toplist_columns,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub default_theme: String,
    pub bordered: bool,
    pub border_rounded: bool,
    pub seek_interval_secs: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub themes: Vec<Theme>,
    pub logger: Logger,
    pub navigation: NavConfig,
    /// Content cache TTL in seconds (0 to disable).
    #[serde(default = "default_content_cache_ttl")]
    pub content_cache_ttl: u64,
    #[serde(default)]
    pub playerbar: PlayerbarConfig,
    #[serde(default)]
    pub titles: TitlesConfig,
    #[serde(default)]
    pub columns: ColumnsConfig,
    /// 歌词高亮渐变风格：warm / cubehelix / rainbow / spectral / viridis / turbo。
    /// 未知值回退到 warm。
    #[serde(default = "default_lyric_gradient")]
    pub lyric_gradient: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TitlesConfig {
    #[serde(default = "default_title_sidebar")]
    pub sidebar: String,
    #[serde(default = "default_title_playlist")]
    pub playlist: String,
    #[serde(default = "default_title_lyrics")]
    pub lyrics: String,
}

fn default_title_sidebar() -> String {
    "NAVIGATION".into()
}
fn default_title_playlist() -> String {
    "\u{266a} QUEUE ({count})".into()
}
fn default_title_lyrics() -> String {
    "\u{266a} LYRICS".into()
}

impl Default for TitlesConfig {
    fn default() -> Self {
        Self {
            sidebar: default_title_sidebar(),
            playlist: default_title_playlist(),
            lyrics: default_title_lyrics(),
        }
    }
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
        overrides.insert("search".into(), default_hotsearch_columns());
        Self {
            songs: default_song_columns(),
            songlist: default_songlist_columns(),
            overrides,
        }
    }
}

impl ColumnsConfig {
    pub fn for_content(&self, content: &ContentState, api: Option<&str>) -> &[ColumnDef] {
        match content {
            ContentState::Songs(_) => &self.songs,
            ContentState::SongLists(_) | ContentState::TopLists(_) => {
                if let Some(api) = api
                    && let Some(cols) = self.overrides.get(api)
                {
                    return cols;
                }
                &self.songlist
            }
            ContentState::HotSearch(_) => {
                if let Some(api) = api
                    && let Some(cols) = self.overrides.get(api)
                {
                    return cols;
                }
                // Built-in fallback for HotSearch when no override configured
                HOTSEARCH_FALLBACK.get_or_init(|| {
                    vec![ColumnDef {
                        header: "HOT SEARCH".into(),
                        field: "keyword".into(),
                        width: None,
                        min_width: Some(1),
                        ratio: None,
                    }]
                })
            }
            ContentState::Singers(_) => {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerbarConfig {
    #[serde(default = "default_pb_filled_symbol")]
    pub filled_symbol: String,
    #[serde(default = "default_pb_unfilled_symbol")]
    pub unfilled_symbol: String,
    #[serde(default = "default_pb_filled_color")]
    pub filled_color: String,
    #[serde(default = "default_pb_unfilled_color")]
    pub unfilled_color: String,
    #[serde(default = "default_pb_unfilled_color_cached")]
    pub unfilled_color_cached: String,
}

fn default_pb_filled_symbol() -> String {
    "━".into()
}
fn default_pb_unfilled_symbol() -> String {
    "─".into()
}
fn default_pb_filled_color() -> String {
    "accent".into()
}
fn default_pb_unfilled_color() -> String {
    "text".into()
}
fn default_pb_unfilled_color_cached() -> String {
    "muted".into()
}

impl Default for PlayerbarConfig {
    fn default() -> Self {
        Self {
            filled_symbol: default_pb_filled_symbol(),
            unfilled_symbol: default_pb_unfilled_symbol(),
            filled_color: default_pb_filled_color(),
            unfilled_color: default_pb_unfilled_color(),
            unfilled_color_cached: default_pb_unfilled_color_cached(),
        }
    }
}

fn default_content_cache_ttl() -> u64 {
    300
}

fn default_lyric_gradient() -> String {
    "rainbow".into()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_theme: Theme::default().name,
            bordered: true,
            border_rounded: false,
            seek_interval_secs: 15,
            themes: Vec::new(),
            logger: Logger::default(),
            navigation: NavConfig::default(),
            content_cache_ttl: 300,
            playerbar: PlayerbarConfig::default(),
            titles: TitlesConfig::default(),
            columns: ColumnsConfig::default(),
            lyric_gradient: default_lyric_gradient(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavConfig {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<NavSectionConfig>,
}

impl Default for NavConfig {
    fn default() -> Self {
        Self {
            sections: vec![
                NavSectionConfig {
                    title: "<accent>▎</accent> <b>DISCOVER</b>".into(),
                    items: vec![
                        NavItemConfig {
                            name: "每日推荐".into(),
                            api: Some("recommend_songs".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: "推荐歌单".into(),
                            api: Some("recommend_resource".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: "排行榜".into(),
                            api: Some("toplist".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: "歌单".into(),
                            api: Some("top_song_list".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: "电台".into(),
                            api: Some("user_radio_sublist".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: "搜索".into(),
                            api: Some("search".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: "热门歌手".into(),
                            api: Some("top_singers".into()),
                            title_template: None,
                        },
                    ],
                },
                NavSectionConfig {
                    title: "<accent>▎</accent> <b>MY MUSIC</b>".into(),
                    items: vec![
                        NavItemConfig {
                            name: "我的音乐云盘".into(),
                            api: Some("user_cloud_disk".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: "我喜欢的音乐".into(),
                            api: Some("__liked__".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: "我的歌单".into(),
                            api: Some("user_song_list".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: "下载管理".into(),
                            api: Some("__download__".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: "本地音乐".into(),
                            api: Some("__local_music__".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: "最近播放".into(),
                            api: Some("__recent__".into()),
                            title_template: None,
                        },
                    ],
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavSectionConfig {
    pub title: String,
    pub items: Vec<NavItemConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavItemConfig {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
    /// Optional title template. Supports `{name}` (item name), `{count}` (item count).
    /// If None, defaults to `"{name} ({count})"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_template: Option<String>,
}

impl NavItemConfig {
    pub fn endpoint(&self) -> Option<ApiEndpoint> {
        self.api.as_deref().and_then(ApiEndpoint::parse)
    }
}

impl Config {
    pub fn load() -> Self {
        let config_dir = dirs::config_dir().map(|d| d.join("pigma"));
        let config_path = config_dir.as_ref().map(|d| d.join("config.toml"));

        let default = Config::default();
        let config = if let Some(path) = &config_path {
            if path.exists() {
                match fs::read_to_string(path) {
                    Ok(content) => match toml::from_str(&content) {
                        Ok(cfg) => cfg,
                        Err(e) => {
                            log::warn!("Failed to parse config.toml: {e}, using defaults");
                            default.clone()
                        }
                    },
                    Err(e) => {
                        log::warn!("Failed to read config.toml: {e}, using defaults");
                        default.clone()
                    }
                }
            } else {
                default.clone()
            }
        } else {
            default.clone()
        };

        if let Some(dir) = &config_dir
            && !dir.join("config.toml").exists()
        {
            let _ = fs::create_dir_all(dir);
            let content = config.to_toml();
            if let Err(e) = fs::write(dir.join("config.toml"), content) {
                log::warn!("Failed to write default config: {e}");
            }
        }
        config
    }

    pub fn save(&self) {
        if let Some(dir) = dirs::config_dir().map(|d| d.join("pigma")) {
            if let Err(e) = fs::create_dir_all(&dir) {
                log::error!("Failed to create config directory: {e}");
                return;
            }
            let content = self.to_toml();
            if let Err(e) = fs::write(dir.join("config.toml"), content) {
                log::error!("Failed to write config.toml: {e}");
            }
        }
    }

    fn to_toml(&self) -> String {
        toml::to_string_pretty(self).expect("Config should always serialize to valid TOML")
    }
}
