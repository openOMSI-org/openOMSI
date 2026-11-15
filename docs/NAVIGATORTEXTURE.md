# Navigator Textures

A bus can have a GPS screen of its own in the cockpit: the mod models the screen, and openOMSI
draws the navigator's small map on it - the route, the next turn, the next stop, the speed and
its limit, as the navigator in the corner of the window shows them.

An openOMSI extension: Omsi.exe has no navigator and leaves such a slot an empty script texture.

### Example model config

```
[navigatortexture]
800
480
elec_busbar_av
1
0
0
0
0

##########################################

[mesh]
gps_screen.o3d

[matl]
gps_screen.dds
0

[useNavigatorTexture]
0
```

- **`[navigatortexture]`**: the screen's width and height in pixels (1 to 4096 each), then,
  optionally, the name of a script variable that switches the screen on. While the variable is
  0 the screen is black; while it is anything else it shows the map. Without the line - or with
  a variable the bus's scripts do not have - the screen is always on. A blank line or the next
  keyword after the size means "no variable".
- Five optional lines after the power variable control this screen: **`mapScale`**,
  **`fixedNorth`**, **`noNavUI`**, **`navCenter`**, then **`showHistory`**,
  in that order. These are values on separate lines,
  not additional keywords. To use them without a power variable, leave its line blank.
  Omitted options keep their defaults; use a blank line to skip an option before a later one.
- **`mapScale`** (default `1`): a zoom multiplier from `0.25` to `4`, inclusive. `2` zooms in to show half the
  usual distance; `0.5` zooms out to show twice the distance. The navigator's automatic zoom
  still applies. Values outside this range, non-finite and invalid values use `1`.
- **`fixedNorth`** (default `0`): `1` keeps north at the top of the map while the bus's
  arrow rotates with its heading. `0` follows the bus's heading as usual.
- **`noNavUI`** (default `0`): `1` fills the screen with the map and hides all information
  cards, including speed, speed limit, ETA, time, temperatures, turn instructions, the place
  row and the timetable. It also hides entry/spawn markers and AI scheduled-bus markers. The
  route, stop markers and the player's bus arrow remain visible. `0` shows the usual navigator
  interface. These options affect the cockpit screen only.
- **`navCenter`** (default `0`): `1` centers the map on the player's bus (the blue circle
  and heading arrow), removing the usual look-ahead offset. It works with either
  `fixedNorth` setting and with any `mapScale`. With `noNavUI=1`, the bus is at the center
  of the entire texture; otherwise it is at the center of the map area above the place row
  and timetable. `0` keeps the usual look-ahead view.
- **`showHistory`** (default `0`): `1` keeps the current trip's original route
  visible behind the bus in the usual route color and shows completed bus stops in grey.
  Upcoming stops keep their usual colors. Completion follows the duty's stop progression;
  the final stop turns grey when the trip is complete. History resets for a new trip or
  when leaving the duty. This also works with `noNavUI=1`. `0` keeps the usual view of
  the remaining route and stops. Free-drive destinations keep their usual behavior.
- **`[useNavigatorTexture] n`**: the material shows the `n`-th `[navigatortexture]` of the
  model (counted from 0). The material's own texture (`gps_screen.dds` above) only names the
  slot: the screen replaces it.

Like an `[htmltexture]`, a `[navigatortexture]` is registered as a `[scriptTexture]` too, and
`[useScriptTexture] i` with its script texture index binds it as well. If a navigator texture
follows `scriptTexture 2`, it is `scriptTexture 3` - but still `navigatorTexture 0`.

### Making the screen

- Model the screen as its own mesh (or its own material in a mesh), UV-mapped over the whole
  texture: the map fills the texture to its edges, from the top-left corner.
- The layout follows the screen's shape: a landscape screen around 5:3 (800 x 480) fits it
  best. On a wide, low screen the texts get smaller so that the cards do not overlap.
- The screen is drawn as a display, unlit, at its own brightness.
- Pick the size by how close the driver sits: 800 x 480 is sharp from the driver's seat;
  larger costs GPU time for nothing a player can see.

### What the player sees

- Only the player's own bus shows the map. The screens of the AI's buses and of the other
  players' buses stay black.
- The screen does not depend on the navigator in the corner of the window: the player can hide
  that one (Shift+N) and the bus's screen stays on.
- The screen keeps its last picture while the city map (Shift+M) is open.
- Only the first `[navigatortexture]` of a bus is drawn.

## For developers

- The keyword is read in `crates/omsi-model/src/screens.rs` (`NavigatorTextureDef`), next to
  the slot bookkeeping it shares with `[htmltexture]` (`reserve_script_texture`,
  `bind_script_texture`). `Model::script_texture_drawn_by_game` tells every consumer that the
  game draws that slot.
- The simulation keeps a 1x1 stand-in for the slot (`ScriptTexture::drawn_by_game`, made by
  `ScriptTexture::slots_of`): it never yields pixels to upload, to the network or to a saved
  situation, and takes none from them or from a plugin.
- The scene makes the slot a render target, black at first (`GpuCache::add_script_slot`,
  `Renderer::clear_texture`).
- The navigator draws into it (`crates/omsi-navigator/src/screen.rs`): `Screen::of_vehicle`
  finds the player's screen, `Navigator::set_screen` hands it over each frame, and the small
  map is drawn by the same `Navigator::draw` as the panel, told how by a `PanelLook` (the
  panel's `hud`, the screen's `screen`).
