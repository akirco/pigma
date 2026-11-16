//! Logging configuration: the level persisted in `config.toml`.

use std::str::FromStr;

use log::Level;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Logger {
    #[serde(default = "default_level", deserialize_with = "deserialize_level")]
    pub log_level: Level,
}

fn default_level() -> Level {
    Level::Info
}

/// Unknown log levels fall back to `info` instead of failing the whole config.
fn deserialize_level<'de, D>(deserializer: D) -> Result<Level, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    Ok(Level::from_str(&raw).unwrap_or_else(|_| {
        log::warn!("Unknown log level `{raw}`, using info");
        Level::Info
    }))
}

impl Default for Logger {
    fn default() -> Self {
        Logger {
            log_level: Level::Info,
        }
    }
}
