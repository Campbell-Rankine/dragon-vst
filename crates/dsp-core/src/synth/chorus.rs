//! Stereo chorus / ensemble, modelled on bucket-brigade (BBD) designs.
//!
//! - **I / II:** one triangle LFO sweeps two short delays (one per side)
//!   in opposite directions, as in the Juno-style chorus: wide and gentle
//!   (I) or faster and deeper (II).
//! - **Ensemble:** three taps of one delay line, swept by a slow and a
//!   fast sine at 0, 120 and 240 degrees, mixed to two outputs: the dense,
//!   shimmering string-machine / Polysix ensemble.
//!
//! The wet path has a low-pass "tone" (BBDs are band-limited) and a gentle
//! saturation. Delays are read with 4-point Hermite interpolation, so the
//! sweeps don't zipper or alias much.

use super::filter::{prewarp, sat, StateVariable, SvfMode};
use super::patch::{ChorusMode, ChorusSettings};
use std::f32::consts::TAU;

/// Longest delay any mode can reach (with depth at its maximum), seconds.
pub const MAX_DELAY_SECS: f32 = 0.02;
/// Most depth (x the mode's own sweep).
pub const MAX_DEPTH: f32 = 2.0;

/// A mode's tuning: centre delay, sweep depth (each in ms) and rate.
struct Tuning {
    centre_ms: f32,
    depth_ms: f32,
    rate_hz: f32,
    /// Ensemble only: a fast shimmer on top of the slow sweep.
    fast_depth_ms: f32,
    fast_rate_hz: f32,
}

fn tuning(mode: ChorusMode) -> Tuning {
    match mode {
        ChorusMode::Off => Tuning { centre_ms: 0.0, depth_ms: 0.0, rate_hz: 0.0, fast_depth_ms: 0.0, fast_rate_hz: 0.0 },
        ChorusMode::One => Tuning { centre_ms: 5.0, depth_ms: 1.5, rate_hz: 0.5, fast_depth_ms: 0.0, fast_rate_hz: 0.0 },
        ChorusMode::Two => Tuning { centre_ms: 5.0, depth_ms: 2.0, rate_hz: 0.85, fast_depth_ms: 0.0, fast_rate_hz: 0.0 },
        ChorusMode::Ensemble => {
            Tuning { centre_ms: 7.0, depth_ms: 1.5, rate_hz: 0.6, fast_depth_ms: 0.25, fast_rate_hz: 5.5 }
        }
    }
}

/// Triangle, -1..1, from a phase in cycles.
#[inline]
fn triangle(p: f32) -> f32 {
    1.0 - 4.0 * ((p + 0.25).rem_euclid(1.0) - 0.5).abs()
}

/// 4-point, 3rd-order Hermite interpolation between `x1` and `x2` at `t` (0..1).
#[inline]
fn hermite(x0: f32, x1: f32, x2: f32, x3: f32, t: f32) -> f32 {
    let c1 = 0.5 * (x2 - x0);
    let c2 = x0 - 2.5 * x1 + 2.0 * x2 - 0.5 * x3;
    let c3 = 0.5 * (x3 - x0) + 1.5 * (x1 - x2);
    ((c3 * t + c2) * t + c1) * t + x1
}

/// A delay line with a power-of-two buffer.
#[derive(Debug, Clone, Default)]
struct DelayLine {
    buf: Vec<f32>,
    mask: usize,
    write: usize,
}

impl DelayLine {
    fn new(min_len: usize) -> Self {
        let len = min_len.next_power_of_two();
        Self { buf: vec![0.0; len], mask: len - 1, write: 0 }
    }

    fn clear(&mut self) {
        self.buf.iter_mut().for_each(|x| *x = 0.0);
        self.write = 0;
    }

    #[inline]
    fn push(&mut self, x: f32) {
        self.write = (self.write + 1) & self.mask;
        self.buf[self.write] = x;
    }

