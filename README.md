# dragon-vst
---

Rust audio plugins (VST3 + CLAP) built with [nih-plug](https://github.com/robbert-vdh/nih-plug), targeting FL Studio on macOS.

VST List:

1. dragon_threeq: a 3-band EQ, growing toward a Pro-Q 4-style parametric/dynamic EQ (`vst/dragon_threeq`)

## Layout

```
crates/dsp-core/   host-independent DSP, unit-tested
vst/dragon_threeq/ the EQ plugin (nih-plug + egui)
xtask/             bundling (`cargo xtask ...`)
```

## Build

```sh
cargo test --workspace
cargo xtask bundle-universal dragon_threeq --release   # arm64 + x86_64
```

Bundles land in `target/bundled/`. Install for FL Studio:

```sh
cp -R "target/bundled/Dragon ThreeQ.vst3" ~/Library/Audio/Plug-Ins/VST3/
cp -R "target/bundled/Dragon ThreeQ.clap" ~/Library/Audio/Plug-Ins/CLAP/
```

Then rescan in FL Studio (Options → Manage plugins → Find plugins).

## Validate

```sh
cargo install --git https://github.com/free-audio/clap-validator --locked clap-validator
clap-validator validate "target/bundled/Dragon ThreeQ.clap"
```

## License

GPL-3.0-or-later (required by nih-plug's VST3 bindings).
