//! Dragon Waave: an 80s-style polyphonic synthesizer.
//!
//! Current state: the instrument plumbing. MIDI notes in, stereo audio out,
//! 8 voices with oldest-note stealing. Each voice is a placeholder sine with
//! a short fixed attack/release so notes don't click. The Polysix-style
//! oscillators, filter, envelopes, LFO and chorus replace the sine later
//! (see `docs/roadmap.md`).

use nih_plug::prelude::*;
use std::f32::consts::TAU;
use std::sync::Arc;

mod params;

pub use params::DragonWaaveParams;

/// Maximum simultaneous voices. Fixed so `process()` never allocates.
pub const NUM_VOICES: usize = 8;

/// Placeholder envelope times (seconds) until the real ADSR exists.
const ATTACK_SECS: f32 = 0.005;
const RELEASE_SECS: f32 = 0.08;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Idle,
    Attack,
    Sustain,
    Release,
}

#[derive(Clone, Copy)]
struct Voice {
    stage: Stage,
    /// Host-supplied voice ID if any, else derived from channel + note.
    voice_id: i32,
    channel: u8,
    note: u8,
    velocity: f32,
    /// Oscillator phase in cycles, `0..1`.
    phase: f32,
    /// Phase increment per sample.
    phase_inc: f32,
    /// Linear envelope level, `0..1`.
    env: f32,
    /// Monotonic counter at note-on, for oldest-voice stealing.
    age: u64,
}

impl Voice {
    const IDLE: Self = Self {
        stage: Stage::Idle,
        voice_id: 0,
        channel: 0,
        note: 0,
        velocity: 0.0,
        phase: 0.0,
        phase_inc: 0.0,
        env: 0.0,
        age: 0,
    };

