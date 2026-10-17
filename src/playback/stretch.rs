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

use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
    time::Duration,
};

use rodio::{ChannelCount, Sample, SampleRate, Source, source::SeekError};

/// Analysis window length as a fraction of the sample rate (≈46 ms). Long
/// enough to hold several cycles of low fundamentals, short enough that the
/// crossfade hides on transients.
const WINDOW_SECONDS: f64 = 0.046;
/// Search tolerance as a fraction of the ideal read hop: the alignment may
/// jitter by this much around the ideal, and no further.
const TOLERANCE_FACTOR: f64 = 0.2;
/// Frames compared when scoring a candidate alignment.
const CORR_FRAMES: usize = 512;
/// Coarse search stride in frames; the winner is refined at unit stride.
const SEARCH_STRIDE: usize = 16;
/// Cost of a full-tolerance deviation from the ideal hop, in correlation units.
/// Must be large enough that a tonal signal (where many offsets correlate
/// equally well) cannot pull the average rate away from the requested factor.
const ALIGNMENT_PENALTY: f64 = 2.0;
/// Stretch factors accepted; callers clamp to narrower ranges.
pub const MIN_FACTOR: f32 = 0.25;
pub const MAX_FACTOR: f32 = 4.0;
const NANOS_PER_SEC: u64 = 1_000_000_000;

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

/// Control/p read-out side of a [`Stretch`] source, shared with the player loop.
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
    /// of the factor. Belongs to the most recently created [`Stretch`].
    pub fn position(&self) -> Duration {
        Duration::from_nanos(self.pos_ns.load(Ordering::Relaxed))
    }
}

fn clamp_factor(factor: f32) -> f32 {
    if !factor.is_finite() {
        1.0
    } else {
        factor.clamp(MIN_FACTOR, MAX_FACTOR)
    }
}

/// A run of queued output samples and where in the recording they came from, so
/// the reported position follows the (deliberately discontinuous) WSOLA read
/// jumps instead of drifting.
#[derive(Debug, Clone, Copy)]
struct Block {
    /// Recording frame of the block's first sample.
    at: u64,
    /// Samples in the block.
    len: usize,
    /// Samples already handed out.
    done: usize,
}

/// Wraps a [`Source`], time-scaling it while preserving pitch.
#[derive(Debug)]
pub struct Stretch<I> {
    input: I,
    handle: SpeedHandle,
    /// Frames per analysis window (even, so `hop == win / 2`).
    win: usize,
    /// Synthesis hop in frames: frames produced per block.
    hop: usize,
    /// Frames scored per candidate alignment.
    corr: usize,
    /// Hann window of length `win`; `window[i] + window[i + hop] == 1`.
    window: Vec<f32>,
    nanos_per_frame: u64,
    /// Lookahead buffer (interleaved) starting at recording frame `buf_start`.
    buf: Vec<f32>,
    buf_start: u64,
    /// Previous analysis window and the recording frame it starts at.
    prev: Vec<f32>,
    prev_start: u64,
    /// Tempo accounting since the current anchor: the frames the read head
    /// *should* have advanced (`ideal`) versus the frames it actually did
    /// (`actual`), plus the factor they were accumulated under. Used to
    /// compensate the drift WSOLA introduces when it snaps to periodic
    /// material; re-anchored whenever the factor changes.
    ideal: f64,
    actual: u64,
    anchor_factor: f32,
    /// Output samples awaiting `next()`, all belonging to `block`.
    out: VecDeque<f32>,
    block: Option<Block>,
    /// A window has been primed, i.e. the stretcher is actually running.
    primed: bool,
    input_done: bool,
    finished: bool,
}

