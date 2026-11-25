# Map packages

A map package is a pre-built picture of a map's surroundings that a map's author (or anyone)
ships with the map. The navigator and the city map draw it under the roads, with street names,
regions, icons, lines and relief. This page is for map authors and for people who write tools
that make packages.

> **Note:** A map needs no package. openOMSI draws every map's own picture from its tiles the
> first time the map is driven: ground, water, woods, trees, paved surfaces, rail and tram tracks,
> and houses. A package adds what lies **outside** the map's tiles (the real town around it,
> rivers, borders), plus named streets, regions, icons and lines. Inside the tiles, the map's own
> layers are drawn over the package. The map's own layers are cached in
> `~/.openomsi/cache/mapgen`; `OMSI_NO_MAP_CACHE=1` makes the game read them again.

openOMSI computes nothing from a package. It opens the files, picks the chunks near the camera
and draws them. All the work (data sources, georeferencing, triangulation) is done beforehand by
the package maker's tool. Omsi.exe ignores the folder, so a map made for both games loses nothing.

## Try the example

The repository has a small example in `docs/examples/openomsi-realmap/`: a lake, a river, a
railway, a border, a park, a wood, a hill with contour lines, three names, a street with a
30 km/h limit, two icons, one region and three lines. It uses the grid of the tutorial map
**Grundorf** and contains no OMSI 2 or OpenStreetMap data.

1. Copy the folder to `Maps/Grundorf/openomsi-realmap/` in the content folder.
2. Start a drive on Grundorf.
3. Open the navigator (`Shift+N`) and the city map (`Shift+M`).

Near the start of the map, the row under the next stop says "Exampleton | Example Street".

