# Terrain

This tutorial shapes and paints the ground of a map: sculpting hills and valleys, ramps and level
beds, height maps from other programs, ground textures, lakes and rivers, and adding tiles. It is
for map makers who have done [Your first map](MAPPER_FIRST_MAP.md).

## What you will build

On a 3 × 3 map:

- a hill with a road climbing it on an even ramp, and a level square for a depot,
- a lake with sloping banks and a small river,
- grass, gravel verges along the road, rock on the steep slopes,
- a ring of new tiles round the map.

## What you need

- A map with at least one road. A new 3 × 3 map with a road on it is enough.
- Optional: a program that edits greyscale pictures (World Machine, Gaea, an image editor), for
  height maps.

> **Tip:** shape the ground before you build much on it. Roads with **Shape the ground under the
> road** follow it, and **Lay on the ground** puts a road back onto it afterwards.

## 1. Sculpt with the brush

1. Press `5` for the **Terrain** tool.
2. Set **Size** and **Strength**. `[` and `]` make the brush smaller and larger.
3. Under **Mode**, pick one and drag on the ground:

   | Mode | What it does |
   | --- | --- |
   | **Raise** | Raises the ground under the brush |
   | **Lower** | Lowers the ground under the brush |
   | **Smooth** | Softens bumps and edges |
   | **Flatten** | Levels the ground to the height where you start dragging |
   | **Roughen** | Gives flat ground the rolls and dips of a natural field; hold longer for more |

4. Raise a hill beside your road, then **Smooth** it so it looks natural.

Leave **Keep the ground under roads** ticked: the brush then does not push the ground up through a
road. Untick it to shape the ground under and beside roads freely.

### A level square

1. Choose **Flatten** and tick **To a height**.
2. Type the height in metres.
3. Drag over the area. It becomes level at exactly that height: a depot yard, a platform.

## 2. Make a ramp

1. Choose **Ramp**.
2. Set **Size**: the ramp is as wide as the brush.
3. Click where the ramp starts, then where it ends. The ground between becomes one even slope, and
   its gradient is shown.

Use it for an embankment, a road up a hill, or a level bed for tracks. Right-click drops a ramp you
started.

### Ground under a road

To bring the ground to a road's height (a road on a raised bed):

1. Choose **Along a spline**.
2. Click the road. The ground under it, as wide as the brush either side, is brought to its height,
   and the pieces joined to it come with it.

Or select several roads with **Select** and click **Align the selected splines**.

### Ground under a building

Select a building on a slope and click **Level ground** in its properties. The ground under its
outline is made level with its foot, blending in 3 m round it.

## 3. Use a height map

A height map is the ground as a grey picture: dark is low, light is high. You can shape it in another
program and bring it back.

### Export

1. In the **Terrain** panel, scroll to **Height map** at the end.
2. Click **Export…** and save the PNG.

The picture is a 16-bit grey PNG, one pixel per terrain sample. A `.txt` file beside it holds the
lowest and highest heights.

### Import

1. Click **Import…** and pick a picture (8- or 16-bit PNG, TIFF, BMP or JPEG).
2. Set how high black and white are (**Black is** / **White is**), or how much they add (**Black
   adds** / **White adds**).
3. Choose **Replace the ground** or **Add to it**.
4. Set **Smoothing**. An 8-bit picture has only 256 steps of height; smoothing hides them.
5. Click **Lay it on the map**.

> **Note:** roads and objects keep their heights. Import height maps before you build, or select the
> roads afterwards and click **Lay on the ground**.

### Flatten everything

**Flatten the whole terrain** (end of the Terrain panel, or **View → Flatten the terrain (roads
follow)**) levels every tile to the map's mean height. Roads, AI paths and junctions follow. One undo
brings the relief back.

## 4. Paint ground textures

1. Press `6` for the **Paint** tool.
2. Under **Ground layers**, the first layer is the **base ground**, under everything.
3. Click **Add…** and choose a texture (gravel, for example). It becomes a new layer.
4. Select the layer, choose **Paint**, set **Size** and **Strength**, and paint on the ground.
5. **Erase** takes the layer away where you paint.

Each layer has settings:

| Setting | What it does |
| --- | --- |
| **Texture** | The layer's picture; its painting stays when you change it |
| **Detail texture** | A fine grain seen close up |
| **Repeats per tile** / **Detail repeats** | How often each picture repeats |
| **Paint detail** | Texels across a tile (128 to 1024); finer is sharper |

The arrows move a layer up or down the list. A layer lower in the list is drawn over the ones before
it. **Remove** deletes a layer.

### Paint along a road

1. Press `1` and select a road (double-click for all of it).
2. Open **Paint the ground along it** in its properties.
3. Pick the **Layer**, choose **beside it** or **under it too**, and set **Out from its edge** and
   **Soft edge**.
4. Click **Paint**. You get a gravel verge, or dirt beside a track.

### Paint by slope and height

1. In the **Paint** panel, scroll to **By slope and height**.
2. Select a layer above the base ground, for example rock.
3. Set **Slope** (how steep), **Height** (how high), **Fade** and **Cover**.
4. Click **Paint the map**. The layer is painted over the whole map wherever the ground is that steep
   and that high.

Use it for rock on slopes, sand by the water or snow on hilltops.

## 5. Make a lake and a river

### A lake

1. Press `5` and choose **Lake / river**.
2. Choose **Lake** and set **Depth** and **Banks**.
3. Click round the shore and close it on the first point (or press `Enter`). Right-click takes a
   point back.

The ground inside is dug to a bed, the banks slope down to the shore, and the tile gets water.

### A river

1. Choose **River** and set **Width**, **Depth** and **Banks**.
2. Click along the river's middle line from its source, and finish on the last point (or press
   `Enter`).

With **Brush**, you paint the water bed by hand instead. The water's level is set where a stroke
starts, so start at the bank.

> **Note:** water in OMSI lies at one level for a whole tile. Any other low place on the same tile
> is flooded too, and a river runs level.

### Water on a tile by hand

1. Press `7` for the **Tiles** tool and click a tile.
2. Tick **Water** and set the level next to **at**.
3. Lower the ground below that level with the Terrain tool where the water should show.

## 6. Add and remove tiles

1. Press `7` for the **Tiles** tool.
2. Click beside the map to add a 300 × 300 m tile.
3. Click **A ring of tiles round the map** to add a tile at every free place round it, corners too,
   each at the height of its neighbours.
4. Right-click a tile to remove it.

The panel shows how many tiles the map has and its area. A clicked tile also shows:

- **Water** and its level,
- **Lamps light the ground at night**: the game works out the street lamps' light on this tile (on
  for every new tile),
- its satellite overlay and own picture.

To take tiles from another map, use **Map → Tiles from another map…**.

## 7. Check and drive

1. Select your road and look at its **Height profile**. Over 8 % is steep for a bus.
2. Press `Ctrl+S`, then `F5`, and drive up your hill.

## What you learned

- **Terrain** (`5`) raises, lowers, smooths, flattens and roughens; **Ramp** makes even slopes.
- Height maps move the ground in and out as a grey picture.
- **Paint** (`6`) holds the ground layers; roads and slopes can be painted in one go.
- **Lake / river** digs water beds; water is one level per tile.
- **Tiles** (`7`) grows the map and sets water and lamplight per tile.

## Next

- [Your own models](MAPPER_MODELS.md): import and edit models for your map.
- [openMapper tools](MAPPER_TOOLS.md): every option of the Terrain, Paint and Tiles tools.
