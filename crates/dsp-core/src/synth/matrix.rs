//! The modulation matrix: [`MOD_SLOTS`] slots, each routing a source to a
//! destination with a bipolar amount, optionally scaled by a second "via"
//! source (e.g. LFO 1 -> Pitch via Mod Wheel: the wheel sets vibrato depth).
//!
//! Each destination has a range in musical units ([`ModDest::range`]):
//! semitones for pitch, octaves for cutoff and rates, and so on. A slot's
//! contribution is `amount x source x via x range`, and slots aimed at the
//! same destination add up.

pub const MOD_SLOTS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModSource {
    /// Off as a source (contributes nothing); as a via, means "unscaled".
    None,
    /// -1..1, after its delay and fade-in.
    Lfo1,
    Lfo2,
    /// ENV 1, 0..1 (velocity-scaled like the amp).
    AmpEnv,
    /// ENV 2, 0..1.
    FilterEnv,
    /// ENV 3, 0..1.
    ModEnv,
    /// 0..1.
    Velocity,
    /// Note number: 0 at middle C (60), +-1 five octaves either side.
    Key,
    /// CC 1, 0..1.
    ModWheel,
    /// The greater of channel pressure and the note's poly pressure, 0..1.
    Aftertouch,
    /// -1..1.
    PitchBend,
    /// A fixed random value per note, -1..1.
    Random,
    /// The voice's slow random drift, -1..1 (moves whatever the Drift
    /// amount; Drift only sets how much it detunes on its own).
    Drift,
}

pub const NUM_SOURCES: usize = 13;

