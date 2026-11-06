# Roadmap

The long-term goal is a Pro-Q 4-style parametric and dynamic EQ: its
features, with our own name and visual design. Dragon 24EQ is the first
working step.

## Done

- **Milestone 0 (scaffold):** workspace, bundling, VST3 + CLAP, universal
  macOS builds, install script, headless smoke test.
- **3-band EQ:** SVF low shelf, bell and high shelf with tests, smoothed
  parameters, and a dark egui editor with coloured rotary dials.
- **Milestone 1 (full static EQ core):**
  - 24 bands with 9 filter types.
  - Cut slopes from 6 to 96 dB/oct.
  - Per-band Stereo / Left / Right / Mid / Side placement.
  - Matched (no-cramping) design for bells and shelves; bilinear for cuts.
  - Tests against analog prototypes (see [dsp.md](dsp.md#accuracy)).
  - Band chips and a band panel in the editor.
  - Audio checks run before every install.
  - **Not done:** Pro-Q's "flat tilt" (a whole-spectrum tilt with no centre
    frequency) and brickwall cuts. Both fit naturally with linear-phase
    mode (milestone 5).

## Next

| # | Milestone | Done when |
|---|---|---|
| 2 | **Graph editor:** log-frequency / dB graph, draggable band nodes, scroll for Q, a combined response curve built from each band's exact response | Usable as a daily static EQ |
| 3 | **Analyzer:** pre, post and sidechain spectra; lock-free ring buffer from audio to GUI; FFT on the GUI thread | 60 fps, no audio-thread contention |
| 4 | **Dynamic EQ:** per-band envelope follower, threshold, range, attack and release; external sidechain | No zipper noise under fast gain changes |
| 5 | **Linear-phase mode:** FIR built from the combined magnitude response, partitioned FFT convolution, latency reported to the host | Host delay compensation lines up |
| 6 | **Polish:** character/saturation modes (oversampled), auto-gain, presets, solo/audition, EQ match | — |
| 7 | **Stretch:** spectral dynamics (STFT), natural-phase mode, collision detection | — |

## Testing to add along the way

- **Validators:** `clap-validator` and `pluginval` in the build script or CI.
- ~~**Smoke test coverage:** non-default parameter settings~~ (done:
  `smoke-checks.txt`).
- **Null tests:** a bypassed band leaves the signal bit-identical.
