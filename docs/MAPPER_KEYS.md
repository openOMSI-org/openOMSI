# openMapper keys and mouse

Every keyboard shortcut and mouse action of openMapper, the map editor for openOMSI. The tables show openMapper's own key set; most keys can be changed. On a Mac, `Ctrl` in these tables is `Cmd`.

What each tool does is in [Tools reference](MAPPER_TOOLS.md). The window itself is explained in [openMapper](MAPPER.md).

## Changing keys

Every command in [All commands](#all-commands) can be put on another key. The keys you change are kept in openMapper's settings.

1. Open the settings: **Search for a command** (`Ctrl+K`), type `Settings…` or `Controls`, press `Enter`. On the start page it is **File → Settings…**.
2. Open the **Keys** tab.
3. Click the key next to a command, then press the new key. `Esc` leaves it as it was.

A key already in use is taken from its old command, which is left without a key. **Back to the default key** returns one command to its key. **Reset all keys** returns them all. The **Find** box finds a command by its name or by a key.

The **Mouse** tab lists the mouse actions of the camera in use.

### Key sets from other programs

**View → Preset Key Shortcut (from other applications)** switches the whole set at once:

| Set | What changes |
| --- | --- |
| openMapper's own | The default. The keys in this document. |
| **OMSI 2's editor: mouse and keys** | OMSI 2 map editor's mouse and its keys. Off by default. |
| **Blender's keys** | Blender's keys for selecting, moving and the mesh. The mouse stays openMapper's own. |

Keys you changed yourself stay on their commands when you switch sets. See [OMSI 2's editor set](#omsi-2s-editor-set) and [Blender set](#blender-set) for the differences.

## Camera

openMapper has two camera styles, chosen in **View → Camera** (or **Settings → General → Camera and mouse**):

- **Unity** (the default): the right button looks round and `W A S D` fly, the middle button pans, `Alt` orbits.
- **Map editor**: the right button looks round and `W A S D` fly, the middle button pans, the wheel zooms to the cursor, a drag on the ground moves the map.

### Keyboard

| Key | Action |
| --- | --- |
| `W` / `S` | Move forward / back (also the up / down arrows) |
| `A` / `D` | Move left / right (also the left / right arrows) |
| `Q` / `E` | Turn the camera left / right. With the right button held: down / up |
| Arrow keys | Move over the map (`Shift`: faster) |
| `Shift` (held) | Move faster (how much: **Settings → Camera → Shift makes it**) |
| `Home` | Look straight down, north up |
| `0` | 2D plan view on / off (3D: tilt and turn again). Also the round **2D / 3D** button at the top right of the view |
| `F` | Go to the selection |
| `Ctrl+1` … `Ctrl+9` | Go to saved view 1–9 |
| `Ctrl+Shift+1` … `Ctrl+Shift+9` | Keep the current view as saved view 1–9 |

### Mouse (Unity camera, the default)

| Action | What it does |
| --- | --- |
| Right button held + move | Look round |
| Right button held + `W A S D` | Fly. `Q` / `E` down / up, `Shift` faster |
| Right button held + wheel | Change the flying speed |
| Wheel | Move towards the point under the cursor |
| Middle drag | Pan |
| `Alt` + left drag | Orbit round the point under the cursor |
| `Alt` + right drag | Zoom |
| `Alt` + middle drag | Pan |
| Left drag on nothing (Select tool) | Select with a box |

### Mouse (Map editor camera)

| Action | What it does |
| --- | --- |
| Right button held + move | Look round (`W A S D` fly, `Q` down, `E` up) |
| Wheel | Zoom to the cursor |
| Middle drag | Pan |
| Left drag on empty ground (Select tool) | Move the map |
| `Shift` / `Ctrl` + left drag on empty ground | Select with a box |
| `Alt` + left drag | Orbit round the point under the cursor |

### Trackpad

| Gesture | What it does |
| --- | --- |
| Two fingers | Pan (Unity camera: up / down zooms, sideways pans) |
| Pinch | Zoom |
| Twist | Turn |
| `Option` + two fingers, or `Option` + drag | Orbit round the point under the cursor |
| `Ctrl` + two fingers | Turn the selection or the object being placed |
| `Shift` + two fingers (Terrain, Paint, Plant brush) | Brush size |

### Wheel with keys held

| Action | What it does |
| --- | --- |
| `Ctrl` + wheel | Turn the selection or the object being placed, 15° a step |
| `Shift` + wheel (Terrain, Paint, Plant brush) | Brush size |

Speeds and sensitivities (flying speed, turning, looking round, orbiting, panning, the wheel, inverting) are in **Settings → Camera**. See [View and camera](MAPPER_TOOLS.md#view-and-camera).

## Tools and windows

| Key | Action |
| --- | --- |
| `1` | Tool: **Select** |
| `2` | Tool: **Roads** |
| `3` | Tool: **Objects** |
| `4` | Tool: **Game logic** |
| `5` | Tool: **Terrain** |
| `6` | Tool: **Paint** |
| `7` | Tool: **Tiles** |
| `B` | Catalogue (**Assets** panel) on / off |
| `M` | Overview map on / off |
| `G` | Tile grid on / off |
| `L` | **Lines and timetable** window |
| `T` | **Traffic and AI** window |
| `I` | **Satellite and terrain** window |
| `Ctrl+F` | **Find in map** |
| `Ctrl+K` | **Search for a command** (also `Ctrl+Shift+P`) |
| `F5` | **Test drive** in openOMSI |
| `Shift+F5` | Test drive from the start point nearest the view |
| `F12` | Save a picture of the view (into `~/Pictures/openMapper`) |
| `Ctrl+S` | Save |
| `Ctrl+Z` | Undo |
| `Ctrl+Shift+Z` | Redo (also `Ctrl+Y`) |
| `Esc` | Close the view's menu, call off what is going on, then back to the Select tool, then deselect |

`Ctrl+Z` and `Ctrl+Y` undo the map even while a text field has the keyboard. The script editor keeps its own undo.

## Selecting and editing

| Key or mouse | Action |
| --- | --- |
| Click | Select an object or a road |
| Double-click a road | Select all of it (every piece up to where it branches or ends) |
| `Shift` + click | Add to / take from the selection |
| `Shift` / `Ctrl` + drag on empty ground | Select with a box (`Shift` adds) |
| Drag the selection | Move it |
| `Ctrl` + click on a grouped thing | Select that one alone, not its group |
| Right-click | The view's menu (see [Right-click menu](#right-click-menu)) |
| `Ctrl+A` | Select all |
| `Del` (also `Backspace`) | Delete the selection |
| `Ctrl+C` / `Ctrl+X` | Copy / cut |
| `Ctrl+V` | Paste at the cursor |
| `Ctrl+D` | Duplicate |
| `Ctrl+G` | Group the selection |
| `Ctrl+Shift+G` | Ungroup |
| `R` | Turn the object by the angle step (`Shift+R`: the other way) |
| `N` | Place the catalogue's object at the cursor. With a spline selected and another tool than Roads: a new piece carried on from its end |
| `V` | Move handles on / off |
| `C` | Rotate handles on / off |
| `X` | Scale handles on / off |
| `Shift+V` | Snap vertex to vertex |
| `Shift+L` | Snap edge to edge |
| `Alt` + arrows | Move the selection one grid step (`Shift`: 10 cm) |
| `Alt+Page Up` / `Alt+Page Down` | Raise / lower the selection 10 cm (`Shift`: 1 m) |
| `.` | Select the next road piece (joined at its end; `Shift`: add it) |
| `,` | Select the previous road piece (joined at its start; `Shift`: add it) |
| `J` | Complete to: a new piece between the free ends of the selected splines |
| `U` | Ruler on / off |
| `K` | Knife on / off |
| `P` | Polygon: fill the selected splines, or click its corners |
| `Shift+P` | Polygon: click its corners |
| `Tab` | Edit the mesh of the selected object (see [Mesh edit mode](#mesh-edit-mode)) |
| `Ctrl+J` | Join the selected objects into one |

### Handles

| Mouse | Action |
| --- | --- |
| Drag an arrow | Move along X, Y or Z |
| Drag a square | Move in its plane |
| Drag the circle | Move over the ground |
| Drag a ring | Turn about X, Y or Z (snaps to the angle step) |
| Drag a scale arrow (cube end) | Scale along that axis |
| Drag a scale square | Scale along two axes |
| Drag the scale circle | Scale along all three |

### Vertex and edge snapping

| Key or mouse | Action |
| --- | --- |
| Click | Pick the source vertex or edge, then the target |
| `Shift` + click | Horizontal only |
| `Esc` or right-click | Cancel |

### Moving and turning by numbers

In the OMSI 2 and Blender key sets, `G` takes hold of the selection: it follows the pointer at once. In the Blender set, `R` does the same for turning. While you hold it:

| Key | Action |
| --- | --- |
| `X` / `Y` / `Z` | Lock the move to that axis |
| Digits, `-`, `.` | Type how far (metres) or the angle (degrees) |
| `Backspace` | Delete the last digit typed |
| `Shift` (turning) | Tenths of a degree |
| `Ctrl` (moving) | Snap onto a path |
| `F9` (moving) | Snap onto the next path |
| `Enter` or a left click | Done |
| `Esc` or a right click | Cancel |

## Drawing roads and outlines

| Key or mouse | Action |
| --- | --- |
| Click, click | Start and end of a road |
| Type in the fields at the view's corner | Length, radius, heading or rise of the road being drawn (empty fields follow the cursor) |
| `Shift` (held) | Draw a straight |
| `Enter` | Build the planned road |
| Numpad `Enter` | Confirm (finish what is being drawn) |
| `Ctrl` (held) while drawing | Join nothing at the ends |
| `Shift` + click (Objects tool) | Place freely, without lining up with the road |
| `K`, then click a road | Cut it there, as often as you like. Right-click or `Esc`: knife off |
| `U`, then click two points | Measure distance, height and gradient |
| `Shift` + clicks (ruler) | The corners of an area |
| Right-click (ruler) | Ruler off |

Outlines (plants in an outline, lake shore, river line, polygon):

| Key or mouse | Action |
| --- | --- |
| Click | Add a corner |
| Click the first corner, or `Enter` | Close the outline (plants: plant it) |
| Right-click (lake, river) | Take the last point back |
| `Esc` (polygon) | Take the last corner back; with no corners, leave polygon mode |

AI paths (**Game logic → AI paths → Edit existing**):

| Key or mouse | Action |
| --- | --- |
| Click a path | Pick it; every point becomes a dot to drag |
| Double-click the path | Put a point in |
| Click a dot, then `Del` | Take that point out |
| `Del` with no dot picked | Delete the whole path |

## Brushes

| Key or mouse | Action |
| --- | --- |
| `[` / `]` | Smaller / larger brush (3–150 m) |
| `Shift` + wheel | Brush size |
| Left drag | Paint, sculpt or plant |

## Right-click menu

A right-click in the view (Select tool, and Game logic on one of its things) selects what is under the cursor and opens a menu:

| Item | Key |
| --- | --- |
| **Copy** | `Ctrl+C` |
| **Cut** | `Ctrl+X` |
| **Paste here** | `Ctrl+V` |
| **Duplicate** | `Ctrl+D` |
| **Delete** | `Del` |
| **Cut the road here** (a road under the cursor) | |
| **Join with the next piece** (one road piece selected) | |
| **Complete to** (splines selected) | `J` |
| **Group** | `Ctrl+G` |
| **Ungroup** | `Ctrl+Shift+G` |
| **Select all like it** | |
| **Go to** | `F` |
| **Crossing editor…** (one object) | |
| **From the selected splines** (polygon) | `P` |
| **Draw a polygon** | `Shift+P` |
| **Export…** | |
| **Keep for other maps…** | |
| **Select all** | `Ctrl+A` |
| **Deselect** | `Esc` |

A right-click on the **Test drive** button starts a test drive from the start point nearest the view.

## Mesh edit mode

`Tab` (or **Edit the mesh** in an object's properties) enters the edit mode. These keys work only there. They follow Blender.

| Key or mouse | Action |
| --- | --- |
| `Tab` | Leave the edit mode |
| `Esc` | Cancel the running operation or drag, else leave the edit mode |
| `1` / `2` / `3` | Pick vertices / edges / faces |
| Click | Pick |
| `Shift` + click | Add to the selection |
| Drag on nothing | Box select |
| A second click, or `Alt` + click | Select a loop |
| `A` | Select all (again: none) |
| `Alt+A` | Select none |
| `Ctrl+I` | Invert the selection |
| `L` | Select the piece under the cursor and what is linked to it |
| `Ctrl+L` | Select linked |
| `E` | Extrude |
| `I` | Inset |
| `Ctrl+B` | Bevel |
| `Ctrl+R` | Loop cut (point at an edge first) |
| Wheel (during bevel or loop cut) | Segments or cuts |
| Click / `Enter` | Finish the operation |
| Right-click / `Esc` | Cancel the operation |
| `F` | Fill |
| `M` | Merge (at the middle) |
| `J` | Connect |
| `Alt+J` | Triangles to quads |
| `Ctrl+T` | Triangulate |
| `Shift+N` | Normals outward |
| `Alt+N` | Flip normals |
| `Shift+D` | Duplicate |
| `Y` | Split |
| `P` | Separate into an object |
| `X`, `Del`, `Backspace` | Delete (vertices, edges or faces, by the pick mode) |
| `V` / `C` | Move / rotate handles |
| `Alt+S` | Scale handles |
| `Alt+Z` | X-ray: see and pick through the mesh |
| `Ctrl+Z` | Undo (the mesh's own undo) |
| `Ctrl+Shift+Z`, `Ctrl+Y` | Redo |

### UV editor

| Mouse | Action |
| --- | --- |
| Click | Pick a face (`Shift` adds) |
| Drag | Move the selected |
| Wheel | Zoom |
| Right button | Pan |

## Complex junction editor

**Tools → Complex junction editor…** has its own keys while the mouse is over its drawing:

| Key or mouse | Action |
| --- | --- |
| Wheel | Zoom |
| Right or middle drag | Pan |
| Left drag on empty ground (Select tool) | Pan |
| `+` / `-` | Zoom in / out |
| `F` | Fit the whole drawing in view |
| `Enter` or a double click | Finish a curve |
| `Esc` | Cancel |
| `Del` / `Backspace` | Delete the picked piece or arm |
| `Ctrl+Z` | Undo |
| `Ctrl+Y`, `Ctrl+Shift+Z` | Redo |
| `Ctrl+D` | Duplicate the picked piece |

## Crossing editor

While **Tools → Crossing editor…** is open, the map is held: clicks pick the crossing's paths, not the map's objects. Dragging still pans.

| Key or mouse | Action |
| --- | --- |
| Click a path on the map | Pick it |
| Drag a path's end | Move it; it snaps to another path's end |
| `Ctrl` + drag an end | Move it freely |
| `N` | Add a path carried on from the picked one |
| `Del` | Remove the picked path |
| `K` | Cut the picked path in two |

## Script editor

In **Map → Scripts…**, while a code editor has the keyboard:

| Key | Action |
| --- | --- |
| `F5` | Run the scripts in the editor (while running: continue) |
| `Shift+F5` | Stop the run |
| `F9` | Breakpoint on the cursor's line (or click left of a line number) |
| `F10` | Step over |
| `F11` | Step into (not running: run and stop at the first line) |
| `Shift+F11` | Step out |
| `Ctrl+Space` | Complete |
| `Up` / `Down`, `Enter` or `Tab` | Choose and take a completion; `Esc` closes the list |
| `Tab` / `Shift+Tab` | Indent / unindent |
| `Ctrl+/` | Comment out |
| `Ctrl+S` | Save |
| `Ctrl+F` | Find (`Enter`: next, `Shift+Enter`: previous) |
| `Ctrl+H` | Replace |
| `Ctrl+G` | Go to a line |
| `Up` / `Down` in the console | Earlier / later lines typed |

## Other windows

| Where | Key or mouse | Action |
| --- | --- | --- |
| Overview map | Click or drag | Move the view there |
| Overview map | Double-click | Larger / smaller |
| Search for a command | `Up` / `Down`, `Enter`, `Esc` | Choose, run, close |
| New map, Test drive | `Enter` | Create / start |
| Satellite and terrain | `Enter` in the search box | Find the place |
| Start page | Double-click a map | Open it |
| Start page | Right-click a map | **Open**, **Make a copy**, **Show in the file manager**, **Move to openMapper's bin** |
| Catalogue | Double-click a picture | Use it |
| Catalogue | Drag a picture into the view | Place it |
| Catalogue | Right-click an entry | Favourite, plant mix |
| Window | Drop a file | Import it (mod, model, picture, GPS track) |

## All commands

The rebindable commands and their keys in each set. A dash means the set leaves the command without a key.

| Command | openMapper | OMSI 2's editor | Blender |
| --- | --- | --- | --- |
| Move forward / back / left / right | `W` `S` `A` `D` | same | same |
| Turn the camera left / right | `Q` / `E` | same | same |
| Look straight down, north up | `Home` | `Home` | `Num 7` |
| 2D plan view on / off | `0` | `0` | `0` |
| Go to the selection | `F` | `F` | `F` |
| Tile grid on / off | `G` | `Shift+G` | `Shift+G` |
| Overview map on / off | `M` | `M` | `M` |
| Catalogue on / off | `B` | `B` | `B` |
| Lines window | `L` | `L` | `L` |
| Traffic window | `T` | `T` | `T` |
| Satellite and terrain window | `I` | `I` | `I` |
| Tool: Select … Tiles | `1` … `7` | same | same |
| Place a new object at the cursor | `N` | `N` | `Shift+A` |
| Move handles | `V` | `G` (grab) | `G` (grab) |
| Rotate handles | `C` | `C` | `C` |
| Scale handles | `X` | `X` | `Shift+S` |
| Snap vertex to vertex | `Shift+V` | `Shift+V` | `Shift+V` |
| Snap edge to edge | `Shift+L` | `Shift+L` | `Shift+L` |
| Edit the mesh | `Tab` | `Tab` | `Tab` |
| Join the selected objects into one | `Ctrl+J` | `Ctrl+J` | `Ctrl+J` |
| Turn the object (`Shift`: the other way) | `R` | `R` | `R` (follows the pointer) |
| Ruler | `U` | `U` | `U` |
| Knife | `K` | `K` | `K` |
| Complete to | `J` | `J` | `J` |
| Build the planned road | `Enter` | `Ctrl+Enter` | `Enter` |
| Confirm (finish what is being drawn) | `Num Enter` | `Enter` | `Num Enter` |
| Smaller / larger brush | `[` / `]` | same | same |
| Delete the selection | `Del` | `Del` | `X` |
| Group / ungroup | `Ctrl+G` / `Ctrl+Shift+G` | same | same |
| Copy / cut / paste at the cursor | `Ctrl+C` / `Ctrl+X` / `Ctrl+V` | same | same |
| Duplicate | `Ctrl+D` | `Ctrl+D` | `Shift+D` (and `Ctrl+D`) |
| Select all | `Ctrl+A` | `Ctrl+A` | – (menu) |
| Undo / redo | `Ctrl+Z` / `Ctrl+Shift+Z` | same | same |
| Save | `Ctrl+S` | `Ctrl+S` | `Ctrl+S` |
| Find in the map | `Ctrl+F` | `Ctrl+F` | `Ctrl+F` |
| Search for a command | `Ctrl+K` | `Ctrl+K` | `F3` |
| Test drive (`Shift`: from the nearest start point) | `F5` | `F5` | `F5` |
| Save a picture of the view | `F12` | `F12` | `F12` |
| Polygon: fill the selected splines, or click | `P` | `P` | `P` |
| Polygon: click its corners | `Shift+P` | `Shift+P` | `Shift+P` |
| Next / previous road piece | `.` / `,` | same | same |

Keys that cannot be changed: the arrows, `Alt` + arrows, `Alt+Page Up / Down`, `Ctrl+1…9`, `Ctrl+Y`, `Ctrl+Shift+P`, `Backspace` (delete), `Esc`, and the keys of the mesh edit mode and the other editors.

### OMSI 2's editor set

**View → Preset Key Shortcut (from other applications) → OMSI 2's editor: mouse and keys** gives the mouse and keys of OMSI 2's own map editor:

| Key or mouse | Action |
| --- | --- |
| Middle drag | Turn the view |
| Right button held, moved up / down | Zoom in / out |
| Right click on the ground | Go there |
| `Shift` + right click | The view's menu |
| Wheel | Zoom |
| Arrows | Move the camera |
| `G` | Move the selection (it follows the pointer) |
| `R` | Turn it by one angle step |
| `N` | Put the catalogue's object at the cursor and move it |
| `Enter` | Confirm what is being drawn (`Ctrl+Enter` builds a road) |
| `Del` | Delete |
| `Shift+G` | Tile grid |

### Blender set

**View → Preset Key Shortcut (from other applications) → Blender's keys**:

| Key | Action |
| --- | --- |
| `G` | Move the selection (it follows the pointer; nothing selected: the move handles) |
| `R` | Turn the selection with the pointer (`Shift+R`: one angle step) |
| `Shift+S` | Scale handles |
| `X` | Delete |
| `Shift+A` | Add an object at the cursor |
| `Shift+D` | Duplicate |
| `Tab` | Edit the mesh |
| `F3` | Search for a command |
| `Num 7` | Look straight down |
| `Shift+G` | Tile grid |

`W A S D` and `Q / E` still move the camera, as in every set. That is why Blender's scale is on `Shift+S` and its select all is left to the menu. The mouse stays openMapper's own.
