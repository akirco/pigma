//! Stream-download setup: prebuffering, cache finalization and the rodio
//! reader shared with the player task.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use ncm_api::SongInfo;
use stream_download::{
    Settings, StreamDownload, StreamPhase, http::HttpStream, storage::temp::TempStorageProvider,
};
use tokio::sync::watch;

use super::{
    super::{
        player::{AudioInput, AudioReader, SharedReader},
        stream_client::HeadersClient,
    },
    AudioSource,
};
use crate::event::PlaybackEvent;

/// Minimum bytes to pre-buffer before playback. Roughly ~12s of audio at 320kbps and
/// ~32s at 128kbps, leaving headroom for streams that download slower than playback. Playback
/// starts once this threshold is reached or the whole stream has finished downloading.
const PREBUFFER_BYTES: u64 = 512 * 1024;

/// Maximum wait for pre-buffering. When the network is too slow or the stream is broken, don't
/// wait forever — start playback anyway once the timeout is reached.
const PREBUFFER_TIMEOUT: Duration = Duration::from_secs(8);

/// Buffer state shared with the stream download progress callback, used to judge whether
/// enough has been buffered before starting playback.
#[derive(Clone)]
struct StreamProgress {
    /// Number of bytes downloaded (written to storage) so far; see `StreamState.current_position`.
    buffered: Arc<AtomicU64>,
    /// Whether the whole stream has finished downloading.
    completed: Arc<AtomicBool>,
    /// Progress notifications: bumped when the download reaches the prebuffer
    /// target or finishes, so the waiter can be woken instead of polling.
    updates: watch::Receiver<u64>,
}

impl AudioSource {
    /// Build stream-download settings that persist the cache entry and notify
    /// the UI once the stream has finished caching to disk. The entry is only
    /// recorded when the download actually completes, so the index never
    /// contains partial files — those caused cache misses (and a re-download)
    /// on restart.
    ///
    /// Besides the cache bookkeeping it also tracks the download progress so
    /// [`Self::wait_for_prebuffer`] can judge whether enough of the stream is
    /// buffered before playback starts. When `mark_cache` is `false` (streaming to a
    /// temporary file with save-on-play off) the progress is still tracked but nothing is
    /// recorded in the cache index.
    fn tracked_settings(
        &self,
        mark_cache: bool,
        song: &Arc<SongInfo>,
        ext: &'static str,
        mut msong: Option<sonar::Song>,
        part_path: Option<std::path::PathBuf>,
    ) -> (Settings<HttpStream<HeadersClient>>, StreamProgress) {
        let event_tx = self.event_tx.clone();
        let cache = self.cache.clone();
        let sent = Arc::new(AtomicBool::new(false));
        let song = Arc::clone(song);
        let (updates_tx, updates_rx) = watch::channel(0u64);
        let progress = StreamProgress {
            buffered: Arc::new(AtomicU64::new(0)),
            completed: Arc::new(AtomicBool::new(false)),
            updates: updates_rx,
        };
        let buffered = progress.buffered.clone();
        let completed = progress.completed.clone();
        let notified = AtomicBool::new(false);
        let settings = Settings::default().on_progress(move |_, state, _| {
            let position = state.current_position;
            buffered.store(position, Ordering::SeqCst);
            let complete = state.phase == StreamPhase::Complete;
            if complete {
                completed.store(true, Ordering::SeqCst);
            }
            // Signal the prebuffer waiter exactly once, when the exit condition
            // becomes reachable: the download reached the prebuffer target or
            // finished. `watch` keeps the value, so a signal sent before the
            // waiter starts is not lost; after that the waiter is gone and
            // further notifications would be wasted wakes.
            if (complete || position >= PREBUFFER_BYTES) && !notified.swap(true, Ordering::SeqCst) {
                updates_tx.send_replace(position);
            }
            // `thirdparty` is optional: NCM streams have no sonar metadata
            // (`resolve_ncm` passes `None`), but their `.part` file must still
            // be renamed and indexed when save-on-play is enabled. Gating the
            // finalization on `Some(msong)` silently dropped every NCM cache.
            if should_finalize_cache(mark_cache, complete, &sent)
                && cache.mark_cached(&song, ext, msong.take(), part_path.as_deref())
            {
                let _ = event_tx.send(PlaybackEvent::Cached(song.id));
            }
        });
        (settings, progress)
    }

