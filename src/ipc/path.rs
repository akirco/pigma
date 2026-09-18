//! Socket-path resolution: the default Unix socket under the cache dir, the
//! Windows named pipe, and the thread-local / process-wide overrides.

use std::{cell::RefCell, path::PathBuf, sync::OnceLock};

#[cfg(unix)]
use crate::utils::pigma_cache_dir;

/// Socket file name inside `pigma_cache_dir()` (Unix only).
pub const SOCKET_FILE: &str = "pigma.sock";

/// Default named-pipe name on Windows.
#[cfg(windows)]
const PIPE_NAME: &str = r"\\.\pipe\pigma";

fn socket_path() -> PathBuf {
    #[cfg(unix)]
    {
        pigma_cache_dir().join(SOCKET_FILE)
    }
    #[cfg(windows)]
    {
        PathBuf::from(PIPE_NAME)
    }
}

thread_local! {
    static SOCKET_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Process-wide socket-path override (set once by the CLI's `--socket` flag).
/// The thread-local test override, when present, still takes precedence.
static SOCKET_GLOBAL: OnceLock<PathBuf> = OnceLock::new();

/// Override the socket path for this thread (used by integration tests, which
/// each bind their own socket so they can run in parallel). Safe because the
/// override is thread-local.
#[doc(hidden)]
pub fn set_socket_path_override(path: Option<PathBuf>) {
    SOCKET_OVERRIDE.with(|c| *c.borrow_mut() = path);
}

/// Override the socket path process-wide (used by the CLI `--socket` flag so a
/// daemon and the `status`/`msg` commands can address a non-default instance).
pub fn set_socket_path(path: Option<PathBuf>) {
    if let Some(p) = path {
        let _ = SOCKET_GLOBAL.set(p);
    }
}

/// Resolve the socket path: a thread-local override if set, otherwise the
/// process-wide override, otherwise the default location under `pigma_cache_dir()`.
pub(super) fn resolve_socket_path() -> PathBuf {
    SOCKET_OVERRIDE
        .with(|c| c.borrow().clone())
        .unwrap_or_else(|| SOCKET_GLOBAL.get().cloned().unwrap_or_else(socket_path))
}
