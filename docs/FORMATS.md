# OMSI 2 content formats

A reference of the OMSI 2 file formats (maps, objects, splines, vehicles, models, scripts,
textures, sounds, people, timetables, weather) as openOMSI reads them. It is for modders and
developers who need to know exactly how a file is read. Each section says what a format is,
where it lives, gives a small example and lists its keywords, then notes where openOMSI behaves
differently from OMSI 2.

openOMSI's own new keywords are summarised in [Modding overview](MODDING.md); this page mentions them
where they belong in a format.

## Common rules

These rules apply to every text format below.

### Text encoding

| Files | Encoding |
| --- | --- |
| `global.cfg`, `tile_x_y.map`, chrono `*.map`, `*.chronoterrrelevant`, situations `.osn`, the personnel file `.odr` | UTF-16 LE with a byte-order mark (`FF FE`) |
| Everything else | The author's ANSI code page |

OMSI reads ANSI files in the system's code page. Stock content is Windows-1252, but a mod uses
its author's: Windows-1251 for Russian mods (LiAZ, PAZ, the Scania Citywide cockpit), 1250 for
Polish and Czech ones. Some newer files are UTF-8.

openOMSI has no system code page, so it decides per file:

1. Valid UTF-8 is read as UTF-8.
2. Windows-1251 when at least half the letters from `0xC0` up stand in runs of three or more
   (Russian words; German has at most two in a row, as in "Größe").
3. Windows-1250 when letters that are symbols in 1252 (ł ą ś Ł Ś Ż) stand next to letters.
4. Windows-1252 otherwise.

A font may have been made for another code page than the text it shows, so a glyph is also
looked up as the same byte in 1251, 1250 and 1252.

Line endings are CR LF. Numbers use `.` as decimal separator and may have an exponent
(`6.21874087223886E-7`).

### The `[keyword]` block format

Almost every text file (`.bus`, `.ovh`, `.sco`, `.sli`, `.cfg`, `.hof`, `.ttp`, `.ttl`, `.ttr`,
`.owt`, `.oop`, `.odr`, `.otp`, `.osn`, `.dsc`, `.hum`, ...) is a list of keyword blocks:

```text
Anything here is a comment.

[friendlyname]
Bus stop sign

[mesh]
sign.o3d
```

The original .bus header says it: the program ignores everything except keywords such as
`[mesh]` that stand alone at the start of a line. The lines that follow are the parameters,
one per line. Then the program searches for the next keyword.

openOMSI reads them the same way:

- A keyword line is the **whole line**. No leading or trailing whitespace: an indented
  `[keyword]` is a help text or a switched-off block in the stock files (the SD202 cabin's
  `[exit]` help, the F90's second rear axle, the EN92's fourth door-slam sound).
- A keyword OMSI knows must be spelled as OMSI spells it: `[matl_noZwrite]`, `[LOD]`,
  `[NightMapMode]`, `[noDistanceCheck]`. A mod's `[matl_nozWrite]` or `[NoMapLighting]` is free
  text. Keywords OMSI does not know are compared without regard to case.
- Parameter lines are read verbatim, in order. An empty line is a legal, empty parameter.
- Free text between blocks is ignored.
- Sub-commands (`attach_trans` and `attach_rot_*` after `[new_attachment]`; `origin_*`,
  `anim_*`, `offset`, `delay`, `maxspeed` after `[newanim]`) are lines of their own anywhere
  after their block. The stock `Timetable_Terminus_Pole_S.sco` has its `attach_trans` after
  `[complexity]`.
- `-<DISABLED>-` ... `-<ENABLED>-`, each alone on its line, switches blocks off in `.bus`/`.ovh`,
  `.sco`, `model.cfg`, `passengercabin.cfg`, `paths.cfg`, `.hum` and `envir.cfg`.

Exceptions: `.hof` cuts trailing tabs, spaces, CR/LF and `"` off every line (the stock depot
files are spreadsheet exports). `ailists.cfg` lower-cases its lines.

### File names

File names inside content files resolve as Windows resolves them:

- `\` and `/` both separate folders.
- Names compare without regard to case, umlauts included.
- A folder name loses one trailing dot; the last name loses all trailing dots and spaces
  (Ahlheim's `anz-oben.jpg.`).

A name with non-ASCII letters may have changed on the way. A zip stores a name without its
UTF-8 flag in the code page of the computer that made it (CP866 on a Russian one), and the
unpacker guesses another: the Scania's `верх.png` arrived as `óąÓň.png` (CP866 read as CP852).
openOMSI also looks for a name as its bytes in CP437, CP852, 1252 or 1250 read again in CP866,
1251, 1250 or 1252.

### Content roots

openOMSI's content folder, mounted archives and the OMSI 2 installation are searched as if they
were copied over each other, highest priority last (see [Mods](MODS.md)).

One exception: a vehicle pack (`Vehicles/<folder>`) present in two roots in two **versions**
does not mix. The copy in a higher-priority root patches the loaded pack only when it has no
`.bus`/`.ovh`/`.zug` of its own, or only ones that pack has too. A pack with vehicles the other
lacks is another version: its files are only used for what the vehicle's own pack lacks.

Example: Ahlheim V5 bundles its own Citaro Facelift next to an installed older one. Mixed, the
Citaro ran the installed pack's varlists and constfiles under its own scripts, and its displays
stayed dark.

## Maps and tiles

A map is a folder under `maps\` with a `global.cfg`, one `tile_x_y.map` per tile and the
timetable, depot and AI files.

### Map files

| File | What it is |
| --- | --- |
| `global.cfg` | The map's header: name, tiles, entry points, ground textures, seasons (UTF-16) |
| `tile_x_y.map` | One tile's objects and splines (UTF-16) |
| `tile_x_y.map.terrain` | Terrain heights: `u32` (= 60), then 61 × 61 `f32` heights |
| `tile_x_y.map.water` | `u32` count, then four `f32`: one water plane with a height at each corner (every stock file has count 1) |
| `tile_x_y.map.LM.bmp` | 256 × 256 24-bit night light map |
| `tile_x_y.map.prt` | Precache list (text: sco path, min id, max id, `-1`) |
| `texture\map\tile_x_y.map.<n>.dds` | Paint mask of ground texture `n` |
| `texture\water.tga`, `texture\water_envmap.bmp` | The water's colour and reflection (stock swatch: 8 × 8 pixels of 47, 74, 83, alpha 192) |
| `.terrain_x.rdy` | Editor cache, ignored |
| `Chrono\<folder>\` | Timetable changes over time (below) |
| `Holidays.txt` | `[holidays]` ranges and `[holiday]` single days, each with a name |
| `timezone.txt` | `[timezone]`, `[DST]`, `[location]` |
| `humans.txt` | The people the map's pedestrians and passengers are drawn from, one `Humans\<group>\<file>.hum` per line |
| `parklist_p.txt`, `parklist_p_<n>.txt` | Parked-car objects (see [Parking spaces](#parking-spaces)) |
| `ailists.cfg` | AI vehicle groups and depots |
| `radio.cfg` | The map's radio stations (openOMSI; see [Radio](RADIO.md)) |
| `trolley.txt` | Overhead wire (openOMSI; see [Trolleybuses](TROLLEYBUS.md)) |

Other openOMSI map files (`openomsi_weather.cfg`, `openomsi_climate.cfg`, `openomsi-realmap/`,
`dependencies.json`) are listed in [Modding overview](MODDING.md).

### global.cfg

| Keyword | Parameters |
| --- | --- |
| `[name]`, `[friendlyname]` | Map name |
| `[description]` ... `[end]` | Description text |
| `[version]`, `[NextIDCode]` | File version, next free object id |
| `[worldcoordinates]` | Tile grid in geographic degrees (see below) |
| `[dynhelperactive]`, `[realrail]`, `[LHT]` | Flags; `[LHT]` is left-hand traffic |
| `[mapcam]` | Start camera of the editor |
| `[standarddepot]`, `[moneysystem]`, `[ticketpack]` | Default depot, currency and ticket files |
| `[repair_time_min]`, `[years]`, `[realyearoffset]` | Repair time, year range, year offset |
| `[backgroundimage]` | Editor background picture |
| `[groundtex]` | A ground texture (below) |
| `[addseason] kind start end` | Season by day of year (see [Seasons](#seasons)) |
| `[trafficdensity_road]`, `[trafficdensity_passenger]` | `hour factor` lines: a curve over the day that scales AI traffic and waiting passengers |
| `[entrypoints]` | Places where the player can start |
| `[splineObjTypes]`, `[scenObjList]` | Object and spline type lists |
| `[map]` | A tile: position and file name |

**`[entrypoints]` records:** index, object id, `0`, x, height, y (within the tile), a quaternion
(x, y, z, w; heading = 2·atan2(y, w)), the index of the object's tile in the `[map]` list, name.

Tile indices in a map's files (entry points, `[splineAttachement_repeater]`, timetable tracks)
count every `[map]` entry, including a tile listed twice (Westcountry 3 lists 33 twice) and
tiles whose files are missing (Berlin-Spandau lists 369 and ships 329). Object ids are not
unique across a map joined from two maps.

**Tile size.** A tile is 300 m square. With `[worldcoordinates]` (Berlin-Spandau) the grid is
1/300 degree and the edge is **371.9 m** on both axes (measured on 271 cross-tile spline links).
Local coordinates then run 0..372, and objects may be saved in a neighbouring tile's file with
coordinates beyond the edge.

**`[groundtex]`** is one ground texture the map may be painted with: texture, detail texture,
then three numbers: the paint mask's size as a power of two, how often the texture repeats
across a tile, how often the detail texture does.

- The first entry is what the whole map starts as and has no mask.
- Each further entry has a per-tile mask `texture\map\tile_x_y.map.<index>.dds`: an 8-bit alpha
  DDS of exactly that size, sometimes with mipmaps. Its **first row is the north edge**.
- The detail texture is multiplied in plainly. The stock ones are bright (noise_low averages
  242), so doubling them like a grey-centred Direct3D detail map would blow the ground out.

### Tile files (`tile_x_y.map`)

| Keyword | What it is |
| --- | --- |
| `[version]` | File version (older versions write shorter records, below) |
| `[terrain]`, `[water]` | The tile has terrain / water |
| `[variable_terrain]`, `[variable_terrainlightmap]` | Editable terrain; light map baked at load |
| `[object]`, `[attachObj]` | A placed object; an object hung on another |
| `[spline]`, `[spline_h]` | A spline |
| `[splineAttachement]`, `[splineAttachement_repeater]` | A row of objects along a spline |
| `[varparent]` | The object whose variables this one uses |
| `[spline_terrain_align]`, `[spline_terrain_align_2] n` | The spline cuts its outline out of the ground |
| `[rule]`, `[kill_rule]` | Traffic rules |
| `[selobject]`, `[selspline]`, `[delete]`, `[typ]`, `[relabel]` | Chrono patches only |

**`[object]`:** `0` (detail level), path, IDCode, **x, y, z**, heading, pitch, bank (degrees,
heading clockwise from north), then the **labels**: a count and exactly that many lines.

- z is relative to the terrain, except for `[absheight]` objects and objects with
  `[splinehelper]` connectors (crossings, switches), whose z is absolute like their splines'.
- Labels are what the editor's Labels dialog edits: sign texts (`@` breaks a line), line
  numbers, a tree's texture, height and height/width ratio, a stop's name and timetable data.
  They fill the object's string variables in order. Any line counts, even an empty one or one
  that looks like a keyword. The count is no type: trees write 3 or 4.

**`[attachObj]`:** `0`, path, IDCode, the parent's IDCode, the parent's instance, the index of
the parent's `[new_attachment]` point, heading, pitch, bank, labels.

- Only instance `0` loads: OMSI refuses objects hung on later objects of a spline attachment row.
- The parent is looked up among the `[object]`, `[attachObj]` and `[splineAttachement]` records
  written **before** it in the tile. An attachment whose parent comes later is not loaded.
- An `[attachObj]` may name a row's IDCode. It then hangs on the row's first object, with the
  row's own type.

**`[spline]` / `[spline_h]`:** `0`, path, IDCode, previous, next, **x, height, y**, heading,
length, radius (0 straight, > 0 turns right), gradient start and end (%), cant start and end,
skew start and end, alignment length, optional `mirror`. A `mirror` spline has its cross-section
turned over: every path lies at the negated offset and runs the other way.

**Older versions** write shorter records:

| From version | Added |
| --- | --- |
| 4 | Labels |
| 5 | Spline cant |
| 6 | IDCodes (older records are numbered as they load) |
| 7 | Spline `mirror` line |
| 8 | `[attachObj]` heading |
| 9 | Leading detail level (an object above the detail setting is not loaded) |
| 10 | `[attachObj]` parent by IDCode (before: its index among the tile's records so far) |
| 11 | Two spline neighbour ids (before: one line, `-1` none, else it continues the previous spline); texture offset |
| 12 | `[attachObj]` pitch and bank |
| 14 | Spline skew |

**`[rule]`:** belongs to the `[object]` or `[spline]` written before it. Its first line is the
`[path]` index within that element. Kinds: `speedlimit`, `overtaking_prohib`, `bus`, `trucks`,
`no_cars`, `trafficdensity`, `priority`. Most rules follow an object: Berlin-Spandau has 5402
trafficdensity, 693 speedlimit, 576 trucks and 64 no_cars rules on objects, against
3114/810/284/4 on splines.

### Spline attachment rows

**`[splineAttachement]`:** `0`, path, IDCode, spline index (in the tile's spline order), lateral
offset, height, start distance, heading, pitch, bank, interval, range, tilt flag, string count,
strings.

**`[splineAttachement_repeater]`** has two more lines after the `0`: the index of the master's
tile in the `[map]` list, and the index of the row's first object on its spline.

How a row is placed:

- Object j lies at start distance + j × interval **along the chain from its start**, while
  j × interval ≤ range. The chain is the splines before the master's, following the `prev`
  links, with direction flips where two splines meet end to end.
- A record places its row on its own spline and on the following splines of the chain, as long
  as they lie in its tile. Where the chain enters another tile, a repeater there carries on.
- This reproduces 177 of the 180 checkable repeater indices of Berlin-Spandau and puts buffer
  stops a few metres before the ends of their tracks.

### Chrono (timetable changes)

`<map>\Chrono\<folder>\Chrono.cfg`:

```text
[startdate]
19910602

