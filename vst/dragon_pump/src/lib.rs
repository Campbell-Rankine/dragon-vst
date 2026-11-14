//! Dragon Pump: a volume ducker ("sidechain pump"), triggered by the host
//! tempo, MIDI notes or a kick on its sidechain input. See
//! `docs/dragon_pump.md`.
//!
//! The gain dips along the curve (Depth, Attack, Hold, Release, Tension),
//! started by one of three triggers:
//! - **Tempo:** the host's beat grid (Rate, Offset).
//! - **MIDI:** every note-on, sample-accurately (Velocity).
//! - **Audio:** kicks found on the sidechain input (SC Threshold, SC Freq,
//!   SC Retrigger; SC Listen plays the detector's band-passed signal).
//!
//! The ducked signal is mixed with the dry one (Mix), then Output. The
//! curve and detector maths is `dsp_core::duck`.

use dsp_core::duck::{tempo_duck, DetectorSettings, DuckShape, GainSmoother, KickDetector, TempoClock, TriggeredDuck};
use nih_plug::prelude::*;
use std::sync::Arc;

mod curve_view;
mod editor;
mod meters;
mod params;
mod shapes;

pub use meters::Meters;
pub use params::{DragonPumpParams, Feel, Mode, NoteValue, Rate};

/// Tempo assumed when the host doesn't report one.
const DEFAULT_BPM: f64 = meters::DEFAULT_BPM as f64;
/// The shape settings are read once per this many samples.
const CONTROL_BLOCK: usize = 32;

pub struct DragonPump {
    params: Arc<DragonPumpParams>,
    /// Input and sidechain levels, shared with the editor.
    meters: Arc<Meters>,
    /// The header's DSP load readout.
    perf: Arc<dragon_ui::perf::PerfStats>,
    sample_rate: f32,
    clock: TempoClock,
    /// MIDI and Audio modes' curve, restarted by each trigger.
    triggered: TriggeredDuck,
    /// Audio mode's hit finder.
    detector: KickDetector,
    smoother: GainSmoother,
}

impl Default for DragonPump {
    fn default() -> Self {
        Self {
            params: Arc::new(DragonPumpParams::default()),
            meters: Arc::default(),
            perf: Arc::default(),
            sample_rate: 48_000.0,
            clock: TempoClock::default(),
            triggered: TriggeredDuck::default(),
            detector: KickDetector::new(48_000.0),
            smoother: GainSmoother::new(48_000.0),
        }
    }
}

