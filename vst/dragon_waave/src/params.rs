//! Parameters. Only master volume for now; oscillator, filter, envelope, LFO
//! and chorus parameters arrive with the real voice (roadmap step 3-4).
//!
//! The `#[id = "..."]` strings are saved in host projects and automation.
//! Never rename or reuse one; add new IDs instead.

use nih_plug::prelude::*;

#[derive(Params)]
pub struct DragonWaaveParams {
    /// Master volume, -60 to +6 dB, stored as a linear factor.
    #[id = "volume"]
    pub volume: FloatParam,
}

impl Default for DragonWaaveParams {
    fn default() -> Self {
        Self {
            volume: FloatParam::new(
                "Volume",
                util::db_to_gain(-12.0),
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
