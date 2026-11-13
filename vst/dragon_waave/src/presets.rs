//! Factory presets, chosen from the editor's Preset dropdown.
//!
//! A preset is a list of `(parameter ID, value text)` pairs on top of the
//! init patch: every parameter not listed goes back to its default. Values
//! are parsed by each parameter's own parser (the same one hosts use), so
//! they read like the editor ("650", "25 ms", "LP24 Six", "1.2 kHz").
//!
//! The matrix's default slots (1: LFO 1 to Pitch via Mod Wheel, 2:
//! Aftertouch to Cutoff) stay in every preset unless it says otherwise, so
//! the wheel and pressure always do something musical. Presets use slots
//! 3-6.

use crate::params::DragonWaaveParams;
use nih_plug::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Init,
    Brass,
    Pads,
    Strings,
    Keys,
    Bells,
    Leads,
    Bass,
}

impl Category {
    pub fn name(self) -> &'static str {
        match self {
            Category::Init => "Init",
            Category::Brass => "Brass",
            Category::Pads => "Pads",
            Category::Strings => "Strings",
            Category::Keys => "Keys",
            Category::Bells => "Bells",
            Category::Leads => "Leads",
            Category::Bass => "Bass",
        }
    }
}

pub struct Preset {
    pub name: &'static str,
    pub category: Category,
    /// The MIDI note the smoke checks play it with.
    #[cfg_attr(not(test), allow(dead_code))]
    pub test_note: u8,
    pub values: &'static [(&'static str, &'static str)],
}

/// Set every parameter: the preset's value, or the default. Each change is
/// a normal host automation gesture.
pub fn apply(params: &DragonWaaveParams, setter: &ParamSetter, preset: &Preset) {
    let context = setter.raw_context;
    let all = params.param_map();
    // Begin every gesture, set every value, then end them all: the change
    // reads to the host as one edit instead of ~150 separate ones.
    // Safety (all three loops): the pointers come from `params`, which
    // outlives this call.
    for (_, ptr, _) in &all {
        unsafe { context.raw_begin_set_parameter(*ptr) };
    }
    for (id, ptr, _) in &all {
        unsafe {
            let normalized = preset
                .values
                .iter()
                .find(|(pid, _)| pid == id)
                .and_then(|(_, text)| ptr.string_to_normalized_value(text))
                .unwrap_or_else(|| ptr.default_normalized_value());
            context.raw_set_parameter_normalized(*ptr, normalized);
        }
    }
    for (_, ptr, _) in &all {
        unsafe { context.raw_end_set_parameter(*ptr) };
    }
}

const fn preset(
    name: &'static str,
    category: Category,
    test_note: u8,
    values: &'static [(&'static str, &'static str)],
) -> Preset {
    Preset { name, category, test_note, values }
}

use Category::*;

