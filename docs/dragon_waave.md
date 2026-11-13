# Dragon Waave

An 80s-style polyphonic synthesizer in the spirit of the Korg Polysix and
Poly-800: subtractive, warm, with a lush ensemble chorus. It goes beyond the
originals wherever more control helps: two oscillators with sync and
cross-mod, three filter models, three envelopes, two LFOs, a mod matrix,
unison and analog drift.

- **Plugin layer:** [`vst/dragon_waave`](../vst/dragon_waave): MIDI in, stereo
  out, parameters, editor.
- **Sound engine:** [`crates/dsp-core/src/synth`](../crates/dsp-core/src/synth):
  host-independent, allocation-free after construction, deterministic, and
  unit-tested.

## Status

| Step | State |
|---|---|
| 1–2 Scaffold, instrument plumbing | Done |
| 7 Smoke test for instruments | Done (see [tools.md](tools.md)) |
| 3 Sound engine | **Done** (phases 1–8) |
| 4 Full parameter set | Done: every engine feature has its parameters |
| 5 Real-time safety | Ongoing: no allocation in `process()` (enforced by `assert_process_allocs`). Dropouts while editing: plan in [dragon_waave_perf.md](dragon_waave_perf.md) |
| 6 Editor | Done: one page styled like Dragon 24EQ, with every control, selector chips, and envelope and LFO displays ([dragon_waave_ui.md](dragon_waave_ui.md)) |
| 8 Docs | This page |

## Signal flow

Per voice, then summed:

```
 OSC 1 ─┐  (sync / cross-mod OSC 1 → OSC 2)
 OSC 2 ─┤
 SUB   ─┼─▶ MIXER ─▶ DRIVE ─▶ HPF ─▶ FILTER ─▶ VCA ─▶ PAN ─┐
 NOISE ─┘                     (LP24 / SVF / MS)            │
                                                           ▼
                      all voices summed ─▶ CHORUS ─▶ OUTPUT (soft clip)

 Per-voice modulators: ENV 1 (amp), ENV 2 (filter), ENV 3 (mod), LFO 1/2, drift
 Global inputs:        pitch bend, mod wheel, aftertouch
```

**Built so far (phases 1–3):**
- Oscillators with sync and cross-mod, sub, noise and ring mod.
- The mixer.
- Drive, the HPF, and all three filter models.
- The three envelopes: ENV 1 drives the amp and ENV 2 the cutoff; ENV 3
  goes wherever the matrix sends it.
- Two LFOs. LFO 1 has fixed routes to pitch, pulse width and cutoff;
  LFO 2 goes wherever the matrix sends it.
- The 6-slot mod matrix.
- Pitch bend, mod wheel and aftertouch (channel and poly).
- Per-voice pan.
- Poly, mono and legato modes, glide, unison (with detune and spread) and
  drift.
- Master volume, the chorus / ensemble, and the output soft clip.

That's the whole signal path, plus an optional 2× oversampled HQ mode
(phase 8).

## Engine modules

| File | Contents | State |
|---|---|---|
| `synth/mod.rs` | `Synth`: note events, `set_patch`, `render`, ended-voice reports | Done |
| `synth/patch.rs` | `Patch`: every sound setting in one `Copy` struct, plus its enums | Phase 1 fields |
| `synth/blep.rs` | PolyBLEP and PolyBLAMP corrections, as (before, after) sample pairs | Done |
| `synth/osc.rs` | Event-based band-limited oscillator; `OscPair` (OSC 1, OSC 2 with sync and cross-mod, phase-locked sub) | Done |
| `synth/noise.rs` | xorshift RNG; white and pink noise | Done |
| `synth/util.rs` | Note-to-frequency conversion and the per-block `Ramp` | Done |
| `synth/voice.rs` | One voice: oscillators, ring mod and noise, then drive, HPF and filter, then the amp and pan; three envelopes, two LFOs, the matrix, glide, unison placement, drift and the steal fade | Phase 6 |
| `synth/alloc.rs` | Voice allocation and stealing (the key stack and mono / legato handling live in `Synth`) | Done |
| `synth/filter.rs` | Drive, one-pole HPF, ladder LP24, SVF (LP/BP/HP/notch), Korg35 "MS" (LP/HP) | Done |
| `synth/env.rs` | DAHDSR envelopes with curves and velocity | Done |
| `synth/lfo.rs` | LFOs (7 shapes, mono / poly / retrigger) with per-note delay and fade-in | Done |
| `synth/matrix.rs` | Mod matrix: 13 sources, 22 destinations, 6 slots with via | Done |
| `synth/chorus.rs` | BBD-style chorus I / II and ensemble; output soft clip | Done |
| `synth/oversample.rs` | Halfband decimator for HQ (2×) mode | Done |

## Engine API

```rust
let mut synth = Synth::new(sample_rate);       // allocates the chorus delay lines; nothing allocates after this
synth.set_patch(&patch);                       // once per block of <= 32 samples
synth.note_on(voice_id, channel, note, velocity);
synth.note_off(voice_id, channel, note);
synth.choke(voice_id, channel, note);
synth.set_pitch_bend(bend);                    // -1..=1; also set_mod_wheel, set_aftertouch, set_poly_pressure
synth.render(&mut left, &mut right);           // overwrites both slices
synth.drain_ended(|end| { /* report VoiceTerminated to the host */ });
synth.reset();                                 // silence; keeps the patch; reuses the buffers (safe on the audio thread)
```

The plugin's `process()`:

1. Splits the buffer into pieces that end at the next note event or after
   32 samples (`RAMP_SAMPLES`), whichever comes first.
