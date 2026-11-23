#!/bin/sh
# Build openBus for macOS (the Mac it runs on: Apple silicon or Intel) and pack it as
# dist/macos/openBus.app. The game binary is the bundle's executable; started with no
# arguments it opens the launcher window. Needs Rust (https://rustup.rs) and the Xcode
# Command Line Tools (xcode-select --install).
set -eu
cd "$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
[ "$(uname -s)" = Darwin ] || { echo "Run this script on macOS." >&2; exit 1; }
xcode-select -p >/dev/null 2>&1 || { echo "Run xcode-select --install, then run this script again." >&2; exit 1; }
command -v cargo >/dev/null 2>&1 || { echo "Install Rust from https://rustup.rs, then run this script again." >&2; exit 1; }
target="${OPENBUS_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"
version="${OPENBUS_VERSION:-$(sh scripts/version.sh 2>/dev/null || echo 0.0.0)}"
export OPENBUS_VERSION="$version"
cargo build --locked --release --target "$target" -p omsi-app -p omsi-launcher-core
out=dist/macos
app="$out/openBus.app"
# (only the bundle is replaced: the folders beside it are the content folder with the mods)
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "target/$target/release/openbus" "$app/Contents/MacOS/openbus"
cp "target/$target/release/openbus-launcher" "$app/Contents/MacOS/openbus-launcher"
cp "assets/steam_redist/libsteam_api.dylib" "$app/Contents/MacOS/"
# the desktop launcher (launcher/, Tauri on WebKit), stamped with the release version (its
# About window and the Info.plist inside it said tauri.conf.json's)
TAURI_CONFIG="{\"version\":\"${version%%-*}\"}" cargo build --locked --release --target "$target" --manifest-path launcher/src-tauri/Cargo.toml
cp "launcher/src-tauri/target/$target/release/openbus-launcher-app" "$app/Contents/MacOS/openbus-launcher-app"
cp assets/icons/app/openbus.icns "$app/Contents/Resources/openbus.icns"
# (the bundle's version is numbers only: a test build's -pr<number> is left out there)
sed -e "s/@VERSION@/${version%%-*}/g" scripts/macos/Info.plist > "$app/Contents/Info.plist"
codesign --force --deep --sign - "$app" >/dev/null 2>&1 || true
printf '\nopenBus %s built. Play: open "%s/%s"\n' "$version" "$PWD" "$app"
