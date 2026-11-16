//! Runtime configuration schema and its TOML load/save path.

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use serde::{Deserialize, Deserializer, Serialize};

use super::{
    BorderConfig, CacheConfig, ColumnsConfig, ContentConfig, EqualizerConfig, GradientPreset,
    Logger, NavConfig, NavPosition, PlayerbarConfig, ProxyTarget, SonarConfig, Theme, TitlesConfig,
};
use crate::config::format;

/// `pigma status` output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub enum StatusFormat {
    #[default]
    #[serde(rename = "plain")]
    Plain,
    #[serde(rename = "json")]
    Json,
}

impl StatusFormat {
    pub fn is_json(self) -> bool {
        self == Self::Json
    }
}

/// Unknown formats fall back to plain instead of failing the whole config.
impl<'de> Deserialize<'de> for StatusFormat {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.as_str() {
            "json" => Self::Json,
            "plain" => Self::Plain,
            _ => {
                log::warn!("Unknown cli_status_format `{raw}`, using plain");
                Self::Plain
            }
        })
    }
}

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
    #[serde(default)]
    pub cli_status_format: StatusFormat,
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
            cli_status_format: StatusFormat::Plain,
        }
    }
}

/// Set when an existing config could not be read/parsed and could not be
/// backed up. Writing in that state would destroy the user's only copy.
static SAVE_BLOCKED: AtomicBool = AtomicBool::new(false);

/// Exact `config.toml` text seen on load or written by the last successful
/// save. `save` refuses to replace a file that no longer matches it, so edits
/// made while pigma is running are not clobbered when the app saves or exits.
static CONFIG_SNAPSHOT: Mutex<Option<String>> = Mutex::new(None);

fn remember_config(content: &str) {
    *crate::lock::lock_recover(&CONFIG_SNAPSHOT) = Some(content.to_string());
}

fn config_snapshot() -> Option<String> {
    crate::lock::lock_recover(&CONFIG_SNAPSHOT).clone()
}

/// Move an unreadable/invalid config aside so the next start can write a fresh
/// default without destroying the user's settings.
fn backup_invalid_config(path: &Path) -> io::Result<PathBuf> {
    let mut name = path.as_os_str().to_os_string();
    name.push(".bak");
    let backup = PathBuf::from(name);
    let _ = fs::remove_file(&backup);
    fs::rename(path, &backup)?;
    Ok(backup)
}

impl Config {
    /// Load the config, creating the config directory and a default
    /// `config.toml` when they do not exist yet.
    pub fn load() -> Self {
        Self::load_inner(true)
    }

    /// Load the config without creating files or directories. Used by one-shot
    /// commands such as `pigma status`, which must not have write side effects.
    pub fn load_read_only() -> Self {
        Self::load_inner(false)
    }

