//! Dragon Waave's sound engine: an 80s-style polyphonic subtractive synth.
//!
//! Host-independent, allocation-free after [`Synth::new`], and
//! deterministic (all randomness comes from fixed seeds).
//!
//! Usage from a plugin, per buffer:
//! 1. Split the buffer at note events and into blocks of at most 32 samples.
//! 2. For each piece: apply the note events due at its start
//!    ([`Synth::note_on`] etc.), call [`Synth::set_patch`] with the current
//!    (smoothed) settings, then [`Synth::render`], then
//!    [`Synth::drain_ended`] to tell the host which voices finished.
//!
//! See `docs/dragon_waave.md` for the design and the build plan.

mod alloc;
pub mod chorus;
pub mod env;
pub mod filter;
pub mod lfo;
pub mod matrix;
pub mod oversample;
pub mod blep;
pub mod noise;
pub mod osc;
pub mod patch;
pub mod util;
mod voice;

pub use osc::MAX_CROSS_MOD;
pub use patch::{
    EnvSettings, FilterModel, FilterSettings, LfoMode, LfoRoutes, LfoSettings, LfoShape, ENV_AMOUNT_OCTAVES,
    LFO_CUTOFF_OCTAVES, LFO_PITCH_SEMITONES, LFO_PWM_RANGE, ModDest, ModSlot, ModSource, MOD_SLOTS, GlideMode,
    Retrigger, VoiceMode, VoiceSettings, ChorusMode, ChorusSettings, DRIFT_TOLERANCE_CENTS, MAX_UNISON, KEY_TRACK_CENTER, VELOCITY_TRACK_OCTAVES, 
    NoiseColor, NoiseSettings, OscSettings, Patch, PhaseMode, SubOctave, SubSettings, SubWave, Waveform,
    MAX_VOICES,
};

use util::Ramp;
use voice::{Envs, FilterNow, LfoNow, Mix, ModNow, Shapes, Unison, Voice, VoiceState};

/// Voice slots: the polyphony maximum plus as many spares, so even a full
/// unison stack stolen at once can fade out instead of being cut.
const POOL: usize = MAX_VOICES * 2;
/// Held keys remembered for mono/legato last-note priority.
const KEY_STACK: usize = 16;

/// A held key.
#[derive(Debug, Clone, Copy, Default)]
struct Key {
    voice_id: Option<i32>,
    channel: u8,
    note: u8,
    velocity: f32,
}

impl Key {
    fn matches(&self, voice_id: Option<i32>, channel: u8, note: u8) -> bool {
        match voice_id {
            Some(id) => self.voice_id == Some(id),
            None => self.channel == channel && self.note == note,
        }
    }
}
/// Samples over which [`Synth::set_patch`] ramps continuous settings.
pub const RAMP_SAMPLES: u32 = 32;
/// Ended-voice reports kept between drains. Extra reports are dropped.
const MAX_ENDS: usize = 64;

/// A voice that has finished sounding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VoiceEnd {
    /// The host's voice ID from the note-on, if it gave one.
    pub voice_id: Option<i32>,
    pub channel: u8,
    pub note: u8,
}

pub struct Synth {
    sample_rate: f32,
    /// The rate the voices run at: the sample rate, or twice it in HQ.
    voice_rate: f32,
    /// Halfband decimators (left, right) for HQ.
    decimators: [oversample::Halfband; 2],
    patch: Patch,
    voices: [Voice; POOL],
    next_age: u64,
    envs: Envs,
    amp_sustain: Ramp,
    filter_sustain: Ramp,
    mod_sustain: Ramp,
    env_amount: Ramp,
    /// The shared LFOs, for `Mono` mode.
    mono_lfos: [lfo::Lfo; 2],
    lfo_pitch: Ramp,
    lfo_pwm: Ramp,
    lfo_cutoff: Ramp,
    mod_amounts: [Ramp; MOD_SLOTS],
    /// Controllers: -1..1, 0..1, 0..1.
    bend: Ramp,
    mod_wheel: Ramp,
    aftertouch: Ramp,
    osc1_level: Ramp,
    osc2_level: Ramp,
    sub_level: Ramp,
    noise_level: Ramp,
    pw1: Ramp,
    pw2: Ramp,
    ring_level: Ramp,
    cross_mod: Ramp,
    /// log2 Hz, so cutoff sweeps are even in pitch.
    cutoff_oct: Ramp,
    resonance: Ramp,
    drive_db: Ramp,
    bass_comp: Ramp,
    /// log2 Hz.
    hpf_oct: Ramp,
    volume: Ramp,
    ends: [VoiceEnd; MAX_ENDS],
    n_ends: usize,
    /// Held keys, oldest first (for last-note priority and legato glide).
    keys: [Key; KEY_STACK],
    n_keys: usize,
    /// Pitch of the last note played, for glide.
    last_pitch: Option<f32>,
    chorus: chorus::Chorus,
    chorus_mix: Ramp,
    chorus_depth: Ramp,
    chorus_tone_g: f32,
}

impl Synth {
    /// A synth with the default patch. The only place that allocates (the
    /// chorus delay lines).
    pub fn new(sample_rate: f32) -> Self {
        Self::with_chorus(sample_rate, chorus::Chorus::new(sample_rate))
    }

    fn with_chorus(sample_rate: f32, chorus: chorus::Chorus) -> Self {
        // `apply_patch` below only recomputes what changed from the default
        // patch, so everything derived from it starts at the default's values.
        let chorus_tone_g = chorus.tone_g(&Patch::default().chorus);
        let mut synth = Self {
            sample_rate,
            voice_rate: sample_rate,
            decimators: [oversample::Halfband::new(); 2],
            patch: Patch::default(),
            voices: std::array::from_fn(|i| Voice::new(0x1234_5678 ^ (i as u32 + 1).wrapping_mul(0x9E37_79B9))),
            next_age: 0,
            envs: Envs::new(&Patch::default(), sample_rate),
            amp_sustain: Ramp::default(),
            filter_sustain: Ramp::default(),
            mod_sustain: Ramp::default(),
            env_amount: Ramp::default(),
            mono_lfos: [lfo::Lfo::new(0x0BAD_5EED), lfo::Lfo::new(0x5EED_0BAD)],
            lfo_pitch: Ramp::default(),
            lfo_pwm: Ramp::default(),
            lfo_cutoff: Ramp::default(),
            mod_amounts: [Ramp::default(); MOD_SLOTS],
            bend: Ramp::default(),
            mod_wheel: Ramp::default(),
            aftertouch: Ramp::default(),
            osc1_level: Ramp::default(),
            osc2_level: Ramp::default(),
            sub_level: Ramp::default(),
            noise_level: Ramp::default(),
            pw1: Ramp::default(),
            pw2: Ramp::default(),
            ring_level: Ramp::default(),
            cross_mod: Ramp::default(),
            cutoff_oct: Ramp::default(),
            resonance: Ramp::default(),
            drive_db: Ramp::default(),
            bass_comp: Ramp::default(),
            hpf_oct: Ramp::default(),
            volume: Ramp::default(),
            ends: [VoiceEnd::default(); MAX_ENDS],
            n_ends: 0,
            keys: [Key::default(); KEY_STACK],
            n_keys: 0,
            last_pitch: None,
            chorus,
            chorus_mix: Ramp::default(),
            chorus_depth: Ramp::default(),
            chorus_tone_g,
        };
        synth.apply_patch(&Patch::default(), 0);
        synth.restart_mono_lfos();
        synth
    }

