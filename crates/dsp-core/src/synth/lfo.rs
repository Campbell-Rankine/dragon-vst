//! LFOs: low-frequency oscillators for vibrato, PWM and filter sweeps.
//!
//! An [`Lfo`] is the oscillator (shape, phase, random state). A [`Depth`]
//! is a per-note delay and fade-in applied on top, so even a shared (mono)
//! LFO eases in on every note, like the Polysix's delayed vibrato.

use super::noise::Rng;
use super::patch::{LfoMode, LfoSettings, LfoShape};
use std::f32::consts::{PI, TAU};

#[derive(Debug, Clone, Copy)]
pub struct Lfo {
    /// Phase in cycles, 0..1.
    phase: f32,
    rng: Rng,
    /// Sample & hold value, and the smooth-random segment's ends.
    held: f32,
    from: f32,
    to: f32,
}

impl Lfo {
    pub fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        let held = rng.next_bipolar();
        let to = rng.next_bipolar();
        Self { phase: 0.0, rng, held, from: held, to }
    }

    pub fn phase(&self) -> f32 {
        self.phase
    }

    /// Restart at `phase` (in cycles). Random shapes pick fresh values.
    pub fn restart(&mut self, phase: f32) {
        self.phase = phase.rem_euclid(1.0);
        self.new_cycle();
    }

    /// Restart at a random phase.
    pub fn restart_random(&mut self) {
        let p = self.rng.next_f32();
        self.restart(p);
    }

    fn new_cycle(&mut self) {
        self.held = self.rng.next_bipolar();
        self.from = self.to;
        self.to = self.rng.next_bipolar();
    }

    /// The value at the current phase, -1..1, then advance by `dt` cycles.
    #[inline]
    pub fn tick(&mut self, shape: LfoShape, dt: f32) -> f32 {
        let p = self.phase;
        let y = match shape {
            LfoShape::Sine => (TAU * p).sin(),
            // Starts at 0 rising, like the sine.
            LfoShape::Triangle => 1.0 - 4.0 * ((p + 0.25).rem_euclid(1.0) - 0.5).abs(),
            LfoShape::SawUp => 2.0 * p - 1.0,
            LfoShape::SawDown => 1.0 - 2.0 * p,
            LfoShape::Square => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoShape::SampleHold => self.held,
            // Cosine interpolation between random points, one per cycle.
            LfoShape::SmoothRandom => self.from + (self.to - self.from) * 0.5 * (1.0 - (PI * p).cos()),
        };
        self.phase += dt;
        if self.phase >= 1.0 {
            self.phase -= self.phase.floor();
            self.new_cycle();
        }
        y
    }
}

/// Per-note LFO depth: 0 during the delay, then a linear fade-in to 1.
#[derive(Debug, Clone, Copy, Default)]
pub struct Depth {
    /// Seconds since note-on, stopped once the fade is done.
    t: f32,
    done: bool,
}

impl Depth {
    pub fn restart(&mut self) {
        *self = Self::default();
    }

    #[inline]
    pub fn tick(&mut self, delay: f32, fade: f32, dt_secs: f32) -> f32 {
        if self.done {
            return 1.0;
        }
        let t = self.t;
        self.t += dt_secs;
        if t < delay {
            0.0
        } else if t < delay + fade {
            (t - delay) / fade
        } else {
            self.done = true;
            1.0
        }
    }
}