Real packages and the generator that makes them are in
[lmonteirotech/openomsi-map-packages](https://github.com/lmonteirotech/openomsi-map-packages).
Its releases hold test packages (Berlin-Spandau and Projekt Szczecin), and its README explains
how to make a package for any map. Their data is OpenStreetMap's, under the ODbL.

## Files

```text
<content folder or OMSI 2>/Maps/<Map>/
  global.cfg
  openomsi-realmap/      the package (fixed name, lower case)
    manifest.cfg         required
    surface.oms          surface, names and named streets
    markers.cfg          optional: icons
    regions.cfg          optional: municipalities
    icons/               optional: pictures for markers.cfg
    lines.lin            optional: the map's lines
```

- Folder and file names are fixed. Nothing is configured.
- Without `manifest.cfg`, the package is ignored. Any other missing file just turns that feature
  off.
- The folder may be in the map's own folder or in the same place of another content root. A
  package for a map you do not own goes into the game's content folder, like a mod, so the OMSI 2
  installation is never written to. The root with the highest priority wins.
- Other files in the folder are ignored.

## What the player sees

| Feature | Needs | Shown |
|---|---|---|
| Surface (water, parks, woods, built-up land, rivers, rail, borders) | `surface.oms` | Under the roads on the small map and the city map |
| Place names | Labels in `surface.oms` | City map |
| Credit | `credit` in the manifest | City map |
| Icons | `markers.cfg` | Small map and city map |
| Municipality and street | `regions.cfg` or named streets | Row under the next stop; city map with a speed-limit sign |
| **Lines** button | `lines.lin` | City map |
| **Terrain** button | Contour lines in `surface.oms` | City map |
| Compass | `north` in the manifest (optional) | City map |

- The small map (`Shift+N`) loads the chunks within its radius around the bus. The city map
  (`Shift+M`) loads all of them once.
- Nothing is fetched from the network.
- The **Map surface under the roads** setting (`nav_surface`, on by default) turns the surface
  on and off. `nav_light` (off by default) gives the maps a light look. Both are in the launcher's
  settings and the pause menu.

## manifest.cfg

`key = value` lines, `#` comments, UTF-8.

```text
format   = 1
grid     = 300
name     = My Map
credit   = (c) OpenStreetMap contributors (ODbL)
north    = 0
water    = #7fb2e5
light_water = #9cc3d6
```

| Key | Meaning | Default |
|---|---|---|
| `format` | Must be `1`. Any other value: the package is ignored. | Required |
| `grid` | The author's tile size in metres (see [Coordinates](#coordinates)). Greater than 1. | Required |
| `name` | For people | |
| `credit` | Shown on the city map. Give it when the data needs attribution. | |
| `north` | Degrees clockwise from the map's up (+Y) to real north. 180 for a map drawn upside down. | 0 |
| `terrain_offset` | Metres added to every contour line's height | 0 |
| Colour keys | `#rrggbb` for the dark look; prefix `light_` for the light look | The game's colours |

Colour keys: `land`, `water`, `river`, `green`, `grass`, `forest`, `sport`, `farm`, `built`,
`rail`, `rail_edge`, `border`. The layers have fixed meanings, so the game keeps the final say on
how they look.

## Coordinates

All positions are **metres of the author's tile grid**, the numbers OMSI 2 shows for the map's
objects:

- `x = tile_column × grid + position_in_tile` (east)
- `y = tile_row × grid + position_in_tile` (north)

openOMSI converts them to its own game metres with the grid it uses for the map. That is why the
manifest states `grid`: a `[worldcoordinates]` map's grid is not 300 m. No latitude or longitude
is stored, so a map may be bent, shortened or fictional.

## surface.oms

A little-endian binary file. Header:

```text
magic    "OMSF"
u32      version (= 1)
f64 x2   origin_x, origin_y         (author's grid metres)
f32 x4   min_x, max_x, min_y, max_y (relative to origin)
f32      chunk_size                 (metres, e.g. 2000)
u32      n_strings ; n_strings x { u16 len, utf-8 bytes }
u32      n_chunks
n_chunks x { i32 cx, i32 cy, u64 offset, u32 byte_len }   the index
... the chunks ...
```

- Positions are stored as `f32` offsets from `origin` (an `f64` pair) to keep precision on large
  maps.
- Chunk `(cx, cy)` covers `[cx × chunk_size, (cx + 1) × chunk_size)` on each axis, relative to
  `origin`. A chunk needs no other chunk.

Each chunk:

```text
u32  n_fills   ; n_fills x  { u8 class, u32 n_tri, f32[6*n_tri] }       triangles
u32  n_lines   ; n_lines x  { u8 class, u8 flags, f32 width_m, u32 name,
                              u32 n_pts, f32[2*n_pts] }                  polylines
u32  n_labels  ; n_labels x { u8 kind, u8 rank, u32 name, f32 x, f32 y } names
```

- Triangles are ready to draw. There are no holes to resolve.
- `name` indexes the string table. `u32::MAX` means no name.
- Unknown classes are skipped, so later versions can add classes.
- Anything wrong (short file, wrong magic or version, a chunk outside the file) means "no
  package". The reader checks every length and never crashes on a bad file.

### Fill classes

| Class | Meaning |
|---|---|
| 0 | Water |
| 1 | Green (parks, gardens) |
| 2 | Built-up land |
| 3 | Grass (grass, meadow, scrub) |
| 4 | Forest (wood, forest) |
| 5 | Sport (pitches, stadiums) |
| 6 | Farmland |

### Line classes

| Class | Meaning | `width_m` |
|---|---|---|
| 0 | River (drawn) | 5 or more: river; less: stream |
| 1 | Railway (drawn) | |
| 2 | Border (drawn) | |
| 3 | Named street (not drawn) | |
| 4 | Named street with limits (not drawn) | Speed limit in km/h; `flags` is the heavy-vehicle limit (0: same or none) |
| 5 | Contour line | Height above sea level, m |
| 6 | Index contour line (every fifth, drawn stronger) | Height above sea level, m |

### Labels

| `kind` | Meaning |
|---|---|
| 0 | District |
| 1 | Terminal |
| 2 | Town |
| 3 | Landmark |
| 4 | Water |

`rank` (0-255) is the label's importance, set by the author. Zoomed out, the city map shows names
of rank 200 and up; closer in, 140 and up, then 100 and up, then all. Names get a halo and never
overlap.

## markers.cfg

```text
# icon | width in pixels at interface size 1 | x | y | name
marker = icons/terminal.png | 30 | 1200.5 | 840.0 | Central Terminal
```

- `icon` is a PNG with straight alpha, up to 1024 × 1024, inside the package folder. No `..` in
  the path.
- All icons are packed into one texture. A line whose picture cannot be read is left out.
- `x`, `y` are grid metres. `name` is for people reading the file.
- Icons are drawn upright, without text, at the given width.

## regions.cfg

One region (municipality) per line, in grid metres:

```text
# region = name | label x | label y | box width | box height | polygon x,y x,y ...
region = Alphaville | 5200.0 | 3100.0 | 4800.0 | 520.0 | 1000.0,1000.0 9000.0,1000.0 9000.0,6000.0 1000.0,6000.0
```

- The polygon decides which municipality the bus is in.
- The box is where and how large the name is written. The author's tool computes it: the largest
  letters that fit inside the visible part of the municipality, one size for all. A box width of
  0 means no name is drawn.
- The city map writes each name as a large, faint watermark while zoomed out.

### Where the bus is

Named streets are lines of class 3 or 4 in `surface.oms`. The nearest piece within 30 m of the
bus names the street. With regions or named streets:

- the small map shows a row under the next stop with the municipality and the street;
- the city map shows the municipality, "You are on: *street*" and the street's limit as a road
  sign (a smaller second sign for heavy vehicles, when different).

## lines.lin

Tab-separated, one record per line, `#` comments. Positions are grid metres relative to `ORIGEM`.

```text
ORIGEM	0.0	0.0
L	110	BRT	North - Central	bus	19900101	0
R	18.9	35	35	0	Every day	Articulated	North Terminal - Central Terminal
P	1250.4 4428.6 1320.0 4330.8 ...
S	1260.8 4409.6 1310.0 4341.5 ...
```

| Record | Fields |
|---|---|
| `ORIGEM` | Origin x, y |
| `L` | Number, group (`BRT`, `EXP`, anything else is a feeder), name; optional vehicle kind (`bus` default, `tram`, `metro`, `rail`), valid from and to (yyyymmdd, 0 = open) |
| `R` | A route of the line (one or two, out and back): km, stops, minutes, circular (0/1), days, model, itinerary |
| `P` | The route's path, as x y pairs |
| `S` | The route's stops, as x y pairs |

A map with eras has an `L` record for each form a line took. The panel lists the forms valid on
the session's date.

With the file, the city map gets a **Lines** button:

- A panel shows the line in focus: length, stops, trip time, days, model, route 1 or 2.
- The list of lines has a tick box each, in tabs by vehicle kind and, for buses, by group.
- One click on a line ticks it, a second opens its data.
- A ticked line is drawn in its own colour with its stops. Vehicles running it (by the line they
  show) get a ring in that colour, and the card counts them.

The route of the player's own trip is not changed.

## Contour lines

Contour lines are lines of class 5, with index lines (every fifth) as class 6. Their `width_m` is
the height above sea level in metres. Choose a spacing that suits the land: 1 m in a city, 5 m on
hills. `terrain_offset` in the manifest is only for packages whose lines use the editor's own
heights.

With contour lines, the city map gets a **Terrain** button. The terrain view shows only water,
rivers, rail and contour lines, with heights written on them: on all lines up close, only on index
lines further away. From far away the lines fade so that the streets stand out.

## Compass

The city map has a compass at the bottom right. Its red tip points to real north, as the
manifest's `north` says (without it, to the map's up). Click it to change the map's orientation:

| Mode | Up is |
|---|---|
| **Map** | The map's own up (the default) |
| **North** | Real north |
| **Heading** | The direction the bus faces; the view turns smoothly with it |

Street names, stops, vehicles, the route and houses turn with the map. The mouse still drags and
zooms where it points.
