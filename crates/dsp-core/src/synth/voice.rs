//! One voice: oscillators (with sync and cross-mod), sub, noise, ring mod,
//! mixer, drive, HPF, filter, and the amp, with its three envelopes.

use super::env::{EnvShared, Envelope};
use super::filter::{hpf_coeff, model_k, prewarp, Drive, Korg35, Ladder, OnePole, StateVariable, SvfMode};
use super::lfo::{start_voice_lfo, Depth, Lfo};
use super::matrix::{evaluate, ModDest, ModSlot, ModSource, Sources, Targets, MOD_SLOTS};
use super::noise::{Noise, Rng};
use super::osc::{OscPair, PairSettings};
use super::patch::{
    FilterModel, LfoMode, LfoShape, NoiseColor, Patch, PhaseMode, Retrigger, SubWave, Waveform, DRIFT_RATE,
    DRIFT_TOLERANCE_CENTS, DRIFT_TOLERANCE_ENV, DRIFT_TOLERANCE_OCTAVES, DRIFT_WANDER_CENTS, DRIFT_WANDER_OCTAVES,
    KEY_TRACK_CENTER,
    VELOCITY_TRACK_OCTAVES,
};
use super::util::note_to_freq;

/// How fast a stolen voice fades out. Short enough to free the voice
/// quickly, long enough not to click.
pub(crate) const STEAL_FADE_SECS: f32 = 0.003;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VoiceState {
    Idle,
    /// Key down.
    Held,
    /// Key up, amp envelope releasing.
    Released,
    /// Stolen: fading out fast. No longer counts toward polyphony and
    /// ignores note-offs.
    Fading,
}

/// The three envelopes' shared settings for the current sample, plus the
/// steal fade rate.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Envs {
    pub amp: EnvShared,
    pub filter: EnvShared,
    pub modulation: EnvShared,
    /// Steal fade decrement per sample.
    pub fade_step: f32,
}

impl Envs {
    pub fn new(p: &Patch, sample_rate: f32) -> Self {
        Self {
            amp: EnvShared::new(&p.amp_env, sample_rate),
            filter: EnvShared::new(&p.filter_env, sample_rate),
            modulation: EnvShared::new(&p.mod_env, sample_rate),
            fade_step: 1.0 / (STEAL_FADE_SECS * sample_rate),
        }
    }
}

/// The discrete (non-ramped) settings a voice needs per sample.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Shapes {
    pub wave1: Waveform,
    pub wave2: Waveform,
    pub sub_wave: SubWave,
    pub sub_divider: u32,
    pub noise_color: NoiseColor,
    pub sync: bool,
}

impl Shapes {
    pub fn from_patch(p: &Patch) -> Self {
        Self {
            wave1: p.osc1.wave,
            wave2: p.osc2.wave,
            sub_wave: p.sub.wave,
            sub_divider: p.sub.octave.divider(),
            noise_color: p.noise.color,
            sync: p.osc2_sync,
        }
    }

    pub fn pair_settings(&self, mix: &Mix) -> PairSettings {
        PairSettings {
            wave1: self.wave1,
            pw1: mix.pw1,
            wave2: self.wave2,
            pw2: mix.pw2,
            sync: self.sync,
            cross_mod: mix.cross_mod,
            sub_wave: self.sub_wave,
            sub_divider: self.sub_divider,
            pitch_ratio1: 1.0,
            pitch_ratio2: 1.0,
        }
    }
}

/// The continuous settings, ramped per sample and shared by every voice.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Mix {
    pub osc1: f32,
    pub osc2: f32,
    pub sub: f32,
    pub noise: f32,
    pub pw1: f32,
    pub pw2: f32,
    pub ring: f32,
    pub cross_mod: f32,
}

impl Mix {
    /// The patch's values without ramping (for starting a note).
    pub fn from_patch(p: &Patch) -> Self {
        Self {
            osc1: p.osc1.level,
            osc2: p.osc2.level,
            sub: p.sub.level,
            noise: p.noise.level,
            pw1: p.osc1.pulse_width,
            pw2: p.osc2.pulse_width,
            ring: p.ring_level,
            cross_mod: p.cross_mod,
        }
    }
}

