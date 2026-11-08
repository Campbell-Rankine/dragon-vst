//! Two-sample polynomial band-limiting corrections (PolyBLEP / PolyBLAMP).
//!
//! A naive sawtooth or square jumps instantly, which has infinite bandwidth
//! and folds back (aliases) as inharmonic tones. Each discontinuity gets a
//! small correction on the sample just before it and the sample just after
//! it, approximating a band-limited version at almost no cost.
//!
//! The oscillators run one sample late so the "before" sample can still be
//! corrected when a discontinuity can't be predicted (a hard-sync reset).
//! Both functions take `d`, the time since the discontinuity at the *after*
//! sample, as a fraction of a sample (`0..1`). The before sample is `1 - d`
//! samples ahead of it. They return `(before, after)` corrections to add.

/// Corrections for an upward step of 1. Scale by the step height.
///
/// The residual is `(1 - |x|)^2 / 2` before the step and `-(1 - |x|)^2 / 2`
/// after it, at a distance of `x` samples.
#[inline]
pub fn blep(d: f32) -> (f32, f32) {
    let e = 1.0 - d;
    (0.5 * d * d, -0.5 * e * e)
}

/// Corrections for a slope change of +1 per sample (a corner bending
/// upward). Scale by the slope change. This is the integral of [`blep`]:
/// `(1 - |x|)^3 / 6` on both sides.
#[inline]
pub fn blamp(d: f32) -> (f32, f32) {
    let e = 1.0 - d;
    (d * d * d * (1.0 / 6.0), e * e * e * (1.0 / 6.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Averaged over every position of the discontinuity within a sample,
    /// the two corrections sum to the integral of the continuous residual:
    /// 0 for BLEP (no DC added), 1/12 for BLAMP.
    #[test]
    fn corrections_average_to_the_residual_integral() {
        let n = 100_000;
        let (mut step, mut ramp) = (0.0f64, 0.0f64);
        for i in 0..n {
            let d = (i as f32 + 0.5) / n as f32;
            let (b, a) = blep(d);
            step += (b + a) as f64;
            let (b, a) = blamp(d);
            ramp += (b + a) as f64;
        }
        assert!((step / n as f64).abs() < 1e-6);
        assert!((ramp / n as f64 - 1.0 / 12.0).abs() < 1e-6);
    }

    #[test]
    fn step_lands_on_the_midpoint() {
        // A step exactly on the after sample: that sample is halfway up.
        assert_eq!(blep(0.0), (0.0, -0.5));
        // A step exactly on the before sample: that one is halfway.
        assert_eq!(blep(1.0), (0.5, 0.0));
    }
}
