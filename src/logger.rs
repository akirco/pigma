use log::{Level, Log, Metadata, Record};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;

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

struct FileLogger {
    file: Mutex<std::fs::File>,
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let ts = crate::utils::local_timestamp();
        let mut file = self.file.lock().unwrap();
        let _ = writeln!(
            file,
            "[{} {:<5} {}] {}",
            ts,
            record.level(),
            record.module_path().unwrap_or_default(),
            record.args()
        );
    }

    fn flush(&self) {
        let _ = self.file.lock().unwrap().flush();
    }
}

pub fn init_logger(config: &Config) -> color_eyre::Result<()> {
    let log_file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(log_file())?;
    log::set_boxed_logger(Box::new(FileLogger {
        file: Mutex::new(log_file),
    }))?;
    log::set_max_level(config.logger.log_level.to_level_filter());
    Ok(())
}
