# DSP: `dsp-core`

Source: [`crates/dsp-core/src/`](../crates/dsp-core/src/). Nothing here depends
on nih-plug; everything is unit-tested with `cargo test -p dsp-core`.

| Module | Role |
|---|---|
| `eq.rs` | Bands: `FilterType`, `Slope`, `BandSettings` → `BandDesign` (coefficients) + `BandFilter` (per-channel state) |
| `matched.rs` | Turns analog prototypes into digital biquads: `matched` (Vicanek) and `bilinear` (pre-warped) |
| `biquad.rs` | Direct-form coefficients: magnitude response, inversion, stability check, conversion to SVF |
| `svf.rs` | The state-variable filter that runs every section |

## Pipeline

```
BandSettings ──▶ analog prototype sections ──▶ digital biquads ──▶ SVF coefficients ──▶ BandFilter::process
 (type, freq,     (normalised to the band       (matched, or        (exact conversion)     (one Svf state
  gain, Q, slope)  frequency, 1-8 sections)      bilinear for cuts)                          per section)
```

```rust
use dsp_core::{BandDesign, BandFilter, BandSettings, FilterType, Slope};

let settings = BandSettings { filter_type: FilterType::Bell, freq: 1_000.0, gain_db: 6.0, q: 1.0, slope: Slope::Db12 };
let design = BandDesign::new(&settings, sample_rate);   // do this when settings change
let mut left = BandFilter::default();                   // one per channel per band
let y = left.process(&design, x);                       // per sample
let db = design.magnitude_db(2_000.0, sample_rate as f64); // designed response, for graphs/tests
```

`analog_magnitude_db(&settings, freq)` gives the ideal analog response: the
target the tests compare against, and what the milestone 2 graph will draw.

## Filter types

With `A = 10^(gain_dB/40)` and `s` normalised to the band frequency:

| Type | Analog prototype | Design |
|---|---|---|
| Bell | (s² + (A/Q)s + 1) / (s² + s/(AQ) + 1) | matched |
| Low shelf | A·(s² + (√A/Q)s + A) / (A s² + (√A/Q)s + 1) | matched* |
| High shelf | A·(A s² + (√A/Q)s + 1) / (s² + (√A/Q)s + A) | matched* |
| Tilt shelf | high shelf ÷ A: lows −gain/2, highs +gain/2 | matched* |
| Notch | (s² + 1) / (s² + s/Q + 1) | matched |
| Band-pass | (s/Q) / (s² + s/Q + 1), unity peak | matched |
| All-pass | (s² − s/Q + 1) / (s² + s/Q + 1) | matched poles, mirrored numerator |
| Low / high cut | Butterworth cascade, see below | bilinear |

\* **Shelf inversion:** a shelf whose poles sit above its zeros (a high boost,
or a low cut) can have poles past Nyquist, which impulse invariance can't
represent. These shelves are designed as their reciprocal (poles and zeros
swapped) and the resulting biquad is inverted. The matched numerator is
minimum-phase, so the inverse is stable.

### Cut slopes

Order N = slope / 6 (6, 12, 18, 24, 30, 36, 48, 72, 96 dB/oct → N = 1 … 16).
Butterworth sections:

- Even N: N/2 sections with Q_k = 1 / (2 cos((2k−1)π / 2N)).
- Odd N: (N−1)/2 sections with Q_k = 1 / (2 cos(kπ / N)), plus one first-order section.
- The user Q scales the most resonant section by Q / 0.7071, so Q 0.707
  is plain Butterworth (−3.01 dB at the corner), and higher values add a peak.

The 6 dB/oct cut ignores Q.

## Matched design (`matched.rs`)

After Martin Vicanek, *Matched Second Order Digital Filters* (2016):

1. **Poles by impulse invariance.** For natural frequency ωp and damping ζ:
   a2 = e^(−2ζωp), and a1 = −2e^(−ζωp)·cos(ωp√(1−ζ²)), using cosh for ζ > 1.
2. **Numerator by magnitude matching.** The numerator is chosen so the
   digital magnitude equals the analog magnitude at DC, at Nyquist, and at the
   band frequency (capped at 0.9π, where the equations stay well-conditioned).

