# openMapper troubleshooting

What to do when a map does not work in the game, how openMapper saves and keeps earlier versions, how several people edit one map, and how to report a bug. For the tools themselves, see [Tools reference](MAPPER_TOOLS.md).

> **Warning:** openMapper is an early release. Keep a copy of a map you care about. Saving keeps the last ten versions, but a copy is cheaper than a regret.

## Map check

**Map → Check map…** lists what would go wrong in the game. It runs in the background; **Check again** runs it once more.

1. Open **Map → Check map…**.
2. Read each issue. Hover it for what it means.
3. Click an issue to fly there.
4. Press **Fix** (or **Join**) where offered, or **Fix all**. Fixes can be undone.

Each issue has a level: an **error** (the game will not load or show it right), a **warning** (probably not what was meant) or a **note** (worth knowing). `openmapper --check path/to/global.cfg` prints the same without a window.

### Content and files

| Check | What it means | How to fix |
| --- | --- | --- |
| **Missing objects** (error) | The map uses an object (`.sco`) that is not installed. The game leaves it out | Install the mod it comes from, or delete the object. An openMapper junction whose file is gone (the map was moved without it) is made again by **Fix** |
| **Missing roads and splines** (error) | A road or spline type (`.sli`) is not installed. That road is missing in the game and buses cannot drive there | Install the mod, or replace the road type (**Find in map → Replace with**) |
| **Missing textures** (warning) | An object, road or ground layer names a picture that cannot be found. It looks white or black in the game | Install the texture, or change the object's or layer's texture |
| **Tiles without a file** (note) | `global.cfg` lists tiles whose file is not there. The game leaves them out | Nothing: they stay listed, because start points and timetables count tiles by their place in the list |
| **Tiles without ground** (note) | A tile has no height file: its ground is flat | Save: saving writes one |

### Roads and links

| Check | What it means | How to fix |
| --- | --- | --- |
| **Road ends not joined** (note) | Two road ends lie on one another but are not joined. Traffic still finds its way, but rows of lamps and posts and the road texture do not run on across, and the editor does not treat them as one road | **Join** links them |
| **Links to splines that are gone** (warning) | A spline says it goes on into one that no longer exists | **Fix** forgets that link |
| **Rows the game does not show** (warning) | A row of objects along a road starts before the road it is laid on (its distance counts from the start of the road's chain). OMSI 2, openOMSI and the editor show none of it | Lay the row again, or give it a larger start in its properties |

### Stops and start points

| Check | What it means | How to fix |
| --- | --- | --- |
| **Bus stops away from a road** (warning) | A stop belongs to the lane it stands beside, within about 25 m. This one has none near, so buses cannot stop at it | Move it next to the road, or **Fix** puts it at the kerb of the nearest lane (within 80 m) |
| **Stops sharing a name** (warning) | Two stops far apart have one name. A pair across a road may share it; elsewhere the game, the displays and the passengers cannot tell them apart | **Fix** gives the later one a number after its name |
| **Start points away from a road** (note) | A start point is away from any lane. Fine in a depot yard; otherwise the bus starts off the road | Move it onto a lane |
| **Start points without their object** (error) | A start point refers to a deleted object | **Fix** removes the start point |
| **No start point** (warning) | There is nowhere for the bus to appear | Place one (**Game logic → Start points**) on a road, or **Fix** puts one on a lane some 40 m before a bus stop |

### IDCodes and tiles

| Check | What it means | How to fix |
| --- | --- | --- |
| **IDCodes used twice** (warning) | Two things share one IDCode: links, timetables and start points may pick the wrong one | Delete one and place it again |
| **Next IDCode too low** (error) | The counter for new IDCodes is below codes already used, so new objects would get codes twice | **Fix** raises it |
| **Objects off their tile** (warning) | An object standing on the ground lies outside its own tile. The game does not show it | Move it a little, or put it again |
| **Tiles with very many objects** (note) | A tile holds far more objects than the stock maps' tiles (up to some 600). The game may stutter when it loads it or looks across it | Thin out a wood planted too densely (**Plant brush**, **Clear**), or plant with more space between trees |
| **Lamps lighting no ground at night** (note) | Street lamps stand on a tile the game does not light from its lamps (`[variable_terrainlightmap]` is off). At night the ground under them stays dark, unless the tile's light map file was made by OMSI 2's editor | **Fix** turns it on, as on nearly every stock tile |

### Timetable and traffic

| Check | What it means | How to fix |
| --- | --- | --- |
| **Trips without a way** (warning) | No way along the roads was found through this trip's stops (no road or track near a stop, a gap after a path, a way not passing a stop). Buses cannot drive it | Open it in **Map → Lines and timetable** to see where it breaks. **View → AI lanes** shows whether the roads are joined and run the right way |
| **Tours without departures** (warning) | A tour has no trips, so no bus runs it | Add departures (**Fill from … to …**) |
| **Tours without a depot** (warning) | A tour names no depot or AI group, or one that is not there: the game has no vehicle to send | Choose a depot in the Lines window, or **Fix** gives it the map's depot (one of OMSI 2's MAN buses added where the map has none) |
| **Traffic groups not in the AI lists** (warning) | The unscheduled traffic names a group the AI lists lack: those cars never appear | Add the group in **Map → Traffic and AI**, or **Fix** copies it from OMSI 2's own maps where they have it |
| **Stops no trip calls at** (note) | A timetable stop no trip uses | Nothing is wrong; it is only not used yet |
| **Depot files (destinations)** (warning or note) | The map has no depot file, a depot names a `.hof` found nowhere, a depot file of another map knows none of the lines' destinations (a new map's depot has Grundorf's), or one lacks what the lines need. Without one the signs stay empty and the IBIS knows no line | **Fix** makes it from the map's lines, or adds what they need. **Map → Depot files** to see and change it |

