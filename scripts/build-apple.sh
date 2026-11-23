#!/bin/sh
# openBus for iPhone, iPad and Mac: the game library (Rust) and the Swift app around it
# (apple/, an Xcode project made by XcodeGen from apple/project.yml). Needs Xcode, Rust
# (https://rustup.rs) and XcodeGen (brew install xcodegen).
#
#   scripts/build-apple.sh project                 the Xcode project (apple/openBus.xcodeproj)
#   scripts/build-apple.sh rust ios|ios-sim|macos  the game library (and on a Mac the game program)
#   scripts/build-apple.sh sim [device]            build for the iOS Simulator, install, start
#   scripts/build-apple.sh mac                     dist/apple/openBus.app (signed when a team is set)
#   scripts/build-apple.sh ipa                     dist/apple/openBus.ipa (an App Store build)
#   scripts/build-apple.sh mac-store               dist/apple/openBus.pkg (the Mac App Store)
#   scripts/build-apple.sh shots <out> WxH [pages] the Swift launcher's pages as PNG files
#
# Signing: OPENBUS_TEAM_ID (the Apple Developer team), and for the App Store / notarising
# an App Store Connect API key: OPENBUS_ASC_KEY_ID, OPENBUS_ASC_ISSUER, OPENBUS_ASC_KEY
# (the .p8 file). Without a team the apps are built unsigned (the Simulator needs none).
# OPENBUS_RUST_TARGET is cargo's target folder (default: target); OPENBUS_MAC_ARCHS the
# Mac's architectures ("arm64 x86_64" for a release; the Mac's own by default).
set -eu
root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:$PATH"
target_dir="${OPENBUS_RUST_TARGET:-${CARGO_TARGET_DIR:-$root/target}}"
export CARGO_TARGET_DIR="$target_dir"
profile="${OPENBUS_RUST_PROFILE:-release}"
version="${OPENBUS_VERSION:-$(sh "$root/scripts/version.sh" 2>/dev/null || echo 0.0.0)}"
export OPENBUS_VERSION="$version"
# the store wants numbers only (a test build's -pr<number> left out); the build number is the
# commit count, as every upload needs a higher one
marketing="${version%%-*}"
build_number="$(git -C "$root" rev-list --count HEAD 2>/dev/null || echo 1)"
team="${OPENBUS_TEAM_ID:-}"
dist="$root/dist/apple"
apple="$root/apple"

cargo_flags() {
    [ "$profile" = release ] && echo "--release" || true
}

rust() {
    case "$1" in
    ios)
        rustup target add aarch64-apple-ios >/dev/null
        IPHONEOS_DEPLOYMENT_TARGET=18.0 cargo rustc --locked $(cargo_flags) --manifest-path "$root/crates/omsi-app/Cargo.toml" --lib --crate-type staticlib --target aarch64-apple-ios
        ;;
    ios-sim)
        rustup target add aarch64-apple-ios-sim >/dev/null
        IPHONEOS_DEPLOYMENT_TARGET=18.0 cargo rustc --locked $(cargo_flags) --manifest-path "$root/crates/omsi-app/Cargo.toml" --lib --crate-type staticlib --target aarch64-apple-ios-sim
        ;;
    macos)
        archs="${OPENBUS_MAC_ARCHS:-$(uname -m)}"
        libs=""
        games=""
        for a in $archs; do
            case "$a" in arm64 | aarch64) t=aarch64-apple-darwin ;; *) t=x86_64-apple-darwin ;; esac
            rustup target add "$t" >/dev/null
            MACOSX_DEPLOYMENT_TARGET=15.0 cargo rustc --locked $(cargo_flags) --manifest-path "$root/crates/omsi-app/Cargo.toml" --lib --crate-type staticlib --target "$t"
            MACOSX_DEPLOYMENT_TARGET=11.0 cargo build --locked $(cargo_flags) --manifest-path "$root/Cargo.toml" -p omsi-app -p omsi-launcher-core --target "$t"
            libs="$libs $target_dir/$t/$profile/libopenbus_game.a"
            games="$games $target_dir/$t/$profile/openbus"
        done
        out="$target_dir/universal-apple-darwin/$profile"
        mkdir -p "$out"
        # shellcheck disable=SC2086
        lipo -create $libs -output "$out/libopenbus_game.a"
        # shellcheck disable=SC2086
        lipo -create $games -output "$out/openbus"
        launchers=""
        for a in $archs; do
            case "$a" in arm64 | aarch64) t=aarch64-apple-darwin ;; *) t=x86_64-apple-darwin ;; esac
            launchers="$launchers $target_dir/$t/$profile/openbus-launcher"
        done
        # shellcheck disable=SC2086
        lipo -create $launchers -output "$out/openbus-launcher"
        ;;
    *)
        echo "rust: ios, ios-sim or macos" >&2
        exit 2
        ;;
    esac
}