impl ModSource {
    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModDest {
    None,
    /// Both oscillators (and the sub).
    Pitch,
    Osc1Pitch,
    Osc2Pitch,
    Osc1PulseWidth,
    Osc2PulseWidth,
    CrossMod,
    Osc1Level,
    Osc2Level,
    SubLevel,
    NoiseLevel,
    RingLevel,
    Cutoff,
    Resonance,
    Drive,
    HpfCutoff,
    /// Voice gain: 1 + mod, floored at 0 (tremolo, accents).
    Amp,
    /// -1 left .. +1 right.
    Pan,
    /// Per-voice LFOs only (Poly / Poly Retrig); a shared Mono LFO has one
    /// rate for every voice.
    Lfo1Rate,
    Lfo2Rate,
    /// Every envelope's attack time.
    EnvAttack,
    /// Every envelope's decay and release times.
    EnvDecay,
}

pub const NUM_DESTS: usize = 22;

impl ModDest {
    /// What a full-scale contribution (amount 100 %, source 1) does.
    pub fn range(self) -> f32 {
        match self {
            ModDest::None => 0.0,
            // Semitones.
            ModDest::Pitch | ModDest::Osc1Pitch | ModDest::Osc2Pitch => 24.0,
            // Pulse width (the whole 5..95 % range from the centre).
            ModDest::Osc1PulseWidth | ModDest::Osc2PulseWidth => 0.45,
            // FM index.
            ModDest::CrossMod => super::osc::MAX_CROSS_MOD,
            // Levels and resonance, 0..1 scale.
            ModDest::Osc1Level
            | ModDest::Osc2Level
            | ModDest::SubLevel
            | ModDest::NoiseLevel
            | ModDest::RingLevel
            | ModDest::Resonance
            | ModDest::Amp
            | ModDest::Pan => 1.0,
            // Octaves.
            ModDest::Cutoff | ModDest::HpfCutoff => 10.0,
            ModDest::Lfo1Rate | ModDest::Lfo2Rate => 4.0,
            // Octaves of time (positive = longer).
            ModDest::EnvAttack | ModDest::EnvDecay => 4.0,
            // dB.
            ModDest::Drive => super::filter::MAX_DRIVE_DB,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModSlot {
    pub source: ModSource,
    pub dest: ModDest,
    /// -1..=1.
    pub amount: f32,
    /// Scales the amount; `None` = unscaled.
    pub via: ModSource,
}

impl ModSlot {
    pub const OFF: Self = Self { source: ModSource::None, dest: ModDest::None, amount: 0.0, via: ModSource::None };

    pub fn is_active(&self) -> bool {
        self.source != ModSource::None && self.dest != ModDest::None && self.amount != 0.0
    }
}

/// Every source's current value for one voice.
#[derive(Debug, Clone, Copy)]
pub struct Sources([f32; NUM_SOURCES]);

impl Sources {
    pub fn new() -> Self {
        Self([0.0; NUM_SOURCES])
    }

    pub fn set(&mut self, s: ModSource, v: f32) {
        self.0[s.index()] = v;
    }

    #[inline]
    fn value(&self, s: ModSource) -> f32 {
        self.0[s.index()]
    }

    #[inline]
    fn via(&self, s: ModSource) -> f32 {
        if s == ModSource::None {
            1.0
        } else {
            self.0[s.index()]
        }
    }
}

impl Default for Sources {
    fn default() -> Self {
        Self::new()
    }
}

/// Summed modulation per destination, in each destination's units.
#[derive(Debug, Clone, Copy)]
pub struct Targets([f32; NUM_DESTS]);

impl Targets {
    pub const ZERO: Self = Self([0.0; NUM_DESTS]);

    #[inline]
    pub fn get(&self, d: ModDest) -> f32 {
        self.0[d.index()]
    }
}

/// Evaluate every active slot.
#[inline]
pub fn evaluate(slots: &[ModSlot; MOD_SLOTS], sources: &Sources) -> Targets {
    let mut t = Targets::ZERO;
    for s in slots.iter().filter(|s| s.is_active()) {
        t.0[s.dest.index()] += s.amount * sources.value(s.source) * sources.via(s.via) * s.dest.range();
    }
    t.0[ModDest::None.index()] = 0.0;
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(source: ModSource, dest: ModDest, amount: f32, via: ModSource) -> ModSlot {
        ModSlot { source, dest, amount, via }
    }

    #[test]
    fn index_counts_match_the_enums() {
        assert_eq!(ModSource::Drift as usize + 1, NUM_SOURCES);
        assert_eq!(ModDest::EnvDecay as usize + 1, NUM_DESTS);
    }

    #[test]
    fn amount_source_via_and_range_multiply() {
        let mut src = Sources::new();
        src.set(ModSource::Lfo1, -0.5);
        src.set(ModSource::ModWheel, 0.25);
        let mut slots = [ModSlot::OFF; MOD_SLOTS];
        slots[0] = slot(ModSource::Lfo1, ModDest::Pitch, 0.5, ModSource::ModWheel);
        let t = evaluate(&slots, &src);
        // 0.5 x -0.5 x 0.25 x 24 st.
        assert_eq!(t.get(ModDest::Pitch), -1.5);
        // Without a via the amount is unscaled.
        slots[0].via = ModSource::None;
        assert_eq!(evaluate(&slots, &src).get(ModDest::Pitch), -6.0);
    }

    #[test]
    fn slots_on_the_same_destination_add() {
        let mut src = Sources::new();
        src.set(ModSource::Velocity, 1.0);
        src.set(ModSource::Key, 0.5);
        let mut slots = [ModSlot::OFF; MOD_SLOTS];
        slots[2] = slot(ModSource::Velocity, ModDest::Cutoff, 0.1, ModSource::None);
        slots[5] = slot(ModSource::Key, ModDest::Cutoff, 0.2, ModSource::None);
        assert!((evaluate(&slots, &src).get(ModDest::Cutoff) - 2.0).abs() < 1e-6); // 1 oct + 1 oct
    }

    #[test]
    fn inactive_slots_do_nothing() {
        let mut src = Sources::new();
        src.set(ModSource::Velocity, 1.0);
        let mut slots = [ModSlot::OFF; MOD_SLOTS];
        slots[0] = slot(ModSource::Velocity, ModDest::None, 1.0, ModSource::None);
        slots[1] = slot(ModSource::None, ModDest::Cutoff, 1.0, ModSource::None);
        slots[2] = slot(ModSource::Velocity, ModDest::Cutoff, 0.0, ModSource::None);
        let t = evaluate(&slots, &src);
        assert!((0..NUM_DESTS).all(|i| t.0[i] == 0.0));
    }

    #[test]
    fn a_via_at_zero_mutes_the_slot() {
        let mut src = Sources::new();
        src.set(ModSource::Lfo1, 1.0);
        let mut slots = [ModSlot::OFF; MOD_SLOTS];
        slots[0] = slot(ModSource::Lfo1, ModDest::Pitch, 1.0, ModSource::ModWheel);
        assert_eq!(evaluate(&slots, &src).get(ModDest::Pitch), 0.0);
    }
}