/// Where a voice's LFO starts on note-on, per mode (mono LFOs aren't
/// restarted by notes).
pub fn start_voice_lfo(lfo: &mut Lfo, s: &LfoSettings) {
    match s.mode {
        LfoMode::Mono => {}
        LfoMode::Poly => lfo.restart_random(),
        LfoMode::PolyRetrigger => lfo.restart(s.phase),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;
    const ALL: [LfoShape; 7] = [
        LfoShape::Sine,
        LfoShape::Triangle,
        LfoShape::SawUp,
        LfoShape::SawDown,
        LfoShape::Square,
        LfoShape::SampleHold,
        LfoShape::SmoothRandom,
    ];

    fn render(shape: LfoShape, hz: f32, secs: f32) -> Vec<f32> {
        let mut l = Lfo::new(1);
        l.restart(0.0);
        (0..(secs * SR) as usize).map(|_| l.tick(shape, hz / SR)).collect()
    }

    #[test]
    fn rate_is_accurate() {
        for hz in [0.1f32, 5.0, 50.0] {
            let secs = 20.0f32.max(10.0 / hz);
            let x = render(LfoShape::Square, hz, secs);
            // Count rising edges: one per cycle.
            let edges = x.windows(2).filter(|w| w[0] < 0.0 && w[1] > 0.0).count() as f32;
            let expected = hz * secs - 1.0; // the first cycle starts high, no edge
            assert!((edges - expected).abs() <= 1.0, "{hz} Hz: {edges} cycles, expected ~{expected}");
        }
    }

    #[test]
    fn shapes_at_known_phases() {
        // Value at phase p for each shape (random ones checked separately).
        let at = |shape: LfoShape, p: f32| {
            let mut l = Lfo::new(1);
            l.restart(p);
            l.tick(shape, 0.0)
        };
        for (shape, points) in [
            (LfoShape::Sine, [(0.0, 0.0), (0.25, 1.0), (0.75, -1.0)]),
            (LfoShape::Triangle, [(0.0, 0.0), (0.25, 1.0), (0.75, -1.0)]),
            (LfoShape::SawUp, [(0.0, -1.0), (0.5, 0.0), (0.75, 0.5)]),
            (LfoShape::SawDown, [(0.0, 1.0), (0.5, 0.0), (0.75, -0.5)]),
            (LfoShape::Square, [(0.0, 1.0), (0.49, 1.0), (0.5, -1.0)]),
        ] {
            for (p, v) in points {
                assert!((at(shape, p) - v).abs() < 1e-5, "{shape:?} at {p}: {}", at(shape, p));
            }
        }
    }

    #[test]
    fn every_shape_stays_in_range() {
        for shape in ALL {
            assert!(render(shape, 7.0, 5.0).iter().all(|v| (-1.0..=1.0).contains(v)), "{shape:?}");
        }
    }

    #[test]
    fn sample_and_hold_steps_once_per_cycle() {
        // 10 Hz = 4 800 samples per cycle.
        let x = render(LfoShape::SampleHold, 10.0, 2.0);
        let changes: Vec<usize> = (1..x.len()).filter(|&i| x[i] != x[i - 1]).collect();
        assert!(changes.len() >= 15, "only {} steps", changes.len());
        for w in changes.windows(2) {
            assert!(w[1] - w[0] >= 4_799 && w[1] - w[0] <= 4_801, "steps {} apart", w[1] - w[0]);
        }
    }

    #[test]
    fn smooth_random_is_continuous() {
        let x = render(LfoShape::SmoothRandom, 10.0, 2.0);
        // Steepest a cosine segment can be: pi/2 x 2 (full swing) per cycle.
        let max_step = PI / 2.0 * 2.0 * 10.0 / SR;
        let worst = x.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0f32, f32::max);
        assert!(worst <= max_step * 1.01, "jump of {worst}, limit {max_step}");
        assert!(x.iter().any(|v| v.abs() > 0.3), "should actually move");
    }

    #[test]
    fn same_seed_same_randomness() {
        let a = render(LfoShape::SampleHold, 20.0, 1.0);
        let b = render(LfoShape::SampleHold, 20.0, 1.0);
        assert_eq!(a, b);
    }

    #[test]
    fn depth_delays_then_fades_in() {
        let mut d = Depth::default();
        let dt = 1.0 / SR;
        let v: Vec<f32> = (0..(0.4 * SR) as usize).map(|_| d.tick(0.1, 0.2, dt)).collect();
        let at = |secs: f32| v[(secs * SR) as usize];
        assert_eq!(at(0.05), 0.0);
        assert!((at(0.2) - 0.5).abs() < 1e-3, "halfway through the fade: {}", at(0.2));
        assert_eq!(at(0.35), 1.0);
        // No delay or fade: full depth at once.
        let mut d = Depth::default();
        assert_eq!(d.tick(0.0, 0.0, dt), 1.0);
    }

    #[test]
    fn note_on_start_depends_on_mode() {
        let s = |mode| LfoSettings { mode, phase: 0.25, ..LfoSettings::default() };
        let mut l = Lfo::new(3);
        l.restart(0.6);
        start_voice_lfo(&mut l, &s(LfoMode::Mono));
        assert_eq!(l.phase(), 0.6, "mono LFOs ignore notes");
        start_voice_lfo(&mut l, &s(LfoMode::PolyRetrigger));
        assert_eq!(l.phase(), 0.25);
        start_voice_lfo(&mut l, &s(LfoMode::Poly));
        let p1 = l.phase();
        start_voice_lfo(&mut l, &s(LfoMode::Poly));
        assert_ne!(p1, l.phase(), "free poly LFOs start at random phases");
    }
}
