//! Trapezoidal-integrated state-variable filter (Andrew Simper / Cytomic,
//! "SvfLinearTrapOptimised2"). Stable under fast coefficient changes, which
//! matters once bands are automated or driven dynamically.
//!
//! This module is only the filter *topology*. Coefficients are designed in
//! [`crate::matched`] and converted with [`crate::Biquad::to_svf`].

/// Coefficients for one second-order section in SVF form. Small and `Copy`;
/// one set can be shared by every channel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SvfCoefs {
    a1: f32,
    a2: f32,
    a3: f32,
    m0: f32,
    m1: f32,
    m2: f32,
}

impl SvfCoefs {
    /// A transparent (unity-gain) section.
    pub const IDENTITY: Self = Self { a1: 1.0, a2: 0.0, a3: 0.0, m0: 1.0, m1: 0.0, m2: 0.0 };

    /// Build from the SVF's natural parameters: `g` = pre-warped integrator
    /// gain, `k` = damping (1/Q), and the mix of input (`m0`), band-pass
    /// (`m1`) and low-pass (`m2`) outputs.
    pub(crate) fn from_parts(g: f64, k: f64, m0: f64, m1: f64, m2: f64) -> Self {
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        Self {
            a1: a1 as f32,
            a2: a2 as f32,
            a3: a3 as f32,
            m0: m0 as f32,
            m1: m1 as f32,
            m2: m2 as f32,
        }
    }

    /// A 2-pole band-pass at `freq` Hz with quality `q`, unity gain at its
    /// centre (constant peak gain), for detectors rather than EQ.
    pub fn band_pass(freq: f32, q: f32, sample_rate: f32) -> Self {
        let freq = freq.clamp(1.0, 0.49 * sample_rate) as f64;
        let g = (std::f64::consts::PI * freq / sample_rate as f64).tan();
        let k = 1.0 / q.max(0.05) as f64;
        Self::from_parts(g, k, 0.0, k, 0.0)
    }
}

/// Per-channel state for one section.
#[derive(Debug, Clone, Copy, Default)]
pub struct Svf {
    ic1eq: f32,
    ic2eq: f32,
}

impl Svf {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    #[inline]
    pub fn process(&mut self, c: &SvfCoefs, v0: f32) -> f32 {
        let v3 = v0 - self.ic2eq;
        let v1 = c.a1 * self.ic1eq + c.a2 * v3;
        let v2 = self.ic2eq + c.a2 * self.ic1eq + c.a3 * v3;
        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;
        c.m0 * v0 + c.m1 * v1 + c.m2 * v2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_passes_through() {
        let mut f = Svf::default();
        for x in [1.0f32, -0.5, 0.25, 0.0, 0.75] {
            assert_eq!(f.process(&SvfCoefs::IDENTITY, x), x);
        }
    }

    /// Steady-state peak of a sine through the filter.
    fn sine_gain(c: &SvfCoefs, freq: f32, sr: f32) -> f32 {
        let mut f = Svf::default();
        let n = (sr * 0.5) as usize;
        let mut peak = 0.0f32;
        for i in 0..n {
            let y = f.process(c, (2.0 * std::f32::consts::PI * freq * i as f32 / sr).sin());
            if i > n / 2 {
                peak = peak.max(y.abs());
            }
        }
        peak
    }

    #[test]
    fn band_pass_is_unity_at_the_centre_and_cuts_either_side() {
        let sr = 48_000.0;
        let c = SvfCoefs::band_pass(80.0, 1.0, sr);
        assert!((sine_gain(&c, 80.0, sr) - 1.0).abs() < 0.01);
        // Two octaves either side: Q 1 band-pass is about -12 dB there.
        assert!(sine_gain(&c, 20.0, sr) < 0.3);
        assert!(sine_gain(&c, 320.0, sr) < 0.3);
        assert!(sine_gain(&c, 1000.0, sr) < 0.1);
    }
}
