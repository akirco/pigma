//! TOML configuration: the runtime `Config` plus the border/cache/column/
//! navigation/playerbar/theme registries.

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
pub use theme::{Theme, ThemeRegistry, theme_fallback};
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
    /// Lyrics highlight gradient style: warm / cubehelix / rainbow / spectral / viridis / turbo.
    #[serde(default)]
    pub lyric_gradient: GradientPreset,
    /// Proxy address (leave empty to disable the proxy).
    #[serde(default = "default_proxy")]
    pub proxy: String,
    /// Proxy target: `normal` proxies only YouTube (default, domestic users),
    /// `reversed` proxies everything except YouTube (overseas users),
    /// `both` proxies everything.
    #[serde(default = "default_proxy_target")]
    pub proxy_target: ProxyTarget,
    /// Maximum number of search results.
    #[serde(default = "default_search_limit")]
    pub search_limit: u16,
    /// Navigation bar position: left (default), right, top, or bottom.
    #[serde(default)]
    pub navigation_position: NavPosition,
    /// Minimum splash screen display time (seconds); auto-transition waits for this even if boot finishes instantly.
    #[serde(default = "default_splash_duration")]
    pub splash_duration_secs: f64,
    /// sonar fallback source config (multi-source fallback when NCM playback fails).
    #[serde(default)]
    pub source_fallback: SonarConfig,
    /// Default template for `pigma status` (plain format).
    #[serde(default = "default_cli_status_template")]
    pub cli_status_template: String,
    /// Default format for `pigma status`: `plain` or `json`.
    #[serde(default = "default_cli_status_format")]
    pub cli_status_format: String,
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
    /// Domestic default: only YouTube goes through the proxy; everything else connects directly.
    Normal,
    /// Overseas users: everything except YouTube goes through the proxy.
    Reversed,
    /// Everything goes through the proxy.
    Both,
}

/// Navigation bar position: left (default), right, top, or bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NavPosition {
    #[default]
    Left,
    Right,
    Top,
    Bottom,
}

impl NavPosition {
    /// The next position in the left → right → top → bottom cycle.
    pub fn cycle(self) -> Self {
        match self {
            NavPosition::Left => NavPosition::Right,
            NavPosition::Right => NavPosition::Top,
            NavPosition::Top => NavPosition::Bottom,
            NavPosition::Bottom => NavPosition::Left,
        }
    }

    /// Human-readable Chinese label used for toasts.
    pub fn label(self) -> &'static str {
        match self {
            NavPosition::Left => "左侧",
            NavPosition::Right => "右侧",
            NavPosition::Top => "顶部",
            NavPosition::Bottom => "底部",
        }
    }
}

fn default_search_limit() -> u16 {
    100
}

fn default_splash_duration() -> f64 {
    2.0
}

fn default_cli_status_template() -> String {
    "{name}  {artist}  {current}/{duration}  {status}  vol {volume}%".into()
}

fn default_cli_status_format() -> String {
    "plain".into()
}

/// Fallback source config (sonar multi-source fallback).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SonarConfig {
    /// Whether fallback sources are enabled.
    pub enabled: bool,
    /// Sources participating in fallback, ordered from highest to lowest priority:
    /// `kuwo`, `kugou`, `bilivideo`, `youtube`.
    pub providers: Vec<String>,
    /// Per-source search timeout (milliseconds).
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
            splash_duration_secs: default_splash_duration(),
            logger: Logger::default(),
            cache: CacheConfig::default(),
            playerbar: PlayerbarConfig::default(),
            titles: TitlesConfig::default(),
            source_fallback: SonarConfig::default(),
            themes: Vec::new(),
            navigation: NavConfig::default(),
            columns: ColumnsConfig::default(),
            cli_status_template: default_cli_status_template(),
            cli_status_format: default_cli_status_format(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let config_dir = utils::pigma_config_dir();
        let config_path = config_dir.join("config.toml");

        let default = Config::default();
        let config = if config_path.exists() {
            match fs::read_to_string(&config_path) {
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
        };

        if !config_path.exists() {
            let _ = fs::create_dir_all(&config_dir);
            let content = config.to_toml();
            if let Err(e) = fs::write(config_path, content) {
                log::warn!("Failed to write default config: {e}");
            }
        }
        config
    }

    pub fn save(&self) {
        let dir = utils::pigma_config_dir();
        if let Err(e) = fs::create_dir_all(&dir) {
            log::error!("Failed to create config directory: {e}");
            return;
        }
        let content = self.to_toml();
        if let Err(e) = fs::write(dir.join("config.toml"), content) {
            log::error!("Failed to write config.toml: {e}");
        }
    }

    fn to_toml(&self) -> String {
        let mut doc = toml_edit::ser::to_string_pretty(self)
            .unwrap()
            .parse::<toml_edit::DocumentMut>()
            .unwrap();
        // Make navigation implicit
        doc["navigation"].as_table_mut().unwrap().set_implicit(true);

        // Iterate over each section and convert items to inline table arrays
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_serializes_save_on_play() {
        let cfg = Config::default();
        let toml = cfg.to_toml();
        assert!(
            toml.contains("save_on_play = true"),
            "missing save_on_play in default config:\n{toml}"
        );
    }
}