/// Filter settings for the current sample, shared by every voice.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FilterNow {
    pub model: FilterModel,
    /// log2 of the cutoff in Hz, before per-voice tracking.
    pub cutoff_oct: f32,
    /// ENV 2 -> cutoff, octaves at full envelope.
    pub env_octaves: f32,
    /// Feedback / damping for the model (see `filter::*_k`).
    pub k: f32,
    pub bass_comp: f32,
    pub drive: Drive,
    /// HPF `G = g / (1 + g)`, or `None` when it's off.
    pub hpf: Option<f32>,
    pub sample_rate: f32,
    /// The unmodulated settings `k`, `drive` and `hpf` came from, for voices
    /// whose mod matrix moves them.
    pub resonance: f32,
    pub drive_db: f32,
    /// log2 Hz.
    pub hpf_oct: f32,
}

/// One LFO's settings for the current sample.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LfoNow {
    pub shape: LfoShape,
    pub mode: LfoMode,
    /// Cycles per sample.
    pub dt: f32,
    pub delay: f32,
    pub fade: f32,
    /// The shared LFO's value this sample (used in `Mono` mode).
    pub mono_value: f32,
}

/// Modulation for the current sample: both LFOs and LFO 1's routes.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ModNow {
    pub lfo: [LfoNow; 2],
    /// Seconds per sample (for LFO delay and fade).
    pub dt_secs: f32,
    /// LFO 1 -> pitch, semitones at full LFO.
    pub pitch_semis: f32,
    /// LFO 1 -> pulse width, at full LFO.
    pub pwm: f32,
    /// LFO 1 -> cutoff, octaves at full LFO.
    pub cutoff_octaves: f32,
    /// Pitch bend, -1..1, and in semitones (x the bend range).
    pub bend: f32,
    pub bend_semis: f32,
    /// 0..1.
    pub mod_wheel: f32,
    /// Channel pressure, 0..1.
    pub aftertouch: f32,
    /// The matrix with this sample's (ramped) amounts.
    pub slots: [ModSlot; MOD_SLOTS],
    /// Glide: the fraction of the remaining pitch distance kept per sample
    /// (0 = no glide).
    pub glide_keep: f32,
    /// Drift amount, 0..1.
    pub drift: f32,
    /// Unison stereo spread, 0..1.
    pub spread: f32,
    /// Whether any slot is active (skips the matrix entirely if not).
    pub matrix_active: bool,
}

/// One voice's filter state (every model, so switching is instant).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct VoiceFilter {
    hpf: OnePole,
    ladder: Ladder,
    svf: StateVariable,
    ms: Korg35,
}

