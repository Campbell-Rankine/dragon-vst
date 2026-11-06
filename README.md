# dragon-vst
---

Rust audio plugins (VST3 + CLAP) built with [nih-plug](https://github.com/robbert-vdh/nih-plug), for macOS (tested in LYNX; any VST3 or CLAP host should work).

Full documentation: [docs/](docs/README.md).

VST List:

1. dragon_24eq: a 24-band parametric EQ (9 filter types, 6-96 dB/oct cuts, mid/side), growing toward a Pro-Q 4-style dynamic EQ (`vst/dragon_24eq`)

## Layout

```
crates/dsp-core/   host-independent DSP, unit-tested
vst/dragon_24eq/ the EQ plugin (nih-plug + egui)
tools/clap-smoke/  headless CLAP host used as a pre-install check
xtask/             bundling (`cargo xtask ...`)
docs/              documentation
```

## Build and install

```sh
./build2lynx.sh dragon_24eq   # test, universal build, smoke test, install
```

This installs `Dragon 24EQ.vst3` and `Dragon 24EQ.clap` into `~/Library/Audio/Plug-Ins/{VST3,CLAP}`. Then rescan plugins in your host. See [docs/tools.md](docs/tools.md) for each step, and for building without installing.

## Validate

```sh
cargo install --git https://github.com/free-audio/clap-validator --locked clap-validator
clap-validator validate "target/bundled/Dragon 24EQ.clap"
```

## License

GPL-3.0-or-later (required by nih-plug's VST3 bindings).