2. For each piece:
   1. If note events are due at its start, applies the parameters first,
      without advancing any smoothers (`Read::Current`). A note that starts
      at the same instant as a parameter change, such as automation landing
      on the note, then sees switches and choices at their new values, and
      continuous controls where their smoothing currently is. The block in
      step 4 continues from there, so nothing jumps.
      - The smoke checks caught both halves of this: first Phase = Reset
        being ignored, then a knob jumping to its target and back.
      - Values read once per note (the envelopes' velocity amounts) aren't
        smoothed, so they always apply to the very next note.
   2. Applies the note events due at its start.
   3. Builds a `Patch` from the parameters, advancing nih-plug's smoothers
      by the piece's length (`Read::Advance`). Before processing starts,
      `initialize` uses the targets instead (`Read::Target`), because the
      smoothers aren't primed yet.
   4. Calls `set_patch` and then `render`.
   5. Reports finished voices to the host as `VoiceTerminated` (CLAP needs
      this for polyphonic modulation).

**Smoothing in two layers:**
- nih-plug smoothers handle automation and knob moves (10–50 ms).
- Inside the engine, `set_patch` ramps continuous values (levels, pulse
  widths, volume) linearly over the next 32 samples, so a block-rate update
  never steps.

## Parameters

IDs are saved in host projects and automation. **Never rename or reuse
one.** Groups use a prefix through `#[nested(id_prefix = ...)]`.

### All parameters

| ID | Name | Range | Default |
|---|---|---|---|
| `osc1_wave`, `osc2_wave` | Osc N Wave | Saw, Pulse, Triangle, Sine | Saw |
| `osc1_oct`, `osc2_oct` | Osc N Octave | 16', 8', 4', 2' | 8' |
| `osc1_semi`, `osc2_semi` | Osc N Semitone | −12 … +12 st | 0 |
| `osc1_fine`, `osc2_fine` | Osc N Fine | −100 … +100 ct | 0 / **+7** |
| `osc1_pw`, `osc2_pw` | Osc N Pulse Width | 5 … 95 % | 50 % |
| `osc1_level`, `osc2_level` | Osc N Level | 0 … 100 % | **100 % / 0 %** |
| `osc1_phase`, `osc2_phase` | Osc N Phase | Free, Reset | Free |
| `osc2_sync` | Osc 2 Sync | Off, On | Off |
| `osc2_xmod` | Osc 2 Cross Mod | FM index 0 … 5 (skewed toward low values) | 0 |
| `mix_ring` | Ring Mod Level | 0 … 100 % | 0 % |
| `flt_type` | Filter Type | Off, LP24 Six, SVF LP / BP / HP / Notch, MS LP / HP | LP24 Six |
| `flt_cutoff` | Filter Cutoff | 20 Hz … 20 kHz | 20 kHz |
| `flt_res` | Filter Resonance | 0 … 100 % | 0 % |
| `flt_drive` | Filter Drive | 0 … 24 dB | 0 dB |
| `flt_keytrack` | Filter Key Track | 0 … 200 % (around middle C) | 0 % |
| `flt_vel` | Filter Velocity | −100 … +100 % | 0 % |
| `flt_bass_comp` | Filter Bass Comp | 0 … 100 % (ladder only) | 0 % |
| `flt_env_amt` | Filter Env Amount | −100 … +100 % (±10 octaves) | 0 % |
| `hpf_cutoff` | HPF Cutoff | Off (20 Hz) … 2 kHz | Off |
| `env1_*`, `env2_*` | Amp …, Filter Env … | See below | See below |
| `lfo1_*` | LFO 1 … | See below | See below |
| `lfo1_pitch` | LFO 1 > Pitch | −100 … +100 % (±12 st, both oscillators) | 0 % |
| `lfo1_pwm` | LFO 1 > PWM | −100 … +100 % (±45 % pulse width, both oscillators) | 0 % |
| `lfo1_cutoff` | LFO 1 > Cutoff | −100 … +100 % (±5 octaves) | 0 % |
| `env3_*` | Mod Env … | Same suffixes as the other envelopes | Like Filter Env |
| `lfo2_*` | LFO 2 … | Same suffixes as LFO 1 | Sine, 0.5 Hz, Mono |
| `mod1_*` … `mod6_*` | Mod N Source / Destination / Amount / Via (`src`, `dest`, `amt`, `via`) | See [Modulation](#modulation-done-phases-4-and-5) | Slots 1–2 pre-set, 3–6 empty |
| `voice_bend_range` | Pitch Bend Range | 0 … 24 semitones | 2 |
| `voice_mode` | Voice Mode | Poly, Mono, Legato | Poly |
| `voice_unison` | Unison | 1 … 8 voices per note | 1 |
| `voice_detune` | Unison Detune | 0 … 100 cents (outermost voices) | 15 ct |
| `voice_spread` | Unison Spread | 0 … 100 % (outermost voices' pan) | 50 % |
| `voice_glide` | Glide | 0 (off) … 10 s | 0 |
| `voice_glide_mode` | Glide Mode | Always, Legato | Always |
| `voice_retrig` | Retrigger | From Current, From Zero | From Current |
| `voice_drift` | Drift | 0 … 100 % | 0 % |
| `chorus_mode` | Chorus Mode | Off, I, II, Ensemble | Off |
| `chorus_rate` | Chorus Rate | 10 … 400 % of the mode's rate | 100 % |
| `chorus_depth` | Chorus Depth | 0 … 200 % of the mode's sweep | 100 % |
| `chorus_mix` | Chorus Mix | 0 % dry … 100 % wet (50 % = equal parts) | 50 % |
| `chorus_tone` | Chorus Tone | 1 … 20 kHz (wet-path low-pass) | 8 kHz |
| `out_clip` | Output Clip | Off, On | Off |
| `hq` | HQ (2x Oversampling) | Off, On | Off |

**Envelope parameters.** `env1_` is the amp envelope ("Amp …") and `env2_`
the filter envelope ("Filter Env …"):

| Suffix | Name | Range | Amp default | Filter Env default |
|---|---|---|---|---|
| `delay` | Delay | 0 … 10 s | 0 | 0 |
| `attack` | Attack | 0.5 ms … 20 s | 5 ms | 5 ms |
| `hold` | Hold | 0 … 10 s | 0 | 0 |
| `decay` | Decay | 1 ms … 20 s | 300 ms | 400 ms |
| `sustain` | Sustain | 0 … 100 % | 100 % | 30 % |
| `release` | Release | 1 ms … 20 s | 80 ms | 200 ms |
| `atk_curve` | Attack Curve | 0 % linear … 100 % analog RC | 100 % | 100 % |
| `dec_curve` | Decay Curve (also release) | 0 % linear … 100 % exponential | 100 % | 100 % |
| `vel` | Velocity (to level) | 0 … 100 % | 100 % | 0 % |
| `vel_atk` | Vel > Attack | 0 … 100 % (full: ×1/16 at velocity 1) | 0 % | 0 % |

**LFO 1 parameters** (`lfo1_` + suffix, named "LFO 1 …"):

| Suffix | Name | Range | Default |
|---|---|---|---|
| `shape` | Shape | Sine, Triangle, Saw Up, Saw Down, Square, Sample & Hold, Smooth Random | Triangle |
| `rate` | Rate | 0.01 … 50 Hz | 5 Hz |
| `sync` | Sync | Off, On (rate from the host tempo) | Off |
| `division` | Division | 4 bars … 1/32, with dotted (D) and triplet (T) | 1/4 |
| `phase` | Phase | 0 … 360° | 0° |
| `delay` | Delay | 0 … 10 s | 0 |
| `fade` | Fade In | 0 … 10 s | 0 |
| `mode` | Mode | Mono, Poly, Poly Retrig | Mono |
| `sub_wave` | Sub Wave | Square, Pulse 25% | Square |
| `sub_oct` | Sub Octave | −1 Oct, −2 Oct | −1 Oct |
| `sub_level` | Sub Level | 0 … 100 % | 0 % |
| `noise_color` | Noise Color | White, Pink | White |
| `noise_level` | Noise Level | 0 … 100 % | 0 % |
| `polyphony` | Polyphony | 1 … 16 | 8 |
| `volume` | Volume | −60 … +6 dB | −12 dB |

The default patch is a single sawtooth through the wide-open LP24 ladder.
Osc 2 is ready at +7 cents, so turning up its level gives an immediate
detuned double.

That's 122 parameters in total:

| Group | Count |
|---|---|
| Two oscillators | 14 |
| Sync, cross-mod and ring | 3 |
| Sub and noise | 5 |
| Filter and HPF | 9 |
| Three envelopes | 30 |
| Two LFOs | 16 |
| LFO 1's routes | 3 |
| Six matrix slots | 24 |
| Bend range | 1 |
| Voice settings | 8 |
| Chorus | 5 |
| Clip, HQ, polyphony and volume | 4 |

Every one of them has a control on the editor's single page; a test
checks this ([dragon_waave_ui.md](dragon_waave_ui.md)).

## Components

### Oscillators: done (phases 1 and 1b)

- **Band-limiting by events:**
  - Each oscillator tracks the discontinuities it crosses during a sample:
    saw wraps, pulse edges, triangle corners and sync resets.
  - Each one is corrected on the samples on *both* sides with the 2-point
    PolyBLEP (for steps) or PolyBLAMP (for corners).
  - A sync reset can't be predicted from OSC 2's own phase. So every
    oscillator outputs **one sample late**, while that sample can still be
    corrected.
  - All oscillators share the delay (0.02 ms at 48 kHz), so OSC 1, OSC 2,
    the sub and ring mod stay aligned.
  - Each oscillator is primed at note-on so its first output is the
    waveform's real value. An unprimed 0 was a one-sample glitch that
    showed up in the triangle's alias test.
- **Waveforms:**
  - **Saw and pulse** are PolyBLEP-corrected.
  - **Triangle** is PolyBLAMP-corrected at its two corners (the slope
    changes by 8·dt per sample there).
  - **Sine** is plain `sin`.
- **Aliasing:** tests require at least 12 dB less alias power than the
  naive waveform at 1.2 kHz (10 dB for the triangle, whose naive form
  already aliases little).
- **Pulse DC removal:** the pulse has its DC offset removed, as an analog
  synth's AC coupling would. A narrow pulse therefore swings
  asymmetrically: at 10 % width it peaks at 1.8 instead of 1, which the
  smoke checks verify.
- **Phase modes:**
  - *Free* starts each note at a random phase, like a free-running analog
    oscillator; stacked notes never phase-lock.
  - *Reset* starts at phase 0 for identical, punchy attacks.
- **Pitch:** octave (footage), semitone and fine combine into one offset.
  Frequency is accurate to 0.1 cent (tested from 27.5 Hz to 12 kHz) and
  clamped to 0.45 × sample rate.
- **Sub-oscillator:** a square or 25 % pulse at ÷2 or ÷4. Its phase is
  derived from OSC 1's phase plus a cycle counter (a frequency divider, as
  on the Polysix), so it stays locked to OSC 1 under any modulation.
- **Noise:** xorshift32 white noise, and pink noise from Paul Kellet's
  filter. Every voice has its own fixed seed, so renders are repeatable.
- **Hard sync:** each time OSC 1 starts a cycle, OSC 2 restarts at that
  exact sub-sample instant.
  - The jump in value gets a BLEP, and the jump in slope a BLAMP (the
    slope matters for synced triangles and sines).
  - OSC 2 then sounds at OSC 1's pitch, and its own pitch sets the timbre.
    Sweeping OSC 2's pitch gives the classic sync lead.
- **Cross-mod:** linear through-zero FM of OSC 2 by OSC 1's output.
  - Frequency deviation = index × OSC 1's frequency, so an index sounds the
    same at every pitch, and the index is the textbook FM index.
  - At an index above 1, OSC 2's frequency goes negative for part of each
    cycle. The phase then runs backwards, and discontinuities crossed
    backwards get the opposite correction.
  - Sine into sine gives DX-style bells and electric pianos. Other
    waveforms give harsher, metallic tones.
- **Ring mod:** OSC 1 × OSC 2 as its own mixer level.

### Mixer and drive: done

- **Mixer:** each source has a 0–100 % level. Levels sum without
  normalisation, so all sources at 100 % can reach several times a single
  one. Drive and the filters' saturation tame this.
- **Drive:** pre-filter gain (0 to +24 dB) into `sat(x) = 2·tanh(x/2)`,
  which limits to ±2.
  - At 0 dB the drive stage is an exact bypass.
  - The saturator fades in over the first 6 dB, so turning drive up from
    zero never jumps.

### Filters: done (phase 2)

All filters are zero-delay-feedback (trapezoidal) designs. Cutoffs are
exact (pre-warped), and they stay stable when cutoff and resonance change
every sample. Each voice has its own filter. The chain is drive → HPF →
filter.

| Model | Character | Algorithm | Resonance |
|---|---|---|---|
| **LP24 "Six"** (default) | Polysix / Poly-800 (SSM2044-like); smooth and warm | 4-pole ZDF ladder (Zavalishin); the loop is solved linearly, then saturated with `sat` | Feedback 0 → 4.4 (self-oscillates; 4 is the linear limit) |
| **SVF LP / BP / HP / Notch** | Clean, Oberheim-like; 12 dB/oct | Trapezoidal SVF (Simper), linear; BP normalised to 0 dB at the centre | Q 0.5 → 20, exponential |
| **MS LP / HP** | MS-20 (Korg35); aggressive and screaming | ZDF Korg35 after Pirkle's VA app note, with `sat` in the loop; LP 12 dB/oct, HP 6 dB/oct (as on the MS-20) | Feedback 0.01 → 2.1 (self-oscillates from about 2) |
| **Off** | Bypass | — | — |

- **Saturation:**
  - The ladder and MS filters saturate inside their feedback loops, so
    self-oscillation settles at a steady, bounded level.
  - Even at 0 drive, a full-scale signal into the ladder is softly
    compressed (−0.7 dB at 1.0). This is part of its analog character.
  - The SVF is linear: at maximum Q its resonant peak is +26 dB.
- **Bass compensation (ladder only):** ladder resonance costs bass, a
  passband loss of 1 / (1 + k), as on the real thing. Bass Comp scales the
  input by up to (1 + k) to restore it.
- **Key tracking:**
  - 0–200 % of an octave per octave, centred on middle C (MIDI 60). At any
    amount, middle C plays with the cutoff exactly as set.
  - At 100 %, a cutoff set to C4 follows the keyboard, so every note is
    filtered the same way relative to its pitch.
- **Velocity:**
  - −100 to +100 %. At +100 % the cutoff is as set at full velocity and up
    to 4 octaves lower at velocity 0.
  - Negative values make soft notes brighter.
- **HPF:** a one-pole high-pass (6 dB/oct, Juno-style thinning), 20 Hz to
  2 kHz. It's bypassed at its minimum, where it shows "Off".
- **Smoothing:** cutoff and HPF ramp in octaves (log frequency), so fast
  moves are even in pitch.
- **Switching models** mid-note resets every voice's filter state, rather
  than running a model on another model's leftover state.
- **Envelope amount:** ENV 2 moves the cutoff by up to ±10 octaves (the
  whole range) at ±100 %. Negative amounts sweep downward.
- **LFO amount:** LFO 1 moves the cutoff by up to ±5 octaves (LFO 1 >
  Cutoff).
- **Planned:**
  - A fast `tan` approximation for the per-sample prewarp (phase 8).
    Currently it's `std` `tan`, once per voice per sample.

### Envelopes: done (phase 3)

- **Routing:**
  - ENV 1 is the amp; the voice ends when it finishes.
  - ENV 2 drives the cutoff, scaled by Filter Env Amount.
  - ENV 3 ("Mod Env") has no fixed route; use it through the mod matrix.
- **Stages:** delay, attack, hold, decay, sustain, release. Each timed
  stage advances a 0→1 progress counter at a rate set by its time, and a
  curve maps progress to level. So **stage times are exact whatever the
  curve**, and changing a time mid-stage just changes the rate.
- **Curves:**
  - **Attack curve:** blends linear with an analog RC charge aimed at 1.3
    and stopped at 1.0, as the Polysix's comparator does. It rises fast and
    rounds off near the top; at 100 % it's 67 % up at half time.
  - **Decay curve** (also used for release): blends linear with an
    exponential fall that's normalised to land exactly on its target on
    time. At 100 % it's below 2 % of its range at half time.
- **No jumps:** every stage starts from the current level, so releasing
  mid-attack, or retriggering, never clicks.
- **Velocity:**
  - *Velocity* scales the level, from 0 % (ignored) to 100 % (level =
    velocity). The amp defaults to 100 %, a velocity-sensitive VCA.
  - *Vel > Attack* shortens the attack for harder notes: at 100 %, full
    velocity makes it 16× shorter.
- **Sustain 0:** once the decay finishes the envelope ends, so a pluck
  frees its voice without waiting for note-off. This keeps polyphony
  available for fast plucked parts.
- **Smoothing:** sustain levels ramp per block. Times apply at once,
  because they only set rates.
- **Steal fade:** a stolen voice's 3 ms fade is a separate gain on top of
  the amp envelope.
- **Phase 6:** a retrigger-from-zero mode for mono and legato. In poly mode
  every note starts in a free voice from silence, so it doesn't apply.

### LFOs: done (phase 4)

- **Shapes:**
  - Sine and triangle both start at 0, rising.
  - Saw up, saw down and square.
  - Sample & hold: a new random value each cycle.
  - Smooth random: glides to a new random value each cycle, with cosine
    interpolation, so it never jumps.
- **Rate:** 0.01–50 Hz.
  - **Sync** replaces the rate with a note division (4 bars to 1/32, dotted
    and triplet) at the host's tempo, assuming 4/4. The plugin layer does
    the BPM → Hz conversion, so `dsp-core` only sees Hz. Without a host
    tempo it assumes 120 BPM.
  - The LFO isn't phase-locked to the song position; it free-runs at the
    synced rate.
- **Modes:**
  - **Mono:** one LFO shared by every voice, free-running, so all notes
    move together. This is the default, as on the Polysix.
  - **Poly:** one per voice, each starting at a random phase: a looser,
    ensemble feel.
  - **Poly Retrig:** one per voice, restarting at **Phase** on every note.
    Use it for synced wobbles and predictable sweeps.
- **Delay and fade-in are per note in every mode.** The LFO has no effect
  for *Delay* seconds after note-on, then fades in linearly over *Fade In*.
  So a shared LFO still gives the Polysix's delayed vibrato on each new
  note.
- **Randomness:** each LFO has its own fixed seed, so renders are
  repeatable.
- **LFO 2** has no fixed routes; use it through the mod matrix.

### Modulation: done (phases 4 and 5)

- **Hard-wired routes:** each has its own knob.
  - ENV 2 → cutoff (Filter Env Amount).
  - LFO 1 → pitch (±12 st), pulse width (±45 %) and cutoff (±5 octaves).
    All are bipolar; a negative amount inverts the LFO.
  - Velocity → amp (Amp Velocity).
  - Pitch bend → pitch, over Pitch Bend Range (0–24 st, default ±2).
  - The pitch and PWM routes act on both oscillators and the sub. Pitch is
    applied every sample as a frequency ratio, so sync and cross-mod follow
    it.
- **Mod matrix: 6 slots.** Each slot has:
  - **Source** and **Destination.**
  - **Amount:** bipolar, −100 to +100 %.
  - **Via:** an optional second source that scales the amount, e.g. LFO 1
    → Pitch *via Mod Wheel*, so the wheel sets the vibrato depth.

  A slot contributes amount × source × via × the destination's range.
  Slots aimed at the same destination add up.
- **Default slots:** silent until the controller moves.
  - Mod 1 is LFO 1 → Pitch 2 % via Mod Wheel (up to ±0.5 st of vibrato).
  - Mod 2 is Aftertouch → Cutoff 20 % (up to +2 octaves).
  - Mods 3–6 start empty.

| Source | Range |
|---|---|
| LFO 1, LFO 2 | −1…1, after their delay and fade-in |
| Amp Env, Filter Env, Mod Env | 0…1, velocity-scaled |
| Velocity | 0…1 |
| Key | 0 at middle C, ±1 five octaves either side |
| Mod Wheel (CC 1) | 0…1 |
| Aftertouch | 0…1: the greater of channel pressure and the note's poly pressure |
| Pitch Bend | −1…1 |
| Random | A fixed value per note, −1…1 |
| Drift | The voice's slow random wander, −1…1. It moves at any Drift setting; Drift only sets how much it detunes on its own |
| None | As a source: off. As a via: unscaled |

| Destination | Range at 100 % |
|---|---|
| Pitch (both oscillators), Osc 1 Pitch, Osc 2 Pitch | ±24 semitones |
| Osc 1 PW, Osc 2 PW | ±45 % pulse width |
| Cross Mod | ±5 (FM index) |
| Osc 1 / Osc 2 / Sub / Noise / Ring Level | ±100 % (added to the mixer level, clamped) |
| Cutoff | ±10 octaves |
| Resonance | ±100 % |
| Drive | ±24 dB |
| HPF Cutoff | ±10 octaves (it can switch the HPF on from Off) |
| Amp | gain × (1 + mod), floored at 0 (−100 % silences) |
| Pan | −1 left … +1 right |
| LFO 1 Rate, LFO 2 Rate | ±4 octaves. Per-voice LFOs only: a Mono LFO has one rate for all voices |
| Env Attack | ±4 octaves of time for all three envelopes' attacks (positive = longer) |
| Env Decay | ±4 octaves of time for all three envelopes' decays and releases |

- **Timing:** sources are each voice's values from the previous sample
  (20 µs at 48 kHz). So any destination may modulate its own source, for
  example Mod Env → Env Decay, with no ordering problem.
- **Smoothing:** slot amounts ramp per block, and controllers (bend, wheel,
  pressure) ramp over 32 samples, so 7-bit MIDI steps don't zipper.
- **Pan law:** centre is unity on both sides. Panning fades the far side,
  so a centred voice is never quieter than before pan existed.
- **Cost:** with every slot off the matrix is skipped, and resonance,
  drive and HPF are only recomputed per voice when the matrix moves them.

### Voices: done (phases 1 and 6)

- **Pool:** 16 voices plus 16 spare slots for stolen voices to fade out
  in.
- **Stealing:** at the polyphony limit, the oldest *released* voice is
  stolen first, then the oldest held one.
  - The victim fades out over 3 ms in a spare slot while the new note
    starts at once. Stealing neither clicks nor delays the new note.
  - Only if every slot is busy is the quietest fading voice cut outright.
  - Lowering polyphony fades the extra voices out immediately.
- **Voice modes:**
  - **Poly.**
  - **Mono:** one note at a time with last-note priority (a 16-key stack).
    Every new note retriggers the envelopes (and per-voice LFOs). Releasing
    the newest key falls back to the newest one still held, and retriggers.
  - **Legato:** like Mono, but overlapping notes only change pitch; the
    envelopes carry on. It retriggers only when you play after letting
    go.
  - **Retrigger:** *From Current* restarts envelopes from their present
    level, the analog way, with no click. *From Zero* restarts from
    silence for a hard, percussive attack.
  - In mono and legato a voice is reused for the new note, so the old note
    is reported to the host as ended and the voice takes the new note's ID.
  - Switching voice mode releases what's playing and starts clean.
- **Glide:**
  - 0–10 s, exponential (analog portamento): about 99 % of the way there
    after the set time.
  - *Always:* every note glides from the previous one.
  - *Legato:* only notes played while another key is held.
  - It works in poly mode too: each new note glides from where the newest
    voice is sounding, even mid-glide.
- **Unison:**
  - Each note uses 1–8 voices, spread evenly from −1 to +1 across the
    stack.
  - **Detune** sets the outermost voices' offset (0–100 cents), with the
    rest in between. **Spread** sets their pan.
  - Each voice is scaled by 1/√N, so stacking adds thickness without a big
    jump in level: N in-phase voices peak at √N.
  - **Polyphony counts voices**, as on hardware unison: 8 voices at
    Unison 4 plays 2 notes. A note never uses more voices than the
    polyphony.
- **Drift ("vintage"):**
  - Each voice slot has fixed "component tolerances": pitch ±3 cents,
    cutoff ±0.15 octave, envelope times ±10 %.
  - On top of that, each voice has its own slow random wander (around
    0.3 Hz): pitch ±5 cents, cutoff ±0.1 octave.
  - All of this is scaled by Drift, so 0 % is exact and 100 % is a
    well-worn poly. The wander is also a matrix source.

### Chorus / ensemble: done (phase 7)

- **Order:** voices are summed, then Volume, then the chorus, then the
  clip. The chorus sees the real output level, so its wet saturation
  behaves the same whatever the voice count.
- **Design:**
  - Bucket-brigade (BBD) style: short modulated delays, read with 4-point
    Hermite interpolation, which is accurate to 1e-4 at fractional delays.
  - The wet path gets a 2-pole **tone** low-pass (BBDs are band-limited)
    and the gentle `sat` saturation.
- **Modes:**

| Mode | Character | How |
|---|---|---|
| **I** | Juno-style: wide and gentle | Triangle LFO at 0.5 Hz sweeps two 5 ms delays ±1.5 ms, left and right in opposite directions |
| **II** | Juno-style: faster and deeper | The same at 0.85 Hz, ±2 ms |
| **Ensemble** | Polysix / string-machine shimmer | The mono sum feeds one 7 ms line with three taps. Each is swept by a slow sine (0.6 Hz, ±1.5 ms) plus a fast one (5.5 Hz, ±0.25 ms) at 0°, 120° and 240°. Left = (2·tap 1 + tap 2) / 3, right = (2·tap 3 + tap 2) / 3 |

- **Rate and Depth** scale the mode's own tuning: 200 % depth doubles the
  sweep. Every mode stays inside its 20 ms buffer at any setting (tested).
- **Mix** crossfades dry to wet: 50 % is equal parts, as on the
  hardware, and 100 % is wet only (a vibrato).
- **Exact bypass:** Off, or a mix of 0, leaves the signal bit-identical.
  The delay lines keep running underneath, so switching the chorus on
  mid-note is seamless.
- **Memory:** the delay lines are the engine's only heap memory. They're
  allocated in `Synth::new`, and `reset()` clears and reuses them, since a
  host may call it from the audio thread.

### Output: done (phase 7)

- **Master volume.**
- **Output Clip:** an optional soft clipper.
  - Exactly linear up to 0.8.
  - Above that it bends smoothly, with its slope never above 1, toward
    ±1. It never goes past full scale.
  - Off by default, so normal levels are untouched.

### Performance and robustness: done (phase 8)

- **Benchmark:** `cargo test -p dsp-core --release synth::tests::bench --
  --ignored --nocapture`. It plays 16 voices with everything on (both
  oscillators, sub, noise, ring, sync and FM, the driven resonant ladder,
  three envelopes, per-voice LFOs, a full matrix, drift and the ensemble).
  Results on the development Mac (Apple Silicon, one core):

  | Mode | CPU | Speed |
  |---|---|---|
  | Normal | about 6.5 % | 15× realtime |
  | HQ | about 13 % | 8× realtime |

- **Editing benchmark:** `cargo test -p dsp-core --release
  synth::tests::bench_editing -- --ignored --nocapture` changes the patch
  every block (block sizes 1–128, with and without note churn) and reports
  mean and worst block time. See [dragon_waave_perf.md](dragon_waave_perf.md).
- **Automation:** `SAMPLE_ACCURATE_AUTOMATION` is off. The engine already
  ramps continuous settings over 32 samples, and splitting buffers at every
  parameter change only added overhead. Note events stay sample-accurate.
  Unchanged patches aren't re-applied.
- **Fast `tan`:** the filters' per-sample prewarp uses a [5/4] Padé
  approximant at x/2 plus the double-angle formula. It's within 2e-6 of
  `std` across the whole cutoff range (tested). It made no measurable
  difference to the benchmark, because `tan` wasn't the bottleneck, but it
  costs nothing in accuracy and is kept.
- **Denormals:** nih-plug's process wrapper turns on flush-to-zero (on x86
  and ARM) around every `process()` and `reset()`. The engine needs no
  extra guards. (Unit tests run without it, which is fine because they
  only check values.)
- **HQ (2× oversampling):**
  - Voices (oscillators, drive, filters, envelopes and LFOs) run at twice
    the sample rate. Every rate is derived from the voice rate, so pitch
    and timing are unchanged (tested).
  - A 63-tap halfband FIR (Kaiser β = 8) filters and decimates the voice
    sum before Volume and the chorus. It's flat to 20 kHz (within
    0.01 dB) and over 80 dB down from 28 kHz, so nothing folds back
    below 20 kHz.
  - It adds 15.5 samples (0.3 ms) of delay, which isn't reported to the
    host; that's negligible for an instrument.
  - Test: C8 through full drive. The 7th and 9th harmonics' aliases (at
    18.7 and 10.3 kHz) drop by more than 30 dB, and the fundamental is
    unchanged within 0.05 dB.
  - Switching HQ mid-note retunes the voices and clears the decimator.