    fn restart_mono_lfos(&mut self) {
        self.mono_lfos[0].restart(self.patch.lfo1.phase);
        self.mono_lfos[1].restart(self.patch.lfo2.phase);
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// The current (sanitized) patch.
    pub fn patch(&self) -> &Patch {
        &self.patch
    }

    /// Update the sound. Continuous settings (levels, pulse widths, volume)
    /// ramp over the next [`RAMP_SAMPLES`] samples; the rest apply at once.
    pub fn set_patch(&mut self, patch: &Patch) {
        self.apply_patch(patch, RAMP_SAMPLES);
    }

    fn apply_patch(&mut self, patch: &Patch, ramp: u32) {
        let p = patch.sanitized();
        let old = self.patch;
        self.patch = p;
        let voice_rate = if p.hq { 2.0 * self.sample_rate } else { self.sample_rate };
        let rate_changed = voice_rate != self.voice_rate;
        if rate_changed {
            self.voice_rate = voice_rate;
            self.decimators.iter_mut().for_each(oversample::Halfband::reset);
        }

        self.osc1_level.set_target(p.osc1.level, ramp);
        self.osc2_level.set_target(p.osc2.level, ramp);
        self.sub_level.set_target(p.sub.level, ramp);
        self.noise_level.set_target(p.noise.level, ramp);
        self.pw1.set_target(p.osc1.pulse_width, ramp);
        self.pw2.set_target(p.osc2.pulse_width, ramp);
        self.ring_level.set_target(p.ring_level, ramp);
        self.cross_mod.set_target(p.cross_mod, ramp);
        self.cutoff_oct.set_target(p.filter.cutoff.log2(), ramp);
        self.resonance.set_target(p.filter.resonance, ramp);
        self.drive_db.set_target(p.filter.drive_db, ramp);
        self.bass_comp.set_target(p.filter.bass_comp, ramp);
        self.hpf_oct.set_target(p.filter.hpf_cutoff.log2(), ramp);
        self.env_amount.set_target(p.filter.env_amount, ramp);
        self.chorus_mix.set_target(p.chorus.mix, ramp);
        self.chorus_depth.set_target(p.chorus.depth, ramp);
        if p.chorus.tone != old.chorus.tone {
            self.chorus_tone_g = self.chorus.tone_g(&p.chorus);
        }
        // Times apply at once (they set rates); sustain levels ramp.
        if rate_changed || p.amp_env != old.amp_env || p.filter_env != old.filter_env || p.mod_env != old.mod_env {
            self.envs = Envs::new(&p, self.voice_rate);
        }
        self.amp_sustain.set_target(p.amp_env.sustain, ramp);
        self.filter_sustain.set_target(p.filter_env.sustain, ramp);
        self.mod_sustain.set_target(p.mod_env.sustain, ramp);
        self.lfo_pitch.set_target(p.lfo1_to.pitch * LFO_PITCH_SEMITONES, ramp);
        self.lfo_pwm.set_target(p.lfo1_to.pwm * LFO_PWM_RANGE, ramp);
        self.lfo_cutoff.set_target(p.lfo1_to.cutoff * LFO_CUTOFF_OCTAVES, ramp);
        for (r, slot) in self.mod_amounts.iter_mut().zip(&p.mod_slots) {
            r.set_target(slot.amount, ramp);
        }
        self.volume.set_target(p.volume, ramp);

        if p.voice.mode != old.voice.mode {
            // Switching between poly and mono: let everything go and start clean.
            for v in self.voices.iter_mut().filter(|v| v.state == VoiceState::Held) {
                v.release();
            }
            self.n_keys = 0;
        }
        if rate_changed
            || p.osc1.pitch_semitones() != old.osc1.pitch_semitones()
            || p.osc2.pitch_semitones() != old.osc2.pitch_semitones()
            || p.voice.detune != old.voice.detune
            || p.voice.drift != old.voice.drift
        {
            for v in self.voices.iter_mut().filter(|v| v.state != VoiceState::Idle) {
                v.retune(&p, self.voice_rate);
            }
        }
        if p.filter.model != old.filter.model {
            // A different model starts from clean state rather than another
            // model's leftovers.
            for v in &mut self.voices {
                v.reset_filter();
            }
        }
        if p.filter.key_track != old.filter.key_track || p.filter.velocity != old.filter.velocity {
            for v in self.voices.iter_mut().filter(|v| v.state != VoiceState::Idle) {
                v.update_tracking(&p);
            }
        }
        if p.polyphony < old.polyphony {
            alloc::enforce_limit(&mut self.voices, p.polyphony);
        }
    }

    /// Start a note. `voice_id` is the host's (CLAP) voice ID, if any.
    pub fn note_on(&mut self, voice_id: Option<i32>, channel: u8, note: u8, velocity: f32) {
        let vs = self.patch.voice;
        let keys_were_held = self.n_keys > 0;
        self.push_key(Key { voice_id, channel, note, velocity });
        let glide = vs.glide > 0.0 && (vs.glide_mode == GlideMode::Always || keys_were_held);
        // Glide from where the newest voice is now (it may be mid-glide),
        // else from the last note played.
        let newest = self.voices.iter().filter(|v| v.is_sounding()).max_by_key(|v| v.age);
        let glide_from = if glide { newest.map(Voice::sounding_pitch).or(self.last_pitch) } else { None };

        let sounding = self.voices.iter().any(Voice::is_sounding);
        match vs.mode {
            VoiceMode::Mono | VoiceMode::Legato if sounding => {
                let retrigger = (vs.mode == VoiceMode::Mono || !keys_were_held).then_some(vs.retrigger);
                self.retarget_sounding(Key { voice_id, channel, note, velocity }, retrigger, glide);
            }
            _ => self.start_note(voice_id, channel, note, velocity, glide_from),
        }
        self.last_pitch = Some(note as f32);
    }

    /// Start a note in fresh voices: one per unison voice.
    fn start_note(&mut self, voice_id: Option<i32>, channel: u8, note: u8, velocity: f32, glide_from: Option<f32>) {
        let n = self.patch.voice.unison.min(self.patch.polyphony);
        alloc::enforce_limit(&mut self.voices, self.patch.polyphony.saturating_sub(n));
        self.next_age += 1;
        for i in 0..n {
            let (slot, cut) = alloc::free_slot(&self.voices);
            if cut {
                self.record_end(slot);
            }
            let place = if n == 1 { Unison::SINGLE } else { Unison::place(i, n) };
            self.voices[slot].start(
                voice_id,
                channel,
                note,
                velocity,
                self.next_age,
                &self.patch,
                &self.envs,
                self.voice_rate,
                place,
                glide_from,
            );
        }
    }

    /// Mono / legato: move the newest note's voice(s) to `key`. Older
    /// voices still releasing (e.g. from before a switch to mono) are left
    /// to finish. The old note is reported as ended, since the voice now
    /// carries the new one.
    fn retarget_sounding(&mut self, key: Key, retrigger: Option<Retrigger>, glide: bool) {
        let newest = self.voices.iter().filter(|v| v.is_sounding()).map(|v| v.age).max();
        for i in 0..POOL {
            if self.voices[i].is_sounding() && Some(self.voices[i].age) == newest {
                self.record_end(i);
                self.voices[i].retarget(
                    key.voice_id,
                    key.channel,
                    key.note,
                    key.velocity,
                    retrigger,
                    glide,
                    &self.patch,
                    &self.envs,
                    self.voice_rate,
                );
            }
        }
        self.last_pitch = Some(key.note as f32);
    }

    /// Release held voices matching the host voice ID, or channel + note
    /// when there is no ID. In mono / legato, releasing the newest key falls
    /// back to the newest one still held.
    pub fn note_off(&mut self, voice_id: Option<i32>, channel: u8, note: u8) {
        let was_newest = self.n_keys > 0 && self.keys[self.n_keys - 1].matches(voice_id, channel, note);
        self.remove_key(voice_id, channel, note);
        let vs = self.patch.voice;
        match vs.mode {
            VoiceMode::Poly => {
                for v in &mut self.voices {
                    if v.state == VoiceState::Held && v.matches(voice_id, channel, note) {
                        v.release();
                    }
                }
            }
            VoiceMode::Mono | VoiceMode::Legato => {
                if self.n_keys == 0 {
                    for v in self.voices.iter_mut().filter(|v| v.state == VoiceState::Held) {
                        v.release();
                    }
                } else if was_newest {
                    let back = self.keys[self.n_keys - 1];
                    let retrigger = (vs.mode == VoiceMode::Mono).then_some(vs.retrigger);
                    self.retarget_sounding(back, retrigger, vs.glide > 0.0);
                }
            }
        }
    }

    fn push_key(&mut self, key: Key) {
        self.remove_key(key.voice_id, key.channel, key.note);
        if self.n_keys == KEY_STACK {
            self.keys.copy_within(1.., 0);
            self.n_keys -= 1;
        }
        self.keys[self.n_keys] = key;
        self.n_keys += 1;
    }

    fn remove_key(&mut self, voice_id: Option<i32>, channel: u8, note: u8) {
        let mut kept = 0;
        for i in 0..self.n_keys {
            if !self.keys[i].matches(voice_id, channel, note) {
                self.keys[kept] = self.keys[i];
                kept += 1;
            }
        }
        self.n_keys = kept;
    }

    /// Pitch bend, -1..=1 (scaled by the patch's bend range).
    pub fn set_pitch_bend(&mut self, bend: f32) {
        self.bend.set_target(bend.clamp(-1.0, 1.0), RAMP_SAMPLES);
    }

    /// Mod wheel (CC 1), 0..=1.
    pub fn set_mod_wheel(&mut self, value: f32) {
        self.mod_wheel.set_target(value.clamp(0.0, 1.0), RAMP_SAMPLES);
    }

    /// Channel pressure, 0..=1.
    pub fn set_aftertouch(&mut self, pressure: f32) {
        self.aftertouch.set_target(pressure.clamp(0.0, 1.0), RAMP_SAMPLES);
    }

    /// Poly (per-note) pressure for matching voices, 0..=1.
    pub fn set_poly_pressure(&mut self, voice_id: Option<i32>, channel: u8, note: u8, pressure: f32) {
        for v in self.voices.iter_mut().filter(|v| v.is_sounding() && v.matches(voice_id, channel, note)) {
            v.pressure = pressure.clamp(0.0, 1.0);
        }
    }

    /// Silence matching voices immediately.
    pub fn choke(&mut self, voice_id: Option<i32>, channel: u8, note: u8) {
        for i in 0..POOL {
            let v = &self.voices[i];
            if v.state != VoiceState::Idle && v.matches(voice_id, channel, note) {
                self.record_end(i);
                self.voices[i].kill();
            }
        }
    }

    /// Render into `left` and `right` (equal lengths), overwriting them.
    pub fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        debug_assert_eq!(left.len(), right.len());
        let shapes = Shapes::from_patch(&self.patch);
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let (sum_l, sum_r) = if self.patch.hq {
                let (a_l, a_r) = self.voices_sample(&shapes);
                let (b_l, b_r) = self.voices_sample(&shapes);
                (self.decimators[0].process(a_l, b_l), self.decimators[1].process(a_r, b_r))
            } else {
                self.voices_sample(&shapes)
            };
            let volume = self.volume.tick();
            let (mut out_l, mut out_r) = (sum_l * volume, sum_r * volume);
            let chorus_now = chorus::ChorusNow {
                mode: self.patch.chorus.mode,
                rate: self.patch.chorus.rate,
                depth: self.chorus_depth.tick(),
                mix: self.chorus_mix.tick(),
                tone_g: self.chorus_tone_g,
            };
            self.chorus.process(&mut out_l, &mut out_r, &chorus_now);
            if self.patch.output_clip {
                out_l = chorus::soft_clip(out_l);
                out_r = chorus::soft_clip(out_r);
            }
            *l = out_l;
            *r = out_r;
        }
    }

    /// One sample of every voice, summed and panned, at the voice rate.
    #[inline]
    fn voices_sample(&mut self, shapes: &Shapes) -> (f32, f32) {
        {
            let mix = Mix {
                osc1: self.osc1_level.tick(),
                osc2: self.osc2_level.tick(),
                sub: self.sub_level.tick(),
                noise: self.noise_level.tick(),
                pw1: self.pw1.tick(),
                pw2: self.pw2.tick(),
                ring: self.ring_level.tick(),
                cross_mod: self.cross_mod.tick(),
            };
            let filter = self.filter_now();
            self.envs.amp.sustain = self.amp_sustain.tick();
            self.envs.filter.sustain = self.filter_sustain.tick();
            self.envs.modulation.sustain = self.mod_sustain.tick();
            let modulation = self.mod_now();

            let (mut sum_l, mut sum_r) = (0.0, 0.0);
            for i in 0..POOL {
                if self.voices[i].state == VoiceState::Idle {
                    continue;
                }
                let y = self.voices[i].next(shapes, &mix, &filter, &self.envs, &modulation);
                // Balance law: unity in the centre, the far side fades out.
                let pan = self.voices[i].pan;
                sum_l += y * (1.0 - pan).min(1.0);
                sum_r += y * (1.0 + pan).min(1.0);
                if self.voices[i].state == VoiceState::Idle {
                    self.record_end(i);
                }
            }
            (sum_l, sum_r)
        }
    }

    /// Report (and forget) every voice that ended since the last call.
    pub fn drain_ended(&mut self, mut f: impl FnMut(VoiceEnd)) {
        for e in &self.ends[..self.n_ends] {
            f(*e);
        }
        self.n_ends = 0;
    }

    /// Voices currently producing sound (including ones fading out).
    pub fn active_voices(&self) -> usize {
        self.voices.iter().filter(|v| v.state != VoiceState::Idle).count()
    }

    /// Silence everything and forget pending reports.
    pub fn reset(&mut self) {
        let patch = self.patch;
        // Reuse the chorus buffers: hosts may reset from the audio thread,
        // where allocating isn't allowed.
        let mut chorus = std::mem::replace(&mut self.chorus, chorus::Chorus::empty());
        chorus.clear();
        *self = Self::with_chorus(self.sample_rate, chorus);
        self.apply_patch(&patch, 0);
        self.restart_mono_lfos();
    }

    /// Advance the shared LFOs and LFO 1's route ramps one sample.
    #[inline]
    fn mod_now(&mut self) -> ModNow {
        let sr = self.voice_rate;
        let mut lfo_now = |i: usize, s: &LfoSettings| {
            let dt = s.rate / sr;
            let mono_value = if s.mode == LfoMode::Mono { self.mono_lfos[i].tick(s.shape, dt) } else { 0.0 };
            LfoNow { shape: s.shape, mode: s.mode, dt, delay: s.delay, fade: s.fade, mono_value }
        };
        let (l1, l2) = (self.patch.lfo1, self.patch.lfo2);
        let mut slots = self.patch.mod_slots;
        for (slot, r) in slots.iter_mut().zip(self.mod_amounts.iter_mut()) {
            slot.amount = r.tick();
        }
        let bend = self.bend.tick();
        let glide = self.patch.voice.glide;
        ModNow {
            // Exponential: about 99 % of the way (e^-4.6) after the glide time.
            glide_keep: if glide > 0.0 { (-4.6 / (glide * sr)).exp() } else { 0.0 },
            drift: self.patch.voice.drift,
            spread: self.patch.voice.spread,
            matrix_active: slots.iter().any(ModSlot::is_active),
            slots,
            bend,
            bend_semis: bend * self.patch.bend_range,
            mod_wheel: self.mod_wheel.tick(),
            aftertouch: self.aftertouch.tick(),
            lfo: [lfo_now(0, &l1), lfo_now(1, &l2)],
            dt_secs: 1.0 / sr,
            pitch_semis: self.lfo_pitch.tick(),
            pwm: self.lfo_pwm.tick(),
            cutoff_octaves: self.lfo_cutoff.tick(),
        }
    }

    /// Advance the filter ramps one sample and derive the shared settings.
    #[inline]
    fn filter_now(&mut self) -> FilterNow {
        let model = self.patch.filter.model;
        let resonance = self.resonance.tick();
        let hpf_oct = self.hpf_oct.tick();
        let drive_db = self.drive_db.tick();
        FilterNow {
            model,
            cutoff_oct: self.cutoff_oct.tick(),
            env_octaves: self.env_amount.tick() * ENV_AMOUNT_OCTAVES,
            k: filter::model_k(model, resonance),
            bass_comp: self.bass_comp.tick(),
            drive: filter::Drive::new(drive_db),
            hpf: filter::hpf_coeff(hpf_oct.exp2(), self.voice_rate),
            sample_rate: self.voice_rate,
            resonance,
            drive_db,
            hpf_oct,
        }
    }

    fn record_end(&mut self, slot: usize) {
        if self.n_ends < MAX_ENDS {
            let v = &self.voices[slot];
            self.ends[self.n_ends] = VoiceEnd { voice_id: v.host_id, channel: v.channel, note: v.note };
            self.n_ends += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::osc::tests::{measured_freq, SR};
    use super::*;

    fn synth_with(patch: Patch) -> Synth {
        let mut s = Synth::new(SR);
        s.apply_patch(&patch, 0);
        s
    }

    /// A raw sine at unity: filter off, volume 1.
    fn sine_patch() -> Patch {
        let mut p = Patch::default();
        p.osc1.wave = Waveform::Sine;
        p.filter.model = FilterModel::Off;
        p.volume = 1.0;
        p
    }

    /// Steady peak of a held note (after 0.1 s) for a patch.
    fn held_peak(p: Patch, note: u8, velocity: f32) -> f32 {
        let mut s = synth_with(p);
        s.note_on(None, 0, note, velocity);
        let x = run(&mut s, SR as usize / 2);
        x[4_800..].iter().fold(0f32, |m, v| m.max(v.abs()))
    }

    fn db(x: f32) -> f32 {
        20.0 * x.log10()
    }

    /// A quiet sine (so the ladder's saturation stays out of the way)
    /// through `model` with its cutoff at `cutoff`.
    fn filtered_sine(model: FilterModel, cutoff: f32) -> Patch {
        let mut p = sine_patch();
        p.osc1.level = 0.01;
        p.filter = FilterSettings { model, cutoff, ..p.filter };
        p
    }

    #[test]
    fn ladder_at_the_played_pitch_is_12db_down() {
        // A4 = 440 Hz; four one-poles are -3 dB each at the cutoff.
        let open = held_peak(filtered_sine(FilterModel::Off, 440.0), 69, 1.0);
        let shut = held_peak(filtered_sine(FilterModel::Ladder, 440.0), 69, 1.0);
        assert!((db(shut / open) + 12.04).abs() < 0.1, "{:.2} dB", db(shut / open));
    }

    /// Cutoff 2 octaves below the note, ENV 2 held at full sustain with an
    /// amount of 2 octaves (20 % of 10): the cutoff lands on the note, -12 dB.
    #[test]
    fn filter_envelope_moves_the_cutoff() {
        let mut p = filtered_sine(FilterModel::Ladder, 110.0);
        p.filter.env_amount = 0.2;
        p.filter_env.sustain = 1.0;
        let open = held_peak(filtered_sine(FilterModel::Off, 0.0), 69, 1.0);
        let rel = db(held_peak(p, 69, 1.0) / open);
        assert!((rel + 12.04).abs() < 0.1, "{rel:.2} dB");
        // Negative amounts close it instead: 2 octaves above, swept down to the note.
        let mut p = filtered_sine(FilterModel::Ladder, 1_760.0);
        p.filter.env_amount = -0.2;
        p.filter_env.sustain = 1.0;
        let rel = db(held_peak(p, 69, 1.0) / open);
        assert!((rel + 12.04).abs() < 0.1, "negative amount: {rel:.2} dB");
    }

    #[test]
    fn zero_sustain_frees_the_voice_without_a_note_off() {
        let mut p = sine_patch();
        p.amp_env.decay = 0.05;
        p.amp_env.sustain = 0.0;
        let mut s = synth_with(p);
        s.note_on(Some(9), 0, 60, 1.0);
        run(&mut s, (0.06 * SR) as usize);
        assert_eq!(s.active_voices(), 0);
        assert_eq!(ends(&mut s), [VoiceEnd { voice_id: Some(9), channel: 0, note: 60 }]);
        s.note_off(Some(9), 0, 60); // harmless
    }

    #[test]
    fn amp_delay_holds_the_note_back() {
        let mut p = sine_patch();
        p.amp_env.delay = 0.1;
        let mut s = synth_with(p);
        s.note_on(None, 0, 69, 1.0);
        let x = run(&mut s, (0.2 * SR) as usize);
        let split = (0.1 * SR) as usize;
        assert!(x[..split].iter().all(|&v| v == 0.0));
        assert!(x[split + 480..].iter().any(|v| v.abs() > 0.5));
    }

    #[test]
    fn amp_velocity_amount() {
        let mut p = sine_patch();
        assert!((db(held_peak(p, 69, 0.5)) + 6.02).abs() < 0.05, "level follows velocity by default");
        p.amp_env.velocity_level = 0.0;
        assert!(db(held_peak(p, 69, 0.5)).abs() < 0.05, "velocity ignored at 0 %");
    }

    /// LFO 1 as a 1 Hz square restarting on the key: +1 for the first half
    /// second, -1 for the next.
    fn square_lfo(p: &mut Patch) {
        p.lfo1 = LfoSettings { shape: LfoShape::Square, rate: 1.0, mode: LfoMode::PolyRetrigger, ..p.lfo1 };
    }

    #[test]
    fn lfo_pitch_route_bends_both_ways() {
        let mut p = sine_patch();
        square_lfo(&mut p);
        p.lfo1_to.pitch = 1.0; // +-12 st
        let mut s = synth_with(p);
        s.note_on(None, 0, 69, 1.0);
        let x = run(&mut s, SR as usize);
        let half = SR as usize / 2;
        let up = measured_freq(&x[2_400..half - 100]);
        let down = measured_freq(&x[half + 2_400..]);
        assert!((up - 880.0).abs() < 0.5, "first half {up} Hz");
        assert!((down - 220.0).abs() < 0.5, "second half {down} Hz");
    }

    #[test]
    fn lfo_cutoff_route_moves_the_filter_in_octaves() {
        // Cutoff 2 octaves below the note; +40 % = +2 octaves on the high half.
        let mut p = filtered_sine(FilterModel::Ladder, 110.0);
        square_lfo(&mut p);
        p.lfo1.rate = 0.5; // the whole held window is in the +1 half
        p.lfo1_to.cutoff = 0.4;
        let open = held_peak(filtered_sine(FilterModel::Off, 0.0), 69, 1.0);
        let rel = db(held_peak(p, 69, 1.0) / open);
        assert!((rel + 12.04).abs() < 0.1, "{rel:.2} dB");
    }

    #[test]
    fn lfo_pwm_route_moves_the_pulse_width() {
        let mut p = sine_patch();
        p.osc1.wave = Waveform::Pulse;
        square_lfo(&mut p);
        p.lfo1.rate = 0.5;
        // 50 % + 45 % = 95 %: with DC removed the pulse swings to -1.9.
        p.lfo1_to.pwm = 1.0;
        assert!((held_peak(p, 69, 1.0) - 1.9).abs() < 0.01);
        p.lfo1_to.pwm = 0.0;
        assert!((held_peak(p, 69, 1.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn lfo_delay_and_fade_hold_the_vibrato_back() {
        let mut p = sine_patch();
        square_lfo(&mut p);
        p.lfo1.mode = LfoMode::Mono;
        p.lfo1.delay = 0.5;
        p.lfo1.fade = 0.2;
        p.lfo1_to.pitch = 1.0;
        let mut s = synth_with(p);
        s.note_on(None, 0, 69, 1.0);
        let x = run(&mut s, SR as usize);
        // Still in the delay: no vibrato at all.
        assert!((measured_freq(&x[2_400..23_000]) - 440.0).abs() < 0.05);
        let v = &s.voices.iter().find(|v| v.is_sounding()).unwrap().lfo_out;
        assert_eq!(v[0].abs(), 1.0, "fully faded in after delay + fade");
    }

    #[test]
    fn mono_lfo_is_shared_and_poly_lfos_are_per_note() {
        for (mode, shared) in [(LfoMode::Mono, true), (LfoMode::PolyRetrigger, false), (LfoMode::Poly, false)] {
            let mut p = sine_patch();
            p.lfo1 = LfoSettings { shape: LfoShape::SawUp, rate: 3.0, mode, ..p.lfo1 };
            let mut s = synth_with(p);
            s.note_on(None, 0, 60, 1.0);
            run(&mut s, 7_000);
            s.note_on(None, 0, 64, 1.0);
            run(&mut s, 100);
            let outs: Vec<f32> = s.voices.iter().filter(|v| v.is_sounding()).map(|v| v.lfo_out[0]).collect();
            assert_eq!(outs[0] == outs[1], shared, "{mode:?}: {outs:?}");
        }
    }

    /// A patch with only the given matrix slots (the default wheel/pressure
    /// slots cleared).
    fn with_slots(mut p: Patch, slots: &[ModSlot]) -> Patch {
        p.mod_slots = [ModSlot::OFF; MOD_SLOTS];
        p.mod_slots[..slots.len()].copy_from_slice(slots);
        p
    }

    fn slot(source: ModSource, dest: ModDest, amount: f32, via: ModSource) -> ModSlot {
        ModSlot { source, dest, amount, via }
    }

    /// Steady pitch of a held note after `setup` runs on the new synth.
    fn held_freq(p: Patch, note: u8, setup: impl FnOnce(&mut Synth)) -> f64 {
        let mut s = synth_with(p);
        setup(&mut s);
        s.note_on(None, 0, note, 1.0);
        measured_freq(&run(&mut s, SR as usize / 2)[4_800..])
    }

    #[test]
    fn default_slots_wait_for_the_wheel_and_pressure() {
        // Untouched controllers: no vibrato, filter as set.
        let p = sine_patch();
        assert!((held_freq(p, 69, |_| {}) - 440.0).abs() < 0.05);
        // Full mod wheel: the default LFO 1 -> Pitch slot adds vibrato.
        let mut s = synth_with(p);
        s.set_mod_wheel(1.0);
        s.note_on(None, 0, 69, 1.0);
        let x = run(&mut s, SR as usize);
        let wobble = (0..4).map(|i| measured_freq(&x[4_800 + i * 2_400..4_800 + (i + 1) * 2_400])).collect::<Vec<_>>();
        let spread = wobble.iter().cloned().fold(f64::MIN, f64::max) - wobble.iter().cloned().fold(f64::MAX, f64::min);
        assert!(spread > 5.0, "wheel should bring in vibrato: {wobble:?}");
    }

    #[test]
    fn via_scales_a_slot_by_its_source() {
        // Square LFO (+1 for the first half second), LFO 1 -> Pitch +50 %
        // (12 st) via the mod wheel.
        let mut p = sine_patch();
        square_lfo(&mut p);
        p.lfo1.rate = 0.5;
        let p = with_slots(p, &[slot(ModSource::Lfo1, ModDest::Pitch, 0.5, ModSource::ModWheel)]);
        assert!((held_freq(p, 69, |_| {}) - 440.0).abs() < 0.05, "wheel at 0 mutes it");
        assert!((held_freq(p, 69, |s| s.set_mod_wheel(1.0)) - 880.0).abs() < 0.1);
        assert!((held_freq(p, 69, |s| s.set_mod_wheel(0.5)) - 440.0 * 2f64.sqrt()).abs() < 0.1);
    }

    #[test]
    fn pitch_bend_uses_the_bend_range() {
        let mut p = sine_patch();
        p.mod_slots = [ModSlot::OFF; MOD_SLOTS];
        let up = held_freq(p, 69, |s| s.set_pitch_bend(1.0));
        assert!((up - 440.0 * 2f64.powf(2.0 / 12.0)).abs() < 0.1, "default +-2 st: {up}");
        p.bend_range = 12.0;
        assert!((held_freq(p, 69, |s| s.set_pitch_bend(-1.0)) - 220.0).abs() < 0.1);
    }

    #[test]
    fn per_oscillator_pitch_destinations() {
        // OSC 2 alone, Velocity -> Osc 2 Pitch +50 % = +12 st; OSC 1 Pitch
        // must not affect it.
        let mut p = sine_patch();
        p.osc1.level = 0.0;
        p.osc2 = OscSettings { wave: Waveform::Sine, fine: 0.0, level: 1.0, ..p.osc2 };
        let p = with_slots(
            p,
            &[slot(ModSource::Velocity, ModDest::Osc2Pitch, 0.5, ModSource::None), slot(ModSource::Velocity, ModDest::Osc1Pitch, -0.5, ModSource::None)],
        );
        assert!((held_freq(p, 69, |_| {}) - 880.0).abs() < 0.1);
    }

    #[test]
    fn key_source_can_replace_key_tracking() {
        // Key is +-1 per 60 semitones; x 10 octaves x 50 % = 1 octave per
        // octave, i.e. 100 % key tracking.
        let p = with_slots(filtered_sine(FilterModel::Ladder, util::note_to_freq(60.0)), &[slot(ModSource::Key, ModDest::Cutoff, 0.5, ModSource::None)]);
        let open = filtered_sine(FilterModel::Off, 0.0);
        for note in [48u8, 72, 84] {
            let rel = db(held_peak(p, note, 1.0) / held_peak(open, note, 1.0));
            assert!((rel + 12.04).abs() < 0.15, "note {note}: {rel:.2} dB");
        }
    }

    #[test]
    fn mod_env_and_lfo2_are_routable() {
        // ENV 3 held at full sustain -> Cutoff +20 % = 2 octaves onto the note.
        let mut p = filtered_sine(FilterModel::Ladder, 110.0);
        p.mod_env.sustain = 1.0;
        let p = with_slots(p, &[slot(ModSource::ModEnv, ModDest::Cutoff, 0.2, ModSource::None)]);
        let open = held_peak(filtered_sine(FilterModel::Off, 0.0), 69, 1.0);
        assert!((db(held_peak(p, 69, 1.0) / open) + 12.04).abs() < 0.1);
        // LFO 2 as a held square -> Cutoff, the same.
        let mut p = filtered_sine(FilterModel::Ladder, 110.0);
        p.lfo2 = LfoSettings { shape: LfoShape::Square, rate: 0.5, mode: LfoMode::PolyRetrigger, ..p.lfo2 };
        let p = with_slots(p, &[slot(ModSource::Lfo2, ModDest::Cutoff, 0.2, ModSource::None)]);
        assert!((db(held_peak(p, 69, 1.0) / open) + 12.04).abs() < 0.1);
    }

    #[test]
    fn aftertouch_channel_and_poly() {
        // Default slot 2: Aftertouch -> Cutoff +20 % (2 octaves).
        let p = filtered_sine(FilterModel::Ladder, 110.0);
        let open = held_peak(filtered_sine(FilterModel::Off, 0.0), 69, 1.0);
        let level = |pressure: &dyn Fn(&mut Synth)| {
            let mut s = synth_with(p);
            s.note_on(Some(1), 0, 69, 1.0);
            pressure(&mut s);
            let x = run(&mut s, SR as usize / 2);
            db(x[4_800..].iter().fold(0f32, |m, v| m.max(v.abs())) / open)
        };
        assert!((level(&|s| s.set_aftertouch(1.0)) + 12.04).abs() < 0.1, "channel pressure");
        assert!((level(&|s| s.set_poly_pressure(Some(1), 0, 69, 1.0)) + 12.04).abs() < 0.1, "poly pressure");
        assert!(level(&|_| {}) < -40.0, "no pressure: cutoff stays 2 octaves down");
    }

    #[test]
    fn pan_moves_voices_across_the_stereo_field() {
        let p = with_slots(sine_patch(), &[slot(ModSource::Velocity, ModDest::Pan, 1.0, ModSource::None)]);
        let mut s = synth_with(p);
        s.note_on(None, 0, 69, 1.0); // velocity 1: hard right
        let (mut l, mut r) = (vec![0.0; 9_600], vec![0.0; 9_600]);
        s.render(&mut l, &mut r);
        let peak = |x: &[f32]| x[4_800..].iter().fold(0f32, |m, v| m.max(v.abs()));
        assert_eq!(peak(&l), 0.0);
        assert!((peak(&r) - 1.0).abs() < 1e-3, "centre-unity balance law: full level on the right");
    }

    #[test]
    fn amp_destination_and_random_source() {
        // Mod wheel -> Amp -100 %: wheel up silences the voice.
        let p = with_slots(sine_patch(), &[slot(ModSource::ModWheel, ModDest::Amp, -1.0, ModSource::None)]);
        let mut s = synth_with(p);
        s.set_mod_wheel(1.0);
        s.note_on(None, 0, 69, 1.0);
        assert!(run(&mut s, 9_600)[4_800..].iter().all(|&v| v == 0.0));
        // Random -> Osc 1 Pitch: each note lands somewhere different, within +-1 st.
        let p = with_slots(sine_patch(), &[slot(ModSource::Random, ModDest::Osc1Pitch, 1.0 / 24.0, ModSource::None)]);
        let mut s = synth_with(p);
        let mut freqs = Vec::new();
        for _ in 0..3 {
            s.note_on(None, 0, 69, 1.0);
            freqs.push(measured_freq(&run(&mut s, 9_600)[2_400..]));
            s.choke(None, 0, 69);
        }
        assert!(freqs.iter().all(|f| (f / 440.0).log2().abs() <= 1.0 / 12.0 + 1e-4), "{freqs:?}");
        assert!(freqs[0] != freqs[1] && freqs[1] != freqs[2], "{freqs:?}");
    }

    #[test]
    fn envelope_time_destinations() {
        // Linear 100 ms attack; Velocity -> Env Attack +25 % = 1 octave longer
        // (200 ms), so at 100 ms the level is 0.5 rather than 1.
        let mut p = sine_patch();
        p.amp_env.attack = 0.1;
        p.amp_env.attack_curve = 0.0;
        let level_at_100ms = |p: Patch| {
            let mut s = synth_with(p);
            s.note_on(None, 0, 69, 1.0);
            run(&mut s, 4_800);
            s.voices.iter().find(|v| v.is_sounding()).unwrap().env_out_amp()
        };
        assert!((level_at_100ms(p) - 1.0).abs() < 1e-3);
        let slow = with_slots(p, &[slot(ModSource::Velocity, ModDest::EnvAttack, 0.25, ModSource::None)]);
        assert!((level_at_100ms(slow) - 0.5).abs() < 0.01, "{}", level_at_100ms(slow));
    }

    #[test]
    fn lfo_rate_destination_speeds_up_per_voice_lfos() {
        let mut p = sine_patch();
        p.lfo1 = LfoSettings { shape: LfoShape::SawUp, rate: 2.0, mode: LfoMode::PolyRetrigger, ..p.lfo1 };
        // Velocity -> LFO 1 Rate +25 % = 1 octave: 4 Hz.
        let p = with_slots(p, &[slot(ModSource::Velocity, ModDest::Lfo1Rate, 0.25, ModSource::None)]);
        let mut s = synth_with(p);
        s.note_on(None, 0, 69, 1.0);
        let mut wraps = 0;
        let mut last = -1.0;
        // 1.1 s: resets at 0.25, 0.5, 0.75 and 1.0 s (the last one would sit
        // exactly on the boundary of a 1 s window).
        for _ in 0..(1.1 * SR) as usize {
            run(&mut s, 1);
            let v = s.voices.iter().find(|v| v.is_sounding()).unwrap().lfo_out[0];
            if v < last {
                wraps += 1;
            }
            last = v;
        }
        assert_eq!(wraps, 4, "4 Hz saw: a reset every 0.25 s");
    }

    fn voice_patch(mode: VoiceMode) -> Patch {
        let mut p = sine_patch();
        p.mod_slots = [ModSlot::OFF; MOD_SLOTS];
        p.voice.mode = mode;
        p
    }

    fn sounding_notes(s: &Synth) -> Vec<u8> {
        let mut n: Vec<u8> = s.voices.iter().filter(|v| v.is_sounding()).map(|v| v.note).collect();
        n.sort();
        n
    }

    fn freq_now(s: &mut Synth) -> f64 {
        measured_freq(&run(s, 4_800))
    }

    #[test]
    fn unison_stacks_detuned_voices_across_the_stereo_field() {
        let mut p = voice_patch(VoiceMode::Poly);
        p.osc1.phase = PhaseMode::Reset;
        p.voice.unison = 3;
        p.voice.detune = 20.0;
        p.voice.spread = 1.0;
        let mut s = synth_with(p);
        s.note_on(None, 0, 69, 1.0);
        assert_eq!(s.active_voices(), 3);
        // Outer voices +-20 cents, the middle one on pitch.
        let mut cents: Vec<f64> = s
            .voices
            .iter()
            .filter(|v| v.is_sounding())
            .map(|v| 1200.0 * (v.osc_dts().0 as f64 * SR as f64 / 440.0).log2())
            .collect();
        cents.sort_by(f64::total_cmp);
        for (c, want) in cents.iter().zip([-20.0, 0.0, 20.0]) {
            assert!((c - want).abs() < 0.01, "{cents:?}");
        }
        // (Not `run`: panned voices make left and right differ.)
        s.render(&mut [0.0; 10], &mut [0.0; 10]);
        let mut pans: Vec<f32> = s.voices.iter().filter(|v| v.is_sounding()).map(|v| v.pan).collect();
        pans.sort_by(f32::total_cmp);
        assert_eq!(pans, [-1.0, 0.0, 1.0]);
        // One note-off releases the whole stack.
        s.note_off(None, 0, 69);
        assert!(s.voices.iter().all(|v| v.state != VoiceState::Held));
    }

    #[test]
    fn unison_level_is_normalised() {
        // Identical in-phase voices (no detune): N voices x 1/sqrt(N) = sqrt(N).
        let mut p = voice_patch(VoiceMode::Poly);
        p.osc1.phase = PhaseMode::Reset;
        p.voice.detune = 0.0;
        p.voice.spread = 0.0;
        for n in [1usize, 4] {
            p.voice.unison = n;
            assert!((held_peak(p, 69, 1.0) - (n as f32).sqrt()).abs() < 1e-3, "unison {n}");
        }
    }

    #[test]
    fn polyphony_counts_unison_voices() {
        let mut p = voice_patch(VoiceMode::Poly);
        p.polyphony = 8;
        p.voice.unison = 4;
        let mut s = synth_with(p);
        for n in [60u8, 64, 67] {
            s.note_on(None, 0, n, 1.0);
        }
        // 8 voices / 4 per note: the third note steals the first.
        assert_eq!(sounding_notes(&s), [64, 64, 64, 64, 67, 67, 67, 67]);
    }

    #[test]
    fn mono_uses_last_note_priority() {
        let mut s = synth_with(voice_patch(VoiceMode::Mono));
        s.note_on(None, 0, 60, 1.0);
        s.note_on(None, 0, 64, 1.0);
        s.note_on(None, 0, 67, 1.0);
        assert_eq!(sounding_notes(&s), [67]);
        // Releasing a key that isn't sounding changes nothing.
        s.note_off(None, 0, 64);
        assert_eq!(sounding_notes(&s), [67]);
        // Releasing the sounding key falls back to the newest one still held.
        s.note_off(None, 0, 67);
        assert_eq!(sounding_notes(&s), [60]);
        assert!((freq_now(&mut s) - 261.63).abs() < 0.05);
        s.note_off(None, 0, 60);
        assert!(s.voices.iter().all(|v| v.state != VoiceState::Held));
    }

    #[test]
    fn mono_retriggers_and_legato_does_not() {
        for (mode, retriggers) in [(VoiceMode::Mono, true), (VoiceMode::Legato, false)] {
            let mut p = voice_patch(mode);
            p.filter_env.decay = 0.05;
            p.filter_env.sustain = 0.0; // the filter env falls to 0 and stays there
            let mut s = synth_with(p);
            s.note_on(None, 0, 60, 1.0);
            run(&mut s, 9_600);
            s.note_on(None, 0, 64, 1.0); // overlapping
            run(&mut s, 240);
            let v = s.voices.iter().find(|v| v.is_sounding()).unwrap();
            assert_eq!(v.filter_env_level() > 0.1, retriggers, "{mode:?}");
            // Legato: playing after letting go does retrigger.
            s.note_off(None, 0, 60);
            s.note_off(None, 0, 64);
            run(&mut s, 100);
            s.note_on(None, 0, 67, 1.0);
            run(&mut s, 240);
            assert!(s.voices.iter().find(|v| v.is_sounding()).unwrap().filter_env_level() > 0.1, "{mode:?} fresh note");
        }
    }

    #[test]
    fn retrigger_from_zero_restarts_the_amp() {
        for (retrigger, dips) in [(Retrigger::FromCurrent, false), (Retrigger::FromZero, true)] {
            let mut p = voice_patch(VoiceMode::Mono);
            p.voice.retrigger = retrigger;
            let mut s = synth_with(p);
            s.note_on(None, 0, 60, 1.0);
            run(&mut s, 4_800);
            s.note_on(None, 0, 64, 1.0);
            run(&mut s, 1);
            let level = s.voices.iter().find(|v| v.is_sounding()).unwrap().env_out_amp();
            assert_eq!(level < 0.1, dips, "{retrigger:?}: amp at {level}");
        }
    }

    #[test]
    fn glide_slides_between_notes() {
        let mut p = voice_patch(VoiceMode::Mono);
        p.voice.glide = 0.2;
        let mut s = synth_with(p);
        s.note_on(None, 0, 57, 1.0); // 220 Hz
        run(&mut s, 4_800);
        s.note_on(None, 0, 69, 1.0); // to 440 Hz
        let early = freq_now(&mut s);
        assert!(early > 230.0 && early < 400.0, "mid-glide {early}");
        run(&mut s, (0.3 * SR) as usize);
        assert!((freq_now(&mut s) - 440.0).abs() < 0.5);
    }

    #[test]
    fn legato_glide_only_glides_overlapping_notes() {
        let mut p = voice_patch(VoiceMode::Legato);
        p.voice.glide = 0.2;
        p.voice.glide_mode = GlideMode::Legato;
        let mut s = synth_with(p);
        s.note_on(None, 0, 57, 1.0);
        run(&mut s, 4_800);
        s.note_off(None, 0, 57);
        s.note_on(None, 0, 69, 1.0); // detached: no glide
        assert!((freq_now(&mut s) - 440.0).abs() < 0.5);
        s.note_on(None, 0, 81, 1.0); // overlapping: glides up from 440
        let mid = freq_now(&mut s);
        assert!(mid > 450.0 && mid < 850.0, "mid-glide {mid}");
    }

    #[test]
    fn poly_glide_slides_each_new_note_from_the_last() {
        let mut p = voice_patch(VoiceMode::Poly);
        p.voice.glide = 0.2;
        let mut s = synth_with(p);
        s.note_on(None, 0, 57, 1.0);
        run(&mut s, 4_800);
        s.choke(None, 0, 57);
        s.note_on(None, 0, 69, 1.0);
        let early = freq_now(&mut s);
        assert!(early > 230.0 && early < 400.0, "{early}");
    }

    #[test]
    fn mono_reports_the_replaced_note_as_ended() {
        let mut s = synth_with(voice_patch(VoiceMode::Legato));
        s.note_on(Some(1), 0, 60, 1.0);
        s.note_on(Some(2), 0, 64, 1.0);
        assert_eq!(ends(&mut s), [VoiceEnd { voice_id: Some(1), channel: 0, note: 60 }]);
        // Note-off for the new ID releases it.
        s.note_off(Some(2), 0, 64);
        s.note_off(Some(1), 0, 60);
        assert!(s.voices.iter().all(|v| v.state != VoiceState::Held));
    }

    #[test]
    fn drift_detunes_each_voice_a_little_and_differently() {
        let mut p = voice_patch(VoiceMode::Poly);
        let notes = [57u8, 60, 64, 69];
        let pitches = |p: Patch| {
            let mut s = synth_with(p);
            for n in notes {
                s.note_on(None, 0, n, 1.0);
            }
            run(&mut s, 100);
            let mut v: Vec<(u8, f64)> = s
                .voices
                .iter()
                .filter(|v| v.is_sounding())
                .map(|v| (v.note, 1200.0 * (v.osc_dts().0 as f64 * SR as f64 / util::note_to_freq(v.note as f32) as f64).log2()))
                .collect();
            v.sort_by_key(|(n, _)| *n);
            v.into_iter().map(|(_, c)| c).collect::<Vec<_>>()
        };
        assert!(pitches(p).iter().all(|c| c.abs() < 0.01), "no drift: exact");
        p.voice.drift = 1.0;
        let cents = pitches(p);
        assert!(cents.iter().all(|c| c.abs() <= DRIFT_TOLERANCE_CENTS as f64 + 0.01), "{cents:?}");
        assert!(cents.windows(2).any(|w| (w[0] - w[1]).abs() > 0.1), "voices should differ: {cents:?}");
    }

    #[test]
    fn drift_is_a_matrix_source() {
        let p = with_slots(voice_patch(VoiceMode::Poly), &[slot(ModSource::Drift, ModDest::Pitch, 0.5, ModSource::None)]);
        let mut s = synth_with(p);
        s.note_on(None, 0, 69, 1.0);
        let x = run(&mut s, SR as usize * 4);
        // A slow wander of up to +-12 st: the pitch clearly moves over 4 s.
        let a = measured_freq(&x[..SR as usize / 2]);
        let b = measured_freq(&x[SR as usize * 3..SR as usize * 7 / 2]);
        assert!((a - b).abs() > 1.0, "{a} vs {b}");
    }

    #[test]
    fn switching_voice_mode_lets_go_cleanly() {
        let mut s = synth_with(voice_patch(VoiceMode::Poly));
        s.note_on(None, 0, 60, 1.0);
        s.note_on(None, 0, 64, 1.0);
        s.set_patch(&voice_patch(VoiceMode::Mono));
        assert!(s.voices.iter().all(|v| v.state != VoiceState::Held));
        s.note_on(None, 0, 67, 1.0);
        run(&mut s, 10);
        assert_eq!(s.voices.iter().filter(|v| v.state == VoiceState::Held).count(), 1);
    }

    fn render_stereo(s: &mut Synth, n: usize) -> (Vec<f32>, Vec<f32>) {
        let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
        s.render(&mut l, &mut r);
        (l, r)
    }

    #[test]
    fn chorus_widens_a_centred_note() {
        for mode in [ChorusMode::One, ChorusMode::Two, ChorusMode::Ensemble] {
            let mut p = sine_patch();
            p.volume = 0.25;
            p.chorus = ChorusSettings { mode, ..p.chorus };
            let mut s = synth_with(p);
            s.note_on(None, 0, 69, 1.0);
            let (l, r) = render_stereo(&mut s, SR as usize);
            assert!(l.iter().zip(&r).skip(4_800).any(|(a, b)| (a - b).abs() > 0.01), "{mode:?}");
        }
    }

    #[test]
    fn output_clip_keeps_a_hot_patch_under_full_scale() {
        let mut p = sine_patch();
        p.osc1.phase = PhaseMode::Reset;
        p.voice = VoiceSettings { unison: 4, detune: 0.0, spread: 0.0, ..p.voice };
        p.volume = 2.0; // 4 in-phase voices x 1/2 x 2 = 4.0 peak
        let peak = |p: Patch| {
            let mut s = synth_with(p);
            s.note_on(None, 0, 69, 1.0);
            let (l, r) = render_stereo(&mut s, 9_600);
            l.iter().chain(&r).fold(0f32, |m, v| m.max(v.abs()))
        };
        assert!((peak(p) - 4.0).abs() < 0.01);
        p.output_clip = true;
        let clipped = peak(p);
        // Full scale exactly at most (f32 tanh saturates to 1.0 for big inputs).
        assert!(clipped <= 1.0 && clipped > 0.95, "{clipped}");
    }

    #[test]
    fn reset_reuses_the_chorus_buffers() {
        let mut s = synth_with(Patch { chorus: ChorusSettings { mode: ChorusMode::Two, ..Patch::default().chorus }, ..sine_patch() });
        s.note_on(None, 0, 69, 1.0);
        render_stereo(&mut s, 1_000);
        let rate = s.chorus.sample_rate();
        s.reset();
        assert_eq!(s.chorus.sample_rate(), rate, "same chorus, not an empty one");
        let (l, r) = render_stereo(&mut s, 1_000);
        assert!(l.iter().chain(&r).all(|&v| v == 0.0), "silent after reset");
        assert_eq!(s.patch().chorus.mode, ChorusMode::Two);
    }

    /// C8 (4186 Hz) through maximum drive: odd harmonics. At 48 kHz the
    /// 7th (29.3 kHz) and 9th (37.7 kHz) fold back to 18.7 and 10.3 kHz,
    /// inharmonic aliases; HQ must remove them and leave the level alone.
    #[test]
    fn hq_removes_drive_aliases() {
        use super::osc::tests::tone_power;
        let mut p = sine_patch();
        p.osc1.phase = PhaseMode::Reset;
        p.filter.drive_db = filter::MAX_DRIVE_DB;
        let f0 = util::note_to_freq(108.0) as f64;
        let measure = |hq: bool| {
            let mut s = synth_with(Patch { hq, ..p });
            s.note_on(None, 0, 108, 1.0);
            let x = run(&mut s, SR as usize + 4_800);
            let x = &x[4_800..];
            let db = |f: f64| 10.0 * tone_power(x, f).log10();
            (db(f0), db(SR as f64 - 7.0 * f0), db(9.0 * f0 - SR as f64))
        };
        let (fund, alias7, alias9) = measure(false);
        let (fund_hq, alias7_hq, alias9_hq) = measure(true);
        assert!((fund - fund_hq).abs() < 0.05, "fundamental {fund:.2} vs HQ {fund_hq:.2} dB");
        assert!(alias7 - alias7_hq > 30.0, "7th alias {alias7:.1} -> {alias7_hq:.1} dB");
        assert!(alias9 - alias9_hq > 30.0, "9th alias {alias9:.1} -> {alias9_hq:.1} dB");
    }

    #[test]
    fn hq_keeps_pitch_and_timing() {
        // Same note, same envelope times: the voice rate doubles but
        // everything is scaled for it.
        let mut p = sine_patch();
        p.amp_env.attack = 0.1;
        p.amp_env.attack_curve = 0.0;
        for hq in [false, true] {
            let mut s = synth_with(Patch { hq, ..p });
            s.note_on(None, 0, 69, 1.0);
            let x = run(&mut s, SR as usize);
            assert!((measured_freq(&x[9_600..]) - 440.0).abs() < 0.05, "hq {hq}");
            // Linear 100 ms attack: halfway up at 50 ms (the HQ filter adds ~0.3 ms).
            // One 440 Hz cycle (109 samples) ending just after 50 ms.
            let at_50ms = x[2_350..2_459].iter().fold(0f32, |m, v| m.max(v.abs()));
            assert!((at_50ms - 0.5).abs() < 0.02, "hq {hq}: {at_50ms}");
        }
    }

    /// CPU benchmark (run in release: `cargo test -p dsp-core --release
    /// synth::tests::bench -- --ignored --nocapture`). 16 voices with
    /// everything on: two oscillators + sub + noise + ring, sync and FM, the
    /// ladder with drive and resonance, three envelopes, per-voice LFOs, a
    /// full matrix, drift, and the ensemble.
    #[test]
    #[ignore]
    fn bench() {
        let mut p = bench_patch();
        for hq in [false, true] {
            p.hq = hq;
            let mut s = synth_with(p);
            for n in 0..MAX_VOICES as u8 {
                s.note_on(None, 0, 40 + n * 2, 0.8);
            }
            let secs = 10.0;
            let (mut l, mut r) = (vec![0.0; 512], vec![0.0; 512]);
            let start = std::time::Instant::now();
            for _ in 0..(secs * SR / 512.0) as usize {
                s.render(&mut l, &mut r);
            }
            let took = start.elapsed().as_secs_f64();
            println!(
                "bench (hq {hq}): {secs} s of 16 voices in {took:.3} s = {:.1} % of one core ({:.0}x realtime)",
                100.0 * took / secs as f64,
                secs as f64 / took
            );
        }
    }

    /// Editing while playing: `bench`'s patch, with cutoff, detune, drive,
    /// an envelope time and a matrix amount changing every block (as a
    /// knob or automation would), at several block sizes. A second pass
    /// also plays a new note every 50 ms, so voices are stolen. Reports
    /// mean and worst block time against the block's real-time budget.
    /// `cargo test -p dsp-core --release synth::tests::bench_editing -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn bench_editing() {
        let base = bench_patch();
        for churn in [false, true] {
            for block in [1usize, 4, 32, 128] {
                let mut s = synth_with(base);
                for n in 0..MAX_VOICES as u8 {
                    s.note_on(None, 0, 40 + n * 2, 0.8);
                }
                let secs = 4.0;
                let blocks = (secs * SR) as usize / block;
                let (mut l, mut r) = (vec![0.0; block], vec![0.0; block]);
                let churn_every = (0.05 * SR) as usize / block.max(1);
                let (mut total, mut worst) = (0.0f64, 0.0f64);
                for b in 0..blocks {
                    // A slow triangle, like a hand on a knob.
                    let x = (b * block) as f32 / SR;
                    let tri = 1.0 - (2.0 * (x * 2.0).fract() - 1.0).abs();
                    let mut p = base;
                    p.filter.cutoff = 200.0 * 25f32.powf(tri);
                    p.filter.drive_db = 12.0 * tri;
                    p.voice.detune = 30.0 * tri;
                    p.amp_env.attack = 0.001 + 0.05 * tri;
                    p.mod_slots[0].amount = 0.1 * tri;
                    let start = std::time::Instant::now();
                    if churn && churn_every > 0 && b % churn_every == 0 {
                        let key = 40 + ((b / churn_every) % 32) as u8;
                        s.note_off(None, 0, key.wrapping_sub(16).max(40));
                        s.note_on(None, 0, key, 0.8);
                    }
                    s.set_patch(&p);
                    s.render(&mut l, &mut r);
                    let took = start.elapsed().as_secs_f64();
                    total += took;
                    worst = worst.max(took);
                }
                let budget = block as f64 / SR as f64;
                println!(
                    "bench_editing (block {block:>3}, churn {churn:>5}): mean {:5.1} %, worst {:6.1} % of the block budget",
                    100.0 * total / blocks as f64 / budget,
                    100.0 * worst / budget
                );
            }
        }
    }

    /// `bench`'s patch: everything on.
    fn bench_patch() -> Patch {
        let mut p = Patch::default();
        p.osc2 = OscSettings { level: 0.8, ..p.osc2 };
        p.sub.level = 0.5;
        p.noise.level = 0.1;
        p.ring_level = 0.2;
        p.osc2_sync = true;
        p.cross_mod = 1.0;
        p.filter = FilterSettings { cutoff: 1_200.0, resonance: 0.6, drive_db: 6.0, env_amount: 0.3, hpf_cutoff: 80.0, ..p.filter };
        p.lfo1 = LfoSettings { mode: LfoMode::Poly, ..p.lfo1 };
        p.lfo1_to = LfoRoutes { pitch: 0.01, pwm: 0.2, cutoff: 0.1 };
        let all = [ModDest::Cutoff, ModDest::Resonance, ModDest::Pan, ModDest::Osc2Pitch, ModDest::Drive, ModDest::Amp];
        for (slot, dest) in p.mod_slots.iter_mut().zip(all) {
            *slot = ModSlot { source: ModSource::Lfo2, dest, amount: 0.05, via: ModSource::Velocity };
        }
        p.lfo2.mode = LfoMode::Poly;
        p.voice.drift = 1.0;
        p.chorus.mode = ChorusMode::Ensemble;
        p.polyphony = MAX_VOICES;
        p
    }

    #[test]
    fn key_tracking_moves_the_cutoff_with_the_note() {
        // Cutoff at middle C with 100% tracking follows the keyboard, so a
        // sine at any note sits right at the cutoff: always -12 dB.
        let mut p = filtered_sine(FilterModel::Ladder, util::note_to_freq(60.0));
        p.filter.key_track = 1.0;
        let open = filtered_sine(FilterModel::Off, 0.0);
        for note in [48u8, 60, 69, 84] {
            let rel = db(held_peak(p, note, 1.0) / held_peak(open, note, 1.0));
            assert!((rel + 12.04).abs() < 0.15, "note {note}: {rel:.2} dB");
        }
        // Half tracking: an octave up moves the cutoff half an octave.
        assert_eq!(voice::cutoff_offset(&Patch { filter: FilterSettings { key_track: 0.5, ..p.filter }, ..p }, 72, 1.0), 0.5);
    }

    #[test]
    fn velocity_tracking_darkens_soft_notes() {
        let mut p = Patch::default();
        p.filter.velocity = 1.0;
        assert_eq!(voice::cutoff_offset(&p, 60, 1.0), 0.0);
        assert_eq!(voice::cutoff_offset(&p, 60, 0.5), -VELOCITY_TRACK_OCTAVES / 2.0);
        p.filter.velocity = -0.5;
        assert_eq!(voice::cutoff_offset(&p, 60, 0.0), VELOCITY_TRACK_OCTAVES / 2.0);
    }

    #[test]
    fn hpf_is_a_bypass_at_its_minimum_and_cuts_when_raised() {
        let mut p = sine_patch();
        let open = held_peak(p, 69, 1.0);
        assert_eq!(open, held_peak(Patch { filter: FilterSettings { hpf_cutoff: 20.0, ..p.filter }, ..p }, 69, 1.0));
        p.filter.hpf_cutoff = 440.0;
        let rel = db(held_peak(p, 69, 1.0) / open);
        assert!((rel + 3.01).abs() < 0.05, "one-pole at its cutoff: {rel:.2} dB");
    }

    #[test]
    fn every_model_stays_finite_with_everything_maxed() {
        for model in [
            FilterModel::Ladder,
            FilterModel::SvfLowPass,
            FilterModel::SvfBandPass,
            FilterModel::SvfHighPass,
            FilterModel::SvfNotch,
            FilterModel::MsLowPass,
            FilterModel::MsHighPass,
        ] {
            let mut p = Patch::default();
            p.sub.level = 1.0;
            p.noise.level = 1.0;
            p.osc2.level = 1.0;
            p.filter = FilterSettings {
                model,
                cutoff: 300.0,
                resonance: 1.0,
                drive_db: filter::MAX_DRIVE_DB,
                key_track: 2.0,
                velocity: 1.0,
                bass_comp: 1.0,
                env_amount: 1.0,
                hpf_cutoff: 2_000.0,
            };
            p.volume = 1.0;
            let mut s = synth_with(p);
            for n in [24u8, 60, 96, 127] {
                s.note_on(None, 0, n, 1.0);
            }
            let x = run(&mut s, 24_000);
            assert!(x.iter().all(|v| v.is_finite() && v.abs() < 1_000.0), "{model:?}");
        }
    }

    #[test]
    fn cutoff_ramps_in_octaves() {
        let mut s = synth_with(Patch::default());
        let mut p = Patch::default();
        p.filter.cutoff = 100.0;
        s.set_patch(&p);
        // Halfway through the ramp the cutoff is the geometric mean.
        for _ in 1..RAMP_SAMPLES / 2 {
            s.cutoff_oct.tick();
        }
        let mid = s.cutoff_oct.tick().exp2();
        let expected = (100.0f32 * 20_000.0).sqrt();
        assert!((mid / expected - 1.0).abs() < 1e-3, "{mid} Hz");
    }

    fn run(s: &mut Synth, n: usize) -> Vec<f32> {
        let mut l = vec![0.0; n];
        let mut r = vec![0.0; n];
        s.render(&mut l, &mut r);
        assert_eq!(l, r);
        l
    }

    fn ends(s: &mut Synth) -> Vec<VoiceEnd> {
        let mut v = Vec::new();
        s.drain_ended(|e| v.push(e));
        v
    }

    fn state_of(s: &Synth, note: u8) -> Vec<VoiceState> {
        s.voices.iter().filter(|v| v.state != VoiceState::Idle && v.note == note).map(|v| v.state).collect()
    }

    #[test]
    fn note_plays_then_releases_and_reports_end() {
        let mut s = synth_with(sine_patch());
        s.note_on(Some(42), 3, 69, 1.0);
        let held = run(&mut s, 4_800);
        let peak = held.iter().fold(0f32, |m, v| m.max(v.abs()));
        assert!((peak - 1.0).abs() < 1e-3, "peak {peak}");
        assert!(ends(&mut s).is_empty());

        s.note_off(None, 3, 69);
        let tail = run(&mut s, (Patch::default().amp_env.release * SR) as usize + 2);
        assert_eq!(*tail.last().unwrap(), 0.0);
        assert_eq!(s.active_voices(), 0);
        assert_eq!(ends(&mut s), [VoiceEnd { voice_id: Some(42), channel: 3, note: 69 }]);
        assert!(ends(&mut s).is_empty(), "reported only once");
    }

    #[test]
    fn played_pitch_follows_octave_semitone_and_fine() {
        let mut p = sine_patch();
        p.osc1.octave = 1; // 4'
        p.osc1.semitone = -5.0;
        p.osc1.fine = 25.0;
        let mut s = synth_with(p);
        s.note_on(None, 0, 57, 1.0); // A3 = 220 Hz
        let x = run(&mut s, SR as usize * 2);
        let expected = 220.0 * 2f64.powf((12.0 - 5.0 + 0.25) / 12.0);
        let cents = 1200.0 * (measured_freq(&x[480..]) / expected).log2();
        assert!(cents.abs() < 0.1, "off by {cents} cents");
    }

    /// Ring mod is the product of the two oscillators: rendering each alone
    /// and multiplying gives the ring-only output (gate, velocity and volume
    /// all 1 once the attack is done).
    #[test]
    fn ring_mod_is_the_product_of_the_oscillators() {
        let base = || {
            let mut p = sine_patch();
            p.osc1.phase = PhaseMode::Reset;
            p.osc2 = OscSettings { wave: Waveform::Saw, semitone: 7.0, phase: PhaseMode::Reset, level: 0.0, ..p.osc2 };
            p
        };
        let render = |p: Patch| {
            let mut s = synth_with(p);
            s.note_on(None, 0, 57, 1.0);
            run(&mut s, 2_000)
        };
        let osc1 = render(base());
        let osc2 = render(Patch { osc1: OscSettings { level: 0.0, ..base().osc1 }, osc2: OscSettings { level: 1.0, ..base().osc2 }, ..base() });
        let ring = render(Patch { osc1: OscSettings { level: 0.0, ..base().osc1 }, ring_level: 1.0, ..base() });
        for i in 500..2_000 {
            assert!((ring[i] - osc1[i] * osc2[i]).abs() < 1e-5, "sample {i}");
        }
        assert!(ring[500..].iter().any(|v| v.abs() > 0.1));
    }

    /// With sync on, OSC 2 alone sounds at OSC 1's pitch. A sine 4 semitones
    /// below OSC 1, restarted every OSC 1 cycle, crosses zero upward once
    /// per cycle (at each restart).
    #[test]
    fn sync_makes_osc2_follow_osc1_pitch() {
        let mut p = sine_patch();
        p.osc1.level = 0.0;
        p.osc2 = OscSettings { wave: Waveform::Sine, semitone: -4.0, fine: 0.0, level: 1.0, ..p.osc2 };
        let freq = |sync: bool| {
            let mut s = synth_with(Patch { osc2_sync: sync, ..p });
            s.note_on(None, 0, 69, 1.0);
            measured_freq(&run(&mut s, SR as usize)[480..])
        };
        assert!((freq(true) - 440.0).abs() < 0.5, "synced: {}", freq(true));
        let free = 440.0 * 2f64.powf(-4.0 / 12.0);
        assert!((freq(false) - free).abs() < 0.5, "free: {}", freq(false));
    }

    #[test]
    fn retunes_held_notes_when_pitch_changes() {
        let mut s = synth_with(sine_patch());
        s.note_on(None, 0, 69, 1.0);
        let mut p = sine_patch();
        p.osc1.semitone = 12.0;
        s.set_patch(&p);
        let x = run(&mut s, SR as usize);
        assert!((measured_freq(&x[480..]) - 880.0).abs() < 0.1);
    }

    #[test]
    fn note_off_matches_host_voice_id() {
        let mut s = synth_with(sine_patch());
        s.note_on(Some(7), 0, 60, 1.0);
        s.note_on(Some(8), 0, 60, 1.0);
        s.note_off(Some(7), 0, 60);
        let mut by_id: Vec<_> = s.voices.iter().filter(|v| v.state != VoiceState::Idle).map(|v| (v.id, v.state)).collect();
        by_id.sort_by_key(|(id, _)| *id);
        assert_eq!(by_id, [(7, VoiceState::Released), (8, VoiceState::Held)]);
    }

    #[test]
    fn steals_oldest_released_then_oldest_held() {
        let mut p = sine_patch();
        p.polyphony = 3;
        let mut s = synth_with(p);
        s.note_on(None, 0, 60, 1.0);
        s.note_on(None, 0, 61, 1.0);
        s.note_on(None, 0, 62, 1.0);
        s.note_off(None, 0, 61);
        // Full: 61 is released, so it goes first even though 60 is older.
        s.note_on(None, 0, 63, 1.0);
        assert_eq!(state_of(&s, 61), [VoiceState::Fading]);
        assert_eq!(state_of(&s, 60), [VoiceState::Held]);
        // Next steal takes the oldest held note.
        s.note_on(None, 0, 64, 1.0);
        assert_eq!(state_of(&s, 60), [VoiceState::Fading]);
        assert_eq!(s.voices.iter().filter(|v| v.is_sounding()).count(), 3);
    }

    #[test]
    fn stolen_voice_fades_without_a_click_and_reports_end() {
        let mut p = sine_patch();
        p.polyphony = 1;
        let mut s = synth_with(p);
        s.note_on(Some(1), 0, 60, 1.0);
        run(&mut s, 4_800);
        s.note_on(Some(2), 0, 72, 1.0);
        let victim = s.voices.iter().position(|v| v.id == 1).unwrap();
        assert_eq!(s.voices[victim].state, VoiceState::Fading);

        let fade_len = (voice::STEAL_FADE_SECS * SR) as usize;
        let mut gates = vec![s.voices[victim].fade];
        for _ in 0..fade_len + 2 {
            run(&mut s, 1);
            gates.push(s.voices[victim].fade);
        }
        let max_drop = gates.windows(2).map(|w| w[0] - w[1]).fold(0f32, f32::max);
        assert!(max_drop <= 1.0 / fade_len as f32 + 1e-6, "gate dropped {max_drop} in one sample");
        assert_eq!(s.voices[victim].state, VoiceState::Idle);
        assert_eq!(ends(&mut s), [VoiceEnd { voice_id: Some(1), channel: 0, note: 60 }]);
        assert_eq!(state_of(&s, 72), [VoiceState::Held]);
    }

    #[test]
    fn lowering_polyphony_fades_extra_voices() {
        let mut s = synth_with(sine_patch());
        for n in 0..6 {
            s.note_on(None, 0, 60 + n, 1.0);
        }
        let mut p = sine_patch();
        p.polyphony = 2;
        s.set_patch(&p);
        assert_eq!(s.voices.iter().filter(|v| v.is_sounding()).count(), 2);
        assert_eq!(state_of(&s, 64), [VoiceState::Held]);
        assert_eq!(state_of(&s, 65), [VoiceState::Held]);
    }

    #[test]
    fn rapid_steals_never_lose_the_new_note() {
        let mut p = sine_patch();
        p.polyphony = MAX_VOICES;
        let mut s = synth_with(p);
        // Far more note-ons than slots, with no time for fades to finish.
        let mut cut = 0;
        for n in 0..100u8 {
            s.note_on(None, 0, n, 1.0);
            assert_eq!(state_of(&s, n), [VoiceState::Held]);
            cut += ends(&mut s).len();
        }
        assert_eq!(s.voices.iter().filter(|v| v.is_sounding()).count(), MAX_VOICES);
        // Once the spare slots are full of fading voices, each new note cuts one.
        assert_eq!(cut, 100 - POOL);
    }

    #[test]
    fn choke_silences_immediately() {
        let mut s = synth_with(sine_patch());
        s.note_on(None, 0, 60, 1.0);
        run(&mut s, 100);
        s.choke(None, 0, 60);
        assert_eq!(s.active_voices(), 0);
        assert!(run(&mut s, 10).iter().all(|&v| v == 0.0));
        assert_eq!(ends(&mut s).len(), 1);
    }

    #[test]
    fn phase_modes() {
        let mut p = sine_patch();
        p.osc1.phase = PhaseMode::Reset;
        p.osc2.phase = PhaseMode::Free;
        let mut s = synth_with(p);
        s.note_on(None, 0, 60, 1.0);
        s.note_on(None, 0, 64, 1.0);
        let phases: Vec<_> = s.voices.iter().filter(|v| v.is_sounding()).map(|v| v.osc_phases()).collect();
        assert!(phases.iter().all(|(p1, _)| *p1 == 0.0));
        assert_ne!(phases[0].1, phases[1].1, "free-running phases should differ per note");
    }

    #[test]
    fn mixer_levels_ramp_instead_of_jumping() {
        let mut s = synth_with(sine_patch());
        s.note_on(None, 0, 69, 1.0);
        run(&mut s, 4_800);
        let mut p = sine_patch();
        p.volume = 0.0;
        s.set_patch(&p);
        let x = run(&mut s, RAMP_SAMPLES as usize + 10);
        assert!(x[0].abs() > 0.0, "should not cut instantly");
        assert!(x[RAMP_SAMPLES as usize..].iter().all(|&v| v == 0.0), "should reach 0 after the ramp");
    }

    #[test]
    fn extreme_settings_stay_finite_and_bounded() {
        let mut p = Patch::default();
        p.osc1 = OscSettings { wave: Waveform::Pulse, octave: 2, semitone: 12.0, fine: 100.0, pulse_width: 0.05, level: 1.0, phase: PhaseMode::Reset };
        p.osc2 = OscSettings { wave: Waveform::Saw, octave: -1, semitone: -12.0, fine: -100.0, ..p.osc1 };
        p.sub = SubSettings { wave: SubWave::Pulse, octave: SubOctave::Down2, level: 1.0 };
        p.noise = NoiseSettings { color: NoiseColor::Pink, level: 1.0 };
        p.osc2_sync = true;
        p.cross_mod = osc::MAX_CROSS_MOD;
        p.ring_level = 1.0;
        p.polyphony = MAX_VOICES;
        p.volume = 1.0;
        let mut s = synth_with(p);
        for n in [0u8, 1, 12, 60, 100, 115, 120, 126, 127, 127, 64, 30, 90, 2, 3, 4, 5] {
            s.note_on(None, 0, n, 1.0);
        }
        let x = run(&mut s, 48_000);
        // 16 voices x 5 sources at full level, worst case all in phase.
        assert!(x.iter().all(|v| v.is_finite() && v.abs() < 16.0 * 5.0 * 2.0 * 2.0));
    }

    #[test]
    fn rendering_is_deterministic() {
        let render = || {
            let mut p = Patch::default();
            p.osc2.level = 1.0;
            p.noise.level = 0.5;
            let mut s = synth_with(p);
            s.note_on(None, 0, 60, 0.8);
            s.note_on(None, 0, 67, 0.8);
            run(&mut s, 2_000)
        };
        assert_eq!(render(), render());
    }

    #[test]
    fn sanitizes_out_of_range_patches() {
        let d = Patch::default();
        let p = Patch {
            polyphony: 0,
            cross_mod: 9.0,
            ring_level: 2.0,
            osc1: OscSettings { pulse_width: 2.0, ..d.osc1 },
            osc2: OscSettings { level: -1.0, ..d.osc2 },
            ..d
        };
        let s = synth_with(p);
        assert_eq!(s.patch().polyphony, 1);
        assert_eq!(s.patch().osc1.pulse_width, 0.95);
        assert_eq!(s.patch().osc2.level, 0.0);
        assert_eq!(s.patch().cross_mod, osc::MAX_CROSS_MOD);
        assert_eq!(s.patch().ring_level, 1.0);
    }

    #[test]
    fn reset_silences_but_keeps_the_patch() {
        let mut p = sine_patch();
        p.polyphony = 3;
        let mut s = synth_with(p);
        s.note_on(None, 0, 60, 1.0);
        s.reset();
        assert_eq!(s.active_voices(), 0);
        assert_eq!(s.patch().polyphony, 3);
        assert!(ends(&mut s).is_empty());
        let _ = s.voices[0].osc_dts();
    }
}
