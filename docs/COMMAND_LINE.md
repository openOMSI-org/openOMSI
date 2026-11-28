# Command line

Every command-line flag of the game program (`openomsi`, `openomsi.exe`), and the developer tools
inside the game window. You need this for shortcuts that skip the launcher, scripted test runs,
offscreen screenshots and dedicated servers. For everyday play the launcher does all of it.

## How the game starts

| Command | What happens |
| --- | --- |
| `openomsi` | opens the launcher |
| `openomsi --menu` | the in-game start menu instead of the launcher |
| `openomsi --map … --bus …` (any flags) | starts a drive with what the flags say |
| `openomsi --no-menu …` | starts a drive with neither launcher nor menu; the command line defines everything |

The in-game start menu asks for map, vehicle, time, traffic, passengers, timetable, weather and
date. Arrow keys change values, `Enter` starts, `Esc` quits.

The launcher starts the game with a full command line; **Drive → Show the command line** shows it.

Example:

```bash
openomsi --root "/path/to/OMSI 2" --map maps/Grundorf/global.cfg \
  --bus Vehicles/MAN_SD200/MAN_SD80.bus --time 07:30 --date 1989-01-15 \
  --traffic 40 --schedule --line 76 --tour 1 --passengers --autostart
```

`--help` lists the flags; `--version` prints the version.

## Installation and content

| Flag | Meaning |
| --- | --- |
| `--root <folder>` | the OMSI 2 folder. Remembered, so needed only once. Without it: `$OMSI_ROOT`, the folder remembered, a folder `OMSI 2 Original` or `OMSI 2` beside the program, the usual Steam places |
| `--content-zip <file.zip>` | read a mod or map archive in place as content, without unpacking; may be repeated |
| `--no-plugins` | no Lua, vehicle or legacy OMSI plugins for this game |

## Map and start

| Flag | Meaning | Default |
| --- | --- | --- |
| `--map <path>` | the map, relative to the root | `maps/Grundorf/global.cfg` |
| `--entry <n>` | entry point (from `global.cfg`) where the vehicle stands | 0 |
| `--auto-entry` | with a duty: the entry point nearest its first stop by road | |
| `--spawn x,y,heading[,z]` | place the vehicle here instead (`z`: a road above the ground) | |
| `--on-foot` | start as a pedestrian where the bus would stand | |
| `--situation <file.osn>` | a saved situation: map, date, time, weather, vehicles, tour | |
| `--save-situation <file.osn>` | offscreen: write the state after `--drive`; window: bind it to F5 | |
| `--tutorial <1-4>` | OMSI's tutorial, with its pages beside the picture | |

## Vehicle

