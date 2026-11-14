//! Volume ducking ("sidechain pump") for Dragon Pump: the curve, Tempo
//! mode's clock, and a click guard. Host-independent; the plugin maps its
//! parameters to these.
//!
//! - [`curve`]: [`DuckShape`], the attack / hold / release / tension curve.
//! - [`clock`]: [`TempoClock`] and [`tempo_duck`], the curve locked to the
//!   beat grid.
//! - [`trigger`]: [`TriggeredDuck`], the curve restarted by each trigger
//!   (MIDI notes, sidechain hits).
//! - [`detector`]: [`KickDetector`], which finds the hits in a sidechain
//!   signal.

pub mod clock;
pub mod curve;
pub mod detector;
pub mod trigger;

pub use clock::{tempo_duck, TempoClock};
pub use detector::{Detection, DetectorSettings, KickDetector};
pub use trigger::TriggeredDuck;
pub use curve::{release_shape, DuckShape, MIN_ATTACK, TENSION_RANGE};

/// A one-pole smoother on the final gain. The curve itself never jumps;
/// this catches the jumps it can't see, such as the transport stopping
/// mid-dip with "run when stopped" off, or a mode change.
#[derive(Debug, Clone, Copy)]
pub struct GainSmoother {
    value: f32,
    coeff: f32,
}

/// The smoother's time constant (s): short enough to leave the shortest
/// attack (0.1 ms) nearly untouched in effect, long enough to stop a click.
pub const GAIN_SMOOTHING: f32 = 0.000_5;

impl GainSmoother {
    pub fn new(sample_rate: f32) -> Self {
        Self { value: 1.0, coeff: (-1.0 / (GAIN_SMOOTHING * sample_rate)).exp() }
    }

    #[inline]
    pub fn next(&mut self, target: f32) -> f32 {
        self.value = target + self.coeff * (self.value - target);
        self.value
    }

    pub fn reset(&mut self) {
        self.value = 1.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoother_follows_and_softens_a_jump() {
        let mut s = GainSmoother::new(48_000.0);
        assert!((s.next(1.0) - 1.0).abs() < 1e-6);
        // A jump to 0: the first step is well short of it...
        let first = s.next(0.0);
        assert!(first > 0.9, "{first}");
        // ...and it's there within 10 time constants (5 ms).
        for _ in 0..240 {
            s.next(0.0);
        }
        assert!(s.next(0.0) < 1e-3);
    }
}
