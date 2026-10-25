//! Ten-band peaking equalizer, wrapped as a rodio [`Source`].
//!
//! The filters are cascaded peaking EQs at the standard octave centers. They
//! run on the decoder output, before the WSOLA stretcher, so the bands stay at
//! their nominal frequencies when speed is changed with pitch preserved.
//!
//! Settings are shared through [`EqHandle`] (relaxed atomics). Changing a band
//! retunes the running filters at the next block boundary instead of restarting
//! the decoder.

use std::sync::{
    Arc,
    atomic::{AtomicI32, AtomicU32, Ordering},
};

use biquad::{Biquad, Coefficients, DirectForm1, ToHertz, Type};
use rodio::{ChannelCount, Sample, SampleRate, Source, source::SeekError};

use crate::config::{BAND_HZ, EqualizerConfig, MAX_GAIN_DB, MIN_GAIN_DB};

/// One-octave bands: Q ≈ √2.
const Q: f32 = std::f32::consts::SQRT_2;
/// Stored gains use 0.1 dB steps.
const FIXED_SCALE: f32 = 10.0;
/// Re-check the shared settings every N frames (~1.5 ms at 44.1 kHz).
const REFRESH_FRAMES: u32 = 64;

#[derive(Debug, Clone)]
pub struct EqHandle(Arc<EqShared>);

#[derive(Debug)]
struct EqShared {
    preamp: AtomicI32,
    gains: [AtomicI32; BAND_HZ.len()],
    generation: AtomicU32,
}

impl EqHandle {
    pub fn new(settings: EqualizerConfig) -> Self {
        let settings = settings.sanitized();
        Self(Arc::new(EqShared {
            preamp: AtomicI32::new(fixed(settings.preamp_db)),
            gains: std::array::from_fn(|i| AtomicI32::new(fixed(settings.bands[i]))),
            generation: AtomicU32::new(0),
        }))
    }

    /// Replace all settings. The audio thread picks them up within ~1.5 ms.
    pub fn apply(&self, settings: EqualizerConfig) {
        let settings = settings.sanitized();
        self.0
            .preamp
            .store(fixed(settings.preamp_db), Ordering::Relaxed);
        for (slot, gain) in self.0.gains.iter().zip(settings.bands) {
            slot.store(fixed(gain), Ordering::Relaxed);
        }
        self.0.generation.fetch_add(1, Ordering::Relaxed);
    }

    fn settings(&self) -> EqualizerConfig {
        EqualizerConfig {
            preamp_db: unfixed(self.0.preamp.load(Ordering::Relaxed)),
            bands: std::array::from_fn(|i| unfixed(self.0.gains[i].load(Ordering::Relaxed))),
        }
    }

    fn generation(&self) -> u32 {
        self.0.generation.load(Ordering::Relaxed)
    }
}

fn fixed(db: f32) -> i32 {
    (db * FIXED_SCALE).round() as i32
}

fn unfixed(value: i32) -> f32 {
    value as f32 / FIXED_SCALE
}

/// A [`Source`] that applies the 10-band equalizer to its input.
pub struct EqSource<I> {
    input: I,
    handle: EqHandle,
    channels: usize,
    channel: usize,
    sample_rate: u32,
    filters: Vec<DirectForm1<f32>>,
    enabled: bool,
    preamp: f32,
    gains: [f32; BAND_HZ.len()],
    generation: u32,
    refresh_in: u32,
}

impl<I: Source<Item = f32> + Send> EqSource<I> {
    pub fn new(input: I, handle: EqHandle) -> Self {
        let settings = handle.settings();
        let channels = input.channels().get() as usize;
        let sample_rate = input.sample_rate().get();
        let mut source = Self {
            input,
            handle,
            channels,
            channel: 0,
            sample_rate,
            filters: Vec::new(),
            enabled: !settings.is_flat(),
            preamp: preamp_gain(settings.preamp_db),
            gains: settings.bands,
            generation: 0,
            refresh_in: 0,
        };
        source.rebuild(settings);
        source
    }

    fn rebuild(&mut self, settings: EqualizerConfig) {
        self.enabled = !settings.is_flat();
        self.preamp = preamp_gain(settings.preamp_db);
        self.gains = settings.bands;
        self.filters = (0..self.channels)
            .flat_map(|_| {
                (0..BAND_HZ.len()).map(|band| {
                    DirectForm1::new(coefficients(self.sample_rate, band, self.gains[band]))
                })
            })
            .collect();
    }

    /// Retune the running filters. DF1 is explicitly the form recommended for
    /// online retuning (minimal artifacts, no state reset).
    fn retune(&mut self, settings: EqualizerConfig) {
        let was_enabled = self.enabled;
        self.enabled = !settings.is_flat();
        self.preamp = preamp_gain(settings.preamp_db);
        self.gains = settings.bands;
        for (i, filter) in self.filters.iter_mut().enumerate() {
            let band = i % BAND_HZ.len();
            filter.update_coefficients(coefficients(self.sample_rate, band, self.gains[band]));
        }
        // Stale state from before an off period would produce a click.
        if !was_enabled && self.enabled {
            for filter in &mut self.filters {
                filter.reset_state();
            }
        }
    }

