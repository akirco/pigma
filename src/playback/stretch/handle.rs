//! Control/read-out side of the stretcher: the factor and position shared with
//! the player loop.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
    time::Duration,
};

/// Stretch factors accepted; callers clamp to narrower ranges. Visible to the
/// submodules' tests only.
pub(super) const MIN_FACTOR: f32 = 0.25;
pub(super) const MAX_FACTOR: f32 = 4.0;

/// How a speed change is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpeedMode {
    /// WSOLA time stretching: the tempo changes while the pitch stays put, and
    /// the badge gains a `*`. The reported sample rate never moves.
    #[default]
    PreservePitch,
    /// Resampling: the reported sample rate is scaled by the factor, so pitch
    /// and tempo move together. Cheaper, and what rodio's `speed()` does.
    ShiftPitch,
}

/// Control/p read-out side of a [`crate::playback::stretch::Stretch`] source,
/// shared with the player loop.
#[derive(Debug, Clone)]
pub struct SpeedHandle {
    factor: Arc<AtomicU32>,
    pos_ns: Arc<AtomicU64>,
    /// 0 = [`SpeedMode::PreservePitch`], 1 = [`SpeedMode::ShiftPitch`].
    mode: Arc<AtomicU32>,
}

impl SpeedHandle {
    pub fn new(factor: f32) -> Self {
        Self {
            factor: Arc::new(AtomicU32::new(clamp_factor(factor).to_bits())),
            pos_ns: Arc::new(AtomicU64::new(0)),
            mode: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Switch between stretching and resampling. Takes effect at the next block
    /// boundary; the buffered audio is flushed rather than dropped.
    pub fn set_mode(&self, mode: SpeedMode) {
        self.mode.store(mode as u32, Ordering::Relaxed);
    }

    pub fn mode(&self) -> SpeedMode {
        match self.mode.load(Ordering::Relaxed) {
            1 => SpeedMode::ShiftPitch,
            _ => SpeedMode::PreservePitch,
        }
    }

    /// Change the stretch factor. Non-finite values fall back to 1.0, the rest
    /// is clamped into [`MIN_FACTOR`]..=[`MAX_FACTOR`].
    pub fn set(&self, factor: f32) {
        self.factor
            .store(clamp_factor(factor).to_bits(), Ordering::Relaxed);
    }

    pub fn factor(&self) -> f32 {
        f32::from_bits(self.factor.load(Ordering::Relaxed))
    }

    /// Position within the recording that has reached the output, independent
    /// of the factor. Belongs to the most recently created `Stretch`.
    pub fn position(&self) -> Duration {
        Duration::from_nanos(self.pos_ns.load(Ordering::Relaxed))
    }

    /// Reset the reported recording position (a new source starts at zero).
    pub(super) fn reset_position(&self) {
        self.pos_ns.store(0, Ordering::Relaxed);
    }

    /// Publish the recording position of the next output frame, in nanoseconds.
    pub(super) fn set_position_nanos(&self, nanos: u64) {
        self.pos_ns.store(nanos, Ordering::Relaxed);
    }
}

fn clamp_factor(factor: f32) -> f32 {
    if !factor.is_finite() {
        1.0
    } else {
        factor.clamp(MIN_FACTOR, MAX_FACTOR)
    }
}
