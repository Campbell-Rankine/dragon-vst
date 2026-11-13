//! Dragon Pump: a volume ducker ("sidechain pump"), triggered by the host
//! tempo, MIDI notes or a kick on its sidechain input. See
//! `docs/dragon_pump.md`.
//!
//! Current state: step 0, the skeleton. Audio passes through with the
//! Output gain. The sidechain (aux) input is declared and metered, but
//! doesn't affect the sound yet, so routing it in a host can be checked
//! before the ducking is built.

use nih_plug::prelude::*;
use std::sync::Arc;

mod editor;
mod meters;
mod params;

pub use meters::Meters;
pub use params::DragonPumpParams;

pub struct DragonPump {
    params: Arc<DragonPumpParams>,
    /// Input and sidechain levels, shared with the editor.
    meters: Arc<Meters>,
}

impl Default for DragonPump {
    fn default() -> Self {
        Self { params: Arc::new(DragonPumpParams::default()), meters: Arc::default() }
    }
}

impl Plugin for DragonPump {
    const NAME: &'static str = "Dragon Pump";
    const VENDOR: &'static str = "Dragon VST";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // Main in/out plus a sidechain input with the same channel count.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[new_nonzero_u32(2)],
            names: PortNames { aux_inputs: &["Sidechain"], ..PortNames::const_default() },
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            aux_input_ports: &[new_nonzero_u32(1)],
            names: PortNames { aux_inputs: &["Sidechain"], ..PortNames::const_default() },
            ..AudioIOLayout::const_default()
        },
    ];

    // MIDI trigger mode (step 4) needs note input; declared now so hosts
    // offer MIDI routing from the start.
    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(self.params.clone(), self.params.editor_state.clone(), self.meters.clone())
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Notes aren't used yet; drain them so the queue stays empty.
        while context.next_event().is_some() {}

        if let Some(sidechain) = aux.inputs.first_mut() {
            self.meters.record_sidechain(peak(sidechain.as_slice_immutable()));
        }
        self.meters.record_input(peak(buffer.as_slice_immutable()));

        for mut frame in buffer.iter_samples() {
            let gain = self.params.output_gain.smoothed.next();
            for x in frame.iter_mut() {
                *x *= gain;
            }
        }
        ProcessStatus::Normal
    }
}

/// Peak absolute sample over every channel.
fn peak<C: AsRef<[f32]>>(channels: &[C]) -> f32 {
    channels.iter().flat_map(|c| c.as_ref().iter()).fold(0.0, |m, x| m.max(x.abs()))
}

impl ClapPlugin for DragonPump {
    const CLAP_ID: &'static str = "com.dragon-vst.dragon-pump";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("A tempo-, MIDI- or sidechain-triggered volume ducker");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect, ClapFeature::Stereo, ClapFeature::Mixing];
}

impl Vst3Plugin for DragonPump {
    const VST3_CLASS_ID: [u8; 16] = *b"DragonPump000001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nih_export_clap!(DragonPump);
nih_export_vst3!(DragonPump);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_spans_every_channel() {
        assert_eq!(peak(&[&[0.1, -0.5], &[0.3, 0.2]]), 0.5);
        assert_eq!(peak::<&[f32]>(&[]), 0.0);
    }
}