# Called by Xcode before the Swift sources are built (the project's build phase).
rust_for_xcode() {
    [ "${OPENBUS_SKIP_RUST:-}" = 1 ] && exit 0
    case "${PLATFORM_NAME:-}" in
    iphoneos) rust ios ;;
    iphonesimulator) rust ios-sim ;;
    macosx) rust macos ;;
    *) echo "unknown platform ${PLATFORM_NAME:-}" >&2; exit 2 ;;
    esac
}

# Called by Xcode after the Mac app is linked: the game program and the Steam library go
# beside the launcher in Contents/MacOS, signed as the app is.
embed_game() {
    dir="$TARGET_BUILD_DIR/$EXECUTABLE_FOLDER_PATH"
    src="$target_dir/universal-apple-darwin/$profile"
    cp "$src/openbus" "$dir/openbus"
    cp "$src/openbus-launcher" "$dir/openbus-launcher"
    cp "$root/assets/steam_redist/libsteam_api.dylib" "$dir/"
    identity="${EXPANDED_CODE_SIGN_IDENTITY:-}"
    if [ -n "$identity" ] && [ "$identity" != "-" ]; then
        ent="$apple/macOS/openBus.entitlements"
        [ "${OPENBUS_STORE:-}" = 1 ] && ent="$apple/macOS/openbus-game-AppStore.entitlements"
        codesign --force --options runtime --timestamp --sign "$identity" "$dir/libsteam_api.dylib"
        for b in openbus openbus-launcher; do
            codesign --force --options runtime --timestamp --entitlements "$ent" --sign "$identity" "$dir/$b"
        done
    else
        codesign --force --sign - "$dir/libsteam_api.dylib" "$dir/openbus" "$dir/openbus-launcher" 2>/dev/null || true
    fi
}

project() {
    command -v xcodegen >/dev/null || { echo "Install XcodeGen: brew install xcodegen" >&2; exit 1; }
    # (project.yml takes the version and the team from the environment)
    OPENBUS_MARKETING_VERSION="$marketing" OPENBUS_BUILD_NUMBER="$build_number" OPENBUS_TEAM_ID="$team" \
        xcodegen generate --quiet --spec "$apple/project.yml" --project "$apple"
}

xcb() {
    xcodebuild -project "$apple/openBus.xcodeproj" OPENBUS_RUST_TARGET="$target_dir" OPENBUS_RUST_PROFILE="$profile" "$@"
}

sim() {
    device="${1:-iPhone 17 Pro}"
    rust ios-sim
    project
    xcb -scheme openBus-iOS -configuration Release -sdk iphonesimulator -destination "platform=iOS Simulator,name=$device" \
        -derivedDataPath "$dist/derived" OPENBUS_SKIP_RUST=1 CODE_SIGNING_ALLOWED=NO build
    app="$dist/derived/Build/Products/Release-iphonesimulator/openBus.app"
    xcrun simctl boot "$device" 2>/dev/null || true
    xcrun simctl install "$device" "$app"
    xcrun simctl launch "$device" io.github.rafaelkaiz.openbus
}

mac() {
    rust macos
    project
    mkdir -p "$dist"
    if [ -n "$team" ]; then
        xcb -scheme openBus-macOS -configuration Release -archivePath "$dist/openBus-macOS.xcarchive" OPENBUS_SKIP_RUST=1 archive
        cat > "$dist/export-mac.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>method</key><string>developer-id</string>
<key>teamID</key><string>$team</string>
<key>signingStyle</key><string>automatic</string>
</dict></plist>
EOF
        xcb -exportArchive -archivePath "$dist/openBus-macOS.xcarchive" -exportPath "$dist" -exportOptionsPlist "$dist/export-mac.plist" $(asc_auth)
        if [ -n "${OPENBUS_ASC_KEY_ID:-}" ]; then
            ditto -c -k --keepParent "$dist/openBus.app" "$dist/openBus-notarize.zip"
            xcrun notarytool submit "$dist/openBus-notarize.zip" --key "$OPENBUS_ASC_KEY" --key-id "$OPENBUS_ASC_KEY_ID" --issuer "$OPENBUS_ASC_ISSUER" --wait
            xcrun stapler staple "$dist/openBus.app"
            rm -f "$dist/openBus-notarize.zip"
        fi
    else
        xcb -scheme openBus-macOS -configuration Release -derivedDataPath "$dist/derived" OPENBUS_SKIP_RUST=1 CODE_SIGN_IDENTITY=- build
        rm -rf "$dist/openBus.app"
        cp -R "$dist/derived/Build/Products/Release/openBus.app" "$dist/"
    fi
    printf '\nopenBus %s for Mac: %s\n' "$version" "$dist/openBus.app"
}