impl<I: Source + Send> Stretch<I> {
    /// Wrap `input`. `handle` must be the same one the player loop controls —
    /// creating a source resets the reported position (it is per source).
    pub fn new(input: I, handle: SpeedHandle) -> Self {
        handle.pos_ns.store(0, Ordering::Relaxed);
        let rate = input.sample_rate().get() as f64;
        // An even frame count keeps the 50% overlap — and with it the
        // complementary Hann pair — exact.
        let win = ((rate * WINDOW_SECONDS) as usize / 2 * 2).max(64);
        let hop = win / 2;
        let window = (0..win)
            .map(|i| (0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / win as f64).cos()) as f32)
            .collect();
        Self {
            win,
            hop,
            corr: hop.min(CORR_FRAMES),
            window,
            nanos_per_frame: (NANOS_PER_SEC as f64 / rate).round().max(1.0) as u64,
            buf: Vec::with_capacity(win * 4),
            buf_start: 0,
            prev: Vec::with_capacity(win * 2),
            prev_start: 0,
            out: VecDeque::with_capacity(win * 2),
            block: None,
            primed: false,
            ideal: 0.0,
            actual: 0,
            anchor_factor: handle.factor(),
            input_done: false,
            finished: false,
            input,
            handle,
        }
    }

    #[inline]
    fn ch(&self) -> usize {
        self.input.channels().get() as usize
    }

    /// Recording frame just past the end of what has been pulled from the input.
    #[inline]
    fn buf_end(&self) -> u64 {
        self.buf_start + (self.buf.len() / self.ch()) as u64
    }

    /// Pull input until the buffer reaches recording frame `target`, or the
    /// input ends (the buffer is then kept frame-aligned).
    fn fill_to(&mut self, target: u64) {
        let ch = self.ch();
        while !self.input_done && self.buf_end() < target {
            for _ in 0..ch {
                match self.input.next() {
                    Some(sample) => self.buf.push(sample),
                    None => {
                        self.input_done = true;
                        break;
                    }
                }
            }
        }
        if self.input_done {
            self.buf.truncate(self.buf.len() / ch * ch);
        }
    }

    /// Queue `frames` buffered frames for output untouched.
    fn push_raw(&mut self, at: u64, frames: usize) {
        let ch = self.ch();
        if frames == 0 || at < self.buf_start {
            return;
        }
        let from = (at - self.buf_start) as usize * ch;
        let to = (from + frames * ch).min(self.buf.len());
        if from >= to {
            return;
        }
        debug_assert!(self.out.is_empty());
        self.out.extend(self.buf[from..to].iter().copied());
        self.block = Some(Block {
            at,
            len: to - from,
            done: 0,
        });
    }

    /// Similarity between the candidate window starting at `start` and the tail
    /// of the previous window, biased against deviating from the ideal hop.
    ///
    /// The raw normalized cross-correlation alone is useless on tonal material:
    /// every offset that is a multiple of the fundamental's period scores the
    /// same maximum, so the search would wander and the *tempo* would drift as
    /// a result. Subtracting a quadratic deviation term keeps the average rate
    /// exactly `ideal` frames per block while still allowing the local jitter
    /// that makes the crossfade seamless.
    fn score(&self, start: u64, ideal: u64, tol: u64) -> f32 {
        let ch = self.ch();
        let avail = (self.prev.len() / ch).saturating_sub(self.hop);
        let len = self.corr.min(self.hop).min(avail);
        if len == 0 {
            return f32::MIN;
        }
        let offset = match start.checked_sub(self.buf_start) {
            Some(d) => d as usize * ch,
            None => return f32::MIN,
        };
        let n = len * ch;
        let prev_tail = match self.prev.get(self.hop * ch..self.hop * ch + n) {
            Some(v) => v,
            None => return f32::MIN,
        };
        let cand = match self.buf.get(offset..offset + n) {
            Some(v) => v,
            None => return f32::MIN,
        };
        let (mut pc, mut pp, mut cc) = (0.0f64, 0.0f64, 0.0f64);
        for (a, b) in prev_tail.iter().zip(cand) {
            let (a, b) = (*a as f64, *b as f64);
            pc += a * b;
            pp += a * a;
            cc += b * b;
        }
        let denom = (pp * cc).sqrt();
        // Silence carries no alignment information: score it neutrally instead
        // of dividing by zero.
        let ncc = if denom < 1e-12 { 0.0 } else { pc / denom };
        let dev = (start - self.prev_start) as f64 - ideal as f64;
        let dev = dev / tol.max(1) as f64;
        (ncc - ALIGNMENT_PENALTY * dev * dev) as f32
    }

    /// WSOLA alignment search: coarse sweep with [`SEARCH_STRIDE`]-frame steps,
    /// then unit-stride refinement around the coarse winner.
    fn find_start(&mut self, lo: u64, hi: u64, ideal: u64, tol: u64) -> u64 {
        if lo >= hi {
            return lo;
        }
        let stride = (hi - lo).div_ceil(SEARCH_STRIDE as u64).max(1);
        let mut best = lo;
        let mut best_score = f32::MIN;
        let mut at = lo;
        loop {
            let s = self.score(at, ideal, tol);
            if s > best_score {
                best_score = s;
                best = at;
            }
            if at >= hi {
                break;
            }
            at = (at + stride).min(hi);
        }
        let (rlo, rhi) = (best.saturating_sub(stride).max(lo), (best + stride).min(hi));
        let mut at = rlo;
        let mut best_score = f32::MIN;
        while at <= rhi {
            let s = self.score(at, ideal, tol);
            if s > best_score {
                best_score = s;
                best = at;
            }
            at += 1;
        }
        best
    }

    /// Crossfade the tail of the previous window with the candidate window
    /// starting at `start`, then slide the window onto the candidate.
    fn emit_block(&mut self, start: u64) {
        let ch = self.ch();
        let tail = self.win - self.hop;
        debug_assert!(self.out.is_empty());
        self.out.reserve(tail * ch);
        for i in 0..tail {
            // Complementary Hann pair sums to 1: when `start` is exactly
            // `prev_start + hop` (factor 1.0) both inputs are the same samples
            // and the output is those samples unchanged.
            let w_new = self.window[i];
            let w_prev = self.window[i + self.hop];
            let cand_at = start + i as u64;
            for c in 0..ch {
                // The final window can be shorter than `win` frames when the
                // stream ends; treat the missing samples as silence.
                let pi = (self.hop + i) * ch + c;
                let p = self.prev.get(pi).copied().unwrap_or(0.0);
                // Past the buffered end (stream tail) the candidate reads as
                // silence, which the window fades in anyway.
                let n = self.sample_at(cand_at, c, ch);
                self.out.push_back(p * w_prev + n * w_new);
            }
        }
        self.block = Some(Block {
            at: self.prev_start + self.hop as u64,
            len: tail * ch,
            done: 0,
        });

        // Slide: prev := candidate window, discard everything before it.
        let skip = (start - self.buf_start) as usize * ch;
        self.buf.drain(..skip.min(self.buf.len()));
        self.buf_start = start;
        self.prev.clear();
        self.prev
            .extend_from_slice(&self.buf[..(self.win * ch).min(self.buf.len())]);
        self.prev_start = start;
    }

    #[inline]
    fn sample_at(&self, frame: u64, channel: usize, ch: usize) -> f32 {
        if frame < self.buf_start {
            return 0.0;
        }
        let idx = (frame - self.buf_start) as usize * ch + channel;
        self.buf.get(idx).copied().unwrap_or(0.0)
    }

    /// Play out whatever is still unheard (the second half of the last window
    /// plus anything buffered behind it) and mark the source finished.
    fn finish(&mut self) -> bool {
        let at = self.prev_start + self.hop as u64;
        let end = self.buf_end();
        if end > at {
            self.push_raw(at, (end - at) as usize);
        }
        self.buf.clear();
        self.prev.clear();
        self.primed = false;
        self.finished = true;
        !self.out.is_empty()
    }

    /// Queue the next block of output. `false` means nothing could be queued.
    fn step(&mut self) -> bool {
        if self.finished {
            return false;
        }
        let ch = self.ch();
        let factor = self.handle.factor();

        if factor != self.anchor_factor {
            // The factor moved: re-anchor the accounting. Measuring drift
            // against the whole pre-change history would make a 2.0x -> 0.5x
            // switch try to "pay back" frames it had legitimately consumed and
            // stall the read head instead of slowing down.
            self.anchor_factor = factor;
            self.ideal = 0.0;
            self.actual = 0;
        }

        if self.primed && self.handle.mode() == SpeedMode::ShiftPitch {
            // Switched to resampling mid-song: hand over everything still
            // unheard in the pipeline, then resume as a passthrough.
            let end = self.buf_end();
            let at = self.prev_start + self.hop as u64;
            if end > at {
                self.push_raw(at, (end - at) as usize);
            }
            let tail_end = self.buf_end();
            self.buf
                .drain(..(tail_end - self.buf_start) as usize * self.ch());
            self.buf_start = tail_end;
            self.prev.clear();
            self.primed = false;
            return !self.out.is_empty();
        }

        if !self.primed {
            // Normal speed, or resampling mode (which needs no processing at
            // all — the rate metadata does the work): forward whole frames
            // straight from the input, no buffering and no latency.
            if factor == 1.0 || self.handle.mode() == SpeedMode::ShiftPitch {
                let at = self.buf_end();
                for _ in 0..ch {
                    match self.input.next() {
                        Some(sample) => self.out.push_back(sample),
                        None => {
                            self.input_done = true;
                            self.out.clear();
                            break;
                        }
                    }
                }
                if self.out.is_empty() {
                    self.finished = true;
                    return false;
                }
                self.buf_start = at + 1;
                self.block = Some(Block {
                    at,
                    len: ch,
                    done: 0,
                });
                return true;
            }
            // Stretched: prime the first window. Its leading half has nothing
            // to overlap with, so it is played untouched.
            self.fill_to(self.buf_end() + self.win as u64);
            if self.buf.len() < self.win * ch {
                // Less than one window remains (a very short clip, or the tail
                // of the stream): play every buffered frame out as-is and stop.
                let end = self.buf_end();
                self.prev_start = self.buf_start.saturating_sub(self.hop as u64);
                self.prev = Vec::new();
                if end > self.buf_start {
                    let frames = (end - self.buf_start) as usize;
                    self.out.extend(self.buf.drain(..));
                    self.block = Some(Block {
                        at: self.buf_start,
                        len: frames * ch,
                        done: 0,
                    });
                    self.buf_start = end;
                }
                self.finished = true;
                return !self.out.is_empty();
            }
            self.prev.clear();
            self.prev.extend_from_slice(&self.buf[..self.win * ch]);
            self.prev_start = self.buf_start;
            self.anchor_factor = factor;
            self.ideal = 0.0;
            self.actual = 0;
            self.push_raw(self.buf_start, self.hop);
            self.primed = true;
            return true;
        }

        // `hop` output frames must consume `hop * factor` input frames. At 1.0x
        // that is the identity alignment (`hop`), reached without any search and
        // with no room to deviate from it.
        let ideal_f = self.hop as f64 * factor as f64;
        let (ideal, tol) = if factor == 1.0 {
            (self.hop as u64, 0u64)
        } else {
            (
                ideal_f.round().max(1.0) as u64,
                ((ideal_f * TOLERANCE_FACTOR) as u64).max(2),
            )
        };

        // Drift compensation. WSOLA happily snaps its read position to the
        // period of tonal material (any multiple of a period correlates equally
        // well), which is invisible locally but shifts the average tempo.
        // Compare what the read head has advanced against what it should have,
        // and ask the next block to make up the difference — bounded by the
        // search tolerance, so a correction can never stall the stream.
        let owed = (self.ideal + ideal_f) - self.actual as f64;
        let target = owed
            .round()
            .clamp(ideal.saturating_sub(tol) as f64, (ideal + tol) as f64)
            .max(1.0) as u64;

        self.fill_to(self.prev_start + ideal + tol + self.win as u64);

        // A candidate window must be fully buffered to be usable. Note that an
        // exhausted input with nothing unheard left lands on `hi < lo` too, so
        // there is only one way out of the stream.
        let latest = self.buf_end().saturating_sub(self.win as u64);
        let lo = (self.prev_start + target.saturating_sub(tol)).max(self.prev_start + 1);
        let hi = (self.prev_start + target + tol).min(latest);
        if hi < lo {
            // End of stream: flush what is still unheard and finish. Carrying
            // on with zero-filled windows would keep the queue alive and the
            // track would never fire `Finished`.
            return self.finish();
        }
        let start = if tol == 0 {
            self.prev_start + ideal
        } else {
            self.find_start(lo, hi, target, tol)
        };
        self.ideal += ideal_f;
        self.actual += start - self.prev_start;
        self.emit_block(start);
        true
    }
}

