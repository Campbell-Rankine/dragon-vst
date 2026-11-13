# dragon-vst documentation

| Doc | What's in it |
|---|---|
| [architecture.md](architecture.md) | Workspace layout, how the crates fit together, real-time rules |
| [dragon_24eq.md](dragon_24eq.md) | The Dragon 24EQ plugin: parameters, signal flow, editor, IDs |
| [dragon_24eq_graph.md](dragon_24eq_graph.md) | Plan for the Dragon 24EQ graph editor (roadmap milestone 2) |
| [dragon_threeq.md](dragon_threeq.md) | The Dragon ThreeQ plugin: three bands, parameters, editor, IDs |
| [dragon_waave.md](dragon_waave.md) | The Dragon Waave synth: design, engine API, parameters, IDs, build plan and status |
| [dragon_waave_ui.md](dragon_waave_ui.md) | The Dragon Waave editor: layout, styling, displays, shared UI crate, tests |
| [dragon_waave_perf.md](dragon_waave_perf.md) | Plan to fix Dragon Waave dropouts while editing, and to lower its CPU cost |
| [dsp.md](dsp.md) | The filter maths in `dsp-core` and how it's tested |
| [tools.md](tools.md) | `build2lynx.sh`, `clap-smoke`, `xtask`, API docs |
| [troubleshooting.md](troubleshooting.md) | Plugin-loading problems and how to diagnose them |
| [roadmap.md](roadmap.md) | Milestones from the 3-band EQ to a Pro-Q 4-style EQ (the synth's plan is in [dragon_waave.md](dragon_waave.md#build-order)) |

## Quick start

```sh
./build2lynx.sh dragon_24eq     # test, build, smoke-test, install
./build2lynx.sh dragon_threeq   # same for Dragon ThreeQ
./build2lynx.sh dragon_waave    # same for Dragon Waave
```

Then rescan plugins in your host (LYNX: Options > Scan for plugins).

## API docs

Generated from the doc comments in the code:

```sh
cargo doc --no-deps -p dsp-core -p dragon_24eq -p dragon_threeq -p dragon_waave -p clap-smoke --open
```
