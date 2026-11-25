# Troubleshooting

What to do when openOMSI does not start, runs slowly or behaves oddly, where the logs are, how to
check your content, and how to report a bug.

## Common problems

| Problem | What to do |
| --- | --- |
| *OMSI 2 was not found* | open **Setup**, press **Browse…** beside *OMSI 2 folder*, choose the folder with `Omsi.exe`, `maps` and `Vehicles`, press **Save**. When it is not saved, the message says what the folder lacks |
| The game closes after a few seconds, or *the graphics device was lost* | update the graphics driver from NVIDIA, AMD or Intel (not the one Windows installs). On Windows try another **Settings → Graphics → Graphics API** |
| An older graphics card | the game falls back to another graphics API by itself; the same setting chooses one by hand (see [Graphics](GRAPHICS.md#graphics-api)) |
| Stuck at a bridge or an invisible wall on a mod map | **Esc → Options →** *Collisions with objects* off (also in **Settings → Gameplay**) |
| Keys do not do what you set | the **Controls** page says which driving keys preset is in use (Settings → Driving → *Driving keys*) |
| The bus does not move | read the notes in the top-left corner: parking brake, air pressure, gearbox. See [Driving](DRIVING.md#starting-a-bus) |
| Multiplayer: you do not meet the others | everyone needs the host's map; a map installed through the Mods page is passed on, one in the OMSI 2 folder is not. See [Playing together](MULTIPLAYER.md) |
| A bus or a map shows errors | check it with the Model test (see [Checking content](#checking-content)) |
| DLSS or FSR 3 is not offered | see [Graphics](GRAPHICS.md#upscaling): Windows, DirectX 12 or Vulkan, and the vendor's DLLs beside the game |
| No radio | see [Radio](RADIO.md#when-nothing-plays) |
| A plugin feature is missing | **Settings → General → Disable plugins** is on by default for new installations |

## Performance

1. Choose a lower **Quality preset** in **Settings → Graphics** (see
   [Graphics](GRAPHICS.md#quality-presets)).
2. Or press **Optimise for my PC**: during your next drive the game measures for about a minute
   and adjusts the graphics to reach the frame rate you aim at. Your settings are saved first;
   **Restore my previous settings** brings them back.
3. To see where the time goes, switch on **Settings → General → Frame rate in the corner** and
   **Performance details** (*Compact* or *Detailed*).

The options that cost most are Real-Time Ray Tracing, *Every light* shadows, 8x anti-aliasing,
long view and object distances, *Every frame* mirrors, and rain and puddles on a weak card. The
launcher marks them **Heavy**.

**Video memory** (Settings → Graphics) is how much of the graphics card the game may use in all.
*Automatic* is nine tenths of the card. Big maps stay within it through compressed textures and
tiles that free everything when they unload.

For exact measurements, see [Command line](COMMAND_LINE.md#measuring-performance).

## A version does not start

openOMSI repairs a start that breaks on its own. When started (a double click, the launcher), it
first starts a small watcher without a window. If the launcher crashes, ends on an error, or has
not drawn its first frames after two minutes, the watcher steps in. It does so when that version
never started on this computer, or failed twice in a row:

1. A newer release, when there is one, is downloaded and installed (its program must start before
   it replaces anything), and openOMSI starts again.
2. Else the version the last update replaced is put back. It is kept until the new version has
   started well once.
3. Else openOMSI starts once more in **safe mode**.
4. Else a message says what to do: download the latest release, and where the log is.

Each step is told in a message box. A version that failed is never installed again on this
computer, only a newer one. The project can also list a released version as broken; such a
version goes back to the one before, or on to a fixed one, at the next start.

A crash in the middle of a drive is not a broken start: nothing is changed then.

The watcher keeps its record in `~/.openomsi/guard/` (`guard.log`, `starts.json`,
`bad-versions`). On Android the next start after one that never reached the launcher looks for an
update at once; going back is the system's business there.

### Safe mode

Safe mode starts the game with the basic render pipelines and no plugins. The watcher uses it by
itself after failed starts. If the game runs in safe mode, try in turn:

- **Settings → General → Disable plugins** on;
- another **Graphics API**;
- **Graphics** set to *Vanilla*;
- **mX Graphics** off.

### When the launcher window does not open

- **Windows**: the launcher needs Microsoft WebView2. On an old Windows without it, the game offers
  Microsoft's installer.
- **Linux**: it needs WebKitGTK 4.1 (see [Linux and Steam Deck](#linux-and-steam-deck)). The game
  says so in a desktop dialog and in the terminal.

After a crash the launcher may offer **Use DirectX 12** (Windows) to draw with DirectX 12 from the
next start; **Settings → Graphics API** changes it back.

## Linux and Steam Deck

The launcher window needs **WebKitGTK 4.1**:

| Distribution | Package |
| --- | --- |
| Debian, Ubuntu | `libwebkit2gtk-4.1-0` |
| Fedora | `webkit2gtk4.1` |
| Arch | `webkit2gtk-4.1` |

The game needs a Vulkan or OpenGL driver (Mesa's `mesa-vulkan-drivers`, or your vendor's).

**SteamOS** (Steam Deck) does not include WebKitGTK and its system is read-only. In Desktop Mode:

1. Open Konsole. If you have no password yet, set one with `passwd`.
2. Run:

   ```bash
   sudo steamos-readonly disable
   sudo pacman -S webkit2gtk-4.1
   sudo steamos-readonly enable
   ```

3. If pacman complains about keys, first run `sudo pacman-key --init` and
   `sudo pacman-key --populate archlinux holo`.

A SteamOS update can remove the package again; then run the same commands once more.

## Logs

All in `~/.openomsi` (on Windows `C:\Users\<you>\.openomsi`):

| File | What it holds |
| --- | --- |
| `game.log` | the last game |
| `launcher.log` | the launcher |
| `guard/guard.log` | the start watcher |
| `diagnostics/` | a record of each game of the last 30 days: when, how long, how smoothly, how it ended |

The first line of `game.log` and `launcher.log` is the build they were made from. **Running
games → Show the log file** opens a running or recently ended game's log.

## Checking content

### In the launcher

**Help → Troubleshooting** (not on phones):

- **Overview** lists problems by content: packs waiting for their bus, buses missing parts, map
  depot files, failed installs. Install what is missing, then scan again.
- **Model test** loads anything the way the game loads it: a bus with its coupled parts, a map
  with every tile, object and spline, an installed mod, or any file or folder (`.bus`, `.ovh`,
  `.sco`, `.sli`, `.hum`, `.owt`, `.o3d`, a `.zip`). Every warning and error is listed with what
  it names that is not there.
  1. Choose what to check (*Bus*, *Map*, *Installed mod*, *File or folder*).
  2. Press **Check**. Tick *Load every object and spline of a map whole* for a deeper, slower check.
  3. Fix the content and press **Check** again.

  *Live, from the running game* shows the warnings of a game you started on the Drive page as they
  happen.

### From the command line

```bash
openomsi --check "Vehicles/MAN_SD200/MAN_SD80.bus"
openomsi --check maps/Grundorf --root "/path/to/OMSI 2" --deep
```

The findings go to the terminal as one JSON line. No window opens. See
[Command line](COMMAND_LINE.md#checking-content).

### A whole installation

When you build openOMSI from source, `omsi-check` loads every content file of an installation and
reports what failed (see [Building from source](BUILDING.md)):

```bash
cargo run --release -p omsi-check -- "/path/to/OMSI 2"
```

## Reporting a bug

1. When the game ends on an error, the launcher shows it with **Copy report** and **Report on
   GitHub**.
2. Make a **support package**: **Settings → General → Support → Support package**. It saves a ZIP
   into `~/.openomsi` with the system, graphics card, driver, settings, controllers, last map and
   bus, and the game records of the last 30 days. No paths, names, chat or addresses. Nothing is
   sent. `openomsi --export-diagnostics <file.zip>` makes one without starting the game.
3. Look inside the ZIP, then attach it to an
   [issue on GitHub](https://github.com/openOMSI-org/openOMSI/issues), or ask on
   [Discord](https://discord.gg/openomsi).

Never attach an API key (such as your Tomorrow.io key) to a report.

## Clearing the cache

**Help → Troubleshooting → Disk cache and multiplayer downloads** shows the size of the local cache
(maps, processed textures, other rebuildable data) and of multiplayer downloads. Close all games
first, then **Clean to limits**, **Clear local cache…** or **Clear multiplayer downloads…**. Mods,
saves and settings are not removed.
