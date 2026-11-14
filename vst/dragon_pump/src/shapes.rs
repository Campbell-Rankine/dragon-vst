//! Shape presets: the CURVE panel's chips, each setting Attack, Hold,
//! Release and Tension together (Depth, Mix and the trigger are left
//! alone). Our own shapes, named by feel; see `docs/dragon_pump.md`.
//!
//! Shapes are written as fractions of a Tempo-mode cycle. In MIDI mode,
//! whose Hold and Release are in ms, they're scaled to a 1/4 note at
//! 120 BPM ([`REFERENCE_CYCLE_MS`]), so a shape sounds the same there.

use crate::params::{DragonPumpParams, Mode};
use nih_plug::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapePreset {
    pub name: &'static str,
    /// What the chip's hover text says.
    pub hint: &'static str,
    pub attack_ms: f32,
    /// Fractions of the cycle.
    pub hold: f32,
    pub release: f32,
    pub tension: f32,
}

pub const SHAPES: [ShapePreset; 8] = [
    ShapePreset { name: "Pump", hint: "The classic: a fast dip, slow-then-fast recovery", attack_ms: 1.0, hold: 0.0, release: 0.6, tension: 0.4 },
    ShapePreset { name: "Soft", hint: "A gentle, even dip", attack_ms: 8.0, hold: 0.0, release: 0.85, tension: 0.0 },
    ShapePreset { name: "Hard", hint: "Slams down, stays low, then snaps back", attack_ms: 0.5, hold: 0.05, release: 0.5, tension: 0.8 },
    ShapePreset { name: "Gate", hint: "A long hold and a fast release: choppy", attack_ms: 1.0, hold: 0.45, release: 0.1, tension: 0.2 },
    ShapePreset { name: "Swell", hint: "The release covers the whole cycle", attack_ms: 2.0, hold: 0.0, release: 1.0, tension: 0.6 },
    ShapePreset { name: "Tight", hint: "A short dip that clears out quickly", attack_ms: 0.5, hold: 0.0, release: 0.25, tension: 0.3 },
    ShapePreset { name: "Late", hint: "A slow 15 ms attack lets the kick's transient through first", attack_ms: 15.0, hold: 0.0, release: 0.6, tension: 0.4 },
    ShapePreset { name: "Breathe", hint: "Recovers fast, then eases in", attack_ms: 3.0, hold: 0.0, release: 0.7, tension: -0.6 },
];

/// The cycle a shape is scaled to in MIDI mode (ms): a 1/4 note at 120 BPM.
pub const REFERENCE_CYCLE_MS: f32 = 500.0;

/// How close the parameters must be to light a preset's chip.
const MATCH_TOLERANCE: f32 = 1e-3;

impl ShapePreset {
    /// The four parameters this shape sets in `mode`, with their values.
    fn targets<'a>(&self, params: &'a DragonPumpParams, mode: Mode) -> [(&'a FloatParam, f32); 4] {
        let (hold, release) = if mode.is_cyclic() {
            ((&params.hold_pct, self.hold), (&params.release_pct, self.release))
        } else {
            ((&params.hold_ms, self.hold * REFERENCE_CYCLE_MS), (&params.release_ms, self.release * REFERENCE_CYCLE_MS))
        };
        [(&params.attack, self.attack_ms), hold, release, (&params.tension, self.tension)]
    }

    /// Whether the parameters are at this shape in `mode` (its chip is lit).
    pub fn matches(&self, params: &DragonPumpParams, mode: Mode) -> bool {
        let close = |a: f32, b: f32| (a - b).abs() <= MATCH_TOLERANCE * b.abs().max(1.0);
        self.targets(params, mode).iter().all(|(p, v)| close(p.value(), *v))
    }

    /// Set the four parameters as one host gesture: every begin, then
    /// every set, then every end, so an undo step covers the whole shape.
    pub fn apply(&self, params: &DragonPumpParams, mode: Mode, setter: &ParamSetter) {
        let targets = self.targets(params, mode);
        for (param, _) in targets {
            setter.begin_set_parameter(param);
        }
        for (param, value) in targets {
            setter.set_parameter(param, value);
        }
        for (param, _) in targets {
            setter.end_set_parameter(param);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pump_is_the_default_shape() {
        let params = DragonPumpParams::default();
        // In both units: MIDI mode's defaults (0 ms, 300 ms) are Pump at
        // the 500 ms reference cycle.
        for mode in [Mode::Tempo, Mode::Midi] {
            let lit: Vec<_> = SHAPES.iter().filter(|s| s.matches(&params, mode)).map(|s| s.name).collect();
            assert_eq!(lit, ["Pump"], "{mode:?}");
        }
    }

    #[test]
    fn every_shape_is_in_range_and_distinct() {
        let params = DragonPumpParams::default();
        let in_range = |p: &FloatParam, v: f32| (p.preview_plain(0.0)..=p.preview_plain(1.0)).contains(&v);
        for (i, s) in SHAPES.iter().enumerate() {
            assert!(in_range(&params.attack, s.attack_ms), "{} attack", s.name);
            assert!(in_range(&params.hold_pct, s.hold), "{} hold", s.name);
            assert!(in_range(&params.release_pct, s.release), "{} release", s.name);
            assert!(in_range(&params.tension, s.tension), "{} tension", s.name);
            assert!(in_range(&params.hold_ms, s.hold * REFERENCE_CYCLE_MS), "{} hold ms", s.name);
            assert!(in_range(&params.release_ms, s.release * REFERENCE_CYCLE_MS), "{} release ms", s.name);
            for other in &SHAPES[i + 1..] {
                assert!(
                    (s.attack_ms, s.hold, s.release, s.tension) != (other.attack_ms, other.hold, other.release, other.tension),
                    "{} and {} are the same shape",
                    s.name,
                    other.name
                );
            }
        }
    }
}
