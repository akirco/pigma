//! Persistent audio player task: owns the output sink and drives playback
//! across song switches, device loss and default-device changes.
//!
//! Submodules:
//! - `reader`: seekable reader shared with the seek controller
//! - `device`: cpal sink creation, device health and stream-error handling
//! - `task`: player loop state, ticks and device following
//! - `sink`: sink lifecycle and (re)starting the rodio player
//! - `control`: control-command handling

mod control;
mod device;
mod reader;
mod sink;
mod task;

pub(super) use reader::{AudioInput, AudioReader, SharedReader};
pub(super) use task::{ControlCmd, run};