[enddate]
0

[deactivate_lines]
5 & 5N
```

| Keyword | Parameters |
| --- | --- |
| `[startdate]`, `[enddate]` | `YYYYMMDD`; `enddate` 0 = never |
| `[ticketpack]`, `[moneysystem]` | Ticket and currency files from that date |
| `[deactivate_lines]` | Line names, one per line |

- The folders are taken in name order. One is active when `startdate ≤ date` and
  (`enddate` = 0 or `date < enddate`).
- An active chrono's `.map` patches, `ailists.cfg` and timetable files are laid over the map's.
- A line a chrono switches off stays off on every later date too. Berlin-Spandau on 2026-09-17
  has 27 lines, without the "5 & 5N" that the change of 1991-06-02 removed.

### Terrain, holes and crossings

**Terrain align.** `[spline_terrain_align]` (no parameter) and `[spline_terrain_align_2] n`
follow a `[spline]`. The editor's "align terrain" already wrote the heights into the `.terrain`
file; at load the flag only makes the spline cut its outline out of the ground.

- The outline is the spline type's `[terrainholeprofile]` extruded along the spline.
- `n` 2 and 4 keep the far end at the spline's end, 3 and 4 the near end at its start.
  Otherwise each point's third value moves it past the end.
- A `.sli` without `[terrainholeprofile]` gets one per string of joined `[profile]`s: its ends
  3 cm in and 3 mm down, a bottom 10 cm under its lowest point reaching 0.5 m past both ends.
- An outline that crosses itself cuts nothing.

> **Note:** openOMSI differs here. Where an aligned spline lies in a trough, it fills the
> trough with solid ground instead of cutting. It cuts as OMSI does where the saved ground
> stands above the spline (a cutting) or where the fill cannot be built.

**Object holes.** Each `[terrainhole] <mesh.o3d>` in a `.sco` or `model.cfg` names an
object-wide cutter. It counts even before the first `[mesh]` and when repeated. The cutter
lies next to its declaring file or in its `model` folder; a `.sco`'s declaration also applies
when it uses a separate `model.cfg`. Explicit cutters apply even to deep excavations. Without
them the terrain stands over the carriageway, which from the driver's seat looks like a
missing road.

**Road surfaces.** Spline profiles are extruded as they are: a road's outer points sit at kerb
height (0.25 m on the Marcel street splines) with no skirt down to the terrain. The terrain is
only cut away under surfaces within about 12 cm of it.

**`[crossing_heightdeformation] mesh.o3d`** (scenery) is how a junction plate meets the
ground. The plate is one flat object placed at an absolute height; the named mesh is a coarse
copy of it. Every vertex of the object moves by the difference between the ground under it and
that base mesh, so the plate keeps its kerbs while its arms come down onto the roads. The
plate's paths take their heights from it. The ground is not pressed into the plate at load.

### Night light maps

`tile_x_y.map.LM.bmp` (256 × 256) holds pools of street lamp light on the ground, north at the
top row. It is added to the terrain at night.

- It covers the tile **and its eight neighbours**: the tile is the middle third (texels
  85⅓..170⅔ each way). Neighbouring light maps are the same picture shifted by a third.
- A tile with `[variable_terrainlightmap]` has its light map baked from the `[maplight]`s of the
  nine tiles at load (OMSI writes it over the file unless options.cfg has
  `[no_generateTerrLightMaps]`). openOMSI bakes it the same way.
- A texel takes each lamp's colour × min(1, (radius / distance)²), added up, held at 1. A lamp
  more than 15.96 radii away along x or y adds nothing.

### Street names

Street names are not stored in a map. They are on its street name signs: a `StreetSign_*`
object (Verkehrszeichen_MC; 670 on Berlin-Spandau) has type flag 1 and the name as its first
string. Its heading is the road's heading + 90°. The navigator reads them for the city map.

## Scenery objects (.sco)

A scenery object is anything placed on a map: a house, a sign, a crossing, a traffic light.
It lives under `Sceneryobjects\<folder>\`, with its meshes in `model\` and textures in
`texture\`.

```text
[friendlyname]
Bus stop pole

[groups]
1
Bus stops

[mesh]
pole.o3d

[collision_mesh]
pole_coll.o3d

