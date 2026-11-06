//! Bad Filter: a parametric/dynamic EQ.
//!
//! Milestone 0: a passthrough plugin with a smoothed output gain and a minimal
//! egui editor. This proves the build, bundling, and FL Studio loading pipeline
//! before any real DSP is added.

use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, widgets, EguiState};
use std::sync::Arc;

mod editor_size {
    pub const WIDTH: u32 = 900;
    pub const HEIGHT: u32 = 500;
}

pub struct BadFilter {
    params: Arc<BadFilterParams>,
}

#[derive(Params)]
pub struct BadFilterParams {
    /// Editor window size, persisted with the plugin state.
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "out_gain"]
    pub output_gain: FloatParam,
}

impl Default for BadFilter {
    fn default() -> Self {
        Self {
            params: Arc::new(BadFilterParams::default()),
        }
    }
}

impl Default for BadFilterParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(editor_size::WIDTH, editor_size::HEIGHT),
            output_gain: FloatParam::new(
                "Output Gain",
                dsp_core::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: dsp_core::db_to_gain(-36.0),
                    max: dsp_core::db_to_gain(36.0),
                    factor: FloatRange::gain_skew_factor(-36.0, 36.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
        }
    }
}

impl Plugin for BadFilter {
    const NAME: &'static str = "Bad Filter";
    const VENDOR: &'static str = "Dragon VST";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |_, _| {},
            move |egui_ctx, setter, _state| {
                egui::CentralPanel::default().show(egui_ctx, |ui| {
                    ui.heading("Bad Filter");
                    ui.label("Milestone 0: passthrough + output gain");
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.label("Output");
                        ui.add(widgets::ParamSlider::for_param(&params.output_gain, setter));
                    });
                });
            },
        )
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        for channel_samples in buffer.iter_samples() {
            let gain = self.params.output_gain.smoothed.next();
            for sample in channel_samples {
                *sample *= gain;
            }
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for BadFilter {
    const CLAP_ID: &'static str = "com.dragon-vst.bad-filter";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("A parametric/dynamic EQ");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Equalizer,
        ClapFeature::Stereo,
        ClapFeature::Mono,
    ];
}

impl Vst3Plugin for BadFilter {
    const VST3_CLASS_ID: [u8; 16] = *b"DragonBadFilter1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Eq];
}

nih_export_clap!(BadFilter);
nih_export_vst3!(BadFilter);