impl<I: Source + Send> Iterator for Stretch<I> {
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let ch = self.ch();
        loop {
            if let Some(block) = &mut self.block {
                if block.done < block.len {
                    // Publish on frame boundaries only: the position is a frame
                    // index, so the other channels of a frame would store it
                    // redundantly.
                    if block.done % ch == 0 {
                        let frame = block.at + (block.done / ch) as u64;
                        self.handle.pos_ns.store(
                            frame.saturating_mul(self.nanos_per_frame),
                            Ordering::Relaxed,
                        );
                    }
                    let sample = self.out.pop_front()?;
                    block.done += 1;
                    if block.done == block.len {
                        self.block = None;
                    }
                    return Some(sample);
                }
                self.block = None;
            }
            debug_assert!(
                self.out.is_empty(),
                "samples queued without a block to locate them"
            );
            if self.step() {
                continue;
            }
            return self.out.pop_front();
        }
    }
}

impl<I: Source + Send> Source for Stretch<I> {
    /// One block per span: the mixer re-reads `sample_rate()` at every span
    /// boundary, which is how a mode/factor change reaches the resampler in
    /// [`SpeedMode::ShiftPitch`].
    fn current_span_len(&self) -> Option<usize> {
        if self.finished && self.out.is_empty() {
            Some(0)
        } else {
            Some(self.hop * self.ch())
        }
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.input.channels()
    }

