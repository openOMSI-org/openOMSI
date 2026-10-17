# Map packages: a pre-built surface under the roads

> **A map needs no package.** openOMSI reads every map's own picture off its tiles the
> first time the map is driven (`omsi_scene::mapgen`, kept in `~/.openomsi/cache/mapgen`): the
> ground by what it is painted with, water, woods and trees, every paved surface as the splines
> and objects lay it, rail and tram tracks, and the houses from their models' roofs with their
> heights. A package as described here adds what lies **outside** the map's tiles (the
> real town around a map, rivers and borders), named streets, regions, icons and lines; inside
> the tiles the map's own layers are drawn over its surface. `OMSI_NO_MAP_CACHE=1` reads the
> layers again; the example `mapgen_dump` (crate `omsi-scene`) draws them into a picture.

A map's author (or anyone) can ship a **pre-built** picture of the map next to the map itself. openOMSI
does not compute, convert or triangulate anything: it opens the package, picks the chunks near the
camera and draws them under the roads of the navigator and the city map. All the work (where the data
comes from, georeferencing, triangulation) is done once, beforehand, by the tool of whoever makes the
package.

Why a package and not "openOMSI reads OpenStreetMap or GeoJSON": loading costs a file read, never a
geometry pipeline; the game never needs to know where the data came from; any author (a real city, a
fictional map, any tool) produces the same files.

It follows the precedent of `radio.cfg`: optional files in the map's folder, read only by openOMSI and
ignored by Omsi.exe, so a map made for both games loses nothing. A map without the folder is drawn from its
own tiles alone.

## 1. Location and files

```
<content folder or OMSI 2>/Maps/<Map>/
  global.cfg
  openomsi-realmap/            the package (fixed name, lower case)
    manifest.cfg               required: its presence makes the package count
    surface.oms                the pre-built surface, the names and the named streets (section 4)
    markers.cfg                optional: icons on the map (section 7)
    regions.cfg                optional: municipalities, their names as a watermark, where the bus is (section 8)
    icons/                     optional: the pictures markers.cfg names
    lines.lin                  optional: the map's lines, for the Lines panel of the city map (section 9)
```

- The folder name and the file names are fixed; nothing is searched or configured. A missing file is a
  feature that is off, never an error. No `manifest.cfg`: the package is ignored.
- The folder may lie in the map's own folder or in the same place of another content root: a package for
  a map one does not own goes in the game's content folder (`Maps/<Map>/openomsi-realmap/`), as a mod does,
  so the OMSI 2 installation is never written to. The root with the most priority wins.
- Other files an author may put in the folder are ignored.

## 2. Manifest

`key = value` lines, `#` comments, UTF-8.

```
format   = 1
grid     = 300
name     = My Map
credit   = (c) OpenStreetMap contributors (ODbL)
# optional colours (any key left out takes the game's own colour for that look)
water    = #7fb2e5
light_water = #9cc3d6
```

- `format`: integer. A reader that does not know it ignores the package.
- `grid`: the author's tile size in metres (section 3). Required, greater than 1.
- `name`: for people. `credit`: shown on the city map; give it when the data needs attribution.
- Colours: the game has a **dark** and a **light** look. `water = #rrggbb` colours the dark map and its
  `light_` twin the light one. Keys: `land water river green grass forest sport farm built rail rail_edge
  border`. The layers are semantic, so the game keeps the last word on how they look.

## 3. Coordinates

Positions are **metres of the author's tile grid**, the numbers OMSI 2 itself shows for the map's
objects: `x = tile_column * grid + position_in_tile`, `y = tile_row * grid + position_in_tile`, x east,
y north. openOMSI turns each position into its own game metres with the grid it uses for the map
(`omsi_map::tile_local_to_world`), which is why the manifest states `grid`: a `[worldcoordinates]` map's
grid is not 300 m. No latitude or longitude is stored, so a map may be bent, shortened or fictional. The
file stores positions as `f32` offsets from an `origin` (an `f64` pair) to keep precision on large maps.

## 4. `surface.oms`

```
magic    "OMSF"
u32      version (= 1)
f64 x2   origin_x, origin_y         (author's grid metres)
f32 x4   min_x, max_x, min_y, max_y (relative to origin)
f32      chunk_size                 (metres, e.g. 2000)
u32      n_strings ; n_strings x { u16 len, utf-8 bytes }
u32      n_chunks
n_chunks x { i32 cx, i32 cy, u64 offset, u32 byte_len }       the index
... the chunks ...
```

Chunk `(cx, cy)` covers `[cx*chunk_size, (cx+1)*chunk_size)` on each axis, relative to `origin`. A reader
finds the chunks near the camera by the index and reads only those. A chunk needs no other chunk.

```
u32  n_fills   ; n_fills x  { u8 class, u32 n_tri, f32[6*n_tri] }         triangles, ready to draw
u32  n_lines   ; n_lines x  { u8 class, u8 flags, f32 width_m, u32 name,
                              u32 n_pts, f32[2*n_pts] }                    polylines
u32  n_labels  ; n_labels x { u8 kind, u8 rank, u32 name, f32 x, f32 y }   names (section 6)
```

- Fill `class`: `0 water, 1 green (parks, gardens), 2 built, 3 grass (grass, meadow, scrub), 4 forest (wood,
  forest), 5 sport (pitches, stadiums), 6 farm`.
- Line `class`: `0 river, 1 rail, 2 border`, drawn; `3 street` and `4 street with limits`, not drawn (section 8). A
  river's `width_m` of 5 or more is a river, less a stream. Other classes are skipped, so a later version can add
  more without breaking this one.
