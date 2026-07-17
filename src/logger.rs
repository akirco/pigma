use log::Level;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;

use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Logger {
    pub log_level: Level,
    #[serde(skip)]
    pub log_file: String,
}

use std::path::PathBuf;

fn log_file() -> PathBuf {
    if cfg!(debug_assertions) {
        PathBuf::from("debug.log")
    } else {
        dirs::config_dir()
            .map(|d| d.join("pigma").join("debug.log"))
            .unwrap_or_else(|| PathBuf::from("debug.log"))
    }
}

impl Default for Logger {
    fn default() -> Self {
        Logger {
            log_level: Level::Debug,
            log_file: log_file().to_string_lossy().to_string(),
        }
    }
}

pub fn init_logger(config: &Config) -> color_eyre::Result<()> {
    let log_file = log_file();
    simplelog::WriteLogger::init(
        config.logger.log_level.to_level_filter(),
        simplelog::Config::default(),
        OpenOptions::new()
            .append(true)
            .create(true)
            .open(&log_file)?,
    )?;
    Ok(())
}
