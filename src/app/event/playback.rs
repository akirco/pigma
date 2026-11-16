//! Playback event handling: engine progress/state changes, queue updates and
//! liked-state updates from background tasks.

use super::{super::App, dispatch::send_event};
use crate::event::{AppEvent, PlaybackEvent};

impl App {
    pub(super) fn handle_playback_event(&mut self, event: PlaybackEvent) {
        match event {
            PlaybackEvent::SongPlay(id) => self.handle_song_play(id),
            PlaybackEvent::Started => self.handle_playback_started(),
            PlaybackEvent::Progress { position, total } => {
                self.playback.on_playback_progress(position, total);
            }
            PlaybackEvent::Finished => {
                self.playback.finish_and_snapshot();
            }
            PlaybackEvent::Error(e) => {
                self.playback.on_playback_error(e);
            }
            PlaybackEvent::DeviceError(message) => {
                self.playback.on_device_error(message);
            }
            PlaybackEvent::LyricsLoaded {
                song_id,
                lyrics,
                translated_lyrics,
            } => self
                .playback
                .on_lyrics_loaded(song_id, lyrics, translated_lyrics),
            PlaybackEvent::HeartbeatSong { request, song } => {
                self.playback.play_heartbeat_song(request, song);
            }
            PlaybackEvent::HeartbeatFallback { request } => {
                self.playback.on_heartbeat_fallback(request);
            }
            PlaybackEvent::SetPlaylistId(id) => {
                // After the content (re)loads, the previous "全量已入队" marker is stale.
                self.queued_playlists.remove(&id);
                self.playback.set_playlist_id(id);
            }
            PlaybackEvent::LikeSong(id, like) => self.handle_like_song(id, like),
            PlaybackEvent::LikedUpdated => {
                self.playback.update_liked_status();
            }
            PlaybackEvent::DislikeSong(id) => {
                let service = self.service.clone();
                let sender = self.events.sender();
                tokio::spawn(async move {
                    if let Err(e) = service.dislike_song(id).await {
                        log::warn!("Dislike failed: {e}");
                        send_event(
                            &sender,
                            AppEvent::Toast(format!(
                                "{} 不感兴趣失败: {e}",
                                crate::icons::ERROR_MARK
                            ))
                            .into(),
                        );
                    }
                });
            }
            PlaybackEvent::Cached(song_id) => {
                if self
                    .playback
                    .state
                    .current_song
                    .as_ref()
                    .is_some_and(|s| s.id == song_id)
                {
                    self.playback.state.cached = true;
                }
            }
            PlaybackEvent::QueueAppend { key, songs } => {
                self.playback.append_songs_to_key(&key, songs);
            }
            PlaybackEvent::QueueLoadDone { playlist_id } => {
                self.queued_playlists.insert(playlist_id);
            }
        }
    }
}