    /// In [`SpeedMode::ShiftPitch`] the factor is expressed here: rodio's mixer
    /// resamples to the device rate, so a raised rate plays the same samples
    /// faster — and higher — which is exactly the behaviour the mode promises.
    /// In [`SpeedMode::PreservePitch`] the rate stays untouched.
    fn sample_rate(&self) -> SampleRate {
        let rate = self.input.sample_rate().get();
        if self.handle.mode() == SpeedMode::ShiftPitch {
            let scaled = rate as f64 * self.handle.factor() as f64;
            if let Some(r) = SampleRate::new(scaled.round().max(1.0) as u32) {
                return r;
            }
        }
        self.input.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.input
            .total_duration()
            .map(|d| d.mul_f64(1.0 / self.handle.factor() as f64))
    }

    /// `pos` addresses the recording. Under [`SpeedMode::PreservePitch`] the
    /// stretcher consumes `factor` input frames per output frame, so the seek
    /// target has to be scaled; under [`SpeedMode::ShiftPitch`] frames are
    /// handed over 1:1 and the factor is carried by the sample rate instead.
    /// The stretch pipeline is dropped either way.
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        let scaled = match self.handle.mode() {
            SpeedMode::PreservePitch => pos.mul_f64(self.handle.factor() as f64),
            SpeedMode::ShiftPitch => pos,
        };
        self.input.try_seek(scaled)?;
        self.out.clear();
        self.block = None;
        self.buf.clear();
        self.prev.clear();
        self.buf_start = 0;
        self.prev_start = 0;
        self.primed = false;
        self.input_done = false;
        self.finished = false;
        self.handle.pos_ns.store(0, Ordering::Relaxed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZero;

    use rodio::buffer::SamplesBuffer;

    use super::*;

    const SR: u32 = 8_000;

    fn buffer(data: Vec<f32>, channels: u16) -> SamplesBuffer {
        SamplesBuffer::new(
            NonZero::new(channels).unwrap(),
            NonZero::new(SR).unwrap(),
            data,
        )
    }

    /// `seconds` of a `freq` Hz sine, mono.
    fn sine(seconds: f64, freq: f64) -> SamplesBuffer {
        let n = (seconds * SR as f64) as usize;
        buffer(
            (0..n)
                .map(|i| (std::f64::consts::TAU * freq * i as f64 / SR as f64).sin() as f32)
                .collect(),
            1,
        )
    }

    fn run(src: SamplesBuffer, factor: f32) -> (Vec<f32>, SpeedHandle) {
        let handle = SpeedHandle::new(factor);
        let out: Vec<f32> = Stretch::new(src, handle.clone()).collect();
        (out, handle)
    }

    fn rms(samples: &[f32]) -> f64 {
        let sum: f64 = samples.iter().map(|v| (*v as f64) * (*v as f64)).sum();
        (sum / samples.len() as f64).sqrt()
    }

    /// Hann-windowed Goertzel energy at `freq`, averaged over the signal.
    fn energy_at(samples: &[f32], freq: f64) -> f64 {
        let n = 2_048usize;
        if samples.len() < n {
            return 0.0;
        }
        let coeff = 2.0 * (2.0 * std::f64::consts::TAU * freq / SR as f64).cos();
        let (mut total, mut blocks) = (0.0f64, 0usize);
        for b in 0..samples.len() / n {
            let (mut s0, mut s1) = (0.0f64, 0.0f64);
            for (i, v) in samples[b * n..(b + 1) * n].iter().enumerate() {
                let win = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / (n - 1) as f64).cos();
                s0 = f64::from(*v) * win + coeff * s1 - s0;
                std::mem::swap(&mut s0, &mut s1);
            }
            total += s0 * s0 + s1 * s1 - coeff * s0 * s1;
            blocks += 1;
        }
        total / blocks as f64
    }

