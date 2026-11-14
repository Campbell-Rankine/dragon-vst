//! The ducking curve: how far the gain is pulled down, as a function of the
//! time since a trigger.
//!
//! The curve is described as a *duck amount* `d` (0 = untouched, 1 = fully
//! ducked); the gain is `1 - depth * d`. After a trigger:
//!
//! ```text
//! d
//! 1 ┤   ╱▔▔▔▔▔▔╲
//!   │  ╱        ╲            Attack: d_start -> 1, linear
//!   │ ╱          ╲           Hold:   1
//!   │╱             ╲_        Release: 1 -> 0, bent by Tension
//! 0 ┼────┬──────┬─────────▶ t
//!     attack  hold  release
//! ```
//!
//! The attack starts from wherever the curve was (`d_start`), so a
//! retrigger mid-curve never jumps. The release is
//! `1 - f(x)`, `x` = progress 0..1, with
//! `f(x) = (e^(a x) - 1) / (e^a - 1)` and `a = TENSION_RANGE * tension`:
//! tension > 0 recovers slowly then fast (the classic pump), < 0 fast then
//! slow, 0 linear. Unlike a power curve its slope is finite everywhere, so
//! no tension setting can click.

/// `a` at tension ±1.
pub const TENSION_RANGE: f32 = 6.0;

/// Shortest attack (s): a ramp of at least this long keeps the dip
/// click-free.
pub const MIN_ATTACK: f32 = 0.000_1;

/// A curve's settings, in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DuckShape {
    /// How far the gain dips: 0..=1 (1 = to silence).
    pub depth: f32,
    /// Fade into the dip (s), at least [`MIN_ATTACK`].
    pub attack: f32,
    /// Time held at the floor (s).
    pub hold: f32,
    /// Time to recover (s).
    pub release: f32,
    /// Recovery bend, -1..=1.
    pub tension: f32,
}

impl Default for DuckShape {
    /// The "Pump" shape at 120 BPM, 1/4 notes (a 0.5 s cycle).
    fn default() -> Self {
        Self { depth: 1.0, attack: 0.001, hold: 0.0, release: 0.3, tension: 0.4 }
    }
}

impl DuckShape {
    /// The shape with every value clamped to its range.
    pub fn sanitized(&self) -> Self {
        Self {
            depth: self.depth.clamp(0.0, 1.0),
            attack: self.attack.max(MIN_ATTACK),
            hold: self.hold.max(0.0),
            release: self.release.max(0.0),
            tension: self.tension.clamp(-1.0, 1.0),
        }
    }

    /// Total length (s) from trigger to fully recovered.
    pub fn length(&self) -> f32 {
        self.attack + self.hold + self.release
    }

    /// The duck amount `t` seconds after a trigger that started at
    /// `d_start`. Expects a [`sanitized`](Self::sanitized) shape.
    #[inline]
    pub fn duck_at(&self, t: f32, d_start: f32) -> f32 {
        if t < 0.0 {
            return d_start;
        }
        if t < self.attack {
            return d_start + (1.0 - d_start) * (t / self.attack);
        }
        let t = t - self.attack;
        if t < self.hold {
            return 1.0;
        }
        let t = t - self.hold;
        if t >= self.release {
            return 0.0;
        }
        1.0 - release_shape(t / self.release, self.tension)
    }

    /// Gain for a duck amount.
    #[inline]
    pub fn gain(&self, d: f32) -> f32 {
        1.0 - self.depth * d
    }
}

/// Recovery progress 0..1 -> fraction recovered 0..1 (see the module docs).
#[inline]
pub fn release_shape(x: f32, tension: f32) -> f32 {
    let a = TENSION_RANGE * tension;
    if a.abs() < 1e-4 {
        return x;
    }
    (a * x).exp_m1() / a.exp_m1()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(tension: f32) -> DuckShape {
        DuckShape { depth: 1.0, attack: 0.001, hold: 0.05, release: 0.2, tension }.sanitized()
    }

    #[test]
    fn starts_dips_holds_and_recovers_exactly() {
        let s = shape(0.4);
        assert_eq!(s.duck_at(0.0, 0.0), 0.0);
        assert_eq!(s.duck_at(0.001, 0.0), 1.0);
        assert_eq!(s.duck_at(0.03, 0.0), 1.0);
        assert!(s.duck_at(s.length(), 0.0) < 1e-6);
        assert_eq!(s.duck_at(s.length() + 1e-6, 0.0), 0.0);
        assert_eq!(s.duck_at(10.0, 0.0), 0.0);
        // Gain: the floor is 1 - depth, the end is exactly unity.
        let half = DuckShape { depth: 0.5, ..s };
        assert_eq!(half.gain(1.0), 0.5);
        assert_eq!(half.gain(0.0), 1.0);
    }

    #[test]
    fn release_is_monotonic_for_every_tension() {
        for tension in [-1.0, -0.5, -0.01, 0.0, 0.01, 0.4, 1.0] {
            let s = shape(tension);
            let mut prev = 1.0;
            for i in 0..=1000 {
                let t = s.attack + s.hold + s.release * i as f32 / 1000.0;
                let d = s.duck_at(t, 0.0);
                assert!(d <= prev + 1e-6, "tension {tension}: rises at step {i}");
                assert!((0.0..=1.0).contains(&d));
                prev = d;
            }
        }
    }

    #[test]
    fn tension_zero_is_linear_and_opposites_mirror() {
        for x in [0.0, 0.1, 0.25, 0.5, 0.9, 1.0] {
            assert!((release_shape(x, 0.0) - x).abs() < 1e-6);
            for t in [0.2, 0.7, 1.0] {
                // f_-a(x) = 1 - f_a(1 - x): the same bend, point-mirrored.
                let mirrored = 1.0 - release_shape(1.0 - x, t);
                assert!((release_shape(x, -t) - mirrored).abs() < 1e-5, "x {x} t {t}");
            }
        }
        // Positive tension recovers slowly first.
        assert!(release_shape(0.5, 0.5) < 0.5);
        assert!(release_shape(0.5, -0.5) > 0.5);
    }

    #[test]
    fn no_step_is_bigger_than_the_attack_ramp_allows() {
        // At 48 kHz with a 1 ms attack the ramp moves 1/48 per sample.
        // No tension may make the release move faster than that.
        let sr = 48_000.0;
        for tension in [-1.0, 0.0, 1.0] {
            let s = shape(tension);
            let mut prev = s.duck_at(0.0, 0.0);
            let mut worst = 0.0f32;
            for n in 1..(s.length() * sr) as usize + 10 {
                let d = s.duck_at(n as f32 / sr, 0.0);
                worst = worst.max((d - prev).abs());
                prev = d;
            }
            assert!(worst <= 1.0 / (s.attack * sr) + 1e-4, "tension {tension}: step {worst}");
        }
    }

    #[test]
    fn retrigger_starts_from_the_current_amount() {
        let s = shape(0.4);
        // Halfway through a release, retrigger: the attack starts there.
        let d_now = s.duck_at(s.attack + s.hold + 0.5 * s.release, 0.0);
        assert!(d_now > 0.0 && d_now < 1.0);
        assert_eq!(s.duck_at(0.0, d_now), d_now);
        assert_eq!(s.duck_at(s.attack, d_now), 1.0);
    }

    #[test]
    fn sanitize_clamps() {
        let s = DuckShape { depth: 2.0, attack: 0.0, hold: -1.0, release: -1.0, tension: 3.0 }.sanitized();
        assert_eq!((s.depth, s.attack, s.hold, s.release, s.tension), (1.0, MIN_ATTACK, 0.0, 0.0, 1.0));
    }
}
