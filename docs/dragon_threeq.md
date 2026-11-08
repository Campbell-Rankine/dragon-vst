# Dragon ThreeQ

A 3-band parametric EQ: Low, Mid and High bands plus output gain. Each band has
the same controls as a [Dragon 24EQ](dragon_24eq.md) band, and all three bands are
on screen at once. Source: [`vst/dragon_threeq/src/`](../vst/dragon_threeq/src/).

| File | Role |
|---|---|
| `lib.rs` | Plugin: per-band runtime state, coefficient updates, stereo routing |
| `params.rs` | Parameters and their enums (type, slope, stereo placement) |
| `editor.rs` | egui editor: three band panels, output panel |
| `theme.rs` | Band colours; the palette, knob and panels come from the shared [`dragon-ui`](../crates/dragon-ui) crate |
| `smoke-checks.txt` | Audio checks run by `build2lynx.sh` (see [tools.md](tools.md)) |

## Signal flow

```
in ─▶ low ─▶ mid ─▶ high ─▶ × output gain ─▶ out
```

- Bands that are off are skipped.
- Each band is a cascade of 1–8 filter sections from `dsp-core`
  (see [dsp.md](dsp.md)).
- Stereo placement (Stereo / Left / Right / Mid / Side) works the same way
  as in Dragon 24EQ. See [its table](dragon_24eq.md#stereo-placement).

## Parameters

Three bands, each with the parameters below. IDs get a `low_`, `mid_` or
`high_` prefix from nih-plug's `#[nested(id_prefix = ...)]`, so the mid band's
frequency is `mid_freq`. Host display names are "Mid Freq" and so on.

| ID | Name | Range | Default | Smoothing |
|---|---|---|---|---|
| `B_on` | B On | on / off | on | — |
| `B_type` | B Type | Bell, Low Shelf, High Shelf, Low Cut, High Cut, Notch, Band Pass, Tilt Shelf, All Pass | see below | — |
| `B_freq` | B Freq | 10 Hz – 22 kHz | see below | logarithmic, 20 ms |
| `B_gain` | B Gain | ±30 dB | 0 dB | linear, 20 ms |
| `B_q` | B Q | 0.1 – 30 | 0.707 | logarithmic, 20 ms |
| `B_slope` | B Slope | 6, 12, 18, 24, 30, 36, 48, 72, 96 dB/oct | 12 dB/oct | — |
| `B_stereo` | B Stereo | Stereo, Left, Right, Mid, Side | Stereo | — |
| `out_gain` | Output Gain | ±36 dB | 0 dB | logarithmic, 50 ms |

`B` is `low` / `mid` / `high` in IDs and `Low` / `Mid` / `High` in names.

**Defaults:** Low is a low shelf at 100 Hz, Mid a bell at 1 kHz, High a high
shelf at 8 kHz, all on and at 0 dB (transparent).

**Which settings apply:**
- Gain applies to bells, shelves and tilt.
- Slope applies to cuts.
- Q applies to everything except a 6 dB/oct cut.

The editor greys out controls that don't apply.

**Compatibility with the original ThreeQ:** the frequency and gain IDs
(`low_freq`, `low_gain`, `mid_freq`, `mid_gain`, `mid_q`, `high_freq`,
`high_gain`, `out_gain`), the CLAP ID and the VST3 class ID all match the
original 3-band Dragon ThreeQ. Projects saved with it load their settings
into this version. Parameters the original didn't have (on, type, slope,
stereo, and the shelves' Q) take their defaults, which match the original's
fixed behaviour. The ranges are wider than the original's, so every saved
value still fits. The shelves now use the matched design from `dsp-core`
instead of the original SVF shelves, so a saved shelf's response may differ
slightly.

**Do not change IDs** (parameter IDs or the enum `#[id]`s). Hosts store them in
projects and automation. Add new IDs for new parameters, and never reuse a removed one.

## Processing

Same as Dragon 24EQ (see [its Processing section](dragon_24eq.md#processing)):
32-sample control blocks, redesigns only when a band's settings change, a
filter-state reset on discrete changes, and no allocation on the audio thread.
The worst case is 3 bands × 96 dB/oct, which is 24 sections.

## Editor

egui, 760 × 540 px. The size is saved with the plugin state.

```
DRAGON THREEQ                       drag to change · shift for fine · double-click to reset
┌ LOW ──────────────────────────────────────────────┐ ┌ OUT ─┐
│ [ON]  Freq  Gain  Q  │  Type  Slope  Stereo       │ │ Gain │
└───────────────────────────────────────────────────┘ └──────┘
┌ MID ──────────────────────────────────────────────┐
│ [ON]  Freq  Gain  Q  │  Type  Slope  Stereo       │
└───────────────────────────────────────────────────┘
┌ HIGH ─────────────────────────────────────────────┐
│ [ON]  Freq  Gain  Q  │  Type  Slope  Stereo       │
└───────────────────────────────────────────────────┘
```

- **Panel title and Freq / Gain / Q** use the band's type colour, or grey
  while the band is off.
- **Type, Slope and Stereo** use peach shades.
- **Controls that don't apply** to the band's type are greyed out and disabled.
- **Colours and knob behaviour** are the same as Dragon 24EQ's. See
  [Colours](dragon_24eq.md#colours-themers) and [Knobs](dragon_24eq.md#knobs-knobrs).

## Identity

| What | Value |
|---|---|
| Display name | Dragon ThreeQ (`NAME` and `bundler.toml`) |
| Cargo package | `dragon_threeq` |
| CLAP ID | `com.dragon-vst.dragon-threeq` |
| VST3 class ID | `DragonThreeQ0001` (exactly 16 bytes) |
| Bundles | `Dragon ThreeQ.vst3`, `Dragon ThreeQ.clap` |

Changing the CLAP or VST3 ID makes hosts treat the plugin as a different
plugin, and existing project inserts show as missing.
