//! Logging setup: a buffered append-only file sink (`debug.log`), rotated at
//! 5 MiB so a long-running TUI cannot grow it without bound.

use std::{
    fs::{self, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

use log::{Level, Log, Metadata, Record};

use crate::{config::Config, lock::lock_recover, persist::pigma_config_dir, time::local_timestamp};

/// Rotate once the log grows past this size.
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
/// Checking `metadata()` on every line would add a syscall to each log; batch
/// the size check.
const SIZE_CHECK_LINES: u32 = 1024;

fn log_file() -> PathBuf {
    if cfg!(debug_assertions) {
        PathBuf::from("debug.log")
    } else {
        pigma_config_dir().join("debug.log")
    }
}

fn rotated_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".1");
    PathBuf::from(name)
}

fn open_log_file(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(file)
}

struct LogFile {
    writer: Option<BufWriter<std::fs::File>>,
    path: PathBuf,
    lines_since_size_check: u32,
}

impl LogFile {
    fn open(path: PathBuf) -> std::io::Result<Self> {
        Ok(Self {
            writer: Some(BufWriter::new(open_log_file(&path)?)),
            path,
            lines_since_size_check: 0,
        })
    }

    /// Move the current file aside and start a fresh one. Called while holding
    /// the logger mutex; never logs itself (that would recurse).
    fn rotate(&mut self) {
        if let Some(mut writer) = self.writer.take() {
            let _ = writer.flush();
        }
        let rotated = rotated_path(&self.path);
        let _ = fs::remove_file(&rotated);
        let _ = fs::rename(&self.path, &rotated);
        self.writer = open_log_file(&self.path).ok().map(BufWriter::new);
        self.lines_since_size_check = 0;
    }

    fn maybe_rotate(&mut self) {
        self.lines_since_size_check = self.lines_since_size_check.saturating_add(1);
        if self.lines_since_size_check < SIZE_CHECK_LINES {
            return;
        }
        self.lines_since_size_check = 0;
        let too_big = self
            .writer
            .as_ref()
            .and_then(|writer| writer.get_ref().metadata().ok())
            .is_some_and(|meta| meta.len() > MAX_LOG_BYTES);
        if too_big {
            self.rotate();
        }
    }
}

struct FileLogger {
    file: Mutex<LogFile>,
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
        let mut state = lock_recover(&self.file);
        if state.writer.is_none() {
            // A previous rotation failed to reopen; try again instead of
            // silently dropping the rest of this session's logs.
            state.writer = open_log_file(&state.path).ok().map(BufWriter::new);
        }
        {
            let Some(file) = state.writer.as_mut() else {
                return;
            };
            let _ = writeln!(
                file,
                "[{} {:<5} {}] {}",
                ts,
                record.level(),
                record.module_path().unwrap_or_default(),
                record.args()
            );
            // Errors/warnings flush immediately so an imminent crash cannot
            // lose them; high-volume debug/info lines rely on the BufWriter
            // filling up.
            if record.level() <= Level::Warn {
                let _ = file.flush();
            }
        }
        state.maybe_rotate();
    }

    fn flush(&self) {
        let mut state = lock_recover(&self.file);
        if let Some(file) = state.writer.as_mut() {
            let _ = file.flush();
        }
    }
}

pub fn init_logger(config: &Config) -> eyre::Result<()> {
    let path = log_file();
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        let _ = crate::persist::ensure_private_dir(parent);
    }
    // A previous session may have left an oversized file; rotate before the
    // first new line instead of waiting for the periodic size check.
    if fs::metadata(&path).is_ok_and(|meta| meta.len() > MAX_LOG_BYTES) {
        let rotated = rotated_path(&path);
        let _ = fs::remove_file(&rotated);
        let _ = fs::rename(&path, &rotated);
    }
    let logger = LogFile::open(path)?;
    log::set_boxed_logger(Box::new(FileLogger {
        file: Mutex::new(logger),
    }))?;
    log::set_max_level(config.logger.log_level.to_level_filter());
    Ok(())
}

/// Flush any buffered log output to disk. Call on shutdown so the final lines
/// (written through the buffered sink) are not lost.
pub fn flush_logger() {
    log::logger().flush();
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Write, path::PathBuf};

    use super::{LogFile, open_log_file, rotated_path};

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pigma_logger_test_{}_{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn rotate_moves_the_current_file_aside_and_reopens() {
        let dir = temp_dir();
        let path = dir.join("debug.log");
        let mut log = LogFile::open(path.clone()).expect("open log");
        log.writer
            .as_mut()
            .expect("writer")
            .write_all(b"hello")
            .expect("write");
        log.writer.as_mut().expect("writer").flush().expect("flush");

        log.rotate();

        assert_eq!(
            fs::read(rotated_path(&path)).expect("read rotated"),
            b"hello"
        );
        assert!(path.exists(), "a fresh log file must be reopened");
        assert!(path.metadata().expect("metadata").len() == 0);

        let _ = fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn log_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir();
        let path = dir.join("debug.log");
        let _file = open_log_file(&path).expect("open log");
        let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);

        let _ = fs::remove_dir_all(dir);
    }
}
