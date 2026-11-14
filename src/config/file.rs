//! Runtime configuration schema and its TOML load/save path.

use std::fs;

use serde::{Deserialize, Serialize};

use super::{
    BorderConfig, CacheConfig, ColumnsConfig, ContentConfig, EqualizerConfig, GradientPreset,
    Logger, NavConfig, NavPosition, PlayerbarConfig, ProxyTarget, SonarConfig, Theme, TitlesConfig,
};
use crate::config::format;

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
    #[serde(default)]
    pub content: ContentConfig,
    #[serde(default)]
    pub equalizer: EqualizerConfig,
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
            content: ContentConfig::default(),
            equalizer: EqualizerConfig::default(),
            cli_status_template: default_cli_status_template(),
            cli_status_format: default_cli_status_format(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let config_dir = crate::persist::pigma_config_dir();
        let _ = crate::persist::ensure_private_dir(&config_dir);
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
            let content = config.to_toml();
            if let Err(e) = crate::persist::write_atomic(&config_path, content.as_bytes()) {
                log::warn!("Failed to write default config: {e}");
            }
        }
        config
    }

    pub fn save(&self) {
        let content = self.to_toml();
        // Never clobber the user's config with an empty serialization result.
        if content.trim().is_empty() {
            log::error!("Refusing to write an empty config.toml");
            return;
        }
        // Atomic write (temp file + fsync + rename): a crash or a kill mid-save
        // must leave the previous config untouched instead of truncating it.
        let path = crate::persist::pigma_config_dir().join("config.toml");
        if let Some(dir) = path.parent() {
            let _ = crate::persist::ensure_private_dir(dir);
        }
        if let Err(e) = crate::persist::write_atomic(&path, content.as_bytes()) {
            log::error!("Failed to write config.toml: {e}");
        }
    }

    fn to_toml(&self) -> String {
        // Serializing a `Config` back to TOML is infallible in practice (it
        // round-trips through the same serde definitions), and the section
        // reshaping below only applies when the expected tables exist — an
        // unusual config (e.g. empty `navigation.sections`) just skips the
        // reshaping instead of panicking.
        let raw = match toml_edit::ser::to_string_pretty(self) {
            Ok(raw) => raw,
            Err(e) => {
                log::error!("Failed to serialize config: {e}");
                return String::new();
            }
        };
        let Ok(mut doc) = raw.parse::<toml_edit::DocumentMut>() else {
            return raw;
        };

        // Make navigation implicit
        if let Some(nav) = doc["navigation"].as_table_mut() {
            nav.set_implicit(true);
            if let Some(sections) = nav["sections"].as_array_of_tables_mut() {
                for section in sections.iter_mut() {
                    format::convert_aot_to_inline(section, "items", "\n  ");
                }
            }
        }

        if let Some(columns) = doc["columns"].as_table_mut() {
            columns.set_implicit(true);
            if let Some(overrides) = columns["overrides"].as_table_mut() {
                overrides.set_implicit(true);
                format::convert_all_aot_to_inline(overrides, "\n  ");
            }
            format::convert_aot_to_inline(columns, "songs", "\n  ");
            format::convert_aot_to_inline(columns, "songlist", "\n  ");
            columns.set_implicit(true);
        }

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

    #[test]
    fn default_config_writes_border_style() {
        let toml = Config::default().to_toml();
        assert!(
            toml.contains("style = \"plain\""),
            "missing border style in default config:\n{toml}"
        );
    }

    #[test]
    fn default_config_writes_content_styles() {
        let toml = Config::default().to_toml();
        assert!(toml.contains("[content]"), "missing [content]:\n{toml}");
        assert!(
            toml.contains("songs = \"table\""),
            "missing default song style:\n{toml}"
        );
    }

    #[test]
    fn empty_nav_sections_do_not_panic() {
        let mut cfg = Config::default();
        cfg.navigation.sections.clear();
        let toml = cfg.to_toml();
        assert!(toml.contains("save_on_play = true"), "unexpected:\n{toml}");
    }
}
