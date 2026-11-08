# Tools

## `build2lynx.sh`: build and install a plugin

```sh
./build2lynx.sh <vst-dir-name>        # e.g. ./build2lynx.sh dragon_24eq
```

Runs from anywhere inside the repo. It finds the repo root with git. Each
step must pass before the next one runs:

1. **Test:** `cargo test --workspace`.
2. **Bundle:** `cargo xtask bundle-universal <package> --release` builds
   arm64 + x86_64 VST3 and CLAP bundles into `target/bundled/`.
3. **Smoke test:** runs [`clap-smoke`](#clap-smoke-headless-host-check) on
   the new `.clap` at default settings (effects: expects unity gain;
   instruments: plays a note and expects sound, then silence), then each
   check in `vst/<name>/smoke-checks.txt` if the file exists.
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
cargo run -p clap-smoke --release -- "target/bundled/Dragon 24EQ.clap" [options]
```

Effects and instruments are both supported. The plugin's `clap.audio-ports`
extension decides which: a plugin with no audio input is an instrument.

Source: [`tools/clap-smoke/src/main.rs`](../tools/clap-smoke/src/main.rs).
A minimal CLAP host with no GUI. For every plugin in the bundle it:

1. Loads the binary and calls `clap_entry.init`.
2. Gets the plugin factory and creates the plugin with a stub host.
3. Calls `init`, then resolves any `--set` options to parameter events
   through the `clap.params` extension.
4. Calls `activate` (48 kHz, 1–512 frames).
5. On a separate thread, as a real audio thread would: `start_processing`,
   then 100 blocks of 512 samples, then `stop_processing`. Parameter events
   go in the first block. Any non-finite output fails the run.
   - **Effects:** the input is a 0.5-amplitude sine (the same on both
     channels).
   - **Instruments:** there is no audio input. A note-on (A4 by default) goes
     in the first block and a note-off in block 50 (about 0.53 s). The plugin
     must have a `clap.note-ports` input.
6. Measures each channel's peak from block 20 (time for smoothing to settle)
   to the note-off, or to the end if there isn't one.
   - **Effects:** converts the peak to a dB change from the input and
     compares it with `--expect-db`.
   - **Instruments:** checks the peak in dBFS lies within `--min-db` ..
     `--max-db` (default -60 .. 0), so the note must be audible and must not
     clip. After the note-off, the last 10 blocks must be below -80 dBFS,
     which catches stuck notes.
7. Calls `deactivate`, saves the state through `clap.state` (checks it
   succeeds), and calls `destroy`.

| Option | Meaning |
|---|---|
| `--set "<name>=<text>"` | Set a parameter by its display name. The plugin parses the text, e.g. `"Band 2 Gain=12"`, `"Band 4 Type=Low Cut"`, `"Band 4 On=On"`, `"Band 4 Slope=48 dB/oct"`. Repeatable. |
| `--tone <Hz>` | Test tone frequency (default 1000) |
| `--expect-db <dB>` | Expected level change on both channels (default 0) |
| `--expect-left-db`, `--expect-right-db` | Per-channel overrides |
| `--tol <dB>` | Allowed error (default 0.05) |
| `--note <key>` | MIDI key to play, 0–127 (default 69 for instruments; effects get no note unless given) |
| `--velocity <0..1>` | Note-on velocity (default 1) |
| `--hold` | Never send note-off; skips the silence check |
| `--min-db`, `--max-db` | Bounds on the held output peak in dBFS (instrument defaults -60 and 0; effects only check them if given) |

`--expect-db` and the per-channel overrides are relative to the input, so
they're rejected for instruments. Use `--min-db` / `--max-db` instead.

It prints the measured levels, then `PASS` (exit 0) or `FAIL: <reason>`
(exit 1). Bad arguments exit with 2.

```sh
# A +12 dB bell measured at its centre:
cargo run -q -p clap-smoke --release -- "target/bundled/Dragon 24EQ.clap" \
    --set "Band 2 Gain=12" --expect-db 12

# Middle C on the synth must peak between -20 and 0 dBFS, then release:
cargo run -q -p clap-smoke --release -- "target/bundled/Dragon Waave.clap" \
    --note 60 --min-db -20
```

**Limitations:**
- **No editor:** it doesn't open the plugin window, so GUI bugs won't show.
- **One tone per run:** it measures the level of a single steady sine, not
  a full frequency response.
- **One note per run:** instruments are checked by peak level only, not
  pitch, timbre or polyphony.
- **CLAP only:** the VST3 build is the same code behind a different
  wrapper, but it isn't loaded here.

### Per-plugin checks: `vst/<name>/smoke-checks.txt`

`build2lynx.sh` runs each line of this file (if the file exists) through
`clap-smoke` before installing. Format:

```
# comment
<description> | <clap-smoke options>
bell +12 dB at its centre | --set "Band 2 Gain=12" --expect-db 12
```

- **Where expectations come from:** take expected values from the analog
  maths ([dsp.md](dsp.md)), not from the plugin's current output, so the
  checks catch regressions instead of locking them in.
- **Restriction:** descriptions can't contain `|`.
- **Dragon 24EQ's 13 checks** cover:
  - a bell at its centre
  - low-cut and high-cut corners and slopes
  - a band-pass
  - a tilt shelf
  - all five stereo placements
  - switching a band off
  - output gain
  - stacked 96 dB/oct high cuts near Nyquist (a regression check)
- **Dragon Waave's checks** cover the volume and velocity levels, and notes
  at both ends of the keyboard playing and releasing.

## `xtask`: bundler

`cargo xtask` is an alias (in `.cargo/config.toml`) for nih-plug's bundler:

```sh
cargo xtask bundle dragon_24eq --release             # native arch only
cargo xtask bundle-universal dragon_24eq --release   # arm64 + x86_64
```

The bundler builds the `cdylib`, wraps it as `.vst3` and `.clap` bundles,
and ad-hoc code-signs them. Universal builds need the x86_64 target:
`rustup target add x86_64-apple-darwin`.

## API docs

```sh
cargo doc --no-deps -p dsp-core -p dragon_24eq -p dragon_threeq -p clap-smoke --open
```

## Optional: external validators

Not wired in yet:

```sh
cargo install --git https://github.com/free-audio/clap-validator --locked clap-validator
clap-validator validate "target/bundled/Dragon 24EQ.clap"
```

`pluginval` (Tracktion) covers VST3 and is worth adding before sharing
builds.

## Toolchain

Rust is installed through rustup (stable) with the
`x86_64-apple-darwin` target for universal builds.
