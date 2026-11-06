//! Direct-form biquad coefficients, used as the common currency between
//! filter *design* ([`crate::matched`]) and filter *processing* ([`crate::Svf`]).

use crate::svf::SvfCoefs;

/// `H(z) = (b0 + b1 z⁻¹ + b2 z⁻²) / (1 + a1 z⁻¹ + a2 z⁻²)`, in f64 for design accuracy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Biquad {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl Biquad {
    pub const IDENTITY: Self = Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 };

    /// Linear magnitude at `w` radians/sample (0 = DC, π = Nyquist).
    pub fn magnitude(&self, w: f64) -> f64 {
        (eval_sq(self.b0, self.b1, self.b2, w) / eval_sq(1.0, self.a1, self.a2, w)).sqrt()
    }

    /// `1/H(z)`. Only valid when the numerator is minimum-phase (zeros inside
    /// the unit circle), which [`crate::matched`] guarantees.
    pub fn inverted(&self) -> Self {
        let n = self.b0;
        Self { b0: 1.0 / n, b1: self.a1 / n, b2: self.a2 / n, a1: self.b1 / n, a2: self.b2 / n }
    }

    /// Both poles strictly inside the unit circle.
    pub fn is_stable(&self) -> bool {
        // Jury criterion for 1 + a1 z⁻¹ + a2 z⁻².
        self.a2.abs() < 1.0 && self.a1.abs() < 1.0 + self.a2
    }

    /// Convert to the SVF topology so the section can be modulated without
    /// clicks. Exact: the SVF has the same transfer function.
    ///
    /// The trapezoidal SVF's denominator is
    /// `(1+gk+g²) + 2(g²-1) z⁻¹ + (1-gk+g²) z⁻²`, which gives `g` and `k`
    /// from `a1, a2`; the three numerator equations then give `m0, m1, m2`.
    pub fn to_svf(&self) -> SvfCoefs {
        let Self { b0, b1, b2, a1, a2 } = *self;
        let p = 1.0 + a1 + a2; // = 4g²/D
        let m = 1.0 - a1 + a2; // = 4/D
        let g = (p / m).sqrt();
        let d = 4.0 / m;
        let k = (1.0 - a2) * d / (2.0 * g);

        let m0 = (b0 - b1 + b2) / m;
        let m2 = (d * b1 - m0 * (2.0 * g * g - 2.0)) / (2.0 * g * g);
        let m1 = d * (b0 - b2) / (2.0 * g) - m0 * k;
        SvfCoefs::from_parts(g, k, m0, m1, m2)
    }
}

/// `|c0 + c1 e^{-jw} + c2 e^{-2jw}|²`
fn eval_sq(c0: f64, c1: f64, c2: f64, w: f64) -> f64 {
    let re = c0 + c1 * w.cos() + c2 * (2.0 * w).cos();
    let im = -(c1 * w.sin() + c2 * (2.0 * w).sin());
    re * re + im * im
}

pub(crate) fn denominator_sq(a1: f64, a2: f64, w: f64) -> f64 {
    eval_sq(1.0, a1, a2, w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Svf;

    /// Reference direct-form-I implementation.
    fn run_df1(bq: &Biquad, input: &[f64]) -> Vec<f64> {
        let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
        input
            .iter()
            .map(|&x| {
                let y = bq.b0 * x + bq.b1 * x1 + bq.b2 * x2 - bq.a1 * y1 - bq.a2 * y2;
                (x2, x1, y2, y1) = (x1, x, y1, y);
                y
            })
            .collect()
    }

    fn noise(n: usize) -> Vec<f64> {
        let mut s: u32 = 0x1234_5678;
        (0..n)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                (s as f64 / u32::MAX as f64) * 2.0 - 1.0
            })
            .collect()
    }

    #[test]
    fn svf_conversion_matches_direct_form() {
        let cases = [
            // Arbitrary stable sections, incl. a first-order one (a2 = 0)
            // and a very low corner (small g).
            Biquad { b0: 1.2, b1: -1.7, b2: 0.62, a1: -1.6, a2: 0.68 },
            Biquad { b0: 0.3, b1: 0.6, b2: 0.3, a1: -0.4, a2: 0.2 },
            Biquad { b0: 0.9, b1: -0.85, b2: 0.0, a1: -0.8, a2: 0.0 },
            Biquad { b0: 1.0, b1: -1.9993, b2: 0.99935, a1: -1.99935, a2: 0.99936 },
        ];
        let input = noise(4096);
        for bq in cases {
            assert!(bq.is_stable(), "{bq:?}");
            let svf = bq.to_svf();
            let mut st = Svf::default();
            let reference = run_df1(&bq, &input);
            for (i, (&x, &r)) in input.iter().zip(&reference).enumerate() {
                let y = st.process(&svf, x as f32) as f64;
                assert!((y - r).abs() < 2e-3 * (1.0 + r.abs()), "{bq:?} sample {i}: svf {y} vs df1 {r}");
            }
        }
    }

    #[test]
    fn inversion_round_trips() {
        let bq = Biquad { b0: 1.2, b1: -1.7, b2: 0.62, a1: -1.6, a2: 0.68 };
        let w = 0.3;
        assert!((bq.magnitude(w) * bq.inverted().magnitude(w) - 1.0).abs() < 1e-12);
    }
}
