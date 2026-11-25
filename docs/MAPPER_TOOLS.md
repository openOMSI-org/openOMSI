# openMapper tools reference

Every tool, panel and window of openMapper, grouped by what you work on. Use it to look up what a tool does, how to use it and what its options mean. For guided walks through whole tasks, see the tutorials: [Tutorial 1: your first map](MAPPER_FIRST_MAP.md), [Tutorial 2: a real place](MAPPER_REAL_PLACE.md), [Tutorial 3: lines and traffic](MAPPER_LINES.md), [Tutorial 4: terrain](MAPPER_TERRAIN.md) and [Tutorial 5: your own models](MAPPER_MODELS.md).

Keys are given for openMapper's own key set. All keys are in [Keys and mouse](MAPPER_KEYS.md). The window is explained in [openMapper](MAPPER.md).

> **Tip:** **Edit → Search for a command…** (`Ctrl+K`) finds every command, tool and window by its name or a word about it, in English or the interface language ("timetable", "lake"). For a feature that lives in a panel (a road's height profile, a junction's lights, planting a wood) it opens the right tool and says where to look.

## Selecting and editing

### Select tool (1)

The Select tool picks objects and roads; the **Properties** panel on the right then shows and changes them.

| Action | How |
| --- | --- |
| Select one thing | Click it |
| Select a whole road | Double-click it (every piece up to where it branches or ends) |
| Select every one of a type | **All like it** in the properties, or **Select all like it** in the right-click menu |
| Add or take away | `Shift` + click |
| Box select | `Shift` / `Ctrl` + drag on empty ground (Unity camera: any drag on nothing) |
| Move the selection | Drag it |
| Turn it | `R` (by the angle step) |
| Delete it | `Del` |
| Copy, cut, paste at the cursor, duplicate | `Ctrl+C`, `Ctrl+X`, `Ctrl+V`, `Ctrl+D` |

Copies keep the links inside them. Rows of objects along roads and things hanging on objects come along.

