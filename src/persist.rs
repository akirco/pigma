//! Low-level persistence helpers: owner-only paths, path sanitation and
//! atomic file replacement.
//!
//! This module deliberately has no dependency on the cache, configuration or
//! UI layers so they can all share it without creating cycles.

use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use dirs::home_dir;

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
    let mut tmp = PathBuf::new();
    let mut created = false;

    let result = (|| -> io::Result<()> {
        // `create_new` refuses to follow a pre-created symlink at the temp
        // path; retry on the (very unlikely) random-name collision.
        let mut attempt = 0u32;
        let mut file = loop {
            tmp = dir.join(format!(
                ".{file_name}.{}.{}.tmp",
                std::process::id(),
                rand::random::<u32>()
            ));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                // Config, queues and cache payloads can contain account data;
                // keep them owner-only.
                options.mode(0o600);
            }
            match options.open(&tmp) {
                Ok(file) => {
                    created = true;
                    break file;
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists && attempt < 8 => {
                    attempt += 1;
                }
                Err(e) => return Err(e),
            }
        };
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)?;
        // Persist the directory entry as well; otherwise the rename itself can
        // be lost on power failure even though the file data was synced.
        #[cfg(unix)]
        if let Ok(dir_file) = fs::File::open(dir) {
            let _ = dir_file.sync_all();
        }
        Ok(())
    })();
    if let Err(e) = result {
        if created && !tmp.as_os_str().is_empty() {
            let _ = fs::remove_file(&tmp);
        }
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

/// Replace illegal characters in a file name with `_`, including control
/// characters that would otherwise break logs or terminal rendering.
pub(crate) fn sanitize_filename(s: &str) -> String {
    let mut out: String = s
        .trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();

    // Windows silently strips trailing dots/spaces; doing it explicitly keeps
    // the on-disk name and the index in sync.
    while out.ends_with('.') || out.ends_with(' ') {
        out.pop();
    }

    // Windows reserved device names must not be used as a plain file stem.
    // The id suffix usually avoids this, but custom templates may not.
    let stem = out.split('.').next().unwrap_or_default();
    if is_windows_reserved(stem) {
        out.insert(0, '_');
    }

    if out.is_empty() {
        out.push('_');
    }
    out
}

fn is_windows_reserved(stem: &str) -> bool {
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
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
    fn sanitize_filename_handles_control_and_windows_reserved_names() {
        assert_eq!(sanitize_filename("a\nb"), "a_b");
        assert_eq!(sanitize_filename("CON"), "_CON");
        assert_eq!(sanitize_filename("name. "), "name");
        assert_eq!(sanitize_filename("   "), "_");
        assert_eq!(sanitize_filename("../x"), ".._x");
    }

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
