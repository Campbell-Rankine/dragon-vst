//! Band-limited oscillators: OSC 1, OSC 2 (with hard sync to OSC 1 and
//! through-zero cross-mod from it) and the phase-locked sub-oscillator.
//!
//! **One sample of delay.** Every discontinuity (a saw wrap, a pulse edge,
//! a triangle corner, a sync reset) is corrected on the samples on both
//! sides of it (see [`super::blep`]). A sync reset can't be predicted from
//! the synced oscillator's own phase, so oscillators output each sample one
//! step late, while it can still be corrected. All oscillators share the
//! delay (about 0.02 ms), so they stay aligned with each other.
//!
//! **Phase can run backwards.** Through-zero FM can make OSC 2's frequency
//! negative. Discontinuities crossed backwards get the opposite step.

use super::blep::{blamp, blep};
use super::patch::{SubWave, Waveform};
use std::f32::consts::TAU;

/// Highest phase increment allowed (about 0.45 x sample rate), either way.
pub const MAX_DT: f32 = 0.45;

/// Highest cross-mod (FM) index.
pub const MAX_CROSS_MOD: f32 = 5.0;

/// Fractional part in `0..1`.
#[inline]
fn frac(x: f32) -> f32 {
    let f = x - x.floor();
    if f >= 1.0 {
        0.0
    } else {
        f
    }
}

/// A waveform as a function of phase, with its discontinuities listed so
/// they can be corrected wherever the phase crosses them.
#[derive(Debug, Clone, Copy)]
struct Shape {
    wave: Waveform,
    pw: f32,
}

/// A discontinuity at phase `at` (in `0..1`): `step` is the jump in value
/// and `bend` the jump in slope (per cycle), both seen moving forward.
#[derive(Debug, Clone, Copy)]
struct Disc {
    at: f32,
    step: f32,
    bend: f32,
}

impl Shape {
    /// Value at phase `t` in `0..=1` (`1` gives the limit from below).
    #[inline]
    fn value(&self, t: f32) -> f32 {
        match self.wave {
            Waveform::Saw => 2.0 * t - 1.0,
            // DC removed, as an analog synth's AC coupling would.
            Waveform::Pulse => (if t < self.pw { 1.0 } else { -1.0 }) - (2.0 * self.pw - 1.0),
            Waveform::Triangle => 1.0 - 4.0 * (t - 0.5).abs(),
            Waveform::Sine => (t * TAU).sin(),
        }
    }

    /// Slope per cycle at phase `t`.
    #[inline]
    fn slope(&self, t: f32) -> f32 {
        match self.wave {
            Waveform::Saw => 2.0,
            Waveform::Pulse => 0.0,
            Waveform::Triangle => {
                if t < 0.5 {
                    4.0
                } else {
                    -4.0
                }
            }
            Waveform::Sine => TAU * (t * TAU).cos(),
        }
    }

    #[inline]
    fn discs(&self) -> ([Disc; 2], usize) {
        let none = Disc { at: 0.0, step: 0.0, bend: 0.0 };
        match self.wave {
            Waveform::Saw => ([Disc { at: 0.0, step: -2.0, bend: 0.0 }, none], 1),
            Waveform::Pulse => (
                [Disc { at: 0.0, step: 2.0, bend: 0.0 }, Disc { at: self.pw, step: -2.0, bend: 0.0 }],
                2,
            ),
            Waveform::Triangle => (
                [Disc { at: 0.0, step: 0.0, bend: 8.0 }, Disc { at: 0.5, step: 0.0, bend: -8.0 }],
                2,
            ),
            Waveform::Sine => ([none, none], 0),
        }
    }
}

/// Corrections collected during one sample.
#[derive(Default)]
struct Corrections {
    before: f32,
    after: f32,
}