    /// Energy-weighted mean frequency over log-spaced bands. Resampling (what
    /// rodio's `speed()` does) scales it by the factor; a pitch-preserving
    /// stretcher must leave it where it was. Measured on noise, because a pure
    /// sine gives WSOLA no alignment information at all.
    fn centroid(samples: &[f32]) -> f64 {
        let (mut num, mut den) = (0.0f64, 0.0f64);
        let mut f = 80.0f64;
        while f < 3_500.0 {
            let e = energy_at(samples, f);
            num += f * e;
            den += e;
            f *= 1.08;
        }
        num / den.max(1e-18)
    }

    /// Deterministic white noise (LCG), so the tests are reproducible.
    fn noise(seconds: f64) -> SamplesBuffer {
        let n = (seconds * SR as f64) as usize;
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut data = Vec::with_capacity(n);
        for _ in 0..n {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            data.push(((state as f64 / u64::MAX as f64) * 2.0 - 1.0) as f32 * 0.4);
        }
        buffer(data, 1)
    }

    #[test]
    fn one_x_is_bit_exact_passthrough() {
        let data: Vec<f32> = (0..5_000).map(|i| (i as f32 * 0.01).sin()).collect();
        let (out, _) = run(buffer(data.clone(), 1), 1.0);
        assert_eq!(out, data, "1.0x must not alter the samples");
    }

    #[test]
    fn stretching_never_touches_the_sample_rate() {
        for factor in [0.5f32, 1.0, 1.7, 3.0] {
            let s = Stretch::new(sine(1.0, 220.0), SpeedHandle::new(factor));
            assert_eq!(
                s.sample_rate().get(),
                SR,
                "factor {factor} changed the rate"
            );
            assert_eq!(s.channels().get(), 1);
        }
    }

    #[test]
    fn rodio_speed_is_resampling_while_stretch_is_not() {
        // The reason this module exists: rodio implements `speed()` by raising
        // the reported sample rate, so the device plays the same samples faster
        // and the pitch moves with the tempo.
        let sped = sine(1.0, 220.0).speed(2.0);
        assert_eq!(
            sped.sample_rate().get(),
            SR * 2,
            "rodio no longer resamples with speed() — reconsider Stretch"
        );
        let stretched = Stretch::new(sine(1.0, 220.0), SpeedHandle::new(2.0));
        assert_eq!(stretched.sample_rate().get(), SR);
    }

    #[test]
    fn speed_up_keeps_pitch_and_shortens() {
        let (out, _) = run(sine(2.0, 220.0), 2.0);
        let secs = out.len() as f64 / SR as f64;
        assert!(
            (secs - 1.0).abs() < 0.1,
            "expected ~1.0s of output, got {secs:.3}s"
        );
        let (nout, _) = run(noise(2.0), 2.0);
        let ratio = centroid(&nout) / centroid(&noise(2.0).collect::<Vec<f32>>());
        assert!(
            (0.9..=1.1).contains(&ratio),
            "speed-up shifted the spectrum: centroid ratio {ratio:.3}"
        );
    }

    #[test]
    fn slow_down_keeps_pitch_and_lengthens() {
        let (out, _) = run(sine(1.0, 220.0), 0.5);
        let secs = out.len() as f64 / SR as f64;
        assert!(
            (secs - 2.0).abs() < 0.15,
            "expected ~2.0s of output, got {secs:.3}s"
        );
        let (nout, _) = run(noise(2.0), 0.5);
        let ratio = centroid(&nout) / centroid(&noise(2.0).collect::<Vec<f32>>());
        assert!(
            (0.9..=1.1).contains(&ratio),
            "slow-down shifted the spectrum: centroid ratio {ratio:.3}"
        );
    }

    #[test]
    fn centroid_metric_tracks_pitch() {
        // Control for the metric used above: an octave must read as an octave,
        // so a ratio near 1.0 for the stretched noise really does mean the
        // pitch content stayed where it was.
        let low: Vec<f32> = sine(2.0, 220.0).collect();
        let high: Vec<f32> = sine(2.0, 440.0).collect();
        let ratio = centroid(&high) / centroid(&low);
        assert!(
            (ratio - 2.0).abs() < 0.15,
            "centroid metric does not track pitch: {ratio:.2}"
        );
    }

    #[test]
    fn tempo_accuracy_holds_on_periodic_material() {
        // A repetitive signal is WSOLA's worst case: many offsets correlate
        // equally well, so an uncompensated search drifts off the requested
        // factor. 8s of material keeps the end-of-stream flush negligible.
        let period = 80usize;
        let n = SR as usize * 8;
        let data: Vec<f32> = (0..n)
            .map(|i| (((i % period) as f64 / period as f64) * std::f64::consts::TAU).sin() as f32)
            .collect();
        for factor in [0.5f32, 0.6, 1.4, 2.0, 3.0] {
            let (out, _) = run(buffer(data.clone(), 1), factor);
            let secs = out.len() as f64 / SR as f64;
            let want = 8.0 / factor as f64;
            let err = ((secs - want).abs() / want) as f32;
            assert!(
                err < 0.03,
                "factor {factor}: got {secs:.2}s, want {want:.2}s"
            );
        }
    }

