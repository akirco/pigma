use std::path::{Path, PathBuf};

use dirs::home_dir;

/// Create `dir` (if needed) and make it owner-only on Unix.
///
/// Used for pigma's config directory, which holds `config.toml` (possibly with
/// proxy credentials) and `debug.log`.
pub fn ensure_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Replace illegal characters in a file name with `_`.
pub fn sanitize_filename(s: &str) -> String {
    s.trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

/// pigma cache root directory (cache files and play queues live under it).
pub fn pigma_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pigma")
}

/// pigma config root directory.
pub fn pigma_config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pigma")
}

pub fn expand_tilde(path: &str) -> PathBuf {
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
    fn test_expand_tilde() {
        let home = home_dir().unwrap();

        assert_eq!(expand_tilde("~"), home);

        let unix_path = expand_tilde("~/.cache/dir/xx");
        assert_eq!(unix_path, home.join(".cache/dir/xx"));

        let win_input = r"~\.cache\dir\xx";
        if cfg!(windows) {
            let expected = home.join(r".cache\dir\xx");
            assert_eq!(expand_tilde(win_input), expected);
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

        let mode = std::fs::metadata(&dir)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
        let _ = std::fs::remove_dir_all(dir);
    }
}