    fn load_inner(create_default: bool) -> Self {
        let config_dir = crate::persist::pigma_config_dir();
        if create_default {
            let _ = crate::persist::ensure_private_dir(&config_dir);
        }
        let config_path = config_dir.join("config.toml");

        let default = Config::default();
        let config = match fs::read_to_string(&config_path) {
            Ok(content) => match toml_edit::de::from_str(&content) {
                Ok(cfg) => cfg,
                Err(e) => {
                    eprintln!("Failed to parse {}: {e}", config_path.display());
                    if create_default {
                        match backup_invalid_config(&config_path) {
                            Ok(backup) => eprintln!(
                                "The invalid config was moved to {}; using defaults.",
                                backup.display()
                            ),
                            Err(backup_err) => {
                                SAVE_BLOCKED.store(true, Ordering::SeqCst);
                                eprintln!(
                                    "Could not back up {}: {backup_err}. Refusing to overwrite it; using defaults.",
                                    config_path.display()
                                );
                            }
                        }
                    } else {
                        eprintln!("Using defaults for this command.");
                    }
                    default
                }
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => default,
            Err(e) => {
                if create_default {
                    // Without a readable file we cannot back it up before a
                    // later save would replace it.
                    SAVE_BLOCKED.store(true, Ordering::SeqCst);
                }
                eprintln!(
                    "Failed to read {}: {e}; using defaults.",
                    config_path.display()
                );
                default
            }
        };

        if create_default && !config_path.exists() {
            let content = config.to_toml();
            if let Err(e) = crate::persist::write_atomic(&config_path, content.as_bytes()) {
                eprintln!(
                    "Failed to write default config {}: {e}",
                    config_path.display()
                );
            }
        }
        // Remember exactly what is on disk now. A later external edit changes
        // this text and makes `save` leave the file alone.
        if let Ok(content) = fs::read_to_string(&config_path) {
            remember_config(&content);
        }
        config
    }

    pub fn save(&self) {
        if SAVE_BLOCKED.load(Ordering::SeqCst) {
            log::error!(
                "Refusing to overwrite config.toml: the existing file could not be read/parsed and could not be backed up"
            );
            return;
        }
        let content = self.to_toml();
        // Never clobber the user's config with an empty serialization result.
        if content.trim().is_empty() {
            log::error!("Refusing to write an empty config.toml");
            return;
        }
        // Atomic write (temp file + fsync + rename): a crash or a kill mid-save
        // must leave the previous config untouched instead of truncating it.
        let path = crate::persist::pigma_config_dir().join("config.toml");
        // Never clobber edits the user made in an editor while pigma was
        // running: the on-disk file must still be exactly what we loaded or
        // last wrote. The next start will load the external version.
        if let Some(known) = config_snapshot() {
            match fs::read_to_string(&path) {
                Ok(current) if current != known => {
                    log::warn!(
                        "config.toml changed on disk while pigma was running; keeping those edits and skipping this save"
                    );
                    return;
                }
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    log::warn!(
                        "config.toml was removed while pigma was running; refusing to recreate it"
                    );
                    return;
                }
                Err(e) => {
                    log::error!("Failed to check config.toml before saving: {e}");
                    return;
                }
            }
        }
        if let Some(dir) = path.parent() {
            let _ = crate::persist::ensure_private_dir(dir);
        }
        if let Err(e) = crate::persist::write_atomic(&path, content.as_bytes()) {
            log::error!("Failed to write config.toml: {e}");
        } else {
            remember_config(&content);
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

    #[test]
    fn unknown_enum_values_fall_back_without_dropping_other_settings() {
        let cfg: Config = toml_edit::de::from_str(
            r#"
default_theme = "dracula"
lyric_gradient = "fire"
proxy_target = "sideways"
navigation_position = "diagonal"

[logger]
log_level = "verbose"

[border]
style = "wavy"

[content]
songs = "grid"

[playerbar]
layout = "compact"
"#,
        )
        .expect("lenient config should still parse");

        assert_eq!(cfg.default_theme, "dracula");
        assert_eq!(cfg.lyric_gradient, crate::config::GradientPreset::Rainbow);
        assert_eq!(cfg.proxy_target, crate::config::ProxyTarget::Normal);
        assert_eq!(cfg.navigation_position, crate::config::NavPosition::Left);
        assert_eq!(cfg.logger.log_level, log::Level::Info);
        assert_eq!(cfg.border.style, crate::config::BorderStyle::Plain);
        assert_eq!(cfg.content.songs, crate::config::ContentStyle::Table);
        assert_eq!(cfg.playerbar.layout, crate::config::LayoutType::Modern);
    }

    #[test]
    fn partial_navigation_keeps_the_default_sections() {
        let cfg: Config =
            toml_edit::de::from_str("[navigation]\nselected_left = \"x\"\n").expect("partial nav");
        assert_eq!(cfg.navigation.selected_left, "x");
        assert!(!cfg.navigation.sections.is_empty());
    }

    #[test]
    fn invalid_theme_color_falls_back_without_dropping_the_theme() {
        let cfg: Config =
            toml_edit::de::from_str("[[themes]]\nname = \"broken\"\nbg = \"not-a-color\"\n")
                .expect("theme with a bad color");
        assert_eq!(cfg.themes.len(), 1);
        assert_eq!(cfg.themes[0].name, "broken");
        assert_eq!(cfg.themes[0].bg, crate::config::Theme::default().bg);
    }

    #[test]
    fn unknown_border_gradient_disables_it() {
        let cfg: Config =
            toml_edit::de::from_str("[border]\nborder_gradient = \"nope\"\n").expect("border");
        assert_eq!(cfg.border.border_gradient, None);
    }

    #[test]
    fn invalid_quality_provider_and_status_format_fall_back() {
        let cfg: Config = toml_edit::de::from_str(
            r#"
cli_status_format = "yaml"

[cache]
quality = "ultra"

[source_fallback]
providers = ["kuwo", "nope", "youtube"]
"#,
        )
        .expect("lenient config");

        assert_eq!(cfg.cli_status_format, StatusFormat::Plain);
        assert_eq!(cfg.cache.quality, ncm_api::SongQuality::Standard);
        assert_eq!(
            cfg.source_fallback.providers,
            vec![sonar::SonarSource::Kuwo, sonar::SonarSource::Youtube]
        );
    }
}
