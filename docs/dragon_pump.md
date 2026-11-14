# Dragon Pump

A volume-ducking ("sidechain pump") mixing plugin. On every beat, MIDI
note or kick, it pulls the track's volume down and lets it swell back
along a curve. That makes room for the kick and gives the classic pumping
groove. The curves, trigger modes and look are our own (Dragon UI style);
no names, curve sets or artwork are taken from any existing product.

Code: `vst/dragon_pump` (plugin and editor) and `crates/dsp-core/src/duck`
(the curve, clock, trigger and detector maths).

```sh
./build2lynx.sh dragon_pump     # test, build, smoke-test, install
```

## Status

**Complete (2026-11-14).** All three trigger modes work and are tested in
`dsp-core` and through `clap-smoke` (22 parameter checks, after the default check, before every install).
Sidechain routing works in LYNX (see [Using it in LYNX](#using-it-in-lynx)).
Ideas not built yet are under [Stretch](#stretch). The build history is
at the end.

## Using it in LYNX

Put Dragon Pump on the track to duck (a bass, pad or whole bus), then pick
a mode with the chips in the header:

| Mode | What triggers a dip | Setup |
|---|---|---|
| **TEMPO** (default) | Every beat of the Rate (1/4 by default), locked to the host's beat position | None. Keeps pumping at the host tempo while stopped, unless Run When Stopped is off |
| **MIDI** | Every note-on, on its exact sample (note-offs are ignored) | Put the kick's MIDI pattern on the same track as Dragon Pump, or copy it there |
| **AUDIO** | Every kick arriving at the **Sidechain** input | Route the kick track to the Sidechain input (below) |

**Routing a sidechain in LYNX** (tested 2026-11-14). Either way works:
- the **SC** menu at the top right of the plugin window: choose the
  source, e.g. **"1 insert 1"** for track 1;
- **Wrapper Settings > Processing**: choose the kick track (e.g. track 1)
  as the input in the "from" setting.

The **SC** meter in the OUT panel moves, and the TRIGGER panel reads
**SIDECHAIN: IN**, when audio arrives.

**Tuning Audio mode:** the lamp beside the detector meter flashes on each
detected kick.
- **Kicks are missed:** lower **Threshold** until the meter's bright part
  crosses the white mark on every kick.
- **It fires on other sounds:** raise Threshold, or turn on **LISTEN**
  (you then hear what the detector hears) and move **Freq** until it's
  mostly the kick's thump.
- **Flams or a kick's bounce cause double dips:** raise **Retrig**.

## Signal flow

```
                 ┌──────────────────── trigger ────────────────────┐
  host tempo ───▶│ TEMPO: TempoClock, Rate + Offset → time in cycle│
  MIDI notes ───▶│ MIDI:  each note-on (× velocity scale)          │──▶ curve ──▶ duck d (0..1)
  sidechain  ───▶│ AUDIO: KickDetector hit                         │
                 └─────────────────────────────────────────────────┘
                                                       gain = 1 − mix · depth · d
  main in ──────────────────────────────────────▶ × smoother(gain) × Output ──▶ out
                                                  (LISTEN: out = band-passed sidechain)
```

- Only the gain moves: the audio is never filtered or delayed, so there's
  no latency.
- **Stereo** is linked (one gain for every channel); mono works too.
- Per 32-sample control block the shape settings are read (smoothed);
  Depth, Mix and Output are smoothed per sample.
- A 0.5 ms one-pole on the final gain (`GainSmoother`) catches the jumps
  the curve can't see: changing mode, or the transport stopping mid-dip
  with Run When Stopped off.
- `SAMPLE_ACCURATE_AUTOMATION` is off (as in Dragon Waave): parameter
  changes don't split the buffer. MIDI notes are still sample-accurate.

## The curve

```
gain
1.0 ┤▔╲                         ╱▔▔▔▔▔▔▔
    │  ╲                     ╱
    │   ╲                 ╱        ← Release (length) and Tension (bend)
    │    ╲             ╱
1−d ┤     ╲▁▁▁▁▁▁▁▁▁▁╱             ← Hold
    └─────┬────┬────────────┬──────▶ time
       Attack  Hold      Release
```

`DuckShape` in `dsp-core` describes it as a duck amount `d` (0 =
untouched, 1 = fully ducked); the gain is `1 − depth·d`.
- **Attack:** linear from wherever the curve was (`d_start`) to 1. Never
  shorter than 0.1 ms.
- **Hold:** 1.
- **Release:** `1 − f(x)` with `x` the progress 0..1 and
  `f(x) = (e^(a·x) − 1)/(e^a − 1)`, `a = 6·tension`. Positive tension
  recovers slowly then fast (the classic pump), negative fast then slow,
  0 is linear. Unlike a power curve its slope is finite everywhere, so no
  tension setting can click (tested against the attack ramp's step).
- **Retriggering mid-curve** starts the attack from the current amount,
  so 1/16 notes, rolls and flams never jump.

**Units by mode:** Tempo mode's Hold and Release are fractions of the
cycle, so a shape stays musical at any tempo or rate. MIDI and Audio have
no cycle, so theirs are in ms. They're separate parameters (`hold_pct` /
`hold_ms`, `release_pct` / `release_ms`), so automation never changes
meaning when the mode changes; the editor shows the pair in use.

**Shape chips** (in the CURVE panel's title row) set Attack, Hold,
Release and Tension together as one undo step, and are lit while the
curve matches. In MIDI and Audio modes the fractions are scaled to a
500 ms reference cycle (a 1/4 note at 120 BPM), so a shape sounds the same
and the defaults light Pump in every mode.

| Shape | Attack | Hold | Release | Tension | Feel |
|---|---|---|---|---|---|
| Pump (default) | 1 ms | 0 | 60 % | +40 | The classic: a fast dip, slow-then-fast recovery |
| Soft | 8 ms | 0 | 85 % | 0 | A gentle, even dip |
| Hard | 0.5 ms | 5 % | 50 % | +80 | Slams down, stays low, then snaps back |
| Gate | 1 ms | 45 % | 10 % | +20 | A long hold and a fast release: choppy |
| Swell | 2 ms | 0 | 100 % | +60 | The release covers the whole cycle |
| Tight | 0.5 ms | 0 | 25 % | +30 | A short dip that clears out quickly |
| Late | 15 ms | 0 | 60 % | +40 | A slow attack lets the kick's transient through first |
| Breathe | 3 ms | 0 | 70 % | −60 | Recovers fast, then eases in |

## Triggers

### Tempo

- **`TempoClock`** keeps the beat position as `base + n·step`: re-synced
  to the host's position at the start of every buffer while playing, and
  advanced per sample from the tempo within it. It never drifts: a 1/4
  cycle at 120 BPM starts on exactly every 24000th sample at 48 kHz.
- **The curve is a pure function of the time into the cycle**
  (`tempo_duck`): loops, jumps and starting mid-bar can't put it out of
  step. A release longer than the cycle carries into the next one
  without a jump; the carried amount is where the previous cycle ended,
  which doesn't depend on where it started, so no state is kept.
- **Rate:** 1/1, 1/2, 1/4, 1/8, 1/16, with dotted and triplet (1/1 only
  straight). **Offset** (±50 ms) shifts the grid for kicks that aren't
  exactly on it.
- **Transport stopped:** keeps running from where it was at the host's
  tempo, unless **Run When Stopped** is off (then unity). No tempo from
  the host: 120 BPM.

### MIDI

- **`TriggeredDuck`** restarts the curve on each note-on, on its sample
  within the buffer.
- **Velocity** (0–100 %) sets how much a note's velocity scales its depth:
  scale `= 1 − vel·(1 − velocity)`. At 0 % every note dips fully. A soft
  note landing in a deep dip glides up to its own floor rather than
  jumping; a note scaled to nothing does nothing.

### Audio

**`KickDetector`**:

```
sidechain (channels averaged) ─▶ band-pass (SC Freq, Q 1) ─▶ |x| ─▶ peak follower ─▶ threshold ─▶ hit
                                  unity at the centre             1 ms up, 50 ms down   + 3 dB hysteresis
                                                                                       + retrigger gap
```

- A hit is the follower **rising through** SC Threshold. It must fall
  3 dB below the threshold to re-arm, so a kick's own wobble counts once
  and a held bass note triggers only once.
- A rise inside **SC Retrigger** of the last hit is ignored, not delayed.
- Each hit restarts the curve at full depth, like a MIDI note.
- Detection lags a kick's start by a few ms, which isn't heard under a
  kick. A lookahead (under [Stretch](#stretch)) would remove it.
- **LISTEN** replaces the output with the band-passed sidechain.

## Editor

760 × 440, fixed size, from `dragon-ui` (panels, knobs, chips, the load
meter).

```
DRAGON PUMP  [TEMPO][MIDI][AUDIO]           shift: fine · double-click: reset  DSP 1% max 2% · GUI 0.9 ms max 1.4
┌ CURVE [Pump][Soft][Hard][Gate][Swell][Tight][Late][Breathe] ┐ ┌ TRIGGER ─────────────────────┐
│ 0 dB ▔╲                  ╱▔▔▔▔▔▔▔                            │ │ Tempo: RATE chips, (Offset), │
│        ╲ ●hold      ●rel╱                                    │ │   RUN STOPPED                │
│         ╲▁▁▁▁▁▁▁▁▁▁▁▁╱   ┊ ← playhead                        │ │ MIDI:  (Velocity)            │
│ -inf ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ (depth floor) ─ ─ ─ ─ ─ ─ ─ ─  │ │ Audio: (Thr)(Freq)(Retrig),  │
│                                         1/4 · 120 BPM        │ │   detector meter ●, LISTEN   │
└──────────────────────────────────────────────────────────────┘ └──────────────────────────────┘
┌ SHAPE ──────────────────────────────────────┐ ┌ OUT ─────────────────────────┐
│ (Depth) (Attack) (Hold) (Release) (Tension) │ │ (Mix) (Output)   IN  SC  GR  │
└─────────────────────────────────────────────┘ └──────────────────────────────┘
```

- **CURVE panel** (`curve_view.rs`):
  - Tempo mode shows one cycle at the host tempo; MIDI and Audio show
    the curve from a trigger over the shortest of 0.25, 0.5, 1, 1.5, 2 or
    3 s that fits it.
  - The y axis is linear gain (0, −6, −12 dB, −∞), so the dip looks as
    deep as it sounds. The reduction is shaded red; the dashed red line is
    the floor (Depth × Mix, where the gain really goes).
  - A peach playhead with a dot on the curve: the position in the cycle,
    or the time since the last trigger.
- **Handles:**
  - **Hold** (on the floor where the hold ends): drag left/right.
  - **Release** (halfway through the release, on the curve): drag
    left/right for Release (it grows twice as fast as the handle moves,
    so the handle stays under the pointer), up/down for Tension (down =
    slower start).
  - **Floor line:** drag up/down for Depth (not below 5 % Mix, where
    Depth barely moves it).
  - Shift drags at a tenth of the speed; double-click resets; the cursor
    and a tooltip show what a spot edits. Each drag is one automation
    gesture per parameter it touches. In MIDI and Audio modes the time
    span is frozen during a drag.
- **TRIGGER panel:** shows the current mode's controls (see the sketch),
  plus the sidechain status.
- **Meters:** IN and SC levels; **GR** (gain reduction) hangs from the
  top, 0 to 40 dB, recovering at 240 dB/s so it follows the pump; the
  detector meter (−60 to 0 dB) with the threshold mark and hit lamp.
- **Load meter** (header): the DSP share of each buffer's time and the
  time to build a GUI frame, with peaks held for a second. It's
  `dragon_ui::perf`, shared with Dragon Waave; see
  [dragon_waave_perf.md](dragon_waave_perf.md#reading-the-meter) for how to
  read it.
- **Threading:** the audio thread publishes everything the editor shows
  through atomics in `meters.rs` (input, sidechain and detector peaks,
  hit count, lowest gain, playhead, tempo) and `dragon_ui::perf`. No
  locks or allocation on the audio thread.

## Parameters

IDs are permanent: hosts store them in projects and automation. Never
rename or reuse one.

| ID | Name | Range | Default | Notes |
|---|---|---|---|---|
| `mode` | Mode | Tempo / MIDI / Audio | Tempo | |
| `depth` | Depth | 0–100 % | 100 % | How far the gain dips |
| `attack` | Attack | 0.1–20 ms | 1 ms | |
| `hold_pct` | Hold | 0–50 % of the cycle | 0 | Tempo mode |
| `release_pct` | Release | 5–100 % of the cycle | 60 % | Tempo mode |
| `hold_ms` | Hold (ms) | 0–500 ms | 0 | MIDI and Audio |
| `release_ms` | Release (ms) | 10–2000 ms | 300 ms | MIDI and Audio |
| `tension` | Tension | −100 to +100 % | +40 % | Release bend |
| `rate` | Rate | 1/1 to 1/16, dotted, triplet | 1/4 | Tempo |
| `offset` | Offset | ±50 ms | 0 | Tempo; + = later |
| `run_stopped` | Run When Stopped | on/off | on | Tempo |
| `vel` | Velocity | 0–100 % | 0 % | MIDI |
| `sc_thresh` | SC Threshold | −60–0 dB | −24 dB | Audio |
| `sc_freq` | SC Freq | 40–400 Hz | 80 Hz | Audio |
| `sc_retrig` | SC Retrigger | 20–500 ms | 60 ms | Audio |
| `sc_listen` | SC Listen | on/off | off | Audio |
| `mix` | Mix | 0–100 % | 100 % | Dry/wet of the ducked signal |
| `out_gain` | Output | ±12 dB | 0 dB | After the ducking |

**Mix vs Depth:** Depth reshapes the curve; Mix blends with the dry
signal, as parallel processing would.

**Ports:** stereo (or mono) main in/out, plus an aux input of the same
width named "Sidechain"; MIDI input. **Identity:** CLAP ID
`com.dragon-vst.dragon-pump` (features AudioEffect, Stereo, Mixing); VST3
class ID `DragonPump000001` (Fx, Dynamics); bundles `Dragon Pump.vst3` /
`.clap`.

## Code layout

| Path | Contents |
|---|---|
| `crates/dsp-core/src/duck/curve.rs` | `DuckShape`, `release_shape` |
| `crates/dsp-core/src/duck/clock.rs` | `TempoClock`, `tempo_duck` |
| `crates/dsp-core/src/duck/trigger.rs` | `TriggeredDuck` (MIDI and Audio) |
| `crates/dsp-core/src/duck/detector.rs` | `KickDetector`, `DetectorSettings` |
| `crates/dsp-core/src/duck/mod.rs` | Re-exports, `GainSmoother` |
| `crates/dsp-core/src/svf.rs` | `SvfCoefs::band_pass` (the detector's filter) |
| `vst/dragon_pump/src/lib.rs` | The plugin: ports, `process()` |
| `vst/dragon_pump/src/params.rs` | Parameters, `Mode`, `Rate` (and its note-value/feel split for the chips) |
| `vst/dragon_pump/src/editor.rs` | Layout, chips, knobs, meters |
| `vst/dragon_pump/src/curve_view.rs` | The CURVE panel: drawing, handles, playhead |
| `vst/dragon_pump/src/shapes.rs` | Shape presets |
| `vst/dragon_pump/src/meters.rs` | Audio-to-editor atomics and meter ballistics |
| `vst/dragon_pump/smoke-checks.txt` | `clap-smoke` checks |
| `crates/dragon-ui/src/perf.rs` | The shared load meter |

## Testing

**Unit tests:** 26 in `dsp-core`'s `duck` module, 27 in the plugin.

| Area | Tests |
|---|---|
| Curve | End points; floor `1 − depth`; ends at exactly unity; release monotonic at every tension; tension 0 linear; ±tension mirror; no step bigger than the attack ramp allows; retrigger continuity |
| Tempo clock | Exact cycle starts at 120 and 90 BPM; dotted and triplet spacing; mid-beat starts; Offset in exact samples; free running and tempo changes; long releases carry over |
| Triggered | Starts on the sample; velocity scaling; no jumps through hard, soft and rolled retriggers; reset |
| Detector | One hit per kick at 120 BPM, each within 5 ms; silence and kicks 30 dB down never trigger; a held bass triggers once; a loud 3 kHz click track is filtered out; the retrigger gap; band-pass gain |
| Editor maths | Curve points (exact corners), handle positions, hit testing, drag maths and clamping, the MIDI view spans, shape presets in both units, rate chips |
| Editor, headless egui | Real drags through a recording host: one begin … end gesture on the right parameter (`hold_pct` in Tempo, `hold_ms` in MIDI) and the right value; empty space sends nothing. The whole editor laid out at 760 × 440 in every mode: nothing spills out of the window, and the header's groups keep 50 px between them |
| Meters | Peaks and hit counts taken once; meter and GR ballistics |

**Smoke checks** (`smoke-checks.txt`, run by `build2lynx.sh`): a 1 kHz
input tone; "peak" (`--expect-db`) shows the gain recovers to unity,
"trough" (the quietest 10 ms window) how far it dips. Expected values
come from the maths, not from measuring the plugin.

| Group | Checks |
|---|---|
| Tempo | Dips on the beat and recovers; Depth 50 % troughs at −5.95 dB (floor −6.02); Depth 0 and Mix 0 untouched; 1/8 notes; a release longer than the cycle at 90 BPM peaks at −12.76 dB; no transport runs at 120 BPM; stopped with Run When Stopped off is unity |
| MIDI (`--note-every`) | Dips and recovers; notes every 0.25 s retrigger mid-release, peaking at −3.99 dB; Velocity 100 % with velocity-0.5 notes troughs at −5.95 dB; Velocity 0 % dips fully; no notes, untouched; Hold 100 ms |
| Audio (`--sidechain-kicks`) | Dips on each kick and recovers; a silent sidechain, a threshold above the kicks, and SC Freq 400 Hz all leave it untouched; Listen plays the band-passed kick at +1.46 dB over the input (from simulating the filter on the kick); Tempo mode ignores the sidechain |
| Output | −6 dB on top of the ducking; the −12 dB limit |

A kick-triggered dip measures about −35 dB where Tempo and MIDI measure
−40. That's the measuring windows, not the dip: they're 10 ms tiles that
line up with on-beat notes, while a detected hit lands a few ms into one.
MIDI notes shifted by 120 samples measure −36 dB the same way.

## Stretch

- **Lookahead:** delay the main signal 1–5 ms and report the latency, so
  Audio mode dips *at* the transient.
- **Per-band ducking:** duck only the lows.
- **MIDI curve select:** choose the shape from a note or CC.
- **Beat-position jitter:** the clock re-syncs to the host every buffer.
  If a host reports a jittery position, re-sync only when the drift
  exceeds a few samples.

## Build history (2026-11-14)

| Step | What |
|---|---|
| 0 | Skeleton: pass-through with Output, sidechain port and meter; `clap-smoke` passes a buffer for every input port. The LYNX sidechain check passed (both routes above) |
| 1 | DSP core and Tempo mode: `DuckShape`, `TempoClock`, `GainSmoother`; `clap-smoke --tempo`, `--trough-max-db`, `--expect-trough-db` |
| 2 | Editor: layout, rate chips, curve display, playhead and GR meter via atomics, headless layout test |
| 3 | Curve editing: draggable handles, shape chips |
| 4 | MIDI mode: `TriggeredDuck`, `mode`, `hold_ms`, `release_ms`, `vel`; `clap-smoke --note-every` |
| 5 | Audio mode: `KickDetector`, `SvfCoefs::band_pass`, the `sc_*` parameters, Listen, detector meter and hit lamp; `clap-smoke --sidechain-kicks` |
| 6 | Polish: the load meter moved to `dragon_ui::perf` and shared with Dragon Waave; this reference doc; README and docs index |

**Decisions:** the name Dragon Pump; all three trigger modes in the first
version; Audio mode ships with the LYNX routing documented above, and MIDI
mode covers hosts without a sidechain.
