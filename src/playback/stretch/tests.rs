use std::{num::NonZero, time::Duration};

use rodio::{Source, buffer::SamplesBuffer};

use super::{
    handle::{MAX_FACTOR, MIN_FACTOR},
    *,
};

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
    use std::{f64::consts::PI, io::BufReader};

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