- Triangles are already triangulated; there are no holes to resolve. `name` indexes the string table
  (`u32::MAX`: none).
- Everything that is wrong ends in "no package": a short file, a wrong magic or version, a chunk outside
  the file. The reader checks every length before reading and never panics on a bad file.

## 5. What openOMSI does with it

At the map's load, in the background step that reads the road network, openOMSI parses the manifest and
the index of `surface.oms`. The small map (Shift+N) builds the chunks within its radius around the bus;
the city map (Shift+M) builds all of them once. Both draw the surface **under the roads**, on the map's own
ground colour and under the map's own layers, in the dark or light look, and the city map shows the credit.
The package is drawn as it is, and nothing is fetched from the network.

Settings (`~/.openomsi/settings.cfg`, the launcher's settings page and the pause menu): `nav_surface`
(on by default) draws the surface; `nav_light` (off) gives the maps a light look.

## 6. Names

`kind` of a label: `0 district, 1 terminal, 2 town, 3 landmark, 4 water`. `rank` 0 to 255 is how important it is (high
shows from far away): the city map writes the names of rank 200 and more from far away, 140 and more, 100 and more,
then all, as the view comes closer, with a dark halo (a light one on the light map), and never one over another. The
author decides the rank; the game derives nothing.

## 7. Icons: `markers.cfg` and `icons/`

```
# icon | width in pixels at interface size 1 | x | y | name
marker = icons/terminal.png | 30 | 1200.5 | 840.0 | Central Terminal
```

- `icon` is a PNG (straight alpha, up to 1024 x 1024) under the package folder, with no `..` in the path. The
  pictures are packed into one texture; one that cannot be read leaves out its lines.
- `x`, `y` are metres of the author's grid (section 3). `name` is for people reading the file.
- The icons are drawn upright at their place on the small map and the city map, with no text, in the width the
  second column says.

## 8. Regions and streets: where the bus is

`regions.cfg`, one region (a municipality) a line, metres of the author's grid:

```
# region = name | label x | label y | box width | box height | polygon x,y x,y ...
region = Alphaville | 5200.0 | 3100.0 | 4800.0 | 520.0 | 1000.0,1000.0 9000.0,1000.0 9000.0,6000.0 1000.0,6000.0
```

The polygon says which municipality the bus is in. The box is where and how big the name fits, **computed by the
author's tools** (the largest letters that fit inside the visible part of the municipality, one cap for all); a box
width of 0 means no room for a readable name, so none is drawn. The city map writes each name as a big, faint
watermark in its box while zoomed out.

The named streets are lines of class 3 (the name) or 4 (the name and the limits: `width_m` is the limit in km/h,
`flags` the limit of heavy vehicles, 0 for the same or none) in `surface.oms`; the nearest piece within 30 m of the
bus tells the street. With regions or named streets, openOMSI shows a **row under the next stop** of the small map
with the municipality and the street, and, on the city map, the municipality, "You are on: <street>" and the
street's limit as a road sign (the heavy vehicles' limit smaller, when it is another). Without them none of it
appears.

## 9. Lines: `lines.lin`

Tab separated, one record a line, `#` comments. Positions are metres of the author's grid, relative to `ORIGEM`.

```
ORIGEM	0.0	0.0
L	110	BRT	North - Central	bus	19900101	0
R	18.9	35	35	0	Every day	Articulated	North Terminal - Central Terminal
P	1250.4 4428.6 1320.0 4330.8 ...
S	1260.8 4409.6 1310.0 4341.5 ...
```

- `L`: the number, the group (`BRT`, `EXP`, anything else is a feeder), the name, then optionally the kind of vehicle
  (`bus` by default, `tram`, `metro`, `rail`) and the dates (yyyymmdd) the line runs from and to, 0 for open. A map with
  eras has a record for each form a line took, and the panel lists the forms that hold on the session's date.
- `R`: a route of the line (one or two, out and back): km, stops, minutes, circular (0 or 1), days, model, itinerary.
  `P` is its way and `S` its stops.

With the file the city map gets a **Lines** button: a panel with the data of the line in focus (length, stops, trip
time, days, model, route 1 or 2), and the list of the map's lines with a tick box each, in tabs by kind of vehicle and
(for buses) by group. A click on a line ticks it, a second one opens its data. A ticked line is drawn on the map in a
colour of its own, with its stops; the vehicles that run it (read from the line they show) get a ring in its colour,
and the card counts them. The route of the trip the player drives is not changed. Without the file there is no button.

## 10. Trying it

- **The example in this repository**, `docs/examples/openomsi-realmap/`: a few made-up shapes (a lake, a
  river, a railway, a border, a park, a wood), three names, a street with a 30 km/h limit, two icons (a stop and a
  station), one region and three lines (two bus lines and a tram), in the tile grid of the tutorial map **Grundorf**,
  about 3 KB in all, with no data of
  OMSI 2 or OpenStreetMap. Copy the folder to `Maps/Grundorf/openomsi-realmap/` in the content folder, start
  Grundorf and open the navigator (Shift+N) and the city map (Shift+M): near the start of the map the row under the
  next stop says "Exampleton | Example Street".
- **Real packages and the generator** that makes them are not in this repository (their data is
  OpenStreetMap's, under the ODbL, a different licence from this project's). They live in
  [lmonteirotech/openomsi-map-packages](https://github.com/lmonteirotech/openomsi-map-packages): its
  releases hold test packages (Berlin-Spandau and Projekt Szczecin) and its README explains how to make a
  package for any map.
