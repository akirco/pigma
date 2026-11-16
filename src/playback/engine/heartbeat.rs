//! Heartbeat play mode: fetch the next song from the heartbeat playlist API
//! and queue it, falling back to sequential playback on failure.

use std::sync::Arc;

use ncm_api::SongInfo;

use super::{PlaybackEngine, mode::PlayMode};
use crate::event::PlaybackEvent;

const MAX_HEARTBEAT_SONGS: usize = 500;
const KEEP_RECENT: usize = 100;

/// Tracks the in-flight heartbeat request so a late API response cannot change
/// the song after the user skipped, paused or left heartbeat mode.
#[derive(Default)]
pub(super) struct HeartbeatGate(u64);

impl HeartbeatGate {
    /// Start a new request, invalidating any earlier one.
    pub(super) fn start(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1);
        self.0
    }

    /// Invalidate the current request without starting a new one (song change,
    /// pause, manual navigation, mode switch).
    pub(super) fn invalidate(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }

    pub(super) fn is_current(&self, request: u64) -> bool {
        request == self.0
    }
}

impl PlaybackEngine {
    pub(super) fn next_heartbeat(&mut self) {
        let song = match self.queue.current_song() {
            Some(s) => s.clone(),
            None => return,
        };
        let playlist_id = match self.state.mode {
            PlayMode::Heartbeat { playlist_id } => playlist_id,
            _ => return,
        };

        let request = self.heartbeat.start();
        log::info!("Heartbeat: song_id={} playlist_id={}", song.id, playlist_id);

        let service = self.service.clone();
        let event_tx = self.event_tx.clone();

        tokio::spawn(async move {
            match service.heartbeat_songs(song.id, playlist_id).await {
                Ok(songs) => {
                    log::info!(
                        "Heartbeat API returned {} songs for song_id={}, playlist_id={}",
                        songs.len(),
                        song.id,
                        playlist_id
                    );
                    if let Some(next_song) = songs.into_iter().next() {
                        if event_tx
                            .send(PlaybackEvent::HeartbeatSong {
                                request,
                                song: Arc::new(next_song),
                            })
                            .is_err()
                        {
                            log::error!("Failed to send HeartbeatSong: receiver dropped");
                        }
                    } else {
                        log::warn!(
                            "Heartbeat API returned empty list for song_id={}, playlist_id={}",
                            song.id,
                            playlist_id
                        );
                        if event_tx
                            .send(PlaybackEvent::HeartbeatFallback { request })
                            .is_err()
                        {
                            log::error!("Failed to send HeartbeatFallback: receiver dropped");
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Heartbeat failed: {e}, falling back to queue");
                    if event_tx
                        .send(PlaybackEvent::HeartbeatFallback { request })
                        .is_err()
                    {
                        log::error!("Failed to send HeartbeatFallback: receiver dropped");
                    }
                }
            }
        });
    }

    pub fn play_heartbeat_song(&mut self, request: u64, song: Arc<SongInfo>) {
        if !self.heartbeat.is_current(request) {
            log::debug!("Heartbeat: ignoring stale song response");
            return;
        }
        self.controller.stop();
        self.queue.push_to_history();

        if self.queue.songs.len() >= MAX_HEARTBEAT_SONGS {
            let current = self.queue.current_index.unwrap_or(0);
            // Trim songs from the front, but keep a buffer of recently played
            // songs so that "previous" navigation still works.
            let trim_to = current.saturating_sub(KEEP_RECENT);
            if trim_to > 0 {
                self.queue.songs.drain(..trim_to);
                if let Some(ref mut idx) = self.queue.current_index {
                    *idx = idx.saturating_sub(trim_to);
                }
            }
        }

        self.queue.songs.push(song);
        self.queue.rebuild_index();
        self.queue.current_index = Some(self.queue.len() - 1);
        // Appending outside the queue's own mutation helpers must still bump the
        // version, or the IPC `status -L` snapshot never refreshes in heartbeat
        // mode.
        self.queue.bump();
        self.start_current_song(None);
    }

    pub fn on_heartbeat_fallback(&mut self, request: u64) {
        if !self.heartbeat.is_current(request) {
            log::debug!("Heartbeat: ignoring stale fallback response");
            return;
        }
        log::info!("Heartbeat fallback: switching to sequential");
        self.set_mode(PlayMode::Sequential);
        self.next();
    }
}

#[cfg(test)]
mod tests {
    use super::HeartbeatGate;

    #[test]
    fn only_the_latest_request_is_current() {
        let mut gate = HeartbeatGate::default();
        let first = gate.start();
        let second = gate.start();

        assert!(!gate.is_current(first), "an older response must be stale");
        assert!(gate.is_current(second));

        gate.invalidate();
        assert!(
            !gate.is_current(second),
            "invalidation must drop the request"
        );
    }
}
