mod border;
mod cache;
mod column;
mod navigation;
mod playerbar;
pub mod theme;
mod titles;

pub use border::*;
pub use cache::*;
pub use column::*;
pub use navigation::*;
pub use playerbar::*;
pub use theme::{Theme, ThemeRegistry};
pub use titles::*;

use serde::{Deserialize, Serialize};
use std::fs;

use crate::utils::GradientPreset;
use crate::{logger::Logger, utils};

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
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub playerbar: PlayerbarConfig,
    #[serde(default)]
    pub titles: TitlesConfig,
    #[serde(default)]
    pub columns: ColumnsConfig,
    /// 歌词高亮渐变风格：warm / cubehelix / rainbow / spectral / viridis / turbo。
    #[serde(default)]
    pub lyric_gradient: GradientPreset,
    /// 代理地址（留空则不使用代理）。
    #[serde(default = "default_proxy")]
    pub proxy: String,
    /// 代理目标：`normal` 仅代理 YouTube（默认，国内用户），`reversed` 除 YouTube 外
    /// 全部走代理（海外用户），`both` 全部走代理。
    #[serde(default = "default_proxy_target")]
    pub proxy_target: ProxyTarget,
    /// 搜索结果数量上限。
    #[serde(default = "default_search_limit")]
    pub search_limit: u16,
    /// 导航栏位置：left（左侧，默认）、right（右侧）、top（顶部）或 bottom（底部）。
    #[serde(default)]
    pub navigation_position: NavPosition,
    /// sonar 兜底源配置（NCM 播放失败时的多源兜底）。
    #[serde(default)]
    pub source_fallback: SonarConfig,
}

fn default_proxy() -> String {
    "http://127.0.0.1:7890".into()
}

fn default_proxy_target() -> ProxyTarget {
    ProxyTarget::Normal
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyTarget {
    /// 国内默认：仅 YouTube 走代理，其余直连。
    Normal,
    /// 海外用户：除 YouTube 外全部走代理。
    Reversed,
    /// 全部走代理。
    Both,
}

/// 导航栏位置：左侧（默认）、右侧、顶部或底部。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NavPosition {
    #[default]
    Left,
    Right,
    Top,
    Bottom,
}

fn default_search_limit() -> u16 {
    100
}

/// 兜底源配置（sonar 多源兜底）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SonarConfig {
    /// 是否启用兜底源。
    pub enabled: bool,
    /// 参与兜底的源，按优先级从高到低排列：
    /// `kuwo`, `kugou`, `bilivideo`, `youtube`。
    pub providers: Vec<String>,
    /// 单个源搜索超时（毫秒）。
    pub timeout_ms: u64,
}

impl Default for SonarConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            providers: vec![
                "kuwo".to_string(),
                "kugou".to_string(),
                "bilivideo".to_string(),
                "youtube".to_string(),
            ],
            timeout_ms: 10000,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_theme: Theme::default().name,
            border: BorderConfig::default(),
            seek_interval_secs: 15,
            lyric_gradient: GradientPreset::default(),
            proxy: default_proxy(),
            proxy_target: default_proxy_target(),
            search_limit: default_search_limit(),
            navigation_position: NavPosition::default(),
            logger: Logger::default(),
            cache: CacheConfig::default(),
            playerbar: PlayerbarConfig::default(),
            titles: TitlesConfig::default(),
            source_fallback: SonarConfig::default(),
            themes: Vec::new(),
            navigation: NavConfig::default(),
            columns: ColumnsConfig::default(),
        }
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
                    Ok(content) => match toml_edit::de::from_str(&content) {
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
        let mut doc = toml_edit::ser::to_string_pretty(self)
            .unwrap()
            .parse::<toml_edit::DocumentMut>()
            .unwrap();
        // navigation 设为隐式
        doc["navigation"].as_table_mut().unwrap().set_implicit(true);

        // 遍历每个 section，把 items 转为内联表数组
        let sections = doc["navigation"]["sections"]
            .as_array_of_tables_mut()
            .unwrap();

        for section in sections.iter_mut() {
            utils::format::convert_aot_to_inline(section, "items", "\n  ");
        }

        let columns = doc["columns"].as_table_mut().unwrap();
        columns.set_implicit(true);

        let overrides = columns["overrides"].as_table_mut().unwrap();
        overrides.set_implicit(true);

        utils::format::convert_all_aot_to_inline(overrides, "\n  ");

        let columns = doc["columns"].as_table_mut().unwrap();
        utils::format::convert_aot_to_inline(columns, "songs", "\n  ");
        utils::format::convert_aot_to_inline(columns, "songlist", "\n  ");
        columns.set_implicit(true);

        doc.to_string()
    }
}