## Presets

35 factory presets, chosen from **PRESET** in the editor header: a dropdown
grouped by category, with ◀ ▶ to step through. Source:
[`vst/dragon_waave/src/presets.rs`](../vst/dragon_waave/src/presets.rs).

| Category | Presets |
|---|---|
| Init | Init |
| Brass | Six Brass (Polysix), Prophet Brass, OB Stab, Vangelis Swell (CS-80), Synth Horns, Brass Section (unison) |
| Pads | Ensemble Pad, Juno Pad (PWM), Glass Pad, Dark Drift, Sync Sweep Pad, Vox Pad, Warm Pad |
| Strings | Poly-800 Strings, Solina Strings |
| Keys | FM E.Piano (DX7-style), FM Tine EP, Wurli Bark, Poly Clav, 80s Organ, Hollow Keys |
| Bells | FM Tubular Bell, Glass Bell, Ring Bell, Music Box, Celesta, Marimba, Vibes |
| Leads | MS Sync Lead, Six Lead, Square Lead |
| Bass | Sub Bass, Six Bass, MS Acid Bass |

- **How a preset is stored:** a list of `(parameter ID, value text)` pairs
  on top of the init patch.
  - The text is parsed by each parameter's own parser, for example
    `("flt_cutoff", "650")`, `("env1_attack", "25 ms")` or
    `("flt_type", "LP24 Six")`.
  - Loading a preset sets every parameter: its own value if listed,
    otherwise the default. So a preset fully replaces the sound. Each change
    is a normal automation gesture, so hosts record it.
  - The choice saves with the project as ordinary parameter values. The
    picker's label, though, isn't saved.
