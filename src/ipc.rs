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
//! on Windows. The endpoint is user-scoped so no authentication is needed.

pub mod client;
pub mod path;
pub mod protocol;
pub mod server;
pub mod snapshot;

pub use client::*;
pub use path::*;
pub use protocol::*;
pub use server::*;
pub use snapshot::*;
