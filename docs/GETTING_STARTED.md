# Getting started

This page takes you from nothing to driving your first bus in openOMSI. Read it once from top to
bottom: it covers what you need, the download, the first start of the launcher and a first drive,
key by key.

## What you need

- **An installed copy of OMSI 2** (Steam or retail) with its stock content. openOMSI has no game
  content of its own: it plays the maps, buses and mods of your OMSI 2 folder. Without one it does
  not start. Your OMSI 2 folder is never written to.
- A computer with Windows, macOS or Linux, a graphics card with Vulkan, DirectX 12, Metal or
  OpenGL, and a keyboard. A mouse, a wheel or a gamepad are optional.
- For phones and tablets see [Android](ANDROID.md) and [iPhone, iPad and Mac](APPLE.md).

## Download

Get the newest stable version from **[openomsi.org/download](https://openomsi.org/download/)**
(the same files are on GitHub under
[Releases → Latest](https://github.com/openOMSI-org/openOMSI/releases/latest)).

| Platform | File to take |
| --- | --- |
| Windows x64 / ARM64 | `openOMSI-<version>-windows-x64-setup.exe` (or `-arm64-setup.exe`) |
| macOS 11+ (Apple silicon / Intel) | `openOMSI-<version>-macos-arm64.dmg` (or `-x64.dmg`) |
| Linux x64 / ARM64 | `openOMSI-<version>-linux-x64.zip` (or `-arm64.zip`) |
| Android 8.0+ | `openOMSI-<version>-android-arm64.apk`, see [Android](ANDROID.md) |

Windows and macOS also have a portable `.zip`. Each release lists the checksums of its files in
`SHA256SUMS`.

## Install

### Windows

1. Run the `-setup.exe`. It installs openOMSI into your user folder
   (`%LOCALAPPDATA%\Programs\openOMSI`) with a Start menu entry. No administrator rights are needed.
2. Start **openOMSI** from the Start menu.
3. If SmartScreen warns about an unknown app, choose **More info → Run anyway**.

With the portable `.zip`, unpack it into a folder you can write to (Documents, a games folder, or
the OMSI 2 folder itself) and run `openomsi.exe`. Do not use `Program Files`: the launcher cannot
update itself there.

### macOS

1. Open the `.dmg` and drag `openOMSI.app` onto **Applications**.
2. Start `openOMSI.app`.
3. If macOS refuses an app from the internet, right-click it, choose **Open**, then **Open** again.

### Linux

1. Unpack the `.zip` into a folder of its own.
2. Run `./openomsi` (first `chmod +x openomsi` if needed).

The game needs a Vulkan or OpenGL driver (Mesa's `mesa-vulkan-drivers`, or your graphics vendor's
driver). The launcher window needs **WebKitGTK 4.1**: `libwebkit2gtk-4.1-0` on Debian and Ubuntu,
`webkit2gtk4.1` on Fedora, `webkit2gtk-4.1` on Arch. For the Steam Deck see
[Troubleshooting](TROUBLESHOOTING.md#linux-and-steam-deck).

### Phones, tablets and the Mac App

Android: [Android](ANDROID.md). iPhone, iPad and the Apple app: [iPhone, iPad and Mac](APPLE.md).

## First start

When openOMSI starts without arguments it opens the **launcher**: a window with a menu on the left
and a page beside it.

1. The launcher looks for OMSI 2 by itself (Steam libraries, the usual folders, Wine bottles).
   When it finds it, the **Drive** page opens.
2. When it does not, the **Setup** page opens with *OMSI 2 was not found*. Then:
   1. Press **Browse…** beside **OMSI 2 folder**.
   2. Pick the folder with `Omsi.exe`, `maps` and `Vehicles` in it. On Steam this is
      `…\Steam\steamapps\common\OMSI 2`.
   3. Press **Save**. The launcher says *OMSI 2 was found.*, or what the chosen folder lacks.
3. A new version may be offered in a dialog. **Update now** installs it and restarts the launcher;
   your mods, content and settings stay.

> **Tip:** The folder can also be given as a path with quotes, as `Omsi.exe` itself, or as a
> folder inside the OMSI 2 folder. All of these work.

## Your first drive

The **Drive** page puts a drive together. Its look depends on the layout (Big Map by default, see
[Launcher and settings](LAUNCHER.md#drive)), but the parts are the same.

1. **Map.** Choose a map, for example *Grundorf* or *Spandau* from OMSI 2.
2. **Line and tour, or a free drive.** Under *Map & duty*, choose **Timetable** to drive a line:
   pick a **Line**, then a **Tour**. For a first try, **Free drive** is simpler: the bus starts at
   the entry point you choose, with no timetable to keep.
3. **Bus.** Under *Bus*, choose a bus and its livery. The stock **MAN SD200** is a good first bus.
   The picture is drawn by the game itself; drag it to turn the bus, scroll to zoom.
4. **Time and weather.** Under *Day & weather*, set the **Start time** and **Date**. Daylight helps
   on a first drive. Leave the weather on **Natural weather**.
5. **Traffic and passengers.** *Cars around* sets the AI traffic. Switch **Passengers** on to
   have people at the stops. Switch on **Bus ready to go** to start with the engine already
   running.
6. Press **Start the duty** (or **Start driving** for a free drive). The game window opens and
   loads the map.

## Getting the bus moving

openOMSI uses the **W A S D + arrows** keys by default: **W** or **Up** is the throttle, **S** or
**Down** the brake, **A** and **D** steer. The other keys are OMSI 2's own.

If you chose **Bus ready to go**, skip to step 4.

1. Press **E**: the battery main switch. On the stock buses it also puts the ignition key in.
2. Hold **M** for about a second: the starter. The engine runs.
3. Press **Shift+D**: the automatic gearbox to D (drive). (Plain **D** steers right; hold Shift to
   reach OMSI's D key.)
4. Press **.** (full stop): the parking brake comes off.
5. Hold **W**: the bus moves. **A** and **D** steer, **S** brakes.

> **Tip:** **Shift+U** does the whole start-up for you (main switch, ignition, starter, gearbox to
> neutral) and types the duty into the IBIS. Then **Shift+D** and **.** and you are off.

At a stop:

1. Brake to a standstill with **S**.
2. Open the doors with **Shift+1** (front door), **Shift+2** (next door), and so on.
3. Sell tickets when a passenger asks: **T**, on a bus without a ticket printer.
4. Close the doors with the same keys and drive on.

Useful while driving:

| Key | What it does |
| --- | --- |
| `F1` / `F2` / `F3` | driver's view / passenger view / outside view |
| `Z` / `X` / `C` | left indicator / hazard lights / right indicator |
| `L` | headlights |
| `O` | mouse steering on and off |
| `Esc` | game menu (pause, options, end the session) |
| `Ctrl+S` | quick save |

Every key is in [Driving](DRIVING.md).

## Where to go next

- [Driving](DRIVING.md): every key, the cameras, the doors and tickets, the IBIS, photo mode.
- [Launcher and settings](LAUNCHER.md): every launcher page and every setting.
- [Graphics](GRAPHICS.md): the graphics modes and how to get more frames per second.
- [Mods](MODS.md): installing buses and maps.
- [Playing together](MULTIPLAYER.md): driving with friends.
- [Troubleshooting](TROUBLESHOOTING.md): when something does not work.