pub const PRESETS: &[Preset] = &[
    preset("Init", Init, 69, &[]),
    // ----- Brass -----
    // Polysix-style: two saws, the LP24 opened by a quick filter envelope,
    // a touch of chorus.
    preset("Six Brass", Brass, 60, &[
        ("osc2_fine", "8"), ("osc2_level", "70"),
        ("flt_cutoff", "650"), ("flt_res", "15"), ("flt_env_amt", "45"), ("flt_keytrack", "50"), ("flt_vel", "30"),
        ("env2_attack", "60 ms"), ("env2_decay", "700 ms"), ("env2_sustain", "45"), ("env2_release", "250 ms"),
        ("env1_attack", "25 ms"), ("env1_release", "250 ms"),
        ("chorus_mode", "I"), ("chorus_mix", "35"), ("voice_drift", "30"),
        ("volume", "-10"),
    ]),
    // Prophet-5-style: wider detune, a slower filter swell, more resonance.
    preset("Prophet Brass", Brass, 60, &[
        ("osc1_fine", "6"), ("osc2_fine", "-6"), ("osc2_level", "90"),
        ("flt_cutoff", "450"), ("flt_res", "25"), ("flt_env_amt", "55"), ("flt_keytrack", "60"),
        ("env2_attack", "140 ms"), ("env2_decay", "1.1 s"), ("env2_sustain", "55"), ("env2_release", "400 ms"),
        ("env1_attack", "90 ms"), ("env1_release", "400 ms"), ("voice_drift", "45"),
    ]),
    // Oberheim-style stab: saw + pulse into the clean SVF, snappy, unison 2.
    preset("OB Stab", Brass, 60, &[
        ("osc2_wave", "Pulse"), ("osc2_pw", "40"), ("osc2_level", "80"), ("osc2_fine", "5"),
        ("flt_type", "SVF LP"), ("flt_cutoff", "1.1 kHz"), ("flt_res", "35"), ("flt_env_amt", "40"),
        ("env2_attack", "2 ms"), ("env2_decay", "260 ms"), ("env2_sustain", "15"), ("env2_release", "150 ms"),
        ("env1_attack", "2 ms"), ("env1_decay", "450 ms"), ("env1_sustain", "65"), ("env1_release", "160 ms"),
        ("voice_unison", "2"), ("voice_detune", "10"), ("voice_spread", "70"),
        ("chorus_mode", "II"), ("chorus_mix", "25"),
    ]),
    // CS-80 / Blade Runner-style: slow swells, delayed vibrato, aftertouch
    // opens the filter further.
    preset("Vangelis Swell", Brass, 60, &[
        ("osc2_fine", "9"), ("osc2_level", "85"),
        ("flt_cutoff", "500"), ("flt_res", "20"), ("flt_env_amt", "40"),
        ("env2_attack", "700 ms"), ("env2_decay", "2 s"), ("env2_sustain", "70"), ("env2_release", "900 ms"),
        ("env1_attack", "350 ms"), ("env1_release", "1.1 s"),
        ("mod2_amt", "35"),
        ("lfo1_shape", "Sine"), ("lfo1_rate", "5.5"), ("lfo1_delay", "600 ms"), ("lfo1_fade", "1.2 s"), ("lfo1_pitch", "3"),
        ("chorus_mode", "I"), ("chorus_mix", "40"), ("voice_drift", "50"),
        ("volume", "-10"),
    ]),
    // Pulse + saw through the MS filter: reedy horns.
    preset("Synth Horns", Brass, 60, &[
        ("osc1_wave", "Pulse"), ("osc1_pw", "30"), ("osc2_level", "50"), ("osc2_fine", "4"),
        ("flt_type", "MS LP"), ("flt_cutoff", "1.4 kHz"), ("flt_res", "25"), ("flt_drive", "4"), ("flt_env_amt", "25"),
        ("env2_attack", "30 ms"), ("env2_decay", "400 ms"), ("env2_sustain", "50"),
        ("env1_attack", "20 ms"), ("env1_release", "180 ms"),
        ("chorus_mode", "I"), ("chorus_mix", "20"),
    ]),
    // A wide unison stack: a whole section on one key.
    preset("Brass Section", Brass, 60, &[
        ("osc2_fine", "7"), ("osc2_level", "80"),
        ("voice_unison", "3"), ("voice_detune", "18"), ("voice_spread", "80"), ("polyphony", "16"),
        ("flt_cutoff", "800"), ("flt_res", "10"), ("flt_env_amt", "35"),
        ("env2_attack", "80 ms"), ("env2_decay", "900 ms"), ("env2_sustain", "60"),
        ("env1_attack", "60 ms"), ("env1_release", "350 ms"),
        ("chorus_mode", "I"), ("chorus_mix", "25"),
        ("volume", "-11"),
    ]),
    // ----- Pads -----
    // The Polysix ensemble on two slow, detuned saws.
    preset("Ensemble Pad", Pads, 60, &[
        ("osc1_fine", "-5"), ("osc2_fine", "7"), ("osc2_level", "90"),
        ("flt_cutoff", "2.4 kHz"), ("flt_res", "10"), ("flt_env_amt", "10"), ("env2_attack", "800 ms"), ("env2_sustain", "70"),
        ("env1_attack", "600 ms"), ("env1_decay", "1 s"), ("env1_sustain", "85"), ("env1_release", "1.5 s"),
        ("hpf_cutoff", "150"), ("chorus_mode", "Ensemble"), ("chorus_mix", "60"), ("voice_drift", "40"),
        ("volume", "-10"),
    ]),
    // Juno-style: one pulse with slow PWM, sub, chorus II.
    preset("Juno Pad", Pads, 60, &[
        ("osc1_wave", "Pulse"), ("sub_level", "40"),
        ("lfo1_shape", "Triangle"), ("lfo1_rate", "0.6"), ("lfo1_pwm", "35"),
        ("flt_cutoff", "3 kHz"), ("flt_res", "10"),
        ("env1_attack", "400 ms"), ("env1_release", "1.2 s"),
        ("hpf_cutoff", "100"), ("chorus_mode", "II"), ("chorus_mix", "55"),
    ]),
    // Triangle and a soft 2' sine in a 3-voice stack: airy and bright.
    preset("Glass Pad", Pads, 60, &[
        ("osc1_wave", "Triangle"), ("osc2_wave", "Sine"), ("osc2_oct", "2'"), ("osc2_level", "35"), ("osc2_fine", "3"),
        ("flt_type", "SVF LP"), ("flt_cutoff", "6 kHz"),
        ("voice_unison", "3"), ("voice_detune", "12"), ("voice_spread", "70"), ("polyphony", "16"),
        ("env1_attack", "900 ms"), ("env1_release", "2 s"),
        ("chorus_mode", "I"), ("chorus_mix", "40"),
    ]),
    // Resonant MS filter slowly breathing on a shared LFO, lots of drift.
    preset("Dark Drift", Pads, 60, &[
        ("osc2_fine", "11"), ("osc2_level", "90"),
        ("flt_type", "MS LP"), ("flt_cutoff", "700"), ("flt_res", "30"),
        ("lfo2_shape", "Sine"), ("lfo2_rate", "0.08"), ("lfo2_mode", "Mono"),
        ("mod3_src", "LFO 2"), ("mod3_dest", "Cutoff"), ("mod3_amt", "15"),
        ("env1_attack", "1.5 s"), ("env1_release", "2.5 s"),
        ("voice_drift", "80"), ("chorus_mode", "Ensemble"), ("chorus_mix", "50"),
    ]),
    // Hard sync swept slowly by the mod envelope.
    preset("Sync Sweep Pad", Pads, 60, &[
        ("osc1_level", "30"), ("osc2_sync", "On"), ("osc2_level", "100"),
        ("env3_attack", "2.5 s"), ("env3_decay", "4 s"), ("env3_sustain", "30"),
        ("mod3_src", "Mod Env"), ("mod3_dest", "Osc 2 Pitch"), ("mod3_amt", "40"),
        ("flt_cutoff", "4 kHz"),
        ("env1_attack", "500 ms"), ("env1_release", "1.8 s"),
        ("chorus_mode", "II"), ("chorus_mix", "40"),
        ("volume", "-11"),
    ]),
    // A band-pass "formant" with a little breath, gently moving.
    preset("Vox Pad", Pads, 60, &[
        ("osc2_fine", "6"), ("osc2_level", "80"), ("noise_color", "Pink"), ("noise_level", "6"),
        ("flt_type", "SVF BP"), ("flt_cutoff", "950"), ("flt_res", "30"),
        ("lfo2_shape", "Triangle"), ("lfo2_rate", "0.3"), ("mod3_src", "LFO 2"), ("mod3_dest", "Cutoff"), ("mod3_amt", "4"),
        ("env1_attack", "700 ms"), ("env1_release", "1.6 s"),
        ("chorus_mode", "Ensemble"), ("chorus_mix", "55"), ("volume", "-7"),
    ]),
    // Saw and triangle with a sub, a slow filter bloom.
    preset("Warm Pad", Pads, 60, &[
        ("osc2_wave", "Triangle"), ("osc2_level", "70"), ("sub_level", "30"),
        ("flt_cutoff", "1.3 kHz"), ("flt_res", "15"), ("flt_env_amt", "15"), ("env2_attack", "1.2 s"), ("env2_sustain", "60"),
        ("env1_attack", "800 ms"), ("env1_release", "2 s"),
        ("voice_drift", "60"), ("chorus_mode", "I"), ("chorus_mix", "45"),
        ("volume", "-10"),
    ]),
    // ----- Strings -----
    // Poly-800-style: saws an octave apart through the ensemble.
    preset("Poly-800 Strings", Strings, 60, &[
        ("osc2_oct", "4'"), ("osc2_level", "50"), ("osc2_fine", "6"),
        ("flt_cutoff", "4 kHz"),
        ("env1_attack", "250 ms"), ("env1_release", "900 ms"), ("env1_vel", "60"),
        ("chorus_mode", "Ensemble"), ("chorus_mix", "70"), ("voice_drift", "30"),
        ("volume", "-10"),
    ]),
    // String-machine-style: thin, bright, heavy ensemble.
    preset("Solina Strings", Strings, 60, &[
        ("osc2_wave", "Pulse"), ("osc2_pw", "30"), ("osc2_oct", "4'"), ("osc2_level", "40"),
        ("hpf_cutoff", "300"), ("flt_cutoff", "6 kHz"),
        ("env1_attack", "150 ms"), ("env1_release", "700 ms"),
        ("chorus_mode", "Ensemble"), ("chorus_mix", "85"), ("chorus_rate", "120"),
        ("volume", "-11"),
    ]),
    // ----- Keys -----
    // DX7 E.Piano-style: a sine carrier (OSC 2) frequency-modulated by a
    // sine at the same pitch (OSC 1, unheard). The mod envelope adds FM at
    // the attack, harder with velocity, for the bell-like "tine" edge.
    preset("FM E.Piano", Keys, 60, &[
        ("osc1_wave", "Sine"), ("osc1_level", "0"), ("osc1_phase", "Reset"),
        ("osc2_wave", "Sine"), ("osc2_level", "100"), ("osc2_fine", "0"), ("osc2_phase", "Reset"), ("osc2_xmod", "0.3"),
        ("flt_type", "Off"),
        ("env3_attack", "0.5 ms"), ("env3_decay", "700 ms"), ("env3_sustain", "0"), ("env3_release", "300 ms"), ("env3_vel", "80"),
        ("mod3_src", "Mod Env"), ("mod3_dest", "Cross Mod"), ("mod3_amt", "40"),
        ("env1_attack", "1 ms"), ("env1_decay", "4 s"), ("env1_sustain", "0"), ("env1_release", "500 ms"), ("env1_vel", "80"),
        ("chorus_mode", "I"), ("chorus_mix", "30"), ("volume", "-8"),
    ]),
    // The modulator 3 octaves up: a short, glassy tine on the attack.
    preset("FM Tine EP", Keys, 60, &[
        ("osc1_wave", "Sine"), ("osc1_level", "0"), ("osc1_oct", "2'"), ("osc1_semi", "12"), ("osc1_phase", "Reset"),
        ("osc2_wave", "Sine"), ("osc2_level", "100"), ("osc2_fine", "0"), ("osc2_phase", "Reset"), ("osc2_xmod", "0.15"),
        ("flt_type", "Off"),
        ("env3_attack", "0.5 ms"), ("env3_decay", "250 ms"), ("env3_sustain", "0"), ("env3_vel", "90"),
        ("mod3_src", "Mod Env"), ("mod3_dest", "Cross Mod"), ("mod3_amt", "20"),
        ("mod4_src", "Velocity"), ("mod4_dest", "Cross Mod"), ("mod4_amt", "6"),
        ("env1_attack", "1 ms"), ("env1_decay", "3 s"), ("env1_sustain", "0"), ("env1_release", "400 ms"), ("env1_vel", "80"),
        ("chorus_mode", "I"), ("chorus_mix", "35"), ("volume", "-7"),
    ]),
    // Wurlitzer-style: FM'd triangle, drive that "barks" with velocity, a
    // gentle tremolo always on. Wheel: deepens the tremolo (slot 1,
    // replacing vibrato).
    preset("Wurli Bark", Keys, 60, &[
        ("osc1_wave", "Sine"), ("osc1_level", "0"),
        ("osc2_wave", "Triangle"), ("osc2_level", "100"), ("osc2_fine", "0"), ("osc2_xmod", "0.6"),
        ("flt_cutoff", "3 kHz"), ("flt_keytrack", "50"), ("flt_drive", "6"),
        ("mod3_src", "Velocity"), ("mod3_dest", "Drive"), ("mod3_amt", "40"),
        ("lfo2_shape", "Sine"), ("lfo2_rate", "5.5"),
        ("mod4_src", "LFO 2"), ("mod4_dest", "Amp"), ("mod4_amt", "-15"),
        ("mod1_src", "LFO 2"), ("mod1_dest", "Amp"), ("mod1_amt", "-20"),
        ("env1_attack", "1 ms"), ("env1_decay", "2.2 s"), ("env1_sustain", "0"), ("env1_release", "200 ms"),
    ]),
    // Clavinet-style: a narrow pulse through a plucked band-pass.
    preset("Poly Clav", Keys, 60, &[
        ("osc1_wave", "Pulse"), ("osc1_pw", "20"),
        ("flt_type", "SVF BP"), ("flt_cutoff", "1.6 kHz"), ("flt_res", "40"), ("flt_env_amt", "30"),
        ("flt_keytrack", "80"), ("flt_vel", "40"),
        ("env2_attack", "0.5 ms"), ("env2_decay", "180 ms"), ("env2_sustain", "0"),
        ("env1_attack", "0.5 ms"), ("env1_decay", "600 ms"), ("env1_sustain", "0"), ("env1_release", "80 ms"),
        ("volume", "-4"),
    ]),
    // Drawbar-ish sines (8', 4', sub) with a fast vibrato and chorus.
    preset("80s Organ", Keys, 60, &[
        ("osc1_wave", "Sine"), ("osc2_wave", "Sine"), ("osc2_oct", "4'"), ("osc2_level", "60"), ("osc2_fine", "0"),
        ("sub_level", "30"), ("flt_type", "Off"),
        ("env1_attack", "2 ms"), ("env1_release", "60 ms"), ("env1_vel", "0"),
        ("lfo1_shape", "Sine"), ("lfo1_rate", "6.5"), ("lfo1_pitch", "1"),
        ("chorus_mode", "II"), ("chorus_mix", "50"), ("volume", "-14"),
    ]),
    // Juno-style plucked keys: hollow pulse, sub, decaying filter.
    preset("Hollow Keys", Keys, 60, &[
        ("osc1_wave", "Pulse"), ("osc1_pw", "30"), ("sub_level", "35"),
        ("flt_cutoff", "1.8 kHz"), ("flt_res", "15"), ("flt_env_amt", "35"),
        ("env2_decay", "500 ms"), ("env2_sustain", "30"),
        ("env1_attack", "2 ms"), ("env1_decay", "1.2 s"), ("env1_sustain", "40"), ("env1_release", "400 ms"),
        ("chorus_mode", "I"), ("chorus_mix", "45"),
        ("volume", "-7"),
    ]),
    // FM electric piano with a stereo auto-pan, always on. Wheel: pans
    // wider (slot 1, replacing vibrato).
    preset("Tremolo EP", Keys, 60, &[
        ("osc1_wave", "Sine"), ("osc1_level", "0"), ("osc1_phase", "Reset"),
        ("osc2_wave", "Sine"), ("osc2_level", "100"), ("osc2_fine", "0"), ("osc2_phase", "Reset"), ("osc2_xmod", "0.3"),
        ("flt_type", "Off"),
        ("env3_attack", "0.5 ms"), ("env3_decay", "700 ms"), ("env3_sustain", "0"), ("env3_release", "300 ms"), ("env3_vel", "90"),
        ("mod3_src", "Mod Env"), ("mod3_dest", "Cross Mod"), ("mod3_amt", "40"),
        ("lfo2_shape", "Sine"), ("lfo2_rate", "4.5"),
        ("mod4_src", "LFO 2"), ("mod4_dest", "Pan"), ("mod4_amt", "40"),
        ("mod1_src", "LFO 2"), ("mod1_dest", "Pan"), ("mod1_amt", "60"),
        ("env1_attack", "1 ms"), ("env1_decay", "4 s"), ("env1_sustain", "0"), ("env1_release", "500 ms"), ("env1_vel", "80"),
        ("volume", "-9"),
    ]),
    // Electric-grand style: a triangle carrier FM'd by a sine an octave
    // up, bright on the attack, harmonic as it rings.
    preset("Tine Grand", Keys, 60, &[
        ("osc1_wave", "Sine"), ("osc1_level", "0"), ("osc1_oct", "4'"), ("osc1_phase", "Reset"),
        ("osc2_wave", "Triangle"), ("osc2_level", "100"), ("osc2_fine", "0"), ("osc2_phase", "Reset"), ("osc2_xmod", "0.2"),
        ("flt_type", "Off"),
        ("env3_attack", "0.5 ms"), ("env3_decay", "400 ms"), ("env3_sustain", "0"), ("env3_vel", "80"),
        ("mod3_src", "Mod Env"), ("mod3_dest", "Cross Mod"), ("mod3_amt", "30"),
        ("env1_attack", "1 ms"), ("env1_decay", "5 s"), ("env1_sustain", "0"), ("env1_release", "400 ms"), ("env1_vel", "80"),
        ("chorus_mode", "I"), ("chorus_mix", "20"),
        ("volume", "-7"),
    ]),
    // 80s analog "piano": saw and pulse through a key-tracked ladder that
    // opens with velocity and closes as the note decays.
    preset("Poly Piano", Keys, 60, &[
        ("osc2_wave", "Pulse"), ("osc2_pw", "40"), ("osc2_level", "100"), ("osc2_fine", "6"),
        ("flt_cutoff", "900"), ("flt_env_amt", "45"), ("flt_vel", "70"), ("flt_keytrack", "100"),
        ("env2_attack", "1 ms"), ("env2_decay", "900 ms"), ("env2_sustain", "10"),
        ("env1_attack", "1 ms"), ("env1_decay", "2.5 s"), ("env1_sustain", "0"), ("env1_release", "300 ms"),
        ("chorus_mode", "I"), ("chorus_mix", "35"),
        ("volume", "-9"),
    ]),
    // Organ with a percussion "click": the 2' harmonic sounds only on the
    // attack (mod env -> OSC 2 level). Wheel: vibrato (default).
    preset("Perc Organ", Keys, 60, &[
        ("osc1_wave", "Sine"), ("osc2_wave", "Sine"), ("osc2_oct", "2'"), ("osc2_level", "0"), ("osc2_fine", "0"),
        ("sub_level", "25"), ("flt_type", "Off"),
        ("env3_attack", "0.5 ms"), ("env3_decay", "180 ms"), ("env3_sustain", "0"),
        ("mod3_src", "Mod Env"), ("mod3_dest", "Osc 2 Level"), ("mod3_amt", "60"),
        ("env1_attack", "2 ms"), ("env1_release", "40 ms"), ("env1_vel", "0"),
        ("lfo1_shape", "Sine"), ("lfo1_rate", "6.8"),
        ("chorus_mode", "II"), ("chorus_mix", "40"),
        ("volume", "-11"),
    ]),
    // Plucked and bright: a narrow pulse plus a saw an octave up, the lows
    // thinned out, a quick decay.
    preset("Harpsichord", Keys, 60, &[
        ("osc1_wave", "Pulse"), ("osc1_pw", "15"),
        ("osc2_oct", "4'"), ("osc2_level", "50"), ("osc2_fine", "0"),
        ("hpf_cutoff", "200"), ("flt_cutoff", "6 kHz"), ("flt_keytrack", "60"),
        ("env1_attack", "0.5 ms"), ("env1_decay", "1.5 s"), ("env1_sustain", "0"), ("env1_release", "120 ms"), ("env1_vel", "30"),
    ]),
    // Modern synth pluck for chords: two detuned saws, a snappy resonant
    // SVF, ensemble.
    preset("Pluck Keys", Keys, 60, &[
        ("osc2_level", "100"), ("osc2_fine", "9"),
        ("flt_type", "SVF LP"), ("flt_cutoff", "600"), ("flt_res", "25"), ("flt_env_amt", "50"), ("flt_vel", "60"),
        ("env2_attack", "0.5 ms"), ("env2_decay", "220 ms"), ("env2_sustain", "0"),
        ("env1_attack", "1 ms"), ("env1_decay", "700 ms"), ("env1_sustain", "0"), ("env1_release", "250 ms"),
        ("chorus_mode", "Ensemble"), ("chorus_mix", "35"),
        ("volume", "-10"),
    ]),
    // Worn, tape-style keys: triangle and a little pink noise, a slow
    // random pitch wow and lots of drift. Wheel: more wow (slot 1,
    // replacing vibrato).
    preset("Lo-Fi Keys", Keys, 60, &[
        ("osc1_wave", "Triangle"), ("noise_color", "Pink"), ("noise_level", "6"),
        ("flt_cutoff", "1.6 kHz"), ("flt_keytrack", "50"),
        ("lfo2_shape", "Smooth Random"), ("lfo2_rate", "0.8"),
        ("mod3_src", "LFO 2"), ("mod3_dest", "Pitch"), ("mod3_amt", "1"),
        ("mod1_src", "LFO 2"), ("mod1_dest", "Pitch"), ("mod1_amt", "2"),
        ("voice_drift", "80"),
        ("env1_decay", "2 s"), ("env1_sustain", "30"), ("env1_release", "400 ms"),
        ("chorus_mode", "I"), ("chorus_mix", "25"),
        ("volume", "-4"),
    ]),
    // Short organ-ish chord stab for house: square plus a saw an octave
    // up, no chorus.
    preset("House Stab", Keys, 60, &[
        ("osc1_wave", "Pulse"),
        ("osc2_oct", "4'"), ("osc2_level", "40"), ("osc2_fine", "5"),
        ("flt_cutoff", "2.2 kHz"), ("flt_res", "10"), ("flt_keytrack", "50"),
        ("env1_attack", "1 ms"), ("env1_decay", "280 ms"), ("env1_sustain", "0"), ("env1_release", "80 ms"),
        ("volume", "-8"),
    ]),
    // ----- Bells -----
    // DX7-style tubular bell: modulator at an inharmonic 3.5 x the carrier
    // (4' + 9.69 st), FM decaying slower than a piano, long ring.
    preset("FM Tubular Bell", Bells, 72, &[
        ("osc1_wave", "Sine"), ("osc1_level", "0"), ("osc1_oct", "4'"), ("osc1_semi", "10"), ("osc1_fine", "-31"),
        ("osc1_phase", "Reset"),
        ("osc2_wave", "Sine"), ("osc2_level", "100"), ("osc2_fine", "0"), ("osc2_phase", "Reset"), ("osc2_xmod", "1"),
        ("flt_type", "Off"),
        ("env3_attack", "0.5 ms"), ("env3_decay", "4 s"), ("env3_sustain", "20"), ("env3_release", "3 s"),
        ("mod3_src", "Mod Env"), ("mod3_dest", "Cross Mod"), ("mod3_amt", "30"),
        ("env1_attack", "1 ms"), ("env1_decay", "7 s"), ("env1_sustain", "0"), ("env1_release", "3 s"),
        ("chorus_mode", "I"), ("chorus_mix", "20"), ("volume", "-10"),
    ]),
    // Ratio sqrt 2 (+6 st): a cold, glassy clang.
    preset("Glass Bell", Bells, 72, &[
        ("osc1_wave", "Sine"), ("osc1_level", "0"), ("osc1_semi", "6"), ("osc1_phase", "Reset"),
        ("osc2_wave", "Sine"), ("osc2_level", "100"), ("osc2_fine", "0"), ("osc2_phase", "Reset"), ("osc2_xmod", "1.2"),
        ("flt_type", "Off"), ("hpf_cutoff", "200"),
        ("env3_attack", "0.5 ms"), ("env3_decay", "1.5 s"), ("env3_sustain", "0"),
        ("mod3_src", "Mod Env"), ("mod3_dest", "Cross Mod"), ("mod3_amt", "25"),
        ("env1_attack", "1 ms"), ("env1_decay", "3.5 s"), ("env1_sustain", "0"), ("env1_release", "2 s"),
        ("chorus_mode", "I"), ("chorus_mix", "30"),
        ("volume", "-10"),
    ]),
    // Ring mod of two sines 2.38 x apart: inharmonic sum and difference tones.
    preset("Ring Bell", Bells, 72, &[
        ("osc1_wave", "Sine"), ("osc1_level", "0"),
        ("osc2_wave", "Sine"), ("osc2_level", "0"), ("osc2_oct", "4'"), ("osc2_semi", "3"), ("osc2_fine", "0"),
        ("mix_ring", "100"), ("flt_type", "Off"),
        ("env1_attack", "1 ms"), ("env1_decay", "4 s"), ("env1_sustain", "0"), ("env1_release", "2 s"),
        ("chorus_mode", "I"), ("chorus_mix", "25"), ("volume", "-9"),
    ]),
    // Tiny, high and plucked.
    preset("Music Box", Bells, 72, &[
        ("osc1_wave", "Triangle"), ("osc1_oct", "2'"),
        ("osc2_wave", "Sine"), ("osc2_oct", "4'"), ("osc2_level", "30"), ("osc2_fine", "0"),
        ("flt_type", "Off"), ("hpf_cutoff", "400"),
        ("env1_attack", "0.5 ms"), ("env1_decay", "1.4 s"), ("env1_sustain", "0"), ("env1_release", "1.2 s"),
        ("chorus_mode", "I"), ("chorus_mix", "25"),
        ("volume", "-11"),
    ]),
    // A 4' sine with a light 2:1 FM ping.
    preset("Celesta", Bells, 72, &[
        ("osc1_wave", "Sine"), ("osc1_level", "0"), ("osc1_oct", "2'"), ("osc1_phase", "Reset"),
        ("osc2_wave", "Sine"), ("osc2_oct", "4'"), ("osc2_level", "100"), ("osc2_fine", "0"), ("osc2_phase", "Reset"),
        ("osc2_xmod", "0.5"), ("flt_type", "Off"),
        ("env3_attack", "0.5 ms"), ("env3_decay", "300 ms"), ("env3_sustain", "0"),
        ("mod3_src", "Mod Env"), ("mod3_dest", "Cross Mod"), ("mod3_amt", "20"),
        ("env1_attack", "1 ms"), ("env1_decay", "1.2 s"), ("env1_sustain", "0"), ("env1_release", "800 ms"),
        ("volume", "-9"),
    ]),
    // A very fast FM thump (2:1) over a short sine: wooden mallets.
    preset("Marimba", Bells, 60, &[
        ("osc1_wave", "Sine"), ("osc1_level", "0"), ("osc1_oct", "4'"), ("osc1_phase", "Reset"),
        ("osc2_wave", "Sine"), ("osc2_level", "100"), ("osc2_fine", "0"), ("osc2_phase", "Reset"), ("osc2_xmod", "0.4"),
        ("flt_type", "Off"),
        ("env3_attack", "0.5 ms"), ("env3_decay", "90 ms"), ("env3_sustain", "0"),
        ("mod3_src", "Mod Env"), ("mod3_dest", "Cross Mod"), ("mod3_amt", "30"),
        ("env1_attack", "0.5 ms"), ("env1_decay", "600 ms"), ("env1_sustain", "0"), ("env1_release", "300 ms"),
        ("env1_vel", "90"), ("volume", "-6"),
    ]),
    // Vibraphone: sine + soft 4' triangle with motor tremolo.
    preset("Vibes", Bells, 72, &[
        ("osc1_wave", "Sine"), ("osc2_wave", "Triangle"), ("osc2_oct", "4'"), ("osc2_level", "25"), ("osc2_fine", "0"),
        ("flt_type", "Off"),
        ("lfo1_shape", "Triangle"), ("lfo1_rate", "4.5"),
        ("mod3_src", "LFO 1"), ("mod3_dest", "Amp"), ("mod3_amt", "-25"),
        ("env1_attack", "1 ms"), ("env1_decay", "3 s"), ("env1_sustain", "0"), ("env1_release", "1.5 s"),
    ]),
    // ----- Leads -----
    // MS-20-style sync lead: the mod envelope sweeps the synced OSC 2.
    preset("MS Sync Lead", Leads, 69, &[
        ("osc1_level", "25"), ("osc2_sync", "On"), ("osc2_level", "100"),
        ("env3_attack", "1 ms"), ("env3_decay", "400 ms"), ("env3_sustain", "25"),
        ("mod3_src", "Mod Env"), ("mod3_dest", "Osc 2 Pitch"), ("mod3_amt", "50"),
        ("flt_type", "MS LP"), ("flt_cutoff", "3 kHz"), ("flt_res", "40"), ("flt_drive", "6"),
        ("voice_mode", "Legato"), ("voice_glide", "60 ms"), ("env1_release", "120 ms"),
    ]),
    // Saw + octave, glide, delayed vibrato.
    preset("Six Lead", Leads, 69, &[
        ("osc2_oct", "4'"), ("osc2_level", "60"), ("osc2_fine", "4"),
        ("flt_cutoff", "2.5 kHz"), ("flt_res", "30"), ("flt_env_amt", "20"),
        ("voice_mode", "Mono"), ("voice_glide", "40 ms"),
        ("lfo1_shape", "Sine"), ("lfo1_rate", "5.5"), ("lfo1_delay", "400 ms"), ("lfo1_fade", "600 ms"), ("lfo1_pitch", "3"),
        ("volume", "-9"),
    ]),
    // Poly-800-style square lead with a sub.
    preset("Square Lead", Leads, 69, &[
        ("osc1_wave", "Pulse"), ("sub_level", "40"),
        ("flt_cutoff", "3 kHz"), ("flt_res", "20"),
        ("voice_mode", "Legato"), ("voice_glide", "30 ms"),
        ("chorus_mode", "I"), ("chorus_mix", "25"),
        ("volume", "-10"),
    ]),
    // Wide unison saws: four stacked, detuned voices per note, gliding
    // legato, vibrato easing in after the attack.
    preset("Stack Lead", Leads, 69, &[
        ("osc2_level", "100"), ("osc2_fine", "7"),
        ("voice_unison", "4"), ("voice_detune", "20"), ("voice_spread", "60"),
        ("flt_cutoff", "4 kHz"), ("flt_res", "15"),
        ("lfo1_shape", "Sine"), ("lfo1_rate", "5.5"), ("lfo1_delay", "400 ms"), ("lfo1_fade", "600 ms"), ("lfo1_pitch", "3"),
        ("voice_mode", "Legato"), ("voice_glide", "40 ms"), ("env1_release", "150 ms"),
    ]),
    // A pure, gliding sine with a faint octave. Aftertouch: vibrato depth
    // (slot 2, replacing aftertouch -> cutoff, since the filter is off).
    preset("Whistle Lead", Leads, 69, &[
        ("osc1_wave", "Sine"), ("osc2_wave", "Sine"), ("osc2_oct", "4'"), ("osc2_level", "15"), ("osc2_fine", "0"),
        ("flt_type", "Off"),
        ("lfo1_shape", "Sine"), ("lfo1_rate", "5.5"), ("lfo1_delay", "300 ms"), ("lfo1_fade", "500 ms"), ("lfo1_pitch", "3"),
        ("mod2_src", "LFO 1"), ("mod2_dest", "Pitch"), ("mod2_amt", "3"), ("mod2_via", "Aftertouch"),
        ("voice_mode", "Mono"), ("voice_glide", "80 ms"),
        ("env1_attack", "10 ms"), ("env1_release", "150 ms"),
        ("volume", "-10"),
    ]),
    // Aggressive: hard sync swept by the mod envelope, a slow LFO keeping
    // it moving, into a driven, resonant MS filter. Wheel: a wider sync
    // sweep (slot 1, replacing vibrato).
    preset("Sync Scream", Leads, 69, &[
        ("osc1_level", "30"), ("osc2_sync", "On"), ("osc2_level", "100"),
        ("env3_attack", "2 ms"), ("env3_decay", "600 ms"), ("env3_sustain", "40"),
        ("mod3_src", "Mod Env"), ("mod3_dest", "Osc 2 Pitch"), ("mod3_amt", "100"),
        ("lfo2_shape", "Triangle"), ("lfo2_rate", "0.5"),
        ("mod4_src", "LFO 2"), ("mod4_dest", "Osc 2 Pitch"), ("mod4_amt", "5"),
        ("mod1_src", "LFO 2"), ("mod1_dest", "Osc 2 Pitch"), ("mod1_amt", "25"),
        ("flt_type", "MS LP"), ("flt_cutoff", "5 kHz"), ("flt_res", "50"), ("flt_drive", "12"), ("hpf_cutoff", "150"),
        ("voice_mode", "Legato"), ("voice_glide", "50 ms"), ("env1_release", "150 ms"),
        ("volume", "-14"),
    ]),
    // Hollow and moving: LFO 1 sweeping the pulse width, a little sub,
    // chorus.
    preset("PWM Lead", Leads, 69, &[
        ("osc1_wave", "Pulse"), ("sub_level", "20"),
        ("lfo1_shape", "Triangle"), ("lfo1_rate", "4"), ("lfo1_pwm", "30"),
        ("flt_cutoff", "3 kHz"), ("flt_res", "20"),
        ("chorus_mode", "I"), ("chorus_mix", "30"),
        ("voice_mode", "Mono"), ("voice_glide", "30 ms"),
        ("volume", "-9"),
    ]),
    // Metallic and vocal: two sines a fifth apart ring-modulated, through
    // a key-tracked band-pass.
    preset("Ring Lead", Leads, 69, &[
        ("osc1_wave", "Sine"), ("osc1_level", "20"),
        ("osc2_wave", "Sine"), ("osc2_semi", "7"), ("osc2_fine", "3"), ("osc2_level", "20"), ("mix_ring", "80"),
        ("flt_type", "SVF BP"), ("flt_cutoff", "700"), ("flt_res", "40"), ("flt_keytrack", "100"),
        ("voice_mode", "Legato"), ("voice_glide", "40 ms"),
        ("volume", "-4"),
    ]),
    // A squelchy, resonant line: a plucked, driven MS filter that
    // brightens with velocity.
    preset("Acid Lead", Leads, 69, &[
        ("flt_type", "MS LP"), ("flt_cutoff", "600"), ("flt_res", "75"), ("flt_drive", "10"), ("flt_env_amt", "55"),
        ("flt_vel", "50"),
        ("env2_attack", "1 ms"), ("env2_decay", "200 ms"), ("env2_sustain", "0"),
        ("voice_mode", "Legato"), ("voice_glide", "60 ms"), ("env1_release", "80 ms"),
        ("volume", "-15"),
    ]),
    // A brassy swell: the filter opens after the note starts. Bend range
    // 12 for dives.
    preset("Bwaa Lead", Leads, 69, &[
        ("osc2_level", "100"), ("osc2_fine", "5"),
        ("flt_cutoff", "500"), ("flt_res", "20"), ("flt_env_amt", "50"),
        ("env2_attack", "150 ms"), ("env2_decay", "400 ms"), ("env2_sustain", "50"),
        ("env1_attack", "20 ms"), ("env1_release", "200 ms"),
        ("voice_mode", "Mono"), ("voice_glide", "50 ms"), ("voice_bend_range", "12"),
        ("volume", "-10"),
    ]),
    // Bright and 8-bit-ish: a narrow pulse, no filter. Wheel: an octave
    // trill. LFO 1's square swings +-6 st (slot 1) and the wheel itself
    // adds +6 st (slot 3), so full wheel alternates the note and the
    // octave above.
    preset("Chip Lead", Leads, 69, &[
        ("osc1_wave", "Pulse"), ("osc1_pw", "12"),
        ("flt_type", "Off"),
        ("lfo1_shape", "Square"), ("lfo1_rate", "12"),
        ("mod1_amt", "25"),
        ("mod3_src", "Mod Wheel"), ("mod3_dest", "Pitch"), ("mod3_amt", "25"),
        ("voice_mode", "Mono"), ("env1_release", "30 ms"),
        ("volume", "-14"),
    ]),
    // ----- Bass -----
    preset("Sub Bass", Bass, 36, &[
        ("osc1_wave", "Sine"), ("sub_level", "50"), ("flt_cutoff", "400"),
        ("voice_mode", "Mono"),
        ("env1_attack", "2 ms"), ("env1_decay", "800 ms"), ("env1_sustain", "80"), ("env1_release", "80 ms"),
        ("volume", "-6"),
    ]),
    // Polysix-style bass: saw + sub, a plucked, driven ladder.
    preset("Six Bass", Bass, 36, &[
        ("sub_level", "60"),
        ("flt_cutoff", "300"), ("flt_res", "35"), ("flt_env_amt", "45"), ("flt_drive", "6"),
        ("env2_attack", "1 ms"), ("env2_decay", "250 ms"), ("env2_sustain", "0"),
        ("env1_decay", "600 ms"), ("env1_sustain", "70"), ("env1_release", "60 ms"),
        ("voice_mode", "Mono"),
        ("volume", "-4"),
    ]),
    // A squelchy, resonant MS bass with glide.
    preset("MS Acid Bass", Bass, 36, &[
        ("flt_type", "MS LP"), ("flt_cutoff", "400"), ("flt_res", "70"), ("flt_env_amt", "50"), ("flt_drive", "8"),
        ("env2_attack", "1 ms"), ("env2_decay", "180 ms"), ("env2_sustain", "0"),
        ("voice_mode", "Legato"), ("voice_glide", "50 ms"), ("env1_release", "60 ms"),
        ("volume", "-9"),
    ]),
    // Two detuned saws, doubled by unison: the slow phasing of a DnB bass.
    // A slow LFO breathes the cutoff. Wheel: opens the filter (slot 1,
    // replacing vibrato).
    preset("Reese Bass", Bass, 36, &[
        ("osc2_level", "100"), ("osc2_fine", "12"),
        ("voice_unison", "2"), ("voice_detune", "15"), ("voice_spread", "30"), ("voice_drift", "40"),
        ("flt_cutoff", "600"), ("flt_res", "10"), ("flt_drive", "6"), ("flt_keytrack", "40"),
        ("lfo2_shape", "Sine"), ("lfo2_rate", "0.3"),
        ("mod3_src", "LFO 2"), ("mod3_dest", "Cutoff"), ("mod3_amt", "5"),
        ("mod1_src", "Mod Wheel"), ("mod1_dest", "Cutoff"), ("mod1_amt", "20"), ("mod1_via", "None"),
        ("voice_mode", "Mono"), ("env1_release", "80 ms"),
        ("volume", "-7"),
    ]),
    // DX-style slap bass: a sine carrier FM'd 1:1, the index snapping shut
    // after the attack. Harder playing = brighter slap.
    preset("FM Bass", Bass, 36, &[
        ("osc1_wave", "Sine"), ("osc1_level", "0"), ("osc1_phase", "Reset"),
        ("osc2_wave", "Sine"), ("osc2_level", "100"), ("osc2_fine", "0"), ("osc2_phase", "Reset"), ("osc2_xmod", "0.5"),
        ("flt_type", "Off"),
        ("env3_attack", "0.5 ms"), ("env3_decay", "150 ms"), ("env3_sustain", "0"), ("env3_vel", "80"),
        ("mod3_src", "Mod Env"), ("mod3_dest", "Cross Mod"), ("mod3_amt", "40"),
        ("env1_attack", "1 ms"), ("env1_decay", "600 ms"), ("env1_sustain", "40"), ("env1_release", "80 ms"),
        ("voice_mode", "Mono"), ("voice_retrig", "From Zero"),
        ("volume", "-5"),
    ]),
    // Funky and tight: a hollow pulse and sub through a resonant, plucked
    // SVF that brightens with velocity.
    preset("Pluck Bass", Bass, 36, &[
        ("osc1_wave", "Pulse"), ("osc1_pw", "35"), ("sub_level", "40"),
        ("flt_type", "SVF LP"), ("flt_cutoff", "350"), ("flt_res", "35"), ("flt_env_amt", "45"), ("flt_vel", "60"),
        ("flt_keytrack", "50"),
        ("env2_attack", "0.5 ms"), ("env2_decay", "120 ms"), ("env2_sustain", "0"),
        ("env1_attack", "1 ms"), ("env1_decay", "500 ms"), ("env1_sustain", "0"), ("env1_release", "60 ms"),
        ("voice_mode", "Mono"),
        ("volume", "-11"),
    ]),
    // 70s pedal-synth weight: two saws an octave apart through a driven
    // ladder with the bass compensation up, gliding legato.
    preset("Pedal Bass", Bass, 36, &[
        ("osc2_level", "100"), ("osc2_oct", "16'"), ("osc2_fine", "-5"),
        ("flt_cutoff", "250"), ("flt_res", "25"), ("flt_drive", "9"), ("flt_bass_comp", "60"), ("flt_keytrack", "40"),
        ("env1_attack", "5 ms"), ("env1_sustain", "90"), ("env1_release", "300 ms"),
        ("voice_mode", "Legato"), ("voice_glide", "80 ms"),
        ("volume", "-7"),
    ]),
    // A sine with a fast downward pitch "thump", kick-drum style. Wheel:
    // deepens the drop (slot 1, replacing vibrato).
    preset("Drop Bass", Bass, 36, &[
        ("osc1_wave", "Sine"), ("osc1_phase", "Reset"), ("sub_level", "30"),
        ("flt_type", "Off"),
        ("env3_attack", "0.5 ms"), ("env3_decay", "60 ms"), ("env3_sustain", "0"),
        ("mod3_src", "Mod Env"), ("mod3_dest", "Pitch"), ("mod3_amt", "50"),
        ("mod1_src", "Mod Env"), ("mod1_dest", "Pitch"), ("mod1_amt", "50"),
        ("env1_sustain", "80"), ("env1_release", "80 ms"),
        ("voice_mode", "Mono"), ("voice_retrig", "From Zero"),
        ("volume", "-6"),
    ]),
    // Tempo-synced 1/8 filter wobble, restarting with each note. Always
    // on; the wheel adds up to 1.5 more octaves (slot 1, replacing
    // vibrato).
    preset("Wobble Bass", Bass, 36, &[
        ("osc2_wave", "Pulse"), ("osc2_level", "100"),
        ("flt_cutoff", "500"), ("flt_res", "45"), ("flt_drive", "10"), ("flt_keytrack", "40"),
        ("lfo2_shape", "Sine"), ("lfo2_sync", "On"), ("lfo2_division", "1/8"), ("lfo2_mode", "Poly Retrig"),
        ("mod3_src", "LFO 2"), ("mod3_dest", "Cutoff"), ("mod3_amt", "15"),
        ("mod1_src", "LFO 2"), ("mod1_dest", "Cutoff"), ("mod1_amt", "15"),
        ("voice_mode", "Mono"), ("env1_release", "80 ms"),
        ("volume", "-5"),
    ]),
    // Snarling, vowel-like: hard sync swept down by the mod envelope into
    // a driven MS low-pass.
    preset("Sync Bass", Bass, 36, &[
        ("osc1_level", "20"), ("osc2_sync", "On"), ("osc2_level", "100"),
        ("env3_attack", "1 ms"), ("env3_decay", "250 ms"), ("env3_sustain", "10"),
        ("mod3_src", "Mod Env"), ("mod3_dest", "Osc 2 Pitch"), ("mod3_amt", "75"),
        ("flt_type", "MS LP"), ("flt_cutoff", "900"), ("flt_res", "30"), ("flt_drive", "6"), ("flt_keytrack", "40"),
        ("voice_mode", "Legato"), ("voice_glide", "30 ms"), ("env1_release", "60 ms"),
        ("volume", "-7"),
    ]),
    // Round, short house bass: sine and triangle with a sub, plus a tiny
    // noise click on the attack.
    preset("Organ Bass", Bass, 36, &[
        ("osc1_wave", "Sine"), ("osc2_wave", "Triangle"), ("osc2_level", "70"), ("osc2_fine", "0"), ("sub_level", "30"),
        ("flt_type", "Off"),
        ("env3_attack", "0.5 ms"), ("env3_decay", "15 ms"), ("env3_sustain", "0"),
        ("mod3_src", "Mod Env"), ("mod3_dest", "Noise Level"), ("mod3_amt", "15"),
        ("env1_attack", "1 ms"), ("env1_decay", "250 ms"), ("env1_sustain", "50"), ("env1_release", "40 ms"),
        ("voice_mode", "Mono"),
        ("volume", "-9"),
    ]),
];

