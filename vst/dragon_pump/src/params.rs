//! Dragon Pump's parameters. IDs are permanent: hosts store them in
//! projects and automation. Never rename or reuse one (see
//! `docs/dragon_pump.md` for the full set).

use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::sync::Arc;

/// Editor size (fixed).
pub const WIDTH: u32 = 760;
pub const HEIGHT: u32 = 440;

/// Hold and Release ranges (fractions of the cycle), shared with the
/// curve's drag handles.
pub const HOLD_MAX: f32 = 0.5;
pub const RELEASE_MIN: f32 = 0.05;
pub const RELEASE_MAX: f32 = 1.0;
/// MIDI and Audio modes' Hold and Release ranges (ms).
pub const HOLD_MS_MAX: f32 = 500.0;
pub const RELEASE_MS_MIN: f32 = 10.0;
pub const RELEASE_MS_MAX: f32 = 2000.0;

/// What starts each dip.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Every beat of the Rate, locked to the host tempo.
    #[id = "tempo"]
    #[name = "Tempo"]
    Tempo,
    /// Every MIDI note-on.
    #[id = "midi"]
    #[name = "MIDI"]
    Midi,
    /// A kick on the sidechain input.
    #[id = "audio"]
    #[name = "Audio"]
    Audio,
}

impl Mode {
    /// Whether Hold and Release are fractions of a cycle (Tempo) rather
    /// than milliseconds (MIDI, Audio).
    pub fn is_cyclic(self) -> bool {
        self == Mode::Tempo
    }
}

/// Tempo mode's cycle length.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rate {
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
}

impl Rate {
    /// Cycle length in quarter-note beats.
    pub fn beats(self) -> f64 {
        match self {
            Rate::Whole => 4.0,
            Rate::Half => 2.0,
            Rate::HalfDotted => 3.0,
            Rate::HalfTriplet => 4.0 / 3.0,
            Rate::Quarter => 1.0,
            Rate::QuarterDotted => 1.5,
            Rate::QuarterTriplet => 2.0 / 3.0,
            Rate::Eighth => 0.5,
            Rate::EighthDotted => 0.75,
            Rate::EighthTriplet => 1.0 / 3.0,
            Rate::Sixteenth => 0.25,
            Rate::SixteenthDotted => 0.375,
            Rate::SixteenthTriplet => 1.0 / 6.0,
        }
    }

    /// The note value and its feel, as the editor's two rows of chips show
    /// them.
    pub fn split(self) -> (NoteValue, Feel) {
        use {Feel::*, NoteValue::*};
        match self {
            Rate::Whole => (Whole, Straight),
            Rate::Half => (Half, Straight),
            Rate::HalfDotted => (Half, Dotted),
            Rate::HalfTriplet => (Half, Triplet),
            Rate::Quarter => (Quarter, Straight),
            Rate::QuarterDotted => (Quarter, Dotted),
            Rate::QuarterTriplet => (Quarter, Triplet),
            Rate::Eighth => (Eighth, Straight),
            Rate::EighthDotted => (Eighth, Dotted),
            Rate::EighthTriplet => (Eighth, Triplet),
            Rate::Sixteenth => (Sixteenth, Straight),
            Rate::SixteenthDotted => (Sixteenth, Dotted),
            Rate::SixteenthTriplet => (Sixteenth, Triplet),
        }
    }

    /// The rate for a note value and feel. 1/1 only comes straight, so it
    /// ignores the feel.
    pub fn join(value: NoteValue, feel: Feel) -> Rate {
        use Feel::*;
        match (value, feel) {
            (NoteValue::Whole, _) => Rate::Whole,
            (NoteValue::Half, Straight) => Rate::Half,
            (NoteValue::Half, Dotted) => Rate::HalfDotted,
            (NoteValue::Half, Triplet) => Rate::HalfTriplet,
            (NoteValue::Quarter, Straight) => Rate::Quarter,
            (NoteValue::Quarter, Dotted) => Rate::QuarterDotted,
            (NoteValue::Quarter, Triplet) => Rate::QuarterTriplet,
            (NoteValue::Eighth, Straight) => Rate::Eighth,
            (NoteValue::Eighth, Dotted) => Rate::EighthDotted,
            (NoteValue::Eighth, Triplet) => Rate::EighthTriplet,
            (NoteValue::Sixteenth, Straight) => Rate::Sixteenth,
            (NoteValue::Sixteenth, Dotted) => Rate::SixteenthDotted,
            (NoteValue::Sixteenth, Triplet) => Rate::SixteenthTriplet,
        }
    }
}

