#!/usr/bin/env bash
# Build, test, bundle, and install a plugin from vst/<name> into the user's
# VST3 and CLAP plugin folders.
#
# Usage: ./build2lynx.sh <vst-dir-name>     e.g. ./build2lynx.sh dragon_24eq
set -euo pipefail

usage() {
    echo "Usage: $0 <vst-dir-name>" >&2
    echo "Available:" >&2
    ls -1 "$REPO_ROOT/vst" >&2
    exit 1
}

# Repo root via git, so the script works wherever it lives in the repo.
REPO_ROOT="$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"

[[ $# -eq 1 ]] || usage
VST_DIR="vst/$1"
[[ -f "$VST_DIR/Cargo.toml" ]] || { echo "error: $VST_DIR/Cargo.toml not found" >&2; usage; }

# Make cargo available even if the shell profile hasn't loaded rustup's env.
if ! command -v cargo >/dev/null 2>&1 && [[ -f "$HOME/.cargo/env" ]]; then
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env"
fi

# Cargo package name (first `name = "..."` in the manifest).
PACKAGE="$(sed -n 's/^name *= *"\(.*\)"/\1/p' "$VST_DIR/Cargo.toml" | head -n1)"

# Bundle display name: bundler.toml's [package] name if set, else the package name.
BUNDLE_NAME="$(awk -v pkg="[$PACKAGE]" '
    $0 == pkg { in_section = 1; next }
    /^\[/     { in_section = 0 }
    in_section && /^name *=/ { sub(/^name *= *"/, ""); sub(/".*$/, ""); print; exit }
' bundler.toml 2>/dev/null || true)"
BUNDLE_NAME="${BUNDLE_NAME:-$PACKAGE}"

echo "==> Testing workspace"
cargo test --workspace

echo "==> Bundling $PACKAGE (universal)"
cargo xtask bundle-universal "$PACKAGE" --release

echo "==> Smoke-testing the CLAP bundle in a headless host"
cargo run -q -p clap-smoke --release -- "target/bundled/$BUNDLE_NAME.clap"

# Optional per-plugin parameter checks: "<description> | <clap-smoke options>".
CHECKS="$VST_DIR/smoke-checks.txt"
if [[ -f "$CHECKS" ]]; then
    echo "==> Running parameter checks from $CHECKS"
    while IFS='|' read -r desc opts; do
        [[ -z "${desc// /}" || "$desc" == \#* ]] && continue
        eval "set -- $opts"
        if out="$(cargo run -q -p clap-smoke --release -- "target/bundled/$BUNDLE_NAME.clap" "$@" 2>&1)"; then
            echo "    ok    ${desc%"${desc##*[! ]}"}"
        else
            echo "    FAIL  $desc" >&2
            echo "$out" >&2
            exit 1
        fi
    done < "$CHECKS"
fi

install_bundle() {
    local ext="$1" dest="$2"
    local src="target/bundled/$BUNDLE_NAME.$ext"
    [[ -d "$src" ]] || { echo "error: expected bundle not found: $src" >&2; exit 1; }
    mkdir -p "$dest"
    # Remove the old bundle first; `cp -R` onto an existing dir would nest it.
    rm -rf "${dest:?}/$BUNDLE_NAME.$ext"
    cp -R "$src" "$dest/"
    echo "    installed $dest/$BUNDLE_NAME.$ext"
}

echo "==> Installing '$BUNDLE_NAME'"
install_bundle vst3 "$HOME/Library/Audio/Plug-Ins/VST3"
install_bundle clap "$HOME/Library/Audio/Plug-Ins/CLAP"

echo "Done. Rescan plugins in LYNX (Options > Scan for plugins)."
