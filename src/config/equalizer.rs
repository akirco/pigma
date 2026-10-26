//! Equalizer settings: ten standard bands plus a preamp.

use serde::{Deserialize, Serialize};

/// Center frequencies of the ten bands, in Hz.
pub const BAND_HZ: [f32; 10] = [
    31.0, 62.0, 125.0, 250.0, 500.0, 1_000.0, 2_000.0, 4_000.0, 8_000.0, 16_000.0,
];

/// Short labels for the UI.
pub const BAND_LABELS: [&str; 10] = [
    "31", "62", "125", "250", "500", "1k", "2k", "4k", "8k", "16k",
];

pub const MIN_GAIN_DB: f32 = -12.0;
pub const MAX_GAIN_DB: f32 = 12.0;
pub const MIN_PREAMP_DB: f32 = -24.0;
pub const MAX_PREAMP_DB: f32 = 0.0;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct EqualizerConfig {
    pub preamp_db: f32,
    /// Gain per [`BAND_HZ`], in dB.
    pub bands: [f32; 10],
}

impl Default for EqualizerConfig {
    fn default() -> Self {
        Self {
            preamp_db: 0.0,
            bands: [0.0; 10],
        }
    }
}

impl EqualizerConfig {
    /// Clamp values from disk/TOML to the supported range and drop NaNs.
    pub fn sanitized(mut self) -> Self {
        if !self.preamp_db.is_finite() {
            self.preamp_db = 0.0;
        }
        self.preamp_db = self.preamp_db.clamp(MIN_PREAMP_DB, MAX_PREAMP_DB);
        for gain in &mut self.bands {
            if !gain.is_finite() {
                *gain = 0.0;
            }
            *gain = gain.clamp(MIN_GAIN_DB, MAX_GAIN_DB);
        }
        self
    }

    pub fn is_flat(&self) -> bool {
        self.preamp_db == 0.0 && self.bands.iter().all(|&gain| gain == 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{EqualizerConfig, MAX_GAIN_DB, MIN_GAIN_DB};

    #[test]
    fn default_is_flat() {
        let eq = EqualizerConfig::default();
        assert!(eq.is_flat());
    }

    #[test]
    fn sanitize_clamps_and_drops_nan() {
        let eq = EqualizerConfig {
            preamp_db: f32::NAN,
            bands: [f32::NAN, 99.0, -99.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        }
        .sanitized();
        assert_eq!(eq.preamp_db, 0.0);
        assert_eq!(eq.bands[0], 0.0);
        assert_eq!(eq.bands[1], MAX_GAIN_DB);
        assert_eq!(eq.bands[2], MIN_GAIN_DB);
    }
}
