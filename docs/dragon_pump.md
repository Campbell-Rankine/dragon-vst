# Dragon Pump: plan

A volume-ducking ("sidechain pump") mixing plugin. It works in the spirit
of the well-known one-knob ducking tools: on every beat, or every kick, it
pulls the track's volume down and lets it swell back along a curve. That
makes room for the kick and gives the classic pumping groove. It uses our
own parametric curves, extra trigger modes and the Dragon UI style. No
names, curve sets or artwork are taken from any existing product.


**Done when:**
- In LYNX, ducking a bass or pad works three ways, each lining up with
  the kick: in time with the host tempo, from MIDI notes, and from a kick
  on the sidechain input.
- There are no clicks at any setting.
- It's tested in `dsp-core` and through `clap-smoke`, and installed by
  `build2lynx.sh` like the others.

## Status

**Step 0 (skeleton): done (2026-11-14).** `vst/dragon_pump` builds,
passes its smoke checks and installs with `./build2lynx.sh dragon_pump`.
- **Audio:** passes through with the Output gain (±12 dB, ID `out_gain`).
  Nothing ducks yet.
- **Ports:** a stereo (or mono) main in/out, plus a sidechain (aux) input
  with the same channel count, named "Sidechain". MIDI input is declared
  for MIDI mode later.