| Flag | Meaning |
| --- | --- |
| `--bus <path>` | the vehicle, e.g. `Vehicles/MAN_SD200/MAN_SD80.bus` |
| `--paint <name or index>` | the livery, from its `.cti` files |
| `--plate "B-AB 1234"` | the number plate |
| `--number <n>` | the fleet number from its `[number]` list |
| `--hof <name>` | the depot file (default: the map's depot group) |
| `--depot <bus>` | a bus of your depot (`~/.openomsi/depot.json`): its wear is restored and saved at the end |
| `--driver <file.odr>` | the driver's personnel file, e.g. `Drivers/OMSI-Fan.odr`; the run is added to it |
| `--autostart` | put the bus into service, as `Shift+U` does. With `--situation` it only types the next trips into the IBIS |
| `--comfortable-cabin-on-start` | the cabin at 22 °C at the start (saved situations keep their own) |
| `--riders <n>` | passengers already seated |
| `--refuel` / `--wash` / `--repair` | the depot's services before the run |
| `--dirt <0..1>` | how dirty the bus starts |
| `--physics rigid\|simple` | rigid body (default) or simple kinematic driving |
| `--setvar name=value[,…]` | set script variables before the scripts' `{init}`, and again after the spawn |
| `--setstr name=value[,…]` | set string variables after the spawn |
| `--triggers name[@seconds],…` | fire script triggers after the spawn (times apply during `--drive`) |

## Time, weather and season

| Flag | Meaning | Default |
| --- | --- | --- |
| `--time HH:MM` | time of day at the start | 09:00 |
| `--date YYYY-MM-DD` | the date; it decides the season | 1989-05-30 |
| `--keep-time` | with a duty: keep `--time` however late the first trip leaves | |
| `--season <season>[-early\|-mid\|-late]` | e.g. `winter`, `autumn-late`, `spring-early` (see [Weather and seasons](WEATHER.md#season)) | by date |
| `--weather <x>` | `natural`, `cycle`, `tomorrow`, or a file such as `Weather/Bodennebel.owt` | natural |

## Traffic, passengers and duty

| Flag | Meaning | Default |
| --- | --- | --- |
| `--traffic <n>` | AI vehicles kept around the camera | 0 |
| `--schedule` | run the timetable: AI buses from the map's TTData | |
| `--no-timetable-buses` | with `--schedule`: your duty's timetable only, no timetable AI buses | |
| `--line <line> --tour <tour>` | your line and tour (needs `--schedule`) | |
| `--trip <HH:MM or n>` | the trip of the tour to start with: its departure, or its place (1 = first) | the trip under way |
| `--whole-tour` | with `--trip`: drive on with the rest of the tour | |
| `--passengers` | people at the stops | |
| `--ai-drive-through-parked` | AI drives through a parked car in its lane, as OMSI's traffic does | |

## Graphics and controls

| Flag | Meaning | Default |
| --- | --- | --- |
| `--enhanced` | the Enhanced renderer | from the settings |
| `--enhanced-plus` | Enhanced with Real-Time Ray Tracing | |
| `--view-distance <m>` | how far round the camera tiles stay loaded | 1200 |
| `--radius <n>` | load only tiles within n tiles of the start, all at once | 2 offscreen |
| `--all` | load every tile of the map | |
| `--view driver\|pax\|outside\|free\|mirror<n>` | the starting view; `mirror<n>` shows what mirror n sees | driver |
| `--drive-keys simple\|wasd\|arrows\|omsi` | the driving keys preset (see [Driving](DRIVING.md#driving-keys)) | simple |

## Multiplayer and server

Details in [Playing together](MULTIPLAYER.md) and [Dedicated server](SERVER.md).

| Flag | Meaning |
| --- | --- |
| `--lan-host <port>` | host a session on this UDP port (0: 27015, or the first free one up to 27024) |
| `--lan-join <where>` | join: a session code, `ip`, `ip:port`, a host name, a bare port (this machine), `auto` (find one on the LAN), or a server's `https://` / `wss://` address |
| `--lan-name <name>` | your name as others see it (default `Driver`) |
| `--lan-ws-fallback <url>` | a gateway to try when a direct UDP join gets no answer |
| `--allow-server-fleet` | allow the server's own bus or repaint downloads for this run |
| `--session-buses <list>` | when hosting: the buses allowed, separated by `;` (folders or `.bus` files). Empty: every bus |
| `--server <server.cfg>` | dedicated server: no window, no sound, no graphics card. A missing file is written with defaults |
| `--server-map <map>` | the server's map instead of `server.cfg`'s `map` |
| `--list-maps` | print the installed maps, one a line, and end |
| `--pack-map <map> --pack-to <folder or .zip>` | pack a map with everything it uses for a server, and end |

## Offscreen pictures and test runs

These render to a file instead of opening a window, for tests and screenshots.

| Flag | Meaning | Default |
| --- | --- | --- |
| `--offscreen <out.png>` | render one frame to this PNG | |
| `--size WxH` | picture size | 1600x900 |
| `--cam x,y,z,yaw,pitch[,fov]` | camera in world metres and degrees | the map's editor camera |
| `--drive <seconds>` | simulate this long at full throttle before rendering | |
| `--snapshots 7,9,11` | also save pictures at these seconds (`out_<t>.png`) | |
| `--follow <id\|auto>` | camera behind this AI car, or the first that overtakes | |
| `--click x,y[,dx,dy]` | press (and drag) the cockpit switch at that pixel, and report it | |
| `--look yaw,pitch` | turn the head or the outside camera by these degrees | |
| `--exit-after <seconds>` | window mode: quit after this long | |
| `--export-glb <bus.glb>` | write the vehicle (with `--bus`, `--paint`) as glTF, and end | |

Example:

```bash
openomsi --offscreen shot.png --bus Vehicles/MAN_SD200/MAN_SD80.bus --drive 10 --snapshots 4,8
```

## Tools

| Command | What it does |
| --- | --- |
| `openomsi --check <file or folder> [--root <folder>] [--deep]` | load content as the game does and print every warning as JSON (see below) |
| `openomsi --export-diagnostics <file.zip>` | write a support package and end, without starting the game |

### Checking content

`--check` loads a vehicle (`.bus`, `.ovh`, with its coupled parts), a scenery object (`.sco`), a
spline (`.sli`), a human (`.hum`), a weather (`.owt`), a mesh (`.o3d`, `.x`), a map (its
`global.cfg` or folder) or a mod (a folder or `.zip`). It lists every warning and error, and looks
for the textures, meshes and files they name. `--deep` loads every object and spline of a map. No
window opens. The launcher's **Model test** uses the same check (see
[Troubleshooting](TROUBLESHOOTING.md#checking-content)).

## The launcher's command line

The launcher's data side runs from a terminal too:

```bash
openomsi-launcher --cli lines '{"map":"maps/Grundorf/global.cfg","date":"1989-05-30"}'
openomsi-launcher --cli install '{"path":"/path/to/mod.7z","mode":"auto"}'
```

Its commands: `config`, `maps`, `vehicles`, `weather`, `lines`, `ibis`, `profiles`, `profile`,
`mods`, `modinfo`, `install`, `instances`, `stop`, `log`, `join`, `settings`, `save_settings`,
`keybindings`, `save_keybindings`, `preview`, `args`, `launch`.

- `lines` takes a `date`: the timetable folders active that day add and remove lines, as in the
  game.
- `install` takes a `mode`: `auto`, `extract` or `inplace` (see [Mods](MODS.md#archives-used-in-place)).
- `modinfo '{"path":…}'` describes an archive before installing it.

## Developer tools

For those who build vehicles. **Ctrl+Shift+D** opens a panel at the right of the picture over the
running game; again closes it (closed, it costs nothing). The game goes on under it. The mouse
works the panel where it is over it, and the cab everywhere else.

| Page | What it does |
| --- | --- |
| **Variables** | search the bus's script variables (`$` first: string variables) with their values. A click puts one on the watch list, kept per bus in `~/.openomsi/devtools/<bus file>.watch`. **Set** a watched variable once, or **Hold** it every frame until *Let go* or the panel closes |
| **Overlays** | draw over the picture: paths and their points (entries, exits), seats and standing places, bounding boxes, axles and wheels, interior lights with their variables, ticket and stamper points, mirrors, centre of gravity, views; for every part of an articulated bus |
| **Mirrors** | move, turn and widen each mirror while you look. *Copy the block* copies its `[add_camera_reflexion_2]` block to paste into the `.bus` (also shown and logged) |
| **Files** | changed bus files are read again within a second while the panel is open (or on its button) |

The **Files** page reloads:

- `paths.cfg` and `[passengercabin]` (not while people are aboard and places, doors or points
  would go);
- `sound.cfg`;
- the `.bus`: physics, cameras, mirrors, bounding box (not when its axles change);
- `model.cfg`: animations, visibility, lights (as long as it names the same meshes);
- the constfiles (as long as the scripts and their variables stay).

A changed script needs the bus placed again. *Open the log* opens `game.log`. Change a copy of a
bus in your content folder, never the installation's own files.

## Debug keys

| Key | What it does |
| --- | --- |
| `Ctrl+F11` | show the camera's position, tile and heading; the log line has it as a `--cam` value |
| `Ctrl+Shift+Page Up` / `Page Down` | run the clock forwards / backwards (held: faster the longer) |
| `Ctrl+Shift+E` | object editor (see [Mods](MODS.md#object-editor)) |
| `Ctrl+Shift+D` | developer tools |
| `F9` | write the run into the personnel file (the log has it without one) |
| `Ctrl+Alt+arrows` | aim the mirror you are looking at |

## Measuring performance

`OMSI_PROFILE=1` logs the time each stage of the frame takes. With `--exit-after N`, or when you
close the game, it writes a summary: frame-time percentiles and the number of frames over 16.7,
33.3, 50 and 100 ms, counted from 15 s after the map loaded. `OMSI_PROFILE_JSON=<file>` also
writes the summary to a file.

```bash
OMSI_PROFILE=1 openomsi --map maps/Grundorf/global.cfg --bus Vehicles/MAN_SD200/MAN_SD80.bus --exit-after 120
```

## Environment variables

Environment variables (`OMSI_*`) switch debug and test behaviour, all off unless set. Most players
never need one. A few that matter outside testing:

| Variable | What it does |
| --- | --- |
| `OMSI_ROOT` | the OMSI 2 folder |
| `OMSI_CONTENT` | another content folder |
| `OMSI_CONTENT_ZIP` | archives read in place, separated like `PATH` |
| `OMSI_MX=off` | start once without mX Graphics, when the game does not start with it |
| `OMSI_NO_UPDATE=1` | no update checks |
| `OMSI_NO_PRESENCE=1` | not counted on the website's "playing now" |

Every variable is in [Environment variables](DEBUG_FLAGS.md).
