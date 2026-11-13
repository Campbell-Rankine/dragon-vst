# Dragon Waave presets: expansion plan

**Status: done (2026-11-14).** All 24 are written, checked and installed;
the basses have been approved in LYNX. The leads and keys are still to be
played through the [listening checklist](#listening-checklist-per-preset-in-lynx).

Add 24 factory presets: 8 keys, 8 leads and 8 basses. That takes the
library from 35 to 59 and fills its thinnest categories. How presets work
today (format, tests, smoke checks) is in
[dragon_waave.md](dragon_waave.md#presets).

| Category | Now | Adding | After |
|---|---|---|---|
| Keys | 6 | 8 | 14 |
| Leads | 3 | 8 | 11 |
| Bass | 3 | 8 | 11 |
| Everything else | 23 | — | 23 |

**Done when:**
- All 24 are in `PRESETS`, and the existing preset tests and generated
  smoke checks pass.
- Every preset passes the new level and range checks (step 1).
- Each one has been played in LYNX through the listening checklist below.
- [dragon_waave.md](dragon_waave.md#presets) lists them.

## Ground rules

These apply to every new preset, so the library feels consistent.

- **Level:** the held peak at the test note, velocity 1, lands in a
  window checked in step 1, so switching presets never jumps unexpectedly
  in volume.
  - **Bass: −8 to −1 dBFS.** Basses sit about 4 dB hotter than the rest,
    because bass is played as the lead part here.
  - **Everything else: −12 to −3 dBFS.**
- **Performance controls:** keep the default matrix slots (slot 1:
  mod wheel → vibrato; slot 2: aftertouch → cutoff) unless the preset gives
  the wheel a better job. If it does, the comment above the preset says
  what the wheel and aftertouch do. Presets use slots 3–6 for their own
  routings, as now.
- **Keys:** poly. Amp velocity 60–90 % and filter velocity on, so playing
  dynamics matter. No glide. Release 150–500 ms.
- **Leads:**
  - Mono or Legato with glide, usually 30–80 ms, so the lines sing.
  - Vibrato either delayed (LFO 1 delay and fade) or on the wheel, never
    always on at full depth.
  - Bend range 2 semitones, except one preset at 12 for dives.
- **Bass:**
  - Mono or Legato, with retrigger From Zero where the attack must punch.
  - Chorus off, or under 20 % mix: chorus smears the low end.
  - Key track the filter (30–60 %) so high notes don't go dull.
  - Must stay clean down to E1 (MIDI 28).
- **CPU:** unison and HQ multiply the voice count. Use unison only where
  it's the sound (Stack Lead, Reese Bass), and never use HQ in a factory
  preset. Check keys presets with the load test in step 1.
- **Names:** descriptive, not hardware brand names (for example
  "Tine Grand", not a product name). Comments may say "X-style" to
  explain the idea, as the existing ones do.
- **Values are starting points.** The recipes below give each sound's
  shape. Final numbers are set by ear in LYNX. The parse test catches any
  text a parameter rejects.

## Sustain pedal: deferred

The synth ignores the sustain pedal (CC 64). Adding it is planned
separately in [dragon_waave_sustain.md](dragon_waave_sustain.md) and is
deferred, so the presets don't wait for it. The keys presets are designed
to work without a pedal: decays are long enough to play legato by hand.

## Step 1: tighter checks

**Done (2026-11-14).** What changed from the plan below:
- **Measuring from note-on:** `clap-smoke` got `--note-at` and
  `--seconds`. Checks start the note at 10240 samples (after `--set`
  values finish smoothing) and run for 3 s. The old window started 0.21 s
  into the note, missing plucks' attacks, and ended at 1.07 s, before slow
  pads (up to 1.5 s attack) peak.
- **Both channels:** the window applies to each channel. Chorus spreads
  them 1–2 dB apart, so the quieter channel sets the level.
- **Retuned:** 18 presets got a new `volume`.
  - Basses: Six Bass −12 → −4, MS Acid Bass −12 → −9, Sub Bass −8 → −6.
    All three now peak around −4.4 to −5 dBFS.
  - Lifted 1–5 dB to clear −12 dBFS with at least 0.4 dB margin: Six
    Brass, Vangelis Swell, Brass Section, Ensemble Pad, Warm Pad, Sync
    Sweep Pad, Vox Pad, Poly-800 Strings, Solina Strings, FM Tine EP,
    Hollow Keys, Glass Bell, Music Box, Six Lead, Square Lead.
  - Init is untouched (−11.84 dBFS, the default patch). The engine is
    deterministic, so near-edge results don't flicker.
- **Checks:** 65 generated lines: 35 levels, 24 range notes, 6 keys
  chord-load checks (measured at 3–5 % p99 against the 40 % limit).

The generated smoke checks only require each preset to be audible
(> −60 dBFS) and not to clip (≤ 0 dBFS). Tighten them before adding 24
presets, so level mismatches show up as test failures and not by ear.

1. **Level window:**
   - The generator emits `--min-db -8 --max-db -1` for Bass and
     `--min-db -12 --max-db -3` for everything else, instead of −60 / 0.
   - First measure the existing 35 presets against it. Retune any that
     fall outside (by adjusting `volume`), or give that preset an
     explicit, commented exception. The three existing basses will most
     likely need raising to the new bass window.
2. **Range notes:** each category also checks its extremes.
   - Bass at 28 (E1) and 48.
   - Leads at 60 and 84.
   - Keys at 36 and 96.
   - These use the wide −60 / 0 window: they catch silence and clipping at
     the edges, not level.
   - Add the notes as a per-category list in the generator, so there's no
     new field on `Preset`.
3. **Load check (keys only):** play an 8-note chord with `--block 128
   --max-load 40`. That catches a keys preset that's too expensive for
   chords. It uses the `clap-smoke` options added for the performance
   work.

## Step 2: bass (8)

**Written (2026-11-14); listening in LYNX next.** All 8 follow the
recipes below with these specifics:
- **Wheel:** where the wheel has a special job (Reese, Drop, Wobble), that
  routing takes slot 1 in place of the default vibrato.
- **Wobble Bass:** its LFO is Poly Retrig, so each note's wobble starts in
  the same place.
- **Levels:** volumes set so each lands at −4.3 to −5.1 dBFS, matching the
  three existing basses after step 1. All are clean at E1 (28) and 48.

Test note 36.

| Name | Idea | Recipe | Wheel / aftertouch |
|---|---|---|---|
| **Reese Bass** | Detuned, phasing DnB bass | Two saws, OSC 2 fine +12 cents. Unison 2, detune 15, spread 30. LP24 at ~600 Hz, res 10, drive 6. LFO 2 (sine, 0.3 Hz, mono) → cutoff, small. Drift 40. Mono. | Wheel opens the cutoff (slot 3, LFO 2 via wheel → cutoff) |
| **FM Bass** | Punchy DX-style slap bass | OSC 1 sine modulator at 1:1, OSC 2 sine carrier. Mod env (0.5 ms attack, 150 ms decay, 0 sustain, velocity 80) → Cross Mod. Filter off. Amp 1 ms / 600 ms decay / sustain 40. Retrigger From Zero. | Default |
| **Pluck Bass** | Funky, tight, velocity-bright | Pulse 35 % + sub 40. SVF LP at ~350 Hz, res 35. Filter env 0.5 ms / 120 ms / 0 sustain, amount 45; filter velocity 60. Key track 50. Amp sustain 0, decay 500 ms. Mono. | Default |
| **Pedal Bass** | Huge, slow, 70s pedal-synth low end | Two saws, OSC 2 at 16' (an octave down), fine −5 cents. Ladder at ~250 Hz, res 25, drive 9, bass comp 60. Amp attack 5 ms, sustain 90, release 300 ms. Legato, glide 80 ms. | Default |
| **Drop Bass** | Sine with a pitch "thump" | Sine + sub 30. Mod env (0.5 ms / 60 ms / 0) → pitch, +24 semitones' worth. Filter off. Amp sustain 80. Mono, From Zero. | Wheel deepens the drop (slot 4, mod env → pitch via wheel) |
| **Wobble Bass** | Tempo-synced filter wobble | Saw + square (OSC 2 pulse 50 %). LP24, res 45, drive 10. LFO 2 synced 1/8, sine → cutoff, 1.5 octaves, **always on**. Mono. | Wheel adds up to 1.5 more octaves of wobble (slot 1 becomes LFO 2 → cutoff via wheel, replacing vibrato) |
| **Sync Bass** | Snarling, vowel-like | OSC 2 synced, level 100; OSC 1 level 20. Mod env (1 ms / 250 ms / 10 %) → OSC 2 pitch, +18 semitones. MS LP at ~900 Hz, res 30, drive 6. Legato, glide 30 ms. | Default |
| **Organ Bass** | Round, short house bass | Sine + triangle at 8' and sub 30. Filter off. Amp 1 ms / 250 ms decay / sustain 50, release 40 ms. For the attack click: white noise at 5 %, gated by a short mod env (0.5 ms / 15 ms / 0) → noise level. Mono. | Default |

## Step 3: leads (8)

**Written (2026-11-14).** Basses approved in LYNX the same day. All 8
leads follow the recipes below, with these changes:
- **Ring Lead:** band-pass at 700 Hz, not 1.5 kHz. With 100 % key
  tracking, 1.5 kHz put the band at ~2.5 kHz at A4, far above the ring
  products (~220 Hz and ~1.1 kHz), and the preset came out 20 dB quiet.
- **Sync Scream:** mod env → OSC 2 pitch at 100 % (+24 semitones, the
  matrix maximum), not +30.
- **Chip Lead:** a bipolar square LFO alone would trill ±12 semitones (two
  octaves apart). LFO 1 → Pitch ±6 semitones via the wheel (slot 1) plus
  wheel → Pitch +6 semitones (slot 3) gives the note and its octave at
  full wheel.
- **Levels:** −9.1 to −9.9 dBFS at A4, matching the existing leads, and
  clean at 60 and 84.

Test note 69.

| Name | Idea | Recipe | Wheel / aftertouch |
|---|---|---|---|
| **Stack Lead** | Wide unison saw lead | Saws on both oscillators, OSC 2 +7 cents. Unison 4, detune 20, spread 60. LP24 at ~4 kHz, res 15. Chorus off. Legato, glide 40 ms. Delayed vibrato. | Default |
| **Whistle Lead** | Pure, glidey sine | Sine; OSC 2 sine at 4', level 15. Filter off. Glide 80 ms, Always. LFO 1 sine 5.5 Hz → pitch, delay 300 ms, fade 500 ms. Amp attack 10 ms. | Aftertouch → vibrato depth (slot 2 becomes LFO 1 → pitch, via aftertouch) |
| **Sync Scream** | Aggressive, swept sync lead | OSC 2 synced. Mod env 2 ms / 600 ms / 40 % → OSC 2 pitch, +30 semitones. LFO 2 (triangle 0.5 Hz) → OSC 2 pitch, small. MS LP at 5 kHz, res 50, drive 12. HPF at 150 Hz. Legato, glide 50 ms. | Wheel adds sync sweep (slot 5, LFO 2 via wheel → OSC 2 pitch) |
| **PWM Lead** | Hollow, moving square lead | Pulse, LFO 1 (triangle 4 Hz) → PWM 30. Sub 20. LP24 at 3 kHz, res 20. Chorus I, 30 %. Mono, glide 30 ms. | Default |
| **Ring Lead** | Metallic, vocal-ish | Two sines, OSC 2 at +7 semitones + 3 cents. Ring 80, OSC levels 20. SVF BP at 1.5 kHz, res 40, key track 100. Legato, glide 40 ms. | Aftertouch → cutoff (default) |
| **Acid Lead** | Squelchy, resonant line | Saw. MS LP at 600 Hz, res 75, drive 10, env amount 55. Filter env 1 ms / 200 ms / 0, velocity 50. Legato, glide 60 ms. | Default |
| **Bwaa Lead** | Brassy swell: the filter opens late | Saw + saw at +5 cents. LP24 at 500 Hz, res 20, env amount 50. Filter env attack 150 ms, decay 400 ms, sustain 50 %. Amp attack 20 ms. Mono, glide 50 ms. Bend range 12. | Wheel → vibrato (default) |
| **Chip Lead** | Bright 8-bit-ish | Pulse 12.5 %. Filter off. LFO 1 square 12 Hz → pitch 12 semitones, via mod wheel (arpeggio-like octave trill). No glide, Mono, amp sustain 100, release 30 ms. | **Wheel adds the octave trill.** It stays off at rest: the narrow pulse is the sound, and an always-on trill would make melodies unplayable |

## Step 4: keys (8)

**Written (2026-11-14).** All 8 follow the recipes below, with these
changes:
- **Lo-Fi Keys and House Stab:** 50 % filter key tracking. Without it,
  high notes fell above their fixed cutoffs (−30 and −21 dBFS at C7).
- **Lo-Fi Keys:** the wow is 1 % (about 0.24 semitones), and the wheel
  adds 2 %. Matrix amounts display as whole percentages, so 0.6 % wouldn't
  survive being shown and parsed again.
- **Tine Grand:** the modulator is an octave up (OSC 1 at 4'), which is
  the 2:1 ratio.
- **Levels:** −8.7 to −9.4 dBFS at C4, matching the existing keys. Every
  8-note chord stays under 4.1 % of the buffer budget (limit 40 %).

Test note 60.

| Name | Idea | Recipe | Wheel / aftertouch |
|---|---|---|---|
| **Tremolo EP** | FM electric piano with stereo auto-pan | `FM E.Piano`'s FM core: mod env → Cross Mod, velocity 90. LFO 2 sine 4.5 Hz → Pan, amount 40, **always on**. Chorus off. | Wheel deepens the pan toward full width (slot 1 becomes LFO 2 → Pan via wheel, replacing vibrato) |
| **Tine Grand** | Electric-grand-style: bright and harmonic | Triangle carrier, sine modulator at 2:1, Cross Mod 0.2. Mod env 0.5 ms / 400 ms / 0 → Cross Mod, velocity 80. Amp decay 5 s, sustain 0, release 400 ms. Chorus I, 20 %. | Default |
| **Poly Piano** | 80s analog "piano": saw + pulse | Saw + pulse 40 %, OSC 2 +6 cents. LP24 at 900 Hz, env amount 45, filter velocity 70, key track 100. Filter env 1 ms / 900 ms / 10 %. Amp 1 ms / 2.5 s / sustain 0, release 300 ms. Chorus I, 35 %. | Default |
| **Perc Organ** | Organ with a percussion "click" harmonic | Sine 8' + sine 2' (OSC 2). Mod env 0.5 ms / 180 ms / 0 → OSC 2 level, amount 60, with OSC 2 base level 0. Sub 25. Amp velocity 0, release 40 ms. Chorus II, 40 %. | LFO 1 6.8 Hz vibrato on the wheel (default) |
| **Harpsichord** | Plucked, bright, two-octave | Pulse 15 %; OSC 2 saw at 4', level 50. HPF at 200 Hz. LP24 at 6 kHz, key track 60. Amp 0.5 ms / 1.5 s / sustain 0, release 120 ms. Amp velocity 30. | Default |
| **Pluck Keys** | Modern synth pluck for chords | Saw + saw at +9 cents. SVF LP, res 25. Filter env 0.5 ms / 220 ms / 0, amount 50, velocity 60. Amp 1 ms / 700 ms / sustain 0, release 250 ms. Ensemble, 35 %. | Default |
| **Lo-Fi Keys** | Warbly, worn tape-style keys | Triangle + pink noise 6 %. LP24 at 1.6 kHz. LFO 2 smooth random 0.8 Hz → pitch, 0.15 semitones (wow). Drift 80. Amp decay 2 s, sustain 30. Chorus I, 25 %. | Wheel adds more wow (slot 3, LFO 2 via wheel → pitch) |
| **House Stab** | Short organ-ish chord stab | Pulse 50 % + saw at +5 cents; OSC 2 at 4', level 40. LP24 at 2.2 kHz, res 10. Amp 1 ms / 280 ms / sustain 0, release 80 ms. Chorus off. | Default |

## Step 4b: existing preset with a built-in tremolo

**Done (2026-11-14).** It measures −9.7 dBFS after the change.

**Wurli Bark** has its tremolo only on the mod wheel (LFO 2 → Amp, −30,
via wheel). Tremolo is part of that sound, so it becomes always on and
gentle, like Tremolo EP:
- Slot 4 loses its via: LFO 2 → Amp at −15.
- Slot 1 (default vibrato) becomes LFO 2 → Amp via wheel at −20, so the
  wheel deepens the tremolo up to today's depth (about −35 in total).

No other existing preset has a wobble or tremolo hidden on the wheel.

## Step 5: docs

- Update the preset table and count in
  [dragon_waave.md](dragon_waave.md#presets).
- Document the new check windows in [tools.md](tools.md) (the Dragon
  Waave checks paragraph).
- Note the Wurli Bark change in the preset comment.

## Listening checklist (per preset, in LYNX)

- Velocity: soft vs hard. Does it get brighter or louder as intended?
- Range: the category's extremes. No silence, no harsh aliasing, no
  thinning out.
- Wheel and aftertouch do what the comment says.
- Keys: an 8-note chord, held by hand (no pedal yet). Watch the header
  meter: DSP should stay under 40 %.
- Leads: legato runs. Glide only where intended, and no clicks on note
  changes.
- Bass: alongside a kick drum. No sub-bass mud from chorus, the attack
  cuts through, and it sits forward in the mix (the hotter bass window).
- Level: A/B against the presets on either side of it in the list.

## Order of work

| # | Step | Size |
|---|---|---|
| 1 | Level window, range notes, keys load check; retune existing outliers | ½ session |
| 2 | Bass ×8, then listen and tune in LYNX | 1 session |
| 3 | Leads ×8, then listen and tune | 1 session |
| 4 | Keys ×8, then listen and tune; Wurli Bark tremolo (4b) | 1 session |
| 5 | Docs | — |

Basses come first: they have the fewest existing presets to compare
against, and they're the most sensitive to the level and range checks.

## Decisions (2026-11-14)

- **Sustain pedal:** deferred; plan saved in
  [dragon_waave_sustain.md](dragon_waave_sustain.md).
- **Level windows:** −12 to −3 dBFS in general; basses hotter, at −8 to
  −1 dBFS.
- **Wheel-driven effects:** where the movement is the sound (Wobble Bass,
  Tremolo EP, and the existing Wurli Bark), it's on by default and the
  wheel adds more. Chip Lead's octave trill stays wheel-only.
