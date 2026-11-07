//! Parameters: three bands (Low, Mid, High) plus output gain.
//!
//! The `#[id = "..."]` strings and the enum `#[id]`s are saved in host
//! projects and automation. Never rename or reuse one; add new IDs instead.
//! Band parameters get a `low_` / `mid_` / `high_` prefix from nih-plug's
//! `#[nested(id_prefix = ...)]`, e.g. `mid_freq`. The frequency and gain IDs
//! (plus `mid_q`) are the same as the original 3-band Dragon ThreeQ, so
//! projects saved with it keep those settings.

use dsp_core::{FilterType, Slope};
use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::sync::Arc;

pub const NUM_BANDS: usize = 3;

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum BandType {
    #[id = "bell"]
    Bell,
    #[id = "low-shelf"]
    #[name = "Low Shelf"]
    LowShelf,
    #[id = "high-shelf"]
    #[name = "High Shelf"]
    HighShelf,
    #[id = "low-cut"]
    #[name = "Low Cut"]
    LowCut,
    #[id = "high-cut"]
    #[name = "High Cut"]
    HighCut,
    #[id = "notch"]
    Notch,
    #[id = "band-pass"]
    #[name = "Band Pass"]
    BandPass,
    #[id = "tilt-shelf"]
    #[name = "Tilt Shelf"]
    TiltShelf,
    #[id = "all-pass"]
    #[name = "All Pass"]
    AllPass,
}

impl From<BandType> for FilterType {
    fn from(t: BandType) -> Self {
        match t {
            BandType::Bell => FilterType::Bell,
            BandType::LowShelf => FilterType::LowShelf,
            BandType::HighShelf => FilterType::HighShelf,
            BandType::LowCut => FilterType::LowCut,
            BandType::HighCut => FilterType::HighCut,
            BandType::Notch => FilterType::Notch,
            BandType::BandPass => FilterType::BandPass,
            BandType::TiltShelf => FilterType::TiltShelf,
            BandType::AllPass => FilterType::AllPass,
        }
    }
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum BandSlope {
    #[id = "6"]
    #[name = "6 dB/oct"]
    Db6,
    #[id = "12"]
    #[name = "12 dB/oct"]
    Db12,
    #[id = "18"]
    #[name = "18 dB/oct"]
    Db18,
    #[id = "24"]
    #[name = "24 dB/oct"]
    Db24,
    #[id = "30"]
    #[name = "30 dB/oct"]
    Db30,
    #[id = "36"]
    #[name = "36 dB/oct"]
    Db36,
    #[id = "48"]
    #[name = "48 dB/oct"]
    Db48,
    #[id = "72"]
    #[name = "72 dB/oct"]
    Db72,
    #[id = "96"]
    #[name = "96 dB/oct"]
    Db96,
}

impl From<BandSlope> for Slope {
    fn from(s: BandSlope) -> Self {
        match s {
            BandSlope::Db6 => Slope::Db6,
            BandSlope::Db12 => Slope::Db12,
            BandSlope::Db18 => Slope::Db18,
            BandSlope::Db24 => Slope::Db24,
            BandSlope::Db30 => Slope::Db30,
            BandSlope::Db36 => Slope::Db36,
            BandSlope::Db48 => Slope::Db48,
            BandSlope::Db72 => Slope::Db72,
            BandSlope::Db96 => Slope::Db96,
        }
    }
}

/// Which part of the stereo signal a band processes.
#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum Placement {
    #[id = "stereo"]
    Stereo,
    #[id = "left"]
    Left,
    #[id = "right"]
    Right,
    #[id = "mid"]
    Mid,
    #[id = "side"]
    Side,
}

/// One band's parameters.
#[derive(Params)]
pub struct BandParams {
    #[id = "on"]
    pub on: BoolParam,
    #[id = "type"]
    pub filter_type: EnumParam<BandType>,
    /// 10 Hz - 22 kHz (clamped just below Nyquist when processing).
    #[id = "freq"]
    pub freq: FloatParam,
    /// +/-30 dB. Only bells, shelves and tilt use it.
    #[id = "gain"]
    pub gain: FloatParam,
    /// 0.1 - 30. Width for bells/notches/band-passes; resonance for shelves and cuts.
    #[id = "q"]
    pub q: FloatParam,
    /// Cut steepness. Only low/high cuts use it.
    #[id = "slope"]
    pub slope: EnumParam<BandSlope>,
    #[id = "stereo"]
    pub placement: EnumParam<Placement>,
}

impl BandParams {
    /// `name` prefixes the host display names, e.g. "Low" -> "Low Freq".
    fn new(name: &str, filter_type: BandType, freq: f32) -> Self {
        Self {
            on: BoolParam::new(format!("{name} On"), true),
            filter_type: EnumParam::new(format!("{name} Type"), filter_type),
            freq: FloatParam::new(
                format!("{name} Freq"),
                freq,
                FloatRange::Skewed { min: 10.0, max: 22_000.0, factor: FloatRange::skew_factor(-2.2) },
            )
            .with_smoother(SmoothingStyle::Logarithmic(20.0))
            .with_value_to_string(formatters::v2s_f32_hz_then_khz(1))
            .with_string_to_value(formatters::s2v_f32_hz_then_khz()),
            gain: FloatParam::new(format!("{name} Gain"), 0.0, FloatRange::Linear { min: -30.0, max: 30.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1)),
            q: FloatParam::new(
                format!("{name} Q"),
                0.707,
                FloatRange::Skewed { min: 0.1, max: 30.0, factor: FloatRange::skew_factor(-2.0) },
            )
            .with_smoother(SmoothingStyle::Logarithmic(20.0))
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            slope: EnumParam::new(format!("{name} Slope"), BandSlope::Db12),
            placement: EnumParam::new(format!("{name} Stereo"), Placement::Stereo),
        }
    }
}

/// All automatable parameters.
#[derive(Params)]
pub struct DragonThreeqParams {
    /// Editor window size, persisted with the plugin state.
    #[persist = "editor-state"]
    pub editor_state: Arc<EguiState>,

    #[nested(id_prefix = "low", group = "Low")]
    pub low: BandParams,
    #[nested(id_prefix = "mid", group = "Mid")]
    pub mid: BandParams,
    #[nested(id_prefix = "high", group = "High")]
    pub high: BandParams,

    /// Output gain, +/-36 dB, stored as a linear factor.
    #[id = "out_gain"]
    pub output_gain: FloatParam,
}

impl DragonThreeqParams {
    /// The bands in processing order: low, mid, high.
    pub fn bands(&self) -> [&BandParams; NUM_BANDS] {
        [&self.low, &self.mid, &self.high]
    }
}

impl Default for DragonThreeqParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(760, 540),
            low: BandParams::new("Low", BandType::LowShelf, 100.0),
            mid: BandParams::new("Mid", BandType::Bell, 1_000.0),
            high: BandParams::new("High", BandType::HighShelf, 8_000.0),
            output_gain: FloatParam::new(
                "Output Gain",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-36.0),
                    max: util::db_to_gain(36.0),
                    factor: FloatRange::gain_skew_factor(-36.0, 36.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
        }
    }
}