The result has no "cramping": a +12 dB, Q 1 bell at 16 kHz / 44.1 kHz hits
its peak exactly. The bilinear (RBJ) version of that bell is 8 dB off at
20 kHz, because it is forced to 0 dB at Nyquist.

### Why cuts use the bilinear transform

A 96 dB/oct cut is 8 sections. Each matched section near Nyquist carries a
small passband error, and across 8 sections they add up to about +8.5 dB at
5–15 kHz for a 20 kHz cut. The bilinear transform, pre-warped so the corner is
exact, keeps the Butterworth passband flat and never boosts. Its one
deviation is a sharper knee near Nyquist: less attenuation just below the
corner (up to 1.6 dB for 96 dB/oct at 18 kHz) and a deeper stopband.

## SVF conversion (`biquad.rs`)

Every section runs as a trapezoidal state-variable filter (Simper/Cytomic).
This form stays stable and click-free when coefficients change. The
conversion from a biquad is exact.

From the SVF's denominator `(1+gk+g²) + 2(g²−1)z⁻¹ + (1−gk+g²)z⁻²`:

```
g  = √((1+a1+a2)/(1−a1+a2))      D = 4/(1−a1+a2)      k = (1−a2)·D/(2g)
m0 = (b0−b1+b2)/(1−a1+a2)
m2 = (D·b1 − m0(2g²−2)) / (2g²)
m1 = D(b0−b2)/(2g) − m0·k
```

The SVF then runs `out = m0·x + m1·bandpass + m2·lowpass`. First-order
sections are the special case a2 = 0. All design maths is f64; the audio
runs in f32.

## Accuracy

Digital vs. analog, max error over 20 Hz–20 kHz (or 0.45·fs), tested at 44.1,
48 and 96 kHz:

| Case | Max error | Notes |
|---|---|---|
| Bells, shelves, tilt (most settings) | ≤ 0.3 dB | |
| Wide bell (Q 0.5) at 8 kHz | ≤ 0.5 dB | worst near 16 kHz |
| Bell +12 dB Q 1 at 16 kHz / 44.1 kHz | ≤ 1.5 dB | peak exact; bilinear would be 8 dB off |
| Band-pass Q 1 at 2 kHz | ≤ 1.0 dB | peak exact; error sits 15–20 dB down |
| Notch | ≤ 0.3 dB | compared where the response is above −30 dB |
| All-pass | 0 | flat by construction |
| Cuts, passband to corner | ≤ 0.2 dB | 0.4 dB (12 dB/oct at 10 kHz) to 1.7 dB (96 dB/oct at 18 kHz): sharper knee, never a boost |
| Cut corner | exact (−3.01 dB) | every slope |

## Tests

`cargo test -p dsp-core` runs 18 tests:

| Test | Checks |
|---|---|
| `digital_matches_analog_prototypes` | The table above, for 18 settings × 3 sample rates |
| `no_cramping_near_nyquist` | 16 kHz bell: exact peak, < 1 dB off at 20 kHz |
| `cuts_never_overshoot_near_nyquist` | No cut ever exceeds 0 dB (regression for the matched-cascade bump) |
| `cut_corner_is_exactly_3db_down` | Every slope, low and high cuts, three frequencies |
| `analog_butterworth_is_3db_down_at_corner` / `cut_slopes_have_the_right_steepness` | Section-Q maths and 6N dB/oct slopes |
| `zero_gain_is_transparent` | Gain-based types at 0 dB are identity |
| `notch_is_deep_at_centre` | Below −60 dB at the centre |
| `every_setting_is_stable_and_finite` | ~10,500 settings: poles inside the unit circle, impulse response stays finite |
| `processing_matches_design_for_a_sine` | SVF cascade output level matches `magnitude_db` |
| `svf_conversion_matches_direct_form` | SVF vs. direct-form output on noise, including first-order and very-low-frequency sections |
| `matches_exactly_at_the_three_points`, `allpass_is_flat`, `inversion_round_trips`, `identity_passes_through`, gain-helper tests | Building blocks |

## Gain helpers (`lib.rs`)

`db_to_gain` and `gain_to_db` convert with a −100 dB floor, so silence
maps to 0.0 and back to −100 dB instead of −∞.
