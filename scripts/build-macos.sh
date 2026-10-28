#!/bin/sh
# Build openOMSI for macOS (the Mac it runs on: Apple silicon or Intel) and pack it as
# dist/macos/openOMSI.app. The game binary is the bundle's executable; started with no
# arguments it opens the launcher window. Needs Rust (https://rustup.rs) and the Xcode
# Command Line Tools; the Intel ANGLE/CGL build also needs the full Xcode app.
set -eu
cd "$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
[ "$(uname -s)" = Darwin ] || { echo "Run this script on macOS." >&2; exit 1; }
xcode-select -p >/dev/null 2>&1 || { echo "Run xcode-select --install, then run this script again." >&2; exit 1; }
command -v cargo >/dev/null 2>&1 || { echo "Install Rust from https://rustup.rs, then run this script again." >&2; exit 1; }
target="${OPENOMSI_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"
if [ "$target" = x86_64-apple-darwin ] && ! xcodebuild -version >/dev/null 2>&1; then
    echo "The Intel ANGLE/CGL build needs the full Xcode app, selected with xcode-select (Command Line Tools alone are insufficient)." >&2
    echo "Install Xcode, then select it as the active developer directory and rerun this script." >&2
    exit 1
fi
version="${OPENOMSI_VERSION:-$(sh scripts/version.sh 2>/dev/null || echo 0.0.0)}"
export OPENOMSI_VERSION="$version"
cargo build --locked --release --target "$target" -p omsi-app -p omsi-launcher-core
out=dist/macos
app="$out/openOMSI.app"
# (only the bundle is replaced: the folders beside it are the content folder with the mods)
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "target/$target/release/openomsi" "$app/Contents/MacOS/openomsi"
cp "target/$target/release/openomsi-launcher" "$app/Contents/MacOS/openomsi-launcher"
cp "assets/steam_redist/libsteam_api.dylib" "$app/Contents/MacOS/"
if [ "$target" = x86_64-apple-darwin ]; then
    # CI can reuse the pinned ANGLE/CGL libraries from a previous successful build.
    # Local builds still compile them from source when no artifact directory is set.
    if [ -n "${OPENOMSI_ANGLE_ARTIFACT_DIR:-}" ]; then
        angle_out="$OPENOMSI_ANGLE_ARTIFACT_DIR"
        angle_license="$angle_out/LICENSE-ANGLE.txt"
    else
        angle_out=$(sh scripts/build-angle-macos.sh)
        angle_source=$(CDPATH= cd -- "$angle_out/../.." && pwd)
        angle_license="$angle_source/LICENSE"
    fi
    for lib in libEGL.dylib libGLESv2.dylib; do
        [ -s "$angle_out/$lib" ] || { echo "Missing ANGLE library: $angle_out/$lib" >&2; exit 1; }
        lipo "$angle_out/$lib" -verify_arch x86_64
    done
    [ -s "$angle_license" ] || { echo "Missing ANGLE license: $angle_license" >&2; exit 1; }
    cp "$angle_out/libEGL.dylib" "$angle_out/libGLESv2.dylib" "$app/Contents/MacOS/"
    cp "$angle_license" "$app/Contents/Resources/LICENSE-ANGLE.txt"
    mv "$app/Contents/MacOS/openomsi" "$app/Contents/MacOS/openomsi-bin"
    clang -arch x86_64 -mmacosx-version-min=11.0 -o "$app/Contents/MacOS/openomsi" scripts/macos/openomsi-cgl-launcher.c
fi
# the desktop launcher (launcher/, Tauri on WebKit), stamped with the release version (its
# About window and the Info.plist inside it said tauri.conf.json's)
TAURI_CONFIG="{\"version\":\"${version%%-*}\"}" cargo build --locked --release --target "$target" --manifest-path launcher/src-tauri/Cargo.toml
cp "launcher/src-tauri/target/$target/release/openomsi-launcher-app" "$app/Contents/MacOS/openomsi-launcher-app"
cp assets/icons/app/openomsi.icns "$app/Contents/Resources/openomsi.icns"
# (the bundle's version is numbers only: a test build's -pr<number> is left out there)
sed -e "s/@VERSION@/${version%%-*}/g" scripts/macos/Info.plist > "$app/Contents/Info.plist"
codesign --force --deep --sign - "$app" >/dev/null 2>&1 || true
printf '\nopenOMSI %s built. Play: open "%s/%s"\n' "$version" "$PWD" "$app"
