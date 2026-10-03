#!/usr/bin/env bash
# Sandboxed install of this checkout. Never touches the real HOME, config, or launcher.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
bash -n "$root/install.sh"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/home" "$scratch/config" "$scratch/data"
real_home="$HOME"
export CARGO_HOME="${CARGO_HOME:-$real_home/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$real_home/.rustup}"
export HOME="$scratch/home"
export XDG_CONFIG_HOME="$scratch/config"
export XDG_DATA_HOME="$scratch/data"
export EOE_PREFIX="$scratch/prefix"
export EOE_SOURCE_DIR="$root"
export EOE_NO_OMARCHY=1
export EOE_YES=1
bash "$root/install.sh"
test -x "$EOE_PREFIX/bin/ee"
"$EOE_PREFIX/bin/ee" --version
test -f "$XDG_CONFIG_HOME/eoe/config.toml"
test ! -e "$HOME/.local/share/applications/ee.desktop"