    /// Wait until the stream download has buffered [`PREBUFFER_BYTES`] bytes (or finished
    /// downloading) before returning, giving playback a head buffer so a slow stream download
    /// doesn't underrun at the start. If [`PREBUFFER_TIMEOUT`] is exceeded without reaching the
    /// target (slow network or a broken stream), also return as usual — better to start playing
    /// with occasional stutter than to block playback indefinitely.
    ///
    /// The wait is driven by the download progress callback (which signals once
    /// the target is reached), not by a poll timer — polling would add up to a
    /// full poll interval of latency before playback could start.
    async fn wait_for_prebuffer(&self, progress: &StreamProgress) {
        let deadline = tokio::time::Instant::now() + PREBUFFER_TIMEOUT;
        let mut updates = progress.updates.clone();
        loop {
            if progress.completed.load(Ordering::SeqCst)
                || progress.buffered.load(Ordering::SeqCst) >= PREBUFFER_BYTES
            {
                return;
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            // `watch` keeps the latest value, so a signal that arrives between
            // the check above and this await is not lost.
            match tokio::time::timeout(remaining, updates.changed()).await {
                Ok(Ok(())) => continue,
                // The download was dropped: nothing more will signal us.
                Ok(Err(_)) => return,
                Err(_) => break,
            }
        }
        log::warn!(
            "预缓冲超时（{}），仅缓冲 {} 字节，开始播放",
            PREBUFFER_TIMEOUT.as_secs_f32(),
            progress.buffered.load(Ordering::SeqCst)
        );
    }

    /// Stream `url` through the cache layer. When "边听边存" is enabled the file is
    /// persisted to the download cache (and indexed on completion); otherwise it is
    /// streamed to a temporary file that is cleaned up when playback ends.
    pub(super) async fn build_stream(
        &self,
        url: url::Url,
        song: &Arc<SongInfo>,
        ext: &'static str,
        msong: Option<sonar::Song>,
    ) -> Result<AudioInput, String> {
        let stream = HttpStream::new(HeadersClient::new(self.stream_client.clone()), url)
            .await
            .map_err(|e| format!("流初始化失败: {e}"))?;

        // Save-on-play downloads into a hidden `.part` file; `mark_cached`
        // renames it into place only after the download completes. Streaming
        // without saving uses temp storage and never touches the cache index.
        let (reader, progress): (Box<dyn AudioReader>, StreamProgress) = if self.save_on_play {
            let (provider, part_path) = self
                .cache
                .create_provider(song)
                .map_err(|e| format!("缓存创建失败: {e}"))?;
            let (settings, progress) =
                self.tracked_settings(true, song, ext, msong, Some(part_path));
            let reader = StreamDownload::from_stream(stream, provider, settings)
                .await
                .map_err(|e| format!("流下载失败: {e}"))?;
            (Box::new(reader), progress)
        } else {
            let (settings, progress) = self.tracked_settings(false, song, ext, msong, None);
            let reader =
                StreamDownload::from_stream(stream, TempStorageProvider::default(), settings)
                    .await
                    .map_err(|e| format!("流下载失败: {e}"))?;
            (Box::new(reader), progress)
        };

        self.wait_for_prebuffer(&progress).await;

        Ok(SharedReader(Arc::new(Mutex::new(reader))))
    }
}

/// Whether this progress callback should write the cache index entry.
///
/// `stream-download` reports progress for every chunk from the first prefetch
/// onwards, so the entry must only be finalized once the download really
/// finished: recording it earlier leaves a truncated file indexed as playable
/// when the user skips to another song mid-download.
fn should_finalize_cache(mark_cache: bool, complete: bool, sent: &AtomicBool) -> bool {
    mark_cache && complete && !sent.swap(true, Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::should_finalize_cache;

    #[test]
    fn cache_entry_is_finalized_only_once_the_download_completes() {
        let sent = AtomicBool::new(false);

        // Prefetching/downloading callbacks must not claim the entry...
        assert!(!should_finalize_cache(true, false, &sent));
        assert!(
            !sent.load(Ordering::SeqCst),
            "an incomplete download must not consume the one-shot mark"
        );

        // ...only the completion callback does, and only once.
        assert!(should_finalize_cache(true, true, &sent));
        assert!(!should_finalize_cache(true, true, &sent));
    }

    #[test]
    fn temporary_streams_never_finalize() {
        assert!(!should_finalize_cache(false, true, &AtomicBool::new(false)));
    }
}
