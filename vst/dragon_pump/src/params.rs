//! Dragon Pump's parameters. Step 0 (the skeleton) has only Output; the
//! ducking parameters arrive with the DSP core. IDs are permanent: hosts
//! store them in projects (see `docs/dragon_pump.md` for the planned set).

use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::sync::Arc;

/// Editor size (fixed).
pub const WIDTH: u32 = 760;
pub const HEIGHT: u32 = 440;

#[derive(Params)]
pub struct DragonPumpParams {
    #[persist = "editor-state"]
    pub editor_state: Arc<EguiState>,

    /// Output gain, ±12 dB, applied after the ducking.
    #[id = "out_gain"]
    pub output_gain: FloatParam,
}

impl Default for DragonPumpParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(WIDTH, HEIGHT),
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
