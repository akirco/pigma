use std::sync::Arc;

use ncm_api::SongInfo;

use crate::event::{AppEvent, Event};

use super::PlaybackEngine;
use super::types::PlayMode;

const MAX_HEARTBEAT_SONGS: usize = 500;

impl PlaybackEngine {
    pub fn next_heartbeat(&mut self) {
        let song = match self.queue.current_song() {
            Some(s) => s.clone(),
            None => return,
        };
        let playlist_id = match self.state.mode {
            PlayMode::Heartbeat { playlist_id } => playlist_id,
            _ => return,
        };

        log::info!("Heartbeat: song_id={} playlist_id={}", song.id, playlist_id);

        let api = self.api.clone();
        let event_tx = self.event_tx.clone();

        tokio::spawn(async move {
            match api.playmode_intelligence_list(song.id, playlist_id).await {
                Ok(songs) => {
                    log::info!(
                        "Heartbeat API returned {} songs for song_id={}, playlist_id={}",
                        songs.len(),
                        song.id,
                        playlist_id
                    );
                    if let Some(next_song) = songs.into_iter().next() {
                        if event_tx
                            .send(Event::App(AppEvent::HeartbeatSong(next_song)))
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
                            .send(Event::App(AppEvent::HeartbeatFallback))
                            .is_err()
                        {
                            log::error!("Failed to send HeartbeatFallback: receiver dropped");
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Heartbeat failed: {e}, falling back to queue");
                    if event_tx
                        .send(Event::App(AppEvent::HeartbeatFallback))
                        .is_err()
                    {
                        log::error!("Failed to send HeartbeatFallback: receiver dropped");
                    }
                }
            }
        });
    }

    pub fn play_heartbeat_song(&mut self, song: SongInfo) {
        self.snapshot_report();
        self.controller.stop();
        self.queue.push_to_history();

        if self.queue.songs.len() >= MAX_HEARTBEAT_SONGS {
            let current = self.queue.current_index.unwrap_or(0);
            let trim_to = current.saturating_sub(MAX_HEARTBEAT_SONGS / 4);
            if trim_to > 0 {
                self.queue.songs.drain(..trim_to);
                if let Some(ref mut idx) = self.queue.current_index {
                    *idx = idx.saturating_sub(trim_to);
                }
            }
        }

        self.queue.songs.push(Arc::new(song));
        self.queue.current_index = Some(self.queue.len() - 1);
        self.start_current_song(None);
    }

    pub fn on_heartbeat_fallback(&mut self) {
        log::info!("Heartbeat fallback: switching to sequential");
        self.set_mode(PlayMode::Sequential);
        self.next();
    }
}
