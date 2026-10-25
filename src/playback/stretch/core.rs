//! WSOLA buffer and alignment search: the actual time-stretching pipeline.

use std::collections::VecDeque;

use rodio::Source;

use super::handle::{SpeedHandle, SpeedMode};

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
const NANOS_PER_SEC: u64 = 1_000_000_000;

/// A run of queued output samples and where in the recording they came from, so
/// the reported position follows the (deliberately discontinuous) WSOLA read
/// jumps instead of drifting.
#[derive(Debug, Clone, Copy)]
pub(super) struct Block {
    /// Recording frame of the block's first sample.
    pub(super) at: u64,
    /// Samples in the block.
    pub(super) len: usize,
    /// Samples already handed out.
    pub(super) done: usize,
}

/// Wraps a [`Source`], time-scaling it while preserving pitch.
#[derive(Debug)]
pub struct Stretch<I> {
    pub(super) input: I,
    pub(super) handle: SpeedHandle,
    /// Frames per analysis window (even, so `hop == win / 2`).
    win: usize,
    /// Synthesis hop in frames: frames produced per block.
    pub(super) hop: usize,
    /// Frames scored per candidate alignment.
    corr: usize,
    /// Hann window of length `win`; `window[i] + window[i + hop] == 1`.
    window: Vec<f32>,
    pub(super) nanos_per_frame: u64,
    /// Lookahead buffer (interleaved) starting at recording frame `buf_start`.
    pub(super) buf: Vec<f32>,
    pub(super) buf_start: u64,
    /// Previous analysis window and the recording frame it starts at.
    pub(super) prev: Vec<f32>,
    pub(super) prev_start: u64,
    /// Tempo accounting since the current anchor: the frames the read head
    /// *should* have advanced (`ideal`) versus the frames it actually did
    /// (`actual`), plus the factor they were accumulated under. Used to
    /// compensate the drift WSOLA introduces when it snaps to periodic
    /// material; re-anchored whenever the factor changes.
    ideal: f64,
    actual: u64,
    anchor_factor: f32,
    /// Output samples awaiting `next()`, all belonging to `block`.
    pub(super) out: VecDeque<f32>,
    pub(super) block: Option<Block>,
    /// A window has been primed, i.e. the stretcher is actually running.
    pub(super) primed: bool,
    pub(super) input_done: bool,
    pub(super) finished: bool,
}

impl<I: Source + Send> Stretch<I> {
    /// Wrap `input`. `handle` must be the same one the player loop controls —
    /// creating a source resets the reported position (it is per source).
    pub fn new(input: I, handle: SpeedHandle) -> Self {
        handle.reset_position();
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
    pub(super) fn ch(&self) -> usize {
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
    pub(super) fn step(&mut self) -> bool {
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
