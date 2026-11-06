# Architecture

## Workspace layout

```
dragon-vst/
├── Cargo.toml            workspace; pins nih-plug to one git commit
├── bundler.toml          display names for bundles (package -> "Dragon 24EQ")
├── build2lynx.sh         test + build + smoke-test + install (see tools.md)
├── .cargo/config.toml    `cargo xtask` alias
├── crates/
│   └── dsp-core/         host-independent DSP (filters), unit-tested
├── vst/
│   └── dragon_24eq/    the 24-band EQ plugin (nih-plug + egui)
├── tools/
│   └── clap-smoke/       headless CLAP host used as a pre-install check
├── xtask/                nih-plug's bundler (`cargo xtask bundle ...`)
└── docs/                 you are here
```

Each plugin lives in `vst/<name>/`, and the folder name must match the Cargo
package name. `build2lynx.sh` relies on that.

## How the pieces fit

```
             ┌──────────────────────────────┐
  host ────▶ │ vst/dragon_24eq            │
 (LYNX,      │  Plugin impl (nih-plug)      │──uses──▶ crates/dsp-core
  FL, ...)   │  ├─ process(): audio thread  │          (BandDesign, BandFilter)
             │  └─ editor(): egui GUI thread│
             └──────────────────────────────┘
                     ▲  shared via Arc<Params>
                     │
              parameters (atomics + smoothers)
```

- **`dsp-core`** has no dependency on nih-plug. Anything that can be tested
  without a host (filter design, later analyzers and FIR design) goes here.
  Other plugins can reuse it.
- **`vst/dragon_24eq`** is a thin layer that maps parameters to `dsp-core`
  types, runs them in `process()`, and draws the editor.
- **Formats:** nih-plug exports each plugin as both **VST3** and **CLAP**
  from the same code (`nih_export_vst3!` / `nih_export_clap!`).

## Threads and real-time rules

| Thread | Runs | Rules |
|---|---|---|
| Audio | `process()`, `reset()` | No allocation, no locks, no I/O, no panics |
| GUI | egui `update` closure | May allocate; changes params only through `ParamSetter` |
| Main | `initialize()`, editor creation | May allocate |

- **Allocations:** the workspace enables nih-plug's `assert_process_allocs`
  feature. A debug build aborts if `process()` allocates, so you find out
  immediately.
- **Sharing:** parameters are the only shared state. The GUI writes them
  through `ParamSetter`, which tells the host about the change. The audio
  thread advances band smoothers once per 32-sample block
  (`smoothed.next_step`), and output gain once per sample.

## Formats, licensing, platform

- **License:** GPL-3.0-or-later. nih-plug's VST3 bindings require it.
- **Platform:** macOS only for now. Bundles are universal (arm64 + x86_64)
  and ad-hoc signed by the bundler.
- **Install locations:** `~/Library/Audio/Plug-Ins/VST3` and `~/Library/Audio/Plug-Ins/CLAP`.
