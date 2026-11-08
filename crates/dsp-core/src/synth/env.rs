//! Envelopes: delay, attack, hold, decay, sustain, release (DAHDSR).
//!
//! Each timed stage runs a progress counter from 0 to 1 at a rate set by
//! its time, and a curve maps progress to level. So stage times are exact
//! whatever the curve, and changing a time mid-stage just changes the rate.
//!
//! - **Attack** blends linear with an analog RC charge aimed at
//!   [`ATTACK_TARGET`] (and stopped at 1): fast at first, rounding off near
//!   the top, the snappy shape of analog envelopes.
//! - **Decay and release** blend linear with an exponential fall, as an
//!   analog envelope discharging. Both reach their target exactly on time.
//! - Every stage starts from the current level, so retriggering or
//!   releasing mid-stage never jumps.

use super::patch::EnvSettings;

/// The RC attack charges toward this and stops at 1.0 (as the Polysix's
/// envelope comparator does), which sets how sharply it rounds off.
pub const ATTACK_TARGET: f32 = 1.3;
/// Exponential sharpness of decay/release at full curve: the fall covers
/// e^-8 (about -70 dB) of its range before the final linear correction.
pub const MAX_DECAY_SHARPNESS: f32 = 8.0;
/// Shortest stage time used for the timed stages (avoids division by 0).
pub const MIN_TIME: f32 = 0.000_5;
/// Velocity -> attack time at full amount: velocity 1 shortens the attack
/// by this many octaves (x 1/16).
pub const VELOCITY_ATTACK_OCTAVES: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Idle,
    Delay,
    Attack,
    Hold,
    Decay,
    Sustain,
    Release,
}

/// Per-sample settings shared by every voice's copy of one envelope,
/// derived from [`EnvSettings`] for the sample rate.
#[derive(Debug, Clone, Copy)]
pub struct EnvShared {
    /// Progress per sample for each stage (infinite for a zero-time
    /// delay or hold, which are skipped).
    delay_rate: f32,
    attack_rate: f32,
    hold_rate: f32,
    decay_rate: f32,
    release_rate: f32,
    attack_curve: f32,
    decay_sharpness: f32,
    pub sustain: f32,
    velocity_level: f32,
    velocity_attack: f32,
}

impl EnvShared {
    pub fn new(s: &EnvSettings, sample_rate: f32) -> Self {
        let rate = |t: f32| 1.0 / (t.max(MIN_TIME) * sample_rate);
        let optional = |t: f32| if t <= 0.0 { f32::INFINITY } else { 1.0 / (t * sample_rate) };
        Self {
            delay_rate: optional(s.delay),
            attack_rate: rate(s.attack),
            hold_rate: optional(s.hold),
            decay_rate: rate(s.decay),
            release_rate: rate(s.release),
            attack_curve: s.attack_curve,
            decay_sharpness: s.decay_curve * MAX_DECAY_SHARPNESS,
            sustain: s.sustain,
            velocity_level: s.velocity_level,
            velocity_attack: s.velocity_attack,
        }
    }
}

/// Attack shape: progress 0..1 -> level 0..1, `curve` 0 = linear, 1 = RC.
#[inline]
pub fn attack_shape(p: f32, curve: f32) -> f32 {
    // RC charge toward ATTACK_TARGET, scaled in time so it reaches 1 at p = 1.
    let k = (ATTACK_TARGET / (ATTACK_TARGET - 1.0)).ln();
    let rc = ATTACK_TARGET * (1.0 - (-p * k).exp());
    p + curve * (rc - p)
}

/// Fall shape: progress 0..1 -> remaining 1..0. `sharpness` 0 = linear.
#[inline]
pub fn fall_shape(p: f32, sharpness: f32) -> f32 {
    if sharpness < 1e-3 {
        return 1.0 - p;
    }
    let end = (-sharpness).exp();
    ((-p * sharpness).exp() - end) / (1.0 - end)
}

#[derive(Debug, Clone, Copy)]
pub struct Envelope {
    stage: Stage,
    /// Progress through the current stage, 0..1.
    p: f32,
    /// Level when the current stage started.
    from: f32,
    /// Unscaled level, 0..1.
    level: f32,
    /// Velocity scaling of the output level.
    scale: f32,
    /// Velocity scaling of the attack rate.
    attack_speed: f32,
}

impl Default for Envelope {
    fn default() -> Self {
        Self { stage: Stage::Idle, p: 0.0, from: 0.0, level: 0.0, scale: 1.0, attack_speed: 1.0 }
    }
}

impl Envelope {
    pub fn stage(&self) -> Stage {
        self.stage
    }

    pub fn is_idle(&self) -> bool {
        self.stage == Stage::Idle
    }

