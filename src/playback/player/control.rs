//! Control-command handling for the player task: switch/seek/stop and volume,
//! speed and speed-mode changes.

use std::{
    io::SeekFrom,
    sync::{Arc, Mutex, atomic::Ordering},
    time::Duration,
};

use super::{
    reader::SharedReader,
    task::{ControlCmd, PlayerTask},
};
use crate::lock::lock_recover;

impl PlayerTask {
    /// Apply a control command. Returns `true` when the caller should skip the
    /// rest of this tick (the seek command does when there is nothing playing).
    pub(super) fn handle_control(&mut self, cmd: ControlCmd) -> bool {
        match cmd {
            ControlCmd::Switch(input, seek_time) => {
                if let Some(ref p) = self.player {
                    p.stop();
                }
                self.player = None;
                self.reader = input;
                self.pending_resume = None;
                self.player = if self.ensure_sink() {
                    self.start_playback(seek_time)
                } else {
                    self.pending_resume = Some((seek_time, false));
                    None
                };
            }
            ControlCmd::SeekTo(seek_time) => {
                if self.player.is_none() {
                    return true;
                }
                // A seek rebuilds the decoder/player, and the fresh rodio player
                // starts playing. Preserve the paused state so seeking while
                // paused never starts audio behind the UI's back.
                let was_paused = self.player.as_ref().is_some_and(|p| p.is_paused());
                if let Some(ref p) = self.player {
                    p.stop();
                }
                self.player = None;
                let _ = lock_recover(&self.reader.0).seek(SeekFrom::Start(0));
                self.pending_resume = None;
                self.player = if self.ensure_sink() {
                    self.start_playback(Some(seek_time))
                } else {
                    self.pending_resume = Some((Some(seek_time), was_paused));
                    None
                };
                if was_paused && let Some(ref p) = self.player {
                    p.pause();
                }
            }
            ControlCmd::Stop => {
                if let Some(ref p) = self.player {
                    p.stop();
                }
                self.player = None;
                self.pending_resume = None;
                self.health.device_lost.store(false, Ordering::Relaxed);
                self.clear_device_error();
                self.reader = SharedReader(Arc::new(Mutex::new(Box::new(std::io::empty()))));
                self.total_duration = None;
                self.seek_offset = Duration::default();
            }
            ControlCmd::Pause => {
                if let Some(ref p) = self.player {
                    p.pause();
                }
                // A pause arriving while the device is being reopened must
                // survive into the pending restart.
                if let Some((_, paused)) = self.pending_resume.as_mut() {
                    *paused = true;
                }
            }
            ControlCmd::Resume => {
                if let Some(ref p) = self.player {
                    p.play();
                }
                if let Some((_, paused)) = self.pending_resume.as_mut() {
                    *paused = false;
                }
            }
            ControlCmd::SetVolume(v) => {
                self.volume = v;
                if let Some(ref p) = self.player {
                    p.set_volume(v);
                }
            }
            ControlCmd::SetSpeed(v) => {
                // The running source picks the new factor up at its next block
                // boundary (~46ms); no decoder restart.
                self.speed.set(v);
            }
            ControlCmd::SetSpeedMode(mode) => self.speed.set_mode(mode),
        }
        false
    }
}
