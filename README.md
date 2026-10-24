<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logos/openomsi-wordmark-light.svg">
    <img alt="openOMSI" src="assets/logos/openomsi-wordmark-dark.svg" width="420">
  </picture>
</p>

<p align="center">
  <a href="https://github.com/openOMSI-org/openOMSI/releases/latest"><img alt="Version" src="https://img.shields.io/github/v/release/openOMSI-org/openOMSI?label=version&color=f47f30&style=for-the-badge"></a>
  <a href="https://openomsi.org/"><img alt="Playing now" src="https://img.shields.io/endpoint?url=https%3A%2F%2Fopenomsi.savvabestbrother.workers.dev%2Fbadge&label=playing%20now&style=for-the-badge&logo=steam&logoColor=white"></a>
  <a href="https://github.com/openOMSI-org/openOMSI/actions/workflows/release.yml"><img alt="Build" src="https://img.shields.io/github/actions/workflow/status/openOMSI-org/openOMSI/release.yml?branch=main&style=for-the-badge&label=build"></a>
  <a href="https://openomsi.org/"><img alt="Docs" src="https://img.shields.io/badge/docs-website-2d3138?style=for-the-badge"></a>
  <a href="https://discord.gg/openomsi"><img alt="Discord" src="https://img.shields.io/badge/discord-join%20us-5865F2?style=for-the-badge&logo=discord&logoColor=white"></a>
  <a href="https://buymeacoffee.com/usonskyyy"><img alt="Buy me a coffee" src="https://img.shields.io/badge/buy%20me%20a%20coffee-support-ffdd00?style=for-the-badge&logo=buymeacoffee&logoColor=black"></a>
  <a href="https://ko-fi.com/usonance"><img alt="Ko-fi" src="https://img.shields.io/badge/ko--fi-support-29abe0?style=for-the-badge&logo=kofi&logoColor=white"></a>
  <a href="LICENSE"><img alt="License" src="https://img.shields.io/github/license/openOMSI-org/openOMSI?style=for-the-badge"></a>
</p>