    #[test]
    fn energy_is_preserved_and_no_silence_is_inserted() {
        let src: Vec<f32> = sine(2.0, 220.0).collect();
        let want = rms(&src[SR as usize / 2..SR as usize]);
        for factor in [0.5f32, 0.8, 1.2, 1.5, 2.0] {
            let (out, _) = run(sine(2.0, 220.0), factor);
            let body = &out[SR as usize / 2..out.len() - SR as usize / 2];
            let ratio = rms(body) / want;
            assert!(
                (0.85..=1.15).contains(&ratio),
                "factor {factor}: rms ratio {ratio:.2} (bad crossfade or dropped audio)"
            );
            let quiet = body
                .chunks(SR as usize / 100)
                .filter(|w| rms(w) < want * 0.2)
                .count();
            assert!(
                quiet == 0,
                "factor {factor}: {quiet} silent 10ms chunks inside the body"
            );
        }
    }

    #[test]
    fn reported_position_tracks_the_recording() {
        // Two seconds of audio must report ~2s of *recording* position for
        // every factor, even though the output length changes with it.
        for factor in [0.5f32, 1.0, 1.5, 2.0, 3.0] {
            let (out, handle) = run(sine(2.0, 220.0), factor);
            let got = handle.position().as_secs_f64();
            assert!(
                (got - 2.0).abs() < 0.06,
                "factor {factor}: reported {got:.2}s of a 2s recording ({} output samples)",
                out.len()
            );
            let mut best = (0.0f64, 0.0f64);
            let mut f = 50.0;
            while f < 2000.0 {
                let e = energy_at(&out, f);
                if e > best.0 {
                    best = (e, f);
                }
                f += 5.0;
            }
            println!("   peak at {:.0}Hz energy {:.1}", best.1, best.0);
        }
    }

    #[test]
    fn position_advances_monotonically() {
        let handle = SpeedHandle::new(1.7);
        let mut s = Stretch::new(sine(3.0, 220.0), handle.clone());
        let mut last = Duration::ZERO;
        for _ in &mut s {
            let pos = handle.position();
            assert!(pos >= last, "position rewound: {last:?} -> {pos:?}");
            last = pos;
        }
    }

    #[test]
    fn stereo_stays_frame_aligned() {
        let n = 300 * 2;
        let data: Vec<f32> = (0..n).map(|i| ((i / 2) as f32 * 0.05).sin()).collect();
        let (out, _) = run(buffer(data.clone(), 2), 1.0);
        assert_eq!(out, data, "short stereo run at 1.0x changed samples");

        let long: Vec<f32> = (0..4_000 * 2)
            .map(|i| ((i / 2) as f32 * 0.05).sin() * 0.5)
            .collect();
        for factor in [0.5f32, 1.5, 2.0] {
            let (out, _) = run(buffer(long.clone(), 2), factor);
            assert_eq!(out.len() % 2, 0, "stereo frame torn at {factor}x");
            assert!(out.iter().all(|v| v.is_finite()), "NaN at {factor}x");
        }
    }

    #[test]
    fn factor_change_mid_stream_keeps_pitch_and_audio_flowing() {
        let handle = SpeedHandle::new(1.0);
        let mut s = Stretch::new(sine(6.0, 220.0), handle.clone());
        // One second at normal speed (passthrough), then 1.8x.
        let mut out: Vec<f32> = (&mut s).take(SR as usize).collect();
        handle.set(1.8);
        out.extend((&mut s).take(SR as usize * 3));
        assert!(out.iter().all(|v| v.is_finite()), "NaN/inf in output");
        assert!(
            out.len() > SR as usize * 3,
            "stream stalled after the speed change"
        );
        // The stretcher's own position report must have followed the new rate.
        let pos = handle.position().as_secs_f64();
        assert!(
            pos > 2.4,
            "position lagged after the speed change: {pos:.2}s"
        );
    }

    #[test]
    fn returning_to_one_x_after_stretching_hands_back_the_rest() {
        let handle = SpeedHandle::new(2.0);
        let src: Vec<f32> = noise(10.0).collect();
        let mut s = Stretch::new(buffer(src.clone(), 1), handle.clone());
        let mut out: Vec<f32> = (&mut s).take(SR as usize).collect();
        handle.set(1.0);
        out.extend((&mut s).take(SR as usize * 10));
        assert!(out.iter().all(|v| v.is_finite()));
        assert!(
            out.len() > SR as usize * 4,
            "stream stopped early when returning to 1.0x"
        );
        // The tail must still be real material, not the synthesized silence a
        // stuck pipeline would produce. Spectral balance is covered by the
        // dedicated pitch tests; over a short noise window it is too noisy.
        let tail = &out[out.len() - SR as usize / 2..];
        assert!(
            rms(tail) > 0.15,
            "ended in silence: tail rms {:.3}",
            rms(tail)
        );
        assert!(
            handle.position().as_secs_f64() > 4.0,
            "position lagged behind the recording: {:?}",
            handle.position()
        );
    }

    #[test]
    fn ending_the_recording_finishes_instead_of_inventing_silence() {
        // The stretcher consumes ahead of the output, so a factor > 1 can reach
        // the end of the stream while `prev` still holds a half window. It must
        // then terminate instead of crossfading into synthesized silence, which
        // would hang the queue and never fire `Finished`.
        let handle = SpeedHandle::new(2.0);
        let mut s = Stretch::new(sine(2.0, 220.0), handle.clone());
        let mut out: Vec<f32> = (&mut s).take(SR as usize).collect();
        handle.set(1.0);
        out.extend(&mut s);
        assert!(
            out.len() < SR as usize * 3 / 2,
            "stream never ended: {} samples",
            out.len()
        );
        assert!(
            out.iter().all(|v| v.is_finite()),
            "non-finite while finishing"
        );
        let tail = &out[out.len() - SR as usize / 4..];
        assert!(
            rms(tail) > 0.4,
            "faded into silence: tail rms {:.3}",
            rms(tail)
        );
    }

