# Navigator textures

A bus can have its own GPS screen in the cockpit. The mod models the screen, and openOMSI draws
the navigator's small map on it: the route, the next turn, the next stop, the speed and the speed
limit. This page is for vehicle modders.

This is an openOMSI extension. Omsi.exe has no navigator and leaves the slot an empty script
texture.

## Example

In the vehicle's `model.cfg`:

```text
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

1. Declare the screen with `[navigatortexture]`: its size, an optional power variable and
   optional display options.
2. Under the screen's `[matl]`, add `[useNavigatorTexture]` with the screen's number (counted
   from 0). The material's own texture (`gps_screen.dds`) only names the slot; the map replaces it.

## `[navigatortexture]` values

One value per line, in this order:

| Line | Value | Default |
|---|---|---|
| 1 | Width in pixels, 1-4096 | |
| 2 | Height in pixels, 1-4096 | |
| 3 | Power variable: the screen is black while it is 0 | Always on |
| 4 | `mapScale`: zoom, 0.25-4 | `1` |
| 5 | `fixedNorth`: `1` keeps north up | `0` |
| 6 | `noNavUI`: `1` shows the map only | `0` |
| 7 | `navCenter`: `1` centres the map on the bus | `0` |
| 8 | `showHistory`: `1` keeps the driven route and served stops | `0` |

- All lines after the size are optional. Omitted options keep their defaults.
- To use an option without a power variable, leave line 3 blank. Use a blank line to skip an
  option before a later one.
- A power variable the bus's scripts do not have leaves the screen always on.

### Options

- **`mapScale`**: `2` zooms in to half the usual distance, `0.5` zooms out to twice the distance.
  The navigator's automatic zoom still applies. Values outside 0.25-4 or invalid values use `1`.
- **`fixedNorth`**: north stays at the top and the bus arrow turns with the heading. With `0` the
  map follows the bus's heading.
- **`noNavUI`**: the map fills the screen. All information cards are hidden: speed, speed limit,
  ETA, time, temperatures, turn instructions, the place row and the timetable. Entry/spawn markers
  and AI scheduled-bus markers are hidden too. The route, stop markers and the bus arrow remain.
- **`navCenter`**: the map is centred on the bus (the blue circle and arrow), without the usual
  look-ahead offset. With `noNavUI=1` the bus is at the centre of the whole texture; otherwise at
  the centre of the map area above the place row and timetable.
- **`showHistory`**: the trip's original route stays visible behind the bus in the usual route
  colour, and served stops turn grey. Upcoming stops keep their colours. The final stop turns grey
  when the trip is complete. History resets for a new trip or when the duty ends. Free-drive
  destinations are not affected.

These options affect the cockpit screen only, not the navigator in the corner of the window.

### Script texture numbers

Like an `[htmltexture]`, a `[navigatortexture]` is also counted as a `[scriptTexture]`. So
`[useScriptTexture]` with its script texture index binds it too. If a navigator texture follows
script texture 2, it is script texture 3, but still navigator texture 0.

## Making the screen

- Model the screen as its own mesh, or its own material in a mesh, UV-mapped over the whole
  texture. The map fills the texture from the top-left corner to its edges.
- A landscape screen around 5:3 (800 × 480) fits the layout best. On a wide, low screen the texts
  get smaller so that the cards do not overlap.
- The screen is drawn as a display: unlit, at its own brightness.
- 800 × 480 is sharp from the driver's seat. Larger sizes cost GPU time without a visible gain.

## What the player sees

- Only the player's own bus shows the map. Screens of AI buses and of other players' buses stay
  black.
- The screen does not depend on the window's navigator. The player can hide that one
  (`Shift+N`) and the cockpit screen stays on.
- The screen keeps its last picture while the city map (`Shift+M`) is open.
- Only the first `[navigatortexture]` of a bus is drawn.
