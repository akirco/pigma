//! Equalizer page actions: opening/closing the page, moving the selected band
//! and applying gain changes to the live audio thread.

use super::App;
use crate::{
    config::{BAND_HZ, EqualizerConfig, MAX_GAIN_DB, MIN_GAIN_DB},
    state::Page,
};

impl App {
    pub(crate) fn open_equalizer(&mut self) {
        self.state.navigation.page = Page::Equalizer;
    }

    pub(crate) fn close_equalizer(&mut self) {
        self.save_equalizer();
        self.state.navigation.page = Page::Main;
    }

    /// Copy the live equalizer settings into the config and write the file.
    /// Called when leaving the page or quitting; adjusting a band only updates
    /// the in-memory config so key-repeat does not hammer the disk.
    pub(crate) fn save_equalizer(&mut self) {
        self.config.equalizer = self.playback.equalizer();
        self.config.save();
    }

    pub(crate) fn eq_select_prev(&mut self) {
        let last = BAND_HZ.len() - 1;
        self.state.eq_selected = if self.state.eq_selected == 0 {
            last
        } else {
            self.state.eq_selected - 1
        };
    }

    pub(crate) fn eq_select_next(&mut self) {
        self.state.eq_selected = (self.state.eq_selected + 1) % BAND_HZ.len();
    }

    pub(crate) fn eq_adjust(&mut self, delta: f32) {
        let mut eq = self.playback.equalizer();
        let band = self.state.eq_selected.min(BAND_HZ.len() - 1);
        eq.bands[band] = (eq.bands[band] + delta).clamp(MIN_GAIN_DB, MAX_GAIN_DB);
        self.apply_equalizer(eq);
    }

    pub(crate) fn reset_equalizer(&mut self) {
        let mut eq = self.playback.equalizer();
        eq.preamp_db = 0.0;
        eq.bands = [0.0; BAND_HZ.len()];
        self.apply_equalizer(eq);
    }

    fn apply_equalizer(&mut self, eq: EqualizerConfig) {
        self.playback.set_equalizer(eq);
        self.config.equalizer = self.playback.equalizer();
    }
}
