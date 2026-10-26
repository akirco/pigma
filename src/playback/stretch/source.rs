//! `Iterator`/`Source` adapters that make the WSOLA core a drop-in rodio source.

use std::time::Duration;

use rodio::{ChannelCount, Sample, SampleRate, Source, source::SeekError};

use super::{core::Stretch, handle::SpeedMode};

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
                        self.handle
                            .set_position_nanos(frame.saturating_mul(self.nanos_per_frame));
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
        self.handle.reset_position();
        Ok(())
    }
}
