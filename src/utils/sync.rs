//! Small synchronization helpers.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Lock `mutex`, recovering the guard even if the lock was poisoned.
///
/// The mutexes in this crate guard a log-file handle, the IPC status/queue
/// snapshots, the in-memory liked/playlist/search registries, the cover state
/// and the shared audio reader. A panic while one is held would otherwise turn
/// into a second panic (or, worse, a silent no-op) on the next access;
/// recovering is safe here because
///
/// - the log file is append-only,
/// - the snapshots and registries are fully replaced or modified by single
///   infallible map/set operations, and
/// - the audio reader is a byte stream whose position is re-established by the
///   player on the next `SeekTo`/rebuild. Refusing to lock a poisoned reader
///   would be far worse than carrying on with it: every later read would fail
///   and the audio could never recover without restarting the process.
pub fn lock_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
