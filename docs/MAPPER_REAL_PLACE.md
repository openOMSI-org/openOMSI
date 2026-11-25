# A real place

This tutorial builds a map of a real town: you place the map on the Earth, lay satellite pictures
and real terrain heights on it, and bring in roads, stops, lines, trees, water and buildings from
OpenStreetMap. It is for map makers who have done [Your first map](MAPPER_FIRST_MAP.md).

## What you will build

A map of a real neighbourhood of your choice, with:

- satellite imagery on the ground and the real terrain heights,
- its streets built as roads with junctions,
- its bus stops, bus lines with a timetable, trees, water and buildings,
- a start point, so you can drive it at once.

## What you need

- openMapper and OMSI 2, set up as in [openMapper](MAPPER.md).
- An internet connection. Imagery, heights and OpenStreetMap data are downloaded and kept in a cache
  (`~/.openomsi/cache/openmapper/tiles`), so nothing is fetched twice.
- Optional: your own key for Google, Mapbox or MapTiler, for sharper imagery in some places.

## 1. Start a map for it

1. Choose **File → New map…**.
2. Pick a **Size:** that covers the area. Each tile is 300 × 300 m: **5 × 5** is 1.5 km across.
3. Pick **keeps right** or **keeps left** as the place drives.
4. Click **Create**.

You can add tiles later with the Tiles tool (`7`), so start small.

## 2. Place the map on the Earth

1. Press `I` (or **Map → Satellite and terrain…**). The **Satellite and terrain** window shows a
   map of the world. Drag to move, use the wheel to zoom.
2. Type the place in **Find a place: a town, a street, an address… (Enter)** and press `Enter`.
3. Click **Place the map here**, then click on the world map where the middle of your map should be.
   The orange tile is your map's middle tile.
4. To turn the map, change **North turned**.

A map with world coordinates (`[worldcoordinates]` in its `global.cfg`) already lies on the Earth
and skips this step.

## 3. Load satellite imagery

1. Under **Imagery**, pick a source. **The sharpest here** marks the best one for this place, with
   how many cm a pixel it gives.
   - Esri World Imagery works everywhere, no key needed.
   - Official orthophotos (no key) cover Berlin, Brandenburg, NRW, Bavaria, Lower Saxony, the
     Netherlands, Switzerland, Austria, France and New York State.
   - Google, Mapbox and MapTiler need your own key: open **Keys for more sources (optional)**.
2. Tick **Also load terrain heights** to get the heights in the same go.
3. Tick **New tiles get imagery by themselves** if you will add tiles later.
4. Click **Load for all tiles**.

Each tile's picture is cut exactly to the tile, at the source's own detail (up to 4096 pixels), so
tiles meet without a seam. Under **Overlay on the ground**, **Show** turns the pictures on or off and
**opacity** fades them. **View → Satellite overlay** does the same from the menu.

Each source's terms of use and attribution are shown in the window.

> **Note:** the pictures are kept in the map's `openMapper` folder. They are left out of shared
> exports unless you tick **Put the imagery into maps exported for sharing (check its licence
> first)**.

## 4. Load terrain heights

If you did not tick **Also load terrain heights**, click **Terrain heights only**.

Under **Terrain heights** you can set:

| Option | What it does |
| --- | --- |
| **Relative: the orange tile's mean height is 0 m** | Heights counted from the middle tile |
| **Raise everything by** | Lifts the whole map |
| **Smoothing** | Softens the raw heights |

The heights come from the AWS Terrain Tiles. Loading them is one undo step, and tile edges match.
If some tiles next to loaded ones have no heights, openMapper offers **Fit their edges** so the
ground does not step there.

> **Tip:** load the heights before you build roads. Roads follow the ground as it is when they are
> built.

## 5. Choose what OpenStreetMap builds with

OpenStreetMap only gives lines and points. openMapper builds them with the road types and objects
you choose, so pick them first:

1. Press `2` (**Roads**) and pick a road type for main roads in the catalogue.
2. Press `4` (**Game logic**), click **Bus stops** and pick a stop object.
3. Press `3` (**Objects**) and pick a tree, or tick **Plant brush: sow trees by painting** and add
   several trees to the plant mix (right-click an entry, **Add to the plant mix**).
4. For buildings, pick a building in the Objects catalogue from the folder you want to use (for
   example `Buildings_MC`).

## 6. Import from OpenStreetMap

1. Choose **Map → From OpenStreetMap…**.
2. Click **Download for the map's tiles**. The window then shows how much there is: ways, stops and
   bus routes.
3. Tick what to build and give each kind its type:

   | Kind | What it brings |
   | --- | --- |
   | **Main roads** | motorway, trunk, primary, secondary |
   | **Streets** | tertiary, residential, unclassified |
   | **Service roads** | yards, car parks, depots (off by default) |
   | **Tram tracks** | each track on its own (off by default) |
   | **Railway tracks** | main lines without sidings (off by default) |
   | **One-way roads** | each carriageway of a dual road, and one-way streets |
   | **Bus stops** | the stops with their names, at the kerb of the nearest lane |
   | **Bus lines** | each bus route through two or more of the map's stops |
   | **Trees and woods** | single trees, rows and woods, with the plant mix |
   | **Buildings** | each outline filled with a model of the folder you chose |
   | **Water** | lakes and rivers |

   For a road kind, choose its type in the Roads catalogue and click **Use the chosen** next to it.
   Do the same for **Bus stops**. For **Buildings**, click **Add the folder of the object chosen in
   the catalogue**.
