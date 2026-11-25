# Mods

How to install buses, maps and scenery for openOMSI, where they go, which file wins when two
mods have the same one, and the small object editor inside the game. Making your own content is
covered in [Modding overview](MODDING.md).

## The content folder

openOMSI never writes to your OMSI 2 folder. Mods go into a **content folder** of its own:

- beside the game (beside `openOMSI.app` on macOS);
- or `~/.openomsi/content` when that place cannot be written to (for example under Program
  Files).

The **Content folder** button on the Mods page opens it, and **Setup** shows where it is.

The content folder is laid out like an OMSI 2 installation: `Vehicles`, `maps`,
`Sceneryobjects`, `Splines`, `Texture`, `Fonts`, `Plugins`, `TicketPacks`, `Drivers`, `Weather`,
and so on.

### Which file wins

The content folder is searched **before** the OMSI 2 folder. So:

- a mod in the content folder is found exactly as if it had been copied into OMSI 2;
- a file with the same name and path replaces the stock one;
- deleting the copy brings the original back.

Archives used in place (see below) are searched as content folders too.

## Installing a mod

1. Open the launcher's **Mods** page.
2. Press **Choose a folder** or **Choose archive**, and pick a mod folder or a `.zip`, `.7z` or
   `.rar`. You can also drop it onto the launcher window.
3. Wait for the install to finish. It is listed under **Installs**.

The launcher sorts the mod into place:

- OMSI-style folders anywhere inside it (`Vehicles`, `maps`, …) are merged into the content
  folder;
- a lone bus, map, object or spline folder is recognised by its `.bus`, `global.cfg`, `.sco` or
  `.sli` files and put under the right folder.

An install runs in the background. The disk is checked for room before anything is unpacked,
everything is unpacked into a staging folder and moved into place in one step, and a cancelled or
failed install leaves nothing behind.

A repaint for a bus that is not installed waits under **Waiting for their bus**, and is installed
when the bus arrives.

### The Mods folder

Anything put into the `Mods` folder shown on the page is installed by itself once it has finished
copying. This is handy for downloads: save them there and start the launcher.

### Archives used in place

A `.zip` laid out like OMSI 2 can be read by the game without unpacking. It goes into the content
folder's `Archives` folder (hard-linked when on the same disk, moved from the `Mods` folder, else
copied after a free-space check). The launcher's lists see the maps and buses inside it.

**Archive install mode** chooses:

| Mode | What it does |
| --- | --- |
| **Auto** (default) | unpacks what fits on the disk, uses an archive in place when its unpacked size does not |
| **Unpacked** | always unpacks |
| **Used in place** | keeps the `.zip` |

`.7z` and `.rar` archives are always unpacked. The page shows each archive's size, file count and
unpacked size, and whether it fits.

## Managing installed mods

The **Installed mods** list can be searched and filtered (*Buses*, *Maps*, *Archives*, *Other*,
*Off*). For each mod:

- **switch it off**: it stays installed but is not used;
- **delete** it (asks first). Object and spline folders that no other map uses go with a map;
  folders another map still uses stay.

Problems with installed content (missing packs, missing depot files, failed installs) are on
**Help → Troubleshooting** (see [Troubleshooting](TROUBLESHOOTING.md#checking-content)).

## Depot files (HOF)

Depot files (`.hof`) can be placed in a top-level `HOFs` folder of the content folder. Every
vehicle can then use them without a copy in each `Vehicles/<bus>` folder. When a vehicle's folder
and `HOFs` have the same file name, the vehicle's own copy wins. The launcher's depot list shows
the shared ones after the bus's own.

### Sharing a map's depot files

A map names the depot files its buses need in `maps/<map>/ailists.cfg`. When you install a mod map
and some of those files are missing from `HOFs`, the Mods page asks whether to share them:

- **Share with every bus** copies them into `HOFs` from the map's folder or from a bus that
  brought them. Every bus, stock ones included, can then use the map's depot.
- **Not for this map** leaves them where they are. Some maps (Grand Paris Moulon, for example)
  have one depot per company or bus, and buses with their own IBIS show wrong destinations with
  another one.

Without the copy, a bus that lacks a depot still finds it in the map's folder, and when you choose
a line the launcher picks the bus's or the map's depot that carries it. The copy belongs to the
map's mod: switching the map off or removing it removes the copy. A depot file the map needs that
no installed content provides shows as a warning on the Mods page.

**Add HOF files to buses** (desktop launcher) copies `.hof` files you choose into `HOFs`. Files
already there are left untouched.

## Multiplayer and mods

A LAN host passes the mods of its map and buses to the players who join. A map installed through
the Mods page is passed on; one copied into the OMSI 2 folder is not. See
[Playing together](MULTIPLAYER.md).

## Object editor

A small part of OMSI's map editor, inside the game. Press **Ctrl+Shift+E** (or choose *Object
editor* under **World options** in the game menu) to turn it on. The free camera starts where the
view is.

### Objects

| Key / mouse | What it does |
| --- | --- |
| `Enter` or click | pick the scenery object nearest the middle of the view (a magenta glow marks it) |
| `Tab` | pick the next nearest |
| `I` / `K` / `J` / `L` | move it forward / back / left / right, as the camera faces |
| `U` / `O` | lower / raise it |
| `N` / `M` | turn it |
| `Shift` with these | a tenth of the step (steps are half a metre and five degrees) |
| drag / wheel / `Shift+wheel` | move / turn / raise or lower it |
| `Delete` | delete it (again: back) |
| `Backspace` | undo everything done to it |
| `C` | copy the picked object; the copy is then the one edited |
| `V` | give the copy the shape of the next object of the same folder |

### Ground

| Key | What it does |
| --- | --- |
| `Page Up` / `Page Down` | raise / lower the ground under the middle of the view (a quarter metre; a twentieth with Shift) |
| `F` | flatten it to the height at the middle |
| `[` / `]` | smaller / larger brush |

### Saving

**Ctrl+S** saves; **Esc** leaves the editor (unsaved changes stay until the end of the session).

Saving writes each changed tile as a copy into the content folder's map folder
(`<content>/maps/<map>/tile_x_y.map`, the ground as its `.map.terrain` copy). The game reads the
copy before the installation. The original map is never written; delete the copy to get it back.

Splines are not edited here. The timetable is edited on the launcher's **Timetable** page. In a
LAN session only the host edits the map, and the edits reach the other players.

For bigger changes to a map use openMapper: [openMapper](MAPPER.md).