    /// Start (or restart) from the current level.
    pub fn trigger(&mut self, velocity: f32, sh: &EnvShared) {
        let v = velocity.clamp(0.0, 1.0);
        // Rescale the held level so the output doesn't jump if the velocity
        // scaling changes on a retrigger.
        let out = self.level * self.scale;
        self.scale = 1.0 - sh.velocity_level * (1.0 - v);
        self.level = if self.scale > 0.0 { (out / self.scale).min(1.0) } else { 0.0 };
        self.attack_speed = (VELOCITY_ATTACK_OCTAVES * sh.velocity_attack * v).exp2();
        self.enter(Stage::Delay);
    }

    /// Start from silence.
    pub fn trigger_from_zero(&mut self, velocity: f32, sh: &EnvShared) {
        self.level = 0.0;
        self.trigger(velocity, sh);
    }

    /// Key up: release from wherever it is.
    pub fn release(&mut self) {
        if !matches!(self.stage, Stage::Idle | Stage::Release) {
            self.enter(Stage::Release);
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn enter(&mut self, stage: Stage) {
        self.stage = stage;
        self.p = 0.0;
        self.from = self.level;
    }

    /// Advance one sample and return the (velocity-scaled) level.
    #[inline]
    pub fn tick(&mut self, sh: &EnvShared) -> f32 {
        self.tick_scaled(sh, 1.0, 1.0)
    }

    /// [`Self::tick`] with the attack rate scaled by `attack_speed` and the
    /// decay and release rates by `decay_speed` (modulation; 2 = twice as
    /// fast).
    #[inline]
    pub fn tick_scaled(&mut self, sh: &EnvShared, attack_speed: f32, decay_speed: f32) -> f32 {
        // Loops only to skip stages that take no time (zero delay or hold).
        loop {
            let (rate, next) = match self.stage {
                Stage::Idle => return 0.0,
                Stage::Sustain => {
                    self.level = sh.sustain;
                    if sh.sustain <= 0.0 {
                        // Nothing left to hear until note-off: finish now so
                        // the voice is freed (plucks don't hog polyphony).
                        self.stage = Stage::Idle;
                    }
                    return self.level * self.scale;
                }
                Stage::Delay => (sh.delay_rate, Stage::Attack),
                Stage::Attack => (sh.attack_rate * self.attack_speed * attack_speed, Stage::Hold),
                Stage::Hold => (sh.hold_rate, Stage::Decay),
                Stage::Decay => (sh.decay_rate * decay_speed, Stage::Sustain),
                Stage::Release => (sh.release_rate * decay_speed, Stage::Idle),
            };
            self.p += rate;
            if self.p >= 1.0 {
                // Stage complete: land exactly on its end level.
                self.level = match self.stage {
                    Stage::Attack | Stage::Hold => 1.0,
                    Stage::Decay => sh.sustain,
                    Stage::Release => 0.0,
                    _ => self.level,
                };
                self.enter(next);
                if rate.is_infinite() {
                    continue;
                }
                return self.level * self.scale;
            }
            self.level = match self.stage {
                Stage::Attack => self.from + (1.0 - self.from) * attack_shape(self.p, sh.attack_curve),
                Stage::Decay => sh.sustain + (self.from - sh.sustain) * fall_shape(self.p, sh.decay_sharpness),
                Stage::Release => self.from * fall_shape(self.p, sh.decay_sharpness),
                _ => self.level,
            };
            return self.level * self.scale;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn settings() -> EnvSettings {
        EnvSettings {
            delay: 0.0,
            attack: 0.01,
            hold: 0.0,
            decay: 0.02,
            sustain: 0.5,
            release: 0.03,
            attack_curve: 0.0,
            decay_curve: 0.0,
            velocity_level: 0.0,
            velocity_attack: 0.0,
        }
    }

    fn samples(secs: f32) -> usize {
        (secs * SR).round() as usize
    }

    /// Ticks until `stage` is left, from the current state.
    fn ticks_in(e: &mut Envelope, sh: &EnvShared, stage: Stage) -> usize {
        let mut n = 0;
        while e.stage() == stage {
            e.tick(sh);
            n += 1;
            assert!(n < 10_000_000);
        }
        n
    }

    #[test]
    fn stage_times_are_exact_for_any_curve() {
        for curve in [0.0f32, 0.5, 1.0] {
            let s = EnvSettings { delay: 0.005, hold: 0.004, attack_curve: curve, decay_curve: curve, ..settings() };
            let sh = EnvShared::new(&s, SR);
            let mut e = Envelope::default();
            e.trigger(1.0, &sh);
            for (stage, secs) in [
                (Stage::Delay, s.delay),
                (Stage::Attack, s.attack),
                (Stage::Hold, s.hold),
                (Stage::Decay, s.decay),
            ] {
                let n = ticks_in(&mut e, &sh, stage);
                assert!(n.abs_diff(samples(secs)) <= 1, "curve {curve}: {stage:?} took {n} samples");
            }
            assert_eq!(e.stage(), Stage::Sustain);
            assert_eq!(e.tick(&sh), 0.5);
            e.release();
            let n = ticks_in(&mut e, &sh, Stage::Release);
            assert!(n.abs_diff(samples(s.release)) <= 1, "curve {curve}: release took {n}");
            assert_eq!(e.tick(&sh), 0.0);
        }
    }

    #[test]
    fn curves_shape_the_middle_not_the_ends() {
        // Linear attack is halfway at half time; RC is well past it.
        assert!((attack_shape(0.5, 0.0) - 0.5).abs() < 1e-6);
        let rc_mid = attack_shape(0.5, 1.0);
        // RC at half time: T (1 - e^(-k/2)), and e^-k = (T - 1) / T.
        let expected = ATTACK_TARGET * (1.0 - ((ATTACK_TARGET - 1.0) / ATTACK_TARGET).sqrt());
        assert!((rc_mid - expected).abs() < 1e-5, "RC midpoint {rc_mid}");
        assert!(rc_mid > 0.65);
        for c in [0.0f32, 0.3, 1.0] {
            assert!(attack_shape(0.0, c).abs() < 1e-6);
            assert!((attack_shape(1.0, c) - 1.0).abs() < 1e-5);
        }
        // Exponential fall is far below linear at half time; both hit 0 at the end.
        assert!((fall_shape(0.5, 0.0) - 0.5).abs() < 1e-6);
        assert!(fall_shape(0.5, MAX_DECAY_SHARPNESS) < 0.02);
        for s in [0.0f32, 2.0, MAX_DECAY_SHARPNESS] {
            assert!((fall_shape(0.0, s) - 1.0).abs() < 1e-6);
            assert!(fall_shape(1.0, s).abs() < 1e-6);
        }
    }

    #[test]
    fn release_mid_attack_starts_from_the_current_level() {
        let sh = EnvShared::new(&settings(), SR);
        let mut e = Envelope::default();
        e.trigger(1.0, &sh);
        let mut last = 0.0;
        for _ in 0..samples(0.005) {
            last = e.tick(&sh);
        }
        e.release();
        let next = e.tick(&sh);
        assert!((next - last).abs() < 0.01, "{last} -> {next}");
        assert!(next < last);
    }

    #[test]
    fn retrigger_continues_from_the_current_level() {
        let sh = EnvShared::new(&settings(), SR);
        let mut e = Envelope::default();
        e.trigger(1.0, &sh);
        for _ in 0..samples(0.1) {
            e.tick(&sh);
        }
        e.release();
        let mut last = 0.0;
        for _ in 0..samples(0.01) {
            last = e.tick(&sh);
        }
        e.trigger(1.0, &sh);
        let next = e.tick(&sh);
        assert!(next >= last && next - last < 0.01, "{last} -> {next}");
        // ...whereas trigger_from_zero restarts from silence.
        e.trigger_from_zero(1.0, &sh);
        assert!(e.tick(&sh) < 0.01);
    }

    #[test]
    fn velocity_scales_level_and_speeds_up_the_attack() {
        let s = EnvSettings { sustain: 1.0, velocity_level: 1.0, velocity_attack: 0.5, ..settings() };
        let sh = EnvShared::new(&s, SR);
        let mut soft = Envelope::default();
        soft.trigger(0.25, &sh);
        let soft_attack = ticks_in(&mut soft, &sh, Stage::Delay) + ticks_in(&mut soft, &sh, Stage::Attack);
        assert!((soft.tick(&sh) - 0.25).abs() < 1e-6, "level follows velocity");
        let mut hard = Envelope::default();
        hard.trigger(1.0, &sh);
        let hard_attack = ticks_in(&mut hard, &sh, Stage::Delay) + ticks_in(&mut hard, &sh, Stage::Attack);
        // Half amount: velocity 1 shortens the attack by 2 octaves (x 1/4),
        // velocity 0.25 by half an octave, so the soft one takes 2^1.5 as long.
        let ratio = soft_attack as f32 / hard_attack as f32;
        assert!((ratio - 2f32.powf(1.5)).abs() < 0.05, "attack ratio {ratio}");
        // Without velocity amounts, velocity changes nothing.
        let flat = EnvShared::new(&EnvSettings { sustain: 1.0, ..settings() }, SR);
        let mut e = Envelope::default();
        e.trigger(0.1, &flat);
        for _ in 0..samples(0.05) {
            e.tick(&flat);
        }
        assert_eq!(e.tick(&flat), 1.0);
    }

    #[test]
    fn zero_sustain_finishes_after_the_decay() {
        let sh = EnvShared::new(&EnvSettings { sustain: 0.0, ..settings() }, SR);
        let mut e = Envelope::default();
        e.trigger(1.0, &sh);
        for _ in 0..samples(0.031) + 2 {
            e.tick(&sh);
        }
        assert!(e.is_idle());
    }

    #[test]
    fn sustain_follows_changes_while_held() {
        let s = settings();
        let mut sh = EnvShared::new(&s, SR);
        let mut e = Envelope::default();
        e.trigger(1.0, &sh);
        for _ in 0..samples(0.05) {
            e.tick(&sh);
        }
        sh.sustain = 0.8;
        assert_eq!(e.tick(&sh), 0.8);
    }
}
