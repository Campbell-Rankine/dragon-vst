# Dragon 24EQ graph editor: plan

Milestone 2 of the [roadmap](roadmap.md). It is designed so that the
analyzer (milestone 3) and dynamic EQ (milestone 4) can be added as layers
without rework.

**Done when:** Dragon 24EQ works as a daily static EQ from the graph alone.
You can see the combined response, drag bands, set Q with the scroll
wheel, create and delete bands, and keep the editor at 60 fps with all 24
bands at 96 dB/oct.

## Starting point

- **Editor:** egui, 900 × 280 px. It has band chips, a band panel and an
  output panel. Band selection is `EditorState::selected` and lives only in
  the GUI.
- **GUI ↔ audio:** the editor gets `Arc<Dragon24EqParams>` and nothing else.
  The GUI doesn't know the sample rate. Nothing else passes from audio to
  GUI.
- **Response API:**
  - `BandDesign::new(&BandSettings, sample_rate)` is pure, `Copy` and
    allocation-free, so the GUI can call it.
  - `BandDesign::magnitude_db(freq, sample_rate)` costs about 4 trig calls,
    a `sqrt` and a `log10` per section per frequency.
  - `analog_magnitude_db(&settings, freq)` rebuilds the prototypes on every
    call.

## Decisions

| Question | Decision | Why |
|---|---|---|
| Analog or designed response? | **Designed** (`BandDesign::magnitude_db`) | It's what you actually hear. Bells and shelves are matched, so they look the same as analog. Cuts are bilinear and do bend near Nyquist, and the graph should show that. This replaces the line in [dsp.md](dsp.md) saying the graph draws the analog response. |
| Sample rate in the GUI | `Arc<AtomicF32>` set in `initialize`, passed to `editor::create`; 48 kHz until the first `initialize` | It's the smallest audio → GUI channel, and the cache below is keyed on it |
| Which values the graph shows | Parameter **targets** (`.value()`), not smoothed values | The curve should follow the mouse at once |
| Cost control | One cached dB array per band, recomputed only when that band's `BandSettings`, the sample rate or the graph width changes | See [Cost](#cost) |
| Window size | **900 × 600**: graph on top (about 868 × 300), chips and panels below unchanged | Pro-Q-like proportions, with the existing panels kept for precise entry |
| Saved editor size | Rename the persist key `editor-state` → `editor-state-v2` | Otherwise projects saved at 900 × 280 reopen at that size with the graph squashed. A GUI size isn't a parameter ID, so renaming it is safe. |
| dB range | ±3 / ±6 / ±12 / ±30 dB, clicked through from a label in the graph's corner, default ±12. Saved with the editor state | Matches the ±30 dB gain range; finer ranges help subtle moves |

## Layout

```
DRAGON 24EQ                                                    ±12 dB ▾
┌──────────────────────────────────────────────────────────────────────┐
│ +12 ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄│
│        ①╲___                  ②                                      │
│   0 ┄┄┄┄┄┄┄┄╲━━━━━━━━━━━━━━━━╱╲━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓③┄┄┄┄┄│
│                                                               ┗━━━━━━│
│ −12 ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄│
│     20   50  100  200   500  1k   2k    5k   10k  20k                 │
└──────────────────────────────────────────────────────────────────────┘
[1][2][3][4][5] … [24]
┌ BAND 2 ───────────────────────────────────────────┐ ┌ OUT ─┐
│ [ON]  Freq  Gain  Q  │  Type  Slope  Stereo        │ │ Gain │
└───────────────────────────────────────────────────┘ └──────┘
```

- **x axis:** log frequency, 10 Hz to 30 kHz. Grid lines at 1–2–5 steps;
  labels at 20, 50, 100 … 20k. The region above Nyquist is shaded.
- **y axis:** dB at the chosen range. Grid every 3 / 6 dB depending on the
  range. The 0 dB line is a little brighter.
- **Drawing order** (later layers are only placeholders for now):
  1. Background and grid
  2. *Analyzer spectra (milestone 3)*
  3. Each band's curve: a faint fill from 0 dB in the band colour. The
     selected band is brighter.
  4. *Dynamic gain range (milestone 4)*
  5. Combined curve: 2 px, near-white
  6. Nodes

## Curves

### Cost

Worst case: 24 bands × 8 sections × about 430 points (one every 2 px) is
about 83k section evaluations, each with ~4 trig calls plus a `sqrt` and a
`log10`. That's a few milliseconds: fine once, but too much every frame.

So:

- **Per-band cache:** `BandCurve { key: (BandSettings, sample_rate, width), db: Vec<f32> }`.
  It's the GUI thread, so allocation is allowed, but the `Vec` is reused.
- **Each frame:** for each band, build `BandSettings` from targets (cheap)
  and compare it with the cached key. Recompute only on a mismatch.
  Dragging one band therefore costs one band's curve per frame.
- **Combined curve:** the sum of the cached arrays of bands that are on
  (dB adds because the bands are in series). That's 24 × 430 additions per
  frame, which is negligible.
- **Off bands:** no curve and no node, as in Pro-Q.

### Stereo placement

Bands placed Left, Right, Mid or Side don't affect every channel, so one
combined curve would be wrong.

- **Main curve:** Stereo bands only, plus all bands if every band is
  Stereo (the usual case, one curve).