> [!WARNING]
> **Early development. Expect bugs.** Things may be missing, broken or change between
> versions. Please report problems in [Issues](https://github.com/openOMSI-org/openOMSI/issues)
> or on our [Discord server](https://discord.gg/openomsi).

**openOMSI** is a from-scratch recreation of the bus simulator **OMSI 2**, written in Rust:
64-bit, multithreaded, with a modern renderer (Metal / Vulkan / DirectX 12 through wgpu).
It loads the existing OMSI 2 maps, buses, scenery and mods as they are.

> [!IMPORTANT]
> **openOMSI needs an installed copy of OMSI 2.** It contains no game content of its own: it
> plays on the maps, vehicles and other files of your OMSI 2 and **will not start without one**.

## What you get

* **Your OMSI 2 content, unchanged** - maps, buses, AI traffic, scenery, scripts, timetables,
  HOF files, weather and plugins load the way OMSI 2.2.032 loads them. The OMSI 2 folder is
  never written to.
* **A new engine** - 64-bit (no 2 GB limit), tiles and textures loaded on worker threads,
  four graphics modes from *Vanilla* (looks like OMSI 2) to *Enhanced+* (physically based
  lighting with ray-traced shadows and reflections).
* **A launcher** - pick a map, line, tour, bus and livery on one screen with a route map and a
  live bus preview; mods from a folder or a `.zip`/`.7z`/`.rar`; keys and game controllers
  (wheels, pedals, force feedback); updates on a Stable or Beta channel.
* **Multiplayer** - LAN sessions and a dedicated server ([docs/SERVER.md](docs/SERVER.md)).
* **Every platform** - Windows, macOS, Linux and Android; VR through OpenXR on Windows
  ([docs/VR.md](docs/VR.md)).
* **More for modders** - PBR materials, larger textures, more lights, and plugins in Lua or
  compiled `.oop` (Lua or WebAssembly), plus OMSI's own plugin DLLs on Windows.

## Download

The newest stable version is always at
[**Releases → Latest**](https://github.com/openOMSI-org/openOMSI/releases/latest) and on the
[website](https://openomsi.org/#/download). Newer, untested builds are published as
*Pre-release* (beta) on the [Releases](https://github.com/openOMSI-org/openOMSI/releases) page.

| Platform | File |
| --- | --- |
| Windows x64 / ARM64 | `openOMSI-<version>-windows-x64-setup.exe` / `-windows-arm64-setup.exe` - installs openOMSI for you, no administrator rights (or the `.zip`: run `openomsi.exe`) |
| macOS (Apple silicon / Intel), macOS 11+ | `openOMSI-<version>-macos-arm64.dmg` / `-macos-x64.dmg` - drag `openOMSI.app` onto Applications (or the `.zip`) |
| Linux x64 / ARM64 | `openOMSI-<version>-linux-x64.zip` / `-linux-arm64.zip` - run `openomsi` |
| Android 8.0+ (arm64) | `openOMSI-<version>-android-arm64.apk` - see [docs/ANDROID.md](docs/ANDROID.md) |
| Dedicated server | `openOMSI-<version>-server-<linux\|windows>-<x64\|arm64>.zip` - see [docs/SERVER.md](docs/SERVER.md) |

`SHA256SUMS` in each release lists the checksums of all files.

The launcher keeps itself up to date: when a newer version is out it offers it at the start
and installs it with your yes. **Settings → General → Updates** chooses the channel (Stable or
Beta), turns the check off or installs without asking.

## Installation

You need **OMSI 2** installed (Steam or retail) with its stock content.

1. **Install** it. On Windows the `-setup.exe` installs openOMSI into your user folder
   (`%LOCALAPPDATA%\Programs\openOMSI`) with a Start menu entry; on macOS open the `.dmg` and
   drag `openOMSI.app` onto Applications. The `.zip` (Linux, or the portable Windows and macOS
   version) goes into a folder of its own that you can write to (your Documents, a games
   folder, or the OMSI 2 folder itself) - not `Program Files`, where the launcher cannot
   update itself.
2. **Start it.**
   * **Windows:** openOMSI in the Start menu (or `openomsi.exe`). SmartScreen may warn about
     an unknown app: *More info* → *Run anyway*.
   * **macOS:** `openOMSI.app`. If macOS refuses an app from the internet: right-click →
     *Open* → *Open*, or run `xattr -dr com.apple.quarantine /path/to/openOMSI.app` once.
   * **Linux:** `./openomsi` (`chmod +x openomsi` first if needed). It needs a Vulkan or
     OpenGL driver (Mesa: `mesa-vulkan-drivers`, or your GPU vendor's driver), and
     **WebKitGTK 4.1** for the launcher window (`libwebkit2gtk-4.1-0` on Debian/Ubuntu,
     `webkit2gtk4.1` on Fedora, `webkit2gtk-4.1` on Arch; for the Steam Deck see the [user
     guide](docs/USER_GUIDE.md#the-launcher)).
   * **Android:** see [docs/ANDROID.md](docs/ANDROID.md) - the OMSI 2 folder is copied onto
     the device first.
3. **Find OMSI 2.** The launcher usually finds the installation by itself (Steam libraries and
   the usual folders). If not, open **Setup**, press **Browse…** beside *OMSI 2 folder*, pick
   the folder with `Omsi.exe`, `maps` and `Vehicles` in it (Steam:
   `…\Steam\steamapps\common\OMSI 2`) and press **Save**.
4. **Drive:** choose a map, line, tour and bus on the **Drive** page and press **Start the duty**.

**Mods** are installed on the **Mods** page (a folder or an archive, or dropped onto the
window) or put into the `Mods` folder next to the game.

### When something goes wrong

* **"OMSI 2 was not found"** - choose the folder under **Setup** (step 3); when it is not
  saved, the message says what the chosen folder lacks.
* **The game closes after a few seconds, or "the graphics device was lost"** - update the
  graphics driver (from NVIDIA, AMD or Intel, not the one Windows installs). On Windows,
  **Settings → Graphics → Graphics API** can switch to DirectX 12, OpenGL or ANGLE (DirectX 11).
* **An older graphics card** - openOMSI falls back to another graphics API by itself; the same
  setting chooses one by hand.
* **Stuck at a bridge or an invisible wall** on a mod map - Esc → Options → *Collisions with
  objects* off (also under **Settings → Gameplay**).
* **Multiplayer: you do not meet the others** - every player needs the host's map. A map
  installed through the Mods page is passed on; one in the OMSI 2 folder is not.
* **Keys do not do what you set** - the **Controls** page shows which driving keys are in
  use (Settings → Driving → *Driving keys*); a key changed there takes effect at once.
* **Anything else** - when the game ends on an error, the launcher shows it with *Copy report*
  and *Report on GitHub*. Logs are in `~/.openomsi` (Windows: `C:\Users\<you>\.openomsi`),
  `game.log` for the last game. **Settings → General → Support → Support package** saves a
  support package into `~/.openomsi` (`openomsi --export-diagnostics <file.zip>` does the same
  from the command line): system, graphics card, driver, settings, controllers, last map and
  bus - no paths, names, chat or addresses. Nothing is sent; look inside before you attach it
  to an issue.

## Goals

1. **1:1 behaviour.** Every content format of the original loads and behaves as in OMSI
   2.2.032. Existing maps and mods work unchanged.
2. **No original code or assets.** Nothing from the original is copied; the formats are
   described in [docs/FORMATS.md](docs/FORMATS.md).
3. **A better engine.** 64-bit, multithreaded, multiplayer, and every platform.

## Documentation

The documentation is on the website, **https://openomsi.org/**, and in [`docs/`](docs):

| Document | What is in it |
| --- | --- |
| [User guide](docs/USER_GUIDE.md) | driving, the launcher, settings, mods, weather, LAN play |
| [Android](docs/ANDROID.md) | the mobile version: install, touch controls, building the APK |
| [Virtual reality](docs/VR.md) | OpenXR headsets on Windows |
| [Dedicated server](docs/SERVER.md) | hosting a multiplayer session without a window |
| [Modding](docs/MODDING.md) | what openOMSI allows beyond OMSI 2: more lights, larger textures, additions OMSI 2 ignores |
| [PBR materials](docs/PBR.md) | normal, roughness, metalness and occlusion maps |
| [Map packages](docs/MAP_PACKAGE.md) | optional surroundings a map can ship for the navigator and city map |
| [Plugins](docs/PLUGINS.md) | Lua and WebAssembly plugins, `.oop` files, OMSI plugin DLLs |
| [Development Tools](https://github.com/openOMSI-org/openOMSI-Development-Tools) | the plugin workbench: projects, API docs, the `.oop` compiler |
| [Add-ons](docs/ADDONS.md) | the launcher's directory of add-ons, and how to list yours |
| [Content formats](docs/FORMATS.md) | every OMSI 2 file format |
| [Routes](docs/ROUTES.md) | timetables, chrono, HOF files, IBIS |
| [Building](docs/BUILDING.md) | building from source on every platform |
| [Architecture](docs/ARCHITECTURE.md) | crates, threading, renderer |
| [Debug switches](docs/DEBUG_FLAGS.md) | environment variables for testing and bug reports |
| [Versioning & releases](docs/VERSIONING.md) | version numbers, beta and stable releases |
| [Changelog](CHANGELOG.md) | what changed in each version |

## Building from source

```sh
git clone https://github.com/openOMSI-org/openOMSI.git && cd openOMSI
scripts/build-macos.sh        # macOS   → dist/macos/openOMSI.app
scripts\build-windows.cmd     # Windows → dist\windows\openomsi.exe
scripts/build-linux.sh        # Linux   → dist/linux/openomsi
scripts/build-android.sh      # Android → dist/android/openOMSI-<version>.apk
scripts/build-server.sh       # server  → dist/server
```

Needs the latest [Rust stable](https://rustup.rs) and the platform's C toolchain (on Linux
also the WebKitGTK 4.1 headers for the launcher); details in [docs/BUILDING.md](docs/BUILDING.md).

## Repository layout

```
openOMSI/
├── crates/        the game, one crate per subsystem (omsi-app is the program `openomsi`;
│                  formats, scripts, simulation, renderer, sound, network, plugins, launcher data)
├── launcher/      the launcher window (Tauri; web UI in launcher/ui)
├── android/       the Android app wrapper
├── apple/         the iPhone, iPad and Mac app wrapper (SwiftUI, not released yet)
├── services/      the "playing now" counter
├── tools/         omsi-check (format coverage), avatar and ambience pack builders, voice chat plugin
├── scripts/       build scripts for every platform, version.sh, release notes
├── assets/        fonts, icons, logos
├── docs/          documentation (also published on the website)
├── site/          the website
├── release/       health.json: released versions known to be broken
└── VERSION        MAJOR.MINOR of the next release
```

## Contributing

Issues and pull requests are welcome - see [CONTRIBUTING.md](CONTRIBUTING.md); how the project is
run, reviewed and released is in [GOVERNANCE.md](GOVERNANCE.md).

## Support

openOMSI is made in free time. If you enjoy it and want to help it along, you can buy me a
coffee or support it on Ko-fi - thank you!

<p>
  <a href="https://buymeacoffee.com/usonskyyy"><img alt="Buy me a coffee" src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" height="48"></a>
  <a href="https://ko-fi.com/usonance"><img alt="Support me on Ko-fi" src="https://storage.ko-fi.com/cdn/kofi2.png?v=3" height="48"></a>
</p>

## Acknowledgements

Some ideas and fixes in openOMSI date back to the public work of
[neoOMSI](https://github.com/neoOMSI/neoOMSI): their reports and pull requests pointed at
bugs and features that we then implemented in our own code. Thank you.

## License

openOMSI is released under the [GNU Affero General Public License, version 3 only](LICENSE)
(`AGPL-3.0-only`). The historical MIT copyright and permission notice is retained in
`LICENSE`; earlier MIT releases keep their terms. Third-party files keep their own licenses.
The launcher's bus preview encodes its pictures with
[jpeg-encoder](https://crates.io/crates/jpeg-encoder): this software is based in part on the
work of the Independent JPEG Group.
The openOMSI name and original project logos are marks of `usonskyyyy`, who created
the original codebase; see [TRADEMARKS.md](TRADEMARKS.md). The official organization is
[openOMSI.org (`openOMSI-org`)](https://github.com/openOMSI-org).
OMSI and OMSI 2 are trademarks of their
respective owners. openOMSI is an independent project and is not affiliated with them.
