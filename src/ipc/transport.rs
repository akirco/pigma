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
pub(super) type Stream = tokio::net::UnixStream;
#[cfg(windows)]
pub(super) type Stream = tokio::net::windows::named_pipe::NamedPipeServer;

/// The stream a client connects with (Unix socket on unix, named pipe on
/// Windows).
#[cfg(unix)]
pub(super) type ClientStream = tokio::net::UnixStream;
#[cfg(windows)]
pub(super) type ClientStream = tokio::net::windows::named_pipe::NamedPipeClient;

/// Connect to the running instance's listener endpoint.
pub(super) async fn connect_client(path: &std::path::Path) -> std::io::Result<ClientStream> {
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
/// variant holds the pipe name plus the first server instance (which reserves
/// the name until the first client connects).
pub(super) struct Listener {
    #[cfg(unix)]
    unix: tokio::net::UnixListener,
    /// Owner of the socket file, used to reject connections from other local
    /// users when the platform exposes peer credentials.
    #[cfg(unix)]
    owner_uid: Option<u32>,
    #[cfg(windows)]
    name: String,
    #[cfg(windows)]
    first: Option<Stream>,
}

impl Listener {
    /// Bind the listener, clearing any stale file left by a previous run on
    /// Unix. Returns `None` when another pigma instance already holds the
    /// endpoint.
    pub(super) fn bind() -> Option<Self> {
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
            // `first_pipe_instance` fails if another process already owns the
            // pipe name. Without it, CreateNamedPipe can silently add another
            // instance and two pigma processes would share the endpoint.
            match create_pipe_server(&name, true) {
                Ok(first) => Some(Self {
                    name,
                    first: Some(first),
                }),
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
    pub(super) fn is_trusted(&self, stream: &Stream) -> bool {
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

    /// Windows access is restricted by the explicit owner-only DACL attached
    /// when each pipe instance is created, so no peer check is needed here.
    #[cfg(windows)]
    pub(super) fn is_trusted(&self, _stream: &Stream) -> bool {
        true
    }

    /// Wait for the next incoming connection, returning the accepted stream.
    pub(super) async fn next(&mut self) -> Option<Stream> {
        #[cfg(unix)]
        {
            self.unix.accept().await.ok().map(|(stream, _)| stream)
        }
        #[cfg(windows)]
        {
            // The instance created by `bind` reserves the pipe name; after the
            // first client attaches, create a fresh instance for the next one.
            let server = match self.first.take() {
                Some(server) => server,
                None => create_pipe_server(&self.name, false).ok()?,
            };
            server.connect().await.ok()?;
            Some(server)
        }
    }
}

/// Create a pipe instance with an owner-only DACL.
///
/// The default named-pipe security descriptor is inherited from the process
/// and can grant access to other local users depending on policy. Attaching an
/// explicit protected DACL (`SY` LocalSystem + `OW` owner) makes the IPC
/// endpoint owner-only, matching the Unix socket's 0600 mode.
#[cfg(windows)]
fn create_pipe_server(
    name: &str,
    first_instance: bool,
) -> std::io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
    use std::ffi::c_void;

    use windows_sys::Win32::{
        Foundation::{HLOCAL, LocalFree},
        Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
            },
            PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
        },
    };

    const SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;OW)";
    let mut sddl: Vec<u16> = SDDL.encode_utf16().chain(std::iter::once(0)).collect();
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();

    // SAFETY: `sddl` is a valid NUL-terminated wide string, `descriptor` points
    // to writable storage, and the optional size pointer is null as allowed by
    // the API.
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_mut_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }

    let mut attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    // SAFETY: `attributes` points to a valid SECURITY_ATTRIBUTES for the
    // duration of the call, as required by the tokio API.
    let result = unsafe {
        let mut options = tokio::net::windows::named_pipe::ServerOptions::new();
        options.first_pipe_instance(first_instance);
        options.create_with_security_attributes_raw(
            std::ffi::OsStr::new(name),
            &mut attributes as *mut _ as *mut c_void,
        )
    };
    // SAFETY: `descriptor` was allocated by
    // ConvertStringSecurityDescriptorToSecurityDescriptorW and is no longer
    // needed once the pipe has been created.
    unsafe {
        LocalFree(descriptor as HLOCAL);
    }
    result
}

/// Create the socket's parent directory and tighten pigma's own cache dir.
///
/// Only the default cache directory is chmodded: an explicit `--socket` path
/// may live in a shared directory (e.g. `/tmp`) that must not be changed.
#[cfg(unix)]
fn ensure_socket_dir(dir: &std::path::Path) {
    let _ = std::fs::create_dir_all(dir);
    if dir == crate::persist::pigma_cache_dir().as_path() {
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
pub(super) fn remove_endpoint() {
    #[cfg(unix)]
    let _ = std::fs::remove_file(resolve_socket_path());
}
