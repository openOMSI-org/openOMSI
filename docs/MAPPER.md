# openMapper

openMapper is the map editor for openOMSI. You draw roads with two clicks, place buildings and trees
from a catalogue, shape and paint the ground, and drive the map in openOMSI with one key. This page
is for anyone who wants to build or change a map: it covers installing, the first start and the
editor's window.

> **Warning:** openMapper is an early release. Things may be missing, broken or change between
> versions. Report problems on [GitHub](https://github.com/openOMSI-org/openMapper/issues) or on
> [Discord](https://discord.gg/openomsi). Keep a copy of a map you care about: saving keeps the last
> ten versions, but a copy is safer.

## What it can do

- **Roads and junctions:** two clicks make a road. Junctions, traffic lights, zebra crossings,
  switches and roundabouts are made for you. Bus pockets, complex junctions and your own road types
  have their own tools.
- **Objects and plants:** a catalogue of every building, tree and spline in your OMSI 2 install and
  your mods, objects that line up with the road, and a plant brush.
- **Terrain and paint:** sculpting brushes, ramps, height maps, ground textures, lakes, rivers and
  tiles.
- **Real places:** satellite imagery, terrain heights and OpenStreetMap data (roads, stops, lines,
  buildings, trees, water).
- **Lines and traffic:** lines, trips, tours, depot files, AI traffic and AI paths.
- **Your own models:** import glTF, FBX, OBJ and more, edit meshes, make materials and PBR maps,
  export.
- **Scripts:** the map's Lua scripts for openOMSI, with an editor and a debugger (**Map → Scripts…**).
- **Trolleybus wire:** overhead wire with sections, insulators and switches (**Map → Trolleybus
  wire…**).
- **Check and drive:** a map check, the game's own checks, safe saving with earlier versions, and a
  test drive in openOMSI on `F5`.

## What you need

- **OMSI 2** (Steam or retail), installed. openMapper builds maps from its content. It never writes
  into the OMSI 2 folder.
- **openOMSI**, for test drives and the in-game check. openMapper finds it beside itself, through
  openOMSI's launcher, or (on macOS) in Applications. See [Getting started](GETTING_STARTED.md) to
  install it.
- A graphics card with Direct3D 12, Vulkan, Metal or OpenGL.

## Download and install

Every release is on the [Releases page](https://github.com/openOMSI-org/openMapper/releases/latest),
built for each system and processor:

| System | Intel / AMD (x64) | ARM64 |
| --- | --- | --- |
| Windows 10 / 11 | `openMapper-<version>-windows-x64-setup.exe` or `.zip` | `openMapper-<version>-windows-arm64-setup.exe` or `.zip` |
| macOS 11 or newer | `openMapper-<version>-macos-x64.dmg` or `.zip` | `openMapper-<version>-macos-arm64.dmg` or `.zip` (Apple silicon) |
| Linux | `openMapper-<version>-linux-x64.tar.gz` | `openMapper-<version>-linux-arm64.tar.gz` |

### Windows

1. Run the `-setup.exe`. It installs for your user only (no administrator rights needed) and adds a
   Start menu entry and a desktop shortcut.
2. To remove it later, use **Settings → Apps** in Windows.

Or unpack the `.zip` anywhere and run `openmapper.exe`. Nothing else is needed.

### macOS

1. Open the `.dmg` and drag **openMapper** onto **Applications**. The `.zip` holds the same
   `openMapper.app`.
2. The app is not notarised. The first time, right-click it and choose **Open** (or go to **System
   Settings → Privacy & Security → Open Anyway**).

### Linux

1. Unpack the archive and run `./openmapper`.
2. For a menu entry, use `openmapper.desktop` and the icon in the same folder.

openMapper needs GTK 3 (for file dialogs) and a Vulkan or OpenGL driver.

### Updates

openMapper looks for a new version when it starts. **Help → Updates…** shows the installed version
and lets you choose what happens when there is a new one:

| Choice | What it does |
| --- | --- |
| **Offer it to me** | Asks each time: **Install and restart** or **Not now** (the default). |
| **Install it and restart by itself** | Installs without asking. |
| **Never look for updates** | No checks. **Check now** still works. |

The download is checked against the release's checksums before the program is replaced. Where there
is no automatic update for your system, **Download it from GitHub by hand** opens the Releases page.

## First start

1. Start openMapper. The start screen opens.
2. If it says **OMSI 2 not found**, click **Choose OMSI 2 folder…** and pick the folder that holds
   `Omsi.exe`, `maps` and `Vehicles`. openMapper otherwise finds it by itself: the folder openOMSI's
   launcher uses, or the usual Steam places. You can change it later with **File → OMSI 2 folder…**
   on the start screen.
3. Pick a theme under **Theme** (dark, light, or as the system).
4. Choose what to do:
   - **New map…** starts an empty map. Follow [Your first map](MAPPER_FIRST_MAP.md).
   - Double-click a map in the **Maps** list (or select it and click **Open**) to edit it.
   - **Browse…** opens a `global.cfg` from anywhere.
   - **Import assets…** brings in a mod, a model or a picture.

The **Maps** list shows recent maps first, then every map of the content folder and of OMSI 2.
Right-click a map for **Make a copy** (a copy under a new name, to try something out), **Show in the
file manager**, or **Move to openMapper's bin** (content folder maps only: the folder goes to
`openMapper\bin\maps`, from where you can put it back by hand).

> **Note:** a map of OMSI 2 itself is saved as a copy into openOMSI's content folder. The original
> stays untouched.

## The window

From top to bottom: the menu bar, the toolbar, then the tool panel (left), the view (middle) and
the properties (right), and the status bar at the bottom.

### Menu bar

| Menu | What is in it |
| --- | --- |
| **File** | New map, open, recent maps, save, reload from disk, export for sharing, export a plan, import assets, earlier versions, multiplayer, close, quit |
| **Edit** | Undo, redo, cut, copy, paste, duplicate, edit history, groups for other maps, find in map, search for a command, select all, delete, deselect |
| **View** | Tile grid, helpers, AI lanes, names, layers, overview map, assets, satellite overlay, saved views, camera, keys, time of day, season, graphics, language, interface size, theme |
| **Tools** | The seven tools, road types and junctions, complex junction editor, crossing editor, PBR maps for a picture |
| **Map** | Map settings, scripts, calendar and place, signal routes, trolleybus wire, chrono scenarios, ground textures, check map, lines and timetable, traffic and AI, depot files, satellite and terrain, OpenStreetMap, GPS tracks, tiles from another map, test drives, check in openOMSI |
| **Help** | Getting started, updates, about |

Every menu entry and many panel actions are also in the command search: press `Ctrl+K` (or click
**Search commands** in the toolbar), type a word and press `Enter`.

### Toolbar

From left to right:

- **New map**, **Open a map**, **Save** (a dot on it means unsaved changes), **Import assets**.
- **Undo** and **Redo**.
- The seven tools, each on a number key:

  | Key | Tool | What it is for |
  | --- | --- | --- |
  | `1` | **Select** | Pick roads and objects and change them in the properties |
  | `2` | **Roads** | Draw roads, tracks, footpaths and other splines |
  | `3` | **Objects** | Place buildings, trees, signs; the plant brush; shapes and lamps |
  | `4` | **Game logic** | Start points, bus stops, car parks, AI paths and other helpers the game does not show |
  | `5` | **Terrain** | Raise, lower, smooth and flatten the ground; ramps; lakes and rivers |
  | `6` | **Paint** | Paint ground textures |
  | `7` | **Tiles** | Add and remove 300 × 300 m tiles; water on a tile |

- Next to **Select**: the **Move**, **Rotate** and **Scale** handles (`V`, `C`, `X`) and the vertex
  and edge snap buttons (`Shift+V`, `Shift+L`).
- **Tile grid** (`G`) and **Assets** (`B`).
- On the right: **Search commands** (`Ctrl+K`), the map's name, and **Test drive** (`F5`). While a
  test drive runs, the button reads **Close test drive**. Right-click it for **Test drive from the
  start point nearest the view** (`Shift+F5`) and **See this view in openOMSI**.

### Tool panel (left)

The panel shows the active tool's options. For **Roads**, **Objects** and **Game logic** it holds
the catalogue: everything in your OMSI 2 install and content folder, with pictures.

- **Show** filters by kind (Roads, Tracks, Footpaths, Trees, Junctions and so on).
- **Search** finds by name. The dropper button next to it picks the type of something you click in
  the view.
- **Favourites** and **Recently used** sit at the top. Right-click an entry to star it.

With **Select**, the panel has two tabs: **Tools** and **Objects**. **Objects** lists everything on
the map, grouped by the folders its files lie in.

### Properties (right)

Click something with **Select** and its properties appear on the right: position, rotation, size,
type, texts, rows of objects along a road, a road's height profile, a junction's lights, and more.
With nothing selected, the panel shows the map's own properties (its picture, the view it opens
with, its numbers).

The full list of what each panel holds is in [openMapper tools](MAPPER_TOOLS.md).

### The view

- The round **2D / 3D** button at the top right switches to a plan from above, north up (`0`).
- The overview map (`M`) sits in a corner. Click or drag on it to move there; double-click for a
  larger one.
- While you draw a road, fields for **Length**, **Radius** and **Heading** appear in the bottom-left
  corner.
- Right-click in the view (with **Select**) for a menu of what can be done there.

Moving the camera (the default, Unity-style):

| Action | Mouse or key |
| --- | --- |
| Look round and fly | Hold the right button, move the mouse, `W` `A` `S` `D` (`Q` / `E` down / up, `Shift` faster) |
| Pan | Middle button drag |
| Zoom | Wheel (towards the cursor) |
| Orbit | `Alt` + left drag |
| Move without the mouse | `W` `A` `S` `D` or the arrow keys; `Q` / `E` turn |
| Look straight down | `Home` |
| Go to the selection | `F` |

On a trackpad, two fingers pan, pinch zooms and twist turns. **View → Camera** switches to a map
editor style camera (dragging the ground moves the map). **View → Preset Key Shortcut (from other
applications)** gives you OMSI 2's editor or Blender's keys. Every key is in
[openMapper keys](MAPPER_KEYS.md).

### Status bar

From left to right:

- **Help line:** what the thing under the mouse does, or the newest message. When idle it reminds
  you that right-click and **Search commands** exist.
- **Switches:** **Angle** (snap headings, also what `R` turns by), **Grid** (snap positions),
  **Knife** (`K`, cut roads), **Ruler** (`U`, measure), **Auto-Terrain**, **Auto-Switch**,
  **Auto-Junction**, **Auto-Adapt**, **Surface** and **Auto-Align**. In a narrow window they fold
  into a `…` menu.
- **Cursor:** X, Y, Z and the tile under the mouse.
- **Counts:** tiles, splines and objects on the map.
- **Graphics:** the graphics card and API in use.
- **Clock:** the view's time of day. Click it to let time run or stop it.

### Assets panel

**Assets** (`B`) opens a larger catalogue, like Unity's project window: folders, a grid of
pictures, search, a kind filter and picture size. Double-click an asset to use it, or drag its
picture into the view to place it. Right-click for its facts, **Place it** / **Build with it**,
favourites, **PBR material…**, **Show in the file manager** and **Delete…** (content folder assets
only; they go to `openMapper\bin`).

### Settings

Press `Ctrl+,` (or search for **Settings…**) to open **Settings**:

| Tab | What you set |
| --- | --- |
| **General** | Language, theme, interface size, camera and mouse style, big maps (load only part of the tiles), graphics API |
| **Graphics** | Graphics quality |
| **Camera** | Flying speed, `Shift` speed-up, turning speed, sensitivity, inverted axes, reset |
| **Keys** | The key set (openMapper, OMSI 2's editor, Blender) and every key, each one changeable |
| **Mouse** | What each mouse and trackpad action does |

The interface comes in the same 28 languages as openOMSI (**View → Language**). Interface size goes
from 80 % to 150 %.

## How a map is saved

Press `Ctrl+S` (or **File → Save**). openMapper writes OMSI 2's own format, so OMSI 2 and openOMSI
both read the map.

- **Where:** in openOMSI's content folder, `maps\<map folder>`. The content folder is normally
  `~/.openomsi/content`, or the `content` folder beside the openOMSI program when the launcher uses
  one. The game reads it before the OMSI 2 install. See [Mods](MODS.md) for the content folder.
- **OMSI 2's own maps** are saved as a copy there. The original is never changed.
- **Junctions and other pieces openMapper makes** go into `Sceneryobjects\openMapper\<map>` of the
  content folder, as ordinary OMSI content.
- **Imported models** go into `Sceneryobjects\openMapper\Imported`.
- **Safe saving:** every file is written under a temporary name first. A crash or a full disk leaves
  the map as it was.
- **Earlier versions:** the files each save replaces are kept for the last 10 saves, in
  `openMapper\backups\<map>` of the content folder. **File → Earlier versions…** puts the map back.
- **Autosave:** your changes are kept every 3 minutes in `openMapper\autosave` of the content
  folder (never in the map). If openMapper ends without saving, it offers them at the next start.
- **openMapper's own data** for a map (imagery, groups) lies in an `openMapper` folder inside the
  map's folder. The game ignores it.

More on saving, versions and recovering a map is in
[openMapper troubleshooting](MAPPER_TROUBLESHOOTING.md).

## Tutorials

Work through them in order; each one builds on the one before.

1. [Your first map](MAPPER_FIRST_MAP.md): from an empty map to driving it in openOMSI.
2. [A real place](MAPPER_REAL_PLACE.md): satellite imagery, terrain heights and OpenStreetMap.
3. [Lines and traffic](MAPPER_LINES.md): stops, lines, timetable, tours, depot files, AI traffic.
4. [Terrain](MAPPER_TERRAIN.md): sculpting, height maps, ground textures, water and tiles.
5. [Your own models](MAPPER_MODELS.md): importing, mesh editing, materials, PBR and export.

## Reference

- [openMapper tools](MAPPER_TOOLS.md): every tool, panel and window.
- [openMapper keys](MAPPER_KEYS.md): every key and mouse action, and how to change them.
- [openMapper troubleshooting](MAPPER_TROUBLESHOOTING.md): the map check, the game check, saving and
  versions, editing a map together, common problems.

Source code: [openMapper on GitHub](https://github.com/openOMSI-org/openMapper), with the
[changelog](https://github.com/openOMSI-org/openMapper/blob/main/CHANGELOG.md).

## Details

### Command line

| Command | What it does |
| --- | --- |
| `openmapper path/to/global.cfg` | Opens that map straight away |
| `openmapper --check path/to/global.cfg` | Prints the map check, no window |
| `openmapper --export global.cfg out.zip` | Exports the map for sharing |
| `openmapper --import file.zip` | Installs a shared map or mod |
| `openmapper --plan global.cfg out.svg` | Writes a plan of the map as SVG |
| `openmapper --mcp [global.cfg] [--allow-edit]` | Runs a Model Context Protocol server on stdin / stdout, so an AI agent can read (and with `--allow-edit`, edit) the map |

### Files on your computer

| What | Where |
| --- | --- |
| openMapper's settings | `~/.openomsi/openmapper.json` |
| Downloaded imagery and heights (cache) | `~/.openomsi/cache/openmapper/tiles` |
| Pictures of the view (`F12`) | `~/Pictures/openMapper` |

### Graphics API

**View → Graphics API** (or **Settings → General**) picks Direct3D 12, Vulkan, Metal or OpenGL.
DirectX 12 is the usual choice on Windows. If the chosen API cannot start, openMapper falls back to
the automatic choice. The change takes effect after a restart. For a single run, set
`OPENMAPPER_BACKEND=dx12` (or another API).

openMapper is free software under the GNU AGPL v3 (`AGPL-3.0-only`), like openOMSI.