/// A [`Rate`]'s note value (the editor's first row of chips).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteValue {
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
}

impl NoteValue {
    pub const ALL: [NoteValue; 5] =
        [NoteValue::Whole, NoteValue::Half, NoteValue::Quarter, NoteValue::Eighth, NoteValue::Sixteenth];

    pub fn label(self) -> &'static str {
        match self {
            NoteValue::Whole => "1/1",
            NoteValue::Half => "1/2",
            NoteValue::Quarter => "1/4",
            NoteValue::Eighth => "1/8",
            NoteValue::Sixteenth => "1/16",
        }
    }
}

/// A [`Rate`]'s feel (the editor's second row of chips).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feel {
    Straight,
    Dotted,
    Triplet,
}

impl Feel {
    pub const ALL: [Feel; 3] = [Feel::Straight, Feel::Dotted, Feel::Triplet];

    pub fn label(self) -> &'static str {
        match self {
            Feel::Straight => "STRAIGHT",
            Feel::Dotted => "DOTTED",
            Feel::Triplet => "TRIPLET",
        }
    }
}

#[derive(Params)]
pub struct DragonPumpParams {
    #[persist = "editor-state"]
    pub editor_state: Arc<EguiState>,

    // ----- Shape -----
    /// How far the gain dips, 0..1 (1 = to silence).
    #[id = "depth"]
    pub depth: FloatParam,
    /// Fade into the dip (ms).
    #[id = "attack"]
    pub attack: FloatParam,
    /// Time held at the floor, as a fraction of the cycle (Tempo mode).
    #[id = "hold_pct"]
    pub hold_pct: FloatParam,
    /// Time to recover, as a fraction of the cycle (Tempo mode).
    #[id = "release_pct"]
    pub release_pct: FloatParam,
    /// Time held at the floor (ms; MIDI and Audio modes).
    #[id = "hold_ms"]
    pub hold_ms: FloatParam,
    /// Time to recover (ms; MIDI and Audio modes).
    #[id = "release_ms"]
    pub release_ms: FloatParam,
    /// Recovery bend, -1..1 (+ = slow then fast, the classic pump).
    #[id = "tension"]
    pub tension: FloatParam,

    // ----- Trigger -----
    #[id = "mode"]
    pub mode: EnumParam<Mode>,
    /// MIDI mode: how much a note's velocity scales its depth, 0..1
    /// (0 = every note dips fully).
    #[id = "vel"]
    pub velocity: FloatParam,
    #[id = "rate"]
    pub rate: EnumParam<Rate>,
    /// Shifts the beat grid (ms, + = later).
    #[id = "offset"]
    pub offset: FloatParam,
    /// Keep pumping at the host tempo while the transport is stopped.
    #[id = "run_stopped"]
    pub run_stopped: BoolParam,

    // ----- Sidechain detector (Audio mode) -----
    /// Hit threshold (dBFS, of the band-passed sidechain).
    #[id = "sc_thresh"]
    pub sc_threshold: FloatParam,
    /// Band-pass centre (Hz), to pick out the kick.
    #[id = "sc_freq"]
    pub sc_freq: FloatParam,
    /// Shortest gap between hits (ms).
    #[id = "sc_retrig"]
    pub sc_retrigger: FloatParam,
    /// Hear the band-passed sidechain instead of the output, to tune SC
    /// Freq.
    #[id = "sc_listen"]
    pub sc_listen: BoolParam,

    // ----- Output -----
    /// Wet/dry of the ducked signal, 0..1.
    #[id = "mix"]
    pub mix: FloatParam,
    /// Output gain, ±12 dB, applied after the ducking.
    #[id = "out_gain"]
    pub output_gain: FloatParam,
}

/// A 0..1 parameter shown as a percentage.
fn percent(name: &str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min, max })
        .with_smoother(SmoothingStyle::Linear(20.0))
        .with_unit("%")
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

