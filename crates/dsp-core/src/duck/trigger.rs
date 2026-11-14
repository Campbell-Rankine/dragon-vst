//! Triggered ducking (MIDI mode, and Audio mode later): each trigger
//! restarts the curve from wherever it is, so retriggers never jump.
//!
//! A trigger can be scaled (velocity): the duck amount is `scale * d`, with
//! `d` the curve. A retrigger starts its attack from the current *scaled*
//! amount, so a soft hit landing on a deep dip glides up to its own,
//! shallower floor rather than jumping.

use super::curve::DuckShape;

/// The time since the last trigger, and where its attack started.
#[derive(Debug, Clone, Copy)]
pub struct TriggeredDuck {
    /// Seconds since the last trigger; infinite when never triggered.
    since: f32,
    /// The curve's starting amount for the current trigger (in units of
    /// `scale`, so it can exceed 1 when a soft hit follows a hard one).
    d_start: f32,
    /// The current trigger's depth scale, 0..=1.
    scale: f32,
}

/// Triggers scaled below this do nothing (a fully velocity-scaled note at
/// velocity 0).
const MIN_SCALE: f32 = 1e-4;
/// `since` stops growing here (s), far past any curve, so it never loses
/// precision.
const SINCE_CAP: f32 = 1.0e4;

impl Default for TriggeredDuck {
    fn default() -> Self {
        Self { since: f32::INFINITY, d_start: 0.0, scale: 1.0 }
    }
}

impl TriggeredDuck {
    /// The duck amount now (0 = untouched, 1 = fully ducked).
    #[inline]
    pub fn amount(&self, shape: &DuckShape) -> f32 {
        if self.since.is_finite() {
            self.scale * shape.duck_at(self.since, self.d_start)
        } else {
            0.0
        }
    }

    /// Start the curve again now, with depth scaled by `scale` (0..=1).
    pub fn trigger(&mut self, shape: &DuckShape, scale: f32) {
        let scale = scale.clamp(0.0, 1.0);
        if scale < MIN_SCALE {
            return;
        }
        let now = self.amount(shape);
        self.d_start = now / scale;
        self.scale = scale;
        self.since = 0.0;
    }

    /// The duck amount for this sample, then advance by `dt` seconds.
    #[inline]
    pub fn next(&mut self, shape: &DuckShape, dt: f32) -> f32 {
        let d = self.amount(shape);
        if self.since.is_finite() {
            self.since = (self.since + dt).min(SINCE_CAP);
        }
        d
    }

    /// Seconds since the last trigger (`None` if never triggered).
    pub fn since(&self) -> Option<f32> {
        self.since.is_finite().then_some(self.since)
    }

    /// Forget any trigger: back to untouched.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn shape() -> DuckShape {
        DuckShape { depth: 1.0, attack: 0.001, hold: 0.01, release: 0.1, tension: 0.4 }.sanitized()
    }

    /// Run `n` samples, triggering (with scale) at the given sample indices.
    fn run(n: usize, triggers: &[(usize, f32)]) -> Vec<f32> {
        let s = shape();
        let mut t = TriggeredDuck::default();
        (0..n)
            .map(|i| {
                for &(at, scale) in triggers {
                    if at == i {
                        t.trigger(&s, scale);
                    }
                }
                t.next(&s, 1.0 / SR)
            })
            .collect()
    }

    #[test]
    fn untouched_until_triggered() {
        assert!(run(1000, &[]).iter().all(|&d| d == 0.0));
        assert_eq!(TriggeredDuck::default().since(), None);
    }

    #[test]
    fn a_trigger_starts_the_dip_at_that_sample() {
        let d = run(10_000, &[(500, 1.0)]);
        assert!(d[..500].iter().all(|&x| x == 0.0));
        assert_eq!(d[500], 0.0, "the attack starts from zero");
        assert!(d[501] > 0.0);
        // Attack 1 ms = 48 samples, then the hold at full depth.
        assert!((d[548] - 1.0).abs() < 1e-3);
        assert_eq!(d[700], 1.0);
        // Fully recovered after attack + hold + release (111 ms).
        assert_eq!(d[500 + 5400], 0.0);
    }

    #[test]
    fn velocity_scales_the_depth() {
        let d = run(2000, &[(0, 0.5)]);
        assert_eq!(d[300], 0.5, "in the hold");
        // Scale 0 does nothing at all.
        assert!(run(2000, &[(0, 0.0)]).iter().all(|&x| x == 0.0));
    }

    #[test]
    fn retriggers_never_jump() {
        // Hard hit, then a soft one mid-hold, then a hard one mid-release,
        // then a rapid roll.
        let mut triggers = vec![(0, 1.0), (300, 0.3), (2000, 1.0)];
        triggers.extend((3000..4000).step_by(100).map(|i| (i, 0.8)));
        let d = run(8000, &triggers);
        let worst = d.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
        // The attack ramp moves at most 1/48 per sample (1 ms at 48 kHz);
        // a retrigger may only add its own attack's slope.
        assert!(worst <= 1.0 / 48.0 + 1e-4, "step {worst}");
        // The soft hit glides up to its own floor of 0.3.
        assert!((d[600] - 0.3).abs() < 1e-6, "{}", d[600]);
    }

    #[test]
    fn reset_forgets_the_trigger() {
        let s = shape();
        let mut t = TriggeredDuck::default();
        t.trigger(&s, 1.0);
        t.next(&s, 0.005);
        assert!(t.amount(&s) > 0.0);
        t.reset();
        assert_eq!(t.amount(&s), 0.0);
        assert_eq!(t.since(), None);
    }
}
