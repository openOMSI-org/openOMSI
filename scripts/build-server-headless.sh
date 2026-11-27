#!/bin/sh
# Build the native renderer-free (experimental) server separately from the
# legacy openomsi --server release. Never replace the production server ZIP.
set -eu
repo="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$repo"
out="${1:-$repo/dist/server-headless-test}"
export PATH="$HOME/.cargo/bin:$PATH"
command -v cargo >/dev/null 2>&1 || {
    echo "Rust/Cargo is required (https://rustup.rs)." >&2
    exit 1
}
cargo build --locked --release -p omsi-server --bin omsi-server
mkdir -p "$out"
cp target/release/omsi-server "$out/omsi-server"
cp scripts/server-headless/server-test.cfg "$out/server-test.cfg"
cp scripts/server-headless/server-simulation.cfg "$out/server-simulation.cfg"
cp scripts/server-headless/run-test.sh "$out/run-test.sh"
cp scripts/server-headless/run-simulation.sh "$out/run-simulation.sh"
cp scripts/server-headless/apply-update.sh "$out/apply-update.sh"
cp docs/HEADLESS_SERVER_TEST.md "$out/README.md"
cp docs/DEDICATED_SERVER_TIMINGS.md "$out/TIMINGS.md"
cp scripts/server-headless/timings-report.py "$out/timings-report.py"
cp LICENSE "$out/LICENSE"
chmod +x "$out/omsi-server" "$out/run-test.sh" "$out/run-simulation.sh" "$out/apply-update.sh"
echo "Experimental native headless test package: $out"
