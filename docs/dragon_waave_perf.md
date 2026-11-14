# Dragon Waave performance: plan

**Symptom (reported after a week of use):** audible buffering — dropouts
and crackle — while the synth plays. It's worst while the sound is being
changed: knobs turned, automation running, presets switched.

**Done when:**
- The clap-smoke automation test below runs with a worst-case process
  call under 30 % of the buffer budget (128 samples at 48 kHz, 16 voices,
  HQ off).
- An hour of normal use in LYNX, turning knobs while playing chords, has no
  dropouts.

## Status

Phases 0 and 1 are done (2026-11-14). The headless measurements below
**did not reproduce the dropouts**: the audio thread stays far inside its
budget even under dense automation. Phase 1 still lowers the cost of
editing, but the cause in LYNX is not yet known. **Next:** play in LYNX
with the new header meter showing, and note the DSP and GUI numbers when a
dropout happens (see [Reading the meter](#reading-the-meter)).

### Results

`clap-smoke`, 16-note chord, 48 kHz, `process()` time as a share of the
buffer's real-time budget (mean / p99). The *heavy* patch adds HQ, unison
4, polyphony 16, ensemble, OSC 2, drift 100 %, drive and resonance.
*Automated* means six parameters (cutoff, detune, drive, OSC 2 fine, key
track, amp attack) each get an event **every sample**: far denser than a
real knob.

| Case | Before | After phase 1 |
|---|---|---|
| Default patch, 128-sample buffers, static | 2.0 / 4.7 % | 1.8 / 3.5 % |
| Default patch, 128, automated | 3.2 / 5.8 % | 2.5 / 4.4 % |
| Heavy patch, 128, static | 8.1 / 17.8 % | 8.2 / 15.8 % |
| Heavy patch, 128, automated | 9.2 / 17.1 % | 8.5 / 16.2 % |
| Heavy patch, 32, automated | 9.3 / 18.1 % | 8.7 / 17.7 % |

The engine on its own (`bench_editing`, 16 voices, patch changed every
block) averages 7–8 % at every block size from 1 to 128 samples. So
`set_patch` itself is cheap.

What this rules out (in a headless host):
- the engine's per-sample cost,
- nih-plug's buffer splitting (it costs ~1 percentage point, now removed),
- the patch rebuild per control block.

Remaining suspects:
- **The editor.** It's 1220 × 760 and rebuilds every frame. In some hosts,
  a busy UI thread delays audio. The meter's GUI number shows this.
- **The host's buffer size or audio settings** in LYNX. A small buffer
  leaves less slack for any spike.
- **Something only a real host does,** such as flush calls while the GUI
  edits parameters, or the host processing on a non-realtime thread.

### Reading the meter

The editor header now shows `DSP a% max b% · GUI c ms max d`:
- **DSP:** share of each buffer's time `process()` used. The average is
  smoothed; the max is held for a second. A max near or over 100 % is a
  dropout caused by the synth.
- **GUI:** time to build one editor frame, before GPU drawing. At 60 fps a
  frame has 16.7 ms in total. If GUI is high when the dropouts happen but
  DSP is low, the editor is the problem (phase 3's GUI items move up).
- **Both low during a dropout:** the problem is outside the plugin.
  Check LYNX's buffer size and CPU meter.

## What we know

- **Steady state is cheap.** The engine benchmark (16 voices, everything
  on) runs at about 6.5 % of one core, 13 % in HQ. A static patch shouldn't
  drop out, so the problem is in what *changes* when you edit.
- **The plugin asks for split buffers.** `lib.rs` sets
  `SAMPLE_ACCURATE_AUTOMATION = true`. In nih-plug (checked in
  `src/wrapper/clap/wrapper.rs` around line 1973; VST3 is the same), that
  splits the host's buffer at **every parameter change**. A knob drag or
  automation lane can send dozens of changes per buffer. Each piece is a
  separate `process()` call with its own setup, and blocks can be 1 sample
  long.
- **Every control block rebuilds everything.** Every 32 samples (and in
  every split piece) `process()` builds a full `Patch` from ~150 parameters
  and calls `set_patch`, whether or not anything changed. Note events build
  a second one.
  - `set_patch` sanitizes the patch, rebuilds all three envelope rate
    sets, recomputes the chorus tone coefficient, and re-targets ~25 ramps.
  - While detune, drift or an oscillator's tuning is moving, it also
    retunes every sounding voice, every block.
- **Some per-sample work only needs redoing when settings move.**
  - Shared, per sample: `filter_now()` computes the drive gain
    (`db_to_gain`) and the HPF coefficient (`exp2` + `tan`). `mod_now()`
    computes the glide coefficient (`exp`) and copies all 6 matrix slots.
  - Per voice, per sample:
    - two `exp2` for envelope time scaling, even when the matrix doesn't
      touch them;
    - two `exp2` for pitch whenever drift is on;
    - an `exp2` and a `tan` for the cutoff;
    - the matrix evaluation.
- **The benchmark doesn't cover editing.** It plays a fixed patch in
  512-sample blocks. It never changes parameters, never renders short
  blocks, and never churns notes (stealing). So it can't see the reported
  problem.
- **Unlikely causes:**
  - Allocation: the engine is allocation-free after construction, and
    `assert_process_allocs` checks this in debug builds.
  - Locks: nothing is shared with the GUI except atomic parameters.
  - Denormals: nih-plug turns on flush-to-zero.

The likely cause was thought to be **splitting plus full rebuilds**: many
tiny `process()` calls, each paying the whole per-block cost. Phase 0
measured it, and it turned out small (see [Results](#results)).

## Phase 0: measure (do first)

1. **Automation bench** (`dsp-core`, `#[ignore]`, beside `bench`):
   - Same patch as `bench`, with 16 voices.
   - Each block, change cutoff, detune, drive and one envelope time, call
     `set_patch`, then render the block.
   - Run with block sizes 1, 4, 32 and 128.
   - Report mean and **worst** block time as a percentage of the block's
     real-time budget.
   - Add a note-churn variant: a new note every 50 ms, so stealing is
     exercised.
2. **Patch-build bench** (`dragon_waave`): time `params.patch(Read::Target, …)`
   plus `set_patch` on its own. This is the fixed per-block overhead.
3. **clap-smoke automation:**
   - New option `--automate <param>:<from>:<to>:<every-N-samples>`. It
     sends a parameter event every N samples across all blocks, so
     nih-plug's splitting is exercised for real.
   - New option `--blocks <size>` to test small host buffers.
   - Report the worst `process()` time against the buffer budget, and fail
     past a threshold.
   - Add it to `smoke-checks.txt` once there's a threshold to hold.
4. **In the host:**
   - Build with `--profile profiling` (symbols kept).
   - Play chords in LYNX while turning cutoff and detune, under Instruments'
     Time Profiler.
   - Check the audio thread (top functions) and the main thread (egui
     cost), to rule the GUI in or out.
5. **DSP meter:** time each `process()` with `Instant` (no allocation) and
   publish the load and peak via atomics (now `crates/dragon-ui/src/perf.rs`, shared with Dragon Pump).
   The editor shows them in its header, with its own frame time. It's
   always on: two clock reads per call cost nothing measurable. It's the
   fastest way to see the problem in the real host.

**Output:** a short results table added to this page, before and after
each phase.

## Phase 1: stop paying for changes we don't need

The likely fix for the reported problem.

1. **Turn off sample-accurate automation** (`SAMPLE_ACCURATE_AUTOMATION = false`).
   - The engine already moves continuous settings over 32-sample ramps, and
     every smoother is 10–50 ms. So splits at parameter changes buy no
     audible precision.
   - Note timing doesn't depend on it: `process()` already splits at note
     events itself.
   - nih-plug still splits on transport changes, which are rare.
   - *Check:* the automation sweep renders identically within −80 dB of
     the current build at block size 128, except in the first 32 samples
     after each change.
2. **Skip unchanged patches** (done as plain equality: the plugin keeps
   the last applied `Patch` and skips `set_patch` when the new one is
   equal). `Patch` is already `PartialEq`.
   - Keep the last applied patch and skip `set_patch` when the new one is
     equal, which is the common case: no knob moving and smoothers settled.
   - Make it cheap to tell: check `smoothed.is_smoothing()` on the smoothed
     parameters and compare the discrete ones. Then the patch is only built
     when something moved.
   - Drop the extra `Read::Current` build at note events when nothing is
     smoothing.
3. **Split `apply_patch` by what changed.**
   - Rebuild `Envs` only when an envelope setting changed.
   - Recompute `chorus_tone_g` only when tone changed.
   - Retune voices only when the tuning inputs changed by more than
     0.01 cent. Smoothed detune, for example, moves every block for 10 ms
     and then stops.
4. **Presets:** begin all gestures, set all values, then end all, instead
   of ~150 separate begin/set/end triples. Hosts that group overlapping
   gestures then record one undo step. With step 1, it no longer causes
   ~150 splits either.

*Check:* re-run phase 0. The expected result is that worst-case block time
while editing drops to near the static case. If the user confirms the
dropouts are gone, phase 2 is headroom work, not a fix.

## Phase 2: control-rate modulation (headroom)

This lowers the baseline cost, so more voices, HQ and small host buffers
all fit.

1. **Shared per-sample settings at control rate.** In `filter_now()` and
   `mod_now()`:
   - When the ramps feeding them are settled, compute the drive gain, HPF
     coefficient and glide coefficient once per block.
   - While they're moving, compute them every sample as now.
   - `Ramp` gains an `is_settled()`.
2. **Per-voice modulation every 8 samples** (`MOD_INTERVAL`):
   - Evaluate the matrix and LFO-depth sums, envelope time scaling, pitch
     ratio and cutoff octave every 8 samples. Linearly interpolate the
     oscillator increments and the filter's `g` in between.
   - Envelopes and the audio path stay per-sample.
   - This is how most synths do it. At 48 kHz, 8 samples is 0.17 ms, well
     above audible modulation rates.
   - Skip the envelope `exp2`s entirely when no matrix slot targets
     envelope times.
3. **Fast `exp2`:** a polynomial `exp2`, as was done for `tan`. Test it to
   within 1e-5 relative error across the range used.
4. **Hold accuracy:**
   - Existing tests must pass unchanged.
   - Add a test comparing a heavily modulated patch rendered with
     `MOD_INTERVAL = 1` against `8`. The difference must stay under −60 dB.
   - Tests that check per-sample behaviour (glide, LFO routes) get an
     interval of 1 through a test-only constructor if needed.

*Target:* the 16-voice benchmark under 4 % normal and 8 % HQ (from 6.5 %
and 13 %).

## Phase 3: only if still needed

- **SIMD across voices:** process 4 voices at once (`f32x4`) in the filter
  and oscillators. It's the biggest remaining win, but also the biggest
  refactor of `Voice`. Don't start it unless phases 1–2 leave a measured
  problem.
- **GUI:**
  - Cache the envelope and LFO display points, keyed on their settings,
    instead of rebuilding them every frame.
  - Repaint only on interaction or parameter change.
  - Only worth doing if Instruments shows the main thread competing with
    audio.

## Risks

- **Automation precision:** turning off sample-accurate automation moves a
  change made mid-buffer to the buffer's start. With 10–50 ms smoothers
  this isn't audible. Mid-buffer note events are unaffected.
- **Control rate:** fast LFOs at high depth (audio-rate FM through the
  matrix) lose some precision at 8-sample updates. If this is audible, LFO
  → pitch can stay per-sample when the LFO rate is above 50 Hz.
- **Change detection:** a missed change means a setting that silently
  doesn't apply. The equality check (step 1.2) is the safe default. Only
  replace it with flags if the benchmark says the comparison itself costs
  something.

## Order of work

| # | Step | Size |
|---|---|---|
| 0 | Benchmarks, clap-smoke `--automate`, meter | **Done** (Instruments run in LYNX still to do) |
| 1 | Sample-accurate off, skip unchanged patches, split `apply_patch`, batched presets | **Done** |
| — | **Check with the user in LYNX using the meter** | **Next** |
| 2 | Control-rate shared settings, 8-sample voice modulation, fast `exp2` | 2–3 sessions |
| 3 | SIMD voices, GUI caching: only if measurements call for it | — |
