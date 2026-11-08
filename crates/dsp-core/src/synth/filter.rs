//! The voice filters: drive, a one-pole high-pass, and the main filter in
//! one of three models.
//!
//! | Model | Character | Structure |
//! |---|---|---|
//! | LP24 "Six" | Polysix / Poly-800 (SSM2044-like): smooth and warm | 4-pole ZDF ladder (Zavalishin), saturating feedback |
//! | SVF | Clean, Oberheim-like; LP / BP / HP / notch | Trapezoidal state-variable filter (Simper) |
//! | MS | MS-20 (Korg35): aggressive, screaming; LP / HP | ZDF Korg35 (Pirkle, after Zavalishin), saturating feedback |
//!
//! All are zero-delay-feedback / trapezoidal designs: their cutoff is exact
//! (pre-warped) and they stay stable when cutoff and resonance change every
//! sample. Coefficients come in as `g = tan(pi * fc / fs)`.
//!
//! The ladder and MS filters saturate inside their feedback loops with
//! [`sat`], which keeps self-oscillation bounded. The SVF is linear.

use super::patch::FilterModel;
use std::f32::consts::PI;

/// Ladder feedback at full resonance. 4 is the linear self-oscillation
/// point; a little more makes it sing reliably, and [`sat`] bounds it.
pub const LADDER_MAX_K: f32 = 4.4;
/// MS (Korg35) feedback at full resonance (self-oscillates from about 2).
pub const MS_MAX_K: f32 = 2.1;
/// MS feedback at zero resonance (the structure needs some).
pub const MS_MIN_K: f32 = 0.01;
/// SVF Q range: 0.5 (no resonance) to 20.
pub const SVF_MIN_Q: f32 = 0.5;
pub const SVF_MAX_Q: f32 = 20.0;
/// Most drive, in dB.
pub const MAX_DRIVE_DB: f32 = 24.0;
/// HPF cutoffs at or below this are "off" (bypassed).
pub const HPF_OFF_HZ: f32 = 20.0;
/// Highest cutoff, as a fraction of the sample rate.
pub const MAX_CUTOFF_RATIO: f32 = 0.45;
/// Lowest cutoff, Hz.
pub const MIN_CUTOFF_HZ: f32 = 5.0;

/// Soft saturation with a headroom of +-2: close to linear for small
/// signals (-0.7 dB at 1.0), limiting smoothly to +-2.
#[inline]
pub fn sat(x: f32) -> f32 {
    2.0 * (0.5 * x).tanh()
}

/// `g = tan(pi fc / fs)` for a cutoff in Hz, clamped to the usable range.
#[inline]
pub fn prewarp(cutoff_hz: f32, sample_rate: f32) -> f32 {
    let fc = cutoff_hz.clamp(MIN_CUTOFF_HZ, MAX_CUTOFF_RATIO * sample_rate);
    fast_tan(PI * fc / sample_rate)
}

/// `tan(x)` for `0 <= x <= 0.45 pi` (cutoffs up to 0.45 x the sample rate),
/// within 2e-6 relative of `f32::tan` and several times faster. A [5/4]
/// Pade approximant at x/2 (where it is very accurate), then the
/// double-angle formula tan x = 2t / (1 - t^2).
#[inline]
pub fn fast_tan(x: f32) -> f32 {
    let h = 0.5 * x;
    let h2 = h * h;
    let t = h * (945.0 - 105.0 * h2 + h2 * h2) / (945.0 - 420.0 * h2 + 15.0 * h2 * h2);
    2.0 * t / (1.0 - t * t)
}

/// Ladder feedback for a 0..1 resonance.
#[inline]
pub fn ladder_k(resonance: f32) -> f32 {
    LADDER_MAX_K * resonance
}

/// SVF damping (1/Q) for a 0..1 resonance. Q rises exponentially so the
/// knob feels even.
#[inline]
pub fn svf_k(resonance: f32) -> f32 {
    1.0 / (SVF_MIN_Q * (SVF_MAX_Q / SVF_MIN_Q).powf(resonance))
}

/// MS (Korg35) feedback for a 0..1 resonance.
#[inline]
pub fn ms_k(resonance: f32) -> f32 {
    MS_MIN_K + (MS_MAX_K - MS_MIN_K) * resonance
}

