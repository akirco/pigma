//! User-facing playback/UI actions shared by keyboard, mouse and IPC input.
//! Keeping them here means the input layer only maps keys to actions.

use super::App;
use crate::{event::CommandPanelAction, state::CommandAction};

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
                    panel.selected = (panel.selected + len - 1) % len;
                }
            }
            CommandPanelAction::Next => {
                if let Some(items) = panel.current_items() {
                    let len = items.len();
                    panel.selected = (panel.selected + 1) % len;
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

    fn execute_command(&mut self, action: CommandAction) {
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
