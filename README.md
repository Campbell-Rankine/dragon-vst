# dragon-vst
---

Rust audio plugins (VST3 + CLAP) built with [nih-plug](https://github.com/robbert-vdh/nih-plug), for macOS (tested in LYNX; any VST3 or CLAP host should work).

Full documentation: [docs/](docs/README.md).

VST List:

1. dragon_24eq: a 24-band parametric EQ (9 filter types, 6-96 dB/oct cuts, mid/side), growing toward a Pro-Q 4-style dynamic EQ (`vst/dragon_24eq`)
2. dragon_threeq: a 3-band EQ (Low, Mid, High) with the same per-band controls as Dragon 24EQ, all on one screen (`vst/dragon_threeq`)
3. dragon_waave: an 80s-style polyphonic synth (work in progress: MIDI plumbing and a placeholder sine voice so far) (`vst/dragon_waave`)

## Layout

```
crates/dsp-core/   host-independent DSP, unit-tested
vst/dragon_24eq/   the 24-band EQ plugin (nih-plug + egui)
vst/dragon_threeq/ the 3-band EQ plugin (nih-plug + egui)
vst/dragon_waave/  the 80s-style synth (nih-plug)
tools/clap-smoke/  headless CLAP host used as a pre-install check
xtask/             bundling (`cargo xtask ...`)
docs/              documentation
```

## Build and install

```sh
./build2lynx.sh dragon_24eq     # test, universal build, smoke test, install
./build2lynx.sh dragon_threeq   # same for Dragon ThreeQ
./build2lynx.sh dragon_waave    # same for Dragon Waave
```

This installs `<Display Name>.vst3` and `<Display Name>.clap` (e.g. `Dragon 24EQ`) into `~/Library/Audio/Plug-Ins/{VST3,CLAP}`. Then rescan plugins in your host. See [docs/tools.md](docs/tools.md) for each step, and for building without installing.

## Validate

```sh
cargo install --git https://github.com/free-audio/clap-validator --locked clap-validator
clap-validator validate "target/bundled/Dragon 24EQ.clap"
```

## License

GPL-3.0-or-later (required by nih-plug's VST3 bindings).
