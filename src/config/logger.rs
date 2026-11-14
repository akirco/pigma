//! Logging configuration: the level persisted in `config.toml`.

use log::Level;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Logger {
    pub log_level: Level,
}

impl Default for Logger {
    fn default() -> Self {
        Logger {
            log_level: Level::Info,
        }
    }
}
