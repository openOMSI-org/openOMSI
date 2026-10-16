#!/bin/sh
# Build openOMSI for Linux x86_64 into dist/linux (the game, which is also the launcher
# window, and the launcher's terminal tools). Needs Rust and, on Debian/Ubuntu:
#   sudo apt install build-essential pkg-config libasound2-dev libudev-dev libgtk-3-dev \
#     libxkbcommon-dev libwayland-dev libssl-dev libwebkit2gtk-4.1-dev
set -eu
cd "$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
command -v cargo >/dev/null 2>&1 || { echo "Install Rust from https://rustup.rs, then run this script again." >&2; exit 1; }
export OPENOMSI_VERSION="${OPENOMSI_VERSION:-$(sh scripts/version.sh 2>/dev/null || echo 0.0.0)}"
cargo build --release -p omsi-app -p omsi-launcher-core
out=dist/linux
mkdir -p "$out"   # (the folder is also the content folder: mods stay)
cp target/release/openomsi target/release/openomsi-launcher "$out/"
# the desktop launcher (launcher/, Tauri on WebKitGTK): a plain start of openomsi opens it (it
# needs WebKitGTK 4.1; where it is missing the game says so)
TAURI_CONFIG="{\"version\":\"${OPENOMSI_VERSION%%-*}\"}" cargo build --locked --release --manifest-path launcher/src-tauri/Cargo.toml
cp launcher/src-tauri/target/release/openomsi-launcher-app "$out/"
cp assets/icons/app/openomsi-256.png "$out/openomsi.png"
# (Steam's library, x86-64 only: an ARM build has no Steam, see crates/omsi-app/build.rs)
[ "$(uname -m)" = x86_64 ] && cp assets/steam_redist/libsteam_api.so "$out/"
cp scripts/linux/openomsi.desktop "$out/"
printf '\nopenOMSI %s built in %s\n' "$OPENOMSI_VERSION" "$out"