- **Performance controls:** every preset keeps the default matrix slots
  (the mod wheel adds vibrato, aftertouch opens the filter) unless it uses
  those slots itself. Presets put their own routings in slots 3–6.
- **How the families are built:**
  - **FM keys and bells:** OSC 1 is an unheard sine modulator and OSC 2 the
    carrier, through Cross Mod. The Mod Env, routed to Cross Mod and
    velocity-scaled, makes the attack brighter.
  - **Bell ratios:** Tubular Bell uses 3.5:1 (4' + 9.69 st), Glass Bell
    √2:1 (+6 st), and Celesta and Marimba 2:1.
  - **Ring Bell** ring-modulates two sines 2.38× apart.
- **Tests:**
  - `presets.rs` checks every ID exists, every value parses and survives
    being shown and parsed again, no preset sets a parameter twice, names
    are unique, and Init is first and empty.
  - Each preset also has a generated smoke check in `smoke-checks.txt`
    (between `# BEGIN PRESETS` and `# END PRESETS`). It plays the preset's
    test note through the real plugin and requires it to be audible
    (> −60 dBFS) and not clip (≤ 0 dBFS).
  - A test fails if that block falls out of date. Run
    `UPDATE_PRESET_CHECKS=1 cargo test -p dragon_waave preset_smoke` to
    regenerate it.
- **Adding a preset:** add a `preset(...)` entry to `PRESETS`, regenerate
  the smoke checks, then run `./build2lynx.sh dragon_waave`.

## Build order

Each phase is wired into the plugin as it lands, so it can be heard in LYNX.

| # | Phase | State |
|---|---|---|
| 1 | `Patch`, BLEP oscillators, sub, noise, mixer, `Synth`/`Voice`, poly allocation with fade-on-steal | **Done** |
| 1b | OSC 2 hard sync, cross-mod, ring mod | **Done** |
| 2 | Ladder LP24 with drive, then SVF and MS, then HPF | **Done** |
| 3 | ENV 1–3 with curves and velocity (replaces the fixed gate) | **Done** |
| 4 | LFOs and hard-wired modulation | **Done** |
| 5 | Mod matrix; pitch bend, mod wheel, aftertouch | **Done** |
| 6 | Mono, legato, glide, unison, drift | **Done** |
| 7 | Chorus / ensemble, output soft clip | **Done** |
| 8 | Hardening: fast math, denormals, performance pass; optional 2× oversampling "HQ" switch | **Done** |

**Later (plugin layer, not engine):** arpeggiator and chord memory, presets
(Polysix brass, Poly-800 strings, MS sync lead, FM-ish bell, sub bass,
ensemble pad), editor.

## Testing

**Engine unit tests** (`cargo test -p dsp-core synth`):

| Area | Tests |
|---|---|
| BLEP | Averaged over every sub-sample position, corrections integrate to the continuous residual (0 for BLEP, 1/12 for BLAMP); a step on a sample lands at the midpoint |
| Oscillators | Pitch within 0.1 cent; aliasing well below naive; no pulse DC at any width; bounded up to and past Nyquist; sub locked at ÷2 / ÷4; synced OSC 2 repeats at OSC 1's period; synced waves alias far less than naive sync; cross-mod sidebands match the Bessel functions J0 and J1 at index 1 and 2; through-zero FM bounded and periodic, with and without sync; all outputs share the one-sample delay |
| Noise | White is uniform and uncorrelated; pink is darker and bounded; same seed gives the same output |
| Filters | Ladder at zero resonance matches the exact bilinear 4-pole response within 0.05 dB (125 Hz–4 kHz); resonance costs exactly 1/(1+k) of bass, and Bass Comp restores it; ladder and MS self-oscillate at the cutoff (within 2 % / 5 %) at a steady level, while the linear SVF rings without growing; SVF LP/HP −6.02 dB, BP 0 dB and notch below −60 dB at the cutoff; MS LP 12 dB/oct and HP 6 dB/oct; HPF −3.01 dB at its cutoff; drive is an exact bypass at 0 and bounded at maximum; every model stays finite with the cutoff jumping randomly every sample at full resonance |
| Envelopes | Delay, attack, hold, decay and release each last their set time within one sample, at linear, mid and full curves; curves change the middle but not the end points (RC attack midpoint T(1 − √((T−1)/T))); release mid-attack and retrigger mid-release continue from the current level; velocity scales level and attack time (2^1.5 ratio at the tested settings) and does nothing at 0 %; sustain 0 ends after the decay; sustain follows changes while held |
| LFOs | Rate within one cycle over 20 s, from 0.1 to 50 Hz; every shape's value at known phases; all shapes stay in −1…1; sample & hold steps exactly once per cycle; smooth random never steps faster than its cosine allows; same seed, same randomness; delay then a linear fade-in (half depth halfway); note-on start depends on mode (mono ignores notes, retrigger goes to the start phase, poly randomises) |
| Matrix | Amount × source × via × range; slots on the same destination add; inactive slots (no source, no destination or 0 amount) do nothing; a via at 0 mutes its slot |
| Voices | Unison 3 sits at −20, 0 and +20 cents and pans hard left, centre and hard right; one note-off releases the stack; unison 1 and 4 peak at 1 and 2 (1/√N); polyphony counts unison voices; mono last-note priority (releasing a non-sounding key does nothing, releasing the sounding key falls back); mono retriggers and legato doesn't (but legato does after letting go); From Zero restarts the amp from silence, From Current doesn't; glide is part-way at first, then lands; legato glide only glides overlapping notes; poly glide; mono reports the replaced note as ended; drift is exact at 0 % and gives each voice a different offset within ±3 cents at 100 %; Drift as a matrix source; switching voice mode lets go cleanly |
| Chorus | Hermite reads within 1e-4 of the true signal at fractional delays; Off and a mix of 0 are bit-exact bypasses; every mode stays inside its buffer at maximum depth; fully wet, every mode wobbles the pitch above and below 440 Hz and makes left and right differ, within ±1; sweeps at 4× rate and 2× depth never step faster than the signal allows; Tone at 1 kHz takes an 8 kHz wet sine down by more than 30 dB; clearing reuses the buffers; the soft clip is exact below 0.8, monotonic, slope ≤ 1, never past 1 |
| HQ / fast math | Fast `tan` within 2e-6 of `std` from 0 to 0.45·fs; the halfband is flat (±0.05 dB) to 19 kHz and below −75 dB from 28 kHz; HQ removes drive aliases (>30 dB) with the fundamental unchanged; HQ keeps pitch and envelope timing |
| Synth | Note on → release → end reported once; the chorus widens a centred note in every mode; Output Clip brings a 4.0 peak down to full scale; reset reuses the chorus buffers and is silent; the default slots stay silent until the wheel or pressure moves, then the wheel brings in vibrato; via scales by the wheel (0 → 440, 0.5 → 440·√2, 1 → 880 Hz); pitch bend follows the bend range (±2 st, then −12 st → 220 Hz); Osc 2 Pitch doesn't move OSC 1; Key → Cutoff 50 % equals 100 % key tracking; Mod Env and LFO 2 route to the cutoff; channel and poly aftertouch both open the filter; Velocity → Pan sends a voice hard right at unity; Mod Wheel → Amp −100 % silences; Random gives each note a different pitch within its range; Velocity → Env Attack +25 % doubles the attack; Velocity → LFO 1 Rate +25 % doubles a per-voice LFO's rate; LFO > pitch bends 440 Hz to exactly 880 and 220 (square, ±12 st); LFO > cutoff moves the filter 2 octaves onto the note (−12.04 dB); LFO > PWM reaches 95 % (peak 1.9); LFO delay keeps the pitch at exactly 440 Hz until it ends, then the LFO is fully faded in; mono LFOs are shared and poly LFOs per note; ENV 2 sweeps the cutoff by its amount, both up and down (−12.04 dB at the note); sustain 0 frees the voice and reports it without a note-off; amp delay holds the note back exactly; amp velocity amount; ladder at the played pitch is −12.04 dB; 100 % key tracking keeps every note at −12 dB; velocity tracking offsets; HPF bypass is bit-exact at its minimum; every model finite with everything maxed; cutoff ramps in octaves; ring mod equals OSC 1 × OSC 2; sync makes OSC 2 follow OSC 1's pitch; pitch follows octave/semitone/fine; held notes retune; voice-ID note-off; steal order; fade-on-steal is click-free and reported; lowering polyphony; 100 rapid steals never lose a note; choke; phase modes; mixer ramps; extreme settings stay finite; deterministic renders; sanitising; reset |

**Plugin test:** parameter defaults produce exactly the engine's default
patch.

**Smoke checks** ([`smoke-checks.txt`](../vst/dragon_waave/smoke-checks.txt),
run by `build2lynx.sh`): the expected peak of each source and waveform is
derived from the waveform maths. The checks cover volume, velocity, square,
10 % pulse, triangle, OSC 2 alone, sub alone, white and pink noise,
extreme notes and octaves, polyphony 1, ring mod (identical sines give
sin², peak 1), a synced saw, maximum cross-mod (a phase-modulated sine
still peaks at 1), and everything on at once without clipping. These
checks set Filter Type to Off, so they test the oscillators alone.
- **Filter checks:** each puts a sine at A4 with the cutoff on 440 Hz,
  where each model's gain is exact: ladder −12.04 dB (also with key
  tracking from C4), SVF LP/HP −6.02, BP 0 and notch silent, MS LP −6.02,
  MS HP −3.01, HPF −3.01. Every measured level matched within 0.05 dB.
  Further checks cover maximum drive (bounded at 2 × Volume) and full
  resonance on the ladder and MS, which stay bounded.