/// When a preset's test note starts (samples): after its `--set` values
/// have finished smoothing, so the level is measured from the note-on and
/// the attack counts.
#[cfg(test)]
const CHECK_NOTE_AT: usize = 10_240;
/// Run length (s): long enough for the slowest pad attack (1.5 s) to peak.
#[cfg(test)]
const CHECK_SECONDS: f32 = 3.0;

impl Category {
    /// The held-peak window (dBFS) every preset in the category must land
    /// in at its test note, velocity 1. Bass sits hotter: it's played as
    /// the lead part.
    #[cfg(test)]
    pub fn level_window(self) -> (f32, f32) {
        match self {
            Category::Bass => (-8.0, -1.0),
            _ => (-12.0, -3.0),
        }
    }

    /// Extra notes each preset is played at to catch silence or clipping
    /// at the edges of its range (checked against the wide -60..0 window).
    #[cfg(test)]
    pub fn range_notes(self) -> &'static [u8] {
        match self {
            Category::Bass => &[28, 48],
            Category::Leads => &[60, 84],
            Category::Keys => &[36, 96],
            _ => &[],
        }
    }
}

#[cfg(test)]
/// Smoke-check lines for every preset (see `smoke-checks.txt`):
/// - its test note must land in the category's level window;
/// - its category's range notes must be audible and not clip;
/// - keys must play an 8-note chord within 40 % of the buffer budget.
pub fn smoke_check_lines(params: &DragonWaaveParams) -> Vec<String> {
    let map = params.param_map();
    let mut lines = Vec::new();
    for p in PRESETS {
        let sets: Vec<String> = p
            .values
            .iter()
            .map(|(id, text)| {
                let ptr = map.iter().find(|(pid, _, _)| pid == id).map(|(_, ptr, _)| *ptr);
                // Safety: the pointer comes from `params`, which outlives this call.
                let name = ptr.map_or_else(|| "?".to_owned(), |ptr| unsafe { ptr.name() }.to_owned());
                format!("--set \"{name}={text}\"")
            })
            .collect();
        let sets = sets.join(" ");
        let common = format!("--hold --note-at {CHECK_NOTE_AT} --seconds {CHECK_SECONDS}");
        let line = |desc: String, opts: String| format!("preset {desc:<26} | {opts} {sets}").trim_end().to_owned();
        let (lo, hi) = p.category.level_window();
        lines.push(line(p.name.to_owned(), format!("--note {} {common} --min-db {lo} --max-db {hi}", p.test_note)));
        for note in p.category.range_notes() {
            lines.push(line(format!("{} @ {note}", p.name), format!("--note {note} {common} --min-db -60 --max-db 0")));
        }
        if p.category == Category::Keys {
            lines.push(line(
                format!("{} chord load", p.name),
                "--note 48 --chord 8 --block 128 --max-load 40 --hold --min-db -60 --max-db 100".to_owned(),
            ));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_value_parses_and_round_trips() {
        let params = DragonWaaveParams::default();
        let map = params.param_map();
        for p in PRESETS {
            let mut seen = BTreeSet::new();
            for (id, text) in p.values {
                assert!(seen.insert(*id), "{}: {id} set twice", p.name);
                let (_, ptr, _) = map.iter().find(|(pid, _, _)| pid == id).unwrap_or_else(|| panic!("{}: no parameter {id}", p.name));
                // Safety: the pointers come from `params`, which outlives them.
                unsafe {
                    let n = ptr.string_to_normalized_value(text).unwrap_or_else(|| panic!("{}: {id} can't parse {text:?}", p.name));
                    // Shown back and parsed again, it must land in the same place.
                    let shown = ptr.normalized_value_to_string(n, true);
                    let again = ptr.string_to_normalized_value(&shown).unwrap_or_else(|| panic!("{}: {id} can't re-parse {shown:?}", p.name));
                    assert!((n - again).abs() < 2e-3, "{}: {id} {text:?} -> {shown:?} -> moved", p.name);
                }
            }
        }
    }

    #[test]
    fn names_are_unique_and_init_comes_first() {
        let names: BTreeSet<_> = PRESETS.iter().map(|p| p.name).collect();
        assert_eq!(names.len(), PRESETS.len());
        assert_eq!(PRESETS[0].name, "Init");
        assert!(PRESETS[0].values.is_empty());
    }

    /// The preset block in `smoke-checks.txt` must match `smoke_check_lines`.
    /// Run with `UPDATE_PRESET_CHECKS=1` to rewrite it.
    #[test]
    fn preset_smoke_checks_are_current() {
        const BEGIN: &str = "# BEGIN PRESETS (generated by presets::tests::preset_smoke_checks_are_current)";
        const END: &str = "# END PRESETS";
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/smoke-checks.txt");
        let file = std::fs::read_to_string(path).unwrap();
        let expected = format!("{BEGIN}\n{}\n{END}", smoke_check_lines(&DragonWaaveParams::default()).join("\n"));
        let current = match (file.find(BEGIN), file.find(END)) {
            (Some(a), Some(b)) => &file[a..b + END.len()],
            _ => "",
        };
        if current == expected {
            return;
        }
        if std::env::var_os("UPDATE_PRESET_CHECKS").is_some() {
            let updated = if current.is_empty() {
                format!("{}\n\n{expected}\n", file.trim_end())
            } else {
                file.replace(current, &expected)
            };
            std::fs::write(path, updated).unwrap();
        } else {
            panic!("smoke-checks.txt's preset block is out of date: run with UPDATE_PRESET_CHECKS=1");
        }
    }
}
