# Building from source

Releases (betas and stable releases) are on the
[Releases](https://github.com/rafaelkaiz/openBus/releases) page; build from source only to
work on openBus itself.

All scripts live in `scripts/`, run from any folder (paths with spaces are fine) and put the
result into `dist/<platform>/`. That folder is also the game's **content folder** (mods go
beside the binary), so the scripts replace only the binaries and never delete anything else
there.

## Requirements

* The latest [Rust stable](https://rustup.rs) (`rustup update stable`): the builds use it,
  and the dependencies raise the oldest Rust they accept as they are updated.
* **macOS**: Xcode Command Line Tools (`xcode-select --install`). Metal is used for drawing.
* **Windows**: Rust with the MSVC toolchain and Visual Studio Build Tools with *Desktop
  development with C++* and the Windows SDK (for ARM64 also its ARM64 build tools and LLVM's
  clang). CMake is needed to build the OpenXR dependency; it must be on `PATH`. Vulkan or
  DirectX 12 is used for drawing.
* **Linux** (Debian/Ubuntu names):
  `sudo apt install build-essential pkg-config libasound2-dev libudev-dev libgtk-3-dev libxkbcommon-dev libwayland-dev libssl-dev libwebkit2gtk-4.1-dev` (WebKitGTK for the desktop launcher).
  Vulkan drivers (Mesa, NVIDIA) are needed to play.
* **Android**: the `aarch64-linux-android` Rust target, JDK 17, and an Android SDK with
  platform 34 or newer, build-tools and the NDK. `scripts/build-android.sh` looks for them
  through `android/env.sh` (`ANDROID_HOME`, `ANDROID_NDK_HOME`). See [ANDROID.md](ANDROID.md).
* **iPhone, iPad and the Mac app**: Xcode and XcodeGen (`brew install xcodegen`).

## Build

| Platform | Command | Result |
| --- | --- | --- |
| macOS | `scripts/build-macos.sh` | `dist/macos/openBus.app` |
| Windows | `scripts\build-windows.cmd` (ARM64: `scripts\build-windows.cmd aarch64-pc-windows-msvc`) | `dist\windows\openbus.exe`, `openbus-launcher-app.exe`, `openbus-launcher.exe` |
| Windows, from a Mac or Linux | `scripts/build-windows-cross.sh` (needs `mingw-w64` and the `x86_64-pc-windows-gnu` target) | `dist/windows/openbus.exe`, `openbus-launcher.exe` (no desktop launcher) |
| Linux | `scripts/build-linux.sh` | `dist/linux/openbus`, `openbus-launcher-app`, `openbus-launcher`, `.desktop` file |
| Android | `scripts/build-android.sh` | `dist/android/openBus-<version>.apk` |
| iPhone, iPad, Mac app | `scripts/build-apple.sh project\|sim\|mac\|ipa\|mac-store` | `apple/openBus.xcodeproj`, `dist/apple/` |
| Dedicated server | `scripts/build-server.sh [folder]` | `dist/server/` with `start.sh` |
| 32-bit plugin host | `scripts/build-plugin-host.sh` | `dist/omsi-plugin-host32.exe` (see [PLUGINS.md](PLUGINS.md)) |

Plain cargo works too: `cargo build --release -p omsi-app` builds `target/release/openbus`.

## The programs

* `openbus` (`crates/omsi-app`) - the game. Started with no arguments it opens the desktop
  launcher beside it, `openbus-launcher-app` (`launcher/`, see its README), and says so
  when that is missing; with arguments it starts a session directly (see
  [USER_GUIDE.md](USER_GUIDE.md)); with `--server server.cfg` it is the dedicated server.
* `openbus-launcher-app` (`launcher/src-tauri`, a workspace of its own) - the launcher. A
  development build finds it when it is built into the same target folder as the game:
  `CARGO_TARGET_DIR=target cargo build --manifest-path launcher/src-tauri/Cargo.toml` puts it
  into `target/debug` beside `openbus` (`--release` for `target/release`).
* `openbus-launcher` (`crates/omsi-launcher-core`) - the launcher's commands for a terminal
  (`openbus-launcher --cli maps`, `--cli install '{"path":"mod.zip"}'` …); without `--cli`
  it starts the game beside it with the same arguments.
* `omsi-check` (`tools/omsi-check`) - loads every content file of an installation and reports
  what failed: `cargo run --release -p omsi-check -- "/path/to/OMSI 2"`.

## Icons

The application icon is made from the logos in `assets/logos`:
`assets/icons/app/openbus.svg` (Windows/Linux) and `openbus-macos.svg` (macOS, with the
standard margin). `openbus.ico` is embedded into the Windows executables at build time
(`build.rs`, `winresource`), `openbus.icns` goes into the macOS bundle, and
`openbus-256.png` is the window icon on Windows and Linux. To regenerate them after changing
the SVGs (needs `cargo install resvg`):

```sh
scripts/make-icons.sh
```

## Tests

```sh
cargo test --workspace
```

Some tests read an OMSI 2 installation (`OMSI_ROOT=/path/to/OMSI 2`) and skip themselves
when there is none.