- **Envelope checks:**
  - Amp sustain 50 % (−6.02 dB).
  - Amp velocity 0 % (velocity ignored).
  - An amp delay longer than the note (exact silence).
  - A zero-sustain pluck that ends by itself.
  - Attack timing: a 10 s linear attack must reach 5.33 % at the 0.533 s
    note-off (predicted −37.46 dBFS, measured −37.48).
  - A filter envelope that sweeps the cutoff 2 octaves up to the note
    (−64.04 dBFS).
- **LFO checks:** a square LFO 1 holds +1 for the whole held window, and
  LFO > Cutoff lifts the cutoff 2 octaves onto the note (−64.04 dBFS). The
  same result must come from:
  - Poly Retrig mode.
  - Mono mode.
  - Tempo sync at 1/1 at 120 BPM, with the free rate set to 40 Hz so a
    sync failure would show.

  Further checks:
  - An LFO delay longer than the note leaves the cutoff 2 octaves below:
    predicted −101.23 dBFS, measured −101.23.
  - LFO > PWM at 100 % reaches 95 % width (peak 1.9, −6.42 dBFS).
- **Matrix checks:** using the empty slots 3–6, each of these sources must
  move the cutoff 2 octaves onto the note (−64.04 dBFS):
  - Velocity.
  - Mod Env.
  - LFO 2.

  Further checks:
  - A via: Velocity → Cutoff 80 % via Velocity, at velocity 0.5, gives
    exactly 2 octaves. Predicted −70.06 dBFS, including the halved amp;
    measured −70.06.
  - Velocity → Amp −50 % gives −6.02 dB.
  - The smoke host can't send bend, CC or pressure, so the engine tests
    cover those.
