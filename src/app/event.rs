//! Application event loop and its per-family handlers, grouped in submodules:
//! - `dispatch`: channel pump + top-level routing
//! - `ipc`: IPC control requests (`pigma msg`)
//! - `playback`: playback engine events
//! - `like`: optimistic liked-state updates

mod dispatch;
mod ipc;
mod like;
mod playback;

pub(super) use dispatch::send_event;