[busstop]
```

A `.sco` takes the whole `model.cfg` vocabulary inline (see [Models](#models-cfg-and-o3d)),
or names a separate one with `[model]`.

| Keyword | What it does |
| --- | --- |
| `[friendlyname]`, `[groups]` | Name and editor groups |
| `[onlyeditor]` | Shown only in the editor (ambient sound objects, parking spaces) |
| `[complexity]` | Detail level the object needs |
| `[rendertype]` | `presurface`, `surface` or `on_surface` (below) |
| `[LightMapMapping]`, `[nomaplighting]`, `[NightMapMode]` | Light map behaviour |
| `[fixed]`, `[absheight]` | Not movable; z is absolute |
| `[collision_mesh]`, `[nocollision]` | Collision |
| `[crossing_heightdeformation]` | Junction plate meets the ground (see [Maps](#terrain-holes-and-crossings)) |
| `[terrainhole]` | Terrain cutter mesh |
| `[surface]` | The object is a road surface |
| `[splinehelper]` | Connector for splines (crossings, switches) |
| `[path]`, `[path_2]` | A traffic path (below) |
| `[use_traffic_light] n`, `[blockpath]`, `[crossingproblem]` | Path rules |
| `[switch]`, `[switchdir]` | Rail switch |
| `[traffic_lights_group]`, `[traffic_light]`, `[phase]`, `[approachdist]`, `[traffic_light_stop]`, `[traffic_light_jump]` | Traffic light programme |
| `[trafficlight]`, `[signal]` | A lamp or rail signal |
| `[model]` | A separate `model.cfg` |
| `[script]`, `[constfile]`, `[varnamelist]`, `[stringvarnamelist]`, `[scriptshare]` | Scripts (see [Scripts](#scripts-osc)) |
| `[lua_script]`, `[lua_range]`, `[lua_rate]` | openOMSI object scripts (see [Scripts of map objects](OBJECT_SCRIPTS.md)) |
| `[sound]`, `[sound_ai]` | A sound.cfg, driven by the object's variables and triggers |
| `[paths]`, `[passengercabin]` | Walking paths and places |
| `[busstop]`, `[entrypoint]`, `[depot]`, `[petrolstation]` | Function of the object |
| `[carpark_p]` | A parking space |
| `[helparrow]` | A route arrow placed on the map |
| `[tree]` | A tree |
| `[add_camera_reflexion]`, `[add_camera_reflexion_2]` | Mirror camera |
| `[mass]`, `[momentofintertia]`, `[cog]`, `[boundingbox]`, `[crashmode_pole]` | Physics of a movable object |
| `[new_attachment]` + `attach_trans`, `attach_rot_x/y/z` | A point other objects hang on |
| `[maplight] x y z r g b radius` | A light on the ground at night |
| `[rail_enh]`, `[third_rail]` | Rail data |
| `[triggerbox_new]`, `[triggerbox_setreverb]` | Sound zones |

### Render type

`[rendertype] presurface` draws the object before the terrain and ordinary scenery, keeping its
mesh and material order. Alpha-blended materials still write depth at transparent texels, so an
invisible cover can keep the terrain from hiding an excavation drawn below it. Alpha-tested
materials keep their cut-outs, and `[matl_noZwrite]` turns off blended depth writes.

### Traffic paths

`[path]` has 12 lines, `[path_2]` 14 (two extra zero fields):

| Line | Meaning |
| --- | --- |
| 1-3 | Start x, y, z in the object frame (x right, y forward, z up) |
| 4 | Heading (degrees, clockwise, relative to the object) |
| 5 | Radius (0 straight, > 0 right turn) |
| 6 | Length |
| 7-8 | Gradient at start and end (rise per metre; the height is their integral along the path) |
| 9 | Kind: 0 street, 1 sidewalk, 2 rail |
| 10 | Width |
| 11 | Direction: 0 along, 1 backwards, 2 both |
| 12 | Turn indicator: 0 none, 2 left, 3 right (for AI blinkers) |

Lanes link by the proximity of end and start points (≤ 1.5 m, heading within 40°). This
reproduces the `prev`/`next` spline chains and the `[splinehelper]` connections of crossings.

### Traffic lights

- `[use_traffic_light] n` after a path binds that path to `[traffic_light]` index n.
- `[traffic_light] name` + `[phase] state seconds` lines: 0 red, 3 red+yellow, 6 green,
  8 yellow, 9 all-red. The last `0 0` phase is red until the `[traffic_lights_group]` cycle
  restarts.
- `[trafficlight]` lamp objects are placed with map flag 1, the extra label is the light
  index, and `[varparent] id` is the crossing. Their `[visible] red|yellow|green 1` meshes follow.
- `[traffic_light_stop] light time ifAnf` and `[traffic_light_jump] light time ifAnf target`
  are points on the cycle clock. `ifAnf` 1 means "nobody is asking at `light`", 0 "somebody is
  asking".

How stops and jumps work:

- A stop holds the clock **just before** its time, so the phase that ends there stays shown.
- If the clock is already at the stop (within 1e-6 s), it holds there.
- A stop never holds in a red-and-yellow changeover: then the clock holds at the stop's own
  time. A stop whose own time is red-and-yellow is passed.
- A jump whose condition holds at the same time as a stop is taken first.

### Changing textures by script

**`[CTC] <variable> <folder> <value>`** reads the folder's `.cti` items. `[CTCTexture] <name>
<default-file>` entries link each item to a material texture. The script's integer variable
selects an item by zero-based index; each `[CTC]` group has its own variable.

**`[texchanges] <file>`** names a `chtex_*.cfg`, relative to the vehicle's root folder (vehicles)
or the model's folder (scenery), for example `texture\chtex_SD.cfg`. It holds
`[newtexchangemaster]` blocks:

```text
[newtexchangemaster]
rollband.bmp
rlbnd_ln1

[entries]
2
rollband_0.bmp
rollband_1.bmp
```

- Line 1 is the texture name as it appears in the `.o3d`. It usually does not exist on disk:
  the mesh carries it only as a key.
- Line 2 is a script variable. Its integer value picks the entry, 0 first.
- Entry files live next to that cfg.
- The masters are model-wide, even though `[texchanges]` is written inside a mesh block.

On scenery, both mechanisms drive adverts on shelters, building signs and other props. An
out-of-range or negative value leaves the model's own texture.

### Parking spaces

`[carpark_p]` `[onlyeditor]` objects (`Generic\car_park.sco`) are parking spaces. The map's
`parklist_p.txt` lists parked-car objects (`Vehicles\X\parked_*.sco`, with `[CTC]` paint schemes
from the vehicle's `.cti`). One is placed per space at random; some spaces stay empty.

A numeric first label on a parking object or spline attachment picks an indexed list: `1` reads
`parklist_p_1.txt`, `2` reads `parklist_p_2.txt`. An empty or non-numeric label uses
`parklist_p.txt`.

### Text on signs

`[texttexture] string-index font w h fullcolor r g b` + `[useTextTexture] n` on a `[matl]`
draws text from the object's labels (`[object]` type flag 1). Text is centred and squeezed to
fit. A `[texttexture]` whose variable is one of the script's string variables (the stock
departure displays) is redrawn whenever the script calls `Refresh_Strings`.

`[helparrow]` objects show when **Map-placed route arrows** is on, independent of the arrows the
game makes from the driven route.

## Splines (.sli)

A spline type is a road, track or path cross-section extruded along the map's splines. It lives
under `Splines\<folder>\`.

```text
[texture]
asphalt.bmp

[profile]
0
0

[profilepnt]
-3.5
0
0
0

[profilepnt]
3.5
0
1
0

[path]
0
1.75
0
3.5
0
```

| Keyword | Parameters |
| --- | --- |
| `[length]` | Segment length |
| `[texture]`, `[matl_alpha]` | A texture of the profile, and its alpha mode |
| `[scaleTexByLength]`, `[patchwork_chain]` | Texture mapping |
| `[heightprofile]` | Height profile the wheels may stand on |
| `[profile]`, `[profilepnt] x z u v` | Cross-section points (extruded as they are) |
| `[path]`, `[path_2]` | A lane (below) |
| `[rail_enh]`, `[third_rail]` | Rail data |
| `[halfcantwidth]` | Width for cant |
| `[onlyeditor]` | Editor only |
| `[terrainholeprofile]`, `[terrainholeprofilepnt]` | Terrain hole outline: a profile, then points of x, height, end offset |

**`[path]`** (5 lines): kind (0 street, 1 sidewalk, 2 rail, 3 flight path), lateral offset
(right positive), height, width, direction (0 along the spline, 1 backwards, 2 both). The lane
runs the whole spline at that offset; lanes of consecutive splines meet at the ends.

## Vehicles (.bus/.ovh)

A vehicle is a folder under `Vehicles\` with one or more `.bus` (player-drivable) or `.ovh`
(other vehicles) files, plus `model\`, `script\`, `sound\` and `texture\`.

```text
[friendlyname]
SD200

[model]
model\model.cfg

[script]
script\scriptlist.cfg

[sound]
sound\sound.cfg

[passengercabin]
model\passengercabin.cfg