    #[test]
    fn clips_shorter_than_one_window_are_played_out_complete() {
        // Shorter than `win` frames and never primed: every input frame must
        // still reach the output, at any factor.
        for factor in [1.0f32, 1.5, 2.5] {
            let data: Vec<f32> = (0..300).map(|i| (i as f32 * 0.02).sin()).collect();
            let (out, _) = run(buffer(data.clone(), 1), factor);
            assert_eq!(out, data, "factor {factor}: frames dropped on a tiny clip");
        }
    }

    #[test]
    fn short_tail_of_a_longer_stream_is_not_dropped() {
        // Enough material to prime, then a tail shorter than a hop at 3.0x:
        // the flush must keep the unheard frames instead of running dry.
        let src: Vec<f32> = (0..(SR as usize + 7))
            .map(|i| (i as f32 * 0.03).sin())
            .collect();
        let (out, handle) = run(buffer(src.clone(), 1), 3.0);
        assert!(!out.is_empty(), "nothing came out");
        assert!(
            handle.position().as_secs_f64() > 0.9,
            "position stalled at {:?}",
            handle.position()
        );
    }

    #[test]
    fn position_rate_tracks_the_factor_across_live_changes() {
        // Position advance per output second must equal the factor — that is
        // the invariant the progress bar and lyric sync rely on, and it has to
        // hold right after a speed change too, not just at a steady factor.
        let handle = SpeedHandle::new(1.0);
        let mut s = Stretch::new(sine(30.0, 220.0), handle.clone());
        let chunk = (SR / 2) as usize; // 0.5s of output per phase
        for factor in [1.0f32, 2.0, 0.5, 1.6, 1.0, 3.0] {
            handle.set(factor);
            let before = handle.position();
            let got: usize = (&mut s).take(chunk).count();
            assert_eq!(got, chunk, "output ran dry at {factor}x");
            let advanced = (handle.position() - before).as_secs_f64();
            let want = 0.5 * factor as f64;
            assert!(
                (advanced - want).abs() / want < 0.3,
                "factor {factor}x: position advanced {advanced:.2}s in 0.5s of output, want {want:.2}s"
            );
        }
    }

    #[test]
    fn total_duration_shrinks_with_the_factor_in_both_modes() {
        for mode in [SpeedMode::PreservePitch, SpeedMode::ShiftPitch] {
            let h = SpeedHandle::new(2.0);
            h.set_mode(mode);
            let s = Stretch::new(sine(2.0, 220.0), h);
            let secs = s.total_duration().unwrap().as_secs_f64();
            assert!(
                (secs - 1.0).abs() < 0.01,
                "{mode:?}: 2s of audio at 2.0x should report {secs:.2}s"
            );
        }
    }

    #[test]
    fn shift_mode_resamples_and_moves_the_pitch() {
        // 不修复音调: the source's reported sample rate carries the factor, so
        // the samples play faster and the pitch rises with them.
        let h = SpeedHandle::new(2.0);
        h.set_mode(SpeedMode::ShiftPitch);
        let s = Stretch::new(sine(2.0, 220.0), h.clone());
        assert_eq!(s.sample_rate().get(), SR * 2, "rate must scale with factor");
        let out: Vec<f32> = s.collect();
        // The samples are handed over untouched; only the rate (and thus the
        // pitch as heard) moves.
        let src: Vec<f32> = sine(2.0, 220.0).collect();
        assert_eq!(out.len(), src.len(), "shift mode must not drop samples");
        assert_eq!(out, src, "shift mode must not alter the samples");
    }

    #[test]
    fn shift_mode_position_still_tracks_the_recording() {
        // The frame cursor advances once per sample in both modes, so the
        // reported recording position stays honest when resampling.
        let h = SpeedHandle::new(2.0);
        h.set_mode(SpeedMode::ShiftPitch);
        let out: Vec<f32> = Stretch::new(sine(2.0, 220.0), h.clone()).collect();
        assert!(!out.is_empty());
        assert!(
            (h.position().as_secs_f64() - 2.0).abs() < 0.1,
            "shift-mode position {:?} should cover the 2s recording",
            h.position()
        );
    }

    #[test]
    fn toggling_mode_mid_stream_keeps_audio_flowing() {
        let h = SpeedHandle::new(1.6);
        let mut s = Stretch::new(sine(8.0, 220.0), h.clone());
        let mut out: Vec<f32> = (&mut s).take(SR as usize).collect();
        h.set_mode(SpeedMode::ShiftPitch);
        out.extend((&mut s).take(SR as usize * 4));
        assert!(out.iter().all(|v| v.is_finite()), "NaN/inf after toggle");
        assert!(
            out.len() > SR as usize * 4,
            "stream stalled after the mode toggle: {} samples",
            out.len()
        );
        // No long gap where the pipeline handed over.
        let quiet = out[out.len() - SR as usize..out.len() - SR as usize / 4]
            .chunks(SR as usize / 100)
            .filter(|w| rms(w) < 0.05)
            .count();
        assert_eq!(quiet, 0, "the mode hand-over dropped audio");

        // ... and back to stretching
        h.set_mode(SpeedMode::PreservePitch);
        let mut out2: Vec<f32> = (&mut s).take(SR as usize * 2).collect();
        out2.extend((&mut s).collect::<Vec<f32>>());
        assert!(out2.iter().all(|v| v.is_finite()));
        assert!(!out2.is_empty(), "no audio after switching back");
    }