# The App Store Connect API key, for xcodebuild's automatic signing on a machine that is
# signed in to no Apple account (the CI).
asc_auth() {
    if [ -n "${OPENBUS_ASC_KEY_ID:-}" ]; then
        echo "-allowProvisioningUpdates -authenticationKeyPath $OPENBUS_ASC_KEY -authenticationKeyID $OPENBUS_ASC_KEY_ID -authenticationKeyIssuerID $OPENBUS_ASC_ISSUER"
    fi
}

ipa() {
    [ -n "$team" ] || { echo "An App Store build needs OPENBUS_TEAM_ID" >&2; exit 1; }
    rust ios
    project
    mkdir -p "$dist"
    xcb -scheme openBus-iOS -configuration Release -destination "generic/platform=iOS" -archivePath "$dist/openBus-iOS.xcarchive" OPENBUS_SKIP_RUST=1 $(asc_auth) archive
    cat > "$dist/export-ios.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>method</key><string>app-store-connect</string>
<key>teamID</key><string>$team</string>
<key>signingStyle</key><string>automatic</string>
<key>destination</key><string>export</string>
</dict></plist>
EOF
    xcb -exportArchive -archivePath "$dist/openBus-iOS.xcarchive" -exportPath "$dist" -exportOptionsPlist "$dist/export-ios.plist" $(asc_auth)
    printf '\nopenBus %s for iPhone and iPad: %s\n' "$version" "$dist/openBus.ipa"
}

mac_store() {
    [ -n "$team" ] || { echo "A Mac App Store build needs OPENBUS_TEAM_ID" >&2; exit 1; }
    export OPENBUS_STORE=1
    rust macos
    project
    mkdir -p "$dist"
    xcb -scheme openBus-macOS -configuration Release -archivePath "$dist/openBus-macOS-store.xcarchive" OPENBUS_SKIP_RUST=1 OPENBUS_STORE=1 \
        CODE_SIGN_ENTITLEMENTS=macOS/openBus-AppStore.entitlements $(asc_auth) archive
    cat > "$dist/export-mac-store.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>method</key><string>app-store-connect</string>
<key>teamID</key><string>$team</string>
<key>signingStyle</key><string>automatic</string>
<key>destination</key><string>export</string>
</dict></plist>
EOF
    xcb -exportArchive -archivePath "$dist/openBus-macOS-store.xcarchive" -exportPath "$dist/store" -exportOptionsPlist "$dist/export-mac-store.plist" $(asc_auth)
}

shots() {
    out="$1"; size="$2"; shift 2
    rust macos
    bin="$target_dir/launcher-shots"
    src="$target_dir/universal-apple-darwin/$profile"
    mkdir -p "$target_dir/launcher-shots-src"
    cp "$apple/Tools/LauncherShots.swift" "$target_dir/launcher-shots-src/main.swift"
    xcrun swiftc -O -target "$(uname -m)-apple-macos15.0" -import-objc-header "$apple/Shared/Bridge/openbus.h" \
        "$apple"/Shared/*/*.swift "$apple/macOS/Platform.swift" "$target_dir/launcher-shots-src/main.swift" \
        -L "$src" -lopenbus_game -L "$root/assets/steam_redist" -lsteam_api -lc++ -liconv \
        -framework Accelerate -framework AudioUnit -framework CoreAudio -framework QuartzCore -framework Metal \
        -framework ApplicationServices -framework CoreGraphics -framework CoreVideo -framework Carbon -framework AppKit \
        -framework GameController -framework IOKit -o "$bin"
    cp "$root/assets/steam_redist/libsteam_api.dylib" "$target_dir/"
    OPENBUS_FONTS="$apple/Resources/Fonts" OMSI_BACKGROUND=1 OMSI_NO_UPDATE=1 "$bin" "$out" "$size" 12 "$@"
}

cmd="${1:-}"
[ $# -gt 0 ] && shift
case "$cmd" in
project) project ;;
rust) rust "$@" ;;
rust-for-xcode) rust_for_xcode ;;
embed-game) embed_game ;;
sim) sim "$@" ;;
mac) mac ;;
ipa) ipa ;;
mac-store) mac_store ;;
shots) shots "$@" ;;
*)
    sed -n '2,20p' "$0"
    exit 2
    ;;
esac