/// A time in ms, skewed so the short end gets more of the travel.
fn ms(name: &str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Skewed { min, max, factor: FloatRange::skew_factor(-1.0) })
        .with_smoother(SmoothingStyle::Linear(20.0))
        .with_unit(" ms")
        .with_step_size(1.0)
}

impl Default for DragonPumpParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(WIDTH, HEIGHT),
            depth: percent("Depth", 1.0, 0.0, 1.0),
            attack: FloatParam::new(
                "Attack",
                1.0,
                FloatRange::Skewed { min: 0.1, max: 20.0, factor: FloatRange::skew_factor(-1.5) },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_unit(" ms")
            .with_step_size(0.1),
            hold_pct: percent("Hold", 0.0, 0.0, HOLD_MAX),
            release_pct: percent("Release", 0.6, RELEASE_MIN, RELEASE_MAX),
            hold_ms: ms("Hold (ms)", 0.0, 0.0, HOLD_MS_MAX),
            release_ms: ms("Release (ms)", 300.0, RELEASE_MS_MIN, RELEASE_MS_MAX),
            tension: percent("Tension", 0.4, -1.0, 1.0),
            mode: EnumParam::new("Mode", Mode::Tempo),
            velocity: percent("Velocity", 0.0, 0.0, 1.0),
            rate: EnumParam::new("Rate", Rate::Quarter),
            offset: FloatParam::new("Offset", 0.0, FloatRange::Linear { min: -50.0, max: 50.0 })
                .with_unit(" ms")
                .with_step_size(0.1),
            run_stopped: BoolParam::new("Run When Stopped", true),
            sc_threshold: FloatParam::new("SC Threshold", -24.0, FloatRange::Linear { min: -60.0, max: 0.0 })
                .with_unit(" dB")
                .with_step_size(0.1),
            sc_freq: FloatParam::new(
                "SC Freq",
                80.0,
                FloatRange::Skewed { min: 40.0, max: 400.0, factor: FloatRange::skew_factor(-1.0) },
            )
            .with_unit(" Hz")
            .with_step_size(1.0),
            sc_retrigger: FloatParam::new(
                "SC Retrigger",
                60.0,
                FloatRange::Skewed { min: 20.0, max: 500.0, factor: FloatRange::skew_factor(-1.0) },
            )
            .with_unit(" ms")
            .with_step_size(1.0),
            sc_listen: BoolParam::new("SC Listen", false),
            mix: percent("Mix", 1.0, 0.0, 1.0),
            output_gain: FloatParam::new(
                "Output",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-12.0),
                    max: util::db_to_gain(12.0),
                    factor: FloatRange::gain_skew_factor(-12.0, 12.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_are_in_quarter_note_beats() {
        assert_eq!(Rate::Quarter.beats(), 1.0);
        assert_eq!(Rate::EighthDotted.beats(), 0.75);
        assert!((Rate::EighthTriplet.beats() * 3.0 - 1.0).abs() < 1e-12);
        assert_eq!(Rate::Whole.beats(), 4.0 * Rate::Quarter.beats());
    }

    #[test]
    fn every_rate_splits_and_joins_back() {
        for i in 0..Rate::variants().len() {
            let rate = Rate::from_index(i);
            let (value, feel) = rate.split();
            assert_eq!(Rate::join(value, feel), rate);
        }
        // 13 rates = 4 values x 3 feels + a straight 1/1.
        assert_eq!(Rate::variants().len(), 4 * Feel::ALL.len() + 1);
        assert_eq!(Rate::join(NoteValue::Whole, Feel::Triplet), Rate::Whole);
    }

    /// Every parameter's default must survive being shown and parsed back.
    #[test]
    fn defaults_round_trip_through_text() {
        let params = DragonPumpParams::default();
        for (id, ptr, _) in params.param_map() {
            // Safety: the pointers come from `params`, which outlives them.
            unsafe {
                let n = ptr.default_normalized_value();
                let shown = ptr.normalized_value_to_string(n, true);
                let again = ptr.string_to_normalized_value(&shown).unwrap_or_else(|| panic!("{id}: can't parse {shown:?}"));
                assert!((n - again).abs() < 2e-3, "{id}: {shown:?} moved");
            }
        }
    }
}