    #[inline]
    fn next(&mut self, attack_step: f32, release_step: f32) -> f32 {
        match self.stage {
            Stage::Idle => return 0.0,
            Stage::Attack => {
                self.env += attack_step;
                if self.env >= 1.0 {
                    self.env = 1.0;
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => {}
            Stage::Release => {
                self.env -= release_step;
                if self.env <= 0.0 {
                    self.env = 0.0;
                    self.stage = Stage::Idle;
                }
            }
        }
        let y = (self.phase * TAU).sin() * self.env * self.velocity;
        self.phase += self.phase_inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        y
    }
}

pub struct DragonWaave {
    params: Arc<DragonWaaveParams>,
    sample_rate: f32,
    voices: [Voice; NUM_VOICES],
    next_age: u64,
}

impl Default for DragonWaave {
    fn default() -> Self {
        Self {
            params: Arc::new(DragonWaaveParams::default()),
            sample_rate: 48_000.0,
            voices: [Voice::IDLE; NUM_VOICES],
            next_age: 0,
        }
    }
}

/// Voice ID used when the host doesn't supply one (MIDI hosts, VST3).
#[inline]
fn fallback_voice_id(channel: u8, note: u8) -> i32 {
    (channel as i32) << 8 | note as i32
}

impl DragonWaave {
    fn note_on(&mut self, voice_id: Option<i32>, channel: u8, note: u8, velocity: f32) {
        // Prefer an idle voice; otherwise steal the oldest. (A short fade on
        // steal comes with the real voice manager.)
        let slot = self
            .voices
            .iter()
            .position(|v| v.stage == Stage::Idle)
            .unwrap_or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, v)| v.age)
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            });
        self.next_age += 1;
        self.voices[slot] = Voice {
            stage: Stage::Attack,
            voice_id: voice_id.unwrap_or_else(|| fallback_voice_id(channel, note)),
            channel,
            note,
            velocity,
            phase: 0.0,
            phase_inc: util::midi_note_to_freq(note) / self.sample_rate,
            env: 0.0,
            age: self.next_age,
        };
    }

    /// Find the voices an event refers to: by voice ID if given, else by
    /// channel + note.
    fn matches(v: &Voice, voice_id: Option<i32>, channel: u8, note: u8) -> bool {
        v.stage != Stage::Idle
            && match voice_id {
                Some(id) => v.voice_id == id,
                None => v.channel == channel && v.note == note,
            }
    }

    fn note_off(&mut self, voice_id: Option<i32>, channel: u8, note: u8) {
        for v in self.voices.iter_mut().filter(|v| Self::matches(v, voice_id, channel, note)) {
            if v.stage != Stage::Release {
                v.stage = Stage::Release;
            }
        }
    }

    fn choke(&mut self, voice_id: Option<i32>, channel: u8, note: u8) {
        for v in self.voices.iter_mut().filter(|v| Self::matches(v, voice_id, channel, note)) {
            *v = Voice::IDLE;
        }
    }

    fn handle_event(&mut self, event: NoteEvent<()>) {
        match event {
            NoteEvent::NoteOn { voice_id, channel, note, velocity, .. } => {
                self.note_on(voice_id, channel, note, velocity)
            }
            NoteEvent::NoteOff { voice_id, channel, note, .. } => self.note_off(voice_id, channel, note),
            NoteEvent::Choke { voice_id, channel, note, .. } => self.choke(voice_id, channel, note),
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

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
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
        true
    }

    fn reset(&mut self) {
        self.voices = [Voice::IDLE; NUM_VOICES];
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let attack_step = 1.0 / (ATTACK_SECS * self.sample_rate);
        let release_step = 1.0 / (RELEASE_SECS * self.sample_rate);

        let mut next_event = context.next_event();
        for (sample_idx, mut frame) in buffer.iter_samples().enumerate() {
            // Apply every event due at (or before) this sample.
            while let Some(event) = next_event {
                if event.timing() > sample_idx as u32 {
                    break;
                }
                self.handle_event(event);
                next_event = context.next_event();
            }

            let volume = self.params.volume.smoothed.next();
            let mut y = 0.0;
            for v in &mut self.voices {
                y += v.next(attack_step, release_step);
            }
            y *= volume;

            for s in frame.iter_mut() {
                *s = y;
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn synth() -> DragonWaave {
        DragonWaave { sample_rate: 48_000.0, ..Default::default() }
    }

    fn active(s: &DragonWaave) -> usize {
        s.voices.iter().filter(|v| v.stage != Stage::Idle).count()
    }

    #[test]
    fn note_on_off_releases_to_idle() {
        let mut s = synth();
        s.note_on(None, 0, 69, 1.0);
        assert_eq!(active(&s), 1);
        let (a, r) = (1.0 / (ATTACK_SECS * 48_000.0), 1.0 / (RELEASE_SECS * 48_000.0));
        let peak = (0..4800).map(|_| s.voices[0].next(a, r).abs()).fold(0.0, f32::max);
        assert!(peak > 0.9, "peak {peak}");
        s.note_off(None, 0, 69);
        for _ in 0..(RELEASE_SECS * 48_000.0) as usize + 2 {
            s.voices[0].next(a, r);
        }
        assert_eq!(active(&s), 0);
    }

    #[test]
    fn steals_oldest_voice() {
        let mut s = synth();
        for n in 0..NUM_VOICES as u8 + 1 {
            s.note_on(None, 0, 60 + n, 1.0);
        }
        assert_eq!(active(&s), NUM_VOICES);
        assert!(!s.voices.iter().any(|v| v.note == 60), "oldest note should be stolen");
        assert!(s.voices.iter().any(|v| v.note == 60 + NUM_VOICES as u8));
    }

    #[test]
    fn voice_id_targets_note_off() {
        let mut s = synth();
        s.note_on(Some(7), 0, 60, 1.0);
        s.note_on(Some(8), 0, 60, 1.0);
        s.note_off(Some(7), 0, 60);
        let stages: Vec<_> = s.voices.iter().filter(|v| v.stage != Stage::Idle).map(|v| (v.voice_id, v.stage)).collect();
        assert!(stages.contains(&(7, Stage::Release)));
        assert!(stages.contains(&(8, Stage::Attack)));
    }
}
