//! Seekable audio reader shared between the player task and the seek
//! controller. The lock is poison-recovering so a panic inside a guarded read
//! can never permanently silence playback.

use std::{
    io::{Read, Seek, SeekFrom},
    sync::{Arc, Mutex},
};

use crate::lock::lock_recover;

pub(crate) trait AudioReader: Read + Seek + Send + Sync {}
impl<T: Read + Seek + Send + Sync> AudioReader for T {}

pub(crate) type AudioInput = SharedReader;

/// A reader wrapper that can be shared between the player and the seek controller.
/// On seek, the controller locks the shared reader, seeks to the new position,
/// then creates a new decoder + player from the seeked position.
///
/// The lock is taken through [`lock_recover`], so a panic inside the guarded
/// region cannot poison it: a poisoned `std::sync::Mutex` would make every
/// later read fail with "audio reader lock poisoned" and the audio could never
/// recover without restarting the process.
///
/// Note that the guard is held across the whole read, and a streaming read
/// blocks until the network download reaches the requested byte offset
/// (`stream-download` waits on a condvar). The lock is therefore held for
/// seconds at a time while the network catches up — the seek path competes
/// for it.
#[derive(Clone)]
pub(crate) struct SharedReader(pub Arc<Mutex<Box<dyn AudioReader + 'static>>>);

impl std::fmt::Debug for SharedReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedReader").finish_non_exhaustive()
    }
}

impl Read for SharedReader {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // `lock()` is a single atomic on the (uncontended) steady-state path and
        // needs no allocation.
        lock_recover(&self.0).read(buf)
    }
}

impl Seek for SharedReader {
    #[inline]
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        lock_recover(&self.0).seek(pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A panic while the reader lock is held must not permanently break the
    /// reader: a poisoned `std::sync::Mutex` would make every later read fail
    /// with "audio reader lock poisoned", and the audio could then never
    /// recover without restarting the process.
    #[test]
    fn reader_survives_a_panicking_guard() {
        let mut reader = SharedReader(Arc::new(Mutex::new(
            Box::new(Cursor::new(b"hello".to_vec())) as Box<dyn AudioReader>,
        )));

        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = lock_recover(&reader.0);
            panic!("panic while holding the reader lock");
        }));
        assert!(panicked.is_err(), "the guard was expected to panic");

        // The lock is poisoned now, but the reader must still work.
        let mut buf = [0u8; 5];
        reader
            .read_exact(&mut buf)
            .expect("read after a poisoned lock");
        assert_eq!(&buf, b"hello");
    }
}
