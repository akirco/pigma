//! Process-level memory helper: heap reclamation.

/// Ask glibc to release free heap pages back to the OS.
///
/// glibc's allocator keeps freed blocks on per-arena free lists and only returns
/// them to the OS opportunistically, so a long-lived process that repeatedly
/// allocates and frees large buffers (decoded audio, streamed chunks, cover
/// bitmaps) sees RSS ratchet upward even though the memory is no longer live.
/// `malloc_trim(0)` walks the top of the heap and releases that slack.
///
/// Called at song boundaries, where a burst of large allocations has just been
/// freed and the process is otherwise idle — not on a hot path.
///
/// No-op on non-glibc targets (notably musl), where `libc::malloc_trim` does not
/// exist.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn trim_heap() {
    // SAFETY: `malloc_trim` takes no arguments that could be invalid, touches
    // only the calling process's own heap, and cannot fail in a way that leaves
    // the allocator in an inconsistent state. It is safe to call repeatedly and
    // from any thread.
    unsafe {
        libc::malloc_trim(0);
    }
}

/// No-op counterpart of [`trim_heap`] for platforms without glibc `malloc_trim`.
#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub fn trim_heap() {}

#[cfg(test)]
mod tests {
    #[test]
    fn trim_heap_is_safe_to_call() {
        // Exercises both the glibc and the no-op cfg branch.
        super::trim_heap();
        super::trim_heap();
    }
}
