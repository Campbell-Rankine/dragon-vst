//! Dragon Waave: an 80s-style polyphonic synthesizer.
//!
//! This crate is the plugin layer: MIDI in, stereo out, parameters, and
//! (later) the editor. The sound engine is `dsp_core::synth`, which is
//! host-independent and unit-tested.
//!
//! Current state (step 3, phases 1-6): two band-limited oscillators (OSC 2
//! with hard sync and through-zero cross-mod from OSC 1), ring mod, a
//! phase-locked sub-oscillator and noise into a mixer; drive, a high-pass
//! and the main filter (LP24 ladder, SVF, or MS-20 style Korg35); up to 16
//! voices with click-free stealing; amp and filter envelopes (DAHDSR with
//! curves and velocity); two tempo-syncable LFOs (LFO 1 hard-wired to
//! pitch, PWM and cutoff); a 6-slot mod matrix; pitch bend, mod wheel and
//! aftertouch; per-voice pan; poly / mono / legato with glide, unison
//! (detune, spread) and drift. Chorus follows; see `docs/dragon_waave.md`.

use dsp_core::synth::{Synth, RAMP_SAMPLES};
use nih_plug::prelude::*;
use std::sync::Arc;

mod params;

pub use params::{DragonWaaveParams, Read};

/// Tempo assumed when the host doesn't report one.
const DEFAULT_BPM: f32 = 120.0;

/// Longest stretch rendered with one patch. Matches the engine's ramp
/// length, so continuous settings move smoothly from block to block.
const CONTROL_BLOCK: usize = RAMP_SAMPLES as usize;

pub struct DragonWaave {
    params: Arc<DragonWaaveParams>,
    synth: Synth,
}

impl Default for DragonWaave {
    fn default() -> Self {
        Self { params: Arc::new(DragonWaaveParams::default()), synth: Synth::new(48_000.0) }
    }
}

impl DragonWaave {
    fn handle_event(&mut self, event: NoteEvent<()>) {
        match event {
            NoteEvent::NoteOn { voice_id, channel, note, velocity, .. } => {
                self.synth.note_on(voice_id, channel, note, velocity)
            }
            NoteEvent::NoteOff { voice_id, channel, note, .. } => self.synth.note_off(voice_id, channel, note),
            NoteEvent::Choke { voice_id, channel, note, .. } => self.synth.choke(voice_id, channel, note),
            // nih-plug gives bend as 0..1 with 0.5 at rest.
            NoteEvent::MidiPitchBend { value, .. } => self.synth.set_pitch_bend(value * 2.0 - 1.0),
            NoteEvent::MidiCC { cc: 1, value, .. } => self.synth.set_mod_wheel(value),
            NoteEvent::MidiChannelPressure { pressure, .. } => self.synth.set_aftertouch(pressure),
            NoteEvent::PolyPressure { voice_id, channel, note, pressure, .. } => {
                self.synth.set_poly_pressure(voice_id, channel, note, pressure)
            }
            _ => {}
        }
    }
}

impl Plugin for DragonWaave {
    const NAME: &'static str = "Dragon Waave";
    const VENDOR: &'static str = "Dragon VST";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // Instrument: no audio input, stereo output.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    // MidiCCs adds pitch bend, CCs (mod wheel) and pressure.
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
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
        self.synth = Synth::new(buffer_config.sample_rate);
        self.synth.set_patch(&self.params.patch(Read::Target, DEFAULT_BPM));
        true
    }

    fn reset(&mut self) {
        self.synth.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let len = buffer.samples();
        let bpm = context.transport().tempo.map_or(DEFAULT_BPM, |t| t as f32);
        let channels = buffer.as_slice();
        let mut next_event = context.next_event();
        let mut pos = 0;

        // Render in pieces that end at the next note event or after
        // CONTROL_BLOCK samples, whichever comes first.
        while pos < len {
            // Notes starting here must see parameter changes made at the same
            // instant (e.g. automation on the note): switches and choices at
            // their new values, continuous controls where their smoothing is
            // now (the block below continues from there, so nothing jumps).
            if next_event.as_ref().is_some_and(|e| e.timing() as usize <= pos) {
                self.synth.set_patch(&self.params.patch(Read::Current, bpm));
            }
            while let Some(event) = next_event {
                if event.timing() as usize > pos {
                    break;
                }
                self.handle_event(event);
                next_event = context.next_event();
            }
            let mut end = (pos + CONTROL_BLOCK).min(len);
            if let Some(event) = &next_event {
                end = end.min(event.timing() as usize);
            }

            self.synth.set_patch(&self.params.patch(Read::Advance((end - pos) as u32), bpm));
            match channels {
                [left, right, ..] => self.synth.render(&mut left[pos..end], &mut right[pos..end]),
                [mono] => {
                    let mut scratch = [0.0f32; CONTROL_BLOCK];
                    self.synth.render(&mut mono[pos..end], &mut scratch[..end - pos]);
                }
                [] => {}
            }

            // Tell the host which voices finished (needed for CLAP
            // polyphonic modulation).
            let timing = (end - 1) as u32;
            self.synth.drain_ended(|e| {
                context.send_event(NoteEvent::VoiceTerminated {
                    timing,
                    voice_id: e.voice_id,
                    channel: e.channel,
                    note: e.note,
                })
            });
            pos = end;
        }

        // Events at or after the buffer end (shouldn't happen) still count.
        while let Some(event) = next_event {
            self.handle_event(event);
            next_event = context.next_event();
        }

        ProcessStatus::KeepAlive
    }
}

impl ClapPlugin for DragonWaave {
    const CLAP_ID: &'static str = "com.dragon-vst.dragon-waave";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("An 80s-style polyphonic synthesizer");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] =
        &[ClapFeature::Instrument, ClapFeature::Synthesizer, ClapFeature::Stereo];
}

impl Vst3Plugin for DragonWaave {
    const VST3_CLASS_ID: [u8; 16] = *b"DragonWaave00001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

nih_export_clap!(DragonWaave);
nih_export_vst3!(DragonWaave);
