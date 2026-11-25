# Your first map

This tutorial takes you from an empty map to driving it in service in openOMSI. It is for anyone
new to openMapper and takes about half an hour. Every step can be undone with `Ctrl+Z`, and
`Ctrl+K` finds any command by name if you get lost.

## What you will build

A small town on 3 × 3 tiles (900 × 900 m):

- two roads that meet at a junction with traffic lights,
- a few buildings and trees,
- two bus stops and a start point,
- line 1 with a trip there and back and a tour that runs all day,
- checked, saved and driven in openOMSI.

## What you need

- openMapper and OMSI 2, set up as in [openMapper](MAPPER.md#first-start).
- openOMSI installed, for the test drive. See [Getting started](GETTING_STARTED.md).

## 1. Create the map

1. On the start screen click **New map…** (or **File → New map…** in the editor).
2. In **Name:** type a name, for example `My town`.
3. Leave **Size:** at **3 × 3 tiles**. Bigger maps can grow later with the Tiles tool.
4. Under **Traffic:** choose **keeps right** or **keeps left**.
5. Leave the rest as it is. You can change all of it later in **Map → Map settings…**:

   | Field | What it sets |
   | --- | --- |
   | **Description:** | The text in the game's map list |
   | **Ground height:** | Metres above sea level |
   | **Busy:** | How many **cars** and **passengers** (1 is normal) |
   | **Years:** | The years the map runs in, and the **year offset** |
   | **Money:** / **Tickets:** | Currency and ticket pack (**as the template** by default) |
   | **Seasons:** | **four, snowy winter**, **mild winter** or **summer only** |
   | **Rail:** | Trams and trains follow their tracks' switches |
   | **Ground texture (all tiles):** | The base ground |

6. Click **Create**.

openMapper opens the new map with the **Roads** tool active and says "Your map is ready. Start with a
road!". The **Getting started** window opens too. It is a checklist that ticks itself as the map
grows, and each open step has a **Show me** button. You can reopen it from **Help → Getting
started**.

## 2. Look around

Hold the right mouse button and move the mouse to look round. While you hold it, `W` `A` `S` `D`
fly and `Q` / `E` go down and up. The wheel zooms towards the cursor, the middle button pans.
`Home` looks straight down, and `0` switches to a flat 2D plan.

The grey lines on the ground are the tile borders (`G` turns them on and off).

## 3. Draw a road

1. Press `2` for the **Roads** tool.
2. In the panel on the left, set **Show** to **Roads** and pick a road type. Its name appears next
   to **Building with:**.
3. Click where the road starts.
4. Move the mouse: a green preview follows. Click where the road ends. The road is built.
5. To go on from its end, click the white dot at the end, then click the next point. The new road
   continues smoothly from the old one.

While you draw:

- Right-click or `Esc` drops the road you started.
- `Shift` draws a straight.
- Type a **Length**, **Radius** or **Heading** into the fields in the bottom-left corner of the view
  and press `Enter` to build exactly that. Empty fields follow the mouse.
- Hold `Ctrl` to join nothing at the ends.

> **Tip:** tick **Go on drawing from the end of the road just built** in the panel to draw a long
> road click after click.

## 4. Make a junction

1. Still in **Roads**, start a second road on one side of the first.
2. End it on the far side, across the first road (or end it on the middle of the first road for a
   T-junction).

The roads are cut and joined with a junction plate, lanes and footpaths round the corners. This
happens while **Make junctions where roads meet** is ticked in the panel (and **Auto-Junction** in
the status bar).

To give it traffic lights:

1. Press `1` (**Select**) and click the junction plate.
2. In the properties, under **Junction**, tick **Traffic lights**. You need three roads or more.
3. Set **Main road green** and **Side roads green** if you like, and click **Apply**. Signal heads
   on poles are put up and the AI stops at red.
4. Tick **Zebra crossings** for the roads that should have one.

If you move the roads later, **Build it again** remakes the plate for the roads as they are.

## 5. Change a road

1. Press `1` (**Select**) and click a road.
   - Double-click selects the whole road, every piece up to where it branches or ends.
2. Drag a white end dot to move that end. Drag the middle dot to bend it.
3. Look at the **Height profile** in the properties. Green is gentle, yellow and orange steeper, red
   very steep. Over 8 % is steep for a bus.
4. If the road floats or digs in after you reshape the ground, click **Lay on the ground**.

`Del` deletes what is selected. `K` (the knife) cuts a road where you click, to give a stretch its own
type.

## 6. Place objects

1. Press `3` for the **Objects** tool.
2. Set **Show** to **Objects** (or type in **Search**) and click a building.
3. Move the mouse beside a road. With **Line up with the nearest road** ticked, the building turns
   to face the road and keeps its front a set distance from it.
4. Click to place it. Click again to place another.
5. `R` turns it (`Shift+R` the other way). Hold `Shift` to place it freely.
6. Right-click to put the object down and stop placing.

For trees:

1. Tick **Plant brush: sow trees by painting** and pick a tree.
2. Paint with the left button. **Size** and **Apart** set the brush. **Clear** takes trees away.
3. Choose **In an outline** and click round an area to fill a wood or a park at once. Press `Enter`
   to plant. Roads are left clear.

For street lamps along a road:

1. Pick a lamp in the Objects catalogue.
2. Press `1`, select the road, and open **Objects along the road** in its properties.
3. Set **Every** (the spacing), **Beside** (the distance from the middle) and tick **Both sides** if
   you want them.
4. Click **Place along the road**.

## 7. Put down bus stops

1. Press `4` for **Game logic** and click **Bus stops**.
2. Pick a stop object in the list.
3. Leave **On both sides of the road** ticked. A twin stop with the same name goes across the road,
   facing the other way.
4. Click beside a road, on the side where the doors open. The stop moves to the kerb of the nearest
   lane.
5. In the properties, type its name under **Stop name**. The twin takes the same name.
6. Put a second stop (and its twin) further along the roads, and name it.

A bus serves a stop only within 15 m of its lane. The **Kind of stop** in the properties can be a
**Compulsory stop**, a **Request stop** or a **Terminus**.

## 8. Put down a start point

The start point is where your bus appears when a drive starts.

1. Still in **Game logic**, click **Start points** and pick a start point object.
2. Click on a lane some way before your first stop, so the bus faces the way to drive.

## 9. Make a line

1. Press `L` (or **Map → Lines and timetable…**).
2. Click **New line**. It is called `1`.
3. In the **Trips** tab, click **New trip**. Picking starts: the stops on the map are marked orange.
4. Click the stops in the view in the order the bus calls at them.
5. Click **Done picking**. The way along the roads is found and drawn in blue. A problem (a stop
   away from a lane, roads that do not join) is named at the stop where it happens.
6. Click **Make the way back**. A second trip with the same stops in reverse is made.

## 10. Give it a timetable

1. Open the **Tours and times** tab.
2. If it says the map has no depot, click **Add a depot with OMSI 2's MAN buses**. AI buses then
   drive the tours you do not drive yourself.
3. Click **New tour**.
4. Set **Fill from** `06:00` **to** `20:00` and a **break** of `5 min` (the defaults), then click
   **Fill**. The tour gets departures: the line's trips one after the other, a break after each.

`T` opens **Traffic and AI** if you want to change how busy the roads are over the day.
[Lines and traffic](MAPPER_LINES.md) goes into all of this.

## 11. Check the map

1. Choose **Map → Check map…**. The **Map check** window lists what would go wrong in the game:
   errors, warnings and notes.
2. Click an issue to fly to it.
3. Click **Fix** on an issue (or **Fix all**) where offered: a stop moved to the kerb, a start point
   or depot added, road ends joined.
4. Click **Check again** until the list is clean, or only notes remain.

**Map → Check in openOMSI…** goes one step further: it runs the game itself on your map, unseen, with
the game's own checks. Both checks are explained in
[openMapper troubleshooting](MAPPER_TROUBLESHOOTING.md).

## 12. Save

Press `Ctrl+S`. The map goes into openOMSI's content folder, under `maps\My town`. Every save keeps
the files it replaced (**File → Earlier versions…**), and your changes are autosaved every 3
minutes.

## 13. Drive it

1. Press `F5` (or click **Test drive** in the toolbar).
2. In the **Test drive** window, set:
   - **Start from:** your start point, a bus stop, or **Where the camera looks**.
   - **Vehicle:** a bus from your install, or **No vehicle (on foot)**.
   - **Time of day**, **Weather**, **Traffic**, and **Passengers at the stops**.
3. Click **Start**. The map is saved and openOMSI starts on it.
4. In the game, set line 1 on the bus and drive. See [Driving](DRIVING.md) for the bus and its
   timetable.

While the game runs, the toolbar button reads **Close test drive** and ends the session.
`Shift+F5` starts from the start point nearest the view without asking.

## What you learned

- **New map…** sets up a map; the **Getting started** list shows what is still missing.
- **Roads** (`2`) draws roads in two clicks and makes junctions by itself.
- **Select** (`1`) changes what you built; the properties hold the details.
- **Objects** (`3`) places buildings and trees; rows go along a road from its properties.
- **Game logic** (`4`) holds stops and start points.
- **Lines and timetable** (`L`) makes a line, its trips and tours.
- **Check map…**, `Ctrl+S` and `F5` take it to the game.

## Next

- [A real place](MAPPER_REAL_PLACE.md): build a real town from satellite pictures and OpenStreetMap.
- [Lines and traffic](MAPPER_LINES.md): more lines, timetables, depot files and AI traffic.
- [openMapper tools](MAPPER_TOOLS.md) and [openMapper keys](MAPPER_KEYS.md) for everything else.
- To share the map: **File → Export map for sharing…** packs it with every file it uses from your
  content folder into a zip. **File → Export a plan of the map…** draws an SVG plan with the roads,
  stops and lines.
