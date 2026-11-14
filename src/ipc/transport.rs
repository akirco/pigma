//! Platform transport for the IPC endpoint: a Unix domain socket on
//! Linux/macOS and a named pipe on Windows.
//!
//! This is the only IPC module that branches on `cfg(unix)` / `cfg(windows)`
//! for the endpoint itself; [`server`](super::server) and [`client`](super::client)
//! speak in terms of the [`Listener`], [`Stream`] and [`ClientStream`] types
//! defined here.

use super::path::resolve_socket_path;

/// The stream accepted by the server (Unix socket on unix, named pipe on
/// Windows).
#[cfg(unix)]
pub(crate) type Stream = tokio::net::UnixStream;
#[cfg(windows)]
pub(crate) type Stream = tokio::net::windows::named_pipe::NamedPipeServer;

/// The stream a client connects with (Unix socket on unix, named pipe on
/// Windows).
#[cfg(unix)]
pub(crate) type ClientStream = tokio::net::UnixStream;
#[cfg(windows)]
pub(crate) type ClientStream = tokio::net::windows::named_pipe::NamedPipeClient;

/// Connect to the running instance's listener endpoint.
pub(crate) async fn connect_client(path: &std::path::Path) -> std::io::Result<ClientStream> {
    #[cfg(unix)]
    {
        ClientStream::connect(path).await
    }
    #[cfg(windows)]
    {
        tokio::net::windows::named_pipe::ClientOptions::new().open(path.to_string_lossy().as_ref())
    }
}

/// Platform-specific listener for the IPC server.
///
/// On Windows a named pipe is re-created for every connection, so the Windows
/// variant holds the pipe name rather than a persistent handle.
pub(crate) struct Listener {
    #[cfg(unix)]
    unix: tokio::net::UnixListener,
    /// Owner of the socket file, used to reject connections from other local
    /// users when the platform exposes peer credentials.
    #[cfg(unix)]
    owner_uid: Option<u32>,
    #[cfg(windows)]
    name: String,
}

impl Listener {
    /// Bind the listener, clearing any stale file left by a previous run on
    /// Unix. Returns `None` when another pigma instance already holds the
    /// endpoint.
    pub(crate) fn bind() -> Option<Self> {
        let path = resolve_socket_path();
        #[cfg(unix)]
        {
            if let Some(dir) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                ensure_socket_dir(dir);
            }
            match tokio::net::UnixListener::bind(&path) {
                Ok(unix) => {
                    secure_socket(&path);
                    Some(Self {
                        unix,
                        owner_uid: socket_owner(&path),
                    })
                }
                Err(_) => {
                    // Either a live instance owns the socket or it is stale.
                    // A connect probe tells us which: if we can connect, another
                    // instance is running and we must not steal the socket.
                    if std::os::unix::net::UnixStream::connect(&path).is_ok() {
                        log::warn!(
                            "ipc: another pigma instance already owns {}",
                            path.display()
                        );
                        return None;
                    }
                    let _ = std::fs::remove_file(&path);
                    tokio::net::UnixListener::bind(&path).ok().map(|unix| {
                        secure_socket(&path);
                        Self {
                            unix,
                            owner_uid: socket_owner(&path),
                        }
                    })
                }
            }
        }
        #[cfg(windows)]
        {
            let name = path.to_string_lossy().into_owned();
            match tokio::net::windows::named_pipe::ServerOptions::new().create(&name) {
                Ok(_) => Some(Self { name }),
                Err(e) => {
                    // Windows releases the pipe name when the owning process
                    // exits, so a failed bind always means a live instance.
                    log::warn!("ipc: another pigma instance already owns the pipe {name}: {e}");
                    None
                }
            }
        }
    }

    /// Whether an accepted connection comes from the same local user.
    ///
    /// The Unix socket is already chmod 0600; peer credentials are a second
    /// line of defense against a race during bind or a mis-set directory mode.
    #[cfg(unix)]
    pub(crate) fn is_trusted(&self, stream: &Stream) -> bool {
        #[cfg(any(
            target_os = "linux",
            target_os = "android",
            target_os = "macos",
            target_os = "ios"
        ))]
        {
            let Some(expected) = self.owner_uid else {
                return true;
            };
            match stream.peer_cred() {
                Ok(cred) => cred.uid() == expected,
                Err(e) => {
                    // Fail open here: the 0600 socket remains the primary guard
                    // and some filesystems/kernels may not expose credentials.
                    log::warn!("ipc: could not read peer credentials: {e}");
                    true
                }
            }
        }
        #[cfg(not(any(
            target_os = "linux",
            target_os = "android",
            target_os = "macos",
            target_os = "ios"
        )))]
        {
            let _ = stream;
            true
        }
    }

    /// Windows named pipes are scoped by the default pipe ACL; no extra check.
    #[cfg(windows)]
    pub(crate) fn is_trusted(&self, _stream: &Stream) -> bool {
        true
    }

    /// Wait for the next incoming connection, returning the accepted stream.
    pub(crate) async fn next(&mut self) -> Option<Stream> {
        #[cfg(unix)]
        {
            self.unix.accept().await.ok().map(|(stream, _)| stream)
        }
        #[cfg(windows)]
        {
            // A fresh server instance per connection; after a client attaches,
            // that instance becomes the connection stream.
            let server = tokio::net::windows::named_pipe::ServerOptions::new()
                .create(&self.name)
                .ok()?;
            server.connect().await.ok()?;
            Some(server)
        }
    }
}

/// Create the socket's parent directory and tighten pigma's own cache dir.
///
/// Only the default cache directory is chmodded: an explicit `--socket` path
/// may live in a shared directory (e.g. `/tmp`) that must not be changed.
#[cfg(unix)]
fn ensure_socket_dir(dir: &std::path::Path) {
    let _ = std::fs::create_dir_all(dir);
    if dir == crate::store::pigma_cache_dir().as_path() {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
}

/// Restrict the socket file to its owner. The bind itself runs with the
/// process umask (typically 0755), so this closes the gap to other local users.
#[cfg(unix)]
fn secure_socket(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
        log::warn!("ipc: failed to restrict {} to 0600: {e}", path.display());
    }
}

/// UID that owns the freshly bound socket file.
#[cfg(unix)]
fn socket_owner(path: &std::path::Path) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|meta| meta.uid())
}

/// Remove the endpoint file so a later run can rebind it. No-op on Windows,
/// which releases the pipe name when the process exits.
pub(crate) fn remove_endpoint() {
    #[cfg(unix)]
    let _ = std::fs::remove_file(resolve_socket_path());
}
