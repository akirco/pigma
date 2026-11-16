//! IPC control requests (`pigma msg ...`) mapped onto `App` actions.

use super::super::App;
use crate::{
    event::IpcEvent,
    lock::lock_recover,
    playback::{NCM_SEARCH_QUEUE_KEY, THIRD_PARTY_QUEUE_KEY},
};

impl App {
    /// Apply a control request received over the IPC socket (`pigma msg`).
    pub(super) fn handle_ipc_event(&mut self, event: IpcEvent) {
        match event {
            IpcEvent::Previous => self.playback.prev(),
            IpcEvent::Next => self.playback.next(),
            IpcEvent::Pause => {
                // Pause only pauses; when stopped it stays stopped (unlike the
                // TUI spacebar which toggles/start).
                if self.playback.state.playing && !self.playback.state.paused {
                    self.playback.toggle_pause();
                }
            }
            IpcEvent::Play { song_id } => {
                if let Some(id) = song_id {
                    // Jump to a song in the active queue and play it. Songs
                    // returned by `pigma msg search` are not queued, so fall
                    // back to the shared search-result registry and enqueue the
                    // result (sonar songs keep their synthetic id, which the
                    // playback source resolves via `sonar_songs`).
                    let fallback = lock_recover(&self.search.results).get(&id).cloned();
                    if !self.playback.play_song_by_id(id)
                        && let Some(song) = fallback
                    {
                        let key = if sonar::is_sonar_song_id(id) {
                            THIRD_PARTY_QUEUE_KEY
                        } else {
                            NCM_SEARCH_QUEUE_KEY
                        };
                        self.playback.append_and_play_key(key, &[song], 0);
                    }
                    let name = self
                        .playback
                        .current_song()
                        .map(|s| s.name.clone())
                        .unwrap_or_default();
                    if name.is_empty() {
                        self.toast(format!("找不到 id={id} 的歌曲"));
                    } else {
                        self.toast(format!("{} 正在播放: {name}", crate::icons::MUSIC_NOTE));
                    }
                } else if self.playback.state.paused || !self.playback.state.playing {
                    // Resume when paused; start when stopped (if a song is queued).
                    self.playback.toggle_pause();
                }
            }
            IpcEvent::TogglePlay => self.playback.toggle_pause(),
            IpcEvent::Volume { delta, absolute } => {
                if let Some(delta) = delta {
                    self.adjust_volume(delta);
                } else if let Some(volume) = absolute {
                    let volume = volume.clamp(0.0, 1.0);
                    self.playback.set_volume(volume);
                    self.toast(format!(
                        " {}  {:.0}%",
                        crate::icons::VOLUME_HIGH,
                        volume * 100.0
                    ));
                }
            }
            IpcEvent::Speed { delta, absolute } => {
                if let Some(delta) = delta {
                    self.adjust_speed(delta);
                } else if let Some(speed) = absolute {
                    self.playback.set_speed(speed);
                    self.speed_toast();
                }
            }
            IpcEvent::Pitch { locked } => {
                match locked {
                    Some(locked) => self.playback.set_pitch_locked(locked),
                    None => {
                        self.playback.toggle_pitch_locked();
                    }
                }
                self.pitch_toast();
            }
            IpcEvent::Mode => {
                let mode = self.playback.cycle_mode();
                let (_, label) = crate::playback::mode_icon(&mode);
                self.toast(format!("播放模式: {label}"));
            }
            IpcEvent::Like => {
                if let Some(song) = self.playback.current_song() {
                    self.events
                        .send(crate::event::PlaybackEvent::LikeSong(song.id, true));
                }
            }
            IpcEvent::Dislike => {
                if let Some(song) = self.playback.current_song() {
                    self.events
                        .send(crate::event::PlaybackEvent::DislikeSong(song.id));
                }
            }
            IpcEvent::ToggleLike => {
                if let Some(song) = self.playback.current_song() {
                    let like = !self.playback.state.liked;
                    self.events
                        .send(crate::event::PlaybackEvent::LikeSong(song.id, like));
                }
            }
            IpcEvent::SwitchList { endpoint, playlist } => {
                // Resolve in the background; the completion arrives as
                // `NavigationEvent::EndpointResolved`, so a slow network cannot
                // freeze the render loop.
                self.spawn_endpoint_load(endpoint, playlist);
            }
        }
    }
}
