mod border;
mod column;
mod navigation;
mod playerbar;
pub mod theme;
mod titles;

pub use border::*;
pub use column::*;
pub use navigation::*;
pub use playerbar::*;
pub use theme::{Theme, ThemeRegistry};
pub use titles::*;

use serde::{Deserialize, Serialize};
use std::fs;

use crate::logger::Logger;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub default_theme: String,
    pub border: BorderConfig,
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
    /// 边听边存缓存目录（绝对路径或相对于 ~/.cache/pigma/ 的路径）。
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,
    /// 边听边存音质等级：standard / higher / exhigh / lossless / hires。
    #[serde(default = "default_quality")]
    pub quality: String,
    /// 缓存文件命名模板。变量：{id} {name} {singer} {album}。
    /// 例："{name}-{singer}"
    #[serde(default = "default_cache_template")]
    pub cache_template: String,
    /// YouTube fallback 代理地址（留空则不使用代理）。
    #[serde(default = "default_proxy")]
    pub proxy: String,
    /// 搜索结果数量上限。
    #[serde(default = "default_search_limit")]
    pub search_limit: u16,
}

fn default_content_cache_ttl() -> u64 {
    300
}

fn default_lyric_gradient() -> String {
    "rainbow".into()
}

fn default_quality() -> String {
    "standard".into()
}

fn default_cache_dir() -> String {
    "downloads".into()
}

fn default_cache_template() -> String {
    "{name}-{singer}".into()
}

fn default_proxy() -> String {
    "http://127.0.0.1:7890".into()
}

fn default_search_limit() -> u16 {
    100
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_theme: Theme::default().name,
            border: BorderConfig::default(),
            seek_interval_secs: 15,
            themes: Vec::new(),
            logger: Logger::default(),
            navigation: NavConfig::default(),
            content_cache_ttl: 300,
            playerbar: PlayerbarConfig::default(),
            titles: TitlesConfig::default(),
            columns: ColumnsConfig::default(),
            lyric_gradient: default_lyric_gradient(),
            cache_dir: default_cache_dir(),
            quality: default_quality(),
            cache_template: default_cache_template(),
            proxy: default_proxy(),
            search_limit: default_search_limit(),
        }
    }
}

fn fix_spacing(s: &str) -> String {
    s.lines()
        .map(|line| {
            if let Some(idx) = line.find("= ") {
                let key = &line[..idx];
                if !key.ends_with(' ') && !key.is_empty() {
                    return format!("{} = {}", key, &line[idx + 2..]);
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn inline_items(table: &mut toml_edit::Table) {
    if let Some(toml_edit::Item::ArrayOfTables(aot)) = table.remove("items") {
        let mut arr = toml_edit::Array::new();
        for tbl in aot {
            let mut inline = toml_edit::InlineTable::new();
            for (k, v) in tbl.iter() {
                if let toml_edit::Item::Value(val) = v {
                    inline.insert(k, val.clone());
                }
            }
            arr.push(toml_edit::Value::InlineTable(inline));
        }
        table.insert(
            "items",
            toml_edit::Item::Value(toml_edit::Value::Array(arr)),
        );
    }
}

fn convert_items_to_inline(item: &mut toml_edit::Item) {
    match item {
        toml_edit::Item::Table(table) => {
            let keys: Vec<String> = table.iter().map(|(k, _)| k.to_string()).collect();
            for key in &keys {
                if let Some(child) = table.get_mut(key.as_str()) {
                    convert_items_to_inline(child);
                }
            }
            inline_items(table);
        }
        toml_edit::Item::ArrayOfTables(aot) => {
            for tbl in aot.iter_mut() {
                inline_items(tbl);
                let keys: Vec<String> = tbl.iter().map(|(k, _)| k.to_string()).collect();
                for key in &keys {
                    if let Some(child) = tbl.get_mut(key.as_str()) {
                        convert_items_to_inline(child);
                    }
                }
            }
        }
        _ => {}
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
                            default
                        }
                    },
                    Err(e) => {
                        log::warn!("Failed to read config.toml: {e}, using defaults");
                        default
                    }
                }
            } else {
                default
            }
        } else {
            default
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
        let pretty =
            toml::to_string_pretty(self).expect("Config should always serialize to valid TOML");
        let mut doc: toml_edit::DocumentMut = pretty
            .parse()
            .expect("toml::to_string_pretty should produce valid TOML");
        doc.fmt();
        convert_items_to_inline(doc.as_item_mut());
        fix_spacing(&doc.to_string())
    }
}
