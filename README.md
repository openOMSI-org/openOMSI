<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logos/openbus-wordmark-light.svg">
    <img alt="openBus" src="assets/logos/openbus-wordmark-dark.svg" width="420">
  </picture>
</p>

<p align="center">
  <a href="https://github.com/rafaelkaiz/openBus/releases/latest"><img alt="Version" src="https://img.shields.io/github/v/release/rafaelkaiz/openBus?label=version&color=f47f30&style=for-the-badge"></a>
  <a href="https://github.com/rafaelkaiz/openBus/actions/workflows/release.yml"><img alt="Build" src="https://img.shields.io/github/actions/workflow/status/rafaelkaiz/openBus/release.yml?branch=main&style=for-the-badge&label=build"></a>
  <a href="LICENSE"><img alt="License" src="https://img.shields.io/github/license/rafaelkaiz/openBus?style=for-the-badge"></a>
</p>

> [!WARNING]
> **Early development. Expect bugs.** Things may be missing, broken or change between
> versions. Please report problems in [Issues](https://github.com/rafaelkaiz/openBus/issues).

**openBus is a modernized OMSI 2.** It keeps full compatibility with native OMSI 2 and gives
it an improved *vanilla* look: the game you know, running on a 64-bit, multithreaded engine
written in Rust, with a modern renderer (Metal / Vulkan / DirectX 12 through wgpu). Your
existing OMSI 2 maps, buses, scenery and mods load as they are.

> [!IMPORTANT]
> **openBus needs an installed copy of OMSI 2.** It contains no game content of its own: it
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
* **Driving as in OMSI 2, and a little more** - timetables, tickets, passengers and AI traffic
  as the original runs them, and a remote cockpit on a phone or tablet.
* **Trams, metros, trains and trolleybuses** - rail vehicles on their own tracks and a real
  overhead wire for trolleybuses ([docs/RAIL.md](docs/RAIL.md),
  [docs/TROLLEYBUS.md](docs/TROLLEYBUS.md)).
* **Multiplayer** - LAN sessions and a dedicated server ([docs/SERVER.md](docs/SERVER.md)).
* **Every platform** - Windows, macOS, Linux and Android; VR through OpenXR on Windows
  ([docs/VR.md](docs/VR.md)).
* **More for modders** - PBR materials, larger textures, more lights, HTML textures, interface
  themes, and plugins in Lua or compiled `.oop` (Lua or WebAssembly), plus OMSI's own plugin
  DLLs on Windows. Plugins can be published in the
  [plugin catalog](https://github.com/openOMSI-org/openOMSI-Plugins).

## openBus and related projects

| Project | What it is |
| --- | --- |
| [openBus](https://github.com/rafaelkaiz/openBus) | the game and its launcher (this repository) |
| [openMapper](https://github.com/openOMSI-org/openMapper) (from openOMSI) | a simple map editor: draw roads, place objects, shape the ground, drive the map in openBus ([docs/OPENMAPPER.md](docs/OPENMAPPER.md)) |
| [openOMSI Plugins](https://github.com/openOMSI-org/openOMSI-Plugins) | the catalog of reviewed, open source Lua plugins |
| [Development Tools](https://github.com/openOMSI-org/openOMSI-Development-Tools) (from openOMSI) | the plugin workbench: projects, API docs, the `.oop` compiler |
| [Add-ons list](docs/ADDONS.md) | projects others make for openBus, shown on the launcher's *Add-ons* page |

## Download

The newest stable version is always at
[**Releases → Latest**](https://github.com/rafaelkaiz/openBus/releases/latest). Newer, untested builds are published as
*Pre-release* (beta) on the [Releases](https://github.com/rafaelkaiz/openBus/releases) page.

| Platform | File |
| --- | --- |
| Windows x64 / ARM64 | `openBus-<version>-windows-x64-setup.exe` / `-windows-arm64-setup.exe` - installs openBus for you, no administrator rights (or the `.zip`: run `openbus.exe`) |
| macOS (Apple silicon / Intel), macOS 11+ | `openBus-<version>-macos-arm64.dmg` / `-macos-x64.dmg` - drag `openBus.app` onto Applications (or the `.zip`) |
| Linux x64 / ARM64 | `openBus-<version>-linux-x64.zip` / `-linux-arm64.zip` - run `openbus` |
| Android 8.0+ (arm64) | `openBus-<version>-android-arm64.apk` - see [docs/ANDROID.md](docs/ANDROID.md) |
| Dedicated server | `openBus-<version>-server-<linux\|windows>-<x64\|arm64>.zip` - see [docs/SERVER.md](docs/SERVER.md) |

`SHA256SUMS` in each release lists the checksums of all files.

The launcher keeps itself up to date: when a newer version is out it offers it at the start
and installs it with your yes. **Settings → General → Updates** chooses the channel (Stable or
Beta), turns the check off or installs without asking.

## Installation

You need **OMSI 2** installed (Steam or retail) with its stock content.

1. **Install** it. On Windows the `-setup.exe` installs openBus into your user folder
   (`%LOCALAPPDATA%\Programs\openBus`) with a Start menu entry; on macOS open the `.dmg` and
   drag `openBus.app` onto Applications. The `.zip` (Linux, or the portable Windows and macOS
   version) goes into a folder of its own that you can write to (your Documents, a games
   folder, or the OMSI 2 folder itself) - not `Program Files`, where the launcher cannot
   update itself.
2. **Start it.**
   * **Windows:** openBus in the Start menu (or `openbus.exe`). SmartScreen may warn about
     an unknown app: *More info* → *Run anyway*.
   * **macOS:** `openBus.app`. If macOS refuses an app from the internet: right-click →
     *Open* → *Open*, or run `xattr -dr com.apple.quarantine /path/to/openBus.app` once.
   * **Linux:** `./openbus` (`chmod +x openbus` first if needed). It needs a Vulkan or
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
* **An older graphics card** - openBus falls back to another graphics API by itself; the same
  setting chooses one by hand.
* **Stuck at a bridge or an invisible wall** on a mod map - Esc → Options → *Collisions with
  objects* off (also under **Settings → Gameplay**).
* **Multiplayer: you do not meet the others** - every player needs the host's map. A map
  installed through the Mods page is passed on; one in the OMSI 2 folder is not.
* **Keys do not do what you set** - the **Controls** page shows which driving keys are in
  use (Settings → Driving → *Driving keys*); a key changed there takes effect at once.
* **Anything else** - when the game ends on an error, the launcher shows it with *Copy report*
  and *Report on GitHub*. Logs are in `~/.openbus` (Windows: `C:\Users\<you>\.openbus`),
  `game.log` for the last game. **Settings → General → Support → Support package** saves a
  support package into `~/.openbus` (`openbus --export-diagnostics <file.zip>` does the same
  from the command line): system, graphics card, driver, settings, controllers, last map and
  bus, and a record of each game of the last 30 days (`~/.openbus/diagnostics`: the day it
  was played, how long and how smoothly, how it ended, the error if it crashed) - no paths,
  names, chat or addresses. Nothing is sent; look inside before you attach it to an issue.

## Goals

1. **Compatibility first.** Every content format of the original loads and behaves as in OMSI
   2.2.032. Existing maps, buses and mods work unchanged, and the OMSI 2 folder is never
   modified.
2. **Vanilla, improved.** The art direction stays that of OMSI 2. Better lighting, shadows,
   textures, anti-aliasing and performance make it look like a cleaner, sharper OMSI 2, not a
   different game. Anything that goes beyond the original is optional.
3. **A modern foundation.** 64-bit, multithreaded, multiplayer and every platform.
4. **No original code or assets.** Nothing from the original is copied; the formats are
   described in [docs/FORMATS.md](docs/FORMATS.md).

## Documentation

The documentation is in [`docs/`](docs):

| Document | What is in it |
| --- | --- |
| [User guide](docs/USER_GUIDE.md) | driving, the launcher, settings, mods, weather, LAN play |
| [Android](docs/ANDROID.md) | the mobile version: install, touch controls, building the APK |
| [Virtual reality](docs/VR.md) | OpenXR headsets on Windows |
| [Dedicated server](docs/SERVER.md) | hosting a multiplayer session without a window |
| [Modding](docs/MODDING.md) | what openBus allows beyond OMSI 2: more lights, larger textures, additions OMSI 2 ignores |
| [PBR materials](docs/PBR.md) | normal, roughness, metalness and occlusion maps |
| [Map packages](docs/MAP_PACKAGE.md) | optional surroundings a map can ship for the navigator and city map |
| [Plugins](docs/PLUGINS.md) | Lua and WebAssembly plugins, `.oop` files, OMSI plugin DLLs |
| [openMapper](docs/OPENMAPPER.md) | the map editor |
| [Trams, metros and trains](docs/RAIL.md) | rail vehicles, tracks and stations |
| [Transport modes](docs/TRANSPORT_MODES.md) | how a vehicle and a map say what kind of transport they are for |
| [Trolleybuses](docs/TROLLEYBUS.md) | the overhead wire and the poles |
| [HTML textures](docs/HTMLTEXTURE.md) | web pages as textures in buses and on maps |
| [Interface themes](docs/THEMES.md) | the look of the game's own screens |
| [Voice chat](docs/VOICE.md) | talking to other players |
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
git clone https://github.com/rafaelkaiz/openBus.git && cd openBus
scripts/build-macos.sh        # macOS   → dist/macos/openBus.app
scripts\build-windows.cmd     # Windows → dist\windows\openbus.exe
scripts/build-linux.sh        # Linux   → dist/linux/openbus
scripts/build-android.sh      # Android → dist/android/openBus-<version>.apk
scripts/build-server.sh       # server  → dist/server
```

Needs the latest [Rust stable](https://rustup.rs) and the platform's C toolchain (on Linux
also the WebKitGTK 4.1 headers for the launcher); details in [docs/BUILDING.md](docs/BUILDING.md).

## Repository layout

```
openBus/
├── crates/        the game, one crate per subsystem (omsi-app is the program `openbus`;
│                  formats, scripts, simulation, renderer, sound, network, plugins, launcher data)
├── launcher/      the launcher window (Tauri; web UI in launcher/ui)
├── android/       the Android app wrapper
├── apple/         the iPhone, iPad and Mac app wrapper (SwiftUI, not released yet)
├── services/      the "playing now" counter
├── tools/         omsi-check (format coverage), avatar and ambience pack builders, voice chat plugin
├── scripts/       build scripts for every platform, version.sh, release notes
├── assets/        fonts, icons, logos
├── docker/        container images of the dedicated server
├── docs/          documentation (also published on the website)
├── site/          the website
├── release/       health.json: released versions known to be broken
└── VERSION        MAJOR.MINOR of the next release
```

## Contributing

Issues and pull requests are welcome - see [CONTRIBUTING.md](CONTRIBUTING.md); how the project is
run, reviewed and released is in [GOVERNANCE.md](GOVERNANCE.md).

## Acknowledgements

Some ideas and fixes in openBus date back to the public work of
[neoOMSI](https://github.com/neoOMSI/neoOMSI): their reports and pull requests pointed at
bugs and features that we then implemented in our own code. Thank you.

## License

openBus is released under the [GNU Affero General Public License, version 3 only](LICENSE)
(`AGPL-3.0-only`). The historical MIT copyright and permission notice is retained in
`LICENSE`; earlier MIT releases keep their terms. Third-party files keep their own licenses.
The launcher's bus preview encodes its pictures with
[jpeg-encoder](https://crates.io/crates/jpeg-encoder): this software is based in part on the
work of the Independent JPEG Group.
openBus is a fork of [openOMSI](https://github.com/openOMSI-org/openOMSI), created by
usonskyyyy and contributors; see [TRADEMARKS.md](TRADEMARKS.md) for the origin of the code and
the use of names and logos. OMSI and OMSI 2 are trademarks of their respective owners. openBus
is an independent project and is not affiliated with them.