- **Editor:** 760 × 440, Dragon UI style. It has a **SIGNAL** panel with
  **IN** and **SIDECHAIN** level meters and a status line ("Sidechain:
  receiving audio" / "no signal"), and an **OUT** panel with the Output
  knob.
- **`clap-smoke`:** now passes a buffer for every audio input port the
  plugin declares (silence for the sidechain). nih-plug's CLAP wrapper
  reads aux buffers without checking the host's count, so a host that
  passed only the main input would make it read past the end.

**Next: the LYNX sidechain check (you, in LYNX).**
1. Rescan plugins, then put Dragon Pump on a bass or pad track.
2. Find LYNX's sidechain option, usually in the plugin slot's menu or the
   kick track's sends/outputs, and send the kick track to Dragon Pump's
   **Sidechain** input.
3. Play. If the **SIDECHAIN** meter moves and the status reads
   *receiving audio*, routing works. Note the steps here.
4. If LYNX has no way to feed it, the meter stays at *no signal*. Audio
   mode will still be built for other hosts, and LYNX users get MIDI mode
   (see [Decisions](#decisions-2026-11-14)).

Then: step 1 (DSP core and Tempo mode).

## What it does

```
                 ┌──────────────── trigger ────────────────┐
  host tempo ───▶│ TEMPO: every 1/4 (rate), locked to beats│
  MIDI notes ───▶│ MIDI:  every note-on                    │──▶ curve ──▶ gain
  sidechain  ───▶│ AUDIO: kick detected on the aux input   │      │
                 └─────────────────────────────────────────┘      ▼
  main in ───────────────────────────────────────────────▶  × gain × output ──▶ out
```

Each trigger restarts the **curve**: gain dips to `1 − depth`, holds, then
recovers to unity. Only the gain moves; the audio itself is never
filtered or delayed (unless the lookahead stretch goal is on).

## The curve

Five parameters, not a fixed bank of drawn shapes, so every shape is
automatable and can be edited on the graph:

```
gain
1.0 ┤▔╲                         ╱▔▔▔▔▔▔▔
    │  ╲                     ╱
    │   ╲                 ╱        ← Release (length) and Tension (bend)
    │    ╲             ╱
1−d ┤     ╲▁▁▁▁▁▁▁▁▁▁╱             ← Hold
    └─────┬────┬────────────┬──────▶ time / cycle
       Attack  Hold      Release
```

| Parameter | Range | Default | Notes |
|---|---|---|---|
| **Depth** | 0–100 % | 100 % | How far the gain dips. 0 = no effect |
| **Attack** | 0.1–20 ms | 1 ms | Fade into the dip. Never zero, so there's no click |
| **Hold** | 0–50 % of cycle (or 0–500 ms) | 0 | Time held at the floor |
| **Release** | 5–100 % of cycle (or 10–2000 ms) | 60 % | Time to recover |
| **Tension** | −100 to +100 | +40 | Recovery bend. − = fast then slow (log), 0 = linear, + = slow then fast (exp, the classic pump) |

- **Units by mode:** in Tempo mode, Hold and Release are fractions of the
  cycle, so a shape stays musical at any tempo or rate. In MIDI and Audio
  modes there's no cycle, so they're in ms.
- **Separate parameters per unit:** `hold_pct`/`hold_ms` and
  `release_pct`/`release_ms`, so automation never changes meaning when the
  mode switches. The editor shows the pair that applies.
- **Shape presets:** a row of chips that set Attack, Hold, Release and
  Tension together. They're our own, named by feel:
  - **Pump** (the default)
  - **Soft**
  - **Hard**
  - **Gate** (long hold, fast release)
  - **Swell** (release covers the whole cycle)
  - **Tight** (short release)
  - **Late** (a slow 15 ms attack, so the kick's transient passes through
    before the dip)
  - **Breathe** (log release)
- **Retriggering mid-curve:** the dip starts from the *current* gain, so
  fast triggers (1/16, rolls, flams) never jump.

## Triggers

| Mode | Trigger | Parameters |
|---|---|---|
| **Tempo** (default) | Every `Rate`, locked to the host's beat position, so the dip lands on the beat | **Rate:** 1/1, 1/2, 1/4 (default), 1/8, 1/16, plus dotted and triplet. **Offset:** −50 to +50 ms nudge, for kicks that aren't exactly on the grid |
| **MIDI** | Every note-on (note-offs ignored), sample-accurate | **Velocity:** 0–100 %, how much velocity scales depth. 0 = every hit is full depth |
| **Audio** | A transient on the **sidechain** (aux) input | **Threshold** (dB). **SC Freq:** a band-pass centre, 40–400 Hz, default 80, to pick out the kick. **Retrigger:** 20–500 ms minimum gap between triggers. **Listen:** hear the filtered sidechain, to tune the band |

- **Transport stopped (Tempo mode):** keep running at the host tempo from
  where it was, so you can tweak while paused. A *Run when stopped*
  switch, on by default, turns it off.
- **No tempo from the host:** 120 BPM, as Dragon Waave does.
- **Audio detector:**
  - a 2-pole band-pass at SC Freq;
  - a peak follower with ~1 ms attack and ~50 ms release;
  - a trigger when the follower rises through the threshold (hysteresis
    3 dB) and the retrigger gap has passed.
- **Detection latency:** detection lags the transient by about 1 ms. That
  won't be heard under a kick. The lookahead stretch goal removes it.

## Other parameters

| Parameter | Range | Default |
|---|---|---|
| **Mode** | Tempo / MIDI / Audio | Tempo |
| **Mix** | 0–100 % (dry/wet of the ducked signal) | 100 % |
| **Output** | ±12 dB | 0 dB |

- **Mix vs Depth:** Depth reshapes the curve; Mix blends with the dry
  signal, as parallel processing would. Both are useful and both are
  cheap.
- **Stereo:** one gain for both channels (linked). Mono works too.
- **IDs:** short and permanent (`depth`, `attack`, `hold_pct`, `hold_ms`,
  `release_pct`, `release_ms`, `tension`, `mode`, `rate`, `offset`, `vel`,
  `sc_thresh`, `sc_freq`, `sc_retrig`, `sc_listen`, `run_stopped`, `mix`,
  `out_gain`). The same rules as the other plugins: never renamed or
  reused.

## Editor (Dragon UI)

About 760 × 440, fixed size. Styled from `dragon-ui`: `#121418` background,
`#242830` panels with peach borders, a peach title, coloured knobs and
ON/OFF pills.

```
DRAGON PUMP      [TEMPO][MIDI][AUDIO]                       DSP 0% · GUI 1.2 ms
┌ CURVE ───────────────────────────────────────────┐ ┌ TRIGGER ─────────────┐
│ 0 dB ▔╲                    ╱▔▔▔▔▔▔▔▔▔             │ │ Rate  [1/4][1/8][1/16]│
│        ╲ ●hold        ●rel╱                      │ │       [1/2][1/1] T  D │
│         ╲▁▁▁▁▁▁▁▁▁▁▁▁▁▁╱   ┊ ← playhead          │ │ (Offset)             │
│ −∞ ───────────────────────────────────────────── │ │ AUDIO: (Thr)(Freq)   │
│ [Pump][Soft][Hard][Gate][Swell][Tight][Late][Br.]│ │        (Retrig)[LISTEN]│
└──────────────────────────────────────────────────┘ └──────────────────────┘
┌ SHAPE ──────────────────────────────────────┐ ┌ OUT ─────────────────────┐
│ (Depth) (Attack) (Hold) (Release) (Tension) │ │ (Mix) (Output)  ▮ GR     │
└─────────────────────────────────────────────┘ └──────────────────────────┘
```

- **Curve panel:**
  - The curve over one cycle (Tempo) or over Attack + Hold + Release (MIDI
    and Audio).
  - Two handles: drag the **hold** handle horizontally for Hold; drag the
    **release** handle horizontally for Release and vertically for
    Tension. Depth is the floor line, also draggable.
  - Each drag is one automation gesture, as with the knobs.
- **Playhead:** a vertical line at the current position in the curve, with
  a dot at the current gain. The audio thread publishes phase and gain as
  two atomics (the same pattern as Dragon Waave's load meter); nothing
  else crosses threads.
- **Gain-reduction meter:** a vertical bar fed by the same atomics.
- **Mode chips** light the controls that apply and grey out the others
  (Rate and Offset for Tempo; Velocity for MIDI; the sidechain controls
  for Audio).
- **Load meter:** the same `DSP · GUI` readout as Dragon Waave. Move
  `perf.rs` into `dragon-ui`, or a small shared crate, so both use one copy.
- **Colours:** curve and handles in cyan; dip floor and GR in red; trigger
  controls in peach; sidechain controls in mint.

## Code layout

| Path | Contents |
|---|---|
| `crates/dsp-core/src/duck/` (new) | `curve.rs`: `DuckShape` and the pure `gain_at(t)`. `clock.rs`: `TempoClock` (beat position → trigger times within a block). `detector.rs`: `KickDetector` (band-pass, follower, threshold, retrigger). `mod.rs`: `Ducker`, which takes triggers and produces the gain per sample, plus the playhead state. No nih-plug. |
| `vst/dragon_pump/src/lib.rs` | Plugin: main stereo in/out plus a **stereo aux input** named "Sidechain" (and mono layouts). MIDI input `Basic`. Calls `Ducker` in 32-sample control blocks, gain per sample. |
| `vst/dragon_pump/src/params.rs` | Parameters above, with persisted editor state |
| `vst/dragon_pump/src/editor.rs` | Layout, chips, knobs |
| `vst/dragon_pump/src/curve_view.rs` | The curve panel: drawing, handles, playhead (point maths unit-tested, as Waave's `displays.rs` is) |
| `vst/dragon_pump/smoke-checks.txt` | `clap-smoke` checks (below) |
| `bundler.toml` | `[dragon_pump] name = "Dragon Pump"` |

**Identity:**
- CLAP ID `com.dragon-vst.dragon-pump`, features `AudioEffect`, `Stereo`,
  `Mixing`.
- VST3 class ID `DragonPump000001` (16 bytes), subcategories `Fx`,
  `Dynamics`.
- Bundles `Dragon Pump.vst3` / `.clap`.

## Testing

**Unit tests (`dsp-core`):**

| Area | Test |
|---|---|
| Curve | `gain_at(0) = 1`; floor = `1 − depth`; it ends at exactly 1; release is monotonic for every tension; tension 0 is linear; ±tension mirror each other |
| Clicks | The largest per-sample gain step is under a bound set by Attack (e.g. a 1 ms attack at 48 kHz: step ≤ 1/48) |
| Retrigger | A retrigger mid-release starts from the current gain (no jump) |
| Tempo clock | At 120 BPM, 1/4: triggers at sample 0, 24000, 48000 … from beat 0. A buffer starting mid-beat triggers at the right offset. Dotted and triplet spacing. Offset shifts by exact samples. Tempo change mid-run |
| Transport | Stopped + Run when stopped keeps the spacing; stopped + off holds unity |
| MIDI | A note-on at sample *n* starts the dip at *n*; velocity scaling |
| Detector | Synthetic kicks (decaying 60 Hz sine bursts) at 120 BPM trigger once each. A sustained bass doesn't retrigger. Retrigger gap respected. Threshold hysteresis |
| Mix and Output | Mix 0 is bit-identical to the input × output gain |

**`clap-smoke` additions** (needed for any tempo or sidechain plugin):
- `--tempo <bpm>`: send a playing transport from beat 0. It's null today,
  so nih-plug sees no tempo.
- `--sidechain-kicks <bpm>`: feed the aux input with synthetic kicks on
  each beat. The host currently gives no aux buffer.
- `--trough-max-db <dB>`: the quietest 10 ms window after settling must be
  below this. That proves the ducking happens; the existing peak checks
  prove it recovers.

**Smoke checks:**

| Check | Expectation |
|---|---|
| Default (Tempo, 1/4, depth 100 %) at 120 BPM, tone in | Peak ≈ input (recovers to unity); trough < −40 dB |
| Depth 50 % | Trough ≈ −6 dB (±0.5) |
| Depth 0 | Unity throughout (±0.05 dB) |
| Mix 0 | Unity throughout |
| MIDI mode, notes every 0.25 s | Trough < −40 dB |
| Audio mode + `--sidechain-kicks 120` | Trough < −40 dB |
| Audio mode, silent sidechain | Unity throughout (no false triggers) |
| No transport | Runs at 120 BPM, no error |

## Steps

| # | Step | Size |
|---|---|---|
| 0 | **Scaffold:** `vst/dragon_pump` passing audio through with Output gain; aux input declared; bundler entry; `build2lynx.sh dragon_pump` installs it; it shows up in LYNX and accepts a sidechain route | ½ session |
| 1 | **DSP core:** curve, tempo clock, `Ducker`, with unit tests; Tempo mode wired; `clap-smoke --tempo` and `--trough-max-db` | 1 session |
| 2 | **Editor:** layout, knobs, mode and rate chips, curve panel drawing; playhead and GR meter via atomics | 1 session |
| 3 | **Curve editing:** draggable handles, shape-preset chips | ½ session |
| 4 | **MIDI trigger** + tests and smoke checks | ½ session |
| 5 | **Audio trigger:** detector, Listen, `clap-smoke --sidechain-kicks`, checks | 1 session |
| 6 | **Polish:** shared perf meter, docs page (`docs/dragon_pump.md` becomes the reference doc, like Waave's), README and docs index | ½ session |
| — | **Stretch:** lookahead (delays the main signal 1–5 ms and reports latency, so Audio mode dips *at* the transient); per-band ducking (only the lows); an external MIDI curve-select | — |

Step 0 goes first because it settles the riskiest unknown early: whether
LYNX routes a sidechain into a VST3/CLAP aux input.

## Risks

- **LYNX sidechain support:** if LYNX can't route audio to a plugin's aux
  input, Audio mode can't be used there. Tempo and MIDI modes don't need
  it. Step 0 checks this before the detector is built.
- **Beat position accuracy:** some hosts report `pos_beats` with jitter, or
  only at the buffer start. The clock derives positions within the buffer
  from tempo, and only re-syncs to the host when the drift exceeds a few
  samples, so dips don't wobble.
- **Loops and jumps:** when the transport loops or is moved, the beat
  position jumps. The clock re-syncs at once and starts a fresh cycle,
  rather than ramping through.

## Decisions (2026-11-14)

- **Name:** Dragon Pump.
- **Scope:** all three trigger modes (Tempo, MIDI, Audio) in the first
  version.
- **Sidechain in LYNX:** not used yet, so step 0 must find out whether
  LYNX can route audio into a plugin's aux input, and how. Record the
  routing steps in the reference doc. If it can't, Audio mode still ships
  (other hosts can use it), and the docs point LYNX users to MIDI mode:
  put the kick's MIDI pattern on Dragon Pump's track, or copy it there.