/// Feedback / damping for `model` at a 0..1 resonance.
#[inline]
pub fn model_k(model: FilterModel, resonance: f32) -> f32 {
    match model {
        FilterModel::Off => 0.0,
        FilterModel::Ladder => ladder_k(resonance),
        FilterModel::SvfLowPass | FilterModel::SvfBandPass | FilterModel::SvfHighPass | FilterModel::SvfNotch => {
            svf_k(resonance)
        }
        FilterModel::MsLowPass | FilterModel::MsHighPass => ms_k(resonance),
    }
}

/// The HPF's one-pole `G = g / (1 + g)` at `hz`, or `None` (bypassed) at
/// or below [`HPF_OFF_HZ`].
#[inline]
pub fn hpf_coeff(hz: f32, sample_rate: f32) -> Option<f32> {
    // A little slack: exp2(log2(20)) may not be exactly 20.
    (hz > HPF_OFF_HZ * 1.0001).then(|| {
        let g = prewarp(hz, sample_rate);
        g / (1.0 + g)
    })
}

/// Pre-filter drive. At 0 dB it's an exact bypass; the saturator fades in
/// over the first 6 dB so turning drive up from zero never jumps.
#[derive(Debug, Clone, Copy)]
pub struct Drive {
    gain: f32,
    wet: f32,
}

impl Drive {
    pub fn new(db: f32) -> Self {
        let db = db.clamp(0.0, MAX_DRIVE_DB);
        Self { gain: crate::db_to_gain(db), wet: (db / 6.0).min(1.0) }
    }

    #[inline]
    pub fn process(&self, x: f32) -> f32 {
        if self.wet == 0.0 {
            x
        } else {
            x + self.wet * (sat(self.gain * x) - x)
        }
    }
}

/// Trapezoidal one-pole low-pass state. `big_g = g / (1 + g)`.
#[derive(Debug, Clone, Copy, Default)]
pub struct OnePole {
    s: f32,
}

impl OnePole {
    #[inline]
    pub fn lowpass(&mut self, x: f32, big_g: f32) -> f32 {
        let v = (x - self.s) * big_g;
        let y = v + self.s;
        self.s = y + v;
        y
    }

    #[inline]
    pub fn highpass(&mut self, x: f32, big_g: f32) -> f32 {
        x - self.lowpass(x, big_g)
    }

    pub fn reset(&mut self) {
        self.s = 0.0;
    }
}

/// 4-pole ladder low-pass (24 dB/oct), Zavalishin's TPT form, with the
/// feedback sum saturated.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ladder {
    s: [f32; 4],
}

impl Ladder {
    /// `k` = feedback (0..[`LADDER_MAX_K`]); `bass_comp` (0..1) restores the
    /// passband level the feedback takes away (a 1 / (1 + k) loss).
    #[inline]
    pub fn process(&mut self, x: f32, g: f32, k: f32, bass_comp: f32) -> f32 {
        let big_g = g / (1.0 + g);
        let b = 1.0 / (1.0 + g);
        let [s1, s2, s3, s4] = self.s;
        // Output if the cascade input were 0: each stage's stored state
        // passed through the stages after it.
        let state_out = b * (big_g * big_g * big_g * s1 + big_g * big_g * s2 + big_g * s3 + s4);
        let g4 = big_g * big_g * big_g * big_g;
        let x = x * (1.0 + bass_comp * k);
        // Solve the zero-delay loop linearly, then saturate the result.
        let u = sat((x - k * state_out) / (1.0 + k * g4));

        let mut y = u;
        for s in &mut self.s {
            let v = (y - *s) * big_g;
            y = v + *s;
            *s = y + v;
        }
        y
    }

