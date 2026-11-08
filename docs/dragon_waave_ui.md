# Dragon Waave editor

The plan for Dragon Waave's custom editor (roadmap step 6), styled like
Dragon 24EQ. The sound engine and parameters are in
[dragon_waave.md](dragon_waave.md).

**Decisions:**
- **One page, with selector chips.** The envelopes, the LFOs and the
  matrix's selections switch what's shown, so nothing is hidden behind
  tabs.
- **A shared UI crate** for the widgets, used by all three plugins.
- **Envelope and LFO shape displays.**

## Status

| Step | What | State |
|---|---|---|
| 1 | Shared `dragon-ui` crate; switch both EQs to it (no visual change) | **Done** |
| 2 | Waave editor skeleton: every control in the layout | **Done** (see [Step 2 notes](#step-2-notes)) |
| 3 | 24EQ-style ENV / LFO chips, themed dropdowns, greying rules | **Done** (see [Step 3 notes](#step-3-notes)) |
| 4 | Envelope and LFO shape displays | **Done** (see [Step 4 notes](#step-4-notes)) |
| 5 | Parameter-coverage test, docs, install | Planned |

## Step 2 notes

What was built, and where it differs from the plan below:

- **Size:** the window is **1220 × 760** and knobs are **40 px**. That's a
  little wider and smaller than planned, so all four rows fit.
  - A headless test, `every_panel_fits_the_window`, draws the editor for
    every ENV and LFO selection and checks the content stays within the
    window and its 12 px margin. It currently fills it exactly:
    1208 × 748.
  - Row 3 has about 280 px of horizontal room left for the step 4 shape
    displays.
- **Matrix rows came forward from step 3:**
  - Six slots of four knobs didn't fit beside VOICE and FX / OUT, so the
    compact rows are already in.
  - Each row is `n [Source ▾] → [Dest ▾] [Amount slider] via [Via ▾]`.
  - The dropdowns are plain egui `ComboBox`es, and each selection is one
    automation gesture. Amount is nih-plug-egui's `ParamSlider`.
  - Theming both is step 3.
- **Placeholder selectors:** ENV (AMP / FILT / MOD) and LFO (1 / 2) use a
  simple column of text buttons. The 24EQ-style chips are step 3.
- **Pills:** HQ, Clip, OSC 2 Sync and LFO Sync already use the shared
  ON/OFF pills.
- **Headless drawing:** `editor.rs`'s tests can draw the editor without a
  host, through a no-op `GuiContext`. Step 5's parameter-coverage test
  builds on it.

## Step 3 notes

- **Chips:**
  - ENV (AMP / FILT / MOD) and LFO (1 / 2) chips sit in their panel's title
    row, drawn like the 24EQ's band chips: filled and lettered in the
    section's colour when **in use**, outlined in peach when selected.
    Hover for the name and whether it's routed.
  - "In use" means it affects the sound:

    | Chip | In use when |
    |---|---|
    | AMP | Always |
    | FILT | Filter Env Amount isn't 0 with the filter on, or an active matrix slot uses it |
    | MOD | An active matrix slot uses it |
    | LFO 1 | Any of its Pitch / PWM / Cutoff routes isn't 0, or the matrix uses it |
    | LFO 2 | The matrix uses it |

    An active slot has a source, a destination and a non-zero amount.
    Using a source as the via counts too.
  - This logic is a set of pure functions (`envs_used`, `lfos_used`,
    `matrix_uses`) with unit tests, since nih-plug has no public way to set
    a parameter outside a host.
- **Themed egui widgets:** `dragon_ui::theme::apply_visuals` restyles egui's
  own widgets for Waave: the matrix dropdowns, their popups and the amount
  sliders. They get dark fills, grey outlines that turn peach on hover, and
  a peach selection.
- **Greying:** as in the table under [Look and feel](#look-and-feel-same-as-dragon-24eq),
  plus two extensions:
  - With the filter Off, Env Amt, Key Trk and Velocity grey out along with
    Cutoff and Resonance, since they only move the cutoff.
  - LFO Phase greys out in Poly mode, where each note starts at a random
    phase.
- **Grid:** the panels form a clean grid. Your first look in LYNX showed
  ragged row ends, and VOICE and FX / OUT taller than MOD MATRIX.

  | Rule | Detail |
  |---|---|
  | Full width | Every row spans the full 1196 px content width |
  | Column split | Every row splits at 573 px (widened from 541 in step 4): OSC 1 / MIXER / ENV / MOD MATRIX on the left; OSC 2 / FILTER / LFO and VOICE (380) + FX / OUT (227) on the right |
  | Row heights | Panels in a row share a height: 129, 129, 213, 213 px |
  | Gaps | 8 px between panels, across and down (knobs inside keep 2 px) |

  - The sizes are in `editor.rs` (`LEFT_COLUMN`, `ROW_HEIGHTS`,
    `VOICE_WIDTH`). Each panel goes through `dragon_ui::widgets::panel_sized`,
    which takes an outer size and returns the panel's rect.
  - The `panels_form_a_clean_grid` test checks the edges, heights and
    column split for every ENV / LFO selection.
  - The extra width leaves room for the step 4 displays: about 120 px in
    ENV and 225 px in LFO.
- **`dragon-ui` additions:** `chip`, `panel_with_header` (a panel with chips
  after the title), `panel_sized` and `theme::apply_visuals`. The panel
  functions now return their response. The EQs use the same
  `panel`, so their look is unchanged.
- **Formatting:** `editor.rs`, `params.rs` and `lib.rs` in `dragon_waave`
  were run through rustfmt at 120 columns. Formatting only; the repo has
  no rustfmt config.

## Step 4 notes

- **Placement:** each display sits to the left of its panel's knobs, at
  the height of the two knob rows. The envelope display is 140 × 164 px
  and the LFO display 170 × 164 px.
  - To make room, the left column grew from 541 to 573 px. FILTER, the
    tightest panel on the right, still fits exactly at 615 px.
  - VOICE is now 380 px and FX / OUT 227 px.
- **Envelope display:**
  - Draws the selected envelope's outline with the engine's
    `attack_shape` and `fall_shape`, including both curve settings.
  - Stage widths follow √time, and sustain gets a fixed 20 % of the width.
  - Faint vertical ticks mark the stage boundaries.
  - Velocity amounts aren't shown: the outline is the full-velocity shape.
- **LFO display:**
  - Draws **two cycles** from the LFO's Phase, using the engine's `Lfo`
    with a fixed seed. The plan said one, but one cycle of Sample & Hold
    is a flat line.
  - A faint centre line and a tick between the cycles help read it.
  - It shows the shape only, not Rate, Delay or Fade In.
- **Dimming:** a display dims when its envelope or LFO isn't in use (the
  same rule that lights the chips).
- **Tests (`displays.rs`):** the point calculations are pure functions with
  unit tests:
  - Envelope outlines run left to right from 0 back to 0, peak at full
    level and sit at sustain.
  - Stage widths follow √time: a 4× longer stage is 2× wider.
  - LFO outlines match the engine: a sine from 90° starts at its peak, all
    shapes stay within ±1, and random shapes are identical every frame.
  - The grid and fit tests confirm the displays fit without stretching
    any panel.

## Look and feel (same as Dragon 24EQ)

- **Base styling:** the same as the 24EQ.

  | Element | Style |
  |---|---|
  | Background | `#121418` |
  | Panels | `#242830` with a 1.5 px peach border and a peach title |
  | Header | "DRAGON WAAVE" in peach, a dim hint line on the right |

- **Knobs:** the same knob widget.
  - A 270° arc. Drag up or down to change, hold Shift for fine control,
    double-click to reset.
  - Each drag is one host automation gesture.
  - 48 px instead of 64 px, so everything fits on one page.
- **Colours:** continuous knobs use their section's colour. Discrete
  choices (wave, type, mode) use peach shades, as the 24EQ's Type, Slope
  and Stereo knobs do.

  | Section | Colour |
  |---|---|
  | Oscillators and mixer | mint |
  | Filter and HPF | cyan |
  | Envelopes | red (light / normal / dark for Amp / Filter / Mod) |
  | LFOs and matrix | peach |
  | Voice, FX and output | grey |

- **Greyed-out controls:** controls that do nothing in the current setup
  are greyed out and disabled, as in the 24EQ:

  | Control | Greyed when |
  |---|---|
  | Pulse width | The wave isn't Pulse |
  | Bass Comp | The filter isn't LP24 |
  | Cutoff and resonance | The filter is Off |
  | LFO Rate | Sync is on |
  | LFO Division | Sync is off |
  | Detune and Spread | Unison is 1 |
  | Glide Mode | Glide is 0 |
  | Retrigger | Voice Mode is Poly |
  | A matrix slot's Amount and Via | Its Source or Destination is None |

## Layout

About 1180 × 760 px, fixed. The size is saved with the project, as in the
EQs.

```
DRAGON WAAVE                                    [HQ] [CLIP]   drag · shift fine · dbl-click reset
┌OSC 1──────────────────────────┐┌OSC 2 ──────────────────────[SYNC]┐
│ Wave Oct Semi Fine PW Level Ph ││ Wave Oct Semi Fine PW Level Ph XMod│
└────────────────────────────────┘└───────────────────────────────────┘
┌MIXER────────────────────────┐┌FILTER───────────────────────────────────────┐
│ SubWave SubOct Sub Noise Col Ring││ Type Cutoff Res Drive Env Key Vel Bass HPF│
└─────────────────────────────┘└──────────────────────────────────────────────┘
┌ENV [AMP][FILT][MOD]──────────────────┐┌LFO [1][2]─────────────────────────────┐
│ ╱‾‾╲___╲   D  A  H  D  S  R          ││ ∿∿ shape  Shape Rate/Div [SYNC] Phase   │
│ (shape)    AtkCurve DecCurve Vel V>A ││          Delay Fade Mode │→Pitch PWM Cut│
└──────────────────────────────────────┘└─────────────────────────────────────────┘
┌MOD MATRIX───────────────────────────┐┌VOICE──────────────────┐┌FX / OUT────────┐
│ 1 [LFO 1 ▾] → [Pitch ▾]  ◔  via[Wheel▾]││ Mode Uni Det Spr Poly ││ Chorus Rate Dpt │
│ 2 … (all six rows visible)           ││ Glide GMode Retr Drift Bend││ Mix Tone Volume│
└──────────────────────────────────────┘└────────────────────────┘└────────────────┘
```

- **ENV chips** (AMP / FILT / MOD):
  - Choose which envelope the panel shows, like the 24EQ's band chips.
  - A chip is lit when its envelope is in use. Amp always is; Filter is
    when Filter Env Amount isn't 0; Mod is when a matrix slot uses it.
- **LFO chips** (1 / 2):
  - Work the same way.
  - LFO 1's fixed routes (→ Pitch / PWM / Cutoff) appear only while LFO 1
    is selected.
- **Mod matrix:**
  - All six rows are always visible.
  - Source, Destination and Via are dropdowns, because 13–22 options is
    too many to step through by dragging.
  - Amount is a mini 28 px bipolar knob.
- **Header:** HQ and Output Clip are ON/OFF pills, like the 24EQ's band
  switch.

## Displays

- **Envelope:** the selected envelope's delay, attack, hold, decay,
  sustain and release.
  - It's drawn with the engine's own `attack_shape` and `fall_shape`, so
    it can't disagree with the sound.
  - Each stage's width follows √time, so a 5 ms attack stays visible next
    to a 10 s release. Sustain gets a fixed width.
  - Faint ticks mark the stage boundaries.
- **LFO:** one cycle of the selected shape, starting at its Phase.
  - It's drawn with the engine's `Lfo`.
  - Random shapes use a fixed seed, so the picture doesn't flicker.
- **Not planned:** a live playhead, which would need an audio-to-GUI
  channel. The displays show the shape, not the motion.

## Code structure

- **`crates/dragon-ui`** (new, shared by all three plugins):

  | File | Contents |
  |---|---|
  | `knob.rs` | The knob, moved as is, plus a `.diameter()` builder for 48 px and 28 px knobs |
  | `theme.rs` | The palette only. The EQ-specific `band_color` stays in each EQ |
  | `widgets.rs` | `panel`, `on_toggle` (and a labelled pill variant), the selector `chip`, `set_bool`, and a themed `enum_combo` |

- **Dragon 24EQ and Dragon ThreeQ** switch to it with no visual change.
  Their duplicate `knob.rs` and palette are deleted.
- **`vst/dragon_waave`:**
  - `editor.rs`: the layout.
  - `displays.rs`: the envelope and LFO drawing.
  - A persisted `editor_state` parameter, as the EQs have.
  - `fn editor()` on the plugin.

## Testing

- **Every parameter is reachable (automated):**
  - A test draws one editor frame headlessly, with a stub `GuiContext`
    that implements nih-plug's GUI-context trait as no-ops.
  - Each widget records the parameter it controls, and the test compares
    those against the plugin's parameter map.
  - It runs for every ENV and LFO chip selection, so a forgotten knob
    fails the test. It also catches layout panics.
- **The EQs are unchanged:** their smoke checks still pass. Their look
  needs checking by eye in LYNX.
- **Manual checklist in LYNX:**
  - Every chip switches its panel.
  - Dropdowns set their parameters, and host automation records them.
  - Greyed controls follow the rules above.
  - The window size is restored on reopen.
  - The displays follow their knobs.
