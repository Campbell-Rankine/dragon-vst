# Dragon 24EQ

A 24-band parametric EQ. Source:
[`vst/dragon_24eq/src/`](../vst/dragon_24eq/src/).

| File | Role |
|---|---|
| `lib.rs` | Plugin: per-band runtime state, coefficient updates, stereo routing |
| `params.rs` | Parameters and their enums (type, slope, stereo placement) |
| `editor.rs` | egui editor: band chips, band panel, output panel |
| `knob.rs` | Rotary knob widget |
| `theme.rs` | Colours |
| `smoke-checks.txt` | Audio checks run by `build2lynx.sh` (see [tools.md](tools.md)) |

## Signal flow

```
in ─▶ band 1 ─▶ band 2 ─▶ … ─▶ band 24 ─▶ × output gain ─▶ out
```

- Bands that are off are skipped.
- Each band is a cascade of 1–8 filter sections from `dsp-core`
  (see [dsp.md](dsp.md)).

### Stereo placement

| Placement | Stereo input | Mono input |
|---|---|---|
| Stereo | both channels, independently | the channel |
| Left / Right | that channel only | the channel |
| Mid | M = (L+R)/2 is filtered, then L = M+S, R = M−S | the channel |
| Side | S = (L−R)/2 is filtered, then decoded the same way | nothing (a mono signal has no side) |

## Parameters

24 bands, each with the parameters below. IDs get a band suffix from
nih-plug's `#[nested(array)]`, so band 3's frequency is `freq_3`. Host
display names are "Band 3 Freq" and so on.

| ID | Name | Range | Default | Smoothing |
|---|---|---|---|---|
| `on_N` | Band N On | on / off | bands 1–3 on, rest off | — |
| `type_N` | Band N Type | Bell, Low Shelf, High Shelf, Low Cut, High Cut, Notch, Band Pass, Tilt Shelf, All Pass | see below | — |
| `freq_N` | Band N Freq | 10 Hz – 22 kHz | see below | logarithmic, 20 ms |
| `gain_N` | Band N Gain | ±30 dB | 0 dB | linear, 20 ms |
| `q_N` | Band N Q | 0.1 – 30 | 0.707 | logarithmic, 20 ms |
| `slope_N` | Band N Slope | 6, 12, 18, 24, 30, 36, 48, 72, 96 dB/oct | 12 dB/oct | — |
| `stereo_N` | Band N Stereo | Stereo, Left, Right, Mid, Side | Stereo | — |
| `out_gain` | Output Gain | ±36 dB | 0 dB | logarithmic, 50 ms |

**Defaults:**
- Bands 1–3 keep the original 3-band layout: low shelf at 100 Hz, bell at
  1 kHz, high shelf at 8 kHz, all at 0 dB.
- Bands 4–24 start off, as bells spread evenly on a log scale from 50 Hz to
  12 kHz, so a newly enabled band lands somewhere distinct.

**Which settings apply:**
- Gain applies to bells, shelves and tilt.
- Slope applies to cuts.
- Q applies to everything except a 6 dB/oct cut.

The editor greys out controls that don't apply.

**Do not change IDs** (parameter IDs or the enum `#[id]`s). Hosts store them in
projects and automation. Add new IDs for new parameters, and never reuse a removed one.
Milestone 1 replaced the 3-band IDs (`low_freq` and so on). Dragon ThreeQ now uses those IDs
and the original CLAP/VST3 IDs again, so projects saved with the 3-band version load into
Dragon ThreeQ (see [dragon_threeq.md](dragon_threeq.md)).

## Processing

- **Control blocks:** audio is processed in blocks of 32 samples
  (`CONTROL_BLOCK`). At the start of each block, every band's smoothers
  advance by the block length.
- **Redesigns:** a band is redesigned only if its settings changed, so
  bands at rest cost nothing extra.
- **Discrete changes:** changing a band's type, slope or placement, or
  switching it on, resets that band's filter state. New sections then never
  start from another design's memory.
- **No allocation:** all state lives in fixed arrays, so the audio thread
  never allocates.

**Cost:** the worst case is 24 bands × 96 dB/oct, which is 192 sections.
Processing 1 s of stereo audio at 48 kHz takes about 0.06 s of CPU on an
Apple Silicon Mac, including plugin load.

## Editor

egui, 900 × 280 px. The size is saved with the plugin state.

```
DRAGON 24EQ                               click a band to edit · double-click to switch on/off
[1][2][3][4][5] … [24]                                       ← band chips
┌ BAND 2 ─────────────────────────────────────────────────┐ ┌ OUT ─┐
│ [ON]  Freq  Gain  Q  │  Type  Slope  Stereo             │ │ Gain │
└─────────────────────────────────────────────────────────┘ └──────┘
```

**Band chips:**
- **Click** a chip to select its band. **Double-click** switches the band
  on or off.
- **When on,** a chip is lit in the band's colour. The selected chip has a
  peach outline.
- **Hover** for a summary of the band's settings.

**Band panel:** controls for the selected band.
- The ON/OFF switch, then Freq / Gain / Q in the band's colour (grey while
  the band is off).
- Type, Slope and Stereo in peach shades.
- Controls that don't apply to the current type are greyed out and disabled.

### Colours (`theme.rs`)

| Band type | Colour |
|---|---|
| Low Shelf / Low Cut | mint `#5CF2B0` / dark mint `#2FB97F` |
| Bell / Band Pass / All Pass | cyan `#00E5FF` / light cyan `#8AF3FF` / dark cyan `#00A3BA` |
| High Shelf / High Cut / Notch | red `#FF3B4E` / dark red `#C42838` / light red `#FF8A96` |
| Tilt Shelf | peach `#FFB38A` |

- **Knobs:** Type is peach `#FFB38A`, Slope light peach `#FFD6BF`, Stereo
  dark peach `#E08A5C`.
- **Background and panels:** background `#121418`; panels `#242830` with a
  1.5 px peach border; title in peach.

### Knobs (`knob.rs`)

- **Drag** up or down to change a value. A full sweep is 200 px; hold
  **Shift** for ten times finer control.
- **Double-click** to reset to the default.
- **Stepped parameters** (Type, Slope, Stereo): the knob keeps its own
  record of the drag position, so slow drags still step through them.
- **Automation:** each drag is wrapped in `begin_set_parameter` /
  `end_set_parameter`, so the host records one automation gesture per drag.
- **Disabled:** adding a knob with `ui.add_enabled(false, …)` greys it out.

## Identity

| What | Value |
|---|---|
| Display name | Dragon 24EQ (`NAME` and `bundler.toml`) |
| Cargo package | `dragon_24eq` |
| CLAP ID | `com.dragon-vst.dragon-24eq` |
| VST3 class ID | `Dragon24EQ000001` (exactly 16 bytes) |
| Bundles | `Dragon 24EQ.vst3`, `Dragon 24EQ.clap` |

Changing the CLAP or VST3 ID makes hosts treat the plugin as a different
plugin, and existing project inserts show as missing.
