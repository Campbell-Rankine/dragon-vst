# dragon-vst
---

Rust audio plugins (VST3 + CLAP) built with [nih-plug](https://github.com/robbert-vdh/nih-plug), targeting FL Studio on macOS.

VST List:

1. bad filter: a parametric/dynamic EQ inspired by FabFilter Pro-Q 4 (`vst/bad_filter`)

## Layout

```
crates/dsp-core/   host-independent DSP, unit-tested
vst/bad_filter/    the EQ plugin (nih-plug + egui)
xtask/             bundling (`cargo xtask ...`)
```

## Build

```sh
cargo test --workspace
cargo xtask bundle-universal bad_filter --release   # arm64 + x86_64
```

Bundles land in `target/bundled/`. Install for FL Studio:

```sh
cp -R "target/bundled/Bad Filter.vst3" ~/Library/Audio/Plug-Ins/VST3/
cp -R "target/bundled/Bad Filter.clap" ~/Library/Audio/Plug-Ins/CLAP/
```

Then rescan in FL Studio (Options → Manage plugins → Find plugins).

## Validate

```sh
cargo install --git https://github.com/free-audio/clap-validator --locked clap-validator
clap-validator validate "target/bundled/Bad Filter.clap"
```

## License

GPL-3.0-or-later (required by nih-plug's VST3 bindings).