impl VoiceFilter {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Drive, HPF and filter. `octaves` is the total cutoff offset; `t`
    /// the matrix targets (resonance, drive and HPF are recomputed only
    /// when the matrix moves them).
    #[inline]
    fn process(&mut self, x: f32, f: &FilterNow, octaves: f32, t: &Targets) -> f32 {
        let drive_mod = t.get(ModDest::Drive);
        let mut x = if drive_mod == 0.0 {
            f.drive.process(x)
        } else {
            Drive::new(f.drive_db + drive_mod).process(x)
        };
        let hpf_mod = t.get(ModDest::HpfCutoff);
        let hpf = if hpf_mod == 0.0 { f.hpf } else { hpf_coeff((f.hpf_oct + hpf_mod).exp2(), f.sample_rate) };
        if let Some(big_g) = hpf {
            x = self.hpf.highpass(x, big_g);
        }
        if f.model == FilterModel::Off {
            return x;
        }
        let res_mod = t.get(ModDest::Resonance);
        let k = if res_mod == 0.0 { f.k } else { model_k(f.model, (f.resonance + res_mod).clamp(0.0, 1.0)) };
        let g = prewarp((f.cutoff_oct + octaves).exp2(), f.sample_rate);
        match f.model {
            FilterModel::Off => x,
            FilterModel::Ladder => self.ladder.process(x, g, k, f.bass_comp),
            FilterModel::SvfLowPass => self.svf.process(x, g, k, SvfMode::LowPass),
            FilterModel::SvfBandPass => self.svf.process(x, g, k, SvfMode::BandPass),
            FilterModel::SvfHighPass => self.svf.process(x, g, k, SvfMode::HighPass),
            FilterModel::SvfNotch => self.svf.process(x, g, k, SvfMode::Notch),
            FilterModel::MsLowPass => self.ms.lowpass(x, g, k),
            FilterModel::MsHighPass => self.ms.highpass(x, g, k),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Voice {
    pub state: VoiceState,
    /// The host's voice ID, if it sent one (reported back when the voice ends).
    pub host_id: Option<i32>,
    /// ID used to match note-offs: the host's, or derived from channel + note.
    pub id: i32,
    pub channel: u8,
    pub note: u8,
    pub velocity: f32,
    /// Note-on order, for oldest-first stealing.
    pub age: u64,
    /// Steal fade gain: 1, falling to 0 once the voice is stolen.
    pub fade: f32,
    amp_env: Envelope,
    filter_env: Envelope,
    mod_env: Envelope,
    lfos: [Lfo; 2],
    lfo_depth: [Depth; 2],
    /// Each LFO's output last sample (after delay/fade), for the matrix.
    pub lfo_out: [f32; 2],
    /// Each envelope's output last sample (amp, filter, mod), for the matrix.
    env_out: [f32; 3],
    /// This note's random modulation source, -1..1.
    random: f32,
    /// Poly (per-note) pressure, 0..1.
    pub pressure: f32,
    /// Pan this sample, -1..1 (unison spread + the matrix).
    pub pan: f32,
    /// Place in the unison stack, -1..1 (0 when not stacked): sets detune
    /// and pan.
    unison_pos: f32,
    /// 1 / sqrt(unison voices), so stacking doesn't jump in level.
    unison_gain: f32,
    /// Remaining glide, semitones (decays to 0).
    glide: f32,
    /// This voice slot's fixed "component tolerances", -1..1: pitch,
    /// cutoff, envelope times. Scaled by the drift amount.
    tolerance: [f32; 3],
    /// Slow random wander and its rate multiplier.
    drift_lfo: Lfo,
    drift_rate: f32,
    drift: f32,
    oscs: OscPair,
    noise: Noise,
    filter: VoiceFilter,
    /// Cutoff offset from key and velocity tracking, octaves.
    cutoff_offset: f32,
    /// For free-running start phases.
    rng: Rng,
}

/// A voice's place in a note's unison stack.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Unison {
    /// -1..1 across the stack (0 for a single voice).
    pub pos: f32,
    pub gain: f32,
}

impl Unison {
    pub const SINGLE: Self = Self { pos: 0.0, gain: 1.0 };

    /// Voice `i` of `n`, spread evenly from -1 to +1.
    pub fn place(i: usize, n: usize) -> Self {
        let pos = if n > 1 { 2.0 * i as f32 / (n - 1) as f32 - 1.0 } else { 0.0 };
        Self { pos, gain: 1.0 / (n as f32).sqrt() }
    }
}

/// Voice ID used when the host doesn't supply one (MIDI, VST3).
pub(crate) fn fallback_id(channel: u8, note: u8) -> i32 {
    (channel as i32) << 8 | note as i32
}

impl Voice {
    pub fn new(seed: u32) -> Self {
        Self {
            state: VoiceState::Idle,
            host_id: None,
            id: 0,
            channel: 0,
            note: 0,
            velocity: 0.0,
            age: 0,
            fade: 1.0,
            amp_env: Envelope::default(),
            filter_env: Envelope::default(),
            mod_env: Envelope::default(),
            lfos: [Lfo::new(seed ^ 0x51ED_270B), Lfo::new(seed ^ 0x2F6B_5A3D)],
            lfo_depth: [Depth::default(); 2],
            lfo_out: [0.0; 2],
            env_out: [0.0; 3],
            random: 0.0,
            pressure: 0.0,
            pan: 0.0,
            unison_pos: 0.0,
            unison_gain: 1.0,
            glide: 0.0,
            tolerance: {
                let mut r = Rng::new(seed ^ 0x7015_CA11);
                [r.next_bipolar(), r.next_bipolar(), r.next_bipolar()]
            },
            drift_lfo: {
                let mut l = Lfo::new(seed ^ 0x0D21_F700);
                l.restart_random();
                l
            },
            drift_rate: {
                let mut r = Rng::new(seed ^ 0x2A7E_5EED);
                0.7 + 0.6 * r.next_f32()
            },
            drift: 0.0,
            oscs: OscPair::default(),
            filter: VoiceFilter::default(),
            cutoff_offset: 0.0,
            noise: Noise::new(seed),
            rng: Rng::new(seed.wrapping_mul(0x2545_F491)),
        }
    }

    pub fn is_sounding(&self) -> bool {
        matches!(self.state, VoiceState::Held | VoiceState::Released)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn start(
        &mut self,
        host_id: Option<i32>,
        channel: u8,
        note: u8,
        velocity: f32,
        age: u64,
        patch: &Patch,
        envs: &Envs,
        sample_rate: f32,
        place: Unison,
        glide_from: Option<f32>,
    ) {
        self.state = VoiceState::Held;
        self.host_id = host_id;
        self.id = host_id.unwrap_or_else(|| fallback_id(channel, note));
        self.channel = channel;
        self.note = note;
        self.velocity = velocity.clamp(0.0, 1.0);
        self.age = age;
        self.fade = 1.0;
        // A new note in a fresh (or freshly freed) voice starts from silence.
        self.amp_env.trigger_from_zero(self.velocity, &envs.amp);
        self.filter_env.trigger_from_zero(self.velocity, &envs.filter);
        self.mod_env.trigger_from_zero(self.velocity, &envs.modulation);
        start_voice_lfo(&mut self.lfos[0], &patch.lfo1);
        start_voice_lfo(&mut self.lfos[1], &patch.lfo2);
        self.lfo_depth = [Depth::default(); 2];
        self.lfo_out = [0.0; 2];
        self.env_out = [0.0; 3];
        self.random = self.rng.next_bipolar();
        self.pressure = 0.0;
        self.pan = 0.0;
        self.unison_pos = place.pos;
        self.unison_gain = place.gain;
        self.glide = glide_from.map_or(0.0, |from| from - note as f32);

        let start_phase = |mode: PhaseMode, rng: &mut Rng| match mode {
            PhaseMode::Free => rng.next_f32(),
            PhaseMode::Reset => 0.0,
        };
        let p1 = start_phase(patch.osc1.phase, &mut self.rng);
        let p2 = start_phase(patch.osc2.phase, &mut self.rng);
        let sub_count = match patch.osc1.phase {
            PhaseMode::Free => self.rng.next_u32(),
            PhaseMode::Reset => 0,
        };
        let shapes = Shapes::from_patch(patch);
        let settings = shapes.pair_settings(&Mix::from_patch(patch));
        self.oscs.start(p1, p2, sub_count, &settings);
        self.retune(patch, sample_rate);
        self.filter.reset();
        self.update_tracking(patch);
    }

    /// Mono / legato: move this sounding voice to a new note. With
    /// `retrigger`, the envelopes and per-voice LFOs restart (from the
    /// current level or from zero) and the velocity updates; without, only
    /// the pitch changes. With `glide`, the pitch slides from where it is.
    #[allow(clippy::too_many_arguments)]
    pub fn retarget(
        &mut self,
        host_id: Option<i32>,
        channel: u8,
        note: u8,
        velocity: f32,
        retrigger: Option<Retrigger>,
        glide: bool,
        patch: &Patch,
        envs: &Envs,
        sample_rate: f32,
    ) {
        let sounding_pitch = self.note as f32 + self.glide;
        self.host_id = host_id;
        self.id = host_id.unwrap_or_else(|| fallback_id(channel, note));
        self.channel = channel;
        self.note = note;
        self.glide = if glide { sounding_pitch - note as f32 } else { 0.0 };
        self.state = VoiceState::Held;
        self.pressure = 0.0;
        if let Some(mode) = retrigger {
            self.velocity = velocity.clamp(0.0, 1.0);
            let envelopes = [
                (&mut self.amp_env, &envs.amp),
                (&mut self.filter_env, &envs.filter),
                (&mut self.mod_env, &envs.modulation),
            ];
            for (env, sh) in envelopes {
                match mode {
                    Retrigger::FromCurrent => env.trigger(self.velocity, sh),
                    Retrigger::FromZero => env.trigger_from_zero(self.velocity, sh),
                }
            }
            start_voice_lfo(&mut self.lfos[0], &patch.lfo1);
            start_voice_lfo(&mut self.lfos[1], &patch.lfo2);
            self.lfo_depth = [Depth::default(); 2];
        }
        self.retune(patch, sample_rate);
        self.update_tracking(patch);
    }

    /// The pitch this voice is sounding (with any glide left), semitones.
    pub fn sounding_pitch(&self) -> f32 {
        self.note as f32 + self.glide
    }

    /// Recompute the key / velocity cutoff offset after a patch change.
    pub fn update_tracking(&mut self, patch: &Patch) {
        self.cutoff_offset = cutoff_offset(patch, self.note, self.velocity);
    }

    /// Key up: every envelope releases.
    pub fn release(&mut self) {
        self.state = VoiceState::Released;
        self.amp_env.release();
        self.filter_env.release();
        self.mod_env.release();
    }

    /// Silence at once.
    pub fn kill(&mut self) {
        self.state = VoiceState::Idle;
        self.fade = 0.0;
        self.amp_env.reset();
        self.filter_env.reset();
        self.mod_env.reset();
    }

    pub fn reset_filter(&mut self) {
        self.filter.reset();
    }

    /// Recompute oscillator frequencies after a pitch-related patch change.
    pub fn retune(&mut self, patch: &Patch, sample_rate: f32) {
        // Unison detune and this voice's pitch tolerance are fixed offsets.
        let note = self.note as f32
            + (self.unison_pos * patch.voice.detune + self.tolerance[0] * patch.voice.drift * DRIFT_TOLERANCE_CENTS)
                / 100.0;
        self.oscs.set_freqs(
            note_to_freq(note + patch.osc1.pitch_semitones()),
            note_to_freq(note + patch.osc2.pitch_semitones()),
            sample_rate,
        );
    }

    pub fn matches(&self, host_id: Option<i32>, channel: u8, note: u8) -> bool {
        match host_id {
            Some(id) => self.id == id,
            None => self.channel == channel && self.note == note,
        }
    }

    /// The matrix's sources for this voice. Envelopes and LFOs are last
    /// sample's values, so a destination may modulate its own source.
    #[inline]
    fn sources(&self, m: &ModNow) -> Sources {
        let mut src = Sources::new();
        src.set(ModSource::Lfo1, self.lfo_out[0]);
        src.set(ModSource::Lfo2, self.lfo_out[1]);
        src.set(ModSource::AmpEnv, self.env_out[0]);
        src.set(ModSource::FilterEnv, self.env_out[1]);
        src.set(ModSource::ModEnv, self.env_out[2]);
        src.set(ModSource::Velocity, self.velocity);
        src.set(ModSource::Key, (self.note as f32 - KEY_TRACK_CENTER) / 60.0);
        src.set(ModSource::ModWheel, m.mod_wheel);
        src.set(ModSource::Aftertouch, m.aftertouch.max(self.pressure));
        src.set(ModSource::PitchBend, m.bend);
        src.set(ModSource::Random, self.random);
        src.set(ModSource::Drift, self.drift);
        src
    }

    /// One sample. May move the voice to `Idle` (check afterwards); sets
    /// [`Self::pan`].
    #[inline]
    pub fn next(&mut self, shapes: &Shapes, mix: &Mix, filter: &FilterNow, envs: &Envs, m: &ModNow) -> f32 {
        if self.state == VoiceState::Idle {
            return 0.0;
        }
        if self.state == VoiceState::Fading {
            self.fade -= envs.fade_step;
        }

        let t = if m.matrix_active { evaluate(&m.slots, &self.sources(m)) } else { Targets::ZERO };

        // Glide and drift.
        if self.glide != 0.0 {
            self.glide *= m.glide_keep;
            if self.glide.abs() < 1e-4 {
                self.glide = 0.0;
            }
        }
        self.drift = self.drift_lfo.tick(LfoShape::SmoothRandom, DRIFT_RATE * self.drift_rate * m.dt_secs);
        let wander_semis = self.drift * m.drift * DRIFT_WANDER_CENTS / 100.0;
        let env_tolerance = 1.0 / (1.0 + self.tolerance[2] * m.drift * DRIFT_TOLERANCE_ENV);

        // LFOs (matrix rate mods apply to per-voice LFOs).
        for i in 0..2 {
            let l = &m.lfo[i];
            let raw = match l.mode {
                LfoMode::Mono => l.mono_value,
                LfoMode::Poly | LfoMode::PolyRetrigger => {
                    let rate_mod = t.get(if i == 0 { ModDest::Lfo1Rate } else { ModDest::Lfo2Rate });
                    let dt = if rate_mod == 0.0 { l.dt } else { l.dt * rate_mod.exp2() };
                    self.lfos[i].tick(l.shape, dt)
                }
            };
            self.lfo_out[i] = raw * self.lfo_depth[i].tick(l.delay, l.fade, m.dt_secs);
        }
        let lfo1 = self.lfo_out[0];

        // Envelopes (positive time mods = longer = slower).
        let attack_speed = (-t.get(ModDest::EnvAttack)).exp2() * env_tolerance;
        let decay_speed = (-t.get(ModDest::EnvDecay)).exp2() * env_tolerance;
        let amp = self.amp_env.tick_scaled(&envs.amp, attack_speed, decay_speed);
        let filter_env = self.filter_env.tick_scaled(&envs.filter, attack_speed, decay_speed);
        let mod_env = self.mod_env.tick_scaled(&envs.modulation, attack_speed, decay_speed);
        self.env_out = [amp, filter_env, mod_env];
        if self.amp_env.is_idle() || self.fade <= 0.0 {
            self.kill();
            return 0.0;
        }

        // Oscillators.
        let mut mix = *mix;
        let mut osc = shapes.pair_settings(&mix);
        let pitch = lfo1 * m.pitch_semis + m.bend_semis + t.get(ModDest::Pitch) + self.glide + wander_semis;
        let (p1, p2) = (pitch + t.get(ModDest::Osc1Pitch), pitch + t.get(ModDest::Osc2Pitch));
        if p1 != 0.0 {
            osc.pitch_ratio1 = (p1 / 12.0).exp2();
        }
        if p2 != 0.0 {
            osc.pitch_ratio2 = (p2 / 12.0).exp2();
        }
        let pw = lfo1 * m.pwm;
        osc.pw1 = (osc.pw1 + pw + t.get(ModDest::Osc1PulseWidth)).clamp(0.05, 0.95);
        osc.pw2 = (osc.pw2 + pw + t.get(ModDest::Osc2PulseWidth)).clamp(0.05, 0.95);
        if m.matrix_active {
            osc.cross_mod = (osc.cross_mod + t.get(ModDest::CrossMod)).clamp(0.0, super::osc::MAX_CROSS_MOD);
            let level = |base: f32, d: ModDest| (base + t.get(d)).clamp(0.0, 1.0);
            mix.osc1 = level(mix.osc1, ModDest::Osc1Level);
            mix.osc2 = level(mix.osc2, ModDest::Osc2Level);
            mix.sub = level(mix.sub, ModDest::SubLevel);
            mix.noise = level(mix.noise, ModDest::NoiseLevel);
            mix.ring = level(mix.ring, ModDest::RingLevel);
        }
        let o = self.oscs.tick(&osc);
        let noise = self.noise.next(shapes.noise_color);
        let x = o.osc1 * mix.osc1 + o.osc2 * mix.osc2 + o.sub * mix.sub + o.osc1 * o.osc2 * mix.ring + noise * mix.noise;

        // Filter: tracking + ENV 2 + LFO 1 + matrix, all in octaves.
        let drift_octaves = m.drift * (self.tolerance[1] * DRIFT_TOLERANCE_OCTAVES + self.drift * DRIFT_WANDER_OCTAVES);
        let octaves = self.cutoff_offset
            + filter_env * filter.env_octaves
            + lfo1 * m.cutoff_octaves
            + t.get(ModDest::Cutoff)
            + drift_octaves;
        let y = self.filter.process(x, filter, octaves, &t);

        self.pan = (self.unison_pos * m.spread + t.get(ModDest::Pan)).clamp(-1.0, 1.0);
        y * amp * self.fade * self.unison_gain * (1.0 + t.get(ModDest::Amp)).max(0.0)
    }

    #[cfg(test)]
    pub fn filter_env_level(&self) -> f32 {
        self.env_out[1]
    }

    #[cfg(test)]
    pub fn env_out_amp(&self) -> f32 {
        self.env_out[0]
    }

    #[cfg(test)]
    pub fn osc_dts(&self) -> (f32, f32) {
        self.oscs.dts()
    }

    #[cfg(test)]
    pub fn osc_phases(&self) -> (f32, f32) {
        self.oscs.phases()
    }
}

/// Cutoff offset in octaves for a note: key tracking around middle C plus
/// velocity tracking (no change at full velocity).
pub(crate) fn cutoff_offset(patch: &Patch, note: u8, velocity: f32) -> f32 {
    let f = &patch.filter;
    f.key_track * (note as f32 - KEY_TRACK_CENTER) / 12.0 + f.velocity * VELOCITY_TRACK_OCTAVES * (velocity - 1.0)
}