- **Extra curves:** for each placement that some band uses (L, R, M, S),
  a thin dashed curve in a neutral tint: Stereo bands plus that
  placement's bands. A small legend in the corner names each.
- **Nodes** of placed bands carry a one-letter badge (L / R / M / S).

## Interaction

| Gesture | Effect | Parameter gestures |
|---|---|---|
| Click node | Select band (chips and panel follow) | — |
| Drag node | x → freq, y → gain (y only for types that use gain; others move along x only) | `begin` freq (+ gain) on drag start, `end` on drag stop |
| Shift + drag | 10× finer, as with knobs | same |
| Cmd + drag | Lock to the drag's main axis (frequency only or gain only) | same |
| Scroll on node or selected band | Q, log steps; disabled for 6 dB/oct cuts | `begin` on first scroll event, `end` after 300 ms without scrolling |
| Double-click empty graph | Switch on the first free band as a bell at that freq/gain and select it | one `begin`/`set`/`end` per param set |
| Double-click node | Switch the band off (matches the chips) | as above |
| Right-click node | Menu: Type, Slope, Stereo, Off | as above |
| Hover node | Tooltip: `Band 3 · Bell · 1.20 kHz · +4.5 dB · Q 1.4` | — |

- **Node position:** (freq, gain) for gain types. For cuts, notches, band
  pass and all pass, y is the 0 dB line, so the node sits where you'd grab
  it.
- **Hit testing:** the nearest node within 10 px wins; the selected band
  wins ties. Nodes are drawn in band order with the selected one last
  (on top).
- **Setting values:** convert the pointer to plain Hz / dB and call
  `setter.set_parameter(&band.freq, hz)`. The parameter's own skew handles
  normalisation, so there's no second mapping to keep in sync.
- **Bounds:** frequency clamps to the parameter range (10 Hz – 22 kHz) and
  gain to ±30 dB, even when the visible range is smaller. Dragging past the
  edge keeps changing the value.

## Code layout

| File | Contents | Tests |
|---|---|---|
| `crates/dragon-ui/src/plot.rs` (new) | `LogAxis` / `LinAxis`: value ↔ pixel mapping, grid steps and labels. Shared so Waave's filter display can reuse it. | Round trips, grid steps |
| `vst/dragon_24eq/src/graph.rs` (new) | Pure model: `BandCurve` cache, combined and per-placement sums, node positions, hit testing, pointer → (Hz, dB) | Matches `magnitude_db` per point; cache invalidates on any settings change and on sample rate or width; hit-test priority; nodes for every type |
| `vst/dragon_24eq/src/graph_view.rs` (new) | egui drawing and interaction; calls `graph.rs` | Exercised by hand, plus the model tests |
| `vst/dragon_24eq/src/editor.rs` | New layout; passes `selected` to the graph; dB range in `EditorState` | — |
| `vst/dragon_24eq/src/lib.rs` | `sample_rate: Arc<AtomicF32>` set in `initialize` | — |
| `vst/dragon_24eq/src/params.rs` | Persist key `editor-state-v2`, size 900 × 600 | — |

**Perf test:** an `#[ignore]` bench that recomputes all 24 bands at
96 dB/oct. Target under 8 ms in release on the development Mac, and
cached frames under 0.2 ms.

## Steps

1. **Static graph:** axes, grid, sample-rate sharing, per-band cache,
   combined curve, band fills. Read-only. *Check:* the curve matches what
   the panel says, and is identical at 44.1, 48 and 96 kHz below 10 kHz.
2. **Nodes:** draw, select and drag; selection synced with chips; gestures
   recorded as host automation. *Check:* one undo step per drag in LYNX.
3. **Rest of the gestures:** scroll for Q, double-click to create or
   switch off, right-click menu, hover tooltips, Shift / Cmd modifiers.
4. **Placement curves**, dB range switch, Nyquist shading, polish.
5. **Docs:** an editor section in [dragon_24eq.md](dragon_24eq.md), the
   graph line in [dsp.md](dsp.md), and the roadmap row marked done.

## Hooks for milestones 3 and 4

- **Analyzer (3):** the spectra go in layer 2 and use the same `LogAxis`.
  It needs a lock-free audio → GUI ring buffer. Add it as a separate
  `Arc` beside the sample rate. The editor already takes extra state by
  then, so no signature churn.
- **Dynamic EQ (4):**
  - The audio thread publishes each band's current dynamic gain in
    `[AtomicF32; 24]`. Only the latest value is needed, so there's no
    queue.
  - The graph draws, per dynamic band, a shaded area between the static
    curve and the curve at static gain + range, plus a live curve at the
    current gain.
  - Only dynamic bands are recomputed per frame, and only when their gain
    moved by more than 0.05 dB. That's at most a few bands' curves per
    frame.
  - **Range handle:** a second, smaller node above or below each dynamic
    band's node, dragged vertically to set the range (as in Pro-Q). This
    needs the milestone 4 parameters (threshold, range, attack, release),
    which are added then, with new IDs.

## Settled

- **Window size:** fixed at 900 × 600; not resizable.
- **Off-band nodes:** hidden, as in Pro-Q (the selected off band included).
  Switch a band on from its chip, or double-click the graph.
