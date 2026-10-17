#!/bin/sh
# Build the dedicated server (Linux, or any Unix) into dist/server - or into the folder
# given as the first argument. The server is the game binary started with --server; the
# folder gets start.sh, run-server.sh (which asks for the folder, the settings and the map)
# and, on the first start, server.cfg with its defaults.
set -eu
repo="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$repo"
dest="${1:-$repo/dist/server}"
export PATH="$HOME/.cargo/bin:$PATH"
command -v cargo >/dev/null || { echo "Install Rust (https://rustup.rs) first." >&2; exit 1; }
export OPENOMSI_VERSION="${OPENOMSI_VERSION:-$(sh scripts/version.sh 2>/dev/null || echo 0.0.0)}"
cargo build --locked --release -p omsi-app
mkdir -p "$dest"
cp target/release/openomsi "$dest/openomsi"
# (Steam's library beside it, x86-64 only: the program does not start without it there)
[ -f target/release/libsteam_api.so ] && cp target/release/libsteam_api.so "$dest/"
cp scripts/server/start.sh "$dest/start.sh"
cp scripts/server/run-server.sh "$dest/run-server.sh"
cp docs/SERVER.md "$dest/README.md"
chmod +x "$dest/start.sh" "$dest/run-server.sh" "$dest/openomsi"
touch "$dest/.openomsi-content"
echo "openOMSI server $OPENOMSI_VERSION in $dest. Start it: $dest/run-server.sh (it asks), or $dest/start.sh /path/to/OMSI2"
