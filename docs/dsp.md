# DSP: `dsp-core`

Source: [`crates/dsp-core/src/`](../crates/dsp-core/src/).

## State-variable filter (`svf.rs`)

A trapezoidal-integrated state-variable filter, after Andrew Simper's
(Cytomic) "SvfLinearTrapOptimised2". We use it instead of textbook biquads
because it stays stable and click-free when coefficients change every
sample. That matters for automation now, and for dynamic EQ later.

### API

```rust
use dsp_core::{Svf, SvfCoefs, SvfShape};

let coefs = SvfCoefs::new(SvfShape::Bell, sample_rate, 1_000.0, 6.0, 1.0);
let mut state = Svf::default();       // one per channel per band
let y = state.process(&coefs, x);     // per sample
state.reset();                        // on transport reset
```

- `SvfCoefs` is the setting (shape, frequency, gain, Q). It's small and
  `Copy`, and can be shared across channels.
- `Svf` is the per-channel memory (two integrator states).
- `SvfCoefs::IDENTITY` passes audio through unchanged.

### Maths

With `A = 10^(gain_dB / 40)` and `g = tan(π · f / fs)`:

| Shape | g used | k | m0 | m1 | m2 |
|---|---|---|---|---|---|
| Bell | g | 1 / (Q·A) | 1 | k·(A² − 1) | 0 |
| Low shelf | g / √A | 1 / Q | 1 | k·(A − 1) | A² − 1 |
| High shelf | g · √A | 1 / Q | A² | k·(1 − A)·A | 1 − A² |

Then:

```
a1 = 1 / (1 + g(g + k)),  a2 = g·a1,  a3 = g·a2

per sample:
  v3 = v0 − ic2
  v1 = a1·ic1 + a2·v3          (band-pass)
  v2 = ic2 + a2·ic1 + a3·v3    (low-pass)
  ic1 = 2·v1 − ic1
  ic2 = 2·v2 − ic2
  out = m0·v0 + m1·v1 + m2·v2
```

- Frequency is clamped to [1 Hz, 0.49 · fs], and Q to at least 0.01.
- The `tan()` pre-warp puts the centre or corner frequency exactly where
  you ask. The curve still narrows ("cramps") slightly close to Nyquist;
  the roadmap covers fixing this.

### Tests

`cargo test -p dsp-core` feeds sines through each shape and measures the
steady-state gain:

| Test | Checks |
|---|---|
| `zero_gain_is_transparent` | All shapes at 0 dB pass 50 Hz, 1 kHz and 10 kHz unchanged (±0.05 dB) |
| `bell_boosts_at_centre_only` | +6 dB at the centre, ~0 dB far away |
| `bell_cut_is_symmetric` | −9 dB cut lands at −9 dB |
| `low_shelf_shapes_lows` | Full gain below, half gain (in dB) at the corner, 0 dB above |
| `high_shelf_shapes_highs` | The same, mirrored |

## Gain helpers (`lib.rs`)

`db_to_gain` and `gain_to_db` convert with a −100 dB floor, so silence
maps to 0.0 and back to −100 dB instead of −∞.
