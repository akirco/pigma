//! Logging setup: a buffered append-only file sink (`debug.log`).

use std::{
    fs::OpenOptions,
    io::{BufWriter, Write},
    path::PathBuf,
    sync::Mutex,
};

use log::{Level, Log, Metadata, Record};
use serde::{Deserialize, Serialize};

use crate::{
    config::Config,
    utils::{local_timestamp, lock_recover, pigma_config_dir},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Logger {
    pub log_level: Level,
}

fn log_file() -> PathBuf {
    if cfg!(debug_assertions) {
        PathBuf::from("debug.log")
    } else {
        pigma_config_dir().join("debug.log")
    }
}

impl Default for Logger {
    fn default() -> Self {
        Logger {
            log_level: Level::Debug,
        }
    }
}

struct FileLogger {
    file: Mutex<BufWriter<std::fs::File>>,
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let ts = local_timestamp();
        let mut file = lock_recover(&self.file);
        let _ = writeln!(
            file,
            "[{} {:<5} {}] {}",
            ts,
            record.level(),
            record.module_path().unwrap_or_default(),
            record.args()
        );
        // Errors/warnings flush immediately so an imminent crash cannot lose
        // them; high-volume debug/info lines rely on the BufWriter filling up.
        if record.level() <= Level::Warn {
            let _ = file.flush();
        }
    }

    fn flush(&self) {
        let _ = lock_recover(&self.file).flush();
    }
}

pub fn init_logger(config: &Config) -> eyre::Result<()> {
    let log_file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(log_file())?;
    log::set_boxed_logger(Box::new(FileLogger {
        file: Mutex::new(BufWriter::new(log_file)),
    }))?;
    log::set_max_level(config.logger.log_level.to_level_filter());
    Ok(())
}

/// Flush any buffered log output to disk. Call on shutdown so the final lines
/// (written through the buffered sink) are not lost.
pub fn flush_logger() {
    log::logger().flush();
}