The tool panel has two tabs. **Tools** holds the transform handles and the snapping buttons. **Objects** lists everything on the map (see [Objects on the map](#objects-on-the-map)).

### Move, Rotate, Scale handles (V, C, X)

Handles on the selection, as in Blender. They keep the same size on the screen however far the camera is.

| Handles | How to use |
| --- | --- |
| **Move** (`V`) | Drag an arrow along X, Y or Z (up), a square to move in its plane, the circle over the ground |
| **Rotate** (`C`) | Drag a ring to turn about X, Y or Z. Turns snap to the angle step. A road turns only about Z |
| **Scale** (`X`) | Drag an arrow ending in a cube to scale along one axis, a square for two, the circle for all three. One object scales along its own axes |

An OMSI 2 map keeps no size for an object. A scaled object therefore becomes a scaled copy of its type beside the original: `name~s1.5_1.5_2.sco` in the content folder, with its meshes, collision mesh, lamps and paths scaled. Scaled back to 1 it is the original again. Roads are not scaled: change their length and width in their properties.

### Object properties

Select one object to see these in the Properties panel.

| Property | What it does |
| --- | --- |
| **Location** (**East**, **North**, **Height above sea level**) | Its place. The height above the ground under it is shown beneath. Each on its own row to drag or type |
| **Rotation** (**Tilt forward (pitch)**, **Bank to the side (roll)**, **Heading**) | Its turn about X, Y and Z |
| Road button by the rotation | **Turn it the way the nearest road runs beside it** |
| Hill button by the rotation | **Lean with the ground**: tilted to the slope under it (a fence or a shed on a hillside) |
| **Stand it upright again** | Undoes the lean |
| **Scale** | Size along each axis. **Keep proportions** on or off; **Back to its own size** |
| **Texts** | The texts the object shows (street signs, stop plates, line numbers). `@` starts a new line |
| **Replace with** | The object chosen in the Objects tool's catalogue takes this one's place |
| **Level ground** | The ground under its outline made level with its foot, blending in 3 m round it (a house on a slope) |
| **All like it** | Select every one of this type on the map |
| **Go to**, **Delete** | As they say |

Some objects keep their height above the ground under them. Moved elsewhere, they stay as high above the ground there.

Texts are drawn in the view with OMSI's `.oft` fonts, as the game draws them (`[texttexture]`).

### Several things selected

With two or more things selected, the Properties panel offers:

| Option | What it does |
| --- | --- |
| **Move**, **Height**, **Turn** + **Apply** | Move, raise and turn all by numbers |
| **Make every object** | Every selected object becomes the object chosen in the catalogue |
| **Make every road** | Every selected road becomes the road type chosen in the Roads tool's catalogue |
| **Even slope along them** | One chain of roads: from the height at one end to the other, evenly (a bridge, a ramp) |
| **Random headings** | Each object turned its own random way (trees, bushes, rocks) |
| **Along the road** | Each turned the way the nearest road runs beside it |
| **All like this one** | Every object takes the heading of the one in the properties |
| **Lean with the ground** / **Stand upright** | Tilt each to the ground under it, or upright again |
| **Line up** | Three or more on the straight line between the two farthest apart, at equal steps (posts, lamps, a row of trees) |
| **Space evenly** | Equal steps between the two farthest apart, each keeping its distance to the side |
| **Level the ground under them** | Each outline made level with its foot, blending 3 m round it |
| **Lay them on the ground**, **Reverse their direction**, **Complete to** | See [Roads and road shape](#roads-and-road-shape) |

### Copies in a row

In an object's or a selection's properties. A fence, a row of houses, an avenue: the selection copied again and again, one undo step.

| Option | Meaning |
| --- | --- |
| **Copies** | How many copies |
| **Step** | Distance from one copy to the next |
| **Turn each** | Each copy turned a few degrees more |
| **Rows** | More than one makes a grid (an orchard, a car park) |
| **Row step** | Distance between rows |

Press **Make**.

### Groups on the map

Things kept together under a name, for example a stop with its shelter and signs.

1. Select two things or more.
2. Press `Ctrl+G` (or **Group** in the right-click menu or the selection's properties).

A click on one member selects the whole group. A drag moves all of it. `Ctrl` + click picks one member alone. `Ctrl+Shift+G` ungroups. Groups are listed in the Select tool's **Objects** tab. They are kept beside the map in `openMapper/groups.json`, which the game ignores.

### Groups for other maps

**Edit → Groups for other maps…** keeps a selection under a name to put down in any map: a stop with its shelter and signs, a crossing with its lights. Rows along roads and things hanging on objects are included.

1. Select what to keep, type a **Name**, press **Keep the selection** (or **Keep for other maps…** in the right-click menu).
2. In another map, press **Use**: the group goes into the clipboard.
3. `Ctrl+V` puts it down at the cursor, as often as wanted.

Groups are kept as tile files in `<content>/openMapper/groups`. **Delete the group** moves it to openMapper's bin.

### Hanging objects

An object's **Hanging on it** section lists the points it has to hang things on (`[new_attachment]`) and what hangs there. Pick an object in the catalogue, then press **Hang** at a point, as OMSI 2's editor does it: signs on a post, a timetable on a stop.

### Linked objects

OMSI 2's `[varparent]`: an object takes its state from another. A signal head shows a light of its crossing's light program; a stop's display shows its stop.

1. Select the object, then **Link to an object…** under **Linked objects**.
2. Click the object it takes its state from (a crossing for a signal head). `Esc` cancels.
3. For a signal head, choose its **Light** by name.

The link is drawn as a line in the view. A crossing lists the heads that take their lights from it. A signal head put within 40 m of a crossing with a light program is linked to it at once. **Unlink** removes the link. The map check names objects linked to one that is gone.

### Path rules

In a road's or an object's properties, as in OMSI 2's editor. Each rule is for one path (lane), counted from 0; the paths' numbers show on the road or object in the view while the rules are open.

| Rule | Meaning |
| --- | --- |
| **Speed limit, km/h** | Speed limit on the path |
| **Traffic density ×** | More or less AI traffic |
| **Priority** | Right of way |
| **Trucks (0: none)** | Trucks allowed |
| **No cars** | Cars kept out |
| **Buses only** | Buses only |

Press **Add a rule**, set the **Path (lane) number**, pick the rule.

### Snapping and status bar switches

The status bar along the bottom holds switches for how things are placed. When the window is too narrow, the rest are under **More switches**.

| Switch | What it does |
| --- | --- |
| **Angle** + step | Snap headings to the angle step (1°, 5°, 10°, 15°, 22.5°, 30°, 45°, 90°; default 15°). Also what `R` turns by |
| **Grid** + step | Snap positions to the grid (0.5, 1, 2, 5 or 10 m; default 1 m) for placing, moving, road ends and reshaping. The grid is drawn round the cursor while on |
| **Knife** | See [Cutting and joining](#cutting-and-joining) |
| **Ruler** | See [Ruler](#ruler) |
| **Auto-Terrain** | What you build follows the ground. Off: roads, paths and tracks run level from where they start. Only affects what you build next |
| **Auto-Switch** | Branching a track off another makes a switch there |
| **Auto-Junction** | Make junctions where roads meet |
| **Auto-Adapt** | Where roads of different width meet, a plate tapers one into the other |
| **Surface** | A road drawn or a spline end dragged onto a road lands on its edge, at the road's height. Over an object it lands on the object's face (a kerb, a bridge deck, a platform); a ring marks the place |
| **Auto-Align** | Line objects up with the nearest road (`Shift`: put it freely). An imported model is never lined up |

### Snap vertex and Snap edge

Toolbar buttons, also in an object's properties.

**Snap vertex to vertex** (`Shift+V`):

1. Select an object.
2. Click a vertex of it (the source), then a target vertex of another object. `Shift`: horizontal only.
3. Optionally click a second point on the object, then a target point or spline end: the object turns to align. `Esc` finishes.

**Snap edge to edge** (`Shift+L`): click an edge of the selected object, then a target edge or road. The object turns and snaps in line with it. `Shift`: horizontal only. `Esc` or right-click cancels.

### Ruler

**Ruler** in the status bar, or `U`.

- Click two points: the distance, the height between them and the gradient, written on the line in the view.
- `Shift` + clicks: the corners of an outline, closed back to the first. Its area (m², hectares) and perimeter seen from above are written over it and in the status bar.
- Right-click: ruler off.

### Find in map

**Edit → Find in map…** (`Ctrl+F`). Type a name, file, IDCode, stop, `x, y` or `tile x y`.

| Found | What you can do |
| --- | --- |
| Stops and start points by name | Go there |
| A point by its coordinates (`x, y`) | Go there |
| A tile by its grid place (`tile 3 -2`), with its number in the tile list | The camera over its middle |
| Every kind of object and spline, with how many | **Select all**, **Next** (select the next one and go there) |
| A kind | **Replace with** the object or road type chosen in the catalogue, one undo step |

### Objects on the map

The Select tool's **Objects** tab: everything on the map as a tree of the folders its files lie in (`Sceneryobjects › Buildings_MC › …`), with **Groups** first. Each kind shows how many there are. Click one to select it and fly to it. Search by name, file or IDCode. Right-click selects every one of a kind or everything in a folder.

### Edit history

**Edit → Edit history…** lists every step. Click a step to go back to just after it. Undo and redo (`Ctrl+Z`, `Ctrl+Shift+Z`) work for every action, including the timetable and traffic.

## Roads and road shape

### Roads tool (2)

1. Pick a road type in the catalogue.
2. Click the start, then the end. The arc builds itself.

A road started at a road end continues smoothly from it. Ended on another end, it joins with an S-curve or bend (biarc). The road is cut into OMSI splines and linked with its neighbours. It can shape the ground under itself.

| Option | What it does |
| --- | --- |
| **Shape the ground under the road** | The ground follows the road |
| **Go on drawing from the end of the road just built** | Each road starts where the last ended |
| **Join road ends, roads and junction pieces near the cursor** | Hold `Ctrl` while drawing to join nothing. Cables, fences and footpaths never join roads or junctions |
| **Make junctions where roads meet** | Automatic junctions (see [Automatic junctions](#automatic-junctions)) |
| **Use my custom junctions where they fit** | Custom junctions with **Use in automatic junctions** on |
| **Go on with the type of the road I start from** | A road started on another takes its type |
| **Keep the height (bridge, embankment)** | An even gradient end to end, the ground left as it is |
| **Snap to road edges and surfaces** | What is drawn beside a road starts on its edge; over an object, on its face |
| **Roundabout: click the centre, then the ring** | See [Roundabouts](#roundabouts) |
| **Bus Pocket Tool** | See [Bus pockets](#bus-pockets) |

### Drawing by numbers

While a road is drawn, fields at the view's corner take **Length**, **Radius** (straight, left, right), **Heading** and **Rise**. Empty fields follow the cursor; **Clear** empties them. `Shift` draws a straight. `Enter` builds it.

**Keep the height** draws bridges and embankments: an even gradient, the rise to the end typed while drawing, the ground left alone. For pillars, select the road and put them along it as objects ([Objects along a road](#objects-along-a-road)).

### Road properties

Select a road to see its shape and settings.

| Property | What it does |
| --- | --- |
| End dots in the view | Drag to move an end. A road joined there follows, tangent at the joint, its far end kept. An end at a junction stays |
| Middle dot | Drag to bend the road |
| **Ends (joined roads follow)**: **height, m**, **gradient, %**, **cant, %** | At start and end. Cant is banking: positive lowers the right side (into a right bend) |
| **Rotate, °** | The direction the road runs at this end. Dragging turns the end; the road joined there turns with it |
| **Even slope end to end** | Both gradients to the straight climb between the ends' heights |
| **Length**, **Radius** (straight, left, right) | By numbers. The road joined at its end is carried along, tangent |
| **Skew** | The start or end cut on a slant: 1 is 45° (two fences meeting at a corner: 1 and -1) |
| **Ground** | **as it is**, **cut away under it**, **cut, held at the end / start / both ends**: a hole in the ground for a tunnel, underpass or pit (OMSI's terrain align) |
| **Linked** (start / end) | Select the piece joined there (`,` / `.`) |
| **Select the whole road** | Every piece linked on from this one (or double-click) |
| **Lay on the ground** | See below |
| **Reverse direction** | See below |
| **Cut**, **Join with the next piece** | See [Cutting and joining](#cutting-and-joining) |
| **Edit its AI lanes** | The lanes become AI paths of their own that **Game logic → AI paths → Edit existing** bends, moves and deletes. The road keeps its look |
| **Junction on an end** | See [Junction on a road end](#junction-on-a-road-end) |

**Lay on the ground** puts a road or a selection of them back onto the ground, after shaping it or laying a height map over it. Heights are smoothed so the road climbs a hill rather than every bump. Where a road not selected joins, its height is kept.

**Reverse direction** turns a road or a chain round where it lies. The traffic on it goes the other way (a one-way street, a track's direction).

`.` and `,` select the next or previous road piece, the camera following: walk a long road piece by piece, as with OMSI 2's editor's next spline. `Shift` adds the piece to the selection.

### Height profile

Select a road or a chain (double-click for all of it). Its properties show the **Height profile** over the ground, coloured by gradient.

- **Length**, **Climbs, falls**, **Steepest** (and where), **Highest over the ground**, **Deepest under it** (in a cut).
- Over 8 % is steep for a bus; over 12 % few streets go.
- Click the profile: go there in the view.
- Drag a dot (a joint) up or down: its height. The gradients stay smooth.

### Cutting and joining

The **Knife** (`K`, the **Knife** switch in the status bar, or **Cut** in a road's properties) splits a road. Use it to give a stretch its own type or settings (a bridge, a tram section).

1. Turn the knife on.
2. Click a road where it should be cut, as often as you like. The two pieces' lengths show at the cursor. With grid snapping on, cuts fall on whole grid steps.
3. Right-click or `Esc` turns the knife off.

A right-click on a road also offers **Cut the road here**. Rows of objects along the road run on over the cut.

**Join with the next piece** makes two pieces one again. It works only where they make one curve. It refuses when nothing comes after, the next piece runs the other way or lies on another tile, the two have different road types or settings (path rules, ground, skew), a row of objects comes from another tile, or they do not bend, climb or bank alike.

**Complete to** (`J`): a new piece between the free ends of the selected splines.

### Parallel copy

Select a road, a track or a chain. **Parallel copy** in its properties copies it alongside, **To the left** or **To the right**, at a distance: a second track, the other carriageway. Bends stay exact arcs about the same centre. The copies are linked as the originals.

**Overhead wire** (under Parallel copy, for trams and electric trains): a wire spline laid on the selected track piece for piece and linked, as the stock maps lay theirs. Its height is the wire spline's own. It needs a wire spline whose name says `Fahrdraht`, `Oberleitung` or `wire`. Press **Lay it**. The masts go in with Objects along a road.

### Objects along a road

Select a road or a chain, pick an object in the catalogue (a lamp, a tree, a bollard). **Objects along the road** puts it every so many metres beside the road, as OMSI spline attachment rows that follow the road.

| Option | Meaning |
| --- | --- |
| **Every** | Spacing in metres |
| **Beside** | Right of the road's direction; negative: left |
| **Free at start / end** | Margins |
| **Height**, **Turn** | Height and turn of each object |
| **Both sides** | A row on each side |
| **Lean with the road** | Each object leans with gradient and banking (bays, arrows on the road); off, it stands upright |

Press **Place along the road**. The road's properties then list its rows: change step, side, height, turn, leaning and **Texts** its objects show, **Replace with**, **Remove**, or **Remove all rows**. A single row object can be **Detach**ed from the spline (it becomes a free object) and a free object **Attach**ed to one.

### Paint the ground along a road

In a selected road's or chain's properties. Lays a ground layer beside or under it, out from its edge with a soft rim: a gravel verge, dirt by a track, grass along a path.

| Option | Meaning |
| --- | --- |
| **Layer** | The ground texture (add one in the Paint tool first) |
| **Where** | **beside it** or **under it too** |
| **Out from its edge** | Width |
| **Soft edge** | Rim width |

Press **Paint**. The Paint tool's brush touches it up afterwards.

### Roundabouts

1. In the Roads tool, tick **Roundabout: click the centre, then the ring**.
2. Pick a one-way road type.
3. Click the centre, then the ring (or drag out to it).

The ring runs anticlockwise where traffic keeps right and clockwise where it keeps left. The roads that ran into the circle are cut back and joined to the ring with junctions. A two-way road type gives a warning.

### Bus pockets

1. In the Roads tool, tick **Bus Pocket Tool**.
2. Set the options below.
3. Press on a road. That stretch, the bay and a taper at each end, is cut out and widened on the kerb side.

The through lanes stay where they were. The width adapter lays a tapering plate with its lanes joined at each end. Each pocket is one undo step. The tool stays on, pocket after pocket, until you untick it.

| Option | Meaning |
| --- | --- |
| **Full width over** | Length of the bay at full width |
| **Bay width** | How much wider the road is along the bay; the other side stays as it is |
| **Tapers in metres** | Taper lengths in metres instead of a ratio |
| **Taper in** / **out** | Metres of road per metre of widening (more: gentler; 1:6 by default) |
| **Both ends alike** | Same taper at both ends |
| **Slide the bay to where it fits** | Near a road end or junction, the bay moves along the road to where both tapers fit |

A bay always gets a taper at both ends. Where one cannot be laid, nothing is changed. A road type made with a **Bus pocket** lane in the road maker can also be laid as a pocket.

### Road types and the road maker

**Tools → Road types and junctions…** (also in the Roads tool). The left lists **Road types**; **Make a new road type…** opens the **Road maker**, which writes an ordinary OMSI spline.

| Road maker option | Meaning |
| --- | --- |
| **Start from** | A ready-made road: Quiet street, Street with a footway on one side, Main road, Avenue with a green strip, One-way street, Street with a bus pocket, Country road, Service road |
| **Name** | The road type's name |
| **Lanes the way the road is drawn**, **Lanes against it** | 0 against: one-way |
| **Lane width** | Width of each lane |
| **Middle strip** | Width; **raised** puts it at footway height with a kerb |
| **Footways** | Width left; **a different one on the right** |
| **Traffic** | **keeps right** / **keeps left** |
| **Kerb** | Kerb height |
| **Bus pocket** | A lane for buses to stop in, right of the roadway |
| Textures | **Roadway texture**, footway textures (left, right), **Middle strip texture** |
| **Road markings** | **Paint my own markings**: **Standard for these lanes**, **Add a line** (**Solid**, **Dashed**, **Double solid**, **Solid and dashed**; width, dash, gap), or **Draw by hand…** |

**Top view** and **Side view** show the road made. **Draw by hand…** opens **Draw road markings**: a board with **Pen**, **Straight line**, **Dashed line**, **Rectangle**, **Arrow** and **Eraser** that paints onto the roadway texture. Pick a plain asphalt texture, or its own lines show too. The roadway texture is laid once across all its lanes.

Press **Create the road type**. Then **Build with it** or **Change it in the road maker**.

For each road type, the window also sets **Its junctions**:

| Option | Meaning |
| --- | --- |
| **Make a junction where it meets another road** | Off: they only cross |
| **Corners** | **× rounder** |
| **Traffic lights by themselves** | Lights on new junctions |
| **Zebra crossings on every arm** | Zebras on new junctions |
| **Junction pieces for it** | The piece offered first when a junction is put on this road's end |

These apply to junctions made from now on. Where several road types meet, the one asking more wins. Tracks branch off one another as switches and cross roads on the level.

## Junctions and crossings

### Automatic junctions

With **Auto-Junction** on, a road drawn into another's middle or across it cuts both. The gap is filled with a junction object of openMapper's own: an asphalt plate, `[path]` lanes for straight on, right and left, footpaths round the corners. It is written as ordinary OMSI content into `Sceneryobjects\openMapper\<map>`.

A road drawn into a plate, or a road end dropped on one, becomes another arm of it.

**Junction moves.** A junction, or any piece with spline helpers, that is moved, turned or raised carries the ends of its roads along, tangent. The road before a short arm bends with it. The ground follows its plate. openMapper's own plates are made again for their roads. Deleting an arm leaves the others joined; with two left, they become a smooth joint.

**Map → Make the junctions again** remakes every junction for its roads as they are now. **Build it again** in a junction's properties does it for one.

### Junction lights and zebra crossings

Select one of openMapper's junctions. Its properties show:

| Option | What it does |
| --- | --- |
| **Traffic lights** | Writes OMSI's light program into it, with the installation's signal heads on poles bound to it. The AI stops at red. Needs three roads or more |
| **Main road green**, **Side roads green** | Green times. The main road is the pair of roads most in line; red-yellow 2 s, yellow 3 s, 2 s all red between |
| **Zebra crossings** | Per road, with a footway across |
| **Build it again** | Makes the plate, kerbs and lanes again for the roads as they are now |

### Junction on a road end

A road's free end can take a junction piece. In the road's properties, **Junction on an end** lists pieces with places for roads; pick one **At the start** or **At the end**. The piece's place for that road type meets the end. If no piece is made for the road type, its first place is used.

### Switches

A track drawn off or into another's middle lays OMSI 2's own EW190 switch (left or right, for its rail type), with **Auto-Switch** on. The main track is cut out under it. The new track joins its diverging heel.

### Complex junction editor

**Tools → Complex junction editor…**: a top view to draw a junction out of small splines (asphalt, road markings, footways, kerbs, any type of the catalogue). Nothing it puts on the map is automatic, so it can be changed spline by spline afterwards.

| Drawing tool | How |
| --- | --- |
| **Select** | Pick a piece or an arm; drag it to move; drag the dots at the ends to change it; `Del` deletes |
| **Line** | Click the start, then the end |
| **Arc** | Click the start, a point on the arc, then the end |
| **Curve** | A smooth path through every point clicked; `Enter`, a double click or **Finish the curve** ends it |
| **Object** | Stand a scenery object (a lamp, a shelter, a bin); on a piece it turns along it |
| **Arm** | Where a road comes in: click the place, then a point out along the road; with the road type it takes |

| Switch | Meaning |
| --- | --- |
| **Snap to the grid and to ends** | Snapping |
| **Auto-Align** | A piece started at another's end goes on in line with it |
| **Auto-Adapt** | A road of another width keeps its edge and gets a taper |
| **Auto-Junction** | Arms found by themselves from free road ends; **Plate with kerbs and footways**, **Turns**, **Rounded corners** (**Corner radius**), **Straight on** |

A picked piece has **Length**, **Radius** (0 straight, positive right, negative left), **Lift** (above the plate: markings lie a little over the asphalt), type, **Copy**, **Reverse**, **Under the one before it** / **Over the one after it**, and **Objects along it**. **The whole drawing** can be moved by **Heading** and **Height**, **Mirror**ed or turned 90°. **Find the arms** makes the free road ends arms. **Layers** can be hidden; a hidden layer is not put on the map. **Preview** shows the junction as it will be; **Lanes** shows the AI's lanes.

**Painted line**: solid, dashed, double, solid and dashed, a **Stop line**, a zebra **Crossing**, a **Centre line** (double yellow), a **Lane line** (white dashed). Colour, width, dash and gap are set. Each is made as a spline with a transparent ground in `Splines\openMapper\marks` and drawn like any piece.

Saving:

| Button | Result |
| --- | --- |
| **Put on the map** | The pieces become ordinary linked splines at the middle of the view, turned as chosen |
| **Save as a custom junction** | A junction piece of the content folder (see below). Needs three arms or more |
| **Save as an object** | The whole drawing as one scenery object with AI lanes along its pieces; no arms needed |
| **Save the drawing** | Kept in `openMapper\junctions\<name>.json` of the content folder; **Open a saved one…** opens it again |

### Custom junctions

A drawing with its arms marked can be saved as a junction piece: a scenery object in `Sceneryobjects\openMapper\Junctions\…`. Its mesh is swept from the drawing's splines. It has `[splinehelper]`s at the arms and a `[path]` along every lane of its pieces, so the AI drives through it. It appears in the catalogue under **Junctions**: roads snap to it, and it moves with its roads.

With **Use in automatic junctions** on, the Roads tool's automatic junctions use it instead of their own plate when roads meet as its arms do:

- the same number of roads,
- each facing within 30° of an arm once the piece is turned to fit,
- of the same road type, or about as wide.

The roads are cut back to its arms and bent onto them. Draw the roads' own types at the arms so they join.

### Crossing editor

**Tools → Crossing editor…** (or the right-click menu on one object) edits the paths, the blocking and the traffic light program of an existing crossing or junction object, as OMSI 2's crossing editor does.

| Part | What you edit |
| --- | --- |
| **Paths** | Kind (**Footway**, **Rail**, **Street**), **Width**, **Direction**, **Turn signal**, **Light**, **Keeps the junction clear** (`[crossingproblem]`), start and end heading, **Radius**, **Length**, **Slope**, **Even grade**, **Surface**, **Split** |
| **Blocks** | Which paths block which |
| **Spline helpers** | Where map roads snap on (x, height, forward) and the road type (`.sli`) it takes |
| **Traffic lights** | Each light's **Phase**s (Red, Red and yellow, Green, Green and yellow, Yellow, Off) and **Duration**, the **Cycle** (`[traffic_lights_group]`), **Detector** (`[approachdist]`), **Request loops**, shown on a **Timeline** |

Paths are drawn in **2D** in the window (**Draw path**: drag from a path's start to its end) or picked in **3D** on the map. A dragged end snaps to another path's end and takes its way and rise; `Ctrl` keeps it free. **Start from a nearby path**, **Complete to a nearby path** and **Connect the roads** join paths to their neighbours and to the road lanes.

Saving: **Save** into the object's own file (the old one kept as `.bak`), into **openMapper's copy**, or **Save as a copy** (a new file in the content folder, the map's object pointed at it). **Revert** goes back to how the object was opened. **Remove the .bak files this editor made** cleans up. A crossing with lights needs signal heads on the map linked to it; the game only runs a program heads are linked to.

### Signal routes

**Map → Signal routes…**, as OMSI 2's editor keeps them in `signalroutes.cfg`. Each railway signal clears when a train wants the track pieces of its route and none stands on them.

1. Select a signal in the view; press **New route for the selected signal**.
2. Select the tracks and switches it protects (double-click a track: all of it); press **Take the selected track** or **Add them**.
3. Set **Aspect** (1 proceed, 2 proceed slowly…), **Distant signal**, **Next signal**, **Speed limit** and **Kind** (the stock maps have 0 and 1).

Each piece's place among its tile's paths is counted again on saving, as the game counts it. All 649 pieces of the stock maps come out the same. The map check names routes whose signal or track is gone.

### Trolleybus wire

**Map → Trolleybus wire…**: openOMSI's overhead wire for trolleybuses, saved as `trolley.txt` beside the map. OMSI 2 has none and ignores it. See also [Trolleybuses](TROLLEYBUS.md).

Ways to lay spans:

| Button | Result |
| --- | --- |
| **Over the selected roads** | Over the kerb lane each way, or one way, at the set **Height** |
| **Over the ticked lanes** | Over the ticked lanes of one selected junction (shown as they are ticked). Three spans meeting make a frog |
| **Read the map's wire splines** | A span along each pair-of-wires spline, as the game reads them |
| **Draw a span by hand** | Click its points in the view, at the wire's height over what is clicked. Started or ended on a span's end it joins it; on a span's middle it cuts it and branches off (a frog) |

**Network**: **Voltage**, **Wire spacing**, **Roughness** (worn or icy wire 0.003 and more). **Sections**: stretches fed by one substation, each live, with feeder resistance and regeneration. Where two sections meet there is an insulator with a dead length. Each **Span** has its section, own spacing, height and way. Every frog (three spans meeting) gets a **Switch**: set by power, the indicator, radio (route codes, a channel, a range) or fixed, standing straight, to the branch or at random at the start.

While the window is open, the wire is drawn in the view: the pair of wires in its section's colour, an arrow the way it runs, frogs yellow, insulators black and white. A span or switch is picked with a click. Joints the game cannot use (four spans in one point) and switches left without a frog are named.

**Kits.** openMapper makes no wire models of its own. A kit names yours: a `.cfg` file for this map (`<map>\trolley\`) or for every map (`openMapper\trolley\` of the content folder). Write it by hand or in the **Wire kit** window: each file typed, chosen from disk or taken from the map's selection, or the whole file edited **As text**.

| Kit entry | What it is |
| --- | --- |
| **Wire spline** | A `.sli` whose profile puts the wires at their height; laid along every span |
| **Mast** | A `.sco`: origin at its foot, arm reaching along its -y to the wire. **Masts** every so many metres, so far from their wire, **Mast turned** as the model needs |
| **Frog** | A `.sco` hung where the wire branches: origin on the wire, y along it |
| **Insulator** | A `.sco` hung where two sections meet |

openMapper lays a copy of each road piece in the kit's wire spline, moved sideways over the lane (straight pieces over a junction's lanes), the masts beside the road, and frogs and insulators wherever the wire has them. All this is put again after every change, in the wire's undo step.

## Objects and catalogue

### Objects tool (3)

1. Pick an object in the catalogue.
2. Click in the view to place it. `R` or `Ctrl` + wheel turns it first.

With **Line up with the nearest road** on, buildings face the nearest road with their front at a set distance from its edge (**Front at the roadside, back**). `Shift` puts it freely. Junction and crossing pieces (spline helpers) click onto the road ends near them.

The catalogue shows the installation's folder tree with thumbnails. Classes are read from the files: **Roads**, **Tracks**, **Footpaths**, **Fences, walls, kerbs**, **Technical (AI, helpers)** splines; **Objects**, **Trees**, **Stops and start points**, **Junctions**. Search, the **Show** filter, **Favourites** and **Recently used** are at the top. Right-click an entry to star it. An eyedropper picks a catalogue entry by clicking an object or spline in the view.

### Assets panel (B)

A panel like Unity's Project window, for every object and road type with pictures.

| Part | What it does |
| --- | --- |
| Folder tree | Folders of objects and splines |
| Grid of pictures | Search, kind filter (**All**, **Objects**, **Roads and splines**, **Textures**), **Picture size** |
| An asset's facts | Its size, where it comes from (**content folder (mod, import, own)** or **OMSI 2**) |
| **Place it** / **Build with it** | Use it. Double-click does the same; drag a picture into the view to place it |
| **Favourite on / off** | Star it |
| **PBR material…** | See [PBR maker](#pbr-maker) |
| **Show in the file manager** | Opens its folder |
| **Delete…** | Content folder only: its folder goes to `openMapper\bin`, from where it can be put back by hand. Maps that use it show it missing |
| **Add as a ground texture** | A texture becomes a ground layer |
| **Import…**, **Shapes…** | Import assets, make shapes |

### Asset import

**File → Import assets…**, the toolbar button, or drop files onto the window.

| Dropped | Result |
| --- | --- |
| A mod (a folder or `.zip` laid out like OMSI 2) | Merged into the content folder |
| A folder of `.sco` files (an object's folder with `model` and `texture`, or several) | A folder under `Sceneryobjects`, ready to place |
| A folder of `.sli` files | A folder under `Splines` |
| Models anywhere in a folder or `.zip` | An object each. A folder with only a `texture` folder beside its models is not taken for a mod |
| A 3D model: `.obj`, `.gltf` / `.glb`, `.fbx`, `.x`, `.o3d` | A new object (see below) |
| A `.zip` exported for sharing | The map installed |

A model keeps every node's transform, its materials' colours and textures (embedded ones too; any picture format is turned into one OMSI reads), and their normal and roughness / metalness / occlusion maps as `_nn` / `_orm`. A `.sco` is written for it, the textures are copied, and so are their PBR maps (`_nn`, `_orm`, `_rr`, `_mm`, `_aa` …, as openOMSI finds them). Imported models come in as openMapper mesh objects, a part for each node. See [Tutorial 5: your own models](MAPPER_MODELS.md).

### Shapes and lamps

**Objects tool → Shapes and lamps** (also **Shapes…** in the Assets panel).

| Item | What it makes |
| --- | --- |
| Shapes | Cube, Plane, Grid, Circle, Triangle, Triangular prism, Wedge, Corner wedge, Pyramid, Sphere, Icosphere, Cylinder, Cone, Torus, Tube, Capsule, Stairs, Arch. Textured in metres, put on the map with a click; sizes and segments change in the properties until edited |
| **Street lamp** | A pole with a lamp that glows and lights the ground at night |
| **Signal head** | A traffic light of the chosen lamps, driven by a crossing's light program (see [Lamps](#lamps)) |
| **Picture / pattern…** | A picture or texture as an object to place, move, turn and scale; **on the ground** or **standing**; several repeats make a pattern |
| **Light source** | A lamp that glows and lights the ground round it (**Light colour**, **Light reach, m**, **Lit** **at night** / **always**) |
| **Sound source…** | A `.wav` that plays where you put the marker, as far as **Sound reach, m** carries; the marker shows in the editor only |
| **Polygon** | See below |

### Polygons

A flat sheet or a slab, of a plain colour or a repeating picture, at the corners' heights.

- **Draw a polygon** (`Shift+P`): click the corners on the map. The first corner or `Enter` closes it; `Esc` takes the last corner back. With **Snap to road edges and surfaces** on, a corner takes the height of the face or edge under it.
- **From the selected splines** (`P`): fill the outline the selected roads and splines make end to end (kerbs round an island). With none selected, `P` starts clicking corners.

Options: **Texture size, m**, **Thickness, m** (0: a sheet), **Above the ground, m**. The mode stays on for the next polygon until `Esc` with no corners. Also in the right-click menu, the command search and a selected road's panel. The result is an object like any other; `Tab` edits its mesh.

### PBR maker

An object's **Create PBR** (or **Tools → PBR maps for a picture…**) makes normal, roughness, metal and occlusion maps from a texture with a live preview. They are written as openOMSI reads them (`_nn`, `_orm`). **Configure PBR** tunes them again or deletes them.

| Option | Meaning |
| --- | --- |
| **Normals (_nn)**: **Bumps**, **Only big shapes**, **Dark parts stand out (not sink in)** | How the normal map is made |
| **Roughness**: **Bright parts smoother** | Roughness from brightness |
| **Metal** | Metalness |
| **Shadow in hollows** | Ambient occlusion |
| **Back to the defaults** | Reset |

A texture with PBR maps of its own already is overridden by new ones made here. **Delete the PBR material** takes them away. In the viewport, normal, roughness, metalness and AO maps are found by openOMSI's rules and drawn with the same tangent frame. See [PBR materials](PBR.md).

### Export a model

Right-click an object or a road → **Export…**, or **Export…** in its properties. Formats: OBJ + MTL, glTF, GLB, FBX (binary, as Blender writes it), Collada, USD, STL, PLY, 3MF, OFF, DirectX `.x` (text or binary) and OMSI `.o3d`. Textures are copied beside (or packed in), in metres with z up. A single object comes out as the model is made; several things (roads too) as they stand together.

## Plants

Plants live in the **Objects** tool. Tick **Plant brush: sow trees by painting**.

| Mode | How |
| --- | --- |
| **Sow** + **With the brush** | Paint to sow the chosen tree with random turns |
| **Clear** + **With the brush** | Paint to take trees (and the mix's objects) away |
| **In an outline** | Click round an area (a wood, a park, a garden); `Enter` or the first point closes it. It is filled at once, or cleared. Roads and tracks stay free. One undo step |

| Option | Meaning |
| --- | --- |
| **Size** | Brush size (`[` / `]`, `Shift` + wheel) |
| **Apart** | Spacing between plants |
| **Mix (picked at random)** | Several trees: right-click catalogue entries → **Add to the plant mix**; **Take out of the plant mix** / **Out of the mix** removes them |

A tile with far more objects than the stock maps' tiles (some 600) gets a note in the map check. Plant with more space between the trees.

## Terrain

### Terrain tool (5)

Brushes shape the ground. Drag in the view.

| Mode | What it does |
| --- | --- |
| **Raise** | Raises the ground under the brush |
| **Lower** | Lowers the ground under the brush |
| **Smooth** | Softens bumps and edges |
| **Flatten** | Levels the ground to the height where you start dragging |
| **Roughen** | Gives flat ground the rolls and dips of a natural field; hold longer for more |
| **Ramp** | Click a start and an end: the ground between becomes one even slope as wide as the brush (an embankment, a ramp, a level bed). Its gradient is shown |
| **Lake / river** | See [Water and tiles](#water-and-tiles) |
| **Along a spline** | Click a spline: the ground under it, as wide as the brush either side, is brought to the spline's height, with the pieces joined to it. **Align the selected splines** does it for a selection |

| Option | Meaning |
| --- | --- |
| **Size** | Brush radius, 3–150 m |
| **Strength** | How fast the brush works |
| **Keep the ground under roads** | On: the brush does not raise the ground through a road |
| **To a height** | Flatten to a typed height instead of where the drag starts (a level square, a platform) |

**Level ground** in an object's properties levels the ground under its outline (see [Object properties](#object-properties)).

**Flatten the whole terrain** (also **View → Flatten the terrain (roads follow)**): every tile's ground levelled to the map's mean height. Roads, AI paths and junctions follow. One undo step brings the relief back.

### Height map

At the end of the Terrain panel. **Height map**: the ground as a grey picture, to shape in another program and bring back.

- **Export…**: the whole map's ground as a 16-bit grey PNG, a pixel per terrain sample. Its lowest and highest height are written in a `.txt` beside it. Shape it in World Machine, Gaea or an image editor.
- **Import…**: any grey picture (8- or 16-bit PNG, TIFF) laid back over all the tiles.

| Import option | Meaning |
| --- | --- |
| **over** … **a pixel** | Metres per pixel |
| **Black is** / **White is** (or **adds**) | Heights of black and white |
| **Replace the ground** / **Add to it** | Replace the relief or add to it |
| **Smoothing** | Hides steps; an 8-bit picture has only 256 steps of height |

Roads and objects keep their heights, so import before building. Press **Lay it on the map**.

Terrain from a real place: see [Satellite and terrain](#satellite-and-terrain). The terrain tutorial is [Tutorial 4: terrain](MAPPER_TERRAIN.md).

## Painting

### Paint tool (6)

Paints with the map's ground textures (OMSI `[groundtex]` masks). **Map → Ground textures…** opens the same tool.

| Part | What it does |
| --- | --- |
| **Ground layers** | The **base ground** and the layers over it. **Add…**, **Remove** |
| Order arrows | Up the list: drawn under the layers after it. Down: drawn over the ones before. A layer's painting moves with it |
| **Texture** | The layer's picture; its painting stays when it is changed |
| **Detail texture** | The fine grain seen close up (OMSI's detail texture) |
| **Repeats per tile**, **Detail repeats** | How often each repeats on a tile |
| **Paint detail** | 128 to 1024 texels across a tile; the masks are made again at the new size |
| **Brush**: **Paint** / **Erase** | Paint the chosen layer, or take it away |

Pick a layer, then drag in the view. Brush size: `[` / `]` or `Shift` + wheel.

### By slope and height

At the end of the Paint panel. Paints the chosen layer over the whole map where the ground is so steep, and so high, fading in at the edges: rock on slopes, sand by the water, snow on the tops.

| Option | Meaning |
| --- | --- |
| **Slope** | From which steepness |
| **Height** | From which height |
| **Fade** | Soft edge |
| **Cover** | How fully it covers |

Choose a layer above the base ground, then **Paint the map**.

See also [Paint the ground along a road](#paint-the-ground-along-a-road).

## Water and tiles

### Tiles tool (7)

A map is made of 300 × 300 m tiles.

| Action | How |
| --- | --- |
| Add a tile | Click beside the map |
| Remove a tile | Right-click it. A map needs at least one tile |
| A ring of tiles | **A ring of tiles round the map**: every free place beside the map, corners too, gets a tile at the height of its neighbours |
| Tile settings | Click a tile |

| Tile setting | Meaning |
| --- | --- |
| **Water** + height | OMSI's `[water]`: a surface over the whole tile at a height, seen where the ground lies below it |
| **Lamps light the ground at night** | `[variable_terrainlightmap]`: the game works out the light pools of the street lamps when it loads the tile. On for every new tile, as on the stock maps. Off: the tile's light map file is shown as it is |
| Satellite overlay | Show it, its opacity, own picture (see [Satellite and terrain](#satellite-and-terrain)) |

`G` shows the tile grid.

### Lakes and rivers

**Terrain → Lake / river**:

| Shape | How |
| --- | --- |
| **Brush** | Paint a lake or a river: the ground is dug to a bed under the water level (where you start the stroke, or the tile's water), and the tile gets water there |
| **Lake** | Click the shore round and close it on the first point (`Enter` closes too, right-click takes a point back) |
| **River** | Click the middle line from its source, finish on the last point (or `Enter`) |

Options: **Depth**, **Width**, **Banks** (how the banks slope to the shore). Water in OMSI lies at one level for a whole tile: another low place on the same tile shows it too, and a river runs level. Start a stroke at the bank.

### Tiles from another map

**Map → Tiles from another map…** puts another map's tiles (ground, painting, water, objects, roads, start points) beside or into this one.

1. **Choose a map…** (its `global.cfg`).
2. On the small plan, click tiles to take or leave them. Grey: this map. Orange: tiles to add. Red: on a tile of this map, left out.
3. **Move** them west, east, north or south into place. **Raise it by** a height if wanted. **Ease the ground where the maps meet**.
4. **Add**.

IDCodes are given anew. Roads running from one tile into another stay joined. Its ground textures are found among this map's or added in the same order. One undo step. Lines, timetable and AI lists stay with the other map.

## Real-place import

The tutorial is [Tutorial 2: a real place](MAPPER_REAL_PLACE.md).

### Satellite and terrain

**Map → Satellite and terrain…** (`I`) puts the map on the Earth and brings imagery and heights.

**Placing the map:**

1. **Find a place** (a town, a street, an address; `Enter`). Place search is OpenStreetMap's Nominatim. You can also drag and zoom the world map.
2. Press **Place the map here**, then click on the map: the middle of the orange tile goes there. **At the view's middle** does it without a click.
3. Set **North turned** for any angle.

A map with `[worldcoordinates]` lies on the Earth by itself; its tiles are Web Mercator tiles.

**Imagery:**

| Option | Meaning |
| --- | --- |
| **Imagery** source | The sources and how sharp they are here are listed; **The sharpest here** is picked |
| **Picture of a tile** | Every pixel of the source, up to 4096 px. Each tile's picture is cut exactly to the tile, so tiles meet without a seam |
| **Also load terrain heights** | Heights with the imagery |
| **New tiles get imagery by themselves** | Each new tile gets its picture |
| **Load for all tiles** / **For the selected tile** | Load (a tile is selected with the Tiles tool) |
| **Terrain heights only** | Heights alone |
| **Overlay on the ground**: **Show**, opacity | As a whole (also **View → Satellite overlay**) and per tile (also in the Tiles tool) |
| **Put the imagery into maps exported for sharing** | Off by default; check its licence first |

Sources:

- Esri World Imagery (no key).
- Official orthophotos with no key: Berlin, Brandenburg, NRW, Bavaria, Lower Saxony, the Netherlands, Switzerland, Austria, France, New York State.
- With your own key (**Keys for more sources (optional)**): Google (Map Tiles API), Mapbox or MapTiler. The key stays on this computer.
- **Your own tile server**: any XYZ, WMTS or WMS server, or Terrarium heights. `{z} {x} {y}` are filled in; a WMS gets `{bbox}` (EPSG:3857) and `{width} {height}`; `{key}` is your key.

Each source's attribution is shown (**Terms of use**).

**Terrain heights** come from the AWS Terrain Tiles (Terrarium).

| Option | Meaning |
| --- | --- |
| **Relative** | The orange tile's mean height is 0 m |
| **Raise everything by** | Extra height |
| **Smoothing** | Smooth the heights |
| **Fit their edges** / **Leave them** | Tiles beside the loaded heights that were not loaded: their edge takes the loaded heights, fading out 60 m into the tile |

Heights load as one undo step; the tiles' shared edges match.

Downloads are cached in `~/.openomsi/cache/openmapper/tiles` and never fetched twice. The map keeps its pictures in `openMapper/` in its folder, which the game ignores. They are left out of shared exports unless asked for.

**Other pictures in the same window:**

| Option | What it does |
| --- | --- |
| **Load the OpenStreetMap map** / **OpenStreetMap map for all tiles** | The map as openstreetmap.org draws it (streets with names, houses, water, green), laid on the tile in place of a satellite picture |
| **OMSI's aerial photos** → **Choose a folder of them…** | Copies a folder of OMSI aerial photos onto the map's tiles. **Hold only the photos of the tiles near the camera** |
| **Own pictures** | See below |

### Own pictures on tiles

A scan, a screenshot, an old plan, laid on tiles and lined up by eye. No place on the Earth is needed.

- One tile: **Load image file…** in the tile's overlay (or drop the picture on the window with the Tiles tool and a tile clicked).
- Several tiles: **Tiles from** … **to** …, then **Load image file over these tiles…**. It is cut into their pieces.
- Line it up: **Turn left 90°**, **Turn right 90°**, any angle, **mirror left-right**, **mirror top-bottom**, **Move**, **Size**, **Reset**, **Remove**. The 3D view follows.

### From OpenStreetMap

**Map → From OpenStreetMap…**, for a map placed on the Earth. Best on tiles with no roads yet. Load the terrain heights first: the roads follow the ground as it is when they are built.

1. **Download for the map's tiles** (Overpass API, kept a week in the cache). What it found is shown with how much there is.
2. Choose what to build, and with what (untick what you don't want).
3. Press **Build**. Everything is one undo step.

| Item | How it is built |
| --- | --- |
| Roads by kind: **Main roads** (motorway, trunk, primary, secondary), **Streets** (tertiary, residential, unclassified), **Service roads** (yards, car parks, depots) | Each with the road type chosen for it, as if drawn: one street's ways joined into one road, corners rounded into OMSI's arcs, cut exactly at the map's edge. Junctions (with Auto-Junction on) where roads meet or cross and where streets end at one node; roads ending in a plate there are joined to it as arms. Tunnels and roads lying on ones the map has are left out |
| **One-way roads** | Each carriageway of a dual road and one-way streets, with the one-way type chosen; else **as their kind** |
| **Tram tracks**, **Railway tracks** | With a track type, crossing roads on the level. With Auto-Switch on, a track branching off or joining another gets the installation's switch there, its new track starting from the diverging heel; otherwise the track it meets is cut there |
| **Bus stops** | With their names; the stop object chosen in **Game logic → Bus stops** at the kerb of the nearest lane |
| **Bus lines** | Each route through two or more of the map's stops becomes a trip of its line, the way found along the roads. Variants stopping alike count once; a route leaving the map is cut to its longest run within it. A tour from 06:00 to 22:00 for each new line, and a depot if there is none |
| **Trees and woods** | Single trees, rows and woods, planted with the plant mix or the tree chosen; none on a road. **apart in woods** sets the spacing |
| **Water** | Lakes and rivers (with their islands) dug below their banks, sloping down from them; OMSI's water put on their tiles a little under the banks |
| **Buildings** | Each outline filled with the model of a chosen catalogue folder (Buildings_MC, …) whose ground plan and height come nearest (OSM's height or storeys, else what its kind usually is), its front to the nearest road. An outline no model fits is left out |

A map without a start point gets one on a lane some 40 m before a bus stop, so the town can be driven at once. Data © OpenStreetMap contributors (ODbL).

### GPS tracks to trace

**Map → GPS tracks to trace…**, or drop the file on the window. A route recorded on a phone or drawn elsewhere (GPX tracks and routes, KML, GeoJSON), laid over a map placed on the Earth.

- It is drawn in magenta in the view, to draw roads along.
- Or **Build a road along it** with the Roads tool's type, as if drawn: corners rounded, junctions where it meets the map's roads, only its runs over the map's tiles. One undo step.

### Street model (MCP only)

The MCP tool `build_streets` builds OpenStreetMap streets through a street model rather than one OSM way per road. OSM ways and imagery are treated as evidence:

| Layer | What it does |
| --- | --- |
| Normalise | Ways split at shared nodes; nodes closer than 1.5 m merged; pieces under 2 m dropped; each way classed as carriageway, sidewalk (a footway 2–14 m beside a carriageway), crossing, yard path or parking aisle |
| Streets | Carriageways chained through their nodes; one-way pairs 6–30 m apart made one divided street; cross-section per side (lanes, widths); sidewalk lines absorbed into the street |
| Alignment | Each axis fitted as tangent-continuous straights and arcs with the class's minimum radius; heights from the ground, smoothed, with a maximum grade |
| Junctions | At street nodes of degree 3 or more, plates from the approaches; priority by class; zebras from crossings |
| Types | A road type chosen from the cross-section, else made with the road maker |
| Build | Splines, plates, paths, then objects at (street, distance, side, offset) along the kerb |
| Check | The MCP tool `street_check` reports heading breaks or gaps at links, free ends that are not dead ends, 2-arm plates, objects in a carriageway or not aligned, paths ending inside a road |

## Lines, timetable and traffic

The tutorial is [Tutorial 3: lines and traffic](MAPPER_LINES.md).

### Game logic tool (4)

| Mode | What it places |
| --- | --- |
| **Start points** | `[entrypoint]`: where the bus appears when a drive starts. On a road or in the depot, facing the way to drive |
| **Bus stops** | Where passengers wait, at the kerb of the nearest lane |
| **Car parks** | Where parked cars stand. Click for one; with **In a row along a line** on, click the start and end of a row along the kerb |
| **AI paths** | Lanes for the AI's **Cars**, **People** or **Trains**. Draw like a road: it snaps to lane ends (green: joined, red: dead end) and along lanes. A car path runs the way you draw it |
| **Other** | Sound sources, help arrows and other helpers the game does not show |

**Show these in the other tools too** keeps game logic visible elsewhere.

**Bus stops:**

| Option | Meaning |
| --- | --- |
| **On both sides of the road** | A twin of the same name across the road, facing the other way. Renaming one renames the other until **Split** |
| **Kind of stop** | **Compulsory stop**, **Request stop**, **Terminus** |
| **Waiting, at most** / **at least** | People waiting |
| **Getting off** | How many get off here compared with other stops |
| **Length** | How far along the stop people wait and a bus may stand |
| **Platform** | The platform's side: right, left or both sides |
| Name | Stop names may not repeat on stops far apart; **Make it different** fixes it |

**AI paths → Edit existing**: pick a path, drag its ends and middle, double-click to add a point, click a dot then `Del` to take a point out, `Del` with no dot to delete the path. **Connect to** joins an end to another point. Lanes are drawn thin with small arrows. Lanes of roads belong to their road type: use **Edit its AI lanes** on the road. **View → AI lanes** shows them in every tool.

### Lines and timetable

**Map → Lines and timetable…** (`L`). Lines, their trips and tours, written as OMSI 2's `TTData` (`.ttl`, `.ttp`, `.ttr`, `StnLinks.cfg`, `Busstops.cfg`). The files of a stock map stay as they are until its timetable is edited. Edits are part of undo and redo.

A line has **Trips** (a way from stop to stop; most lines have two, there and back) and **Tours** (one bus's day: which trip it drives when).

| Line action | What it does |
| --- | --- |
| **New line** | A new line |
| Copy | A copy of the line with its trips and tours (a variant to change) |
| **Players can drive it** | The line is offered to the player |
| **All trips** | Every trip of the map, those no tour drives too, to edit, give to a tour or delete |

**Trips tab:**

1. **New trip**, then **Pick stops on the map** and click the stops (marked orange) in order, or **Add stop…** from a list.
2. The way along the roads is found by itself and drawn blue. Each problem is named.
3. **Make the way back** adds a trip with the same stops the other way round.

| Trip option | Meaning |
| --- | --- |
| **Destination sign** | The line number the trip shows |
| **Drives on** | **Roads (bus)** or **Tracks (tram, train)** |
| **Pick the paths on the map** | Choose the way path by path, as in OMSI 2's editor; kept when the stops change. **Find the way automatically** forgets them |
| **Cut before the first stop** / **Cut after the last stop** | Where the way may run on |
| **Time profile** | Trip time, **Times from stop to stop** (fixed by hand or **Spread all by distance**), several profiles per trip (rush hour, night) |
| **How the stops are served** | Per stop: **On demand**, **Always**, **Not served (checkpoint)**, **Wait for departure time**, **Always + wait for departure time** |

**Tours and times tab:**

| Option | Meaning |
| --- | --- |
| **New tour**, **Number** | A tour |
| **Buses from** | The depot group the AI buses come from |
| **Runs on** | Days: Mo–Su, **Holidays**, **Every day**, **Monday to Friday**, **Weekends and holidays**, **In the school holidays**, **On school days** |
| **Fill from** … **to** … **break** … **Fill** | Departures fill themselves from a first and last time with a break after each trip |
| Departures | Each with the time profile it runs to |

A map without a depot gets one: **Add a depot with OMSI 2's MAN buses**, so AI buses drive the tours.

**Vehicles tab** (OMSI 2's `car_use/*.ocu`): for a span of dates (**From** … **to**), the types its tours are given: **Only these types**, or **Mostly these types** with a **share of the tours**. **Fleet numbers of single tours** fixes a bus to a tour.

**Holidays and school holidays** (`Holidays.txt`): **Public holidays** run only tours with Holidays ticked; school holidays pick the school-holiday or school-day tours. **Copy to** copies the days a year later (moving feasts like Easter are set by hand).

When roads a trip runs on are cut by a new junction, moved or deleted, its way is found again on saving (if a lane its saved track names is gone, or no longer as long). The game then does not lose the bus. Other trips' files, and trains on their tracks, stay as they are.

### Traffic and AI

**Map → Traffic and AI…** (`T`).

| Tab | What it holds |
| --- | --- |
| **Over the day** | How busy the map is at each hour: **Road traffic** and **Passengers**, as curves with draggable points (1 is normal). **Typical day…**: Quiet village, Town with rush hours, The same all day. A group can have its own curve, for some days only |
| **Vehicle groups** | The car groups and depots of `ailists.cfg`, vehicles picked from the installation. **Drives about on its own** for car groups. A depot's buses with fleet number and plate, and its **Depot file (.hof)** |
| **Parked cars and people** | What stands on the car parks, who walks about and waits, who drives the AI buses |
| **Depot files** | See below |

A group the map lacks can be copied from OMSI 2's own maps (**Make it with OMSI 2's vehicles**). An `ailists.cfg` in OMSI 2's first format is written in the second one when changed.

### Depot files

**Map → Depot files (destinations)…**, or the Traffic window's last tab. The map's `.hof` files give its buses their destinations, the stop names on their displays and the routes their IBIS knows.

- **Make one from the lines**: a destination for each trip's terminus, the stops it calls at, a route with its code for each way a line goes, the front and side signs' texts split into two lines where long.
- **Add them**: later lines are added; what is there is kept.
- Every destination's and stop's texts (IBIS, front sign lines, side sign, roller blind texture, display lines, full name), a route's code (line number and two digits: 7601 is line 76, route 01), destination and stops in order can be changed.
- **Use it for every depot** sets it for the AI depot groups.

Files are saved into the map's folder (where OMSI 2 looks) and the content's `HOFs` folder (where openOMSI looks). The sample map and a town from OpenStreetMap get theirs with their lines.

## Models, mesh editor and materials

openMapper's own objects are kept as `<name>.openmapper.json` beside the `.sco` and `.x` written from it, so OMSI 2 and openOMSI read them as any other object. The tutorial is [Tutorial 5: your own models](MAPPER_MODELS.md).

### Mesh object properties

| Option | Meaning |
| --- | --- |
| **Edit the mesh (Tab)** | Enter the edit mode. Any object can be edited: a copy of its type is made first, textures and lamps carried over |
| **Shared by … objects** / **Make this one single** | Changes apply to all objects of the type, unless made single |
| **Height above sea level** | Off: its height is kept above the ground |
| **Solid (collides)** | Collision |
| **Parts** | Meshes listed: shown or not, renamed, **Duplicate the part**, **Delete the part**, **Add a shape as a part**, **Separate parts into objects** |
| **Modifiers**, **Materials**, **Lamps**, **Bake**, **UV map** | See below |
| **Export…** | See [Export a model](#export-a-model) |

Several objects selected: **Join into one object** (`Ctrl+J`), and booleans.

### Edit mode

Vertices, edges or faces (`1 2 3`) picked with a click, `Shift` + click, a box, a second click or `Alt` + click for a loop. `L` / `Ctrl+L` linked, `A`, `Alt+A`, `Ctrl+I`. Moved, turned and scaled with the handles. X-ray picking (`Alt+Z`). It has its own undo (`Ctrl+Z`). **Show the textures** with a **Shading opacity**.

| Operation | Key |
| --- | --- |
| **Extrude** | `E` |
| **Inset** | `I` |
| **Bevel** | `Ctrl+B` |
| **Loop cut** | `Ctrl+R` |
| **Subdivide** | |
| **Fill** | `F` |
| **Bridge loops** | |
| **Connect** | `J` |
| **Poke** | |
| **Merge** (at the middle, first, last, collapse, by distance 1 mm) | `M` |
| **Dissolve vertices / edges / faces** | |
| **Delete** (or **Only faces**) | `X` |
| **Duplicate** | `Shift+D` |
| **Split** | `Y` |
| **Separate into an object** | `P` |
| **Spin 360°** (12 steps round the up axis) | |
| Mirror, **Smooth vertices** | |
| **Triangulate** / **Triangles to quads** | `Ctrl+T` / `Alt+J` |
| **Flip normals** / **Normals outward** | `Alt+N` / `Shift+N` |
| **Shade smooth** / **Shade flat** | |
| **Mark seam** / **Clear seam**, **Mark sharp** / **Clear sharp** | |
| Material per face: **Assign to selected faces** | |

Extrude, inset, bevel and loop cut follow the mouse until a click; the wheel sets segments or cuts. **Adjust the last operation** changes it afterwards (**Slide**, **Cuts**).

### Booleans (CSG)

Two or more objects selected; the one in the properties is cut by the others.

| Operation | Result |
| --- | --- |
| **Union** | One object of all, their insides gone |
| **Difference (cut out)** | The others cut out of the one in the properties |
| **Intersection** | Only what all share |
| **Slice (keep both pieces)** | The first cut into what is outside and inside the others |

**Keep the cutting objects** keeps the cutters. A **Boolean** modifier works with another part.

### Modifiers

As Blender's stack: on, off, order, **Apply** (make it part of the mesh), **Remove**.

| Modifier | Main settings |
| --- | --- |
| **Array** | Count, Relative, Offset, Merge touching ends |
| **Mirror** | Axis, Cut away the other side first, Weld at the mirror |
| **Solidify** | Thickness, Close the rim |
| **Bevel** | Width, Segments, Angle |
| **Subdivision surface** | Levels, Smooth (Catmull-Clark) |
| **Decimate** | Ratio |
| **Triangulate**, **Smooth**, **Weld**, **Edge split** | Factor, Repeat, Distance, Angle |
| **Displace** | Strength, Size, Seed, Along the normals |
| **Wireframe** | Thickness |
| **Screw** | Steps, Height, Axis |
| **Simple deform** | Twist, Bend, Taper, Stretch |
| **Cast** | Sphere or Box, Factor |
| **Boolean** | Difference, Union, Intersection, With the part |
| **Follow the ground** | Every vertex follows the terrain under it, **Above the ground** |

### Materials

| Option | Meaning |
| --- | --- |
| **Colour (multiplies the texture)** | Base colour |
| **Texture** | Any picture; OMSI's formats kept, others made PNG. **Save as…**: PNG, JPEG, BMP, TGA, TIFF, WebP or DDS |
| **Transparency** | **Opaque**, **Cut out** (leaves, fences), **Blended** (glass) |
| **Glows** | Light of its own; black for none |
| **Shine** | Specular |
| **Normal map**, **ORM map** | openOMSI's `_nn`, `_orm` |
| **Night texture**, **Light map** | OMSI's night texture and light map |
| **Import materials…** | From an `.mtl`, a glTF / GLB / FBX, or the pictures of one PBR material (base colour, normal, roughness, metalness, occlusion, opacity, emission), packed as openOMSI reads them |

### UV maps

**Unwrap**: **By seams (conformal)**, **Smart project**, **Cube projection (metres)**, **Cylinder projection**, **Sphere projection**, **Project from the view**, **Lightmap pack**. Mark seams on edges first for an unwrap by seams.

The **UV editor** window shows the texture under the faces: pick, move, turn, **Flip**, **Scale ×2** / **÷2**, **Fit**, **Pack islands**, **Average island scale**, **Seams from islands**, **Smart unwrap**, **Box unwrap**.

### Bake

As Blender's: the object's textures and colours painted into one picture on a new UV map, so the whole object is one material (fewer draws in the game). Optionally with **Ambient occlusion** (how far its shadows reach) and **Lamps' light at night (light map)**.

### Lamps

| Lamp | What it is |
| --- | --- |
| **Glow** (**Add a glow**) | OMSI's `[light_enh_2]`: a bulb or lit sign. Colour, size, **Seen from one side** (a cone) or **Star** (all round); **On**: **Always**, **At night** or by a script **variable** |
| **Light on the ground** (**Add light on the ground**) | `[maplight]`: the pool of light a street lamp throws at night. Colour, **Reach** |

**Street lamp** and **Signal head** in **Shapes and lamps** make one of chosen lamps in one click. The signal head is lit by its own script from the phase of the crossing it is linked to, as OMSI 2's `Ampel_Kfz_1`. It takes its light from a crossing with a light program within 40 m.

### Roads as meshes

A road or spline's shape made an object, to edit as a mesh: **Copy as an object** (beside it) or **Replace by an object** (the spline taken away, its lanes with it). For several splines: **Copy as one object**, **Replace by one object**, **Polygon from them**.

## Scripts

**Map → Scripts…**: a small IDE for the map's code in openOMSI. See [Writing plugins](PLUGINS.md) and [Plugin API reference](PLUGIN_API.md) for the API.

The map's scripts are a project: `plugins/map_<folder>/` of the content folder, a folder plugin of openOMSI. A map's old one-file script becomes its first script.

| Part | What it is |
| --- | --- |
| Scripts | Start in a chosen order when the map is played (right-click to move one). Each can be switched off (**Starts with the map**). One that fails leaves the others running. `map.on`, `map.every`, `map.after`, `map.watch` act on this map only |
| **Modules** | Code the scripts share: `require("modules.util")` |
| **Data** | Files the scripts read (`data/`), pictures and sounds |

**Editor**: tabs, Lua and Rust in colour, line numbers, indenting that follows the code, completion of the game's whole API (313 functions, 65 events, with their help and permissions) and of the file's own names. Find and replace, go to a line, comment out (`Ctrl+/`). Mistakes are found as you type: Lua 5.4 syntax, functions and events the game does not have, modules `require` cannot find. The **API** tab lists functions and events; double-click puts one into the script.

**Run and debug** (`F5`): the scripts run with the game's `omsi` API, sandboxed as the game sandboxes them. What needs a bus or a duty answers as the game does without one, and the console says so.

- The clock at a chosen time and speed sends `frame`, `minute` and `hour`.
- **Events**: any game event sent by hand with its arguments.
- Breakpoints (click beside a line, `F9`), pause, step over / into / out (`F10`, `F11`, `Shift+F11`).
- **Call stack**, each frame's **Locals** and **Upvalues** with tables to open.
- **Console** evaluates Lua in the frame stopped at.
- **Storage and objects**: the plugin's storage and the objects' variables.
- An endless loop is stopped after 10 s.

**Object scripts (Lua)**: scripts of scenery objects (`[lua_script]` of a `.sco`) are listed, edited, added to the selected object's type and run against the objects of that type on the map (`object_init`, `object_frame`, `object_click`). OMSI 2's `.osc` scripts of the selected object are read and checked with openOMSI's own compiler. Any object gets a script of its own in one click (Lua, or OMSI 2's `.osc` with its variable list). An object of the installation becomes the map's own copy first. Its lamps and parts follow the variables the script sets.

**Build .oop**: one file of the map's scripts to share, as the Development Tools' compiler makes it: obfuscated, with the permissions its code needs, signed with a key when given (**Sign with the key**). It goes into the project's `dist` folder.

**Rust plugin**: **Make the Rust plugin** from the Development Tools' template, edit it here, **Build and install** with `cargo` into `plugins/map_<folder>_rs.oop`. It needs Rust (rustup) with the `wasm32-unknown-unknown` target.

The MCP server's `script` tool lists, reads, writes and runs the project too.

## Map settings

### New map

**File → New map…** starts from OMSI 2's own template (`template/NewMap`).

| Option | Meaning |
| --- | --- |
| **Name**, **Description** | For the game's map list |
| **Size** | Tiles |
| **Ground height** | m above sea level |
| **Ground texture (all tiles)** | More layers are painted later |
| **Traffic** | **keeps right** / **keeps left** |
| **Busy** | Cars and passengers |
| **Years**, **year offset** | Added to today's year when the game starts at the real date |
| **Money**, **Tickets** | Money system and ticket pack |
| **Seasons** | **four, snowy winter**, **mild winter**, **summer only** |
| **Rail** | Trams and trains follow their tracks' switches |
| Start from a real place | The satellite, terrain and OpenStreetMap window opens at once |

The car groups its traffic names are copied from the stock maps: the template has none, so its streets would stay empty. The **Getting started** checklist opens by itself for a new map.

### Opening maps

**File → Open map…** or the start page opens OMSI 2's own maps (saved as a copy into the content folder; the original stays untouched) and mod maps. Right-click a map where maps are chosen: **Make a copy** under a new name, **Show in the file manager**, or **Move to openMapper's bin** (content folder maps only; `openMapper\bin\maps`, from where it can be put back by hand). A map opened again opens where it was left (kept in openMapper's settings, not in the map).

### Map properties

With nothing selected, the Properties panel shows the map.

| Part | What it holds |
| --- | --- |
| **Map** | Name, folder, traffic side, start points, area, tiles, roads, tracks, objects, stops |
| **Picture** | **Import your thumbnail**: a picture cut to 370 × 280, saved as `picture.jpg`, shown where maps are chosen |
| **Open the map with this view** | The camera as it is now is where the map opens (`[mapcam]`), here and in OMSI 2's editor |
| **Edit description** | Descriptions in several languages (`global_<LANG>.dsc`): name and description per language |

### Map settings window

**Map → Map settings…**, as in OMSI 2's editor.

| Setting | Meaning |
| --- | --- |
| **Money**, **Tickets** | Money system and ticket pack |
| **Depot** | The map's depot group |
| **Years**, **Year offset** | Added to today's year when the game starts at the real date (Berlin-Spandau: -25 for the 1980s) |
| **Repair time** | Minutes |
| **Rail** | Real rail: trams and trains follow their tracks' switches |
| **Helper** | Dynamic helper active (kept as it is) |
| **Seasons** | Spring, Summer, Summer (dry), Autumn, Winter, Winter with snow, each from a date. Summer where none is set |

### Calendar and place

**Map → Calendar and place…**, the map's own files as OMSI 2's editor keeps them.

| Tab | What it holds |
| --- | --- |
| **Place and time** | **Latitude**, **Longitude**, **Time zone** (where the sun stands). **Summer time** per year; **EU rules for these years** fills in last Sunday of March to last Sunday of October |
| **Holidays** | **Public holidays** (Sunday's timetable) and **School holidays** (the holiday one) by year |
| **Number plates** | The AI cars' plates, one a line; **Make** them from a pattern such as `B-XX 9999` (X: a letter, 9: a digit) |

### Chrono scenarios

**Map → Chrono scenarios…**: the map's changes over the years, listed with their dates, tiles, the lines they take off and their descriptions.

- **Show the map as on** a chosen day: every scenario active then is laid over the map, as the game does.
- **New scenario**: starts on the day shown. The installation's scenarios are brought into the map's own copy first.
- Edit a scenario's **From** / **To** dates, **Takes off** (lines), name, description, **Own tickets** and **Own money**.
- **From its first day, the selection in the view**: **appears** (taken into the scenario), **is gone** (from that day on), **becomes** the catalogue's type, **its texts become**, **Its path rules from that day**. Written as OMSI 2's editor does (`[selobject]`, `[delete]`, `[typ]`).
- **What it changes**, tile by tile: go there, take out of the scenario, or put back into the map itself. Every stock patch is written back unchanged.
- **Edit its own things on the map**: what the scenario adds comes onto the map to select, move and change; what you place meanwhile is the scenario's too. **Done** (or saving, a test drive, another map) writes it back.
- **Edit its timetable**: the Lines window shows the map's timetable with the scenario's laid over. Lines, trips and tours changed there are the scenario's; a map line taken away is taken off from its day. Only what differs is kept in its `TTData`.
- **Edit its AI lists**: the Traffic window, the same way. Added vehicles, type groups and a depot's own depot file go into `ailists_#upd.cfg`.

### Export and plan

| Command | Result |
| --- | --- |
| **File → Export map for sharing…** | A zip laid out like OMSI 2 with the map and every file of the content folder it uses (generated junctions, imported models with textures and PBR maps, mod assets), never the installation's own. A `readme.txt` lists what it needs. Dropping the zip on openMapper installs it |
| **File → Export a plan of the map…** | An SVG from above: tiles, water, roads at their widths, tracks and footpaths, every bus line's way in its own colour with a key, stops with names, a scale bar and north. To share or print |

A map's Lua scripts are code: Import does not install them; the readme says so.

## View and camera

### View menu

| Item | What it does |
| --- | --- |
| **Tile grid** (`G`) | The tile borders |
| **What the game does not show** | Helpers, invisible splines |
| **AI lanes** | Lanes in every tool, thin with small arrows |
| **Names of stops and start points** | Names over them near the camera |
| **Show**: **Trees**, **Objects**, **Roads and splines** | Layers. What is hidden is not picked or box-selected either |
| **Overview map** (`M`) | In the corner: tiles, roads, tracks and the camera. Click or drag to move there; double-click for a larger one |
| **Assets** (`B`) | The Assets panel |
| Satellite overlay | See [Satellite and terrain](#satellite-and-terrain) |
| **Flatten the terrain (roads follow)** | See [Terrain](#terrain) |
| **Saved views…** (`Ctrl+1…9`) | Places to come back to, named and kept with the map. `Ctrl+1…9` flies to the first nine, `Ctrl+Shift+1…9` keeps the view there |
| **Go to selection** (`F`) | Fly to it |
| **Save a picture of the view** (`F12`) | Into `~/Pictures/openMapper` |
| **Put the windows back in their first places** | For a window left somewhere awkward |
| **Language**, **Interface size**, **Theme** | 28 languages (those of openOMSI); 80–150 %; **Dark theme**, **Light theme**, **As the system** |
| **Camera** | **Unity** or **Map editor** |
| **Preset Key Shortcut (from other applications)** | See [Keys and mouse](MAPPER_KEYS.md#key-sets-from-other-programs) |
| **Load all tiles at once** / **Tiles loaded at once** | Off: only tiles round the camera load, as in OMSI 2's editor: 9, 25, 49, 100 (the usual, about 1.7 km) or 225 |
| **Time of day** | See below |
| **Season** | **Summer**, **Dry summer**, **Spring**, **Autumn**, **Winter**, **Snow**, with the seasons' own textures as OMSI keeps them (`Fall`, `Winter`, `WinterSnow` …), the ground's too |
| **Graphics API** | **Automatic**, DirectX 12, Vulkan, Metal, OpenGL (as the system has them). After a restart |
| **Graphics quality** | **Low (no shadows, rough edges, 3 km view)**, **Medium (softer shadows, 8 km view)**, **High** |

The round **2D / 3D** button at the top right of the view (`0`) switches to a plan view from above, north up.

### Time of day

**View → Time of day**:

- **Plain day (as the game by day)** or **At an hour** with a slider.
- The sun stands where it is then, today, at the map's place (Calendar and place, summer time counted). The sky goes through dusk to a moonlit night. Shadows follow the sun.
- By night the objects' night textures and light maps glow (lit windows, street lamps, signs) as in the game.
- **Let time run**: the hour moves on while the editor runs. In Plain day it starts the clock at 12:00.
- **Speed, for testing**: 1x (a minute of the map a second), 5x, or an hour a second, for the sun and the lamps' cycle.

The day icon in the status bar starts and stops the clock too.

### Settings window

Open with **Search for a command** → `Settings…` (or `Controls`); on the start page, **File → Settings…**.

| Tab | What it holds |
| --- | --- |
| **General** | **Interface** (language, theme, size), **Camera and mouse**, **Big maps** (load a part of the tiles at a time), **Graphics API**, **Quality** |
| **Graphics** | Graphics options |
| **Camera** | **Flying speed**, **Keys over the map**, **Shift makes it**, **Turning with Q / E**, **Looking round**, **Orbiting**, **Panning**, **Zoom (wheel)**, invert options, **Flying builds up speed the longer the keys are held**, **Reset the camera** |
| **Keys** | Every key, changeable (see [Keys and mouse](MAPPER_KEYS.md#changing-keys)) |
| **Mouse** | The mouse and trackpad actions |

## Check and test drive

### Getting started

**Help → Getting started** opens by itself for a new map (**Show this for new maps**). A checklist that ticks itself as the map grows: ground, a road, a bus stop by the road, a start point, a line with a trip, a tour with departures, buses for the tours, checked, saved, a test drive. **Show me** shows each step.

### Map check

**Map → Check map…**: what would go wrong in the game, each issue explained, a click away, many put right with one more click (**Fix**, **Join**, **Fix all**; undoable). The list of checks and how to fix each is in [Checks and problems](MAPPER_TROUBLESHOOTING.md#map-check).

### Check in openOMSI

**Map → Check in openOMSI…**: the map saved and run by the game itself, unseen, with its own checks. The game's picture of the map from your view is shown beside what it found, problems first. A place it names is a click away. A few seconds on a small map. See [Checks and problems](MAPPER_TROUBLESHOOTING.md#check-in-openomsi).

### Test drive

The **Test drive** button (`F5`) asks:

| Option | Choices |
| --- | --- |
| **Start from** | **Where the camera looks**, a **Start point**, a **Bus stop**, or **See this view in openOMSI** (a free camera) |
| **Vehicle** | Any installed vehicle, or **No vehicle (on foot)** |
| **Time of day** | The hour |
| **Weather** | **The map's own**, **Natural weather**, **Weather cycle** |
| **Traffic**, **Passengers at the stops** | How busy |

**Start** saves the map and starts openOMSI on it. One session at a time: while it runs the button reads **Close test drive** and ends it.

- `Shift+F5`, **Map → Test drive from the start point nearest the view**, or a right-click on **Test drive**: start without asking.
- **Map → See this view in openOMSI**: this view with a free camera.
- **Map → Test drive in OMSI 2 (the original)…**: the map is copied into the OMSI 2 folder (Maps, and the objects, roads and textures made for it). Nothing there is replaced except earlier copies of this map. OMSI 2 itself runs on Windows only.

### MCP bridge for agents

`openmapper --mcp [path/to/global.cfg] [--allow-edit]` is a [Model Context Protocol](https://modelcontextprotocol.io) server on stdin / stdout, with no window. It holds the editor itself (with undo and redo), the catalogue and the models, so an agent works on a map as a person does. To add it to an agent:

```bash
claude mcp add openmapper -- openmapper --mcp /path/to/global.cfg --allow-edit
```

| Group | Tools |
| --- | --- |
| Seeing the map | `map_info`, `list_maps`, `render` (a picture from above: ground by height, water, roads at width, houses and plates, trees, named stops, a coordinate grid), `shot`, `raster`, `query`, `inspect`, `convert`, `terrain`, `catalogue`, `check`, `street_check`, `timetable`, `editor_state`, `reload`, `open_map` |
| As a person (`--allow-edit`) | `set_editor`, `click`, `stroke`, `select`, `command`, `undo`, `redo` |
| Many clicks in one call (`--allow-edit`) | `draw_road`, `roundabout`, `bus_pocket`, `place_objects`, `bus_stop`, `start_point`, `ai_path`, `terrain_brush`, `ramp`, `water`, `paint`, `paint_area`, `track_beds`, `form_ground`, `ground_under`, `plant`, `edit_object`, `edit_spline`, `build_streets`, `auto_junctions`, `road_type`, `plate`, `lay_spline`, `complex_junction`, `delete`, `tiles`, `ground_textures`, `make_line` (a bus line with its way and timetable), `fix`, `script`, `sample`, `new_map`, `save`, `export` |
| Batching | `batch` runs many calls in one request as one undo step |

Points are `[x, y]` world metres, latitude / longitude, or tile + local.

### Command line

| Command | What it does |
| --- | --- |
| `openmapper path/to/global.cfg` | Open a map straight away |
| `openmapper --check path/to/global.cfg` | The map check, printed, no window |
| `openmapper --export global.cfg out.zip` | Export the map for sharing |
| `openmapper --import file` | Install an exported zip or a mod |
| `openmapper --plan global.cfg out.svg` | Export a plan of the map |
| `openmapper --mcp [global.cfg] [--allow-edit]` | The MCP server |
| `openmapper --icon out.png 512` | Write the program's icon |