- **Voice checks:**
  - Unison 4 in phase, ×2. The host sets Unison Detune at the same instant
    as the note, so detune glides from 15 ct to 0 over its 10 ms smoothing.
    That leaves the stack a few degrees apart: −6.01 dBFS instead of
    −5.98.
  - Unison 2 spread hard left and right gives 1/√2 per side (−15.01 dBFS).
  - Mono mode, legato mode with glide, and full drift each keep the level.
- **Chorus and output checks:**
  - Chorus mix 0 % is an exact bypass (−12.00 dBFS).
  - Chorus II fully wet keeps the level: predicted −12.05 dBFS from the wet
    saturation, measured −12.05.
  - Ensemble fully wet stays bounded.
  - A hot patch (4 in-phase unison voices at Volume +6 dB) reads +11.99
    dBFS unclipped and exactly 0.00 dBFS with Output Clip on.
- **HQ checks:** HQ keeps an A4 sine's level exactly, and HQ with
  everything driven (sync, FM, resonance 90 %, full drive, unison 4,
  ensemble) stays bounded. Every check
also verifies the note decays to silence after note-off.

**Planned with later phases:**
- Envelope timing within 5 %.
- Legato note stack.
- Chorus at mix 0 is bit-identical to dry.
- A golden-render checksum.