### Links, rail, wire and scenarios

| Check | What it means | How to fix |
| --- | --- | --- |
| **Objects linked to one that is gone** (warning) | An object takes its state (`[varparent]`) from an object no longer on the map: a signal head stays dark, a display blank | Link it to another in its properties (**Linked objects**), or **Fix** forgets the link |
| **Signal heads linked to no crossing** (note) | A signal head is linked to no crossing while others like it are, so it stays dark | Select it and link it in its properties (**Linked objects**) |
| **Signal routes naming what is gone** (warning) | A signal route names a signal or track pieces no longer on the map: the signal may never clear | **Map → Signal routes**: take the track again, or delete the route |
| **Trolleybus wire the game cannot use** (warning) | openOMSI makes a switch only where exactly three spans meet; four in one point gets none. A switch where no frog is left is dropped; a span in a section the network lacks has no substation | **Map → Trolleybus wire** |
| **Chrono changes of what is gone** (note) | A chrono scenario removes, retypes or relabels something no longer on the map: that change does nothing | **Map → Chrono scenarios**, the scenario's properties: take the change out |

## Check in openOMSI

**Map → Check in openOMSI…** saves the map and runs it in the game itself, unseen, with the game's own checks. The game's picture of the map from your view is shown beside what it found, problems first. It takes a few seconds on a small map. It needs openOMSI installed.

- **Every tile (slower on a big map)**: check all tiles, not only those near the view.
- **Run again** after a change.
- Click an underlined line to go to the place it names.
- **The game's whole output** shows the full log, for when something went wrong.

| Line in the report | What it means | What to do |
| --- | --- | --- |
| `road check: N of M points along the driving lanes have no road surface under them` | Lanes with no road surface under them: a bus there drives on nothing | Look at the `bare point at` lines: close a gap between roads, or lay a road where the lane runs |
| `bare point at (x, y, z), nearest surface ±d m` | One such point | Click it to go there |
| `buried check: N of M road points have ground standing over them` | Roads buried in the ground | **Lay on the ground**, or lower the ground beside the road; for a cutting, set the road's **Ground** to **cut away under it** |
| `ground-cut check`, `ground-over-road check` | Ground that a road's terrain cut would remove with nothing under it; road points 3 cm to 1.5 m under the ground | Check the road's **Ground** setting and the terrain round it |
| `wheel check: … wheel points, … under a face, … off the lane's height` | Steps a bus wheel cannot climb: a face that is a wall to the tyre, or a lane more than 30 cm off the road's height | Smooth the joint, or even the road's heights (**Even slope end to end**) |
| `spline check: N of M chained ends differ in height by over 10 cm` | Chained roads apart in height at their joint | Drag the joint in the **Height profile**, or set both ends' **height, m** alike |
| `entry point check: N of M entry points have no road surface under them` | Start points off the road | Move the start point onto the road |
| `spike check` | Road faces standing taller than profile, gradient and cant allow | Look at the road's ends: height, gradient and cant |
| `check trips: …` (gaps, taken the wrong way round, lanes not as long as the file says) | Every trip's way on the lanes. A gap means the bus loses its way there | Open the trip in **Lines and timetable**; save again so its way is found anew |