    /// The input from `delay` samples ago (at least 1, fractional).
    #[inline]
    fn read(&self, delay: f32) -> f32 {
        let d = delay.max(1.0);
        let whole = d as usize;
        let t = d - whole as f32;
        // Sample `whole` ago is x1 and `whole + 1` ago is x2; t moves from x1 toward x2.
        let at = |back: usize| self.buf[self.write.wrapping_sub(back) & self.mask];
        hermite(at(whole.saturating_sub(1)), at(whole), at(whole + 1), at(whole + 2), t)
    }
}

/// Per-sample chorus controls.
#[derive(Debug, Clone, Copy)]
pub struct ChorusNow {
    pub mode: ChorusMode,
    /// Rate multiplier on the mode's own rate.
    pub rate: f32,
    /// Depth multiplier on the mode's own sweep, 0..=[`MAX_DEPTH`].
    pub depth: f32,
    /// 0 = dry .. 1 = wet only.
    pub mix: f32,
    /// Tone low-pass `g`.
    pub tone_g: f32,
}

#[derive(Debug, Clone)]
pub struct Chorus {
    sample_rate: f32,
    left: DelayLine,
    right: DelayLine,
    /// Slow and fast LFO phases, cycles.
    phase: f32,
    fast_phase: f32,
    tone: [StateVariable; 2],
}

/// Butterworth damping for the tone filter.
const TONE_K: f32 = std::f32::consts::SQRT_2;

impl Chorus {
    /// Allocates its delay lines (the only allocation in the synth).
    pub fn new(sample_rate: f32) -> Self {
        let len = (MAX_DELAY_SECS * sample_rate) as usize + 8;
        Self {
            sample_rate,
            left: DelayLine::new(len),
            right: DelayLine::new(len),
            phase: 0.0,
            fast_phase: 0.0,
            tone: [StateVariable::default(); 2],
        }
    }

