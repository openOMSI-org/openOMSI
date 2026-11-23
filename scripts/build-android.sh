#!/bin/sh
# Build openBus for Android (arm64) as dist/android/openBus-<version>.apk: the game and
# its launcher in one app (NativeActivity + libopenbus_game.so, see
# crates/omsi-app/src/android.rs). Needs Rust with the aarch64-linux-android target
# (rustup target add aarch64-linux-android), the Android SDK (platform 34+, build-tools,
# the NDK) and a JDK 17; see android/env.sh for where they are looked for.
#
#   scripts/build-android.sh            build the APK
#   scripts/build-android.sh install    ... and install it on the phone attached by USB (adb)
set -eu
cd "$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
. android/env.sh
[ -x "$CC_aarch64_linux_android" ] || { echo "The Android NDK was not found (set ANDROID_NDK_HOME)." >&2; exit 1; }
[ -f "$ANDROID_JAR" ] || { echo "No Android platform in $ANDROID_HOME/platforms (sdkmanager \"platforms;android-35\")." >&2; exit 1; }
[ -x "$BUILD_TOOLS/aapt" ] || { echo "No build-tools in $ANDROID_HOME (sdkmanager \"build-tools;35.0.0\")." >&2; exit 1; }
version="${OPENBUS_VERSION:-$(sh scripts/version.sh 2>/dev/null || echo 0.0.0)}"
export OPENBUS_VERSION="$version"
# the version code: the commit count (each release one higher)
code=$(git rev-list --count HEAD 2>/dev/null || echo 1)
# a pull request's test build (OPENBUS_PR=<number>) is an app of its own, "openBus PR #N":
# it installs beside the release without replacing it or being refused for its signing key,
# and shares the openBus folder (settings, content) with it
package=org.openbus.game
label=openBus
if [ -n "${OPENBUS_PR:-}" ]; then package=org.openbus.game.pr; label="openBus PR #$OPENBUS_PR"; fi
target=aarch64-linux-android
build=android/build
rm -rf "$build"
mkdir -p "$build/classes" "$build/apk/lib/arm64-v8a" dist/android

# --- the native code
cargo rustc --locked --profile android --target "$target" -p omsi-app --lib --crate-type cdylib
so="target/$target/android/libopenbus_game.so"
cp "$so" "$build/apk/lib/arm64-v8a/"
# every symbol the library needs must be in the system's libraries (Android loads it with
# BIND_NOW: one missing symbol and the app closes before its window opens)
sysroot="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$HOST_TAG/sysroot/usr/lib/aarch64-linux-android/$ANDROID_API"
"$NDK_BIN/llvm-nm" -D "$so" | awk '$1 == "U" {print $2}' | sed 's/@.*//' | sort -u > "$build/needed.txt"
for l in libc libm libdl liblog libandroid libOpenSLES libaaudio libvulkan; do
    [ -f "$sysroot/$l.so" ] && "$NDK_BIN/llvm-nm" -D --defined-only "$sysroot/$l.so" | awk '{print $3}'
done | sed 's/@.*//' | sort -u > "$build/provided.txt"
missing=$(comm -23 "$build/needed.txt" "$build/provided.txt")
if [ -n "$missing" ]; then
    echo "libopenbus_game.so needs symbols no system library of API $ANDROID_API has:" >&2
    echo "$missing" >&2
    exit 1
fi
# (a C++ dependency linked against the shared C++ library needs it beside it)
if "$NDK_BIN/llvm-readelf" -d "$so" | grep -q 'libc++_shared.so'; then
    cp "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$HOST_TAG/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so" "$build/apk/lib/arm64-v8a/"
fi

# --- the activity (Java)
"$JAVA_HOME/bin/javac" -source 8 -target 8 -nowarn -Xlint:-options -encoding UTF-8 \
    -classpath "$ANDROID_JAR" -d "$build/classes" android/java/org/openbus/game/OmsiActivity.java android/java/org/openbus/game/LauncherBridge.java
"$BUILD_TOOLS/d8" --release --min-api 26 --lib "$ANDROID_JAR" --output "$build/apk" \
    $(find "$build/classes" -name '*.class')

# --- the launcher page (the desktop launcher's, shown in a web view over the window)
mkdir -p "$build/assets/launcher"
cp -R launcher/ui "$build/assets/launcher/ui"

# --- the package
sed -e "s/@VERSION_CODE@/$code/" -e "s/@VERSION_NAME@/$version/" -e "s/android:label=\"openBus\"/android:label=\"$label\"/"     android/AndroidManifest.xml > "$build/AndroidManifest.xml"
"$BUILD_TOOLS/aapt" package -f --rename-manifest-package "$package" -M "$build/AndroidManifest.xml" -S android/res -A "$build/assets" -I "$ANDROID_JAR" -F "$build/unsigned.apk"
( cd "$build/apk" && "$BUILD_TOOLS/aapt" add ../unsigned.apk classes.dex lib/arm64-v8a/* >/dev/null )
"$BUILD_TOOLS/zipalign" -f -p 4 "$build/unsigned.apk" "$build/aligned.apk"
key=android/debug.keystore
# (the CI signs with the same key every time when it is given as a secret: an update then
# installs over the last release)
if [ ! -f "$key" ] && [ -n "${ANDROID_KEYSTORE_B64:-}" ]; then
    printf '%s' "$ANDROID_KEYSTORE_B64" | base64 -d > "$key"
fi
if [ ! -f "$key" ]; then
    "$JAVA_HOME/bin/keytool" -genkeypair -keystore "$key" -storepass android -keypass android \
        -alias openbus -keyalg RSA -keysize 2048 -validity 10000 \
        -dname "CN=openBus, O=openBus, C=DE" >/dev/null 2>&1
fi
apk="dist/android/openBus-$version.apk"
"$BUILD_TOOLS/apksigner" sign --ks "$key" --ks-pass pass:android --key-pass pass:android \
    --ks-key-alias openbus --out "$apk" "$build/aligned.apk"
rm -f "$apk.idsig"
printf '\nopenBus %s for Android: %s (%s)\n' "$version" "$PWD/$apk" "$(du -h "$apk" | cut -f1)"
if [ "${1:-}" = install ]; then
    "$ANDROID_HOME/platform-tools/adb" install -r "$apk"
fi