A share of a few points in a great many is not marked as a problem; the stock maps have that too. Places at the map's edge or a road's dead end are greyed: the game measures a lane's last point on the road's very edge, and a bus at the end stands over it. Nothing to mend there.

If openOMSI stops with an error, the window says so. Read the game's whole output, and see [Troubleshooting](TROUBLESHOOTING.md) for the game itself.

## Saving and earlier versions

### Where files go

openMapper never writes into the OMSI 2 folder. A stock map you open is saved as a copy in openOMSI's content folder; the game reads that folder before the installation.

| What | Where |
| --- | --- |
| Maps, imported assets, generated junctions | openOMSI's content folder: the one the launcher set up beside the game, otherwise `~/.openomsi/content` (`OMSI_CONTENT` overrides it) |
| openMapper's own junction pieces | `Sceneryobjects\openMapper\<map>` and `Sceneryobjects\openMapper\Junctions` in the content folder |
| Backups of saves | `<content>/openMapper/backups/<map>` |
| Autosave | `<content>/openMapper/autosave` |
| Deleted maps and assets | `<content>/openMapper/bin` (put back by hand) |
| Groups for other maps | `<content>/openMapper/groups` |
| Complex junction drawings | `<content>/openMapper/junctions/<name>.json` |
| Map scripts | `<content>/plugins/map_<folder>/` |
| Map's own editor files (groups, imagery, conflicts) | `openMapper/` inside the map's folder; the game ignores it |
| Settings, keys, recent maps | `~/.openomsi/openmapper.json` |
| Window layout | `~/.openomsi/openmapper_ui.ron` |
| Log | `~/.openomsi/logs/openmapper.log` (the last run's: `openmapper.previous.log`) |
| Imagery and OSM cache | `~/.openomsi/cache/openmapper/tiles` |
| Pictures of the view (`F12`) | `~/Pictures/openMapper` |

The map's files are written in OMSI 2's format: UTF-16 with CR LF, tiles at `[version] 14`, `.map.terrain`, A8 DDS painting masks `texture/map/tile_x_y.map.N.dds`.

### Safe saving

`Ctrl+S` saves. A dot on the toolbar's save button means there are unsaved changes.

Every file is written under a temporary name and put in place only once all of them are written. A full disk or a crash leaves the map as it was. If a backup cannot be kept, nothing is saved, and the changes stay in the editor.

### Earlier versions

Before every save, the files it replaces are kept: the last 10 saves.

1. Open **File → Earlier versions…**.
2. Pick the save to go back before ("before the save of …").
3. Press **Go back to this**.

The map is put back as it was before that save. Later changes go.

### Autosave

Your changes are autosaved every 3 minutes to `<content>/openMapper/autosave`, never into the map itself. The timetable and the traffic are included.

If openMapper ended without saving, the next start shows **Unsaved changes found**:

- **Restore the changes** brings them back. Save to keep them.
- **Drop them** forgets them.

If the map's files were changed after the autosave (by OMSI 2's editor, say), the dialog warns that restoring puts the older, autosaved tiles over those changes.

### The map changed on disk

When another program changes the map's files while openMapper has it open:

- With no unsaved changes here, the changes are read in.
- With unsaved changes, **The map changed on disk** asks: **Reload from disk** or **Keep mine**. **File → Reload from disk** does it at any time.

When the installation's own copy of a map changes after openMapper's copy was saved, **The map changed in OMSI 2's folder** offers **Bring the changes over** (the replaced files are kept as an earlier version) or **Keep openMapper's copy**.

## Collaboration

**File → Multiplayer…** lets several people edit one map at once, like Roblox Studio's Team Create. One person hosts the map open in their openMapper; the others join with an address.

### Hosting and joining

**On the same network:**

1. Open the map, type **Your name** and a **Password**.
2. Tick **Others on this network can join** and press **Host**.
3. Give out one of the listed addresses, for example `192.168.0.10:27560`. Port 27560 is the default (**Port**). The host's firewall must let the port in.

**From anywhere:**

