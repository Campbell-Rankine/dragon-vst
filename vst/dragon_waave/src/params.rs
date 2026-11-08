//! Parameters, mapped onto `dsp_core::synth::Patch` by [`DragonWaaveParams::patch`].
//!
//! The `#[id = "..."]` strings and enum `#[id]`s are saved in host projects
//! and automation. Never rename or reuse one; add new IDs instead.
//! Oscillator parameters get an `osc1_` / `osc2_` prefix from
//! `#[nested(id_prefix = ...)]`, e.g. `osc1_wave`. Later sections follow the
//! same scheme (`flt_`, `env1_`, `lfo1_`, `mod1_`, ...); see
//! `docs/dragon_waave.md`.

use dsp_core::synth::filter::{HPF_OFF_HZ, MAX_DRIVE_DB};
use dsp_core::synth::{
    ChorusMode, ChorusSettings, EnvSettings, FilterModel, FilterSettings, GlideMode, LfoMode, LfoRoutes, LfoSettings,
    LfoShape, ModDest, ModSlot, ModSource, NoiseColor, NoiseSettings, OscSettings, Patch, PhaseMode, Retrigger,
    SubOctave, SubSettings, SubWave, VoiceMode, VoiceSettings, Waveform, MAX_CROSS_MOD, MAX_UNISON, MAX_VOICES,
    MOD_SLOTS,
};
use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::sync::Arc;

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum WaveParam {
    #[id = "saw"]
    Saw,
    #[id = "pulse"]
    Pulse,
    #[id = "triangle"]
    Triangle,
    #[id = "sine"]
    Sine,
}

impl From<WaveParam> for Waveform {
    fn from(w: WaveParam) -> Self {
        match w {
            WaveParam::Saw => Waveform::Saw,
            WaveParam::Pulse => Waveform::Pulse,
            WaveParam::Triangle => Waveform::Triangle,
            WaveParam::Sine => Waveform::Sine,
        }
    }
}

/// Octave in organ footage, as on the Polysix.
#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum Footage {
    #[id = "16"]
    #[name = "16'"]
    F16,
    #[id = "8"]
    #[name = "8'"]
    F8,
    #[id = "4"]
    #[name = "4'"]
    F4,
    #[id = "2"]
    #[name = "2'"]
    F2,
}

