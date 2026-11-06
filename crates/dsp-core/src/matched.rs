//! Magnitude-matched digital design of second-order analog prototypes, after
//! Martin Vicanek, "Matched Second Order Digital Filters" (2016).
//!
//! Poles come from impulse invariance (exact pole positions, no frequency
//! warping). The numerator is then chosen so the digital magnitude equals the
//! analog magnitude at DC, at Nyquist, and at a third frequency (the design
//! frequency). Unlike the bilinear transform, the response doesn't "cramp"
//! towards Nyquist.

use crate::biquad::{denominator_sq, Biquad};
use std::f64::consts::PI;

/// Analog second-order prototype with `s` normalised to the design frequency:
/// `H(s) = (n[2] s² + n[1] s + n[0]) / (d[2] s² + d[1] s + d[0])`.
/// A first-order denominator has `d[2] = 0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Analog2 {
    pub n: [f64; 3],
    pub d: [f64; 3],
}

impl Analog2 {
    /// `|H(jΩ)|²` at normalised frequency `omega` = f / f_design.
    pub fn magnitude_sq(&self, omega: f64) -> f64 {
        let w2 = omega * omega;
        let (nr, ni) = (self.n[0] - self.n[2] * w2, self.n[1] * omega);
        let (dr, di) = (self.d[0] - self.d[2] * w2, self.d[1] * omega);
        (nr * nr + ni * ni) / (dr * dr + di * di)
    }

    pub fn swapped(&self) -> Self {
        Self { n: self.d, d: self.n }
    }

    /// Natural frequency of the poles (normalised).
    pub fn pole_freq(&self) -> f64 {
        (self.d[0] / self.d[2]).sqrt()
    }

    /// Natural frequency of the zeros (normalised).
    pub fn zero_freq(&self) -> f64 {
        (self.n[0] / self.n[2]).sqrt()
    }
}

/// Highest frequency (radians/sample) used as the third match point. Very
/// close to Nyquist the match equations become ill-conditioned.
const MAX_MATCH_W: f64 = 0.9 * PI;

/// Design a digital biquad matching `proto` at design frequency `f0` Hz.
pub fn matched(proto: &Analog2, f0: f64, sample_rate: f64) -> Biquad {
    let w0 = 2.0 * PI * f0 / sample_rate;

    // Poles by impulse invariance.
    let (a1, a2) = if proto.d[2] == 0.0 {
        let pole = -proto.d[0] / proto.d[1]; // normalised, negative
        (-(pole * w0).exp(), 0.0)
    } else {
        let wp = proto.pole_freq() * w0;
        let zeta = proto.d[1] / (2.0 * (proto.d[0] * proto.d[2]).sqrt());
        let decay = (-zeta * wp).exp();
        let a1 = if zeta <= 1.0 {
            -2.0 * decay * (wp * (1.0 - zeta * zeta).sqrt()).cos()
        } else {
            -2.0 * decay * (wp * (zeta * zeta - 1.0).sqrt()).cosh()
        };
        (a1, decay * decay)
    };

    // Analog magnitude² at DC, Nyquist and the third match point.
    let h_dc = proto.magnitude_sq(0.0);
    let h_ny = proto.magnitude_sq(PI / w0);
    let wm = w0.min(MAX_MATCH_W);
    let h_m = proto.magnitude_sq(wm / w0);

    // Vicanek's "phi" basis: |B(w)|² = B0 φ0 + B1 φ1 + B2 φ2.
    let s = (wm / 2.0).sin().powi(2);
    let (phi0, phi1) = (1.0 - s, s);
    let phi2 = 4.0 * phi0 * phi1;

    let big_a0 = (1.0 + a1 + a2).powi(2);
    let big_a1 = (1.0 - a1 + a2).powi(2);
    let big_b0 = big_a0 * h_dc;
    let big_b1 = big_a1 * h_ny;
    let big_b2 = (h_m * denominator_sq(a1, a2, wm) - big_b0 * phi0 - big_b1 * phi1) / phi2;

    let (sb0, sb1) = (big_b0.sqrt(), big_b1.sqrt());
    let w = 0.5 * (sb0 + sb1);
    let b0 = 0.5 * (w + (w * w + big_b2).max(0.0).sqrt());
    let b1 = 0.5 * (sb0 - sb1);
    let b2 = if b0 > 0.0 { -big_b2 / (4.0 * b0) } else { 0.0 };

    Biquad { b0, b1, b2, a1, a2 }
}

/// Bilinear transform of `proto`, pre-warped so `f0` maps exactly. Used for
/// cut filters (see `eq.rs` for why). First-order prototypes (`d[2] = 0`)
/// give a first-order section (`a2 = b2 = 0`).
pub fn bilinear(proto: &Analog2, f0: f64, sample_rate: f64) -> Biquad {
    let k = (PI * f0 / sample_rate).tan();
    let ([n0, n1, n2], [d0, d1, d2]) = (proto.n, proto.d);
    if d2 == 0.0 {
        // s = (1/K)(1 - z⁻¹)/(1 + z⁻¹), multiplied through by K(1 + z⁻¹).
        let a0 = d1 + d0 * k;
        return Biquad {
            b0: (n1 + n0 * k) / a0,
            b1: (n0 * k - n1) / a0,
            b2: 0.0,
            a1: (d0 * k - d1) / a0,
            a2: 0.0,
        };
    }
    // Multiplied through by K²(1 + z⁻¹)².
    let kk = k * k;
    let a0 = d2 + d1 * k + d0 * kk;
    Biquad {
        b0: (n2 + n1 * k + n0 * kk) / a0,
        b1: (2.0 * n0 * kk - 2.0 * n2) / a0,
        b2: (n2 - n1 * k + n0 * kk) / a0,
        a1: (2.0 * d0 * kk - 2.0 * d2) / a0,
        a2: (d2 - d1 * k + d0 * kk) / a0,
    }
}

/// All-pass sections have flat magnitude, so magnitude matching can't find
/// their numerator. Use the mirrored denominator instead.
pub fn matched_allpass(proto: &Analog2, f0: f64, sample_rate: f64) -> Biquad {
    let p = matched(proto, f0, sample_rate);
    Biquad { b0: p.a2, b1: p.a1, b2: 1.0, a1: p.a1, a2: p.a2 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db(x: f64) -> f64 {
        20.0 * x.log10()
    }

    #[test]
    fn matches_exactly_at_the_three_points() {
        let fs = 48_000.0;
        let f0 = 3_000.0;
        // Bell, +9 dB, Q 2.
        let a = 10f64.powf(9.0 / 40.0);
        let q = 2.0;
        let proto = Analog2 { n: [1.0, a / q, 1.0], d: [1.0, 1.0 / (a * q), 1.0] };
        let bq = matched(&proto, f0, fs);
        let w0 = 2.0 * PI * f0 / fs;
        for (w, omega) in [(1e-9, 0.0), (PI, PI / w0), (w0, 1.0)] {
            let digital = db(bq.magnitude(w));
            let analog = 10.0 * proto.magnitude_sq(omega).log10();
            assert!((digital - analog).abs() < 1e-6, "w={w}: {digital} vs {analog}");
        }
    }

    #[test]
    fn allpass_is_flat() {
        let proto = Analog2 { n: [1.0, -2.0, 1.0], d: [1.0, 2.0, 1.0] };
        let bq = matched_allpass(&proto, 1_000.0, 48_000.0);
        for w in [0.01, 0.1, 1.0, 3.0] {
            assert!((bq.magnitude(w) - 1.0).abs() < 1e-9);
        }
    }
}
