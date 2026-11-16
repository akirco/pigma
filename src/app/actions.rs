//! User-facing playback/UI actions shared by keyboard, mouse and IPC input.
//! Keeping them here means the input layer only maps keys to actions.

use ratatui::layout::Rect;

use super::App;
use crate::{
    event::{CommandPanelAction, PlaybackEvent},
    service::ApiEndpoint,
    state::{CommandAction, ContentState, Page},
    text_input::TextInput,
};

impl App {
    pub(crate) fn toggle_playback(&mut self) {
        let was_paused = self.playback.state.paused;
        self.playback.toggle_pause();
        if let Some(song) = self.playback.current_song() {
            if was_paused {
                self.toast(format!("{}  {}", crate::icons::CONTROL_PLAY, song.name));
            } else {
                self.toast(format!("{}  {}", crate::icons::CONTROL_PAUSE, song.name));
            }
        }
    }

    /// Cycle the playback mode and show the same toast as the main page.
    pub(crate) fn cycle_play_mode(&mut self) {
        let mode = self.playback.cycle_mode();
        let (icon, label) = crate::playback::mode_icon(&mode);
        self.toast(format!("{icon} 循环: {label}"));
    }

    /// Adjust playback volume by `delta` (fraction of 0..=1), clamped to bounds
    /// and surfaced as a toast. Keyboard `+`/`-` mirrors the playerbar scroll.
    pub fn adjust_volume(&mut self, delta: f64) {
        let new = (self.playback.state.volume + delta).clamp(0.0, 1.0);
        self.playback.set_volume(new);
        self.toast(format!(
            " {}  {:.0}%",
            crate::icons::VOLUME_HIGH,
            new * 100.0
        ));
    }

    /// Adjust the playback speed by `delta` (steps of 0.1), clamped to the
    /// supported range and surfaced as a toast. Keyboard `[`/`]`.
    pub fn adjust_speed(&mut self, delta: f64) {
        self.playback.adjust_speed(delta);
        self.speed_toast();
    }

    /// Report the current speed the way the player bar shows it: `*1.2x` when
    /// the pitch is kept, `1.2x` when the speed change shifts it.
    pub fn speed_toast(&mut self) {
        let badge = self.playback.state.speed_badge();
        self.toast(format!("{} 速度 {badge}", crate::icons::SPEED));
    }

    /// Report the current speed/pitch mode: keeping the pitch (WSOLA stretch)
    /// vs. letting the speed shift it (resampling). A bare `*1.0x` badge does
    /// not tell the two apart at normal speed, so spell the mode out.
    pub fn pitch_toast(&mut self) {
        let mode = if self.playback.state.pitch_locked {
            format!("{} 变速不变调（保持音高）", crate::icons::SPEED)
        } else {
            format!("{} 变速也变调（音高跟随速度）", crate::icons::SPEED)
        };
        self.toast(mode);
    }

    /// Flip between pitch-keeping (WSOLA stretch) and pitch-shifting
    /// (resampling) speed. Keyboard `t`.
    pub fn toggle_pitch_locked(&mut self) {
        self.playback.toggle_pitch_locked();
        self.pitch_toast();
    }

    pub fn cycle_nav_position(&mut self) {
        self.config.navigation_position = self.config.navigation_position.cycle();
        self.config.save();
        let pos = self.config.navigation_position;
        self.toast(format!(
            "{} 导航栏位置: {}",
            crate::icons::NAV_POSITION,
            pos.label()
        ));
    }

    /// The song under the cursor in the content list, when the page shows songs.
    fn selected_song(&self) -> Option<&std::sync::Arc<ncm_api::SongInfo>> {
        match self.state.navigation.content.as_ref() {
            ContentState::Songs(songs) => songs.get(self.state.navigation.content_selected),
            _ => None,
        }
    }

    /// Like the playing song (`S`).
    pub(crate) fn like_current_song(&mut self) {
        let Some((id, name)) = self.playback.current_song().map(|s| (s.id, s.name.clone())) else {
            return;
        };
        self.events.send(PlaybackEvent::LikeSong(id, true));
        self.toast(format!("{}  {}", crate::icons::HEART_FILLED, name));
    }

    /// Like the selected row (`s`).
    pub(crate) fn like_selected_song(&mut self) {
        let Some((id, name)) = self.selected_song().map(|s| (s.id, s.name.clone())) else {
            return;
        };
        self.events.send(PlaybackEvent::LikeSong(id, true));
        self.toast(format!("{}  {}", crate::icons::HEART_FILLED, name));
    }