[newachse]
...
```

| Keyword | What it is |
| --- | --- |
| `[type]` | 2 rail vehicle, 3 aircraft (street traffic only uses type 0) |
| `[friendlyname]`, `[friendlyname_inv]` | Name in the vehicle list |
| `[ai_veh_type]`, `[rowdy_factor]`, `[ai_brakeperformance]`, `[ai_deltaheight]` | AI behaviour |
| `[coupling_back]`, `[coupling_front]`, `[couple_back]`, `[couple_front]`, `[coupling_front_character]` | Coupling |
| `[control_cable_back]`, `[control_cable_front]` | Control cables between cars |
| `[boogies]`, `[sinus]`, `[rail_body_osc]`, `[contact_shoe]` | Rail running gear |
| `[add_camera_driver]`, `[add_camera_pax]`, `[set_camera_std]`, `[set_camera_outside_center]` | Cameras |
| `[view_schedule]`, `[view_ticketselling]` | Camera views |
| `[add_camera_reflexion]`, `[add_camera_reflexion_2]` | Mirror cameras |
| `[schwerpunkt]`, `[rollwiderstand]`, `[rot_pnt_long]`, `[inv_min_turnradius]` | Physics |
| `[newachse]` + `[achse_long]`, `[achse_maxwidth]`, `[achse_minwidth]`, `[achse_raddurchmesser]`, `[achse_feder]`, `[achse_maxforce]`, `[achse_daempfer]`, `[achse_antrieb]`, `[achse_inertia_inv]` | An axle |
| `[number]`, `[registration_automatic]`, `[registration_list]`, `[registration_free]` | Fleet number and plates |
| `[kmcounter_init]` | Start odometer |
| `[model]`, `[script]`, `[sound]`, `[paths]`, `[passengercabin]` | The vehicle's other files |
| `[spotlight]` | Headlight beam (below) |

openOMSI adds `[add_camera_reflexion_static]`, `[spotlight_2]`, `[spotlight_cookie]`,
`[suspension]`, `[vehicle_script]`, the `[trolley_*]` blocks and the `[rail_*]` blocks. See
[Modding overview](MODDING.md), [Suspension types](SUSPENSION.md), [Trolleybuses](TROLLEYBUS.md) and
[Trams, metros and trains](RAIL.md).

### Vehicle variables

Built-in variables are listed in `program\varlist_roadvehicle.txt`, callbacks in
`program\callbacklist_*.txt`. Generated per axle, for `{n}_{L|R}`: `Wheel_Rotation_`,
`Wheel_RotationSpeed_`, `Axle_Steering_`, `Axle_Suspension_`, `Axle_Springfactor_`,
`Axle_Brakeforce_`, `Axle_SurfaceID_`. Also `PAX_Entry{n}_Open/_Req`, `PAX_Exit{n}_...` and
`Debug_0..5`.

- `Wheel_Rotation_*` and `Axle_Steering_*` are radians (the stock model.cfg multiplies a wheel
  by 180/π). Positive steering is to the right.
- `n_Wheel` is rpm (`antrieb.osc` computes power as `M_Wheel · n_Wheel · π / 30000` kW).
- `wearlifespan` is set by the engine and must be 1. At 0 every random part lifetime is 0,
  and the SD200's rear door reopens by itself for ever.
- `Weather_Temperature` is known before a vehicle's `{init}` runs: engine and heater scripts
  start from it.
- `PrecipRate` is signed. `rain.osc` adds `Timegap * PrecipRate` to the window wetness
  variables, so a positive rate soaks the glass and a negative one dries it. `wiper.osc` clears
  the wiped area as the blade sweeps.
- `DirtRate` is how fast the windscreen soils now (`dirt.osc` adds it into `Dirt_Wiped`);
  `Dirt_Norm` is how dirty the body is. Both are the `[alphascale]` of a dirt overlay.
  `veh_wash` clears them.

openOMSI adds `PAX_Entry8..15`, `PAX_Exit8..15` and `PAX_Entry{n}_Busy`/`PAX_Exit{n}_Busy`
(see [Modding overview](MODDING.md)).

### Collisions

The engine fires `{trigger:collision}` on the vehicle after a crash. `coll_energy` is that
crash's energy in kJ and reads the same however often the script asks. `coll_pos_*` is where
the bodies met (vehicle frame, metres), down at the bumpers. The SD200/SD202/NL202 scripts add
the energy to the engine's account only for a hit behind `coll_pos_y` < -4.70 and below
`coll_pos_z` < 1.10.

### Engine callbacks

- `(M.V.GetHeightAbovePoint)` takes x y z in the vehicle frame and returns how high that point
  stands over the first face a ray meets coming down from 3 m above it: -3 at least, 1000 with
  nothing there. The NL/NG lift and the Urbino's kneeling sensor use it.
- `(M.V.GetHumanCountOnPathLink)` takes a `paths.cfg` link index and returns how many
  passengers stand on it (the NL/NG fare gate swings out of their way).
- `GetTTTerminusIndex` spells "TT" with two T's.

### The vehicle list and coupled parts

OMSI offers the `.bus`/`.ovh` files that have a `[friendlyname]`. The GN92's rear section, most
`_KI` AI variants and the AI cars have none.

- A rear section has `[coupling_front]` and usually `[scriptshare]`: no scripts of its own, the
  front's variables drive it. It is named by the front's `[couple_back] file reversed`, resolved
  from the front's folder in whichever content root has it.
- With `[scriptshare]` the rear section's `\S:n` materials are the front's script textures, and
  its `[texttexture]`s read the front's string variables.
- `Trains\*.zug` lists the cars of a train: pairs of lines, a vehicle file (or pool group name)
  and a reverse flag (1 = coupled rear end forward). The first entry leads.

openOMSI also keeps a coupled part with a copied `[friendlyname]` out of the list, spawns the
front section when a rear one is asked for, and refuses a rear section nothing couples. A
vehicle folder without any vehicle file (a repaint for a bus that is not installed) is not
offered.

**Articulation.** The joint is reported to the front section's scripts as
`articulation_<n>_alpha` (about the vertical axis, clockwise degrees) and
`articulation_<n>_beta` (about the transverse axis); `n` is the coupled part's position in the
train. The stock jackknife protection brakes over 47° while reversing. The bellows are
`[smoothskin]` meshes whose `[setbone]` ids are **the bone mesh's position in the model's
first LOD**; the modelled pose is the straight one.

### Passenger cabin

`passengercabin.cfg` (named by `[passengercabin]`):

| Keyword | What it is |
| --- | --- |
| `[entry] n`, `[exit] n` | A door, by path point; `{noticketsale}` and `{withbutton}` lines mark it |
| `[linkToNextVeh]`, `[linkToPrevVeh]` | Path points that join two coupled cabins |
| `[stamper]` | Ticket validator |
| `[ticket_sale]`, `[ticket_sale_money_point]`, `[ticket_sale_change_point]` (and `_2`) | Cash desk |
| `[passpos] x y z seat-height rot` | A place |
| `[drivpos]` | The driver's place |
| `[illumination_interior]` | Interior lamps |

- `[passpos]` x, y, z is the **hip** when the seat height is above zero, the **foot** when it is
  zero (a standing place). openOMSI accepts one or two script variable names on the lines after
  it (see [Modding overview](MODDING.md)).
- `{noticketsale}`: passengers using that door walk straight to a seat. `{noticketsale}` and
  `{withbutton}` are lines of their own that mark the entry read last, wherever they stand.
- Coupled sections: the rear section's seats and exits are numbered after the front's. OMSI
  joins two cabins only where both give link points. openOMSI also crosses a bus joint without
  them, between the ends of the aisles, but not a trailer on a lorry's hitch
  (`[coupling_front_character]` type 0): such a trailer is a cabin of its own.

**Walking paths** (`paths.cfg`, named by `[paths]`): `[pathpnt] x y z` (vehicle frame) with
`[next_roomheight]` and `[next_stepsound]`, `[pathlink] a b`, `[pathlink_oneway] a b`,
`[stepsoundpack]`. Passengers walk entry → cash desk → seat, and seat → nearest exit.

### Doors and stops

Entries and exits are numbered in file order; that number is how engine and script talk.

1. The engine writes `PAX_Entry<i>_Req` while someone at the kerb wants in through entry `i`,
   and `PAX_Exit<i>_Req` while a rider wants out through exit `i`.
2. The script answers with `PAX_Entry<i>_Open` / `PAX_Exit<i>_Open`. Only then does anyone move.
3. Writes to an index the cabin does not declare are ignored, so one script serves the two-door
   and three-door variants of a bus.
4. Pressing an outside opener fires the trigger `door_aussenoeffner`.

The AI door scripts keep the rear door open only while a `PAX_Exit<i>_Req` stands, so every
rider on the way out keeps asking.

`AI_Scheduled_AtStation` is a handshake with an AI vehicle's script:

1. The engine writes 1 at a stop; the script opens the doors.
2. The engine writes -1 when the bus is to leave; the script closes the doors, releases the stop
   brake and writes 0 back.
3. The bus pulls away once it reads the 0.

### Mirrors and shadows

- `[add_camera_reflexion] x y z dist fov yaw pitch` (`_2` adds one number) is a mirror camera.
  Yaw is clockwise from forward (201 = back, turned towards the flank). Camera N draws into the
  texture `reflexionN.bmp` on the mirror mesh, whose UVs are already mirrored. A rear section's
  cameras are numbered after the front's.
- An `[isshadow]` mesh (`D_schatten.o3d` with `Shadow.tga`) is the shadow blob at model z = 0.
  A vehicle's z = 0 is where its tyres touch with the springs *unloaded*, so at rest it is 10-16
  cm under the road. openOMSI lays the blob on the plane the wheels stand on and leaves it out
  of the sun shadow map.

### Depot files (.hof)

A `.hof` in the vehicle folder holds a map's destinations and stops for the displays and IBIS.

| Keyword | What it is |
| --- | --- |
| `[name]` | Depot name (`$.yard` in scripts) |
| `[global_strings]` | Global strings; string 0 is the announcement folder |
| `[servicetrip]` | Out-of-service destination |
| `[stringcount_terminus]`, `[stringcount_busstop]` | Strings per record |
| `[addterminus]`, `[addterminus_allexit]`, `[addterminus_list]` | Destination: code, ident, then the strings |
| `[addbusstop]`, `[addbusstop_list]` | Stop |
| `[infosystem_trip]`, `[infosystem_busstop_list]` | Routes for the IBIS |

Depot callbacks: `GetRouteIndex(code)` (code = line × 100 + route), `GetRouteTerminusIndex`,
`GetTerminusCode`, `GetTerminusIndex`, `GetTerminusString(idx, n)`, `GetBusstopCount`,
`GetRouteBusstopIdent`, `GetBusstopIndex("ident")`, `GetBusstopString(idx, n)`,
`GetDepotStringGlobal(n)`. Called with index -1 (what a lookup answers for an unknown code)
they return "" or -1, never entry 0. openOMSI adds `GetRadioName` and `GetRadioSong`.

- AI buses get their destination the original way: string `SetLineTo` + `AI_target_index` and
  the `ai_scheduled_settarget` trigger. This also happens when the depot file has no row for the
  trip's terminus (`AI_target_index` then stays as it was).
- When the bus folder lacks the depot file the map's `ailists.cfg` wants, openOMSI takes it
  from another vehicle folder. A bus of a plain `[aigroup_2]` pool takes the map's depot of its
  folder, else the folder's first.
- The FloFix "Atron" IBIS starts in a PIN mode (`IBIS_mode` 10): type the `PIN` constant of its
  constfile and confirm before the other keys work.

**Announcements.** IBIS-2 builds `..\..\Announcements\<folder>\<stop ident>[_#terminus].wav`
(relative to the sound folder; `<folder>` is the hof's global string 0) and fires
`(T.F.ev_IBIS_Ansagen)`, a trigger that takes the file from the string stack. A sound.cfg
`[sound] N` entry (a number instead of a file) listening to that trigger plays it.

## Models (.cfg and .o3d)

A `model.cfg` lists a vehicle's or object's meshes and their materials, animations and lights.
The meshes are `.o3d` files (or `.x`, glTF, FBX, OBJ) in the same folder.

```text
[LOD]
0.02

[mesh]
body.o3d

[matl]
body.bmp
0

[matl_envmap]
envmap.bmp
0.5

[mesh]
door.o3d

[newanim]
origin_from_mesh
anim_rot
door_0
90
```

### model.cfg keywords

| Group | Keywords |
| --- | --- |
| Detail | `[LOD]`, `[VFDmaxmin]`, `[detail_factor]`, `[tex_detail_factor]`, `[noDistanceCheck]` |
| Meshes | `[mesh]`, `[mesh_ident]`, `[viewpoint]`, `[shadow]`, `[isshadow]`, `[mouseevent]`, `[visible]`, `[item]`, `[setvar]` |
| Skinning | `[setbone]`, `[smoothskin]`, `[animparent]` |
| Animation | `[newanim]` + `origin_trans`, `origin_rot_x/y/z`, `origin_from_mesh`, `anim_rot`, `anim_trans`, `offset`, `delay`, `maxspeed` |
| Materials | `[matl]`, `[matl_change]`, `[matl_item]`, `[matl_alpha]`, `[matl_noZwrite]`, `[matl_noZcheck]`, `[matl_Zbias]`, `[matl_allcolor]`, `[matl_freetex]`, `[matl_lightmap]`, `[matl_nightmap]`, `[matl_bumpmap]`, `[matl_envmap]`, `[matl_envmap_mask]`, `[matl_envmaprealtime]`, `[matl_transmap]`, `[matl_raindropmap]`, `[matl_texadress_mirror/clamp/border/mirroronce]`, `[texcoordtransX/Y]`, `[alphascale]`, `[texchanges]` |
| Textures by script | `[CTC]`, `[CTCTexture]`, `[scripttexture]`, `[useScriptTexture]`, `[texttexture]`, `[texttexture_enh]`, `[useTextTexture]` |
| Lights | `[illumination]`, `[illumination_interior]`, `[light]`, `[light_enh]`, `[light_enh_2]`, `[spotlight]`, `[interiorlight]` |
| Particles | `[smoke]`, `[particle_emitter]`, `PS_attachTo` |
| Ground | `[terrainhole]` |

openOMSI adds `[visible_range]` (and `!` / `||` in `[visible]`), `[spotlight_2]`,
`[spotlight_cookie]`, `[matl_glow]`, `[led_glow_effect]`, `[htmltexture]`/`[useHtmlTexture]`,
`[navigatortexture]`/`[useNavigatorTexture]` and more than four lamps in
`[illumination_interior]`. See [Modding overview](MODDING.md), [HTML textures](HTMLTEXTURE.md) and
[Navigator textures](NAVIGATORTEXTURE.md).

### Visibility and LOD

OMSI decides per **object** (scenery object, vehicle), never per mesh. It uses the model's
radius R, `[detail_factor]` D (default 1) and `[noDistanceCheck]`. With d the distance to the
camera and z the depth along the view:

- The object is dropped when `d > R + maxObjDist` or `(z − R) / maxObjDist > 1`. Both checks
  are skipped with `[noDistanceCheck]`.
- It is also dropped when `size / D < minObjSize`, where `size = 2R / (d · fov · π/180)`: the
  object's diameter over its distance as a share of the vertical field of view.
- The values come from `options.cfg`: `[performance_maxObjDist]` (750 in the shipped file),
  `[performance_minObjSize]` (0.013) and `[performance_minObjSizeRefl]` (0.046) for reflections.
- A detail factor above 1 makes an object vanish sooner (clutter); below 1 keeps it longer.

**`[LOD] x`** appends x to the model's list; the meshes after it belong to that level. With the
same `size` (not divided by D), OMSI takes the **first** level in file order whose x ≤ size,
else the **last** level. A model with one `[LOD]` is drawn at any size. A mesh written before
the first `[LOD]` joins the first level.

Lights count whatever level their mesh belongs to: 40 stock models declare theirs after the far
`[LOD] 0` mesh.

### Animations (`[newanim]`)

```text
[newanim]
origin_rot_y
-90
anim_rot
door_0
90
```

- `anim_rot` turns about the **x axis** of the origin frame; `anim_trans` slides along it.
  Doors turn that axis up with `origin_rot_y -90`, wheels spin about it directly.
- `origin_from_mesh` takes the pivot matrix stored in the `.o3d`: its first row is the axis,
  its fourth row the origin.
- Several blocks on one mesh compose **in file order**: the first transforms the mesh first,
  and later ones act on the result. The SD200's door leaves are the test case (−170° about the
  fold, then +80.5° about the post).
- OMSI evaluates rotations in its left-handed Direct3D frame. In a right-handed frame
  (x right, y forward, z up) every angle changes sign.

### Materials

**`[matl] texture index`** starts a material slot. Commands after it change that slot.

**`[matl_change] texture index variable`** + one `[matl_item]` makes a variant switched by a
variable:

- The variable is rounded to the nearest whole number. Item n shows for 1 ≤ n ≤ the number of
  items, the plain material otherwise. A variable no script declares is registered at 0.
- Commands between `[matl_change]` and its first `[matl_item]` change the plain material.
- Each `[matl_item]` starts as a copy of the plain material at that point and may replace its
  alpha mode, transmap, light map and so on.
- `[matl_transmap]` + `[matl_alpha] 2` items are washer water or condensation that exist only
  while the variable is on.
- `[matl_allcolor]` items (diffuse rgba, ambient rgb, specular rgb, emissive rgb, power) tint the
  variant and add `texture × emissive` self-illumination.

**`[matl_nightmap]`** is a self-illumination texture.

- In a `[matl_item]` it glows at full strength while the variable is on, by day as well
  (warning lamps, dashboard screens).
- On a vehicle's plain `[matl]` it glows whenever the mesh is drawn, by day as well (dashboard
  lamps switched by `[visible]`).