4. If something is still missing, the window lists it under **To build, choose (or untick):**.
5. Click **Build**. The button shows how many ways and stops it will build.

What you get:

- **Roads** are built as if you drew them: one street's ways joined into one road, corners rounded
  into arcs, cut exactly at the map's edge. Junctions are made where roads meet or cross (with
  **Auto-Junction** on). Tunnels and roads lying on top of roads you already have are left out.
- **One-way roads** use a one-way type if you chose one, else their kind's type.
- **Tracks** cross roads on the level. With **Auto-Switch** on, a track that branches off another
  gets a switch from your install. Other branches are joined with switches by hand.
- **Bus lines:** each route becomes a trip of its line, the way found along the roads. Each new
  line gets a tour from 06:00 to 22:00, and the map gets a depot if it has none.
- **Trees** are planted with the plant mix, never on a road. **apart in woods** sets their spacing.
- **Water:** the ground inside lakes and rivers is lowered below the banks, and the tile gets water
  a little under the banks. **deep** sets how deep.
- **Buildings:** each outline gets the model whose ground plan and height come nearest, its front
  to the nearest road. An outline no model fits is left out.
- **A start point** is added on a lane about 40 m before a bus stop if the map has none.

Everything one build brings is one undo step. Data © OpenStreetMap contributors (ODbL).

> **Tip:** build on tiles with no roads yet. Roads lying on roads the map already has are skipped.

## 7. Trace a GPS track (optional)

If you recorded a route on your phone, or drew one in another program:

1. Choose **Map → GPS tracks to trace…**, or drop the file on the window. GPX, KML and GeoJSON work.
2. Click **Load a file…**. The track is drawn in magenta in the view.
3. Draw roads along it by hand, or pick a road type in the Roads tool and use **Build a road along
   it:**. The road is built as if drawn, with junctions where it meets your roads, and is one undo
   step.

## 8. Clean up the result

OpenStreetMap data is made for navigation, not for a driving game. Expect some work:

1. **Run the check.** **Map → Check map…** names road ends that lie on each other but are not
   joined, stops away from lanes, trips without a way and more. Use **Fix** where offered.
2. **Compare with the picture.** Turn the satellite overlay on, lower its **opacity**, and look for
   roads that are offset from the real street. Select a road (`1`, double-click for all of it) and
   drag its end and middle dots onto the picture.
3. **Delete what you do not need.** Select stray service roads or duplicate pieces and press `Del`.
4. **Remake junctions.** After moving roads, select a junction and click **Build it again**, or use
   **Map → Make the junctions again** for the whole map.
5. **Lay roads on the ground.** If a road floats or digs in, select it and click **Lay on the
   ground**. Check its **Height profile** for steep stretches.
6. **Swap types in one go.** `Ctrl+F` (**Find in map…**) finds every object or road of a type. Pick
   a new type in the catalogue and replace them all in one undo step.
7. **Swap single buildings.** Select a building, pick another in the catalogue and use **Replace
   with** in its properties.
8. **Check in the game.** **Map → Check in openOMSI…** shows lanes without road under them and
   steps a bus wheel cannot climb.
9. **Save and drive.** `Ctrl+S`, then `F5`.

## More ways to get a picture on the ground

- **Load the OpenStreetMap map** (in the Satellite and terrain window, or per tile) lays the map as
  openstreetmap.org draws it, with street names, instead of a photo. It is easy to trace.
- **Own pictures:** a scan, a screenshot, an old plan. With the Tiles tool (`7`) click a tile and
  use **Load image file…**, or drop the picture on the window. Under **Own pictures** in the Satellite
  and terrain window, one picture can cover a rectangle of tiles and be turned, mirrored, moved and
  sized by eye. No place on the Earth is needed.
- **OMSI's aerial photos:** a folder of OMSI 2's tile photos can be imported under **OMSI's aerial
  photos**.
- **Tiles from another map:** **Map → Tiles from another map…** puts another map's tiles (ground,
  paint, water, objects, roads) beside or into this one.

## What you learned

- **Satellite and terrain** (`I`) places the map on the Earth and loads imagery and heights.
- **From OpenStreetMap** builds roads, stops, lines, trees, water and buildings with the types you
  choose, in one undo step.
- **GPS tracks to trace** shows a recorded route to draw along.
- The map check, the satellite overlay and **Find in map** help clean up the import.

## Next

- [Lines and traffic](MAPPER_LINES.md): tune the imported lines, their timetables and the depot.
- [Terrain](MAPPER_TERRAIN.md): reshape the loaded ground and paint it.
- [openMapper tools](MAPPER_TOOLS.md) for every option of these windows.