1. Tick **Others anywhere can join (a free Cloudflare tunnel, nothing to set up on the router)**.
2. openMapper starts a Cloudflare quick tunnel (openOMSI's own). `cloudflared` is fetched the first time and checked against its published SHA-256.
3. Give out the `https://….trycloudflare.com` address. It changes every time.

If the tunnel cannot be opened (no `cloudflared`, or no internet), the LAN addresses still work.

**Joining:**

1. Save the map you have open: joining closes it.
2. In **Join a map**, type or paste the address, your name and the password. Press **Join**.

The host's map is copied into `maps/<folder>_multiplayer` and opened. A map of the same name you have is left alone. Everybody needs the same objects and splines installed: only the map is sent, not the content it uses.

### What is shared

- Shared as they are made: every tile (objects, roads and splines, ground, paint, water), the global settings with the tile list, and the groups.
- Each edit goes to the host with the versions of the tiles it changed. When somebody else changed one first, the edit is put back and its author told. Nobody's work is written over unseen.
- Undo stays each person's own. Steps on a tile somebody else changed since are forgotten.
- Every editor hands out IDCodes from its own block, so two people placing things at once never make the same number.
- The window shows who is in (each in their colour), the addresses, whether your edits have reached the others, and a chat.
- Others' cursors are drawn in the view with their names and tools. What they selected is ringed (objects) or traced (roads) in their colour.

**Not shared yet:** the timetable, the traffic and AI, and the descriptions stay the host's. A guest's change of them is put back, with a message. **The host saves the map.** A guest cannot save during the session; their copy can be saved once they leave.

### Conflicts and recovery

Conflicts are found per tile, and per global settings and groups. The host takes an edit only when every part of it is still at the version it was made from.

- **An edit is sent once its gesture ends.** A drag, a brush stroke or a road being drawn is one step. While it is open, others' edits are held and put in once it ends. Cursors, the people list and the chat come in at once. At most 4096 messages or 512 MiB are held; beyond that the session ends and the map stays open.
- **Conflicting local work is kept.** Before the host's state replaces local work it conflicts with, that work is written to a recovery file in the map's `openMapper/conflicts` folder (512 MiB at most). If it cannot be written, the session ends and the local work stays open. The folder is not emptied by itself.
- **Recover it:** after leaving, **File → Multiplayer → Recover a conflicting edit…** puts a copy back into the open map as one undoable edit. It does not save the map or send the edit.
- **Queues are bounded.** The editor sends one edit at a time; only the latest cursor goes. A send queue that still fills (a stalled connection) closes the connection rather than block or drop an edit. The map stays open and can be saved as a separate copy.

### Security

- **Anybody who joins can change the whole map.** There are no roles or read-only guests. Give the password only to people you would let edit the map.
- **A password is needed.** Hosting without one must be confirmed, and is possible only on the local network, never through the tunnel. Over the internet the password must have at least 8 letters.
- **On the local network the connection is not encrypted.** Somebody listening can see the map and its edits. The password itself is never sent: the host sends a fresh nonce with a salt and a work factor, and the guest answers with an HMAC of the nonce under a key derived from the password (PBKDF2-HMAC-SHA256, 200,000 rounds). Somebody who catches a nonce and its answer can still try passwords offline, so **a long password keeps it out of reach.** Use the tunnel when the network is not yours.
- **Through the tunnel** the connection is encrypted (HTTPS to Cloudflare, Cloudflare to the host's `cloudflared`). Cloudflare carries the traffic. The address is public: anybody who has it can try to join.

### Limits of a host

| What | Limit |
| --- | --- |
| People in a session | 16 (the host counted) |
| Connections still in their handshake | 8 at once; more are closed |
| A handshake | 10 seconds in all |
| Wrong passwords | Checked one at a time; each makes the next wait 250 ms, doubling up to 4 s (forgotten after a minute); a wrong one ends the connection |
| One message from a guest | 64 MiB (unpacked) |
| One edit | 4096 tiles and other parts |
| What the host holds | 512 MiB and 4096 parts more than the map it started with |
| Where new tiles may lie | Within 32 tiles of the map's tiles at the start |
| A guest that stops reading | Let go once 256 messages wait for it |
| A guest not heard from | Gone after 45 seconds |

An edit beyond these is turned down whole, and its author told why. Each edit sends whole tiles, so painting on tiles with many layers through the tunnel can be slow.

### Messages you may see

| Message | Meaning |
| --- | --- |
| **Somebody changed this first: your edit was put back** | A conflict; your work is in `openMapper/conflicts` |
| **The timetable, the traffic and the descriptions are the host's in multiplayer: your change was put back** | Not shared yet |
| **That lies too far from the map for this session: your edit was put back** | Beyond 32 tiles |
| **The host takes no more: the edit is too big, or the session holds as much as it may.** | A limit was reached |
| **The connection to the host was lost. The map stays open; it can be saved as a copy.** | Join again; it starts from the host's map as it is |
| **Wrong password**, **The session is full**, **The host has another version of openMapper** | As they say. Everybody needs the same version |
| **In multiplayer the host saves the map. Leave the session to save this as a copy of your own.** | A guest pressed Save |

### Not done yet

- Conflicts cover whole tiles: two people editing different objects on one tile can conflict.
- The timetable, the traffic, the descriptions and chrono patches are not edited together.
- A connection does not resume by itself, and offline edits are not replayed. Keep a separate copy first if local work matters.
- Sending content, read-only guests, roles, removing somebody and projects that stay hosted are not there.
- Behaviour on Windows and macOS, and through a real tunnel, still needs testing by hand.

## Performance

| Problem | Try |
| --- | --- |
| A big or very detailed map is slow | Turn off **View → Load all tiles at once**, and pick fewer **Tiles loaded at once** (9 is about 500 m round the camera). Others load as you go there |
| Low frame rate | **View → Graphics quality → Low** or **Medium** (fewer shadows, shorter view) |
| A dense town is hard to click in | **View → Show**: hide trees, objects or roads. Hidden things are not picked either |
| The game stutters on a tile | The map check's **Tiles with very many objects**: thin out woods |
| Painting through the multiplayer tunnel is slow | Each edit sends whole tiles: fewer ground layers, or the LAN |

When nothing moves, openMapper draws only a few frames a second to save power.

## Common problems

| Problem | What to do |
| --- | --- |
| **OMSI 2 not found** on the start page | **Choose OMSI 2 folder…**: the folder with `Omsi.exe`, `maps` and `Vehicles`. openMapper needs an installed OMSI 2 (Steam or retail) |
| **openOMSI was not found: install it to test maps** | Install openOMSI ([Getting started](GETTING_STARTED.md)). Test drives and **Check in openOMSI** need it |
| **The graphics could not be started.** | **View → Graphics API**: pick another and restart. `OPENMAPPER_BACKEND=dx12` (or another API) sets it for one run. If the chosen API cannot start, openMapper falls back to the automatic one |
| macOS says the app cannot be opened | It is not notarised. Right-click it and choose **Open**, or **System Settings → Privacy & Security → Open Anyway** |
| Linux: no file dialogs, or no window | Install GTK 3 and a Vulkan or OpenGL driver |
| A stock map I edited is unchanged in OMSI 2 | openMapper saves stock maps as a copy in the content folder. openOMSI reads it; for OMSI 2 use **Map → Test drive in OMSI 2 (the original)…** (Windows only) |
| The bus does not appear | Place a start point on a road (map check: **No start point**) |
| Buses drive past a stop | It is away from the road, or its kind is **Request stop** / **Not served (checkpoint)** |
| The streets stay empty | The AI groups are missing (map check: **Traffic groups not in the AI lists**), or the traffic curve is low (**Traffic and AI → Over the day**) |
| No AI buses on the tours | The tours have no depot (map check: **Tours without a depot**) |
| Bus signs and IBIS are blank | No depot file (map check: **Depot files (destinations)**) |
| White or black objects | A missing texture (map check: **Missing textures**) |
| Signal heads stay dark | Link them to their crossing (**Linked objects**), and give the crossing a light program |
| The ground is dark under street lamps at night | Turn on **Lamps light the ground at night** for the tile |
| A row of lamps along a road has vanished | Map check: **Rows the game does not show** |
| The bus loses its way in the game | Map check: **Trips without a way**; **Check in openOMSI** shows the gaps |
| A window is off the screen | **View → Put the windows back in their first places** |
| Keys do nothing, or do the wrong thing | Check **View → Preset Key Shortcut (from other applications)**, and **Reset all keys** in the settings ([Keys and mouse](MAPPER_KEYS.md)) |
| An update is offered | **Help → Updates…**: **Install and restart**, **Not now**, or choose when to be asked. A download that does not match its checksum is not installed |

## Reporting a bug

1. Make sure you have the latest version (**Help → Updates…**).
2. Note what you did, what you expected and what happened.
3. Find the map's `global.cfg` path.
4. If openMapper stopped, take the end of the log: `~/.openomsi/logs/openmapper.log`, or `openmapper.previous.log` after a restart. A crash writes its message, place and backtrace there.
5. Open an issue at [github.com/openOMSI-org/openMapper/issues](https://github.com/openOMSI-org/openMapper/issues), in English, one problem per issue. Attach the log and a screenshot. Or ask on the [Discord server](https://discord.gg/openomsi).

Security problems are reported privately through GitHub's **Report a vulnerability** on the openMapper repository, not as a public issue.
