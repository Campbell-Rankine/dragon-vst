//! Dragon ThreeQ: a 3-band parametric EQ.
//!
//! Three fixed bands (Low, Mid, High; by default a low shelf, a bell and a
//! high shelf). Each band has a type (bell, shelves, cuts, notch, band-pass,
//! tilt, all-pass), frequency, gain, Q, cut slope (6-96 dB/oct) and stereo
//! placement (stereo / left / right / mid / side). Bands run low -> mid ->
//! high, then output gain. Filters are designed in `dsp-core` (matched to the
//! analog response, so no cramping near Nyquist) and run as SVF cascades.
//!
//! See `docs/dragon_threeq.md` in the repository for the full write-up.

use dsp_core::{BandDesign, BandFilter, BandSettings, FilterType, Slope};
use nih_plug::prelude::*;
use std::sync::Arc;

mod editor;
mod params;
mod theme;

pub use params::{BandParams, BandSlope, BandType, DragonThreeqParams, Placement, NUM_BANDS};

/// Coefficients are recomputed at most once per this many samples while a
/// parameter is moving (smoothers still advance per block).
const CONTROL_BLOCK: usize = 32;

/// Audio-thread state for one band.
#[derive(Clone, Copy)]
struct BandRuntime {
    active: bool,
    /// Settings the current design was made from.
    designed: Option<BandSettings>,
    /// Discrete settings; a change resets the filter state to avoid
    /// running new sections on stale state.
    discrete: (FilterType, Slope, Placement),
    design: BandDesign,
    /// Two filters: left/right for stereo placements, or the single mid or
    /// side signal in filter 0.
    filters: [BandFilter; 2],
}

impl Default for BandRuntime {
    fn default() -> Self {
        Self {
            active: false,
            designed: None,
            discrete: (FilterType::Bell, Slope::Db12, Placement::Stereo),
            design: BandDesign::IDENTITY,
            filters: [BandFilter::default(); 2],
        }
    }
}

pub struct DragonThreeq {
    params: Arc<DragonThreeqParams>,
    sample_rate: f32,
    bands: [BandRuntime; NUM_BANDS],
}

impl Default for DragonThreeq {
    fn default() -> Self {
        Self {
            params: Arc::new(DragonThreeqParams::default()),
            sample_rate: 48_000.0,
            bands: [BandRuntime::default(); NUM_BANDS],
        }
    }
}

impl DragonThreeq {
    /// Advance every band's smoothers by `steps` samples and redesign bands
    /// whose settings changed. Called once per control block.
    fn update_bands(&mut self, steps: u32) {
        for (rt, bp) in self.bands.iter_mut().zip(self.params.bands()) {
            // Advance smoothers even while a band is off so they don't jump
            // from a stale value when it's switched on.
            let freq = bp.freq.smoothed.next_step(steps);
            let gain_db = bp.gain.smoothed.next_step(steps);
            let q = bp.q.smoothed.next_step(steps);

            if !bp.on.value() {
                rt.active = false;
                continue;
            }

            let settings = BandSettings {
                filter_type: bp.filter_type.value().into(),
                freq,
                gain_db,
                q,
                slope: bp.slope.value().into(),
            };
            let discrete = (settings.filter_type, settings.slope, bp.placement.value());

            if !rt.active || rt.discrete != discrete {
                rt.filters.iter_mut().for_each(BandFilter::reset);
            }
            if !rt.active || rt.designed != Some(settings) {
                rt.design = BandDesign::new(&settings, self.sample_rate);
                rt.designed = Some(settings);
            }
            rt.active = true;
            rt.discrete = discrete;
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
        // Force every band to be redesigned for the new sample rate.
        self.bands = [BandRuntime::default(); NUM_BANDS];
        true
    }

    fn reset(&mut self) {
        for rt in &mut self.bands {
            rt.filters.iter_mut().for_each(BandFilter::reset);
        }
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(self.params.clone(), self.params.editor_state.clone())
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        for (_, mut block) in buffer.iter_blocks(CONTROL_BLOCK) {
            self.update_bands(block.samples() as u32);

            for mut frame in block.iter_samples() {
                let out_gain = self.params.output_gain.smoothed.next();
                match frame.len() {
                    1 => {
                        let x = frame.get_mut(0).unwrap();
                        *x = process_mono(&mut self.bands, *x) * out_gain;
                    }
                    _ => {
                        let l = *frame.get_mut(0).unwrap();
                        let r = *frame.get_mut(1).unwrap();
                        let (l, r) = process_stereo(&mut self.bands, l, r);
                        *frame.get_mut(0).unwrap() = l * out_gain;
                        *frame.get_mut(1).unwrap() = r * out_gain;
                    }
                }
            }
        }
        ProcessStatus::Normal
    }
}

/// Run one stereo frame through every active band, in order.
#[inline]
fn process_stereo(bands: &mut [BandRuntime; NUM_BANDS], mut l: f32, mut r: f32) -> (f32, f32) {
    for rt in bands.iter_mut().filter(|rt| rt.active) {
        let [fl, fr] = &mut rt.filters;
        let d = &rt.design;
        match rt.discrete.2 {
            Placement::Stereo => {
                l = fl.process(d, l);
                r = fr.process(d, r);
            }
            Placement::Left => l = fl.process(d, l),
            Placement::Right => r = fr.process(d, r),
            Placement::Mid => {
                let side = 0.5 * (l - r);
                let mid = fl.process(d, 0.5 * (l + r));
                (l, r) = (mid + side, mid - side);
            }
            Placement::Side => {
                let mid = 0.5 * (l + r);
                let side = fl.process(d, 0.5 * (l - r));
                (l, r) = (mid + side, mid - side);
            }
        }
    }
    (l, r)
}

/// Mono: every placement except Side processes the single channel (the side
/// signal of a mono source is silent, so a Side band does nothing).
#[inline]
fn process_mono(bands: &mut [BandRuntime; NUM_BANDS], mut x: f32) -> f32 {
    for rt in bands.iter_mut().filter(|rt| rt.active && rt.discrete.2 != Placement::Side) {
        x = rt.filters[0].process(&rt.design, x);
    }
    x
}

impl ClapPlugin for DragonThreeq {
    const CLAP_ID: &'static str = "com.dragon-vst.dragon-threeq";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("A 3-band parametric EQ");
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
