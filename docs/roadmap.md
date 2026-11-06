# Roadmap

The long-term goal is a Pro-Q 4-style parametric and dynamic EQ: its
features, with our own name and visual design. Dragon ThreeQ is the first
working step.

## Done

- **Milestone 0 (scaffold):** workspace, bundling, VST3 + CLAP, universal
  macOS builds, install script, headless smoke test.
- **3-band EQ:** SVF low shelf, bell and high shelf with tests, smoothed
  parameters, and a dark egui editor with coloured rotary dials.

## Next

| # | Milestone | Done when |
|---|---|---|
| 1 | **Full static EQ core:** 24 bands (`#[nested(array)]`), all shapes (bell, shelves, cuts, notch, band-pass, tilt, all-pass), 6–96 dB/oct cut slopes, per-band stereo / left / right / mid / side, high-frequency cramping fix (Vicanek matched filters) | Unit tests match analog prototype responses within tolerance |
| 2 | **Graph editor:** log-frequency / dB graph, draggable band nodes, scroll for Q, a combined response curve built from each band's exact response | Usable as a daily static EQ |
| 3 | **Analyzer:** pre, post and sidechain spectra; lock-free ring buffer from audio to GUI; FFT on the GUI thread | 60 fps, no audio-thread contention |
| 4 | **Dynamic EQ:** per-band envelope follower, threshold, range, attack and release; external sidechain | No zipper noise under fast gain changes |
| 5 | **Linear-phase mode:** FIR built from the combined magnitude response, partitioned FFT convolution, latency reported to the host | Host delay compensation lines up |
| 6 | **Polish:** character/saturation modes (oversampled), auto-gain, presets, solo/audition, EQ match | — |
| 7 | **Stretch:** spectral dynamics (STFT), natural-phase mode, collision detection | — |

## Testing to add along the way

- **Validators:** `clap-validator` and `pluginval` in the build script or CI.
- **Smoke test coverage:** non-default parameter settings, so the bands
  are checked changing the sound and not just passing it through.
- **Null tests:** a bypassed band leaves the signal bit-identical.