    #[test]
    fn handle_clamps_invalid_factors() {
        let h = SpeedHandle::new(1.0);
        h.set(99.0);
        assert_eq!(h.factor(), MAX_FACTOR);
        h.set(0.0);
        assert_eq!(h.factor(), MIN_FACTOR);
        h.set(f32::NAN);
        assert_eq!(h.factor(), 1.0);
    }

    /// Ad-hoc probe on real music (run manually):
    /// `PIGMA_PROBE=/path.mp3:2.0 cargo test --offline --lib real_recording_probe -- --ignored --nocapture`
    ///
    /// Reports the tempo ratio, plus the log-frequency scale that best maps the
    /// original clip's averaged log-magnitude spectrum onto the stretched one
    /// (1.000 = pitch preserved). Only averaged spectra are compared, so the
    /// time alignment between the two signals is irrelevant. Pitch preservation
    /// is asserted deterministically on synthetic tones above; real material is
    /// where WSOLA actually has alignment information to work with.
    #[test]
    #[ignore]
    fn real_recording_probe() {
        use std::f64::consts::PI;
        use std::io::BufReader;

        const N: usize = 4_096;

        fn avg_log_spectrum(samples: &[f32], ch: usize) -> Vec<f64> {
            let mut acc = vec![0.0f64; N / 2];
            let mut used = 0usize;
            for b in 0..(samples.len() / (N * ch)).min(16) {
                let (mut re, mut im) = (vec![0.0f64; N / 2], vec![0.0f64; N / 2]);
                let hann: Vec<f64> = (0..N)
                    .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / (N - 1) as f64).cos())
                    .collect();
                let frame: Vec<f64> = (0..N)
                    .map(|i| samples[(b * N + i) * ch] as f64 * hann[i])
                    .collect();
                for k in 1..N / 2 {
                    let (mut cs, mut sn) = (1.0f64, 0.0f64);
                    let (dcs, dsn) = (
                        (2.0 * PI * k as f64 / N as f64).cos(),
                        (2.0 * PI * k as f64 / N as f64).sin(),
                    );
                    for v in &frame {
                        re[k] += v * cs;
                        im[k] -= v * sn;
                        let nc = cs * dcs - sn * dsn;
                        sn = sn * dcs + cs * dsn;
                        cs = nc;
                    }
                }
                for k in 1..N / 2 {
                    acc[k] += (re[k] * re[k] + im[k] * im[k] + 1e-20).ln();
                }
                used += 1;
            }
            for v in &mut acc {
                *v /= used.max(1) as f64;
            }
            acc
        }

        fn best_scale(a: &[f64], b: &[f64]) -> f64 {
            let mean = |v: &[f64]| v[40..N / 2].iter().sum::<f64>() / (N / 2 - 40) as f64;
            let (za, zb): (Vec<f64>, Vec<f64>) = (
                a.iter().map(|v| v - mean(a)).collect(),
                b.iter().map(|v| v - mean(b)).collect(),
            );
            let mut best = (f64::MIN, 1.0f64);
            let mut scale = 0.4f64;
            while scale <= 2.6 {
                let (mut num, mut den) = (0.0, 0.0);
                for (kb, zb_k) in zb.iter().enumerate().take(N / 2).skip(40) {
                    let ka = kb as f64 / scale;
                    let k0 = ka.floor() as usize;
                    if k0 + 1 >= N / 2 || k0 < 1 {
                        continue;
                    }
                    let frac = ka - k0 as f64;
                    let av = za[k0] * (1.0 - frac) + za[k0 + 1] * frac;
                    num += av * zb_k;
                    den += av * av;
                }
                let r2 = if den > 0.0 { num * num / den } else { 0.0 };
                if r2 > best.0 {
                    best = (r2, scale);
                }
                scale += 0.005;
            }
            best.1
        }

        let spec = std::env::var("PIGMA_PROBE").unwrap_or_default();
        let (path, factor) = spec.split_once(':').unwrap_or((spec.as_str(), "2.0"));
        if path.is_empty() {
            return;
        }
        let factor: f32 = factor.parse().unwrap();
        let file = BufReader::new(std::fs::File::open(path).unwrap());
        let dec = rodio::Decoder::new(file).unwrap();
        let sr = dec.sample_rate().get();
        let ch = dec.channels().get() as usize;
        let orig: Vec<f32> = dec.take(sr as usize * ch * 12).collect();
        let buf = SamplesBuffer::new(
            NonZero::new(ch as u16).unwrap(),
            NonZero::new(sr).unwrap(),
            orig.clone(),
        );
        let out: Vec<f32> = Stretch::new(buf, SpeedHandle::new(factor)).collect();
        let seconds = |v: &[f32]| v.len() as f64 / (sr as usize * ch) as f64;
        println!(
            "factor {factor}x: {:.1}s -> {:.1}s (tempo ratio {:.4})",
            seconds(&orig),
            seconds(&out),
            seconds(&orig) / seconds(&out),
        );
        println!(
            "pitch scale: {:.3} (want 1.000 = pitch preserved)",
            best_scale(&avg_log_spectrum(&orig, ch), &avg_log_spectrum(&out, ch)),
        );
    }
}
