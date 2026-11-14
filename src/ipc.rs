//! IPC between the running TUI and the `pigma status` / `pigma msg` CLI
//! commands.
//!
//! The TUI binds a listener and accepts one-line JSON requests:
//!
//! - `{"cmd":"status"}` → the server replies with a serialized `StatusSnapshot`.
//! - `{"cmd":"subscribe"}` → the server streams each snapshot change as a JSON
//!   line until the connection closes (event push for waybar / other clients).
//! - `{"cmd":"msg","action":...}` → the server forwards an `IpcEvent` into the
//!   app's event channel and replies `{"ok":true}`.
//!
//! Transport is platform-specific: a Unix domain socket at
//! `~/.cache/pigma/pigma.sock` on Linux/macOS, and a named pipe `\\.\pipe\pigma`
//! on Windows. The Unix endpoint is chmod 0600 and peer credentials are checked
//! where available, so only the owning user can control the instance.

mod client;
mod path;
mod protocol;
mod server;
mod snapshot;
mod state;

mod transport;

pub use client::*;
pub use path::*;
pub use protocol::*;
pub use server::*;
pub use snapshot::*;
pub(crate) use state::IpcState;