impl Corrections {
    /// A step of `height` and a slope change of `bend` per sample, `d`
    /// samples before the current one.
    #[inline]
    fn add(&mut self, height: f32, bend: f32, d: f32) {
        if height != 0.0 {
            let (b, a) = blep(d);
            self.before += height * b;
            self.after += height * a;
        }
        if bend != 0.0 {
            let (b, a) = blamp(d);
            self.before += bend * b;
            self.after += bend * a;
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Oscillator {
    /// Phase in cycles, `0..1`.
    phase: f32,
    /// The previous sample, output on the next tick (and still correctable).
    pending: f32,
}

impl Oscillator {
    pub fn set_phase(&mut self, phase: f32) {
        self.phase = frac(phase);
    }

    /// Start at `phase`, with the first output being the waveform's value
    /// there (rather than a jump from 0).
    pub fn start(&mut self, phase: f32, wave: Waveform, pw: f32) {
        self.phase = frac(phase);
        self.pending = Shape { wave, pw }.value(self.phase);
    }

    pub fn phase(&self) -> f32 {
        self.phase
    }

    /// Advance one sample by `dt` cycles (negative runs backwards). With
    /// `sync = Some(d)`, the phase resets to 0 `d` samples before the end
    /// of this step.
    ///
    /// Returns the band-limited sample from one step ago, and `Some(d)` if
    /// the phase wrapped forward `d` samples before the end of this step.
    #[inline]
    pub fn tick(&mut self, wave: Waveform, pw: f32, dt: f32, sync: Option<f32>) -> (f32, Option<f32>) {
        let dt = dt.clamp(-MAX_DT, MAX_DT);
        let shape = Shape { wave, pw };
        let mut c = Corrections::default();
        let mut wrap = None;

        let end = match sync {
            None => segment(&shape, self.phase, dt, 0.0, 1.0, &mut c, &mut wrap),
            Some(d) => {
                // Run up to the reset, jump, then run the rest of the sample.
                let until = 1.0 - d;
                let reached = frac(segment(&shape, self.phase, dt, 0.0, until, &mut c, &mut wrap));
                // Moving backwards, land at the top of the cycle instead.
                let restart = if dt >= 0.0 { 0.0 } else { 1.0 };
                c.add(
                    shape.value(restart) - shape.value(reached),
                    (shape.slope(restart) - shape.slope(reached)) * dt,
                    d,
                );
                segment(&shape, restart, dt, until, d, &mut c, &mut wrap)
            }
        };
        self.phase = frac(end);

        let out = self.pending + c.before;
        self.pending = shape.value(self.phase) + c.after;
        (out, wrap)
    }
}

/// Move from phase `from` for `len` samples starting `start` samples into
/// the step, correcting every discontinuity crossed. Returns the
/// (unwrapped) end phase.
#[inline]
fn segment(
    shape: &Shape,
    from: f32,
    dt: f32,
    start: f32,
    len: f32,
    c: &mut Corrections,
    wrap: &mut Option<f32>,
) -> f32 {
    let to = from + dt * len;
    if dt == 0.0 {
        return to;
    }
    // Samples from a crossing at phase `p` to the end of the step.
    let d_at = |p: f32| 1.0 - (start + (p - from) / dt);
    let (discs, n) = shape.discs();
    // |dt| < 0.5 and `from` is in 0..=1, so only this cycle and the next
    // (forward) or this one (backward) can be crossed.
    for disc in &discs[..n] {
        for p in [disc.at, disc.at + 1.0] {
            if dt > 0.0 && from < p && p <= to {
                c.add(disc.step, disc.bend * dt, d_at(p));
            } else if dt < 0.0 && to <= p && p < from {
                c.add(-disc.step, -disc.bend * dt, d_at(p));
            }
        }
    }
    if dt > 0.0 && from < 1.0 && to >= 1.0 {
        *wrap = Some(d_at(1.0));
    }
    to
}

/// Per-sample settings for [`OscPair::tick`].
#[derive(Debug, Clone, Copy)]
pub struct PairSettings {
    pub wave1: Waveform,
    pub pw1: f32,
    pub wave2: Waveform,
    pub pw2: f32,
    /// Reset OSC 2 whenever OSC 1 starts a cycle.
    pub sync: bool,
    /// FM index of OSC 1 into OSC 2's frequency, `0..=MAX_CROSS_MOD`.
    pub cross_mod: f32,
    pub sub_wave: SubWave,
    /// 2 or 4.
    pub sub_divider: u32,
    /// Pitch modulation as frequency ratios (1 = none): OSC 1 (and the
    /// sub, which follows it) and OSC 2.
    pub pitch_ratio1: f32,
    pub pitch_ratio2: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PairOut {
    pub osc1: f32,
    pub osc2: f32,
    pub sub: f32,
}

fn sub_pw(wave: SubWave) -> f32 {
    match wave {
        SubWave::Square => 0.5,
        SubWave::Pulse => 0.25,
    }
}

/// OSC 1, OSC 2 and the sub, with sync and cross-mod between them.
#[derive(Debug, Clone, Copy, Default)]
pub struct OscPair {
    osc1: Oscillator,
    osc2: Oscillator,
    sub: Oscillator,
    /// Completed OSC 1 cycles: the sub's phase is (count + OSC 1 phase) / divider.
    count: u32,
    dt1: f32,
    dt2: f32,
    /// Last OSC 1 output, the modulator for cross-mod.
    last1: f32,
}

impl OscPair {
    pub fn set_freqs(&mut self, f1: f32, f2: f32, sample_rate: f32) {
        self.dt1 = (f1 / sample_rate).clamp(0.0, MAX_DT);
        self.dt2 = (f2 / sample_rate).clamp(0.0, MAX_DT);
    }

    /// Start a note at the given phases. `sub_count` picks which OSC 1 cycle
    /// within the sub's longer cycle it starts in.
    pub fn start(&mut self, phase1: f32, phase2: f32, sub_count: u32, s: &PairSettings) {
        let div = s.sub_divider.max(1);
        self.osc1.start(phase1, s.wave1, s.pw1);
        self.osc2.start(phase2, s.wave2, s.pw2);
        self.count = sub_count % div;
        let sub_phase = (self.count as f32 + self.osc1.phase()) / div as f32;
        self.sub.start(sub_phase, Waveform::Pulse, sub_pw(s.sub_wave));
        self.last1 = 0.0;
    }

    pub fn dts(&self) -> (f32, f32) {
        (self.dt1, self.dt2)
    }

    pub fn phases(&self) -> (f32, f32) {
        (self.osc1.phase(), self.osc2.phase())
    }

    #[inline]
    pub fn tick(&mut self, s: &PairSettings) -> PairOut {
        let dt1 = (self.dt1 * s.pitch_ratio1).min(MAX_DT);
        let (osc1, wrap) = self.osc1.tick(s.wave1, s.pw1, dt1, None);

        // Linear through-zero FM: deviation = index x OSC 1 frequency, so the
        // index means the same at every pitch.
        let dt2 = self.dt2 * s.pitch_ratio2 + s.cross_mod * dt1 * self.last1;
        self.last1 = osc1;
        let (osc2, _) = self.osc2.tick(s.wave2, s.pw2, dt2, if s.sync { wrap } else { None });

        let div = s.sub_divider.max(1);
        let (sub, _) = self.sub.tick(Waveform::Pulse, sub_pw(s.sub_wave), dt1 / div as f32, None);
        if wrap.is_some() {
            // Re-lock the sub to OSC 1 (removes float drift; normally a no-op).
            self.count = (self.count + 1) % div;
            self.sub.set_phase((self.count as f32 + self.osc1.phase()) / div as f32);
        }

        PairOut { osc1, osc2, sub }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub const SR: f32 = 48_000.0;

    fn settings(wave1: Waveform, wave2: Waveform) -> PairSettings {
        PairSettings {
            wave1,
            pw1: 0.5,
            wave2,
            pw2: 0.5,
            sync: false,
            cross_mod: 0.0,
            sub_wave: SubWave::Square,
            sub_divider: 2,
            pitch_ratio1: 1.0,
            pitch_ratio2: 1.0,
        }
    }

    fn render_pair(f1: f32, f2: f32, s: &PairSettings, n: usize) -> Vec<PairOut> {
        let mut p = OscPair::default();
        p.set_freqs(f1, f2, SR);
        p.start(0.0, 0.0, 0, s);
        (0..n).map(|_| p.tick(s)).collect()
    }

    pub fn render(wave: Waveform, freq: f32, pw: f32, n: usize) -> Vec<f32> {
        let mut o = Oscillator::default();
        o.start(0.0, wave, pw);
        (0..n).map(|_| o.tick(wave, pw, freq / SR, None).0).collect()
    }

    /// Power of the sinusoid at `freq` (Goertzel). Exact for integer-Hz
    /// tones over a whole number of seconds.
    pub fn tone_power(x: &[f32], freq: f64) -> f64 {
        let w = std::f64::consts::TAU * freq / SR as f64;
        let c = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0f64, 0.0f64);
        for &v in x {
            let s = v as f64 + c * s1 - s2;
            s2 = s1;
            s1 = s;
        }
        let n = x.len() as f64;
        let mag_sq = s1 * s1 + s2 * s2 - c * s1 * s2;
        2.0 * mag_sq / (n * n)
    }

    /// Power that is not at DC or a harmonic of `f0`, i.e. aliasing, relative
    /// to the total, in dB.
    fn alias_db(x: &[f32], f0: f64) -> f64 {
        let n = x.len() as f64;
        let mean = x.iter().map(|&v| v as f64).sum::<f64>() / n;
        let total = x.iter().map(|&v| (v as f64 - mean).powi(2)).sum::<f64>() / n;
        let mut harmonics = 0.0;
        let mut k = 1.0;
        while k * f0 < SR as f64 / 2.0 {
            harmonics += tone_power(x, k * f0);
            k += 1.0;
        }
        10.0 * ((total - harmonics).max(1e-30) / total).log10()
    }

    /// Naive (aliasing) waveform, optionally hard-synced to a master at `sync_hz`.
    fn naive(wave: Waveform, f0: f32, sync_hz: Option<f32>, n: usize) -> Vec<f32> {
        let shape = Shape { wave, pw: 0.5 };
        let (mut t, mut m) = (0.0f32, 0.0f32);
        (0..n)
            .map(|_| {
                let y = shape.value(t);
                t = frac(t + f0 / SR);
                if let Some(fm) = sync_hz {
                    m += fm / SR;
                    if m >= 1.0 {
                        m -= 1.0;
                        t = m / (fm / SR) * (f0 / SR);
                    }
                }
                y
            })
            .collect()
    }

    /// Frequency from upward zero crossings (linear interpolation), over the
    /// span between the first and last crossing.
    pub fn measured_freq(x: &[f32]) -> f64 {
        let mut crossings = Vec::new();
        for i in 1..x.len() {
            if x[i - 1] < 0.0 && x[i] >= 0.0 {
                let frac = x[i - 1] as f64 / (x[i - 1] as f64 - x[i] as f64);
                crossings.push(i as f64 - 1.0 + frac);
            }
        }
        let span = crossings.last().unwrap() - crossings[0];
        (crossings.len() - 1) as f64 / span * SR as f64
    }

    #[test]
    fn pitch_is_accurate_to_a_tenth_of_a_cent() {
        for f in [27.5f32, 261.63, 440.0, 3520.0, 12_000.0] {
            let x = render(Waveform::Sine, f, 0.5, SR as usize * 4);
            let cents = 1200.0 * (measured_freq(&x) / f as f64).log2();
            assert!(cents.abs() < 0.1, "{f} Hz off by {cents} cents");
        }
    }

    /// Integer-Hz test tones so harmonics and aliases fall on exact 1 Hz bins.
    #[test]
    fn blep_waves_alias_far_less_than_naive() {
        let n = SR as usize;
        for (wave, f0, min_gain_db) in [
            (Waveform::Saw, 1_201.0, 12.0),
            (Waveform::Saw, 4_801.0, 9.0),
            (Waveform::Pulse, 1_201.0, 12.0),
            (Waveform::Triangle, 1_201.0, 10.0),
        ] {
            let blep = alias_db(&render(wave, f0, 0.5, n), f0 as f64);
            let raw = alias_db(&naive(wave, f0, None, n), f0 as f64);
            assert!(raw - blep >= min_gain_db, "{wave:?} {f0} Hz: naive {raw:.1} dB, blep {blep:.1} dB");
        }
    }

    #[test]
    fn pulse_has_no_dc_at_any_width() {
        for pw in [0.05f32, 0.25, 0.5, 0.8, 0.95] {
            // 100 Hz = 480 samples per cycle: average over 100 whole cycles.
            let x = render(Waveform::Pulse, 100.0, pw, 48_000);
            let mean = x.iter().map(|&v| v as f64).sum::<f64>() / x.len() as f64;
            assert!(mean.abs() < 1e-3, "pw {pw}: DC {mean}");
        }
    }

    #[test]
    fn output_is_bounded_up_to_nyquist() {
        for wave in [Waveform::Saw, Waveform::Pulse, Waveform::Triangle, Waveform::Sine] {
            for f in [20.0f32, 1_000.0, 10_000.0, 21_000.0, 40_000.0] {
                for pw in [0.05f32, 0.5, 0.95] {
                    let x = render(wave, f, pw, 4_800);
                    let peak = x.iter().fold(0f32, |m, v| m.max(v.abs()));
                    // DC removal can push a narrow pulse to almost +-1.9.
                    assert!(peak.is_finite() && peak <= 2.0, "{wave:?} {f} Hz pw {pw}: peak {peak}");
                }
            }
        }
    }

    #[test]
    fn sub_oscillator_is_locked_an_octave_or_two_down() {
        for (div, wave) in [(2u32, SubWave::Square), (4, SubWave::Square), (2, SubWave::Pulse)] {
            let s = PairSettings { sub_divider: div, sub_wave: wave, ..settings(Waveform::Saw, Waveform::Saw) };
            let x: Vec<f32> = render_pair(440.0, 440.0, &s, SR as usize * 2).iter().map(|o| o.sub).collect();
            let f = measured_freq(&x);
            let expected = 440.0 / div as f64;
            assert!((f / expected - 1.0).abs() < 1e-4, "div {div}: {f} Hz, expected {expected}");
        }
    }

    /// Hard sync: OSC 2 restarts with every OSC 1 cycle, so its output
    /// repeats at OSC 1's period whatever its own frequency.
    #[test]
    fn sync_locks_osc2_to_osc1_period() {
        let period = |s: &PairSettings| {
            // 480 Hz = exactly 100 samples per cycle.
            let x: Vec<f32> = render_pair(480.0, 480.0 * 1.37, s, 4_800).iter().map(|o| o.osc2).collect();
            (1_000..4_700).map(|i| (x[i] - x[i + 100]).abs()).fold(0f32, f32::max)
        };
        let synced = period(&PairSettings { sync: true, ..settings(Waveform::Saw, Waveform::Saw) });
        let free = period(&settings(Waveform::Saw, Waveform::Saw));
        assert!(synced < 0.02, "synced OSC 2 not periodic at OSC 1's rate: {synced}");
        assert!(free > 0.5, "unsynced OSC 2 should not repeat at OSC 1's rate");
    }

    #[test]
    fn synced_waves_alias_far_less_than_naive_sync() {
        let n = SR as usize;
        let f1 = 1_201.0;
        for (wave, ratio, min_gain_db) in [
            (Waveform::Saw, 1.37f32, 12.0),
            (Waveform::Saw, 2.71, 12.0),
            (Waveform::Pulse, 1.61, 12.0),
            (Waveform::Triangle, 1.37, 10.0),
            (Waveform::Sine, 1.37, 10.0),
        ] {
            let s = PairSettings { sync: true, ..settings(Waveform::Saw, wave) };
            let x: Vec<f32> = render_pair(f1, f1 * ratio, &s, n).iter().map(|o| o.osc2).collect();
            let blep = alias_db(&x, f1 as f64);
            let raw = alias_db(&naive(wave, f1 * ratio, Some(f1), n), f1 as f64);
            assert!(raw - blep >= min_gain_db, "{wave:?} x{ratio}: naive {raw:.1} dB, blep {blep:.1} dB");
        }
    }

    /// Sine into sine: OSC 2 = sin(2 pi f2 t + I sin(2 pi f1 t + phi)), whose
    /// components are Bessel functions of the index: J0(I) at f2, J1(I) at
    /// f2 +- f1.
    #[test]
    fn cross_mod_index_matches_fm_theory() {
        // J0 and J1 at I = 1 and I = 2.
        for (index, j0, j1) in [(1.0f32, 0.765_197_7, 0.440_050_6), (2.0, 0.223_890_8, 0.576_724_8)] {
            let s = PairSettings { cross_mod: index, ..settings(Waveform::Sine, Waveform::Sine) };
            let x: Vec<f32> = render_pair(300.0, 2_400.0, &s, SR as usize).iter().map(|o| o.osc2).collect();
            let amp = |f: f64| (2.0 * tone_power(&x, f)).sqrt();
            assert!((amp(2_400.0) - j0).abs() < 0.01, "I={index}: carrier {} vs J0 {j0}", amp(2_400.0));
            assert!((amp(2_100.0) - j1).abs() < 0.01, "I={index}: lower sideband {} vs J1 {j1}", amp(2_100.0));
            assert!((amp(2_700.0) - j1).abs() < 0.01, "I={index}: upper sideband {} vs J1 {j1}", amp(2_700.0));
        }
    }

    /// At an index above 1 with equal frequencies, OSC 2's frequency goes
    /// negative for part of each cycle (through-zero).
    #[test]
    fn through_zero_fm_stays_bounded_and_periodic() {
        for wave in [Waveform::Saw, Waveform::Pulse, Waveform::Triangle, Waveform::Sine] {
            for sync in [false, true] {
                let s = PairSettings { cross_mod: MAX_CROSS_MOD, sync, ..settings(Waveform::Sine, wave) };
                let x: Vec<f32> = render_pair(480.0, 480.0, &s, 9_600).iter().map(|o| o.osc2).collect();
                assert!(x.iter().all(|v| v.is_finite() && v.abs() <= 2.5), "{wave:?} sync {sync}");
                // Modulator and carrier share a period, so the result repeats.
                let worst = (2_000..9_400).map(|i| (x[i] - x[i + 100]).abs()).fold(0f32, f32::max);
                assert!(worst < 0.05, "{wave:?} sync {sync}: not periodic ({worst})");
            }
        }
    }

    #[test]
    fn all_outputs_are_one_sample_late_together() {
        let s = settings(Waveform::Sine, Waveform::Sine);
        let out = render_pair(1_000.0, 1_000.0, &s, 3);
        // Each output is the value one step behind the phase: first the
        // start phase (0, where sin is 0 and the sub square is high).
        let dt = 1_000.0 / SR;
        assert_eq!(out[0], PairOut { osc1: 0.0, osc2: 0.0, sub: 1.0 });
        assert!((out[1].osc1 - (TAU * dt).sin()).abs() < 1e-6);
        assert!((out[2].osc1 - (TAU * 2.0 * dt).sin()).abs() < 1e-6);
        assert_eq!(out[2].osc1, out[2].osc2);
    }
}
