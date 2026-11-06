# Tools

## `build2lynx.sh`: build and install a plugin

```sh
./build2lynx.sh <vst-dir-name>        # e.g. ./build2lynx.sh dragon_threeq
```

Runs from anywhere inside the repo. It finds the repo root with git. Each
step must pass before the next one runs:

1. **Test:** `cargo test --workspace`.
2. **Bundle:** `cargo xtask bundle-universal <package> --release` builds
   arm64 + x86_64 VST3 and CLAP bundles into `target/bundled/`.
3. **Smoke test:** runs [`clap-smoke`](#clap-smoke-headless-host-check) on
   the new `.clap`.
4. **Install:** replaces `~/Library/Audio/Plug-Ins/VST3/<Name>.vst3` and
   `~/Library/Audio/Plug-Ins/CLAP/<Name>.clap`. The old bundle is deleted
   first, because `cp -R` onto an existing bundle would nest the new copy
   inside the old one.

**How it finds names:**
- `<vst-dir-name>` must be a folder in `vst/` containing a `Cargo.toml`.
  The package name is read from that manifest.
- The bundle file name comes from the package's `[<package>] name = "..."`
  entry in `bundler.toml`, or falls back to the package name.

**Other details:**
- If `cargo` isn't on `PATH`, the script loads `~/.cargo/env`.
- Run with no argument (or a wrong name) to print usage and the available
  plugins.

After installing, rescan plugins in the host. If a plugin window is already
open, close and reopen it; see [troubleshooting.md](troubleshooting.md).

## `clap-smoke`: headless host check

```sh
cargo run -p clap-smoke --release -- "target/bundled/Dragon ThreeQ.clap"
```

Source: [`tools/clap-smoke/src/main.rs`](../tools/clap-smoke/src/main.rs).
A minimal CLAP host with no GUI. For every plugin in the bundle it:

1. Loads the binary and calls `clap_entry.init`.
2. Gets the plugin factory and creates the plugin with a stub host.
3. Calls `init` and `activate` (48 kHz, 1–512 frames).
4. On a separate thread, as a real audio thread would: `start_processing`,
   then 100 blocks of a 0.5-amplitude stereo 1 kHz sine, then
   `stop_processing`.
5. Checks that the output peak is 0.5 ± 0.001, which means unity-gain
   passthrough at default settings.
6. Calls `deactivate`, saves the state through the `clap.state` extension
   (checks it succeeds), and calls `destroy`.

It prints `PASS` and exits with 0, or prints `FAIL: <reason>` and exits
with 1.

**Limitations:**
- **No editor:** it doesn't open the plugin window, so GUI bugs won't show.
- **Default settings only:** the passthrough check assumes every default
  is flat (0 dB). If a plugin ships with a non-flat default, update the
  check in `run()`.
- **CLAP only:** the VST3 build is the same code behind a different
  wrapper, but it isn't loaded here.

## `xtask`: bundler

`cargo xtask` is an alias (in `.cargo/config.toml`) for nih-plug's bundler:

```sh
cargo xtask bundle dragon_threeq --release             # native arch only
cargo xtask bundle-universal dragon_threeq --release   # arm64 + x86_64
```

The bundler builds the `cdylib`, wraps it as `.vst3` and `.clap` bundles,
and ad-hoc code-signs them. Universal builds need the x86_64 target:
`rustup target add x86_64-apple-darwin`.

## API docs

```sh
cargo doc --no-deps -p dsp-core -p dragon_threeq -p clap-smoke --open
```

## Optional: external validators

Not wired in yet:

```sh
cargo install --git https://github.com/free-audio/clap-validator --locked clap-validator
clap-validator validate "target/bundled/Dragon ThreeQ.clap"
```

`pluginval` (Tracktion) covers VST3 and is worth adding before sharing
builds.

## Toolchain

Rust is installed through rustup (stable) with the
`x86_64-apple-darwin` target for universal builds.