- On a scenery object it fades in with the night. Street lights use `NightlightA`, bus panels
  `elec_busbar_main`.

**`[matl_lightmap]`** is a light *mask* multiplied with the diffuse texture, scaled by its
variable (the SD202's `D86_02_L1.bmp` masks the lit atlas regions).

**`[matl_envmap] tex factor`**: reflectivity = diffuse alpha × factor, the factor saturating at
1. SD202 bodies write `10` with paint alpha 0.12-0.19 (a gloss); windows have alpha 0.5 and
factor 1. A texture without alpha (a DXT1 livery) does not reflect. The sphere map has the sky
at the bottom and is read as Direct3D's sphere mapping does.

**`[matl_transmap]`** uses its texture's *alpha* as the slot's alpha. A picture without an
alpha channel is opaque. `[matl_transmap] \S:n` uses script texture n.

**`[matl_freetex] <texture> <string variable>`** uses a file name the script builds at run time.
The SD200's roller sets `Rollband_Tex_V` from the depot and terminus strings, so
`Linienlisten\<yard>_RLB.jpg` resolves.

**`[texcoordtransX/Y] <variable>`** scrolls **one material slot's** texture, not the whole mesh.

**openOMSI extensions** (OMSI skips them as unknown lines):

- `[matl_glow] <texture> <value>` makes the slot its own light. The texture is a greyscale mask
  (white full, black none); the light is the material's own colour. `value` is a brightness on
  the scale of the **Led glow** setting (`6` matches its default). Only Enhanced draws it. It
  uses the light map's slot, so a slot with a `[matl_lightmap]` keeps that and has no glow. The
  last `[matl_glow]` of a slot counts.
- `[led_glow_effect] <value>` (0..1) is how much of the **Led glow** setting the slot's dots burn
  with. `0` means the slot is no LED panel. Without it, Enhanced guesses: a `\S:n` script mask
  under an all-white light map is taken for an LED panel.

### Smoke particles

`[smoke]` has nineteen lines, numbers or variable names:

| Lines | Meaning |
| --- | --- |
| 1-3 | Position |
| 4-6 | Direction |
| 7-8 | Speed and its spread |
| 9 | Frequency (per second) |
| 10 | Lifetime (s) |
| 11 | Brake factor (per 1/20 s, at any frame rate) |
| 12 | Gravity (9.81 × gravity × dt off the vertical speed; negative lifts) |
| 13-14 | Start size, growth per second |
| 15 | Initial alpha (the final alpha is 0, so a puff fades out) |
| 16 | Skipped |
| 17-19 | Red, green, blue |

A puff is a square facing the screen, turned by a random angle, blended with `rauch.tga`'s
alpha. OMSI lets puffs fall through the road, so the road cuts them off in a straight line.

> **Note:** openOMSI fades a puff out over the lowest 6-25 cm above the ground it started over
> and leaves out a puff wholly under it. OMSI also draws a puff's level speed towards the wind;
> openOMSI does not yet, so puffs slow to a standstill.

### Lights

- `[light_enh]`: position, rgb 0-255, size (m), fading variable (name or constant), brightness
  factor, z offset, effect bits, fade time, optional texture. It is read into the same lamp as a
  `[light_enh_2]` (omnidirectional, turned to the viewer).
- `[light_enh_2]`: position, direction, up, omni, rotating, rgb, size, cone inner/outer,
  variable, factor, z offset, parameters, cone, time constant, bitmap.
- `[spotlight]` (vehicle): position, direction, rgb, range, inner, outer. The active one is
  chosen by `Spot_Select` (negative = none).
- `[interiorlight] variable range r g b x y z`.
- `[maplight] x y z r g b radius` (scenery): a point light at night, full within `radius`,
  inverse-square beyond, out of range at six times it. The colour is the brightness: a petrol
  station's sign declares 0.1 red over 20 m.

openOMSI's `[spotlight_2]` and `[spotlight_cookie]` (a lamp whose beam is a picture) are in
[Modding overview](MODDING.md). In Enhanced with shadows on, the lamps on the front of the player's
vehicle that light the road share one shadow map, so what stands in their light throws a
shadow ahead.

### Script textures

`[scripttexture] w h` declares an RGBA image per index. `[useScriptTexture] n` puts it on a
material; `[matl_transmap] \S:n` uses its alpha.

Scripts draw with `(M.V.ST...)` callbacks, arguments pushed in order (index first):

| Callback | What it does |
| --- | --- |
| `STNewTex(i)`, `STLock(i)`, `STUnlock(i)` | Start, lock, upload |
| `STFilter(i)` | Filter the texture |
| `STSetColor(i, a, r, g, b)` | Drawing colour |
| `STDrawPixel(i, x, y)`, `STDrawRect(i, x1, y1, x2, y2)` | Draw |
| `STTextOut(i, x, y, font, mode, spacing, "text")` | Text (below) |
| `STReadPixel(i, x, y)` + `STGetR/G/B/A(0)` | Read a pixel |
| `STLoadTex("path", i)` | Load a picture; pushes nothing |
| `STCopyColor(a, b)` | Copy |
| `GetFontIndex("name")` | Register an `.oft` font; **-1** if it does not exist |
| `TextLength(font, "text")` | Drawn width (glyph advance plus the font gap) |
| `NrSpecRandom(seed)` | Stable pseudo-random number in [0, 1) |

- `STTextOut` mode bit 0: colour from the font's colour bitmap, else the `STSetColor` colour;
  alpha from the font's mask. `mode & 3 == 2` writes only covered pixels.
- `STLoadTex` paths are relative to the object's `Texture` folder, then the global `Texture`.
- Fonts are found by `[newfont]` name in every content root's `Fonts`. openOMSI draws a missing
  weight with another of the same family and size ("churafont++ Numeric 26x11 Bold" →
  "churafont++ Numeric 26x11").
- A work texture keeps its picture in the colour channels with alpha 0; LED textures keep it in
  alpha only, one texture per LED colour.
- In colour mode a glyph pixel takes the colour bitmap's pixel at the same row and column, often
  a small one-colour swatch. OMSI stops drawing text at a row past the colour bitmap's height;
  openOMSI repeats the colour bitmap instead.

### .o3d (binary mesh)

```text
84 19            magic
ver              u8   (1, 3, 4, 5, 7 seen)
[flags]          u8   (ver >= 3)  bit 0: 32-bit triangle indices
[key]            u32  (ver >= 4)  usually 0
sections, tagged by one byte, until the end of the file:
  0x17 vertices  count (u16, or u32 if ver >= 3) × {x y z nx ny nz u v} f32
  0x49 triangles count (u16/u32) × {i0 i1 i2 (u16 or u32), material u16}
  0x26 materials count u16 × {diffuse rgba, specular rgb, emissive rgb, power} f32 × 11, name (u8 length + bytes)
  0x79 matrix    16 × f32 (row-major, row 3 = translation)
  0x54 bones     count u16 × {name (u8 length), weight count u16 × {vertex index (u16/u32), weight f32}}
```

- Stock content always has the order `17 49 26 79 [54]` (checked on all 4026 stock files).
- Any other byte where a tag is expected is skipped, one byte at a time. Mods rely on it: the MB
  Sprinter 412D XLWB pack has junk bytes inside the matrix.
- **Axes:** X right, **Y up, Z forward** (Direct3D). Vertices are in the parent frame. The 0x79
  matrix is the mesh's pivot frame for `origin_from_mesh`, not applied when drawing. The engine
  swaps Y and Z to get its world frame (X east, Y north, Z up).

**Scrambled vertices (version ≥ 4).** The `key` (`0xFFFFFFFF` = none) selects the original
loader's vertex scrambling, which openOMSI reproduces:

```text
state = (key + version - 4 + (flags&2 ? 0x17D : 0)) mod 0xFDE8     (u16)
n     = vertex_count mod 0xFDE8;  b = 0
for each vertex (raw x y z nx ny nz u v):
    if key == 0: state = flags&2 ? 0x130 : 0
    state = (state*n + n*b) mod 8000
    b     = trunc(|frac(x)·frac(y)·frac(z)|·600) mod 256          (from the raw position)
    if state < 1000: swap(x,y) elif state < 3000: swap(x,z) elif state > 7000: swap(y,z)
    if state % 4 == 0: nx = -nx;  if state % 6 == 0: ny = -ny;  if state % 7 == 0: nz = -nz
    if state < 600: swap(ny,nz) elif state > 4500: swap(nx,ny) elif state > 6500: swap(nx,nz)
    if state % 5 == 0: u -= (state%100)² / 10000
    if state % 3 == 0: v -= (state%50)²  / 2500
```

Triangles, materials, matrix and bones are stored plainly. OMSI also refuses keys that are not
in its registration list; openOMSI does not check that.

### DirectX .x meshes

Text `.x` files (`xof 0303txt`) are flattened as `D3DXLoadMeshFromX` does:

- Every `Frame`'s `FrameTransformMatrix` applies to the meshes inside it, parent after child.
  The 16 numbers are row-major for row vectors (translation in elements 12-14).
- Normals are transformed by the inverse transpose (for unevenly scaled frames).
- A data object may carry a name (`FrameTransformMatrix relative {`), as the 3ds Max exporter
  writes it.
- `DeclData` inside a `Mesh` carries per-vertex data as a Direct3D 9 vertex declaration. Its
  first NORMAL and TEXCOORD replace `MeshNormals` / `MeshTextureCoords`; packed types widen to
  floats. Without it such meshes (the PAD-Labs houses, the Köln trees) come out in one colour.

Binary `.x` (`xof 0303bin 0032`, or `0064` for double floats) holds the same objects as
little-endian tokens and loads like the text files. The compressed `tzip`/`bzip` variants are
refused.

### Other model formats (openOMSI)

A `[mesh]` may also name a **glTF 2.0** (`.gltf` with its `.bin`, or `.glb`), **FBX** (binary or
ASCII) or **Wavefront OBJ** (with its `.mtl`) file. These are right-handed with y up; openOMSI
turns z round, so a model built in Blender with the front along +y comes in facing the way it
faced in the modeller. glTF and FBX bring their colour textures (as files or packed), skins and
morph targets; OBJ brings `Kd`, `d`/`Tr` and `map_Kd`. See [Tutorial 5: your own models](MAPPER_MODELS.md).

## Scripts (.osc)

Scripts are the reverse-Polish programs that drive vehicles and objects. They live in a
vehicle's `script\` folder, listed by the `[script]` file with their varlists and constfiles.

```text
{frame}
    (L.L.door_0) 0.9 >
    {if}
        1 (S.L.PAX_Entry0_Open)
    {else}
        0 (S.L.PAX_Entry0_Open)
    {endif}
{end}
```

### Stack and operators

- An 8-float **working stack** and a 10-float **register file** (`l0..l9` load, `s0..s9` store
  without popping).
- A value pushes onto `st[0]`; everything shifts down and `st[7]` is dropped.
- **Binary operators pop both operands and push the result.** Popping refills `st[7]` with 0, so
  an underflow reads zeros. (`a b -` means `a - b`.)
- Binary: `+ - * / % = < > <= >= && || min max`. Division and `%` by zero give 0.
- Unary (replace `st[0]`): `sin arcsin arctan exp sqrt sqr sgn abs trunc ! /-/` (negate).
- `d` duplicates. `pi` pushes π. `random` replaces `st[0]` by a whole number in `[0, n)` with
  `n = |round(st[0])|`.
- Operator names are compared as spelled: `Min` and `$CutEnd` are not operators.

### Variables

Access is `(X.Y.name)`:

| 1st letter | Meaning | 2nd letter | Meaning |
| --- | --- | --- | --- |
| `L` | Load a float | `L` | Local (object) variable |
| `S` | Store `st[0]` | `S` | System variable (`Timegap`, `GetTime`, ...) |
| `C` | Constant from a constfile | `M` | Map variable |
| `F` | Curve from a constfile: pushes `curve(st[0])` | `$` | String variable (`L.$`, `S.$`) |
| `M` | Macro call (`M.L.name`) or callback (`M.V.name`) | `V` | Host callback |
| `T` | Sound trigger (`T.L.name`, `T.F.name`) | | |

System variables (`varlist_system.txt`): `Timegap GetTime NoSound Pause Time Day Month Year
DayOfYear mouse_x mouse_y PrecipType PrecipRate coll_pos_x/y/z coll_energy Weather_Temperature
Weather_AbsHum wearlifespan AutoClutch SunAlt`.

- `mouse_x`/`mouse_y` are the mouse movement this frame while a `[mouseevent]` mesh is held. Its
  `<event>_drag` trigger runs every frame the button is down, with 0 when the hand is still.
- `AutoClutch` is 1 unless the options say `[no_automaticClutch]`. At 0, a gear put in without
  the clutch pedal is thrown out at once.

### Strings

`"literal"` (may contain spaces) pushes onto the string stack. String operators: `$+ $= $d $*
$< $> $<= $>= $length $msg $RemoveSpaces $cutBegin $cutEnd $SetLengthL $SetLengthR $SetLengthC
$IntToStr $IntToStrEnh $StrToFloat`.

| Operator | Behaviour |
| --- | --- |
| `$IntToStrEnh` | Format string: first character is the pad, the rest the width (`"02"`, `" 2"`) |
| `$length` | Pushes the length and leaves the string |
| `$StrToFloat` | -1 for text that is no number |
| `$SetLengthL n` | Keeps the left n characters, or pads on the right |
| `$SetLengthR n` | Keeps the right n characters, or pads on the left |
| `$SetLength*` | Leave n on the number stack (unlike `$cutBegin`/`$cutEnd`) |
| `$msg` | Shows the top string without popping it |

A `$` word that is not a string operator compiles to nothing, silently (a lone `$`, `$=>`,
`$SetLengthM` in mods). Any other unknown word is an error.

### Blocks and flow

- Blocks: `{init}`, `{frame}`, `{frame_ai}`, `{macro:name}`, `{trigger:name}` ... `{end}`.
- Only `{end}` ends a block. A block without it runs on into the next block's code, across
  script files too (the Opel Manta's `antrieb_frame` into `calc_eng_M`), through the words
  before the next file's first header, at the same macro depth.
- Block words are case-sensitive: `{End}` is not `{end}`.
- A `(M.L.name)` finds only a macro defined **below** it, in the script files taken in their
  listed order (Omsi.exe builds the word stream from its end). A macro defined above its call,
  or the macro itself, is `macroinvalid`, and the call does nothing.
- `{if}` ... `{else}` ... `{endif}` tests `st[0] != 0` and does **not** pop it. The stock
  scripts depend on it (`(L.L.bremse_feststell) {if} ! (S.L.bremse_feststell)`).
- A reached `{else}` jumps to the next `{endif}`, even when a stray `{endif}` already closed its
  `{if}` (the Procity's dashboard script has one).
- Comments start with `'`. `%stackdump%` writes the stack to the log.
- Macro, trigger and variable names are case-insensitive.
- A trigger or macro defined in two script files is the **later** file's. The O530 Citaro's
  `engine.osc` redefines `cp_batterietrennschalter_toggle` of its `cockpit.osc`.

### Constfiles and varlists

```text
[const]
max_speed
80

[newcurve]
torque

[pnt]
0
0

[pnt]
2000
1200
```

- `[const] name value`, `[newcurve] name` followed by `[pnt] x y` points (piece-wise linear,
  clamped at the ends).
- Constants and curves keep the **first** definition of a name (case-insensitive), within a
  file and across constfiles in their listed order.
- Varlists and stringvarlists: one name per line.
- A `(F.L.name)` whose curve no constfile defines is logged as an error and gives 0.

OMSI's error names: `SC_ErrorInCommand_{varinvalid,macroinvalid,constantinvalid,functioninvalid}`,
`SC_ErrorInConstfile`, `SC_pnt_before_newcurve`.

### Compatibility patches

A few mod scripts cannot show what they were written for under these rules. openOMSI patches
them before compiling, recognised by their text:

- LiAZ 5292 line matrix: the line number is written with `" 3" $IntToStrEnh`, so line 5E shows
  as `  5E` instead of blank or `   E`.
- Aachen's ibox: route entry uses a string digit buffer, so typed `0` then `1` shows `01` and a
  lone `1` shows `1`, without automatic leading zeros.

## Textures

Textures live in a `texture\` folder beside the model, or in the global `Texture` folder. The
supported file formats and the texture search order are in [Modding overview](MODDING.md); PBR maps
are in [PBR materials](PBR.md).

### Texture sidecar (`<texture>.<ext>.cfg`)

```text
[surface]
2

[moisture]

[puddles]
```

| Keyword | What it does |
| --- | --- |
| `[terrainmapping]` | Map the texture in world coordinates |
| `[moisture]` | The surface darkens when it rains |
| `[puddles]` | The surface collects puddles |
| `[surface] n` | Material: 0 asphalt, 1 concrete, 2 cobblestone, 3 dirt, 4 grass, 5 gravel, 6 snow, 7 deep snow. Scripts read it as `Axle_SurfaceID_` |

### Seasons

global.cfg `[addseason] kind start_day end_day` (day of year): 1 spring, 2 autumn, 3 winter,
4 winter with snow, 5 dry summer. Season textures live in `texture\Spring`, `Fall`, `Winter`,
`WinterSnow` and `SummerDry` subfolders, with the same file names, and take precedence.

- A texture without a winter picture takes its autumn one.
- A weather with `[snow]` takes the `WinterSnow` pictures whatever the season. With
  `[snowOnRoad]` as well, `WinterSnowfall` pictures come first.
- A texture with no snow picture takes its winter one, else its autumn one.

### Textures OMSI reads that other loaders refuse

- A palette bitmap counting more colours than its bit depth holds (4-bit with `biClrUsed` 17:
  the A21's and Urbino's `LCD-Innenanzeige.bmp`).
- DDS files whose fourcc is a D3DFORMAT number (36, 113, 116: 16-bit and float RGBA).

### On the graphics card

openOMSI keeps textures compressed on the GPU where it can:

- DDS files with DXT1-DXT5 (and DX10 BC1-BC3) data go up as they are, with the file's own mip
  chain. A file without one gets levels made the way D3DX makes them. A DXT1 block with
  c0 ≤ c1 using index 3 is transparent black; such a file counts as having alpha.
- Uncompressed files (BMP, TGA, JPG, PNG, raw DDS) are compressed on loading: to BC1 without
  alpha, else BC3. This happens only when the result is close enough (colour PSNR ≥ 33 dB,
  alpha ≥ 30 dB); noisy cobbles and asphalt stay RGBA.
- Sides that are not multiples of four are resampled to the nearest multiple. Pictures with a
  side under 128 texels, or under 64² texels in all, stay RGBA.
- A tile's own pictures (light map, road cut, paint masks) keep one level. A mask of only 0 and
  255 becomes 1-bit BC1, a soft one BC3.
- Text and script textures and render targets stay RGBA. Scenery `[texttexture]` pictures are
  shared when they show the same thing.
- Compressed textures are cached in `~/.openomsi/cache/textures`.

`options.cfg`'s `[texture]` (two values) and `[texmemlimit]` (401.0 MB in the stock file) are
not imported. openOMSI uses its own `texture_compression=` (on by default) and `video_memory=`
(MB for everything on the card; nine tenths of a discrete card's memory when unset). See
[Graphics](GRAPHICS.md). For tests, `OMSI_NO_BC` and `OMSI_NO_TEXCOMPRESS` turn
compression off (see [Environment variables](DEBUG_FLAGS.md)).

## Sounds

A `sound.cfg` lists a vehicle's or object's sounds and when they play. It lives in the
`sound\` folder, named by `[sound]` in the `.bus`, `.ovh` or `.sco`.

```text
[loopsound]
kompressor@600.wav
44100
engine_n
600
1

[conditionSingle]
engine_n
200
3

[sound]
door_close.wav
1

[trigger]
ev_doorclose_0
```

| Keyword | Parameters |
| --- | --- |
| `[sound]` | File, volume. A number instead of a file plays the file a trigger hands over (announcements) |
| `[loopsound]` | File, sample rate, pitch variable, its reference value, volume |
| `[next_random] n a b` | The next entry is used only when the vehicle's random number n lies in [a, b) |
| `[3d] x y z range` | Positioned sound with a range |
| `[dir] x y z` | Direction |
| `[noloop]` | Play once when the conditions start to hold |
| `[important]`, `[checkloading]`, `[onlyone]` | Flags |
| `[viewpoint] n` | Where it is heard: 1 the player's bus from outside, 2 from its cab, 4 other vehicles (a mask; none: everywhere) |
| `[volcurve] variable` + `[pnt] x y` | Volume curve over a variable (linear, held at the ends) |
| `[conditionSingle]`, `[conditionInt]` | Variable, **value**, then relation |
| `[conditionBool]` | Variable, value (equality) |
| `[trigger] name` | Play when this trigger fires |

Relations: 0 `<>`, 1 `=`, 2 `<`, 3 `>`, 4 `<=`, 5 `>=`. So `engine_n 200 3` means "the engine
runs" and `velocity 2 2` means "standing".

How an entry plays:

- With a `[trigger]` it plays once each time the trigger fires, from the start and never looped,
  **without looking at its conditions**.
- Without a trigger it loops for as long as its conditions hold and it is audible (horn, rain,
  compressor). `[sound]` and `[loopsound]` both loop by default.
- `[noloop]` makes it play once when its conditions start to hold.
- A `[loopsound]` plays at `|pitch variable| · rate / reference` Hz and is silent below 100 Hz.
- A `[3d]` sound fades with `min(range / distance, 1)`. A sound without `[3d]` is heard at the
  same level everywhere.
- The rear section of an articulated bus has its own `[sound]` and plays it on the shared
  scripts' triggers and variables.

**Outside sounds heard in the cab.** The player's bus's outside entries (`[viewpoint]` without 2) come in at
`Snd_OutsideVol` of their volume, as in OMSI. Traffic heard from the cab is at
`0.2 + Snd_OutsideVol`. openOMSI also filters the treble out of the bus's outside sounds: a
low-pass at about 500 Hz with everything shut, opening up to the full range at
`Snd_OutsideVol` 0.95. Walking through a door, cab and street sounds are mixed by how far
inside the bus the eyes are (0.4 m in: all cab).

**Radio variables.** OMSI plays no music itself. `Snd_Radio` (the cassette player of the stock
buses), `SndExt_Radio`, `SndVol_Radio` and `SndExt_RadioPlaylist` (the Sound Extension plugin)
are variables that plugins read. openOMSI plays internet stations for them; see
[Radio](RADIO.md) and the map's `radio.cfg` format there.

## Humans (.hum)

A `.hum` file describes a person: model, size and joints. Files live in `Humans\<group>\`.

```text
[model]
model\man01.cfg

[seatheight]
0.45

[humangeom]
0.25
1.78
```

| Keyword | Parameters |
| --- | --- |
| `[model]` | The person's `model.cfg` |
| `[seatheight]` | Seat height |
| `[humangeom]` | Feet distance, height |
| `[links]` | 22 numbers: hip xyz, knee xyz, waist yz, shoulder xyz, elbow xyz, neck yz, hand xyz, finger xyz |
| `[voice]` | Voice |
| `[walk_param]` | Walking parameters |
| `[mass]` | Mass |
| `[age]` | Age |

- `[links]` are for the right side, model frame x right / y forward / z up. The mesh is stored
  in a T-pose. Left is −x in every mesh checked.
- The model's `[setbone] name id` binds `.o3d` bones to engine ids: −2/−3 thighs L/R, −4/−5
  shins, −6/−7 upper arms, −8/−9 forearms, −10 hip, −11 torso, −12 head, −13/−14 hands.
- `[links]` values are not always right (the GSPNS man04 gives the hip at x 0.6). openOMSI takes
  a leg joint from the mesh when the given one lies outside its limb's vertices.
- Clothing variants work like paint schemes: `[CTC] Colorscheme <folder> 0` names a folder
  **relative to the .hum file** whose `.cti` items replace the `[CTCTexture] farbschema`
  texture.
- A map's `humans.txt` lists which people walk there. Berlin-Spandau and Grundorf name 15 stock
  types, so add-on people installed for another map never walk there.

## Timetables

Timetable files live in the map's `TTData\` folder.

| File | What it is |
| --- | --- |
| `Busstops.cfg` | `[busstop] name tile-index object-id offset 0 0` |
| `StnLinks.cfg` | Links between stops: `[StnLink] length from to ...` + `[StnLink_entry] id path tile length -1 0 0` |
| `*.ttp` | A trip |
| `*.ttr` | A track: the lane sequence of a trip |
| `*.ttl` | A line: tours and departures |
| `*.ocu` | Car use: which vehicles run which tours |
| `Trains\*.zug` | A train's cars (see [Vehicles](#the-vehicle-list-and-coupled-parts)) |

Tile index means the tile's position in global.cfg's `[map]` list.

### Trips (.ttp)

```text
[trip]

Hbf
5

[station_typ2]
12345

[profile]
Weekday
1.5
```

| Keyword | Parameters |
| --- | --- |
| `[trip]` | Track (empty for a bus going by its station links), terminus, line |
| `[trainreverse]` | The train runs reversed |
| `[station_typ2] object-id` | A stop |
| `[station]` | Old-style stop, 8 lines: object id, track entry index, name, tile index, four numbers |
| `[profile] name minutes` | A time profile |
| `[profile_man_arr_time]`, `[profile_man_dep_time]`, `[profile_otherstopping]` | Profile details |

A "type 1 (old)" trip (trains, and whole mod maps such as Novi Sad) uses `[station]` records
and runs on the track its `[trip]` names ("1 Klisa-Liman I" → `1_Klisa-Liman1.ttr`).

### Tracks, lines and car use

- `.ttr`: `[track_entry] id path-index tile-index internal-path-no length 0`. `id` is the spline
  or object id in that tile, `path-index` the `[path]` index in its `.sli`/`.sco`. The internal
  number is OMSI's per-tile path cache.
- `.ttl`: `[userallowed]`, `[priority]`, `[newtour] number ai-group extra`,
  `[addtrip] trip profile departure-minutes`.
- `.ocu`: `[valid]`, `[line]`, `[onlytypes]` ... `[end]`, `[types_prefered]`, `[number_tour]`.

Routing notes:

- AI buses follow a route in whichever direction it runs along a path. Station links and
  tracks do drive one-way paths backwards (invisible helper streets).
- A station link often runs a few paths past its station, while the next link starts at the
  stop on another path (122 of Spandau's 505 joins). Those extra paths are not driven.

## Weather and lighting

### Weather (.owt)

A weather preset lives in `Weather\`.

```text
[name]
Light rain

[fog]
8000
1

[clouds]
Overcast
1

[precip]
1
80
0
0
0

[temp]
12
10
```

| Keyword | Parameters |
| --- | --- |
| `[name]`, `[description]` ... `[end]` | Name and text |
| `[fog]` | Visibility (m), fog density |
| `[clouds]` | Type, value: `-1` clear, `Cumulus` 1..3, `Overcast` 1 |
| `[precip]` | Five numbers: kind (0 none, 1 rain, 2 snow), strength (0..255), three more |
| `[groundwet]` | Three numbers; the first is the ground wetness (0..255) |
| `[snow]`, `[snowOnRoad]` | Snow lies; snow lies on the road too |
| `[wind]` | Direction (degrees), speed (m/s) |
| `[temp]` | Temperature (°C), **dew point** (°C) |
| `[press]` | Air pressure (hPa, default 1013) |

- The second `[temp]` value is the dew point, not the humidity: saturated weathers such as
  ground fog write 9/9. openOMSI turns it into `Weather_AbsHum` (g/m³) as OMSI does.
- The cloud cover is drawn from `Texture\clouds.tga` (a grey density map) as a layer about
  1500 m up, drifting with the wind.

openOMSI moves weather forward through the hours and can use live weather (see
[Weather and seasons](WEATHER.md)). OMSI's `[currWeather_ICAO]` (weather from an airport's METAR report)
is read too: visibility, wind, temperature and dew point, pressure, cloud cover and
rain/drizzle/snow.

### envir.cfg

| Keyword | Parameters |
| --- | --- |
| `[sky_textures]` | Day, twilight, night sky |
| `[twilight_start_end]` | Sun altitudes of twilight |
| `[lightcolor_A]`, `[lightcolor_B]`, `[lightcolor_C]` | 5 RGB stops each (nadir, twilight start, sunrise, twilight end, zenith) for direct sun, light from above and ambient |
| `[cloudtype]` | Name, texture, height, code |

## Misc formats

### Fonts (.oft)

```text
[newfont]
Matrix
matrix_colour.bmp
matrix_alpha.bmp
16
1

[char]
A
0
7
0
```

`[newfont] name bitmap alpha height gap` + `[char] ch x0 x1 y`. Glyph pixels sit in columns
x0..x1 (x1 exclusive).

### Languages (.olf)

`KEY<TAB>text` lines; the first line is the language code.

### Money and tickets

- Currency: `[currency] name decimals`, `[coin] o3d value`, `[bill] o3d value`.
- Ticket pack (`.otp`): `[ticketpack]` (stamper probability, ticket-buying probability,
  chattiness, complaint probability), `[voicepath]`, and `[ticket]`/`[ticket_2]` blocks: name,
  English name, max stations, minimum age, maximum age, price, display string. `[ticket_2]`
  adds a day-ticket flag and a probability.
- Item lists (`.cti`): `[item] name variable texture` and `[setvar] name value`.

### Situations (.osn)

A saved game, UTF-16 LE with a BOM and CR LF:

| Keyword | Parameters |
| --- | --- |
| `[name]`, `[description]` ... `[end]` | Name and text |
| `[map]`, `[actuWeather]` | Map and weather |
| `[time]` | Year, day of year, h, m, s |
| `[centerkachel]` | Tile x, y |
| `[mapcam]` (6 numbers), `[egopos]` (5) | Cameras |
| `[TT_active]` | Timetable on |
| `[vehicle]` | File, x height y, 7 orientation numbers, tile x y, odometer km, paint |
| `[coupledWith]`, `[ismyVehicle]` | Per vehicle |
| `[vars] n`, `[stringvars] n` | Name/value pairs |
| `[settimetable]` | The vehicle's timetable |
| `[myvehicle]`, `[view]` | Player's vehicle and view |

openOMSI adds `[openOMSI_name_chosen]` (the player named the save), `[openOMSI_riders]` (number
of passengers aboard) and `[openOMSI_car]` (the variables of a coupled car with scripts of its
own, such as a tram's rear cab; OMSI keeps one set per train).

### Drivers (.odr)

The personnel file, UTF-16 LE with a BOM.

| Keyword | Contents |
| --- | --- |
| `[ident]` | Name, sex, date of birth, date of hire |
| `[busstops]` | Stops served, of those reached too late, and left too early |
| `[hektom]` | Distance driven in hectometres |
| `[crashs]` | Crashes, hurt pedestrians, abscondings, and of those the heavy ones |
| `[tickets]` | Tickets sold, takings |
| `[rating]` | Driving (0 excellent to 10 perilous), passenger comfort, ticket selling, then two accumulators |
| `[perbusinfo]` | Per-bus data |

The stock file has the two accumulators at zero; openOMSI uses them for the distance the ratings
were averaged over and the number of jolts.

### Descriptions (.dsc)

Translations of a name and description, in a file named after the stem: `global_ENG.dsc` beside
`global.cfg`, also for a `.bus`/`.ovh` or `.owt`. It holds `[name]` (or `[friendlyname]`) and
`[description]`. The German text is the file's own, so only other languages have a `.dsc`; a
language without one falls back to `_ENG`. They are ANSI like the rest, though some were saved
as UTF-16 with a byte-order mark.

### Input and options

- `keyboard.cfg`: `[game]` / `[vehicles]` + `[entry] name scancode modifier`. The modifier is a
  mask: 1 the action gets the key's state every frame (throttle, brake, steering), 2 Shift,
  4 Ctrl. openOMSI adds 8 for Alt, which OMSI ignores.
- `gamectrler.cfg`: `ctrl`, `axis`, `buttons`, `FFScale`.
- `options.cfg`: the `[performance_*]` keys and others. openOMSI imports what it uses; see
  [Launcher and settings](LAUNCHER.md).