impl Footage {
    /// Octaves relative to 8'.
    fn octave(self) -> i32 {
        match self {
            Footage::F16 => -1,
            Footage::F8 => 0,
            Footage::F4 => 1,
            Footage::F2 => 2,
        }
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum PhaseParam {
    #[id = "free"]
    Free,
    #[id = "reset"]
    Reset,
}

impl From<PhaseParam> for PhaseMode {
    fn from(p: PhaseParam) -> Self {
        match p {
            PhaseParam::Free => PhaseMode::Free,
            PhaseParam::Reset => PhaseMode::Reset,
        }
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum SubWaveParam {
    #[id = "square"]
    Square,
    #[id = "pulse"]
    #[name = "Pulse 25%"]
    Pulse,
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum SubOctaveParam {
    #[id = "down1"]
    #[name = "-1 Oct"]
    Down1,
    #[id = "down2"]
    #[name = "-2 Oct"]
    Down2,
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum NoiseColorParam {
    #[id = "white"]
    White,
    #[id = "pink"]
    Pink,
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum FilterTypeParam {
    #[id = "off"]
    Off,
    #[id = "lp24"]
    #[name = "LP24 Six"]
    Ladder,
    #[id = "svf_lp"]
    #[name = "SVF LP"]
    SvfLowPass,
    #[id = "svf_bp"]
    #[name = "SVF BP"]
    SvfBandPass,
    #[id = "svf_hp"]
    #[name = "SVF HP"]
    SvfHighPass,
    #[id = "svf_notch"]
    #[name = "SVF Notch"]
    SvfNotch,
    #[id = "ms_lp"]
    #[name = "MS LP"]
    MsLowPass,
    #[id = "ms_hp"]
    #[name = "MS HP"]
    MsHighPass,
}

impl From<FilterTypeParam> for FilterModel {
    fn from(t: FilterTypeParam) -> Self {
        match t {
            FilterTypeParam::Off => FilterModel::Off,
            FilterTypeParam::Ladder => FilterModel::Ladder,
            FilterTypeParam::SvfLowPass => FilterModel::SvfLowPass,
            FilterTypeParam::SvfBandPass => FilterModel::SvfBandPass,
            FilterTypeParam::SvfHighPass => FilterModel::SvfHighPass,
            FilterTypeParam::SvfNotch => FilterModel::SvfNotch,
            FilterTypeParam::MsLowPass => FilterModel::MsLowPass,
            FilterTypeParam::MsHighPass => FilterModel::MsHighPass,
        }
    }
}

/// A cutoff frequency, 20 Hz upward, with a log-feeling knob.
fn cutoff_param(name: &str, default: f32, max: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed {
            min: 20.0,
            max,
            factor: FloatRange::skew_factor(-2.2),
        },
    )
    .with_smoother(SmoothingStyle::Logarithmic(20.0))
    .with_value_to_string(formatters::v2s_f32_hz_then_khz(1))
    .with_string_to_value(formatters::s2v_f32_hz_then_khz())
}

/// A percentage stored as a fraction, over `min..=max` (e.g. -1..1 = -100..100 %).
fn percent_param(name: &str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min, max })
        .with_smoother(SmoothingStyle::Linear(10.0))
        .with_unit("%")
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum LfoShapeParam {
    #[id = "sine"]
    Sine,
    #[id = "triangle"]
    Triangle,
    #[id = "saw_up"]
    #[name = "Saw Up"]
    SawUp,
    #[id = "saw_down"]
    #[name = "Saw Down"]
    SawDown,
    #[id = "square"]
    Square,
    #[id = "sample_hold"]
    #[name = "Sample & Hold"]
    SampleHold,
    #[id = "smooth_random"]
    #[name = "Smooth Random"]
    SmoothRandom,
}

impl From<LfoShapeParam> for LfoShape {
    fn from(s: LfoShapeParam) -> Self {
        match s {
            LfoShapeParam::Sine => LfoShape::Sine,
            LfoShapeParam::Triangle => LfoShape::Triangle,
            LfoShapeParam::SawUp => LfoShape::SawUp,
            LfoShapeParam::SawDown => LfoShape::SawDown,
            LfoShapeParam::Square => LfoShape::Square,
            LfoShapeParam::SampleHold => LfoShape::SampleHold,
            LfoShapeParam::SmoothRandom => LfoShape::SmoothRandom,
        }
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum LfoModeParam {
    #[id = "mono"]
    Mono,
    #[id = "poly"]
    Poly,
    #[id = "poly_retrig"]
    #[name = "Poly Retrig"]
    PolyRetrigger,
}

impl From<LfoModeParam> for LfoMode {
    fn from(m: LfoModeParam) -> Self {
        match m {
            LfoModeParam::Mono => LfoMode::Mono,
            LfoModeParam::Poly => LfoMode::Poly,
            LfoModeParam::PolyRetrigger => LfoMode::PolyRetrigger,
        }
    }
}

/// Tempo-synced LFO cycle lengths. D = dotted (x 1.5), T = triplet (x 2/3).
#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum Division {
    #[id = "4/1"]
    #[name = "4 bars"]
    Bars4,
    #[id = "2/1"]
    #[name = "2 bars"]
    Bars2,
    #[id = "1/1"]
    #[name = "1/1"]
    Whole,
    #[id = "1/2"]
    #[name = "1/2"]
    Half,
    #[id = "1/2d"]
    #[name = "1/2 D"]
    HalfDotted,
    #[id = "1/2t"]
    #[name = "1/2 T"]
    HalfTriplet,
    #[id = "1/4"]
    #[name = "1/4"]
    Quarter,
    #[id = "1/4d"]
    #[name = "1/4 D"]
    QuarterDotted,
    #[id = "1/4t"]
    #[name = "1/4 T"]
    QuarterTriplet,
    #[id = "1/8"]
    #[name = "1/8"]
    Eighth,
    #[id = "1/8d"]
    #[name = "1/8 D"]
    EighthDotted,
    #[id = "1/8t"]
    #[name = "1/8 T"]
    EighthTriplet,
    #[id = "1/16"]
    #[name = "1/16"]
    Sixteenth,
    #[id = "1/16d"]
    #[name = "1/16 D"]
    SixteenthDotted,
    #[id = "1/16t"]
    #[name = "1/16 T"]
    SixteenthTriplet,
    #[id = "1/32"]
    #[name = "1/32"]
    ThirtySecond,
}

impl Division {
    /// Length of one LFO cycle in quarter-note beats (4/4 time).
    pub fn beats(self) -> f32 {
        match self {
            Division::Bars4 => 16.0,
            Division::Bars2 => 8.0,
            Division::Whole => 4.0,
            Division::Half => 2.0,
            Division::HalfDotted => 3.0,
            Division::HalfTriplet => 4.0 / 3.0,
            Division::Quarter => 1.0,
            Division::QuarterDotted => 1.5,
            Division::QuarterTriplet => 2.0 / 3.0,
            Division::Eighth => 0.5,
            Division::EighthDotted => 0.75,
            Division::EighthTriplet => 1.0 / 3.0,
            Division::Sixteenth => 0.25,
            Division::SixteenthDotted => 0.375,
            Division::SixteenthTriplet => 1.0 / 6.0,
            Division::ThirtySecond => 0.125,
        }
    }

    /// LFO rate in Hz at `bpm`.
    pub fn hz(self, bpm: f32) -> f32 {
        bpm / 60.0 / self.beats()
    }
}

/// One LFO's parameters. Nested with an id prefix (`lfo1_`).
#[derive(Params)]
pub struct LfoParams {
    #[id = "shape"]
    pub shape: EnumParam<LfoShapeParam>,
    #[id = "rate"]
    pub rate: FloatParam,
    #[id = "sync"]
    pub sync: BoolParam,
    #[id = "division"]
    pub division: EnumParam<Division>,
    #[id = "phase"]
    pub phase: FloatParam,
    #[id = "delay"]
    pub delay: FloatParam,
    #[id = "fade"]
    pub fade: FloatParam,
    #[id = "mode"]
    pub mode: EnumParam<LfoModeParam>,
}

impl LfoParams {
    fn new(name: &str, d: &LfoSettings) -> Self {
        let shape = match d.shape {
            LfoShape::Sine => LfoShapeParam::Sine,
            LfoShape::Triangle => LfoShapeParam::Triangle,
            LfoShape::SawUp => LfoShapeParam::SawUp,
            LfoShape::SawDown => LfoShapeParam::SawDown,
            LfoShape::Square => LfoShapeParam::Square,
            LfoShape::SampleHold => LfoShapeParam::SampleHold,
            LfoShape::SmoothRandom => LfoShapeParam::SmoothRandom,
        };
        let mode = match d.mode {
            LfoMode::Mono => LfoModeParam::Mono,
            LfoMode::Poly => LfoModeParam::Poly,
            LfoMode::PolyRetrigger => LfoModeParam::PolyRetrigger,
        };
        Self {
            shape: EnumParam::new(format!("{name} Shape"), shape),
            rate: FloatParam::new(
                format!("{name} Rate"),
                d.rate,
                FloatRange::Skewed {
                    min: 0.01,
                    max: 50.0,
                    factor: FloatRange::skew_factor(-2.5),
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            sync: BoolParam::new(format!("{name} Sync"), false),
            division: EnumParam::new(format!("{name} Division"), Division::Quarter),
            phase: FloatParam::new(
                format!("{name} Phase"),
                d.phase,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit("°")
            .with_value_to_string(Arc::new(|v| format!("{:.0}", v * 360.0)))
            .with_string_to_value(Arc::new(|s| {
                s.trim()
                    .trim_end_matches('°')
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .map(|deg| (deg / 360.0).clamp(0.0, 1.0))
            })),
            delay: time_param(format!("{name} Delay"), d.delay, 0.0, 10.0),
            fade: time_param(format!("{name} Fade In"), d.fade, 0.0, 10.0),
            mode: EnumParam::new(format!("{name} Mode"), mode),
        }
    }

    /// Settings at `bpm` (used when synced).
    pub(crate) fn settings(&self, bpm: f32) -> LfoSettings {
        LfoSettings {
            shape: self.shape.value().into(),
            rate: if self.sync.value() {
                self.division.value().hz(bpm)
            } else {
                self.rate.value()
            },
            phase: self.phase.value(),
            delay: self.delay.value(),
            fade: self.fade.value(),
            mode: self.mode.value().into(),
        }
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum SourceParam {
    #[id = "none"]
    None,
    #[id = "lfo1"]
    #[name = "LFO 1"]
    Lfo1,
    #[id = "lfo2"]
    #[name = "LFO 2"]
    Lfo2,
    #[id = "env1"]
    #[name = "Amp Env"]
    AmpEnv,
    #[id = "env2"]
    #[name = "Filter Env"]
    FilterEnv,
    #[id = "env3"]
    #[name = "Mod Env"]
    ModEnv,
    #[id = "velocity"]
    Velocity,
    #[id = "key"]
    Key,
    #[id = "mod_wheel"]
    #[name = "Mod Wheel"]
    ModWheel,
    #[id = "aftertouch"]
    Aftertouch,
    #[id = "pitch_bend"]
    #[name = "Pitch Bend"]
    PitchBend,
    #[id = "random"]
    Random,
    #[id = "drift"]
    Drift,
}

impl SourceParam {
    const ALL: [(SourceParam, ModSource); 13] = [
        (SourceParam::None, ModSource::None),
        (SourceParam::Lfo1, ModSource::Lfo1),
        (SourceParam::Lfo2, ModSource::Lfo2),
        (SourceParam::AmpEnv, ModSource::AmpEnv),
        (SourceParam::FilterEnv, ModSource::FilterEnv),
        (SourceParam::ModEnv, ModSource::ModEnv),
        (SourceParam::Velocity, ModSource::Velocity),
        (SourceParam::Key, ModSource::Key),
        (SourceParam::ModWheel, ModSource::ModWheel),
        (SourceParam::Aftertouch, ModSource::Aftertouch),
        (SourceParam::PitchBend, ModSource::PitchBend),
        (SourceParam::Random, ModSource::Random),
        (SourceParam::Drift, ModSource::Drift),
    ];

    fn to_engine(self) -> ModSource {
        Self::ALL
            .iter()
            .find(|(p, _)| *p == self)
            .map_or(ModSource::None, |(_, s)| *s)
    }

    fn from_engine(s: ModSource) -> Self {
        Self::ALL
            .iter()
            .find(|(_, e)| *e == s)
            .map_or(SourceParam::None, |(p, _)| *p)
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum DestParam {
    #[id = "none"]
    None,
    #[id = "pitch"]
    Pitch,
    #[id = "osc1_pitch"]
    #[name = "Osc 1 Pitch"]
    Osc1Pitch,
    #[id = "osc2_pitch"]
    #[name = "Osc 2 Pitch"]
    Osc2Pitch,
    #[id = "osc1_pw"]
    #[name = "Osc 1 PW"]
    Osc1PulseWidth,
    #[id = "osc2_pw"]
    #[name = "Osc 2 PW"]
    Osc2PulseWidth,
    #[id = "xmod"]
    #[name = "Cross Mod"]
    CrossMod,
    #[id = "osc1_level"]
    #[name = "Osc 1 Level"]
    Osc1Level,
    #[id = "osc2_level"]
    #[name = "Osc 2 Level"]
    Osc2Level,
    #[id = "sub_level"]
    #[name = "Sub Level"]
    SubLevel,
    #[id = "noise_level"]
    #[name = "Noise Level"]
    NoiseLevel,
    #[id = "ring_level"]
    #[name = "Ring Level"]
    RingLevel,
    #[id = "cutoff"]
    Cutoff,
    #[id = "resonance"]
    Resonance,
    #[id = "drive"]
    Drive,
    #[id = "hpf"]
    #[name = "HPF Cutoff"]
    HpfCutoff,
    #[id = "amp"]
    Amp,
    #[id = "pan"]
    Pan,
    #[id = "lfo1_rate"]
    #[name = "LFO 1 Rate"]
    Lfo1Rate,
    #[id = "lfo2_rate"]
    #[name = "LFO 2 Rate"]
    Lfo2Rate,
    #[id = "env_attack"]
    #[name = "Env Attack"]
    EnvAttack,
    #[id = "env_decay"]
    #[name = "Env Decay"]
    EnvDecay,
}

impl DestParam {
    const ALL: [(DestParam, ModDest); 22] = [
        (DestParam::None, ModDest::None),
        (DestParam::Pitch, ModDest::Pitch),
        (DestParam::Osc1Pitch, ModDest::Osc1Pitch),
        (DestParam::Osc2Pitch, ModDest::Osc2Pitch),
        (DestParam::Osc1PulseWidth, ModDest::Osc1PulseWidth),
        (DestParam::Osc2PulseWidth, ModDest::Osc2PulseWidth),
        (DestParam::CrossMod, ModDest::CrossMod),
        (DestParam::Osc1Level, ModDest::Osc1Level),
        (DestParam::Osc2Level, ModDest::Osc2Level),
        (DestParam::SubLevel, ModDest::SubLevel),
        (DestParam::NoiseLevel, ModDest::NoiseLevel),
        (DestParam::RingLevel, ModDest::RingLevel),
        (DestParam::Cutoff, ModDest::Cutoff),
        (DestParam::Resonance, ModDest::Resonance),
        (DestParam::Drive, ModDest::Drive),
        (DestParam::HpfCutoff, ModDest::HpfCutoff),
        (DestParam::Amp, ModDest::Amp),
        (DestParam::Pan, ModDest::Pan),
        (DestParam::Lfo1Rate, ModDest::Lfo1Rate),
        (DestParam::Lfo2Rate, ModDest::Lfo2Rate),
        (DestParam::EnvAttack, ModDest::EnvAttack),
        (DestParam::EnvDecay, ModDest::EnvDecay),
    ];

    fn to_engine(self) -> ModDest {
        Self::ALL
            .iter()
            .find(|(p, _)| *p == self)
            .map_or(ModDest::None, |(_, d)| *d)
    }

    fn from_engine(d: ModDest) -> Self {
        Self::ALL
            .iter()
            .find(|(_, e)| *e == d)
            .map_or(DestParam::None, |(p, _)| *p)
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum VoiceModeParam {
    #[id = "poly"]
    Poly,
    #[id = "mono"]
    Mono,
    #[id = "legato"]
    Legato,
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum GlideModeParam {
    #[id = "always"]
    Always,
    #[id = "legato"]
    Legato,
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum RetriggerParam {
    #[id = "current"]
    #[name = "From Current"]
    FromCurrent,
    #[id = "zero"]
    #[name = "From Zero"]
    FromZero,
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum ChorusModeParam {
    #[id = "off"]
    Off,
    #[id = "i"]
    #[name = "I"]
    One,
    #[id = "ii"]
    #[name = "II"]
    Two,
    #[id = "ensemble"]
    Ensemble,
}

/// One mod matrix slot. Nested with an id prefix (`mod1_` .. `mod6_`).
#[derive(Params)]
pub struct ModSlotParams {
    #[id = "src"]
    pub source: EnumParam<SourceParam>,
    #[id = "dest"]
    pub dest: EnumParam<DestParam>,
    #[id = "amt"]
    pub amount: FloatParam,
    #[id = "via"]
    pub via: EnumParam<SourceParam>,
}

impl ModSlotParams {
    fn new(n: usize, d: &ModSlot) -> Self {
        Self {
            source: EnumParam::new(format!("Mod {n} Source"), SourceParam::from_engine(d.source)),
            dest: EnumParam::new(format!("Mod {n} Destination"), DestParam::from_engine(d.dest)),
            amount: percent_param(&format!("Mod {n} Amount"), d.amount, -1.0, 1.0).with_step_size(0.001),
            via: EnumParam::new(format!("Mod {n} Via"), SourceParam::from_engine(d.via)),
        }
    }

    fn slot(&self, steps: Read) -> ModSlot {
        ModSlot {
            source: self.source.value().to_engine(),
            dest: self.dest.value().to_engine(),
            amount: smoothed(&self.amount, steps),
            via: self.via.value().to_engine(),
        }
    }
}

/// A time in seconds over `min..=max`, skewed so short times get most of
/// the travel, shown as ms below 1 s.
fn time_param(name: String, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed {
            min,
            max,
            factor: FloatRange::skew_factor(-2.5),
        },
    )
    .with_value_to_string(Arc::new(|v| {
        if v < 1.0 {
            format!("{:.1} ms", v * 1_000.0)
        } else {
            format!("{v:.2} s")
        }
    }))
    .with_string_to_value(Arc::new(|s| {
        let s = s.trim().to_ascii_lowercase();
        match s.strip_suffix("ms") {
            Some(ms) => ms.trim().parse::<f32>().ok().map(|v| v / 1_000.0),
            None => s.trim_end_matches('s').trim().parse().ok(),
        }
    }))
}

/// One envelope's parameters. Nested with an id prefix (`env1_`, `env2_`).
#[derive(Params)]
pub struct EnvParams {
    #[id = "delay"]
    pub delay: FloatParam,
    #[id = "attack"]
    pub attack: FloatParam,
    #[id = "hold"]
    pub hold: FloatParam,
    #[id = "decay"]
    pub decay: FloatParam,
    #[id = "sustain"]
    pub sustain: FloatParam,
    #[id = "release"]
    pub release: FloatParam,
    #[id = "atk_curve"]
    pub attack_curve: FloatParam,
    #[id = "dec_curve"]
    pub decay_curve: FloatParam,
    #[id = "vel"]
    pub velocity_level: FloatParam,
    #[id = "vel_atk"]
    pub velocity_attack: FloatParam,
}

impl EnvParams {
    fn new(name: &str, d: &EnvSettings) -> Self {
        Self {
            delay: time_param(format!("{name} Delay"), d.delay, 0.0, 10.0),
            attack: time_param(format!("{name} Attack"), d.attack, 0.000_5, 20.0),
            hold: time_param(format!("{name} Hold"), d.hold, 0.0, 10.0),
            decay: time_param(format!("{name} Decay"), d.decay, 0.001, 20.0),
            sustain: percent_param(&format!("{name} Sustain"), d.sustain, 0.0, 1.0),
            release: time_param(format!("{name} Release"), d.release, 0.001, 20.0),
            attack_curve: percent_param(&format!("{name} Attack Curve"), d.attack_curve, 0.0, 1.0),
            decay_curve: percent_param(&format!("{name} Decay Curve"), d.decay_curve, 0.0, 1.0),
            velocity_level: percent_param(&format!("{name} Velocity"), d.velocity_level, 0.0, 1.0),
            velocity_attack: percent_param(&format!("{name} Vel > Attack"), d.velocity_attack, 0.0, 1.0),
        }
    }

    pub(crate) fn settings(&self, steps: Read) -> EnvSettings {
        EnvSettings {
            delay: self.delay.value(),
            attack: self.attack.value(),
            hold: self.hold.value(),
            decay: self.decay.value(),
            sustain: smoothed(&self.sustain, steps),
            release: self.release.value(),
            attack_curve: smoothed(&self.attack_curve, steps),
            decay_curve: smoothed(&self.decay_curve, steps),
            // Read once per note at note-on, so they apply at once (unsmoothed).
            velocity_level: self.velocity_level.value(),
            velocity_attack: self.velocity_attack.value(),
        }
    }
}

/// How [`DragonWaaveParams::patch`] reads smoothed parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Read {
    /// The target values, ignoring smoothing. For before processing starts:
    /// smoothers read 0 until the host primes them.
    Target,
    /// Where the smoothers are now, without advancing them. For notes that
    /// start between blocks, so they see the same values the block will.
    Current,
    /// Advance the smoothers by this many samples (one block).
    Advance(u32),
}

/// A float parameter's value, read as `read` says.
fn smoothed(p: &FloatParam, read: Read) -> f32 {
    match read {
        Read::Target => p.value(),
        Read::Current => p.smoothed.previous_value(),
        Read::Advance(steps) => p.smoothed.next_step(steps),
    }
}

/// A 0..100 % mixer level stored as 0..1.
fn level_param(name: String, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_smoother(SmoothingStyle::Linear(10.0))
        .with_unit("%")
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

#[derive(Params)]
pub struct OscParams {
    #[id = "wave"]
    pub wave: EnumParam<WaveParam>,
    #[id = "oct"]
    pub octave: EnumParam<Footage>,
    #[id = "semi"]
    pub semitone: IntParam,
    #[id = "fine"]
    pub fine: FloatParam,
    #[id = "pw"]
    pub pulse_width: FloatParam,
    #[id = "level"]
    pub level: FloatParam,
    #[id = "phase"]
    pub phase: EnumParam<PhaseParam>,
}

impl OscParams {
    fn new(name: &str, defaults: &OscSettings) -> Self {
        let footage = match defaults.octave {
            -1 => Footage::F16,
            1 => Footage::F4,
            2 => Footage::F2,
            _ => Footage::F8,
        };
        Self {
            wave: EnumParam::new(format!("{name} Wave"), WaveParam::Saw),
            octave: EnumParam::new(format!("{name} Octave"), footage),
            semitone: IntParam::new(
                format!("{name} Semitone"),
                defaults.semitone as i32,
                IntRange::Linear { min: -12, max: 12 },
            )
            .with_unit(" st"),
            fine: FloatParam::new(
                format!("{name} Fine"),
                defaults.fine,
                FloatRange::Linear {
                    min: -100.0,
                    max: 100.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(10.0))
            .with_unit(" ct")
            .with_step_size(0.1),
            pulse_width: FloatParam::new(
                format!("{name} Pulse Width"),
                defaults.pulse_width,
                FloatRange::Linear { min: 0.05, max: 0.95 },
            )
            .with_smoother(SmoothingStyle::Linear(10.0))
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            level: level_param(format!("{name} Level"), defaults.level),
            phase: EnumParam::new(format!("{name} Phase"), PhaseParam::Free),
        }
    }

    fn settings(&self, steps: Read) -> OscSettings {
        OscSettings {
            wave: self.wave.value().into(),
            octave: self.octave.value().octave(),
            semitone: self.semitone.value() as f32,
            fine: smoothed(&self.fine, steps),
            pulse_width: smoothed(&self.pulse_width, steps),
            level: smoothed(&self.level, steps),
            phase: self.phase.value().into(),
        }
    }
}

#[derive(Params)]
pub struct DragonWaaveParams {
    /// Editor window size, persisted with the plugin state.
    #[persist = "editor-state"]
    pub editor_state: Arc<EguiState>,

    #[nested(id_prefix = "osc1", group = "Osc 1")]
    pub osc1: OscParams,
    #[nested(id_prefix = "osc2", group = "Osc 2")]
    pub osc2: OscParams,

    /// Hard sync of OSC 2 to OSC 1.
    #[id = "osc2_sync"]
    pub osc2_sync: BoolParam,
    /// Through-zero FM of OSC 2 by OSC 1, as an FM index.
    #[id = "osc2_xmod"]
    pub cross_mod: FloatParam,
    /// OSC 1 x OSC 2 mixer level.
    #[id = "mix_ring"]
    pub ring_level: FloatParam,

    #[id = "sub_wave"]
    pub sub_wave: EnumParam<SubWaveParam>,
    #[id = "sub_oct"]
    pub sub_octave: EnumParam<SubOctaveParam>,
    #[id = "sub_level"]
    pub sub_level: FloatParam,

    #[id = "noise_color"]
    pub noise_color: EnumParam<NoiseColorParam>,
    #[id = "noise_level"]
    pub noise_level: FloatParam,

    #[id = "flt_type"]
    pub filter_type: EnumParam<FilterTypeParam>,
    #[id = "flt_cutoff"]
    pub cutoff: FloatParam,
    #[id = "flt_res"]
    pub resonance: FloatParam,
    #[id = "flt_drive"]
    pub drive: FloatParam,
    #[id = "flt_keytrack"]
    pub key_track: FloatParam,
    #[id = "flt_vel"]
    pub velocity_track: FloatParam,
    #[id = "flt_bass_comp"]
    pub bass_comp: FloatParam,
    #[id = "flt_env_amt"]
    pub env_amount: FloatParam,
    #[id = "hpf_cutoff"]
    pub hpf_cutoff: FloatParam,

    /// ENV 1: amp.
    #[nested(id_prefix = "env1", group = "Amp Env")]
    pub amp_env: EnvParams,
    /// ENV 2: filter.
    #[nested(id_prefix = "env2", group = "Filter Env")]
    pub filter_env: EnvParams,

    #[nested(id_prefix = "lfo1", group = "LFO 1")]
    pub lfo1: LfoParams,
    /// LFO 1's hard-wired routes.
    #[id = "lfo1_pitch"]
    pub lfo1_pitch: FloatParam,
    #[id = "lfo1_pwm"]
    pub lfo1_pwm: FloatParam,
    #[id = "lfo1_cutoff"]
    pub lfo1_cutoff: FloatParam,

    /// ENV 3: free, for the mod matrix.
    #[nested(id_prefix = "env3", group = "Mod Env")]
    pub mod_env: EnvParams,
    /// LFO 2: free, for the mod matrix.
    #[nested(id_prefix = "lfo2", group = "LFO 2")]
    pub lfo2: LfoParams,

    #[nested(id_prefix = "mod1", group = "Mod 1")]
    pub mod1: ModSlotParams,
    #[nested(id_prefix = "mod2", group = "Mod 2")]
    pub mod2: ModSlotParams,
    #[nested(id_prefix = "mod3", group = "Mod 3")]
    pub mod3: ModSlotParams,
    #[nested(id_prefix = "mod4", group = "Mod 4")]
    pub mod4: ModSlotParams,
    #[nested(id_prefix = "mod5", group = "Mod 5")]
    pub mod5: ModSlotParams,
    #[nested(id_prefix = "mod6", group = "Mod 6")]
    pub mod6: ModSlotParams,

    #[id = "voice_bend_range"]
    pub bend_range: IntParam,
    #[id = "voice_mode"]
    pub voice_mode: EnumParam<VoiceModeParam>,
    #[id = "voice_unison"]
    pub unison: IntParam,
    #[id = "voice_detune"]
    pub detune: FloatParam,
    #[id = "voice_spread"]
    pub spread: FloatParam,
    #[id = "voice_glide"]
    pub glide: FloatParam,
    #[id = "voice_glide_mode"]
    pub glide_mode: EnumParam<GlideModeParam>,
    #[id = "voice_retrig"]
    pub retrigger: EnumParam<RetriggerParam>,
    #[id = "voice_drift"]
    pub drift: FloatParam,

    #[id = "chorus_mode"]
    pub chorus_mode: EnumParam<ChorusModeParam>,
    #[id = "chorus_rate"]
    pub chorus_rate: FloatParam,
    #[id = "chorus_depth"]
    pub chorus_depth: FloatParam,
    #[id = "chorus_mix"]
    pub chorus_mix: FloatParam,
    #[id = "chorus_tone"]
    pub chorus_tone: FloatParam,
    #[id = "out_clip"]
    pub output_clip: BoolParam,
    /// Run the voices at 2x the sample rate.
    #[id = "hq"]
    pub hq: BoolParam,

    #[id = "polyphony"]
    pub polyphony: IntParam,

    /// Master volume, -60 to +6 dB, stored as a linear factor.
    #[id = "volume"]
    pub volume: FloatParam,
}

impl Default for DragonWaaveParams {
    fn default() -> Self {
        let d = Patch::default();
        Self {
            editor_state: EguiState::from_size(crate::editor::WIDTH, crate::editor::HEIGHT),
            osc1: OscParams::new("Osc 1", &d.osc1),
            osc2: OscParams::new("Osc 2", &d.osc2),
            osc2_sync: BoolParam::new("Osc 2 Sync", d.osc2_sync),
            cross_mod: FloatParam::new(
                "Osc 2 Cross Mod",
                d.cross_mod,
                // Skewed so the subtle low end (gentle FM sheen) gets most of the travel.
                FloatRange::Skewed {
                    min: 0.0,
                    max: MAX_CROSS_MOD,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_smoother(SmoothingStyle::Linear(10.0))
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            ring_level: level_param("Ring Mod Level".into(), d.ring_level),
            sub_wave: EnumParam::new("Sub Wave", SubWaveParam::Square),
            sub_octave: EnumParam::new("Sub Octave", SubOctaveParam::Down1),
            sub_level: level_param("Sub Level".into(), d.sub.level),
            noise_color: EnumParam::new("Noise Color", NoiseColorParam::White),
            noise_level: level_param("Noise Level".into(), d.noise.level),
            filter_type: EnumParam::new("Filter Type", FilterTypeParam::Ladder),
            cutoff: cutoff_param("Filter Cutoff", d.filter.cutoff, 20_000.0),
            resonance: percent_param("Filter Resonance", d.filter.resonance, 0.0, 1.0),
            drive: FloatParam::new(
                "Filter Drive",
                d.filter.drive_db,
                FloatRange::Linear {
                    min: 0.0,
                    max: MAX_DRIVE_DB,
                },
            )
            .with_smoother(SmoothingStyle::Linear(10.0))
            .with_unit(" dB")
            .with_step_size(0.1),
            key_track: percent_param("Filter Key Track", d.filter.key_track, 0.0, 2.0),
            velocity_track: percent_param("Filter Velocity", d.filter.velocity, -1.0, 1.0),
            bass_comp: percent_param("Filter Bass Comp", d.filter.bass_comp, 0.0, 1.0),
            env_amount: percent_param("Filter Env Amount", d.filter.env_amount, -1.0, 1.0),
            hpf_cutoff: cutoff_param("HPF Cutoff", d.filter.hpf_cutoff, 2_000.0)
                .with_value_to_string(Arc::new(|v| {
                    if v <= HPF_OFF_HZ + 0.01 {
                        "Off".into()
                    } else if v < 1_000.0 {
                        format!("{v:.1} Hz")
                    } else {
                        format!("{:.2} kHz", v / 1_000.0)
                    }
                }))
                .with_string_to_value({
                    let hz = formatters::s2v_f32_hz_then_khz();
                    Arc::new(move |s| {
                        if s.trim().eq_ignore_ascii_case("off") {
                            Some(HPF_OFF_HZ)
                        } else {
                            hz(s)
                        }
                    })
                }),
            amp_env: EnvParams::new("Amp", &d.amp_env),
            filter_env: EnvParams::new("Filter Env", &d.filter_env),
            lfo1: LfoParams::new("LFO 1", &d.lfo1),
            lfo1_pitch: percent_param("LFO 1 > Pitch", d.lfo1_to.pitch, -1.0, 1.0).with_step_size(0.001),
            lfo1_pwm: percent_param("LFO 1 > PWM", d.lfo1_to.pwm, -1.0, 1.0),
            lfo1_cutoff: percent_param("LFO 1 > Cutoff", d.lfo1_to.cutoff, -1.0, 1.0),
            mod_env: EnvParams::new("Mod Env", &d.mod_env),
            lfo2: LfoParams::new("LFO 2", &d.lfo2),
            mod1: ModSlotParams::new(1, &d.mod_slots[0]),
            mod2: ModSlotParams::new(2, &d.mod_slots[1]),
            mod3: ModSlotParams::new(3, &d.mod_slots[2]),
            mod4: ModSlotParams::new(4, &d.mod_slots[3]),
            mod5: ModSlotParams::new(5, &d.mod_slots[4]),
            mod6: ModSlotParams::new(6, &d.mod_slots[5]),
            bend_range: IntParam::new(
                "Pitch Bend Range",
                d.bend_range as i32,
                IntRange::Linear { min: 0, max: 24 },
            )
            .with_unit(" st"),
            voice_mode: EnumParam::new("Voice Mode", VoiceModeParam::Poly),
            unison: IntParam::new(
                "Unison",
                d.voice.unison as i32,
                IntRange::Linear {
                    min: 1,
                    max: MAX_UNISON as i32,
                },
            ),
            detune: FloatParam::new(
                "Unison Detune",
                d.voice.detune,
                FloatRange::Linear { min: 0.0, max: 100.0 },
            )
            .with_smoother(SmoothingStyle::Linear(10.0))
            .with_unit(" ct")
            .with_step_size(0.1),
            spread: percent_param("Unison Spread", d.voice.spread, 0.0, 1.0),
            glide: time_param("Glide".into(), d.voice.glide, 0.0, 10.0),
            glide_mode: EnumParam::new("Glide Mode", GlideModeParam::Always),
            retrigger: EnumParam::new("Retrigger", RetriggerParam::FromCurrent),
            drift: percent_param("Drift", d.voice.drift, 0.0, 1.0),
            chorus_mode: EnumParam::new("Chorus Mode", ChorusModeParam::Off),
            chorus_rate: FloatParam::new(
                "Chorus Rate",
                d.chorus.rate,
                FloatRange::Skewed {
                    min: 0.1,
                    max: 4.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            chorus_depth: percent_param("Chorus Depth", d.chorus.depth, 0.0, dsp_core::synth::chorus::MAX_DEPTH),
            chorus_mix: percent_param("Chorus Mix", d.chorus.mix, 0.0, 1.0),
            chorus_tone: FloatParam::new(
                "Chorus Tone",
                d.chorus.tone,
                FloatRange::Skewed {
                    min: 1_000.0,
                    max: 20_000.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_value_to_string(formatters::v2s_f32_hz_then_khz(1))
            .with_string_to_value(formatters::s2v_f32_hz_then_khz()),
            output_clip: BoolParam::new("Output Clip", d.output_clip),
            hq: BoolParam::new("HQ (2x Oversampling)", d.hq),
            polyphony: IntParam::new(
                "Polyphony",
                d.polyphony as i32,
                IntRange::Linear {
                    min: 1,
                    max: MAX_VOICES as i32,
                },
            ),
            volume: FloatParam::new(
                "Volume",
                d.volume,
                FloatRange::Skewed {
                    min: util::db_to_gain(-60.0),
                    max: util::db_to_gain(6.0),
                    factor: FloatRange::gain_skew_factor(-60.0, 6.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
        }
    }
}

impl DragonWaaveParams {
    /// The matrix slots in order.
    pub fn mod_slots(&self) -> [&ModSlotParams; MOD_SLOTS] {
        [&self.mod1, &self.mod2, &self.mod3, &self.mod4, &self.mod5, &self.mod6]
    }

    /// The patch at tempo `bpm`, with smoothed values read as `steps` says.
    pub fn patch(&self, steps: Read, bpm: f32) -> Patch {
        Patch {
            osc1: self.osc1.settings(steps),
            osc2: self.osc2.settings(steps),
            sub: SubSettings {
                wave: match self.sub_wave.value() {
                    SubWaveParam::Square => SubWave::Square,
                    SubWaveParam::Pulse => SubWave::Pulse,
                },
                octave: match self.sub_octave.value() {
                    SubOctaveParam::Down1 => SubOctave::Down1,
                    SubOctaveParam::Down2 => SubOctave::Down2,
                },
                level: smoothed(&self.sub_level, steps),
            },
            noise: NoiseSettings {
                color: match self.noise_color.value() {
                    NoiseColorParam::White => NoiseColor::White,
                    NoiseColorParam::Pink => NoiseColor::Pink,
                },
                level: smoothed(&self.noise_level, steps),
            },
            osc2_sync: self.osc2_sync.value(),
            cross_mod: smoothed(&self.cross_mod, steps),
            ring_level: smoothed(&self.ring_level, steps),
            filter: FilterSettings {
                model: self.filter_type.value().into(),
                cutoff: smoothed(&self.cutoff, steps),
                resonance: smoothed(&self.resonance, steps),
                drive_db: smoothed(&self.drive, steps),
                key_track: smoothed(&self.key_track, steps),
                velocity: smoothed(&self.velocity_track, steps),
                bass_comp: smoothed(&self.bass_comp, steps),
                env_amount: smoothed(&self.env_amount, steps),
                hpf_cutoff: smoothed(&self.hpf_cutoff, steps),
            },
            amp_env: self.amp_env.settings(steps),
            filter_env: self.filter_env.settings(steps),
            // ENV 3's controls arrive with the mod matrix (phase 5).
            mod_env: self.mod_env.settings(steps),
            lfo1: self.lfo1.settings(bpm),
            lfo2: self.lfo2.settings(bpm),
            lfo1_to: LfoRoutes {
                pitch: smoothed(&self.lfo1_pitch, steps),
                pwm: smoothed(&self.lfo1_pwm, steps),
                cutoff: smoothed(&self.lfo1_cutoff, steps),
            },
            mod_slots: {
                let slots = [&self.mod1, &self.mod2, &self.mod3, &self.mod4, &self.mod5, &self.mod6];
                let mut out = [ModSlot::OFF; MOD_SLOTS];
                for (o, p) in out.iter_mut().zip(slots) {
                    *o = p.slot(steps);
                }
                out
            },
            bend_range: self.bend_range.value() as f32,
            voice: VoiceSettings {
                mode: match self.voice_mode.value() {
                    VoiceModeParam::Poly => VoiceMode::Poly,
                    VoiceModeParam::Mono => VoiceMode::Mono,
                    VoiceModeParam::Legato => VoiceMode::Legato,
                },
                unison: self.unison.value() as usize,
                detune: smoothed(&self.detune, steps),
                spread: smoothed(&self.spread, steps),
                glide: self.glide.value(),
                glide_mode: match self.glide_mode.value() {
                    GlideModeParam::Always => GlideMode::Always,
                    GlideModeParam::Legato => GlideMode::Legato,
                },
                retrigger: match self.retrigger.value() {
                    RetriggerParam::FromCurrent => Retrigger::FromCurrent,
                    RetriggerParam::FromZero => Retrigger::FromZero,
                },
                drift: smoothed(&self.drift, steps),
            },
            chorus: ChorusSettings {
                mode: match self.chorus_mode.value() {
                    ChorusModeParam::Off => ChorusMode::Off,
                    ChorusModeParam::One => ChorusMode::One,
                    ChorusModeParam::Two => ChorusMode::Two,
                    ChorusModeParam::Ensemble => ChorusMode::Ensemble,
                },
                rate: self.chorus_rate.value(),
                depth: smoothed(&self.chorus_depth, steps),
                mix: smoothed(&self.chorus_mix, steps),
                tone: self.chorus_tone.value(),
            },
            output_clip: self.output_clip.value(),
            hq: self.hq.value(),
            polyphony: self.polyphony.value() as usize,
            volume: smoothed(&self.volume, steps),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parameter defaults must produce exactly the engine's default patch.
    #[test]
    fn divisions_convert_to_hz() {
        assert_eq!(Division::Quarter.hz(120.0), 2.0);
        assert_eq!(Division::Whole.hz(120.0), 0.5);
        assert!((Division::EighthTriplet.hz(120.0) - 6.0).abs() < 1e-5);
        assert!((Division::QuarterDotted.hz(90.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn defaults_match_the_engine() {
        assert_eq!(
            DragonWaaveParams::default().patch(Read::Target, 120.0),
            Patch::default()
        );
    }
}
