//! [`Patch`]: every sound setting of the synth in one plain `Copy` struct.
//!
//! The plugin builds a `Patch` from its (smoothed) parameters once per
//! control block and hands it to [`Synth::set_patch`](super::Synth::set_patch).
//! Values are in musical units (semitones, cents, 0..1 levels); `Synth`
//! clamps anything out of range.

/// Most voices the synth can play at once. Voice storage is fixed at this
/// size, plus a few slots for voices fading out after being stolen.
pub const MAX_VOICES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waveform {
    Saw,
    /// Variable width; see [`OscSettings::pulse_width`].
    Pulse,
    Triangle,
    Sine,
}

/// What an oscillator's phase does at note-on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseMode {
    /// Start at a random phase, as a free-running analog oscillator would.
    /// Each note sounds slightly different; stacked notes don't phase-lock.
    Free,
    /// Start at phase 0 every note: identical, punchier attacks.
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubWave {
    Square,
    /// 25 % pulse: thinner and more nasal than the square.
    Pulse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubOctave {
    Down1,
    Down2,
}

impl SubOctave {
    /// Frequency divider relative to oscillator 1.
    pub fn divider(self) -> u32 {
        match self {
            SubOctave::Down1 => 2,
            SubOctave::Down2 => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoiseColor {
    White,
    /// -3 dB/oct: darker, closer to analog noise sources.
    Pink,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OscSettings {
    pub wave: Waveform,
    /// Octave relative to 8' (the played pitch): -1 = 16', 0 = 8', 1 = 4', 2 = 2'.
    pub octave: i32,
    /// -12..=12 semitones.
    pub semitone: f32,
    /// -100..=100 cents.
    pub fine: f32,
    /// Fraction of the cycle the pulse is high, 0.05..=0.95. Pulse wave only.
    pub pulse_width: f32,
    /// Mixer level, 0..=1.
    pub level: f32,
    pub phase: PhaseMode,
}

impl OscSettings {
    /// Total pitch offset from the played note, in semitones.
    pub fn pitch_semitones(&self) -> f32 {
        12.0 * self.octave as f32 + self.semitone + self.fine / 100.0
    }
}

/// Square or pulse one or two octaves below oscillator 1, phase-locked to
/// it (a frequency divider, as on the Polysix).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SubSettings {
    pub wave: SubWave,
    pub octave: SubOctave,
    /// Mixer level, 0..=1.
    pub level: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoiseSettings {
    pub color: NoiseColor,
    /// Mixer level, 0..=1.
    pub level: f32,
}

/// The main filter. See [`super::filter`] for the models.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterModel {
    /// Bypassed (drive and HPF still apply).
    Off,
    /// LP24 "Six": 4-pole ladder, Polysix / Poly-800 style.
    Ladder,
    SvfLowPass,
    SvfBandPass,
    SvfHighPass,
    SvfNotch,
    /// MS-20 style Korg35 low-pass (12 dB/oct).
    MsLowPass,
    /// MS-20 style Korg35 high-pass (6 dB/oct, resonant).
    MsHighPass,
}

/// Key tracking is centred here: at any tracking amount, middle C (MIDI 60)
/// plays with the cutoff exactly as set.
pub const KEY_TRACK_CENTER: f32 = 60.0;
/// Octaves of cutoff that full velocity tracking spans between velocity 0
/// and 1.
pub const VELOCITY_TRACK_OCTAVES: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FilterSettings {
    pub model: FilterModel,
    /// Hz, 20..=20 000. Modulation (key, velocity) moves it in octaves.
    pub cutoff: f32,
    /// 0..=1; self-oscillates near 1 in the ladder and MS models.
    pub resonance: f32,
    /// Pre-filter drive, 0..=24 dB (0 = clean bypass).
    pub drive_db: f32,
    /// 0..=2: cutoff octaves per octave played, relative to middle C.
    pub key_track: f32,
    /// -1..=1: cutoff change from velocity. Positive = softer notes darker
    /// (the cutoff is as set at full velocity, up to
    /// [`VELOCITY_TRACK_OCTAVES`] lower at velocity 0).
    pub velocity: f32,
    /// 0..=1: ladder only, restores the bass that resonance removes.
    pub bass_comp: f32,
    /// -1..=1: ENV 2 -> cutoff, as a fraction of [`ENV_AMOUNT_OCTAVES`].
    pub env_amount: f32,
    /// One-pole high-pass before the filter, Hz. At or below
    /// [`HPF_OFF_HZ`](super::filter::HPF_OFF_HZ) it's bypassed.
    pub hpf_cutoff: f32,
}

/// One envelope. Times in seconds; levels and amounts 0..=1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvSettings {
    /// 0..=10 s before the attack starts.
    pub delay: f32,
    /// 0.5 ms..=20 s.
    pub attack: f32,
    /// 0..=10 s at full level before the decay.
    pub hold: f32,
    /// 1 ms..=20 s from full level to sustain.
    pub decay: f32,
    pub sustain: f32,
    /// 1 ms..=20 s from wherever it is to 0.
    pub release: f32,
    /// 0 = linear, 1 = analog RC (fast start, rounded top).
    pub attack_curve: f32,
    /// Decay and release: 0 = linear, 1 = exponential.
    pub decay_curve: f32,
    /// How much velocity scales the level: 0 = not at all, 1 = level =
    /// velocity.
    pub velocity_level: f32,
    /// How much velocity shortens the attack: at 1, full velocity makes it
    /// 16 x shorter.
    pub velocity_attack: f32,
}

impl EnvSettings {
    fn sanitized(&self) -> Self {
        Self {
            delay: self.delay.clamp(0.0, 10.0),
            attack: self.attack.clamp(0.000_5, 20.0),
            hold: self.hold.clamp(0.0, 10.0),
            decay: self.decay.clamp(0.001, 20.0),
            sustain: self.sustain.clamp(0.0, 1.0),
            release: self.release.clamp(0.001, 20.0),
            attack_curve: self.attack_curve.clamp(0.0, 1.0),
            decay_curve: self.decay_curve.clamp(0.0, 1.0),
            velocity_level: self.velocity_level.clamp(0.0, 1.0),
            velocity_attack: self.velocity_attack.clamp(0.0, 1.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LfoShape {
    Sine,
    Triangle,
    SawUp,
    SawDown,
    Square,
    /// A new random value each cycle, held.
    SampleHold,
    /// A new random target each cycle, glided to (cosine).
    SmoothRandom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LfoMode {
    /// One LFO shared by every voice, free-running: notes move together.
    Mono,
    /// One per voice, each starting at a random phase: a looser, analog
    /// ensemble feel.
    Poly,
    /// One per voice, restarting at the start phase on every note.
    PolyRetrigger,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LfoSettings {
    pub shape: LfoShape,
    /// 0.01..=50 Hz (the plugin converts tempo-synced divisions to Hz).
    pub rate: f32,
    /// Start phase in cycles (0..1), used by `Mono` at reset and by
    /// `PolyRetrigger` on every note.
    pub phase: f32,
    /// 0..=10 s after note-on before the LFO has any effect.
    pub delay: f32,
    /// 0..=10 s fade-in after the delay.
    pub fade: f32,
    pub mode: LfoMode,
}

impl Default for LfoSettings {
    /// A gentle triangle vibrato source, shared like the Polysix's.
    fn default() -> Self {
        Self { shape: LfoShape::Triangle, rate: 5.0, phase: 0.0, delay: 0.0, fade: 0.0, mode: LfoMode::Mono }
    }
}

impl LfoSettings {
    fn sanitized(&self) -> Self {
        Self {
            rate: self.rate.clamp(0.01, 50.0),
            phase: self.phase.rem_euclid(1.0),
            delay: self.delay.clamp(0.0, 10.0),
            fade: self.fade.clamp(0.0, 10.0),
            ..*self
        }
    }
}

/// Semitones of pitch at a full (+-100 %) LFO 1 > Pitch amount.
pub const LFO_PITCH_SEMITONES: f32 = 12.0;
/// Pulse-width swing at a full LFO 1 > PWM amount (0.5 +- 0.45 covers the
/// whole 5..95 % range).
pub const LFO_PWM_RANGE: f32 = 0.45;
/// Octaves of cutoff at a full LFO 1 > Cutoff amount.
pub const LFO_CUTOFF_OCTAVES: f32 = 5.0;

/// LFO 1's hard-wired routes, each -1..=1 (negative inverts the LFO).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LfoRoutes {
    /// Both oscillators' pitch, x [`LFO_PITCH_SEMITONES`].
    pub pitch: f32,
    /// Both oscillators' pulse width, x [`LFO_PWM_RANGE`].
    pub pwm: f32,
    /// Filter cutoff, x [`LFO_CUTOFF_OCTAVES`].
    pub cutoff: f32,
}

pub use super::matrix::{ModDest, ModSlot, ModSource, MOD_SLOTS};

/// Most voices one note can stack in unison.
pub const MAX_UNISON: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceMode {
    Poly,
    /// One note at a time (last-note priority); every note retriggers the
    /// envelopes.
    Mono,
    /// One note at a time; overlapping notes only change pitch, so the
    /// envelopes carry on (retriggering only from no keys held).
    Legato,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlideMode {
    /// Every note glides from the previous one.
    Always,
    /// Only notes played while another key is held glide.
    Legato,
}

/// Where envelopes restart from when a mono/legato voice is retriggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retrigger {
    /// From the current level, as an analog envelope does (no click).
    FromCurrent,
    /// From zero, for a hard, percussive restart.
    FromZero,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChorusMode {
    Off,
    /// Juno-style chorus: wide and gentle.
    One,
    /// Juno-style chorus: faster and deeper.
    Two,
    /// Three-phase string-machine / Polysix ensemble.
    Ensemble,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChorusSettings {
    pub mode: ChorusMode,
    /// Multiplier on the mode's own rate, 0.1..=4.
    pub rate: f32,
    /// Multiplier on the mode's own depth, 0..=2.
    pub depth: f32,
    /// 0 = dry .. 1 = wet only (0.5 = equal parts).
    pub mix: f32,
    /// Wet-path low-pass, 1..=20 kHz (BBDs are band-limited).
    pub tone: f32,
}

impl Default for ChorusSettings {
    fn default() -> Self {
        Self { mode: ChorusMode::Off, rate: 1.0, depth: 1.0, mix: 0.5, tone: 8_000.0 }
    }
}

/// Cents of fixed per-voice pitch offset at full drift (voice tolerance).
pub const DRIFT_TOLERANCE_CENTS: f32 = 3.0;
/// Octaves of fixed per-voice cutoff offset at full drift.
pub const DRIFT_TOLERANCE_OCTAVES: f32 = 0.15;
/// Fraction of fixed per-voice envelope-time variation at full drift.
pub const DRIFT_TOLERANCE_ENV: f32 = 0.1;
/// Cents of slow random pitch wander at full drift.
pub const DRIFT_WANDER_CENTS: f32 = 5.0;
/// Octaves of slow random cutoff wander at full drift.
pub const DRIFT_WANDER_OCTAVES: f32 = 0.1;
/// Rate of the wander, Hz (each voice varies around it).
pub const DRIFT_RATE: f32 = 0.3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceSettings {
    pub mode: VoiceMode,
    /// 1..=[`MAX_UNISON`] voices per note (never more than the polyphony).
    pub unison: usize,
    /// 0..=100 cents: the outermost unison voices sit this far either side.
    pub detune: f32,
    /// 0..=1: the outermost unison voices pan this far either side.
    pub spread: f32,
    /// Glide time, 0..=10 s (0 = off): about the time to reach the new pitch.
    pub glide: f32,
    pub glide_mode: GlideMode,
    pub retrigger: Retrigger,
    /// 0..=1: per-voice tolerances and slow random wander ("vintage").
    pub drift: f32,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            mode: VoiceMode::Poly,
            unison: 1,
            detune: 15.0,
            spread: 0.5,
            glide: 0.0,
            glide_mode: GlideMode::Always,
            retrigger: Retrigger::FromCurrent,
            drift: 0.0,
        }
    }
}

/// Octaves of cutoff that a full (+-100 %) filter envelope amount sweeps:
/// the whole 20 Hz..20 kHz range.
pub const ENV_AMOUNT_OCTAVES: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Patch {
    pub osc1: OscSettings,
    pub osc2: OscSettings,
    pub sub: SubSettings,
    pub noise: NoiseSettings,
    /// Hard sync: OSC 2 restarts every time OSC 1 starts a cycle, so it
    /// sounds at OSC 1's pitch with a timbre set by its own (the classic
    /// sync lead when OSC 2's pitch is swept).
    pub osc2_sync: bool,
    /// Cross-mod: linear through-zero FM of OSC 2 by OSC 1, as an FM index
    /// (0..=5; frequency deviation = index x OSC 1's frequency). Sine into
    /// sine gives DX-style bells and electric pianos.
    pub cross_mod: f32,
    /// Ring mod (OSC 1 x OSC 2) mixer level, 0..=1.
    pub ring_level: f32,
    pub filter: FilterSettings,
    /// ENV 1: amp.
    pub amp_env: EnvSettings,
    /// ENV 2: filter (scaled by [`FilterSettings::env_amount`]).
    pub filter_env: EnvSettings,
    /// ENV 3: free, for the mod matrix (phase 5).
    pub mod_env: EnvSettings,
    pub lfo1: LfoSettings,
    /// LFO 2: free, for the mod matrix (phase 5).
    pub lfo2: LfoSettings,
    /// LFO 1's hard-wired routes.
    pub lfo1_to: LfoRoutes,
    pub mod_slots: [ModSlot; MOD_SLOTS],
    /// Pitch bend range, 0..=24 semitones each way.
    pub bend_range: f32,
    pub voice: VoiceSettings,
    pub chorus: ChorusSettings,
    /// Soft-clip the output (transparent below 0.8, never past full scale).
    pub output_clip: bool,
    /// HQ: run the voices at 2x the sample rate (less aliasing from drive,
    /// sync, FM and high resonance; about twice the CPU).
    pub hq: bool,
    /// Voices that may sound at once, 1..=[`MAX_VOICES`].
    pub polyphony: usize,
    /// Master output gain (linear).
    pub volume: f32,
}

impl Default for Patch {
    /// A single sawtooth, the classic starting point.
    fn default() -> Self {
        let osc = OscSettings {
            wave: Waveform::Saw,
            octave: 0,
            semitone: 0.0,
            fine: 0.0,
            pulse_width: 0.5,
            level: 1.0,
            phase: PhaseMode::Free,
        };
        Self {
            osc1: osc,
            // Slightly detuned so turning it up gives an immediate chorus.
            osc2: OscSettings { fine: 7.0, level: 0.0, ..osc },
            sub: SubSettings { wave: SubWave::Square, octave: SubOctave::Down1, level: 0.0 },
            noise: NoiseSettings { color: NoiseColor::White, level: 0.0 },
            osc2_sync: false,
            cross_mod: 0.0,
            ring_level: 0.0,
            // Wide open: the init patch is close to the raw oscillators.
            filter: FilterSettings {
                model: FilterModel::Ladder,
                cutoff: 20_000.0,
                resonance: 0.0,
                drive_db: 0.0,
                key_track: 0.0,
                velocity: 0.0,
                bass_comp: 0.0,
                env_amount: 0.0,
                hpf_cutoff: super::filter::HPF_OFF_HZ,
            },
            // Organ-like amp: quick in, full sustain, short release, level
            // following velocity.
            amp_env: EnvSettings {
                delay: 0.0,
                attack: 0.005,
                hold: 0.0,
                decay: 0.3,
                sustain: 1.0,
                release: 0.08,
                attack_curve: 1.0,
                decay_curve: 1.0,
                velocity_level: 1.0,
                velocity_attack: 0.0,
            },
            // A typical filter sweep shape, ready for the env amount.
            filter_env: EnvSettings {
                delay: 0.0,
                attack: 0.005,
                hold: 0.0,
                decay: 0.4,
                sustain: 0.3,
                release: 0.2,
                attack_curve: 1.0,
                decay_curve: 1.0,
                velocity_level: 0.0,
                velocity_attack: 0.0,
            },
            mod_env: EnvSettings {
                delay: 0.0,
                attack: 0.005,
                hold: 0.0,
                decay: 0.4,
                sustain: 0.3,
                release: 0.2,
                attack_curve: 1.0,
                decay_curve: 1.0,
                velocity_level: 0.0,
                velocity_attack: 0.0,
            },
            lfo1: LfoSettings::default(),
            lfo2: LfoSettings { shape: LfoShape::Sine, rate: 0.5, ..LfoSettings::default() },
            lfo1_to: LfoRoutes::default(),
            // Two classic routings, silent until the wheel / pressure move:
            // the mod wheel brings in vibrato, aftertouch opens the filter.
            mod_slots: {
                let mut slots = [ModSlot::OFF; MOD_SLOTS];
                slots[0] = ModSlot { source: ModSource::Lfo1, dest: ModDest::Pitch, amount: 0.02, via: ModSource::ModWheel };
                slots[1] = ModSlot { source: ModSource::Aftertouch, dest: ModDest::Cutoff, amount: 0.2, via: ModSource::None };
                slots
            },
            bend_range: 2.0,
            voice: VoiceSettings::default(),
            chorus: ChorusSettings::default(),
            output_clip: false,
            hq: false,
            polyphony: 8,
            volume: crate::db_to_gain(-12.0),
        }
    }
}

impl Patch {
    /// The patch with every value clamped to its documented range.
    pub fn sanitized(&self) -> Self {
        let osc = |o: &OscSettings| OscSettings {
            octave: o.octave.clamp(-1, 2),
            semitone: o.semitone.clamp(-12.0, 12.0),
            fine: o.fine.clamp(-100.0, 100.0),
            pulse_width: o.pulse_width.clamp(0.05, 0.95),
            level: o.level.clamp(0.0, 1.0),
            ..*o
        };
        Self {
            osc1: osc(&self.osc1),
            osc2: osc(&self.osc2),
            sub: SubSettings { level: self.sub.level.clamp(0.0, 1.0), ..self.sub },
            noise: NoiseSettings { level: self.noise.level.clamp(0.0, 1.0), ..self.noise },
            osc2_sync: self.osc2_sync,
            cross_mod: self.cross_mod.clamp(0.0, super::osc::MAX_CROSS_MOD),
            ring_level: self.ring_level.clamp(0.0, 1.0),
            filter: FilterSettings {
                cutoff: self.filter.cutoff.clamp(20.0, 20_000.0),
                resonance: self.filter.resonance.clamp(0.0, 1.0),
                drive_db: self.filter.drive_db.clamp(0.0, super::filter::MAX_DRIVE_DB),
                key_track: self.filter.key_track.clamp(0.0, 2.0),
                velocity: self.filter.velocity.clamp(-1.0, 1.0),
                bass_comp: self.filter.bass_comp.clamp(0.0, 1.0),
                env_amount: self.filter.env_amount.clamp(-1.0, 1.0),
                hpf_cutoff: self.filter.hpf_cutoff.clamp(super::filter::HPF_OFF_HZ, 2_000.0),
                ..self.filter
            },
            amp_env: self.amp_env.sanitized(),
            filter_env: self.filter_env.sanitized(),
            mod_env: self.mod_env.sanitized(),
            lfo1: self.lfo1.sanitized(),
            lfo2: self.lfo2.sanitized(),
            lfo1_to: LfoRoutes {
                pitch: self.lfo1_to.pitch.clamp(-1.0, 1.0),
                pwm: self.lfo1_to.pwm.clamp(-1.0, 1.0),
                cutoff: self.lfo1_to.cutoff.clamp(-1.0, 1.0),
            },
            mod_slots: self.mod_slots.map(|s| ModSlot { amount: s.amount.clamp(-1.0, 1.0), ..s }),
            bend_range: self.bend_range.clamp(0.0, 24.0),
            voice: VoiceSettings {
                unison: self.voice.unison.clamp(1, MAX_UNISON),
                detune: self.voice.detune.clamp(0.0, 100.0),
                spread: self.voice.spread.clamp(0.0, 1.0),
                glide: self.voice.glide.clamp(0.0, 10.0),
                drift: self.voice.drift.clamp(0.0, 1.0),
                ..self.voice
            },
            chorus: ChorusSettings {
                rate: self.chorus.rate.clamp(0.1, 4.0),
                depth: self.chorus.depth.clamp(0.0, super::chorus::MAX_DEPTH),
                mix: self.chorus.mix.clamp(0.0, 1.0),
                tone: self.chorus.tone.clamp(1_000.0, 20_000.0),
                ..self.chorus
            },
            output_clip: self.output_clip,
            hq: self.hq,
            polyphony: self.polyphony.clamp(1, MAX_VOICES),
            volume: self.volume.max(0.0),
        }
    }
}
