# Building from source

This page is for contributors who want to build openOMSI themselves. To play, download a
beta or a stable release from the [Releases](https://github.com/openOMSI-org/openOMSI/releases)
page instead.

## Prerequisites

Every platform needs the latest stable Rust from [rustup.rs](https://rustup.rs). Keep it current
with `rustup update stable`: the dependencies raise the oldest Rust they accept when they are
updated.

### macOS

- Xcode Command Line Tools: `xcode-select --install`.
- For the iPhone, iPad and Mac App Store apps: Xcode and XcodeGen (`brew install xcodegen`).

The game draws with Metal.

### Windows

- Rust with the MSVC toolchain.
- Visual Studio Build Tools with **Desktop development with C++** and the Windows SDK.
- For ARM64: the ARM64 build tools of Visual Studio and LLVM's `clang`.
- CMake on `PATH` (the OpenXR dependency is built with it).

The game draws with Vulkan, DirectX 12, DirectX 11 or OpenGL through ANGLE.

### Linux

On Debian or Ubuntu:

```bash
sudo apt install build-essential pkg-config libasound2-dev libudev-dev libgtk-3-dev \
  libxkbcommon-dev libwayland-dev libssl-dev libwebkit2gtk-4.1-dev
```

WebKitGTK 4.1 is for the desktop launcher. To play, you need Vulkan drivers (Mesa or NVIDIA).

### Android

- The Rust target: `rustup target add aarch64-linux-android`.
- JDK 17.
- An Android SDK with platform 34 or newer, build-tools and the NDK.

`scripts/build-android.sh` finds them through `android/env.sh`. It reads `ANDROID_HOME` (or
`ANDROID_SDK_ROOT`), `ANDROID_NDK_HOME` and `JAVA_HOME`, and picks the newest NDK, build-tools
and platform it finds. See [Android](ANDROID.md) for the app itself.

## Get the code

```bash
git clone https://github.com/openOMSI-org/openOMSI.git
cd openOMSI
```

To send changes, fork the repository and work in your fork. `CONTRIBUTING.md` in the repository
has the rules for pull requests.

## Build and run

Every build script lives in `scripts/`. You can run it from any folder, and paths with spaces
work. The result goes into `dist/<platform>/`.

> **Note:** `dist/<platform>/` is also the game's content folder: mods go beside the program.
> The scripts replace only the programs and never delete anything else there.

| Platform | Command | Result |
| --- | --- | --- |
| macOS | `scripts/build-macos.sh` | `dist/macos/openOMSI.app` |
| Windows | `scripts\build-windows.cmd` | `dist\windows\` |
| Windows, from macOS or Linux | `scripts/build-windows-cross.sh` | `dist/windows/` (no desktop launcher) |
| Linux | `scripts/build-linux.sh` | `dist/linux/` |
| Android | `scripts/build-android.sh` | `dist/android/openOMSI-<version>.apk` |
| iPhone, iPad, Mac app | `scripts/build-apple.sh <command>` | `apple/openOMSI.xcodeproj`, `dist/apple/` |
| Dedicated server | `scripts/build-server.sh [folder]` | `dist/server/` |
| 32-bit plugin host | `scripts/build-plugin-host.sh` | `dist/omsi-plugin-host32.exe` |

To run a desktop build, start `openomsi` (`openomsi.exe` on Windows, or open the `.app` on a
Mac). With no arguments it opens the launcher.

The version of a build comes from `scripts/version.sh`: `MAJOR.MINOR` from the `VERSION` file,
the patch number from the git tags. Set `OPENOMSI_VERSION` to override it. See
[Versions and releases](VERSIONING.md).

### Plain cargo

For quick work you don't need the scripts:

```bash
cargo build --release -p omsi-app      # the game: target/release/openomsi
cargo run --release -p omsi-app        # build and start it
```

A plain start opens the desktop launcher, so build that into the same target folder:

```bash
CARGO_TARGET_DIR=target cargo build --release --manifest-path launcher/src-tauri/Cargo.toml
```

Leave out `--release` in both commands for `target/debug`. The launcher is a Cargo workspace of
its own (`launcher/src-tauri`), excluded from the main one.

> **Tip:** Measure speed only with release builds. Debug builds are much slower, on macOS above
> all (every Objective-C message is checked there).

## The targets

### Desktop

The scripts build three programs:

| Program | Source | What it is |
| --- | --- | --- |
| `openomsi` | `crates/omsi-app` | The game |
| `openomsi-launcher-app` | `launcher/src-tauri` | The desktop launcher (Tauri) |
| `openomsi-launcher` | `crates/omsi-launcher-core` | The launcher's commands for a terminal |

`openomsi` decides what to do from its arguments:

- No arguments: it opens `openomsi-launcher-app` beside it. If that is missing, or its window
  cannot open, the game says so.
- Session arguments: it starts a session directly. See [Command line](COMMAND_LINE.md).
- `--server server.cfg`: it runs as the dedicated server.

`openomsi-launcher --cli <command> [json]` runs one launcher command in a terminal, for example
`openomsi-launcher --cli maps` or `--cli install '{"path":"mod.zip"}'`. Without `--cli` it starts
the game beside it with the same arguments.

Per platform, the scripts also copy:

- **macOS**: the bundle with `libsteam_api.dylib`, ad-hoc signed. `OPENOMSI_TARGET` picks the
  Rust target (default: the Mac's own).
- **Windows**: `openomsi.exe`, `openomsi-launcher.exe`, `openomsi-launcher-app.exe` and the 32-bit
  plugin host `omsi-plugin-host32.exe`; `steam_api64.dll` on x64. For ARM64 run
  `scripts\build-windows.cmd aarch64-pc-windows-msvc`.
- **Windows cross build**: needs MinGW (`brew install mingw-w64` or `apt install mingw-w64`); the
  script adds the `x86_64-pc-windows-gnu` target. It builds the game and `openomsi-launcher.exe`,
  without debug info.
- **Linux**: the three programs, `openomsi.png`, `openomsi.desktop`, and `libsteam_api.so` on
  x86-64.

> **Note:** The release packages for Windows also carry Google's ANGLE (`libEGL.dll`,
> `libGLESv2.dll`) for the ANGLE graphics API. A local build does not include it, and the
> game then skips that API.

### Launcher

The desktop launcher is a Tauri 2 window over a web page (`launcher/ui`: Preact, no build step,
no npm). Tauri comes from the organisation's fork, `openOMSI-org/tauri-openomsi`, pinned by git
revision in `launcher/src-tauri/Cargo.toml`.

```bash
cargo build --release --manifest-path launcher/src-tauri/Cargo.toml
```

On Linux it needs `libwebkit2gtk-4.1-dev`, `libgtk-3-dev` and `librsvg2-dev`.

To check it without a window, run `OPENOMSI_LAUNCHER_SELFTEST=1 openomsi-launcher-app`. To work
on the page in a browser, serve `launcher/` with `python3 -m http.server -d launcher` and open
`/ui/`. `launcher/README.md` has the details and the page tests.

### Android

```bash
scripts/build-android.sh            # build the APK
scripts/build-android.sh install    # build it and install it over USB (adb)
```

The game is built as a library, `libopenomsi_game.so`, for a NativeActivity. The launcher's page
runs in a web view over the game's window. The build uses the `android` Cargo profile (release
without debug info).

The APK is signed with `android/debug.keystore`; the script makes one when it is missing. With
`OPENOMSI_PR=<number>` the build is a separate app, "openOMSI PR #N", that installs beside the
release.

### Apple

`scripts/build-apple.sh` builds the game library (Rust) and the Swift app around it. The Xcode
project is made by XcodeGen from `apple/project.yml`.

| Command | What it does |
| --- | --- |
| `project` | Makes `apple/openOMSI.xcodeproj` |
| `rust ios\|ios-sim\|macos` | Builds only the game library (on a Mac also the game program) |
| `sim [device]` | Builds for the iOS Simulator, installs and starts it |
| `mac` | `dist/apple/openOMSI.app` |
| `ipa` | `dist/apple/openOMSI.ipa`, an App Store build |
| `mac-store` | `dist/apple/openOMSI.pkg`, for the Mac App Store |
| `shots <out> WxH [pages]` | The Swift launcher's pages as PNG files |

Signing uses `OPENOMSI_TEAM_ID`. The App Store and notarising also need an App Store Connect API
key: `OPENOMSI_ASC_KEY_ID`, `OPENOMSI_ASC_ISSUER` and `OPENOMSI_ASC_KEY` (the `.p8` file).
Without a team the apps are unsigned; the Simulator needs no signing. `OPENOMSI_MAC_ARCHS`
sets the Mac architectures (`"arm64 x86_64"` for a release). See [iPhone, iPad and Mac](APPLE.md).

### Dedicated server

```bash
scripts/build-server.sh [folder]
```

The server is the game program started with `--server`. The folder (default `dist/server/`)
gets `start.sh`, `run-server.sh` (which asks for the folder, the settings and the map) and a
copy of [Dedicated server](SERVER.md) as `README.md`. `server.cfg` is written with its defaults on the
first start.

### 32-bit plugin host

OMSI's plugin DLLs are 32-bit, so they run in a separate host process. The Windows script builds
it already. On macOS or Linux:

```bash
rustup target add i686-pc-windows-gnu
brew install mingw-w64          # or apt install mingw-w64
scripts/build-plugin-host.sh
```

Copy `dist/omsi-plugin-host32.exe` next to the game. Off Windows the game runs it through Wine.
See [Writing plugins](PLUGINS.md).

## Tests and checks

```bash
cargo test --workspace
```

Tests that read the original OMSI 2 installation are ignored by default. Run them with the
install's path:

```bash
OMSI_ROOT="/path/to/OMSI 2" cargo test -p <crate> -- --ignored
```

To load every content file of an installation and list what fails:

```bash
cargo run --release -p omsi-check -- "/path/to/OMSI 2"
```

A pull request also runs the size budget, the formatting check and the crate-layer check.
[Refactor checks](REFACTOR_CHECKS.md) explains them, and the checks that show a refactor
changed nothing.

## Icons

The application icon is made from the logos in `assets/logos`:

- `assets/icons/app/openomsi.svg` for Windows and Linux.
- `assets/icons/app/openomsi-macos.svg` for macOS (with the standard margin).

From them come `openomsi.ico` (embedded into the Windows programs at build time), `openomsi.icns`
(the macOS bundle) and `openomsi-256.png` (the window icon on Windows and Linux). After you
change the SVGs, regenerate them:

```bash
scripts/make-icons.sh
```

It needs `resvg` (`cargo install resvg`), `iconutil` (macOS) and Python 3 with Pillow and
fontTools. It also renders the pictures of the Windows installer and the macOS disk image.

## Common build errors

| Error | Fix |
| --- | --- |
| "Install Rust from https://rustup.rs" | Install Rust, or add `~/.cargo/bin` to `PATH`. |
| A dependency needs a newer `rustc` | `rustup update stable` |
| "Run xcode-select --install" | Install the Xcode Command Line Tools. |
| The lock file needs to be updated (`--locked`) | Update your checkout. Don't run `cargo update` in a pull request. |
| Windows: CMake not found while building `openxr` | Install CMake and put it on `PATH`. |
| Windows: linker or SDK headers missing | Install the Build Tools' **Desktop development with C++** and the Windows SDK. |
| Linux: `pkg-config` cannot find `alsa`, `libudev`, `gtk+-3.0` or `webkit2gtk-4.1` | Install the packages listed under [Linux](#linux). |
| "Install MinGW: brew install mingw-w64" | Needed for the cross build and the plugin host. |
| "The Android NDK was not found" | Install the NDK, or set `ANDROID_NDK_HOME`. |
| "No Android platform" / "No build-tools" | `sdkmanager "platforms;android-35"` and `sdkmanager "build-tools;35.0.0"` |
| "libopenomsi_game.so needs symbols no system library of API … has" | A dependency uses an API newer than `ANDROID_API` (26). Find the dependency that pulls in that symbol. |
| "Install XcodeGen" | `brew install xcodegen` |
| The game says the launcher is missing | Build `launcher/src-tauri` into the same target folder (see [Plain cargo](#plain-cargo)). |
| The disk fills up | `target/` grows to many gigabytes. Run one build at a time, and `cargo clean` old targets. |
