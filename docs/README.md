# dragon-vst documentation

| Doc | What's in it |
|---|---|
| [architecture.md](architecture.md) | Workspace layout, how the crates fit together, real-time rules |
| [dragon_24eq.md](dragon_24eq.md) | The Dragon 24EQ plugin: parameters, signal flow, editor, IDs |
| [dragon_threeq.md](dragon_threeq.md) | The Dragon ThreeQ plugin: three bands, parameters, editor, IDs |
| [dsp.md](dsp.md) | The filter maths in `dsp-core` and how it's tested |
| [tools.md](tools.md) | `build2lynx.sh`, `clap-smoke`, `xtask`, API docs |
| [troubleshooting.md](troubleshooting.md) | Plugin-loading problems and how to diagnose them |
| [roadmap.md](roadmap.md) | Milestones from the 3-band EQ to a Pro-Q 4-style EQ |

## Quick start

```sh
./build2lynx.sh dragon_24eq     # test, build, smoke-test, install
./build2lynx.sh dragon_threeq   # same for Dragon ThreeQ
```

Then rescan plugins in your host (LYNX: Options > Scan for plugins).

## API docs

Generated from the doc comments in the code:

```sh
cargo doc --no-deps -p dsp-core -p dragon_24eq -p dragon_threeq -p clap-smoke --open
```
