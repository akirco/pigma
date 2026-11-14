//! Low-level persistence helpers: owner-only paths, path sanitation and
//! atomic file replacement.
//!
//! This module deliberately has no dependency on the cache, configuration or
//! UI layers so they can all share it without creating cycles.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use dirs::home_dir;

mod ext;

pub(crate) use ext::canonical_ext;

/// Atomically write `bytes` to `path`, creating its parent directory if needed.
///
/// A unique temp file is written in the same directory, `fsync`ed, then renamed
/// over the target. `rename` is atomic within a filesystem, so a reader — or a
/// process crash mid-write — never observes a truncated file: the previous
/// contents stay intact until the swap. The data is synced before the rename so
/// a power loss cannot leave the target pointing at a zero-length temp file; at
/// worst the rename is lost and the old contents remain.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;

    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("cache");
    let tmp = dir.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        rand::random::<u32>()
    ));

    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // Config, queues and cache payloads can contain account data;
            // keep them owner-only.
            options.mode(0o600);
        }
        let mut file = options.open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// Create `dir` (if needed) and make it owner-only on Unix.
///
/// Used for pigma's config directory, which holds `config.toml` (possibly with
/// proxy credentials) and `debug.log`.
pub(crate) fn ensure_private_dir(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Replace illegal characters in a file name with `_`.
pub(crate) fn sanitize_filename(s: &str) -> String {
    s.trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

/// pigma cache root directory (cache files and play queues live under it).
pub(crate) fn pigma_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pigma")
}

/// pigma config root directory.
pub(crate) fn pigma_config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pigma")
}

pub(crate) fn expand_tilde(path: &str) -> PathBuf {
    let home = match home_dir() {
        Some(h) => h,
        None => return PathBuf::from(path),
    };

    if path == "~" {
        return home;
    }

    if let Some(rest) = path.strip_prefix("~/") {
        return home.join(rest);
    }

    if cfg!(windows)
        && let Some(rest) = path.strip_prefix("~\\")
    {
        return home.join(rest);
    }

    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use dirs::home_dir;

    use super::*;

    #[test]
    fn expand_tilde_handles_home_and_plain_paths() {
        let home = home_dir().unwrap();

        assert_eq!(expand_tilde("~"), home);
        assert_eq!(expand_tilde("~/.cache/dir/xx"), home.join(".cache/dir/xx"));

        let win_input = r"~\.cache\dir\xx";
        if cfg!(windows) {
            assert_eq!(expand_tilde(win_input), home.join(r".cache\dir\xx"));
        } else {
            assert_eq!(expand_tilde(win_input), PathBuf::from(win_input));
        }
    }

    #[cfg(unix)]
    #[test]
    fn ensure_private_dir_makes_the_directory_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!(
            "pigma_private_dir_test_{}_{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        ensure_private_dir(&dir).expect("create private dir");

        let mode = fs::metadata(&dir).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn write_atomic_creates_dir_and_replaces_without_leftovers() {
        let dir =
            std::env::temp_dir().join(format!("pigma_persist_test_{}", rand::random::<u64>()));
        let path = dir.join("nested").join("entry.json");

        write_atomic(&path, b"first").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"first");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "atomic writes must be owner-only");
        }

        write_atomic(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");

        let leftovers: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {leftovers:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
