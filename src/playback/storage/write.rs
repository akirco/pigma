//! Queue snapshot persistence: atomic file writes and the sequence gate that
//! keeps disk order aligned with save order.

use std::{
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use crate::lock::lock_recover;

/// Write a queue file and its active-queue marker atomically.
///
/// Both go through [`crate::persist::write_atomic`] (temp file + fsync + rename),
/// so a crash mid-save leaves the previous queue/session loadable instead of a
/// truncated JSON file that fails to restore on the next start.
fn write_queue_files(queue_path: &Path, active_path: &Path, id: &str, json: &str) -> bool {
    let queue_ok = match crate::persist::write_atomic(queue_path, json.as_bytes()) {
        Ok(()) => true,
        Err(e) => {
            log::warn!("Failed to save queue {}: {e}", queue_path.display());
            false
        }
    };
    let active_ok = match crate::persist::write_atomic(active_path, id.as_bytes()) {
        Ok(()) => true,
        Err(e) => {
            log::warn!("Failed to save active queue marker: {e}");
            false
        }
    };
    // Both files must match: advancing the save sequence after a partial write
    // would let a stale queue stay paired with a newer active-queue marker.
    queue_ok && active_ok
}

/// Serializes queue snapshots so disk order always follows save order.
///
/// `save_queue` snapshots are written from detached blocking tasks, which may
/// finish in any order; without a guard an older snapshot can land after a
/// newer one (and after the shutdown save), so the next start restores the
/// wrong queue or playhead. Each save takes a monotonically increasing sequence
/// and only the highest sequence is allowed to write.
#[derive(Default)]
pub(super) struct SaveGate {
    next_seq: AtomicU64,
    last_written: Mutex<u64>,
}

impl SaveGate {
    pub(super) fn next_seq(&self) -> u64 {
        self.next_seq.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Skip work for a snapshot that is already superseded.
    pub(super) fn is_stale(&self, seq: u64) -> bool {
        seq <= *lock_recover(&self.last_written)
    }

    pub(super) fn commit(
        &self,
        seq: u64,
        queue_path: &Path,
        active_path: &Path,
        id: &str,
        json: &str,
    ) {
        // Hold the lock across the check + write: a lower sequence that reaches
        // the lock later must not overwrite the newer snapshot.
        let mut last = lock_recover(&self.last_written);
        if seq <= *last {
            return;
        }
        if write_queue_files(queue_path, active_path, id, json) {
            *last = seq;
        }
    }
}
