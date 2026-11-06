# dragon-vst
---

Rust audio plugins (VST3 + CLAP) built with [nih-plug](https://github.com/robbert-vdh/nih-plug), for macOS (tested in LYNX; any VST3 or CLAP host should work).

Full documentation: [docs/](docs/README.md).

VST List:

1. dragon_threeq: a 3-band EQ, growing toward a Pro-Q 4-style parametric/dynamic EQ (`vst/dragon_threeq`)

## Layout

```
crates/dsp-core/   host-independent DSP, unit-tested
vst/dragon_threeq/ the EQ plugin (nih-plug + egui)
tools/clap-smoke/  headless CLAP host used as a pre-install check
xtask/             bundling (`cargo xtask ...`)
docs/              documentation
```

## Build and install

```sh
./build2lynx.sh dragon_threeq   # test, universal build, smoke test, install
```

This installs `Dragon ThreeQ.vst3` and `Dragon ThreeQ.clap` into `~/Library/Audio/Plug-Ins/{VST3,CLAP}`. Then rescan plugins in your host. See [docs/tools.md](docs/tools.md) for each step, and for building without installing.

## Validate

```sh
cargo install --git https://github.com/free-audio/clap-validator --locked clap-validator
clap-validator validate "target/bundled/Dragon ThreeQ.clap"
```

## License

GPL-3.0-or-later (required by nih-plug's VST3 bindings).