    fn refresh(&mut self) {
        let generation = self.handle.generation();
        if generation == self.generation {
            return;
        }
        self.generation = generation;
        let settings = self.handle.settings();
        let channels = self.input.channels().get() as usize;
        let sample_rate = self.input.sample_rate().get();
        if channels != self.channels
            || sample_rate != self.sample_rate
            || self.filters.len() != channels * BAND_HZ.len()
        {
            self.channels = channels;
            self.sample_rate = sample_rate;
            self.channel = 0;
            self.rebuild(settings);
        } else {
            self.retune(settings);
        }
    }
}

impl<I: Source<Item = f32> + Send> Iterator for EqSource<I> {
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.input.next()?;
        if self.refresh_in == 0 {
            self.refresh();
            self.refresh_in = REFRESH_FRAMES;
        } else {
            self.refresh_in -= 1;
        }

        let channel = self.channel;
        self.channel = if self.channels == 0 {
            0
        } else {
            (self.channel + 1) % self.channels
        };
        if !self.enabled {
            return Some(sample);
        }

        let mut out = sample * self.preamp;
        let base = channel * BAND_HZ.len();
        for band in 0..BAND_HZ.len() {
            if let Some(filter) = self.filters.get_mut(base + band) {
                out = filter.run(out);
            }
        }
        Some(out)
    }
}

impl<I: Source<Item = f32> + Send> Source for EqSource<I> {
    fn current_span_len(&self) -> Option<usize> {
        self.input.current_span_len()
    }

    fn channels(&self) -> ChannelCount {
        self.input.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.input.sample_rate()
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        self.input.total_duration()
    }

    fn try_seek(&mut self, pos: std::time::Duration) -> Result<(), SeekError> {
        self.input.try_seek(pos)?;
        self.channel = 0;
        for filter in &mut self.filters {
            filter.reset_state();
        }
        Ok(())
    }
}

fn preamp_gain(preamp_db: f32) -> f32 {
    10f32.powf(preamp_db / 20.0)
}

fn coefficients(sample_rate: u32, band: usize, gain_db: f32) -> Coefficients<f32> {
    if gain_db == 0.0 {
        return identity();
    }
    let f0 = BAND_HZ[band];
    // Leave headroom below Nyquist; the 16 kHz band must degrade to passthrough
    // on 32 kHz sources instead of failing.
    if sample_rate == 0 || f0 >= sample_rate as f32 * 0.45 {
        return identity();
    }
    Coefficients::from_params(
        Type::PeakingEQ(gain_db.clamp(MIN_GAIN_DB, MAX_GAIN_DB)),
        (sample_rate as f32).hz(),
        f0.hz(),
        Q,
    )
    .unwrap_or_else(|_| identity())
}

fn identity() -> Coefficients<f32> {
    Coefficients {
        a1: 0.0,
        a2: 0.0,
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZero;

    use rodio::buffer::SamplesBuffer;

    use super::*;

    const SR: u32 = 44_100;

    fn buffer(data: Vec<f32>) -> SamplesBuffer {
        SamplesBuffer::new(NonZero::new(1).unwrap(), NonZero::new(SR).unwrap(), data)
    }

    fn sine(freq: f64, seconds: f64) -> Vec<f32> {
        let n = (seconds * SR as f64) as usize;
        (0..n)
            .map(|i| (std::f64::consts::TAU * freq * i as f64 / SR as f64).sin() as f32)
            .collect()
    }

    fn rms(samples: &[f32]) -> f64 {
        let sum: f64 = samples.iter().map(|s| (*s as f64) * (*s as f64)).sum();
        (sum / samples.len() as f64).sqrt()
    }

    fn run(settings: EqualizerConfig, samples: Vec<f32>) -> Vec<f32> {
        let handle = EqHandle::new(settings);
        EqSource::new(buffer(samples), handle).collect()
    }

    #[test]
    fn flat_config_is_a_passthrough() {
        let data = sine(440.0, 0.1);
        assert_eq!(run(EqualizerConfig::default(), data.clone()), data);
    }

    #[test]
    fn peaking_band_boosts_its_center_frequency() {
        let data = sine(1_000.0, 0.5);
        let mut bands = [0.0; 10];
        bands[5] = 6.0;
        let out = run(
            EqualizerConfig {
                bands,
                ..EqualizerConfig::default()
            },
            data.clone(),
        );
        // Skip the filter settling region.
        let head = (SR as usize) / 10;
        let ratio = rms(&out[head..]) / rms(&data[head..]);
        assert!(
            (ratio - 1.995).abs() < 0.15,
            "+6 dB at 1 kHz produced ratio {ratio:.3}"
        );
    }
}