impl DragonPump {
    /// The curve for the next `samples`, in seconds. Tempo mode's Hold and
    /// Release are fractions of the cycle; the other modes' are in ms.
    fn shape(&self, samples: u32, mode: Mode, cycle_secs: f32) -> DuckShape {
        let p = &self.params;
        let (hold, release) = if mode.is_cyclic() {
            (p.hold_pct.smoothed.next_step(samples) * cycle_secs, p.release_pct.smoothed.next_step(samples) * cycle_secs)
        } else {
            (p.hold_ms.smoothed.next_step(samples) / 1000.0, p.release_ms.smoothed.next_step(samples) / 1000.0)
        };
        DuckShape {
            depth: 1.0, // applied per sample with Mix (both smoothed per sample)
            attack: p.attack.smoothed.next_step(samples) / 1000.0,
            hold,
            release,
            tension: p.tension.smoothed.next_step(samples),
        }
        .sanitized()
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

    fn initialize(
        &mut self,
        _layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;
        self.smoother = GainSmoother::new(self.sample_rate);
        self.clock = TempoClock::default();
        self.triggered.reset();
        self.detector = KickDetector::new(self.sample_rate);
        true
    }

    fn reset(&mut self) {
        self.smoother.reset();
        self.triggered.reset();
        self.detector.reset();
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(self.params.clone(), self.params.editor_state.clone(), self.meters.clone(), self.perf.clone())
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let started = std::time::Instant::now();
        // The sidechain's channels (none if the host passed no buffer).
        let sidechain: &[&mut [f32]] = match aux.inputs.first_mut() {
            Some(sc) => sc.as_slice_immutable(),
            None => &[],
        };
        self.meters.record_sidechain(peak(sidechain));
        self.meters.record_input(peak(buffer.as_slice_immutable()));

        // Where the beat grid is: the host's position while playing,
        // otherwise carry on from where we were.
        let transport = context.transport();
        let bpm = transport.tempo.unwrap_or(DEFAULT_BPM);
        let playing = transport.playing;
        let beats_per_sample = bpm / 60.0 / self.sample_rate as f64;
        match (playing, transport.pos_beats()) {
            (true, Some(pos)) => self.clock.sync(pos, beats_per_sample),
            _ => self.clock.free_run(beats_per_sample),
        }
        let mode = self.params.mode.value();
        let running = playing || self.params.run_stopped.value();
        let rate_beats = self.params.rate.value().beats();
        let secs_per_beat = 60.0 / bpm;
        let cycle_secs = (rate_beats * secs_per_beat) as f32;
        let offset_beats = self.params.offset.value() as f64 / 1000.0 / secs_per_beat;
        let velocity_amount = self.params.velocity.value();
        let dt = 1.0 / self.sample_rate;
        if mode.is_cyclic() {
            self.triggered.reset();
        }
        if mode != Mode::Audio {
            self.detector.reset();
        }
        let detector_settings = DetectorSettings {
            threshold_db: self.params.sc_threshold.value(),
            freq: self.params.sc_freq.value(),
            retrigger: self.params.sc_retrigger.value() / 1000.0,
        };
        let listen = mode == Mode::Audio && self.params.sc_listen.value();
        let sc_scale = if sidechain.is_empty() { 0.0 } else { 1.0 / sidechain.len() as f32 };
        let (mut detector_peak, mut hits) = (0.0f32, 0u32);

        let mut gain_low = 1.0f32;
        let mut next_event = context.next_event();
        let mut sample: u32 = 0;
        for (_, mut block) in buffer.iter_blocks(CONTROL_BLOCK) {
            let shape = self.shape(block.samples() as u32, mode, cycle_secs);
            for mut frame in block.iter_samples() {
                // Note-ons due by this sample restart the curve here.
                while let Some(event) = next_event {
                    if event.timing() > sample {
                        break;
                    }
                    if let (Mode::Midi, NoteEvent::NoteOn { velocity, .. }) = (mode, event) {
                        self.triggered.trigger(&shape, velocity_scale(velocity, velocity_amount));
                    }
                    next_event = context.next_event();
                }
                // Audio mode: a hit on the sidechain restarts the curve here.
                let mut filtered = 0.0;
                if mode == Mode::Audio {
                    let i = sample as usize;
                    let x = sc_scale * sidechain.iter().map(|ch| ch.get(i).copied().unwrap_or(0.0)).sum::<f32>();
                    let d = self.detector.process(x, &detector_settings);
                    filtered = d.filtered;
                    detector_peak = detector_peak.max(d.level);
                    if d.hit {
                        hits += 1;
                        self.triggered.trigger(&shape, 1.0);
                    }
                }
                let duck = match mode {
                    Mode::Tempo if running => {
                        let t = self.clock.time_in_cycle(rate_beats, offset_beats, secs_per_beat) as f32;
                        self.clock.advance();
                        tempo_duck(&shape, t, cycle_secs)
                    }
                    Mode::Tempo => 0.0,
                    Mode::Midi | Mode::Audio => self.triggered.next(&shape, dt),
                };
                let depth = self.params.depth.smoothed.next();
                let mix = self.params.mix.smoothed.next();
                let duck_gain = self.smoother.next(1.0 - mix * depth * duck);
                gain_low = gain_low.min(duck_gain);
                let gain = duck_gain * self.params.output_gain.smoothed.next();
                for x in frame.iter_mut() {
                    // Listen: hear what the detector hears instead.
                    *x = if listen { filtered } else { *x * gain };
                }
                sample += 1;
            }
        }
        // Anything timed past the buffer (shouldn't happen) is dropped.
        while next_event.is_some() {
            next_event = context.next_event();
        }

        // For the editor: the playhead (seconds into the cycle, or since the
        // last trigger) and the gain-reduction meter.
        let pos = match mode {
            Mode::Tempo => running.then(|| {
                (self.clock.time_in_cycle(rate_beats, offset_beats, secs_per_beat)) as f32
            }),
            Mode::Midi | Mode::Audio => self.triggered.since(),
        };
        self.meters.record_duck(gain_low, pos, bpm as f32);
        self.meters.record_detector(detector_peak, hits);
        self.perf.record(started, buffer.samples(), self.sample_rate);
        ProcessStatus::Normal
    }
}

/// How deep a note dips (0..1): full at any velocity when `amount` is 0,
/// in proportion to velocity when it's 1.
fn velocity_scale(velocity: f32, amount: f32) -> f32 {
    1.0 - amount * (1.0 - velocity.clamp(0.0, 1.0))
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

    #[test]
    fn velocity_scales_depth_by_the_amount() {
        assert_eq!(velocity_scale(0.2, 0.0), 1.0);
        assert!((velocity_scale(0.2, 1.0) - 0.2).abs() < 1e-6);
        assert!((velocity_scale(0.5, 0.5) - 0.75).abs() < 1e-6);
        assert_eq!(velocity_scale(1.0, 1.0), 1.0);
    }
}
