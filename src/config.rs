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

impl Default for Config {
    fn default() -> Self {
        Self {
            default_theme: Theme::default().name,
            bordered: true,
            border_rounded: false,
            seek_interval_secs: 15,
            themes: vec![
                Theme::default(),
                Theme::terminal(),
                Theme::dracula(),
                Theme::nord(),
                Theme::gruvbox_dark(),
                Theme::solarized_dark(),
                Theme::tokyo_night(),
                Theme::catppuccin_mocha(),
                Theme::one_dark(),
                Theme::monokai(),
                Theme::rose_pine(),
                Theme::kanagawa(),
            ],
            logger: Logger::default(),
            navigation: NavConfig::default(),
            content_cache_ttl: 300,
            playerbar: PlayerbarConfig::default(),
            titles: TitlesConfig::default(),
            columns: ColumnsConfig::default(),
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
                    title: "DISCOVER".into(),
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
                    title: "MY MUSIC".into(),
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
        let mut o = String::with_capacity(2048);

        o.push_str(&format!("default_theme = \"{}\"\n", self.default_theme));
        o.push_str(&format!("bordered = {}\n", self.bordered));
        o.push_str(&format!("border_rounded = {}\n", self.border_rounded));
        o.push_str(&format!(
            "seek_interval_secs = {}\n",
            self.seek_interval_secs
        ));
        o.push_str(&format!("content_cache_ttl = {}\n", self.content_cache_ttl));
        o.push('\n');

        // themes — use toml crate for complex ratatui::Color serialization
        for theme in &self.themes {
            if let Ok(s) = toml::to_string_pretty(theme) {
                o.push_str("[[themes]]\n");
                o.push_str(&s);
                o.push('\n');
            }
        }

        o.push_str("[logger]\n");
        o.push_str(&format!(
            "log_level = \"{}\"\n",
            format_log_level(self.logger.log_level)
        ));
        o.push('\n');

        // playerbar
        o.push_str("[playerbar]\n");
        o.push_str(&format!(
            "filled_symbol = \"{}\"\n",
            self.playerbar.filled_symbol
        ));
        o.push_str(&format!(
            "unfilled_symbol = \"{}\"\n",
            self.playerbar.unfilled_symbol
        ));
        o.push_str(&format!(
            "filled_color = \"{}\"\n",
            self.playerbar.filled_color
        ));
        o.push_str(&format!(
            "unfilled_color = \"{}\"\n",
            self.playerbar.unfilled_color
        ));
        o.push_str(&format!(
            "unfilled_color_cached = \"{}\"\n",
            self.playerbar.unfilled_color_cached
        ));
        o.push('\n');

        // titles
        o.push_str("[titles]\n");
        o.push_str(&format!("sidebar = \"{}\"\n", self.titles.sidebar));
        o.push_str(&format!("playlist = \"{}\"\n", self.titles.playlist));
        o.push_str(&format!("lyrics = \"{}\"\n", self.titles.lyrics));
        o.push('\n');

        // columns
        o.push_str("[columns]\n");
        o.push_str(&format!("songs = [{}]\n", fmt_columns(&self.columns.songs)));
        o.push_str(&format!(
            "songlist = [{}]\n",
            fmt_columns(&self.columns.songlist)
        ));
        if !self.columns.overrides.is_empty() {
            o.push_str("[columns.overrides]\n");
            let mut keys: Vec<&String> = self.columns.overrides.keys().collect();
            keys.sort();
            for key in keys {
                if let Some(cols) = self.columns.overrides.get(key) {
                    o.push_str(&format!("{key} = [{}]\n", fmt_columns(cols)));
                }
            }
        }
        o.push('\n');

        // navigation — manual format with inline columns
        o.push_str("[navigation]\n\n");
        for section in &self.navigation.sections {
            o.push_str("[[navigation.sections]]\n");
            o.push_str(&format!("title = \"{}\"\n\n", section.title));
            for item in &section.items {
                o.push_str("[[navigation.sections.items]]\n");
                o.push_str(&format!("name = \"{}\"\n", item.name));
                if let Some(ref api) = item.api {
                    o.push_str(&format!("api = \"{}\"\n", api));
                }
                if let Some(ref template) = item.title_template {
                    o.push_str(&format!("title_template = \"{}\"\n", template));
                } else {
                    o.push_str("title_template = \"{name} ({count})\"\n");
                }
                o.push('\n');
            }
        }

        o
    }
}

fn format_log_level(level: log::Level) -> &'static str {
    match level {
        log::Level::Error => "error",
        log::Level::Warn => "warn",
        log::Level::Info => "info",
        log::Level::Debug => "debug",
        log::Level::Trace => "trace",
    }
}

fn format_column_def(c: &ColumnDef) -> String {
    let mut parts = vec![
        format!("header = \"{}\"", c.header),
        format!("field = \"{}\"", c.field),
    ];
    if let Some(v) = c.width {
        parts.push(format!("width = {}", v));
    }
    if let Some(v) = c.min_width {
        parts.push(format!("min_width = {}", v));
    }
    if let Some((a, b)) = c.ratio {
        parts.push(format!("ratio = [{}, {}]", a, b));
    }
    format!("{{ {} }}", parts.join(", "))
}

fn fmt_columns(cols: &[ColumnDef]) -> String {
    cols.iter()
        .map(format_column_def)
        .collect::<Vec<_>>()
        .join(", ")
}
