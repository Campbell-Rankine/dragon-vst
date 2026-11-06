# Dragon ThreeQ

A 3-band EQ: low shelf, mid bell and high shelf, plus output gain. Source:
[`vst/dragon_threeq/src/`](../vst/dragon_threeq/src/).

## Signal flow

Per channel (mono or stereo):

```
in ─▶ low shelf ─▶ mid bell ─▶ high shelf ─▶ × output gain ─▶ out
```

Each band is a [`dsp_core::Svf`](dsp.md). Every band at 0 dB is exactly
transparent; the smoke test relies on this.

## Parameters

| ID | Name | Range | Default | Smoothing |
|---|---|---|---|---|
| `low_freq` | Low Freq | 20 Hz – 1 kHz | 100 Hz | logarithmic, 20 ms |
| `low_gain` | Low Gain | ±18 dB | 0 dB | linear, 20 ms |
| `mid_freq` | Mid Freq | 100 Hz – 10 kHz | 1 kHz | logarithmic, 20 ms |
| `mid_gain` | Mid Gain | ±18 dB | 0 dB | linear, 20 ms |
| `mid_q` | Mid Q | 0.1 – 10 | 0.707 | logarithmic, 20 ms |
| `high_freq` | High Freq | 1 – 20 kHz | 8 kHz | logarithmic, 20 ms |
| `high_gain` | High Gain | ±18 dB | 0 dB | linear, 20 ms |
| `out_gain` | Output Gain | ±36 dB | 0 dB | logarithmic, 50 ms |

- The shelves use a fixed Q of 0.707 (`SHELF_Q`).
- Frequency and Q ranges are skewed so the middle of a dial sits musically,
  not at the arithmetic midpoint.
- **Do not change the IDs.** Hosts store them in projects and automation.
  Add new IDs for new parameters, and never reuse a removed one.

### Coefficient updates

`process()` pulls every smoothed value once per sample. A band's
coefficients are recomputed only when its (freq, gain, Q) changed since
last time (`DragonThreeq::update_band`). While a dial is still, that costs
one comparison per band. While it's moving, a band costs one `tan()` per
sample.

## Editor

Built with egui, 900 × 250 px. The size is saved with the plugin state.

| Panel | Colour | Dials |
|---|---|---|
| LOW | mint `#5CF2B0` | Freq, Gain |
| MID | cyan `#00E5FF` | Freq, Gain, Q |
| HIGH | red `#FF3B4E` | Freq, Gain |
| OUT | grey | Gain |

- **Background:** `#121418`. Panels are a lighter `#242830` with a 1.5 px
  peach (`#FFB38A`) border. The "DRAGON THREEQ" title is also peach.
- **Changing colours:** they all live in the `theme` module in `lib.rs`.

### Dials (`knob.rs`)

- **Drag** up or down to change a value. A full sweep is 200 px; hold
  **Shift** for ten times finer control.
- **Double-click** to reset to the default.
- **Gain dials** are bipolar: the coloured arc grows from 12 o'clock, so
  boost and cut read at a glance.
- **Automation:** each drag is wrapped in `begin_set_parameter` /
  `end_set_parameter`, so the host records one automation gesture per drag.

## Identity

| What | Value |
|---|---|
| Display name | Dragon ThreeQ (`NAME` and `bundler.toml`) |
| Cargo package | `dragon_threeq` |
| CLAP ID | `com.dragon-vst.dragon-threeq` |
| VST3 class ID | `DragonThreeQ0001` (exactly 16 bytes) |
| Bundles | `Dragon ThreeQ.vst3`, `Dragon ThreeQ.clap` |

Changing the CLAP or VST3 ID makes hosts treat the plugin as a different
plugin, and existing project inserts show as missing. The display name and
package name can change without breaking projects, but stale bundles under
the old name must be removed from the plugin folders by hand.