    /// Unlike the playing song (`D`).
    pub(crate) fn unlike_current_song(&mut self) {
        let Some((id, name)) = self.playback.current_song().map(|s| (s.id, s.name.clone())) else {
            return;
        };
        self.events.send(PlaybackEvent::LikeSong(id, false));
        self.toast(format!(
            "{} 已取消喜欢: {}",
            crate::icons::HEART_OUTLINE,
            name
        ));
    }

    /// `d` on the selected row: dislike it on the recommendation page so it
    /// stops being suggested, otherwise read the key as "unlike".
    pub(crate) fn dislike_selected_song(&mut self) {
        let Some((id, name)) = self.selected_song().map(|s| (s.id, s.name.clone())) else {
            return;
        };
        if self.selected_endpoint() == Some(ApiEndpoint::RecommendSongs) {
            self.events.send(PlaybackEvent::DislikeSong(id));
            self.toast(format!("{}  {}", crate::icons::ERROR_MARK, name));
        } else {
            self.events.send(PlaybackEvent::LikeSong(id, false));
            self.toast(format!(
                "{} 已取消喜欢: {}",
                crate::icons::HEART_OUTLINE,
                name
            ));
        }
    }

    /// Queue the selected row right after the current song (`a`).
    pub(crate) fn add_selected_next(&mut self) {
        let Some(song) = self.selected_song().cloned() else {
            return;
        };
        let name = song.name.clone();
        self.playback.add_next(song);
        self.toast(format!("{}  下一首: {name}", crate::icons::NEXT_INDICATOR));
    }

    /// Cycle the queues listed on the playlist page (`Tab`/`BackTab`), moving
    /// the cursor to whatever that queue is playing.
    pub(crate) fn switch_queue_page(&mut self, forward: bool) {
        if let Some(key) = self.playback.switch_queue(forward) {
            self.state.navigation.playlist_selected =
                self.playback.queue_current_index().unwrap_or(0);
            self.toast(format!("{} 队列: {key}", crate::icons::QUEUE));
        }
    }

    /// Switch to the queue tab under the pointer on the playlist page. Returns
    /// whether the click landed on the tab bar, so the row handler does not
    /// also run.
    pub(crate) fn queue_tab_click(&mut self, col: u16, row: u16) -> bool {
        let nav = &self.state.navigation;
        if nav.queue_tab_area.width == 0 || !nav.queue_tab_area.contains((col, row).into()) {
            return false;
        }
        let x = (col - nav.queue_tab_area.x) as usize + nav.queue_tab_scroll_x as usize;
        let tab = nav
            .queue_tab_hits
            .iter()
            .find(|hit| hit.columns.contains(&x))
            .map(|hit| hit.index);
        if let Some(tab) = tab {
            self.switch_queue_index(tab);
        }
        true
    }

    /// Switch to the queue tab at `index` (mouse click on its name).
    pub(crate) fn switch_queue_index(&mut self, index: usize) {
        if let Some(key) = self.playback.switch_to_queue(index) {
            self.state.navigation.playlist_selected =
                self.playback.queue_current_index().unwrap_or(0);
            self.toast(format!("{} 队列: {key}", crate::icons::QUEUE));
        }
    }

    /// A left click on the player bar: play/pause, previous/next, like toggle,
    /// play-mode cycle, or seek to the clicked point on the progress gauge.
    pub(crate) fn playerbar_click(&mut self, col: u16, row: u16) {
        let hits = self.playerbar_hits();
        let point = (col, row).into();
        if hits.play.contains(point) {
            self.toggle_playback();
        } else if hits.prev.contains(point) {
            self.playback.prev();
        } else if hits.next.contains(point) {
            self.playback.next();
        } else if hits.cover.contains(point) {
            self.toggle_lyrics_transition();
        } else if hits.like.contains(point) {
            if self.playback.state.liked {
                self.unlike_current_song();
            } else {
                self.like_current_song();
            }
        } else if hits.mode.contains(point) {
            self.cycle_play_mode();
        } else if hits.progress.contains(point) {
            self.seek_to_point(col, hits.progress);
        }
    }

