//! Dragon ThreeQ: a 3-band EQ.
//!
//! Current state: a 3-band EQ (low shelf, mid bell, high shelf) plus output
//! gain, with a dark egui editor of rotary knobs.
//!
//! Signal flow per channel: low shelf -> mid bell -> high shelf -> output gain.
//! See `docs/dragon_threeq.md` in the repository for the full write-up.

use dsp_core::{Svf, SvfCoefs, SvfShape};
use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, EguiState};
use std::sync::Arc;

mod knob;
use knob::Knob;

/// Mono or stereo; see `AUDIO_IO_LAYOUTS`.
const MAX_CHANNELS: usize = 2;
/// Low shelf, mid bell, high shelf, in processing order.
const NUM_BANDS: usize = 3;
/// Fixed Q for the shelves (Butterworth: no overshoot at the corner).
const SHELF_Q: f32 = 0.707;

/// Editor colours. Band colours: low = mint, mid = cyan, high = red.
mod theme {
    use nih_plug_egui::egui::Color32;

    pub const BACKGROUND: Color32 = Color32::from_rgb(0x12, 0x14, 0x18);
    /// The background, lightened, with the same cool blue tint.
    pub const PANEL: Color32 = Color32::from_rgb(0x24, 0x28, 0x30);
    pub const PEACH: Color32 = Color32::from_rgb(0xff, 0xb3, 0x8a);
    pub const TEXT: Color32 = Color32::from_gray(220);
    pub const MINT: Color32 = Color32::from_rgb(0x5c, 0xf2, 0xb0);
    pub const CYAN: Color32 = Color32::from_rgb(0x00, 0xe5, 0xff);
    pub const RED: Color32 = Color32::from_rgb(0xff, 0x3b, 0x4e);
    pub const OUTPUT: Color32 = Color32::from_gray(200);
}

/// The plugin instance. Owns the audio-thread state; parameters are shared
/// with the editor through [`DragonThreeqParams`].
pub struct DragonThreeq {
    params: Arc<DragonThreeqParams>,
    sample_rate: f32,
    /// Filter state per channel, per band.
    filters: [[Svf; NUM_BANDS]; MAX_CHANNELS],
    coefs: [SvfCoefs; NUM_BANDS],
    /// (freq, gain, q) the coefficients were last computed from.
    coef_inputs: [(f32, f32, f32); NUM_BANDS],
}

/// Automatable parameters.
///
/// The `#[id = "..."]` strings are saved in host projects and automation.
/// Never rename or reuse one, or existing projects will lose those settings.
#[derive(Params)]
pub struct DragonThreeqParams {
    /// Editor window size, persisted with the plugin state.
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    /// Low shelf corner, 20 Hz - 1 kHz.
    #[id = "low_freq"]
    pub low_freq: FloatParam,
    /// Low shelf gain, +/-18 dB.
    #[id = "low_gain"]
    pub low_gain: FloatParam,

    /// Mid bell centre, 100 Hz - 10 kHz.
    #[id = "mid_freq"]
    pub mid_freq: FloatParam,
    /// Mid bell gain, +/-18 dB.
    #[id = "mid_gain"]
    pub mid_gain: FloatParam,
    /// Mid bell width, 0.1 (wide) - 10 (narrow).
    #[id = "mid_q"]
    pub mid_q: FloatParam,

    /// High shelf corner, 1 - 20 kHz.
    #[id = "high_freq"]
    pub high_freq: FloatParam,
    /// High shelf gain, +/-18 dB.
    #[id = "high_gain"]
    pub high_gain: FloatParam,

    /// Output gain, +/-36 dB, stored as a linear factor.
    #[id = "out_gain"]
    pub output_gain: FloatParam,
}

fn freq_param(name: &str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Skewed { min, max, factor: FloatRange::skew_factor(-2.0) })
        .with_smoother(SmoothingStyle::Logarithmic(20.0))
        .with_value_to_string(formatters::v2s_f32_hz_then_khz(1))
        .with_string_to_value(formatters::s2v_f32_hz_then_khz())
}

fn band_gain_param(name: &str) -> FloatParam {
    FloatParam::new(name, 0.0, FloatRange::Linear { min: -18.0, max: 18.0 })
        .with_smoother(SmoothingStyle::Linear(20.0))
        .with_unit(" dB")
        .with_value_to_string(formatters::v2s_f32_rounded(1))
}

impl Default for DragonThreeqParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(900, 250),

            low_freq: freq_param("Low Freq", 100.0, 20.0, 1_000.0),
            low_gain: band_gain_param("Low Gain"),

            mid_freq: freq_param("Mid Freq", 1_000.0, 100.0, 10_000.0),
            mid_gain: band_gain_param("Mid Gain"),
            mid_q: FloatParam::new(
                "Mid Q",
                0.707,
                FloatRange::Skewed { min: 0.1, max: 10.0, factor: FloatRange::skew_factor(-2.0) },
            )
            .with_smoother(SmoothingStyle::Logarithmic(20.0))
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            high_freq: freq_param("High Freq", 8_000.0, 1_000.0, 20_000.0),
            high_gain: band_gain_param("High Gain"),

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