    /// A placeholder with no buffers (no allocation), for swapping.
    pub fn empty() -> Self {
        Self {
            sample_rate: 0.0,
            left: DelayLine::default(),
            right: DelayLine::default(),
            phase: 0.0,
            fast_phase: 0.0,
            tone: [StateVariable::default(); 2],
        }
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Silence the delay lines and restart the LFOs (no allocation).
    pub fn clear(&mut self) {
        self.left.clear();
        self.right.clear();
        self.phase = 0.0;
        self.fast_phase = 0.0;
        self.tone = [StateVariable::default(); 2];
    }

    pub fn tone_g(&self, settings: &ChorusSettings) -> f32 {
        prewarp(settings.tone, self.sample_rate)
    }

    /// Process one stereo frame in place. Off, or a mix of 0, is an exact
    /// bypass (the delay lines keep running so turning it on is seamless).
    #[inline]
    pub fn process(&mut self, l: &mut f32, r: &mut f32, c: &ChorusNow) {
        let (dry_l, dry_r) = (*l, *r);
        let tune = tuning(c.mode);
        let ms = self.sample_rate / 1_000.0;
        let dt = 1.0 / self.sample_rate;

        let (wet_l, wet_r) = match c.mode {
            ChorusMode::Off => {
                self.left.push(dry_l);
                self.right.push(dry_r);
                return;
            }
            ChorusMode::One | ChorusMode::Two => {
                self.left.push(dry_l);
                self.right.push(dry_r);
                let sweep = tune.depth_ms * c.depth * triangle(self.phase);
                // Opposite directions on each side: the stereo width.
                let dl = (tune.centre_ms + sweep) * ms;
                let dr = (tune.centre_ms - sweep) * ms;
                (self.left.read(dl), self.right.read(dr))
            }
            ChorusMode::Ensemble => {
                // The classic ensemble feeds one line with the mono sum.
                self.left.push(0.5 * (dry_l + dry_r));
                self.right.push(0.0);
                let mut taps = [0.0f32; 3];
                for (i, tap) in taps.iter_mut().enumerate() {
                    let offset = i as f32 / 3.0;
                    let slow = (TAU * (self.phase + offset)).sin() * tune.depth_ms;
                    let fast = (TAU * (self.fast_phase + offset)).sin() * tune.fast_depth_ms;
                    *tap = self.left.read((tune.centre_ms + c.depth * (slow + fast)) * ms);
                }
                ((2.0 * taps[0] + taps[1]) / 3.0, (2.0 * taps[2] + taps[1]) / 3.0)
            }
        };
        self.phase = (self.phase + tune.rate_hz * c.rate * dt).fract();
        self.fast_phase = (self.fast_phase + tune.fast_rate_hz * c.rate * dt).fract();

        if c.mix == 0.0 {
            return;
        }
        let wet_l = sat(self.tone[0].process(wet_l, c.tone_g, TONE_K, SvfMode::LowPass));
        let wet_r = sat(self.tone[1].process(wet_r, c.tone_g, TONE_K, SvfMode::LowPass));
        *l = dry_l + c.mix * (wet_l - dry_l);
        *r = dry_r + c.mix * (wet_r - dry_r);
    }
}

/// Output soft clipper: exactly linear up to [`CLIP_KNEE`], then bends
/// smoothly (matching slope) toward +-1, never past it.
pub const CLIP_KNEE: f32 = 0.8;

#[inline]
pub fn soft_clip(x: f32) -> f32 {
    let a = x.abs();
    if a <= CLIP_KNEE {
        x
    } else {
        let room = 1.0 - CLIP_KNEE;
        x.signum() * (CLIP_KNEE + room * ((a - CLIP_KNEE) / room).tanh())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::osc::tests::{measured_freq, SR};

    fn settings(mode: ChorusMode, mix: f32) -> ChorusNow {
        ChorusNow { mode, rate: 1.0, depth: 1.0, mix, tone_g: prewarp(20_000.0, SR) }
    }

    fn sine(f: f32, i: usize) -> f32 {
        0.5 * (TAU * f * i as f32 / SR).sin()
    }

    /// Run a stereo sine (same on both sides) through the chorus.
    fn render(c: &mut Chorus, s: &ChorusNow, f: f32, n: usize) -> (Vec<f32>, Vec<f32>) {
        (0..n)
            .map(|i| {
                let (mut l, mut r) = (sine(f, i), sine(f, i));
                c.process(&mut l, &mut r, s);
                (l, r)
            })
            .unzip()
    }

    #[test]
    fn hermite_delay_is_accurate_at_fractional_delays() {
        let mut d = DelayLine::new(1024);
        let f = 500.0;
        for i in 0..2_000 {
            d.push(sine(f, i));
        }
        let now = 1_999;
        for delay in [10.0f32, 10.25, 10.5, 33.7] {
            let expected = 0.5 * (TAU * f * (now as f32 - delay) / SR).sin();
            assert!((d.read(delay) - expected).abs() < 1e-4, "delay {delay}");
        }
    }

    #[test]
    fn off_and_zero_mix_are_exact_bypasses() {
        for (mode, mix) in [(ChorusMode::Off, 0.5), (ChorusMode::One, 0.0), (ChorusMode::Ensemble, 0.0)] {
            let mut c = Chorus::new(SR);
            let (l, r) = render(&mut c, &settings(mode, mix), 440.0, 4_800);
            let dry: Vec<f32> = (0..4_800).map(|i| sine(440.0, i)).collect();
            assert_eq!(l, dry, "{mode:?} mix {mix}");
            assert_eq!(r, dry);
        }
    }

    #[test]
    fn every_mode_stays_within_the_delay_buffer() {
        let ms = SR / 1_000.0;
        for mode in [ChorusMode::One, ChorusMode::Two, ChorusMode::Ensemble] {
            let t = tuning(mode);
            let max = (t.centre_ms + MAX_DEPTH * (t.depth_ms + t.fast_depth_ms)) * ms;
            let min = (t.centre_ms - MAX_DEPTH * (t.depth_ms + t.fast_depth_ms)) * ms;
            assert!(max < MAX_DELAY_SECS * SR, "{mode:?} reaches {max} samples");
            assert!(min >= 2.0, "{mode:?} gets too short ({min} samples)");
        }
    }

    #[test]
    fn wet_chorus_wobbles_the_pitch_and_widens() {
        for mode in [ChorusMode::One, ChorusMode::Two, ChorusMode::Ensemble] {
            let mut c = Chorus::new(SR);
            let (l, r) = render(&mut c, &settings(mode, 1.0), 440.0, SR as usize * 4);
            // Doppler from the sweep: short windows land above and below 440 Hz.
            let freqs: Vec<f64> = (0..16).map(|k| measured_freq(&l[k * 12_000..k * 12_000 + 6_000])).collect();
            let (lo, hi) = freqs.iter().fold((f64::MAX, f64::MIN), |(a, b), &f| (a.min(f), b.max(f)));
            assert!(lo < 439.5 && hi > 440.5, "{mode:?}: {lo:.2}..{hi:.2} Hz");
            assert!(l.iter().zip(&r).any(|(a, b)| (a - b).abs() > 0.05), "{mode:?}: left and right should differ");
            assert!(l.iter().chain(&r).all(|v| v.is_finite() && v.abs() <= 1.0));
        }
    }

    #[test]
    fn sweeps_are_smooth() {
        // A 100 Hz sine changes by at most 2 pi f A / fs per sample; the
        // swept delay may add a little Doppler but no jumps.
        let max_step = TAU * 100.0 * 0.5 / SR;
        for mode in [ChorusMode::One, ChorusMode::Two, ChorusMode::Ensemble] {
            let mut c = Chorus::new(SR);
            let mut s = settings(mode, 1.0);
            s.depth = MAX_DEPTH;
            s.rate = 4.0;
            let (l, _) = render(&mut c, &s, 100.0, SR as usize);
            let worst = l[2_000..].windows(2).map(|w| (w[1] - w[0]).abs()).fold(0f32, f32::max);
            assert!(worst < max_step * 1.2, "{mode:?}: step {worst} vs {max_step}");
        }
    }

    #[test]
    fn tone_darkens_the_wet_path() {
        let mut bright = Chorus::new(SR);
        let mut dark = Chorus::new(SR);
        let mut s = settings(ChorusMode::One, 1.0);
        let (b, _) = render(&mut bright, &s, 8_000.0, 9_600);
        s.tone_g = prewarp(1_000.0, SR);
        let (d, _) = render(&mut dark, &s, 8_000.0, 9_600);
        let peak = |x: &[f32]| x[4_800..].iter().fold(0f32, |m, v| m.max(v.abs()));
        // 2-pole low-pass 3 octaves below: about -36 dB.
        assert!(peak(&d) < peak(&b) * 0.03, "{} vs {}", peak(&d), peak(&b));
    }

    #[test]
    fn clear_silences_without_reallocating() {
        let mut c = Chorus::new(SR);
        let ptr = c.left.buf.as_ptr();
        render(&mut c, &settings(ChorusMode::Two, 1.0), 440.0, 1_000);
        c.clear();
        assert_eq!(c.left.buf.as_ptr(), ptr);
        let (l, _) = render(&mut c, &settings(ChorusMode::Two, 1.0), 0.0, 1_000);
        assert!(l.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn soft_clip_is_transparent_below_the_knee_and_bounded() {
        for x in [-0.8f32, -0.3, 0.0, 0.5, 0.8] {
            assert_eq!(soft_clip(x), x);
        }
        let mut last = soft_clip(-10.0);
        for i in 0..20_000 {
            let x = -10.0 + i as f32 * 0.001;
            let y = soft_clip(x);
            assert!(y.abs() < 1.0 + 1e-6, "{x} -> {y}");
            assert!(y >= last, "monotonic");
            assert!((y - last).abs() <= 0.001 + 1e-6, "slope never above 1");
            last = y;
        }
    }
}
