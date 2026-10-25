//! Pitch-preserving playback speed (WSOLA time stretching).
//!
//! rodio's own [`rodio::Source::speed`] raises the source's reported sample
//! rate, which necessarily shifts the pitch by the same factor — 1.5x playback
//! ends up a fifth higher. Musical speed control needs the opposite: same
//! pitch, different duration.
//!
//! This module implements WSOLA (Waveform Similarity Overlap Add): the stream
//! is cut into 50%-overlapping windows, and each new window's read position is
//! nudged within a small tolerance to the point whose head matches the previous
//! window's tail best. The two halves are crossfaded with complementary Hann
//! windows, so every `hop` output frames consume about `hop * factor` input
//! frames while the sample rate — and with it the pitch — stays untouched.
//!
//! Two properties make this usable as a drop-in source:
//! - the factor lives in a [`SpeedHandle`] (relaxed atomic) read at every block
//!   boundary, so speed changes apply live without restarting the decoder;
//! - at exactly 1.0x the search is skipped: the ideal hop makes the two
//!   crossfade inputs the same samples and the complementary Hann pair sums to
//!   1, so the block is an identity; and before the first window is primed the
//!   source passes the input through untouched, meaning default playback adds
//!   neither processing nor latency.
//!
//! The handle also reports the position within the *recording* that reached the
//! output, which stays exact whatever the factor is — unlike rodio's
//! `Player::get_pos()`, which only counts output (wall-clock) frames.
//!
//! The implementation is split into [`handle`] (the shared control/read-out
//! side), [`core`] (the WSOLA buffer and alignment search) and [`source`] (the
//! `Iterator`/`Source` adapters); the public names are re-exported here so
//! consumers keep using `stretch::{SpeedHandle, Stretch}`.

mod core;
mod handle;
mod source;

#[cfg(test)]
mod tests;

pub use core::Stretch;
pub use handle::{SpeedHandle, SpeedMode};