impl Default for DragonThreeq {
    fn default() -> Self {
        Self {
            params: Arc::new(DragonThreeqParams::default()),
            sample_rate: 48_000.0,
            filters: [[Svf::default(); NUM_BANDS]; MAX_CHANNELS],
            coefs: [SvfCoefs::IDENTITY; NUM_BANDS],
            // NaN never compares equal, forcing a recompute on the first sample.
            coef_inputs: [(f32::NAN, f32::NAN, f32::NAN); NUM_BANDS],
        }
    }
}

impl DragonThreeq {
    const SHAPES: [SvfShape; NUM_BANDS] = [SvfShape::LowShelf, SvfShape::Bell, SvfShape::HighShelf];

    /// Recompute a band's coefficients only when its (smoothed) inputs changed.
    #[inline]
    fn update_band(&mut self, band: usize, freq: f32, gain: f32, q: f32) {
        if self.coef_inputs[band] != (freq, gain, q) {
            self.coef_inputs[band] = (freq, gain, q);
            self.coefs[band] = SvfCoefs::new(Self::SHAPES[band], self.sample_rate, freq, gain, q);
        }
    }
}

impl Plugin for DragonThreeq {
    const NAME: &'static str = "Dragon ThreeQ";
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

    fn initialize(
        &mut self,
        _layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;
        self.coef_inputs = [(f32::NAN, f32::NAN, f32::NAN); NUM_BANDS];
        true
    }

    fn reset(&mut self) {
        for channel in &mut self.filters {
            for filter in channel {
                filter.reset();
            }
        }
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |ctx, _| {
                let mut visuals = egui::Visuals::dark();
                visuals.panel_fill = theme::BACKGROUND;
                visuals.window_fill = theme::BACKGROUND;
                ctx.set_visuals(visuals);
            },
            move |ctx, setter, _state| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(16.0))
                    .show(ctx, |ui| {
                        ui.label(
                            egui::RichText::new("DRAGON THREEQ").size(20.0).strong().color(theme::PEACH),
                        );
                        ui.add_space(10.0);

                        ui.horizontal_top(|ui| {
                            band_panel(ui, "LOW", theme::MINT, |ui| {
                                ui.add(Knob::new(&params.low_freq, setter, "Freq", theme::MINT));
                                ui.add(Knob::new(&params.low_gain, setter, "Gain", theme::MINT).bipolar());
                            });
                            band_panel(ui, "MID", theme::CYAN, |ui| {
                                ui.add(Knob::new(&params.mid_freq, setter, "Freq", theme::CYAN));
                                ui.add(Knob::new(&params.mid_gain, setter, "Gain", theme::CYAN).bipolar());
                                ui.add(Knob::new(&params.mid_q, setter, "Q", theme::CYAN));
                            });
                            band_panel(ui, "HIGH", theme::RED, |ui| {
                                ui.add(Knob::new(&params.high_freq, setter, "Freq", theme::RED));
                                ui.add(Knob::new(&params.high_gain, setter, "Gain", theme::RED).bipolar());
                            });
                            band_panel(ui, "OUT", theme::OUTPUT, |ui| {
                                ui.add(Knob::new(&params.output_gain, setter, "Gain", theme::OUTPUT).bipolar());
                            });
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
            let p = &self.params;
            let low = (p.low_freq.smoothed.next(), p.low_gain.smoothed.next(), SHELF_Q);
            let mid = (p.mid_freq.smoothed.next(), p.mid_gain.smoothed.next(), p.mid_q.smoothed.next());
            let high = (p.high_freq.smoothed.next(), p.high_gain.smoothed.next(), SHELF_Q);
            let out_gain = p.output_gain.smoothed.next();

            self.update_band(0, low.0, low.1, low.2);
            self.update_band(1, mid.0, mid.1, mid.2);
            self.update_band(2, high.0, high.1, high.2);

            for (ch, sample) in channel_samples.into_iter().enumerate().take(MAX_CHANNELS) {
                let mut x = *sample;
                for (filter, coefs) in self.filters[ch].iter_mut().zip(&self.coefs) {
                    x = filter.process(coefs, x);
                }
                *sample = x * out_gain;
            }
        }
        ProcessStatus::Normal
    }
}

/// A titled, bordered group of knobs for one band.
fn band_panel(ui: &mut egui::Ui, title: &str, color: egui::Color32, add_knobs: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(theme::PANEL)
        .stroke(egui::Stroke::new(1.5_f32, theme::PEACH))
        .corner_radius(8.0)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(title).size(13.0).strong().color(color));
                ui.add_space(4.0);
                ui.horizontal(add_knobs);
            });
        });
}

impl ClapPlugin for DragonThreeq {
    const CLAP_ID: &'static str = "com.dragon-vst.dragon-threeq";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("A 3-band EQ");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Equalizer,
        ClapFeature::Stereo,
        ClapFeature::Mono,
    ];
}

impl Vst3Plugin for DragonThreeq {
    const VST3_CLASS_ID: [u8; 16] = *b"DragonThreeQ0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Eq];
}

nih_export_clap!(DragonThreeq);
nih_export_vst3!(DragonThreeq);
