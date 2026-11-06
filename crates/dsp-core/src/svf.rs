//! Trapezoidal-integrated state-variable filter (Andrew Simper / Cytomic,
//! "SvfLinearTrapOptimised2"). Stable under fast coefficient changes, which
//! matters once bands are automated or driven dynamically.

use std::f32::consts::PI;

/// Filter response shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvfShape {
    Bell,
    LowShelf,
    HighShelf,
}

/// Mixing coefficients for one filter setting. Cheap to copy; compute with
/// [`SvfCoefs::new`] whenever frequency, gain, or Q change.
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
    /// A transparent (unity-gain) filter.
    pub const IDENTITY: Self = Self { a1: 1.0, a2: 0.0, a3: 0.0, m0: 1.0, m1: 0.0, m2: 0.0 };

    /// `freq` in Hz (clamped below Nyquist), `gain_db` boost/cut, `q` > 0.
    pub fn new(shape: SvfShape, sample_rate: f32, freq: f32, gain_db: f32, q: f32) -> Self {
        let freq = freq.clamp(1.0, sample_rate * 0.49);
        let q = q.max(0.01);
        let a = 10f32.powf(gain_db / 40.0);
        let g = (PI * freq / sample_rate).tan();

        let (g, k, m0, m1, m2) = match shape {
            SvfShape::Bell => {
                let k = 1.0 / (q * a);
                (g, k, 1.0, k * (a * a - 1.0), 0.0)
            }
            SvfShape::LowShelf => {
                let k = 1.0 / q;
                (g / a.sqrt(), k, 1.0, k * (a - 1.0), a * a - 1.0)
            }
            SvfShape::HighShelf => {
                let k = 1.0 / q;
                (g * a.sqrt(), k, a * a, k * (1.0 - a) * a, 1.0 - a * a)
            }
        };

        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        Self { a1, a2, a3, m0, m1, m2 }
    }
}

/// Per-channel filter state.
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
    use crate::gain_to_db;

    const SR: f32 = 48_000.0;

    /// Steady-state gain in dB of a sine at `freq` through the filter.
    fn measure_db(coefs: SvfCoefs, freq: f32) -> f32 {
        let mut f = Svf::default();
        let n = (SR as usize) / 2;
        let mut peak = 0f32;
        for i in 0..n {
            let x = (2.0 * PI * freq * i as f32 / SR).sin();
            let y = f.process(&coefs, x);
            if i > n / 2 {
                peak = peak.max(y.abs());
            }
        }
        gain_to_db(peak)
    }

    fn assert_db(actual: f32, expected: f32, tol: f32, what: &str) {
        assert!((actual - expected).abs() < tol, "{what}: expected {expected} dB, got {actual} dB");
    }

    #[test]
    fn zero_gain_is_transparent() {
        for shape in [SvfShape::Bell, SvfShape::LowShelf, SvfShape::HighShelf] {
            let c = SvfCoefs::new(shape, SR, 1000.0, 0.0, 0.707);
            for f in [50.0, 1000.0, 10_000.0] {
                assert_db(measure_db(c, f), 0.0, 0.05, &format!("{shape:?} @ {f} Hz"));
            }
        }
    }

    #[test]
    fn bell_boosts_at_centre_only() {
        let c = SvfCoefs::new(SvfShape::Bell, SR, 1000.0, 6.0, 1.0);
        assert_db(measure_db(c, 1000.0), 6.0, 0.1, "centre");
        assert_db(measure_db(c, 30.0), 0.0, 0.2, "far below");
        assert_db(measure_db(c, 15_000.0), 0.0, 0.3, "far above");
    }

    #[test]
    fn bell_cut_is_symmetric() {
        let c = SvfCoefs::new(SvfShape::Bell, SR, 2000.0, -9.0, 2.0);
        assert_db(measure_db(c, 2000.0), -9.0, 0.1, "centre cut");
    }

    #[test]
    fn low_shelf_shapes_lows() {
        let c = SvfCoefs::new(SvfShape::LowShelf, SR, 200.0, 6.0, 0.707);
        assert_db(measure_db(c, 20.0), 6.0, 0.2, "below shelf");
        assert_db(measure_db(c, 200.0), 3.0, 0.2, "corner is half gain");
        assert_db(measure_db(c, 10_000.0), 0.0, 0.1, "above shelf");
    }

    #[test]
    fn high_shelf_shapes_highs() {
        let c = SvfCoefs::new(SvfShape::HighShelf, SR, 5000.0, -6.0, 0.707);
        assert_db(measure_db(c, 50.0), 0.0, 0.1, "below shelf");
        assert_db(measure_db(c, 5000.0), -3.0, 0.2, "corner is half gain");
        assert_db(measure_db(c, 20_000.0), -6.0, 0.4, "above shelf");
    }
}