    /// Seek to the fraction of the current song under `col` in the progress
    /// gauge. `seek_relative` is the only seek entry point, so pass the delta.
    fn seek_to_point(&mut self, col: u16, area: Rect) {
        if area.width == 0 {
            return;
        }
        let Some(song) = self.playback.current_song() else {
            return;
        };
        let total_secs = song.duration as f64 / 1000.0;
        if total_secs <= 0.0 {
            return;
        }
        let ratio = ((col - area.x) as f64 / area.width as f64).clamp(0.0, 1.0);
        let current = self.playback.state.progress * total_secs;
        self.playback.seek_relative(ratio * total_secs - current);
    }

    /// Empty every queue but the active one (`w`), then focus whichever tab is
    /// left so the playlist page never ends up with no focus.
    pub(crate) fn clear_play_queue(&mut self) {
        self.playback.clear_queue();
        let mut message = format!(" {}  已清空播放队列", crate::icons::CLEAR_QUEUE);
        if self.state.navigation.page == Page::Playlist {
            // After clearing, only one queue remains: switch_queue returns None
            // when only one is left, so focus it explicitly (the last/rightmost
            // tab) to avoid an empty focus.
            let key = match self.playback.switch_queue(false) {
                Some(key) => Some(key),
                None => {
                    let last = self.playback.queue_keys().last().cloned();
                    if let Some(ref key) = last {
                        self.playback.activate_queue(key);
                    }
                    last
                }
            };
            if let Some(key) = key {
                self.state.navigation.playlist_selected =
                    self.playback.queue_current_index().unwrap_or(0);
                message = format!("{} 已清空播放队列 · 队列: {key}", crate::icons::QUEUE);
            }
        }
        // One toast only: a second one would overwrite `toast_msg` before the
        // frame is drawn, so the "cleared" feedback was never visible.
        self.toast(message);
    }

    /// Start filtering the playlist page queue (`/`): snapshot the queue so
    /// leaving the filter can restore it, then open an empty input.
    pub(crate) fn begin_queue_filter(&mut self) {
        let songs = self.playback.queue_songs();
        self.state.navigation.search.unfiltered_songs = Some(songs.to_vec());
        self.state.navigation.search.filter_queue_only = true;
        self.state.navigation.search.active = true;
        self.state.navigation.search.input = TextInput::new();
    }

    /// Command-panel navigation state machine. Called from the event router in
    /// `app/event/dispatch.rs`, so `pub(super)` keeps it visible to `app::*`.
    pub(super) fn handle_command_panel(&mut self, action: CommandPanelAction) {
        let panel = &mut self.state.command_panel;
        match action {
            CommandPanelAction::Open => {
                panel.open = true;
                panel.selected = 0;
            }
            CommandPanelAction::Close => panel.back(),
            CommandPanelAction::Previous => {
                if let Some(items) = panel.current_items() {
                    let len = items.len();
                    if len > 0 {
                        // `% len` first, so a stale selected index from a
                        // previous menu level cannot overflow the addition.
                        panel.selected = (panel.selected % len + len - 1) % len;
                    }
                }
            }
            CommandPanelAction::Next => {
                if let Some(items) = panel.current_items() {
                    let len = items.len();
                    if len > 0 {
                        panel.selected = (panel.selected + 1) % len;
                    }
                }
            }
            CommandPanelAction::Select => {
                let action = panel.enter();
                if action.is_some() {
                    panel.open = false;
                }
                if let Some(action) = action {
                    self.execute_command(action);
                }
            }
        }
    }

    /// Shared by the command panel and the `b` shortcut, so the border toggle
    /// has one implementation (and one toast) instead of two divergent paths.
    pub(super) fn execute_command(&mut self, action: CommandAction) {
        match action {
            CommandAction::ToggleBordered => {
                self.state.border.enabled = !self.state.border.enabled;
                self.toast(format!(
                    "BORDER: {}",
                    if self.state.border.enabled {
                        "ON"
                    } else {
                        "OFF"
                    }
                ));
            }
            CommandAction::ToggleSaveOnPlay => {
                let enabled = !self.config.cache.save_on_play;
                self.config.cache.save_on_play = enabled;
                self.playback.set_save_on_play(enabled);
                self.config.save();
                self.toast(format!("边听边存: {}", if enabled { "ON" } else { "OFF" }));
            }
            CommandAction::CycleNavPosition => self.cycle_nav_position(),
            CommandAction::SwitchTheme(name) => {
                let msg = format!("THEME: {name}");
                self.config.default_theme = name;
                self.config.save();
                self.toast(msg);
            }
        }
    }
}
