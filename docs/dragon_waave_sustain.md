# Dragon Waave sustain pedal: plan (deferred)

**Status:** deferred (2026-11-14). It was split out of the
[preset plan](dragon_waave_presets.md) so the presets don't wait for it.
Pick it up whenever keys playing needs a pedal.

**Problem:** the plugin only handles CC 1 (mod wheel) among MIDI CCs
(`vst/dragon_waave/src/lib.rs`, `handle_event`). A sustain pedal (CC 64)
does nothing, so keys presets can't be played the way most people play
keys.

**Done when:** holding the pedal sustains released notes in Poly mode,
lifting it releases them, and Mono / Legato behave as described below. All
of it is tested, and the docs list CC 64.

## Engine (`crates/dsp-core/src/synth`)

- **API:** `Synth::set_sustain(down: bool)`, beside `set_mod_wheel` and
  `set_aftertouch`.
- **Voice state:**
  - A voice gets a `sustained` flag.
  - A note-off while the pedal is down sets `sustained` instead of
    releasing. The voice stays `Held` for envelope purposes.
  - Lifting the pedal releases every voice that is `sustained` and whose
    key is no longer down.
- **Replaying a sustained note:** it retriggers as it does now (a new
  note-on for a sounding key), and clears `sustained` on the old voice's
  replacement.
- **Mono / Legato:**
  - The key stack keeps following physically held keys only, so
    last-note priority and legato glide are unchanged.
  - The pedal only delays the final release when the stack empties: the
    last note keeps sounding until the pedal lifts.
- **Polyphony and stealing:** sustained voices count toward polyphony and
  are stolen like released voices: oldest first, with the existing steal
  fade.
- **Host voice IDs:** a sustained voice still reports `VoiceTerminated` only
  when it actually ends. That's already the case, since reporting happens
  at the voice's end.
- **Reset:** clears the pedal state and all `sustained` flags.

## Plugin (`vst/dragon_waave/src/lib.rs`)

- `NoteEvent::MidiCC { cc: 64, value, .. }` → `synth.set_sustain(value >= 0.5)`.
- No new parameter: the pedal is a performance control, like the mod
  wheel, and isn't saved with the project.

## Tests (engine)

| Test | Expectation |
|---|---|
| Note-off with pedal down | Voice keeps sounding at sustain level |
| Pedal up | Sustained voices release (amp envelope enters release) |
| Key still held at pedal up | That voice keeps sounding |
| Replay a sustained note | One voice for that key, not two, matching the existing retrigger behaviour |
| Polyphony 4, pedal down, play 6 notes | Oldest sustained voices are stolen with the steal fade |
| Mono, pedal down, release all keys | Last note sustains; pedal up releases it |
| Legato glide with pedal | Glide follows held keys only, unchanged |
| Reset | Pedal state cleared |

A `clap-smoke` check isn't needed: the plugin side is one line, and
the engine tests cover the behaviour.

## Docs

- [dragon_waave.md](dragon_waave.md): engine API (`set_sustain`) and the
  controllers list (pitch bend, mod wheel, aftertouch, **sustain**).
- [dragon_waave_presets.md](dragon_waave_presets.md): the keys listening
  check can then include the pedal.

**Size:** about half a session.