    pub fn reset(&mut self) {
        self.s = [0.0; 4];
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvfMode {
    LowPass,
    /// Normalised: unity gain at the centre at every resonance.
    BandPass,
    HighPass,
    Notch,
}

/// Trapezoidal SVF (Simper), 12 dB/oct, linear.
#[derive(Debug, Clone, Copy, Default)]
pub struct StateVariable {
    ic1: f32,
    ic2: f32,
}

impl StateVariable {
    /// `k` = damping, 1/Q.
    #[inline]
    pub fn process(&mut self, x: f32, g: f32, k: f32, mode: SvfMode) -> f32 {
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        match mode {
            SvfMode::LowPass => v2,
            SvfMode::BandPass => k * v1,
            SvfMode::HighPass => x - k * v1 - v2,
            SvfMode::Notch => x - k * v1,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Korg35 (MS-20) filter, 12 dB/oct, low-pass or high-pass, after Will
/// Pirkle's VA Korg35 (app note AN-5), with the saturator in the loop.
#[derive(Debug, Clone, Copy, Default)]
pub struct Korg35 {
    a: f32,
    b: f32,
    c: f32,
}

impl Korg35 {
    /// `k` = feedback ([`MS_MIN_K`]..[`MS_MAX_K`]).
    #[inline]
    pub fn lowpass(&mut self, x: f32, g: f32, k: f32) -> f32 {
        let big_g = g / (1.0 + g);
        // a: input LP1, b: LP2 in the loop, c: HP1 in the feedback path.
        let y1 = lp_tick(&mut self.a, x, big_g);
        let feedback = self.b * (k - k * big_g) / (1.0 + g) - self.c / (1.0 + g);
        let alpha0 = 1.0 / (1.0 - k * big_g + k * big_g * big_g);
        let u = sat(alpha0 * (y1 + feedback));
        let y = k * lp_tick(&mut self.b, u, big_g);
        let _ = hp_tick(&mut self.c, y, big_g);
        y / k
    }

    #[inline]
    pub fn highpass(&mut self, x: f32, g: f32, k: f32) -> f32 {
        let big_g = g / (1.0 + g);
        // a: input HP1, b: HP2 in the loop, c: LP1 in the feedback path.
        let y1 = hp_tick(&mut self.a, x, big_g);
        let feedback = -self.b * big_g / (1.0 + g) + self.c / (1.0 + g);
        let alpha0 = 1.0 / (1.0 - k * big_g + k * big_g * big_g);
        let y = sat(k * (alpha0 * y1 + feedback));
        let h = hp_tick(&mut self.b, y, big_g);
        let _ = lp_tick(&mut self.c, h, big_g);
        y / k
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[inline]
fn lp_tick(s: &mut f32, x: f32, big_g: f32) -> f32 {
    let v = (x - *s) * big_g;
    let y = v + *s;
    *s = y + v;
    y
}

#[inline]
fn hp_tick(s: &mut f32, x: f32, big_g: f32) -> f32 {
    x - lp_tick(s, x, big_g)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::osc::tests::{measured_freq, tone_power, SR};
    use std::f32::consts::TAU;

    /// Steady-state gain in dB of `f` for a sine at `freq`, measured after
    /// the filter has settled. Integer-Hz tones over whole seconds.
    fn gain_db(freq: f32, amp: f32, mut f: impl FnMut(f32) -> f32) -> f64 {
        let n = SR as usize;
        let y: Vec<f32> = (0..2 * n).map(|i| f(amp * (TAU * freq * i as f32 / SR).sin())).collect();
        let out = (2.0 * tone_power(&y[n..], freq as f64)).sqrt();
        20.0 * (out / amp as f64).log10()
    }

    /// 4-pole ladder at zero resonance: four one-poles, |H| = (1 + r^2)^-2,
    /// with r the bilinear-warped frequency ratio (exact for the TPT form).
    fn ladder_expected_db(f: f64, fc: f64) -> f64 {
        let warp = |f: f64| (std::f64::consts::PI * f / SR as f64).tan();
        let r = warp(f) / warp(fc);
        -40.0 * (1.0 + r * r).log10()
    }

    const FC: f32 = 1_000.0;

    #[test]
    fn ladder_matches_the_analog_four_pole_response() {
        let g = prewarp(FC, SR);
        // Small signal so the saturator is effectively linear.
        for f in [125.0f32, 500.0, 1_000.0, 2_000.0, 4_000.0] {
            let mut l = Ladder::default();
            let db = gain_db(f, 0.01, |x| l.process(x, g, 0.0, 0.0));
            let expected = ladder_expected_db(f as f64, FC as f64);
            assert!((db - expected).abs() < 0.05, "{f} Hz: {db:.2} dB, expected {expected:.2}");
        }
    }

    #[test]
    fn ladder_resonance_costs_bass_unless_compensated() {
        let g = prewarp(FC, SR);
        let k = ladder_k(0.5);
        let mut plain = Ladder::default();
        let mut comp = Ladder::default();
        let lost = gain_db(50.0, 0.01, |x| plain.process(x, g, k, 0.0));
        let kept = gain_db(50.0, 0.01, |x| comp.process(x, g, k, 1.0));
        let expected = -20.0 * (1.0 + k as f64).log10();
        assert!((lost - expected).abs() < 0.1, "uncompensated {lost:.2} dB, expected {expected:.2}");
        assert!(kept.abs() < 0.1, "compensated {kept:.2} dB");
    }

    /// At full resonance each model rings on by itself at (about) its
    /// cutoff, at a steady, bounded level.
    #[test]
    fn full_resonance_self_oscillates_at_the_cutoff() {
        let g = prewarp(FC, SR);
        let mut ladder = Ladder::default();
        let mut ms = Korg35::default();
        let mut svf = StateVariable::default();
        type Model<'a> = (&'a str, Box<dyn FnMut(f32) -> f32 + 'a>, f64);
        let models: Vec<Model> = vec![
            ("ladder", Box::new(|x| ladder.process(x, g, LADDER_MAX_K, 0.0)), 0.02),
            ("ms", Box::new(|x| ms.lowpass(x, g, MS_MAX_K)), 0.05),
        ];
        for (name, mut f, tol) in models {
            // Kick it with a short burst, then let it run with no input.
            let y: Vec<f32> = (0..SR as usize * 2).map(|i| f(if i < 10 { 1.0 } else { 0.0 })).collect();
            let tail = &y[SR as usize..];
            let freq = measured_freq(tail);
            assert!((freq / FC as f64 - 1.0).abs() < tol, "{name}: rings at {freq:.1} Hz");
            let peak = |s: &[f32]| s.iter().fold(0f32, |m, v| m.max(v.abs()));
            let (early, late) = (peak(&tail[..4_800]), peak(&tail[tail.len() - 4_800..]));
            assert!(late > 0.1 && late < 4.0, "{name}: level {late}");
            assert!((late / early - 1.0).abs() < 0.05, "{name}: not steady ({early} -> {late})");
        }
        // The linear SVF at maximum Q rings for a long time but doesn't grow.
        let y: Vec<f32> = (0..SR as usize).map(|i| svf.process(if i < 10 { 1.0 } else { 0.0 }, g, svf_k(1.0), SvfMode::BandPass)).collect();
        assert!((measured_freq(&y[1_000..]) / FC as f64 - 1.0).abs() < 0.01);
        assert!(y.iter().all(|v| v.abs() < 1.0));
    }

    /// SVF responses at the cutoff with Q = 0.5: LP and HP are at -6 dB (|H| = Q),
    /// BP at 0 dB (normalised), notch at -infinity.
    #[test]
    fn svf_modes_at_the_cutoff() {
        let g = prewarp(FC, SR);
        let k = svf_k(0.0);
        assert_eq!(k, 2.0);
        for (mode, expected) in [
            (SvfMode::LowPass, -6.02),
            (SvfMode::HighPass, -6.02),
            (SvfMode::BandPass, 0.0),
        ] {
            let mut f = StateVariable::default();
            let db = gain_db(FC, 1.0, |x| f.process(x, g, k, mode));
            assert!((db - expected).abs() < 0.05, "{mode:?}: {db:.2} dB");
        }
        let mut f = StateVariable::default();
        assert!(gain_db(FC, 1.0, |x| f.process(x, g, k, SvfMode::Notch)) < -60.0);
        // An octave either side the LP/HP slopes are 12 dB/oct asymptotically.
        let mut lp = StateVariable::default();
        let mut hp = StateVariable::default();
        let lp_db = gain_db(8_000.0, 1.0, |x| lp.process(x, prewarp(500.0, SR), k, SvfMode::LowPass));
        let hp_db = gain_db(62.0, 1.0, |x| hp.process(x, prewarp(1_000.0, SR), k, SvfMode::HighPass));
        assert!((lp_db - -48.0).abs() < 2.0, "LP 4 oct above: {lp_db:.1}");
        assert!((hp_db - -48.0).abs() < 2.0, "HP 4 oct below: {hp_db:.1}");
    }

    /// With almost no feedback the MS low-pass is two one-poles (-6 dB at
    /// the cutoff, 12 dB/oct beyond) and the MS high-pass is one (-3 dB,
    /// 6 dB/oct), as on the MS-20.
    #[test]
    fn ms_filter_slopes_at_low_resonance() {
        let g = prewarp(FC, SR);
        let analog = |f: f64, hp: bool| {
            let (r, poles) = if hp { (FC as f64 / f, 1.0) } else { (f / FC as f64, 2.0) };
            -10.0 * (1.0 + r * r).log10() * poles
        };
        for f in [250.0f32, 1_000.0, 4_000.0] {
            let mut lp = Korg35::default();
            let mut hp = Korg35::default();
            let lp_db = gain_db(f, 0.01, |x| lp.lowpass(x, g, MS_MIN_K));
            let hp_db = gain_db(f, 0.01, |x| hp.highpass(x, g, MS_MIN_K));
            assert!((lp_db - analog(f as f64, false)).abs() < 0.5, "LP {f} Hz: {lp_db:.2}");
            assert!((hp_db - analog(f as f64, true)).abs() < 0.5, "HP {f} Hz: {hp_db:.2}");
        }
    }

    #[test]
    fn fast_tan_matches_std_over_the_cutoff_range() {
        let n = 100_000;
        let worst = (0..=n)
            .map(|i| {
                let x = MAX_CUTOFF_RATIO * PI * i as f32 / n as f32;
                let exact = (x as f64).tan();
                if exact == 0.0 { 0.0 } else { ((fast_tan(x) as f64 - exact) / exact).abs() }
            })
            .fold(0f64, f64::max);
        assert!(worst < 2e-6, "worst relative error {worst:e}");
    }

    #[test]
    fn one_pole_highpass_is_3db_down_at_cutoff() {
        let g = prewarp(FC, SR);
        let mut f = OnePole::default();
        let db = gain_db(FC, 1.0, |x| f.highpass(x, g / (1.0 + g)));
        assert!((db + 3.01).abs() < 0.02, "{db:.2} dB");
    }

    #[test]
    fn drive_bypasses_at_zero_and_is_bounded() {
        let off = Drive::new(0.0);
        assert_eq!(off.process(0.7), 0.7);
        assert_eq!(off.process(-3.0), -3.0);
        let full = Drive::new(MAX_DRIVE_DB);
        assert!((0..1000).map(|i| full.process(i as f32 * 0.01 - 5.0)).all(|y| y.abs() <= 2.0));
        // Continuous as drive rises from 0.
        assert!((Drive::new(0.01).process(1.0) - 1.0).abs() < 0.01);
    }

    /// Cutoff jumping anywhere every sample, full resonance, loud input:
    /// nothing blows up.
    #[test]
    fn stable_under_wild_modulation() {
        let mut rng = crate::synth::noise::Rng::new(7);
        let mut ladder = Ladder::default();
        let mut svf = StateVariable::default();
        let mut ms = Korg35::default();
        let mut msh = Korg35::default();
        for i in 0..SR as usize {
            let fc = 20.0 * 1_000f32.powf(rng.next_f32());
            let g = prewarp(fc, SR);
            let x = 4.0 * if (i / 50) % 2 == 0 { 1.0 } else { -1.0 };
            let outs = [
                ladder.process(x, g, LADDER_MAX_K, 1.0),
                svf.process(x, g, svf_k(1.0), SvfMode::LowPass),
                ms.lowpass(x, g, MS_MAX_K),
                msh.highpass(x, g, MS_MAX_K),
            ];
            for (m, y) in outs.iter().enumerate() {
                assert!(y.is_finite() && y.abs() < 1_000.0, "model {m} sample {i}: {y}");
            }
        }
    }
}
