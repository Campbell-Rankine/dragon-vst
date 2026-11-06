//! Parameters: 24 identical bands plus output gain.
//!
//! The `#[id = "..."]` strings, the enum `#[id]`s, and the band count are
//! saved in host projects and automation. Never rename or reuse one; add new
//! IDs instead. Band parameters get a `_1` .. `_24` suffix from nih-plug's
//! `#[nested(array)]`, e.g. `freq_3`.

use dsp_core::{FilterType, Slope};
use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::sync::Arc;

pub const NUM_BANDS: usize = 24;

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
    fn new(index: usize, on: bool, filter_type: BandType, freq: f32) -> Self {
        let n = index + 1;
        Self {
            on: BoolParam::new(format!("Band {n} On"), on),
            filter_type: EnumParam::new(format!("Band {n} Type"), filter_type),
            freq: FloatParam::new(
                format!("Band {n} Freq"),
                freq,
                FloatRange::Skewed { min: 10.0, max: 22_000.0, factor: FloatRange::skew_factor(-2.2) },
            )
            .with_smoother(SmoothingStyle::Logarithmic(20.0))
            .with_value_to_string(formatters::v2s_f32_hz_then_khz(1))
            .with_string_to_value(formatters::s2v_f32_hz_then_khz()),
            gain: FloatParam::new(format!("Band {n} Gain"), 0.0, FloatRange::Linear { min: -30.0, max: 30.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1)),
            q: FloatParam::new(
                format!("Band {n} Q"),
                0.707,
                FloatRange::Skewed { min: 0.1, max: 30.0, factor: FloatRange::skew_factor(-2.0) },
            )
            .with_smoother(SmoothingStyle::Logarithmic(20.0))
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            slope: EnumParam::new(format!("Band {n} Slope"), BandSlope::Db12),
            placement: EnumParam::new(format!("Band {n} Stereo"), Placement::Stereo),
        }
    }

    /// Defaults: bands 1-3 are the original 3-band layout (low shelf, bell,
    /// high shelf, all flat). Bands 4-24 start off, spread log-evenly from
    /// 50 Hz to 12 kHz so a newly enabled band lands somewhere distinct.
    fn default_for(index: usize) -> Self {
        match index {
            0 => Self::new(0, true, BandType::LowShelf, 100.0),
            1 => Self::new(1, true, BandType::Bell, 1_000.0),
            2 => Self::new(2, true, BandType::HighShelf, 8_000.0),
            i => {
                let t = (i - 3) as f32 / (NUM_BANDS - 4) as f32;
                Self::new(i, false, BandType::Bell, 50.0 * (12_000.0f32 / 50.0).powf(t))
            }
        }
    }
}

/// All automatable parameters.
#[derive(Params)]
pub struct Dragon24EqParams {
    /// Editor window size, persisted with the plugin state.
    #[persist = "editor-state"]
    pub editor_state: Arc<EguiState>,

    #[nested(array, group = "Band")]
    pub bands: [BandParams; NUM_BANDS],

    /// Output gain, +/-36 dB, stored as a linear factor.
    #[id = "out_gain"]
    pub output_gain: FloatParam,
}

impl Default for Dragon24EqParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(900, 280),
            bands: std::array::from_fn(BandParams::default_for),
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
