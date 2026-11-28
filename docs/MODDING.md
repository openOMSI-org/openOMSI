# Modding

This page is the starting point for people who make buses, maps and objects for openOMSI. It
lists what openOMSI reads exactly like OMSI 2, which OMSI 2 limits it lifts, and the keywords it
adds, with links to the detailed pages.

## OMSI 2 content works as it is

openOMSI reads OMSI 2 content without conversion: `.bus`, `.ovh`, `.sco`, `.sli`, `model.cfg`,
`.o3d`, `.x`, `.osc` scripts, varlists, sound configs, maps, timetables and depot files. A mod made
for OMSI 2 needs no changes.

Everything on this page is an addition. A file that uses an openOMSI keyword still loads in
OMSI 2, which skips lines it does not know. Extra values written after the ones OMSI 2 reads are
ignored there too.

- File formats in detail: [File formats](FORMATS.md).
- Installing mods and the content folder: [Mods](MODS.md).
- Making maps and models with the editor: [openMapper](MAPPER.md) and
  [Importing models](MAPPER_MODELS.md).

## Limits openOMSI lifts

| Area | OMSI 2 | openOMSI |
|---|---|---|
| Memory | 32-bit program, about 2 GB of address space | 64-bit, no address-space limit |
| Texture size | Limited by memory and Direct3D 9 | Up to 16384 × 16384, halved until the card accepts it |
| Texture formats | BMP, TGA, JPG, DDS (DXT1-5) | Also DDS DX10 (BC4-BC7, float), KTX, PNG, WebP, EXR and more ([list](#textures)) |
| Interior lamps per mesh | 4 | Up to 63 |
| Spotlights on a vehicle | One at a time (`Spot_Select`) | Plus any number of `[spotlight_2]` and `[spotlight_cookie]` lamps |
| World lights | Few at once | Up to 32 point and spot lights per 25 m square, nearest first |
| Interior lamps per vehicle | Limited | No limit, drawn per pixel |
| Mesh visibility | One `[visible]` condition counts | Ranges, negation and groups of conditions |
| Passenger doors | 8 entries, 8 exits | 16 entries, 16 exits |
| Ticket validators | Only the last `[stamper]` is used | Every `[stamper]`, nearest to the passenger's door |
| Passenger places | Always available | Can be switched by a script variable |
| Meshes, materials, `[matl_change]` items, `[CTC]` entries, cameras, doors, passenger places, wheels, axles | Limited counts | No limit |
| Script variables and string variables | Limited count | No limit |
| `.o3d` with 32-bit indices | Limited | Full vertex and face count |
| Script texture, `[htmltexture]`, `[texttexture]` size | Limited | Any size the graphics card accepts |
| Suspension | Coil spring and damper | Coil, progressive, leaf or air, per axle |
| Model formats | `.o3d`, `.x` | Also glTF/GLB, FBX and OBJ |

## New keywords

Each keyword is described in a section below or on its own page.

### Model files (`model.cfg`, object models)

| Keyword | What it does | Details |
|---|---|---|
| `[visible_range]` | Shows a mesh while a variable is inside a range | [Visibility](#mesh-visibility) |
| `!` and `\|\|` in `[visible]` | Negate a condition, start an alternative group | [Visibility](#mesh-visibility) |
| `[spotlight_2]` | An extra switchable spotlight (pair or single) | [Lights](#extra-spotlights) |
| `[spotlight_cookie]` | A spotlight whose beam is a picture | [Lights](#picture-spotlights) |
| `[matl_glow]` | A material that is its own light source | [Glowing materials](#glowing-materials) |
| `[light_emitter_interior]` | A light-mapped mesh lights the saloon | [Light-emitting meshes](#light-emitting-meshes) |
| `[led_glow_effect]` | How strongly an LED panel's dots glow | [Glowing materials](#glowing-materials) |
| `[matl_emissive]`, `[matl_emissive_nits]`, `[matl_emissive_kelvin]`, `[matl_emissive_texture]`, `[matl_emissive_fade]` | A material's own light: colour, HDR strength or cd/m², temperature, script control and fading | [Emissive materials](PBR.md#emissive-materials) |
| `[matl_basecolor]`, `[matl_metallic]`, `[matl_roughness]`, `[matl_normal_strength]`, `[matl_occlusion_strength]` | PBR values of a material without texture maps | [PBR materials](PBR.md#factors-without-textures) |
| `[matl_nodirt]` | A material that takes no road dirt | [Road dirt](#road-dirt) |
| `[htmltexture]`, `[useHtmlTexture]` | A display drawn from an HTML page | [HTML textures](HTMLTEXTURE.md) |
| `[navigatortexture]`, `[useNavigatorTexture]` | A GPS screen showing the navigator | [Navigator textures](NAVIGATORTEXTURE.md) |
| More than four numbers in `[illumination_interior]` | More interior lamps per mesh | [Interior lamps](#interior-lamps) |

### Vehicle files (`.bus`, `.ovh`)

| Keyword | What it does | Details |
|---|---|---|
| `[add_camera_reflexion_static]` | A fixed camera for a CCTV or reversing screen | [Camera screens](#camera-screens) |
| `[suspension]` | Leaf, air or progressive springs per axle | [Suspension](SUSPENSION.md) |
| `[vehicle_script]` | A Lua or `.oop` script that belongs to the vehicle | [Plugins](PLUGINS.md) |
| `[procedural_dirt_off]`, `[procedural_dirt_rate]`, `[procedural_dirt_box]` | The vehicle's road dirt: off, how fast, its layout | [Road dirt](#road-dirt) |
| `[trolley_pole]`, `[trolley_rheostat]`, `[trolley_async]` | Trolley poles and electric drive | [Trolleybuses](TROLLEYBUS.md) |
| `[rail_class]`, `[rail_power]`, `[rail_traction]`, `[rail_brake]`, `[rail_coupler]`, `[rail_body_osc]`, `[pantograph_var]`, `[pantograph_trigger]` | Rail vehicle behaviour | [Rail](RAIL.md) |

### Passenger cabin (`passengercabin.cfg`)

| Keyword | What it does | Details |
|---|---|---|
| Two extra lines after `[passpos]` | Switch a place on and off, report who sits there | [Passenger places](#passenger-places) |
| Several `[stamper]` blocks | One validator per door | [Ticket validators](#ticket-validators) |

### Scenery objects (`.sco`)

| Keyword | What it does | Details |
|---|---|---|
| `[lua_script]`, `[lua_range]`, `[lua_rate]` | Lua or `.oop` logic for map objects | [Object scripts](OBJECT_SCRIPTS.md) |
| `[htmltexture]` in the object's model | A sign or screen drawn from HTML | [HTML textures](HTMLTEXTURE.md#scenery-objects) |

### Files in a map folder

| File | What it does | Details |
|---|---|---|
| `radio.cfg` | The map's own radio stations and frequencies | [Map radio](#map-radio-stations) |
| `openomsi_weather.cfg` | Live weather of a real place | [Weather](WEATHER.md) |
| `openomsi_climate.cfg` | Monthly climate for natural weather | [Climate](#climate-of-a-map) |
| `trolley.txt` | The overhead wire | [Trolleybuses](TROLLEYBUS.md) |
| `openomsi-realmap/` | A pre-built map picture for the navigator and city map | [Map packages](MAP_PACKAGE.md) |
| `dependencies.json` | Signed publisher packs the map needs | [Publisher packs](PUBLISHER_PACKS.md) |

### Script variables and callbacks

| Name | What it is | Details |
|---|---|---|
| `PAX_Entry8_Open` … `PAX_Entry15_Open`, the same with `_Req` and for `PAX_Exit` | Doors 9 to 16 | [Doors](#passenger-doors) |
| `PAX_Entry<n>_Busy`, `PAX_Exit<n>_Busy` | Someone stands in the doorway | [Doors](#passenger-doors) |
| `ev_Stamper<n>`, `PAX_Stamper_Used`, `PAX_Stamper<n>_Used` | A ticket was stamped at validator *n* | [Ticket validators](#ticket-validators) |
| `(M.V.GetRadioName)`, `(M.V.GetRadioSong)` | The radio's station and song | [Bus radio display](#bus-radio-display) |
| `Snd_Radio_Text` | Station and song text for a radio display | [Bus radio display](#bus-radio-display) |
| `Trolley_Pole<k>_Elev`, `Trolley_Pole<k>_Azim` | Trolley pole angles | [Trolleybuses](TROLLEYBUS.md) |

## Textures

- Textures up to 16384 × 16384 pixels load. If the graphics card cannot take that size, the
  texture is halved until it fits.
- A file is read by its content, whatever its extension says.
- A texture named in a model (`bus.bmp`) is looked for as `bus.dds` first, then under its own
  name, then as `.bmp`, `.tga`, `.jpg`, `.png`, `.jpeg`, `.webp`, `.tif`, `.tiff`, `.gif`,
  `.ktx2`, `.ktx`, `.qoi`, `.hdr`, `.exr` and `.pnm`.
- A texture keeps its full resolution within 150 m of the camera, so a bus's own 4K textures
  stay sharp. Distant scenery loses detail only when video memory runs out (the video memory
  setting, see [Graphics](GRAPHICS.md)). This replaces OMSI's `[texmemlimit]`.

Supported formats:

| Format | Variants |
|---|---|
| DDS | OMSI's DXT1-5 and uncompressed; DX10 header with BC4, BC5, BC6H, BC7; R10G10B10A2, R11G11B10 float, R9G9B9E5, 16- and 32-bit float, 16-bit channels, B5G6R5, B5G5R5A1, B4G4R4A4, R8G8, R8, A8; DX10 cube maps; ATI1, ATI2, BC4U, BC5U, RXGB; Direct3D float formats |
| KTX 1, KTX 2 | RGBA8, BC1-BC7, ETC1, ETC2/EAC, ASTC (not supercompressed) |
| Others | PNG (also 16-bit), JPEG (also progressive and CMYK), BMP, TGA, WebP, GIF, TIFF, QOI, OpenEXR, Radiance HDR, ICO, PNM, farbfeld |

For a two-channel normal map (BC5, ATI2), the z component is rebuilt from x and y.

### PBR maps

Normal, roughness, metalness and occlusion maps placed beside a texture are picked up
automatically. See [PBR materials](PBR.md).

## Models

### Model formats

A `[mesh]` (and every other place that names a mesh) may point to a **glTF/GLB**, **FBX** or
**OBJ** file as well as `.o3d` and `.x`. The format is detected from the file's content.

- The model is read as the modelling tool shows it: exported from Blender with the vehicle's
  front along +Y, as for an `.o3d`.
- Units are metres. FBX files may use any unit.
- Every node's transform is applied.
- Materials keep their base colour and colour texture. The material is named after the
  picture's file name, so `[matl] <picture> <n>` in the `.cfg` works as for an `.o3d`.
- A texture is found beside the model or in a `texture` folder beside it. A texture packed into
  a `.glb` or `.fbx` is read from the file.
- Skins (bone names and weights) and blend shapes are kept.
- FBX can be ASCII or binary of any version. OBJ reads its `.mtl` (`Kd`, `d`, `map_Kd`) and any
  polygon.

`.o3d` files with 32-bit indices (the long-index flag) are drawn with their full vertex and face
count.

### Mesh visibility

A mesh can have several visibility conditions, so no script is needed to show a mesh by two
variables at once.

```text
[mesh]
door_light.o3d

[visible_range]
door_open
0.5
1.5

[visible]
lights_on
1
```

- `[visible_range]` takes a variable, a lowest and a highest value. The mesh shows while the
  variable is between them, both ends included.
- Once a mesh has a `[visible_range]`, a `!` or a `||`, **all** its conditions must hold.
- With only plain `[visible]` blocks, OMSI 2's rule stays: the last one counts. To combine two
  variables, write one of them as a `[visible_range]` around its value.
- A `!` after the variable name negates the condition. `door_open !` with `1` shows the mesh
  while the door is not open. On a `[visible_range]` it means "outside the range".
- A line `||` between blocks starts another group. The mesh shows when all conditions before
  it hold, or all conditions after it.
- A variable the vehicle does not have reads 0, as in OMSI 2.

### Interior lamps

OMSI 2 lights a mesh with at most four `[interiorlight]` lamps, the four numbers of its
`[illumination_interior]`. openOMSI reads up to 63. Write the extra lamp numbers on the lines
after the first four. A blank line ends the list.

```text
[mesh]
saloon.o3d

[illumination_interior]
0
1
2
3
4
5
6
7
```

OMSI 2 reads the first four and skips the rest. The same works in `passengercabin.cfg`.

### Light-emitting meshes

`[light_emitter_interior]` after a `[mesh]` turns the mesh's light-mapped parts into lamps that
light the saloon, from what the texture shows. Use it for ceiling light strips, a lit
destination sign's back, an illuminated advert, a ticket machine's glow.

```text
[mesh]
saloon_lights.o3d

[light_emitter_interior]
1.5
4
lights_innen
```

- Every line after the tag is optional. A blank line or the next keyword ends it.
  - Line 1: the strength (`1` = as bright as the texture shows). Default `1`.
  - Line 2: the most lamps it may make (1 to 16). Default `4`.
  - Line 3: the variable that switches the light. Default: the `[matl_lightmap]`'s variable.
- The mesh needs a `[matl_lightmap]` or `[matl_nightmap]`. A night map alone has no switch:
  give the variable on line 3.
- openOMSI samples the map on every triangle of those materials. The lit triangles are
  gathered into groups, and each group becomes an `[interiorlight]` at the centre of its light,
  in its light's colour. A bigger and brighter lit area reaches further.
- The lamps are added to every mesh that has an `[illumination_interior]`, so the seats,
  floor and walls are lit by them, and they cast shadows like the other saloon lamps.
- OMSI 2 skips the tag.

### Glowing materials

`[matl_glow] <texture> <value>` after a `[matl]` makes the material its own light.

- The texture is a greyscale mask: white shines fully, black not at all.
- The light has the material's own colour.
- `value` is the brightness on the scale of the **LED glow** setting: `6` matches that
  setting's default, `20` is brighter than its maximum. The setting itself does not scale it.
- It shows by day and by night, only in the Enhanced renderers.
- A material that also has a `[matl_lightmap]` keeps the light map and does not glow.
- For a switched glow (a tail lamp), put it on a `[matl_item]` of a `[matl_change]`.

`[led_glow_effect] <value>` (0 to 1) says how much an LED panel's dots glow: `1` is the full
**LED glow** setting, `0.5` half of it, `0` means the material is no LED panel. Without it,
openOMSI guesses: a material with a `\S:n` script mask and an all-white light map counts as an
LED panel.

Both keywords are described in full in [File formats](FORMATS.md).

## Road dirt

With **Road dirt on the vehicles** on (the default, Enhanced and Enhanced+), every vehicle
gathers road dirt where it lands, laid out by its `.bus` file's `[boundingbox]` and axles (see
[Graphics](GRAPHICS.md#road-dirt)). It takes the place of the vehicle's own dirt overlay: a
material with `[alphascale] Dirt_Norm` is hidden while the road dirt is drawn, so the two
never show together. Off, or in Vanilla and Vanilla+, the vehicle's own dirt shows as in
OMSI 2. The windscreen's `Dirt_Wiped` layer, which the wipers clear, stays as it is.

A vehicle works without any of the following. They let a modder tune or refuse it; OMSI 2 skips
them.

| Keyword | File | What it does |
|---|---|---|
| `[procedural_dirt_off]` | `.bus`, `.ovh` | No road dirt on this vehicle: it keeps its own dirt overlay whatever the setting. For a bus whose dirt textures are made for it. |
| `[procedural_dirt_rate]` + a number | `.bus`, `.ovh` | How fast it gathers dirt: `1` as any vehicle, `0.5` half as fast, `0` never. The rain washes it off as fast as any. |
| `[procedural_dirt_box]` + six numbers | `.bus`, `.ovh` | The box the dirt is laid out on, as `[boundingbox]` (width, length, height, then the middle's x, y, z). For a body the `[boundingbox]` does not fit, such as one with a long overhang or a roof box. |
| `[matl_nodirt]` | model `.cfg`, after a `[matl]` | This material takes no road dirt: a destination display, a chrome strip, a logo, glass that should stay clear. |

A rear section of an articulated bus is laid out by its own file and carries the dirt of the
front section. To look at the dirt without driving for an hour, start the game with
`OMSI_ROAD_DIRT=spray,film,dust,wet[,slush,mud]` (each 0..1, see
[Debug flags](DEBUG_FLAGS.md)).

## Lights

### Extra spotlights

`[spotlight_2]` in `model.cfg` is a spotlight of its own. Low beam, main beam, fog lamps or a
lamp over a door can light the road together, each switched by its own variable. They add to
the one `[spotlight]` that `Spot_Select` picks.

```text
[spotlight_2]
0.95
5.95
0.652
0
1
-0.3
255
255
233
200
30
80
lights_fern
0
```

The values, one per line:

| Lines | Meaning |
|---|---|
| 1-3 | Position x, y, z |
| 4-6 | Direction x, y, z |
| 7-9 | Red, green, blue (0-255) |
| 10 | Range (m) |
| 11-12 | Inner and outer cone angle |
| 13 | Variable: 0 off, 1 full, between dimmed. A number is a constant. |
| 14 | `0` (or left out): a mirrored pair. `1`: a single lamp. |

- With flag `0`, a twin lamp stands on the other side, mirrored across the vehicle's axis.
  Put the position on one headlamp. A pair is as bright as one `[spotlight]` of the same colour.
- With flag `1` there is one lamp, for example over a door or a cornering lamp.
- The position is used as written. Unlike a `[spotlight]`, it is not moved to the vehicle's front.
- A rear section's model may have its own. Their variables are the bus's.

### Picture spotlights

`[spotlight_cookie]` is a lamp whose beam is a picture: the picture is what the lamp would show
on a wall in front of it.

```text
[spotlight_cookie]
0.9
5.85
0.75
0
1
-0.04
255
250
235
80
70
30
lowbeam_right.png
lights_main
0.15
cornering_yaw
```

| Lines | Meaning |
|---|---|
| 1-3 | Position x, y, z |
| 4-6 | Direction x, y, z |
| 7-9 | Red, green, blue (0-255) |
| 10 | Range (m) |
| 11 | Horizontal angle the picture covers (degrees) |
| 12 | Vertical angle the picture covers (degrees) |
| 13 | The picture |
| 14 | Switching variable |
| 15 | Optional: fade time (s) |
| 16 | Optional: variable that turns the lamp right (degrees) |
| 17 | Optional: variable that turns the lamp up (degrees) |

- The picture's right is the lamp's right, seen from behind the lamp.
- White is as bright as a headlamp's strongest beam. Black is no light.
- The Enhanced renderers draw up to eight different pictures at once.
- The classic renderer, and a lamp whose picture is missing, light a plain cone instead.

Sobol's buses (MAN Enhance V4, Urbino III and others) write a shorter layout with the same
keyword. The picture carries the colour there, so there are no colour numbers or cone angles:

```text
[spotlight_cookie]
0.71507
5.82032
0.67736
0
1
0
200
lights_fern_front_1
0
cookie_fern_H4_lens.png
0.05
lamp_pitch
lamp_yaw
```

| Lines | Meaning |
|---|---|
| 1-3 | Position x, y, z |
| 4-6 | Direction x, y, z |
| 7 | Range (m) |
| 8 | Switching variable (a number is a constant) |
| 9 | Flag: 0 (or left out) for a twin across the vehicle's axis, 1 for the one lamp |
| 10 | The picture, in the vehicle's texture folder |
| 11 | Optional: fade time (s) |
| 12 | Optional: variable that turns the lamp up (degrees) |
| 13 | Optional: variable that turns the lamp right (degrees) |

The picture is 1280 x 1024, 0.09375 degrees a pixel (-60 to 60 across, 36 to -60 up/down); the
lamp covers that whole frame. The twin's place and direction are mirrored, but its offsets are
not, so both beams steer the same way. The two offsets come in the opposite order from the
openOMSI layout above. A block with no picture name after the flag is read in the openOMSI
layout, so both can live in one model.

With shadows on, the player's headlamps also cast shadows in Enhanced. The front lamps that
light the road share one shadow map.

## Camera screens

`[add_camera_reflexion_static]` in the `.bus` adds a fixed camera for a screen: a CCTV monitor
of the doors, or a reversing camera.

```text
[add_camera_reflexion_static]
0.0
-5.2
2.4
0.5
60
180
-30
```

- It takes the same values as `[add_camera_reflexion]`: x, y, z, distance, field of view, yaw,
  pitch.
- It counts together with the mirrors. Camera *N* draws into the texture `reflexionN.bmp`.
- Its picture does not follow the driver's head. It looks along its yaw (degrees clockwise from
  the vehicle's forward) and pitch (up).
- The picture is not mirrored, so map the screen's texture the right way round.

In an articulated bus, the cameras of a rear section count on from those of the section in
front. With mirrors 0 to 3 in the front section, the first camera of the rear section is
number 4. A cab screen showing `reflexion4.bmp` then shows that camera.

## Passengers

### Passenger doors

OMSI 2 has door variables for eight `[entry]` and eight `[exit]` paths. openOMSI has them for
sixteen of each: `PAX_Entry8_Open` to `PAX_Entry15_Open`, the same with `_Req`, and the same for
`PAX_Exit`.

- An entry or exit past the eighth becomes a door of its own once a script sets its `_Open`
  variable (or a varlist lists it). Its passengers then wait for that variable and request the
  door through its own `_Req`.
- Without its own variable, it opens with the eighth door, as in OMSI 2. Its passengers'
  requests go through the eighth's `_Req`. (OMSI 2 loses these requests.)

`PAX_Entry<n>_Busy` and `PAX_Exit<n>_Busy` (*n* 0 to 15) are 1 while someone is on the door's
threshold or between the door and the step outside: what a light barrier sees. People queueing
outside a closed door, in the aisle or on the upper deck do not count.

- A door script can keep a door open, or open it again, while `_Busy` is set.
- Like `_Req`, they are written before the scripts run each frame and cleared after.
- A door past the eighth without its own variables reports through the eighth's.

### Passenger places

In openOMSI a `[passpos]` may name script variables on the lines after its five values:

```text
[passpos]
0.94
0.04
0.92
0.43
0
layout_transverse
seat_12_taken
```

- **First extra line:** while this variable is 0, no new passenger takes the place. Whoever sits
  there stays until they get off. A bus can have two seating layouts and switch with a setvar.
- **Second extra line:** the game sets this variable to 1 while someone is on the place and to 0
  when it is free. A tip-up seat can fold down for the person on it.
- Both are ordinary variables from the bus's varlists.
- A place without the lines, or naming a variable the scripts do not have, is always available.
- A blank line or the next block ends the list. OMSI 2 skips the lines.

## Blind spot sensors: what the traffic sees

OMSI 2 scripts cannot ask the game what is around the vehicle, so a blind spot monitor in an
add-on could only be faked (a timer, a fixed route trigger). openOMSI measures it: the AI
traffic looks at the cars around the player's bus each frame and puts the result into six
variables of the bus's varlists.

| Variable | What it holds |
| --- | --- |
| `BSM_Left`, `BSM_Right`, `BSM_Rear` | 1 while a car is in that zone, 0 while it is clear |
| `BSM_Left_Dist`, `BSM_Right_Dist`, `BSM_Rear_Dist` | the gap to it in metres (0 while clear) |

The zones are the bus's own: 3.5 m out from each side along the body (plus 2 m past the
front so a car just pulling alongside counts), and 20 m behind the rear body line, all
within 3.5 m of the centre line. The gap is measured between the two bodies, not between
their centres. `BSM_Left`/`BSM_Right` are from the driver's seat, so they turn with the bus.

Declare only the variables the bus uses; a name the varlist does not have is not written at
all, and a bus that declares none of them is not touched - OMSI 2 add-ons need no change:

```
[varlist]
BSM_Left
BSM_Left_Dist
```

Then light a mirror lamp or play a sound as usual:

```
(L.L.BSM_Right) 1 =
{if}
  (L.L.BSM_Right_Dist) 3 <
  {if}
    (S.L.bsm_beep) 1 (S.L.mirror_lamp_right) 1
  {endif}
{endif}
```

What it does not do yet: no per-vehicle zone sizes (they are the same for every bus), no
front zone, no object type (a car, a cyclist or a pedestrian reads the same) and no relative
speed - the traffic's cars are all there is to see. Pedestrians and cyclists on foot are not
in the zones.

### Ticket validators

OMSI 2 uses only the last `[stamper]` of a `passengercabin.cfg`. openOMSI keeps every
`[stamper]` of every section. A passenger stamps at the validator nearest to the door they came
in by.

1. Write one `[stamper]` block per validator: path point, then x, y, z of the device.
2. The validators are numbered from 0 in the order of the file.

When a ticket is stamped at validator *N*:

| Event | Kind |
|---|---|
| `ev_Stamper` | Sound trigger (as in OMSI 2) |
| `ev_Stamper<N>` | Sound trigger, so each validator can have its own sound |
| `PAX_Stamper_Used` | Script trigger |
| `PAX_Stamper<N>_Used` | Script trigger |

OMSI 2 still uses the last block and fires only `ev_Stamper`. A bus that gives each validator its
own sound can mute the `ev_Stamper` sound with a variable that the `PAX_Stamper_Used` trigger sets.

## Scripts and plugins

- Script variables and string variables have no count limit.
- `(M.V.GetRadioName)` and `(M.V.GetRadioSong)` put the bus radio's station name and song title
  on the string stack. Both are empty while the radio is off or the station sends nothing.
  OMSI 2 does not have them, so check for an empty string.
- A vehicle can carry its own Lua or `.oop` scripts with `[vehicle_script]`. Lua plugins can
  read and write script variables, fire triggers and react to game events. See
  [Plugins](PLUGINS.md) and the [plugin API](PLUGIN_API.md).
- Map objects can run Lua scripts: see [Object scripts](OBJECT_SCRIPTS.md).
- The developer tools (`Ctrl+Shift+D`) show and hold variables, draw paths, places, boxes,
  axles, lights, mirrors and views over the bus, and reload `paths.cfg`, `[passengercabin]`,
  `sound.cfg`, `.bus`, `model.cfg` and constfiles while driving. See
  [Command line and developer tools](COMMAND_LINE.md).

## Displays

| Display | Use it for | Page |
|---|---|---|
| `[htmltexture]` | IBIS, passenger information, signs, any screen drawn from HTML | [HTML textures](HTMLTEXTURE.md) |
| `[htmltexture]` with the modern engine | Screens that need flexbox, positioning or modern JavaScript | [Modern HTML engine](HTMLTEXTURE_MODERN.md) |
| `[navigatortexture]` | A GPS screen in the cockpit | [Navigator textures](NAVIGATORTEXTURE.md) |
| `[add_camera_reflexion_static]` | CCTV and reversing cameras | [Camera screens](#camera-screens) |

## Radio

What the player sees and hears is on the [Radio](RADIO.md) page. This section covers what a map
or a bus can add. Omsi.exe reads none of it.

### Map radio stations

Put a `radio.cfg` beside the map's `global.cfg`, written like the player's: one
`name = address` per line.

- The map's stations come first on the station buttons while the map is driven. The player's
  stations follow. A player station with the same address as a map station is left out.
- The file's `volume` line is ignored: loudness stays the player's choice.
- The file is read again when another map is loaded.

### Frequencies

After the address, list the frequencies a station is on, for radios that show one:

```text
# name = address | frequency @ x, y | ...
Radiozurnal = https://rozhlas.stream/radiozurnal.mp3 | 94.6 @ 25500, 20000 | 90.9 @ 2307, -717
Regional    = https://example.org/regional.mp3 | 97.9
```

- `| 94.6` is the frequency everywhere. `| 94.6 @ x, y` is the frequency near that place.
- A station may list as many places as it has transmitters along the route.
- The place nearest to the bus decides the shown frequency. There is no blending, so one place
  per town is enough.
- `x, y` are game metres: the tile's column and row times 300 m, plus the position in the tile.

To find coordinates, look in the log when a bus is placed: `spawned at entry point 38
"Chlum,hl.sil." (12810.6, 3735.5, 60.3)`. Or take an entry point's numbers from `global.cfg`
(`[entrypoints]`: the position in the tile, then the tile's index into `[map]`). These are map
positions, not coordinates on the globe.

### Bus radio display

A radio script that shows the station and song declares the string variable `Snd_Radio_Text`
(in a `[stringvarnamelist]` file of the bus) and shows it in a text texture. openOMSI writes the
text there while the radio plays: ten characters, with longer text scrolling.

The radio itself reacts to the variables of OMSI's radio plugins:

| Variable | Meaning |
|---|---|
| `Snd_Radio` | 1 while a cassette or the radio plays (the first station) |
| `SndExt_Radio` | The station button, 0 = off |
| `SndVol_Radio` | Volume, 0 to 1, up to 2 |

Dmitrij's "Magnitola" works as it is. While it shows its track, its `magnitola_1` variable
(`frequency@station`, `@` being the line break) gets the map's frequency for the current place
in its first line and the station and song in its second.

## Weather

### Live weather of a real place

A map can have live weather: add an `openomsi_weather.cfg` beside its `global.cfg`. The format
and the player's setup are on the [Weather](WEATHER.md) page.

### Climate of a map

A map can give its climate in `openomsi_climate.cfg`. The natural weather preset uses it as its
baseline.

```text
[temperature]
-1.5
0.2
4.1
8.9
13.8
17.0
18.9
18.4
14.3
9.2
4.1
0.4

[precipitation]
42
35
41
37
55
68
71
62
49
40
47
50
```

- `[temperature]`: 12 monthly mean temperatures in °C, January to December, one per line.
- `[precipitation]`: optional, 12 monthly values in millimetres. Not used yet.

## Trolleybuses

OMSI 2 has no overhead wire of its own. Its trolleybus add-ons ship a plugin DLL that reads
Omsi.exe's memory and cannot run in openOMSI. openOMSI simulates the wire itself. A trolleybus
add-on needs two things:

1. **In the vehicle's `.bus`:** a `[trolley_pole]` per pole (roof hinge, boom length, spring
   force, mass, retriever and motor) and a drive, `[trolley_rheostat]` or `[trolley_async]`.
   - A vehicle with its own scripted drive just adds the poles. Its torque then works on the
     wire and stops without it.
   - A model with its own pole meshes says `visible 0` and rotates them by the game's
     `Trolley_Pole<k>_Elev` and `_Azim` variables.
2. **For the map:** a `trolley.txt` beside its `global.cfg`, or in a mod's `maps/<name>/` folder
   for a map that has none. It lists the wire spans with their sag, the sections and dead
   sections, the insulators and the switches.

The map's masts and hangers stay the map's own objects. Every key and variable is on the
[Trolleybuses](TROLLEYBUS.md) page. Converting existing add-ons: [Trolleybus mods](TROLLEY_MODS.md).

## Rail vehicles and routes

Trams, metros and trains use their own `.bus` blocks and a map's transport folder:

- [Rail](RAIL.md): vehicle classes, track, pantographs, traction, brakes and couplings.
- [Transport modes](TRANSPORT_MODES.md) and [Transport formats](TRANSPORT_FORMATS.md):
  stations, routes, wires, switches, depots and signals.
- [Routes](ROUTES.md): lines and timetables of a map.

## What stays as in OMSI 2

These behave as in OMSI 2, so existing content works unchanged:

- The script stack (8 values) and registers (`l0`-`l9`, `s0`-`s9`).
- One `[spotlight]` lit at a time per vehicle, the one `Spot_Select` picks. `[spotlight_2]` and
  `[spotlight_cookie]` add more.
- 100 particles per emitter.

In a multiplayer session, other players see up to 7 doors, 15 wheels and 127 lamps of a vehicle.

## Specialised pages

| Page | Covers |
|---|---|
| [File formats](FORMATS.md) | Every OMSI 2 file format as openOMSI reads it |
| [PBR materials](PBR.md) | Normal, roughness, metalness and occlusion maps |
| [HTML textures](HTMLTEXTURE.md) | Displays drawn from HTML, and the `window.omsi` API |
| [Modern HTML engine](HTMLTEXTURE_MODERN.md) | Flexbox, modern CSS and full JavaScript for displays |
| [Navigator textures](NAVIGATORTEXTURE.md) | A GPS screen in the cockpit |
| [Suspension](SUSPENSION.md) | Leaf, air and progressive springs |
| [Object scripts](OBJECT_SCRIPTS.md) | Lua logic for map objects |
| [Map packages](MAP_PACKAGE.md) | A pre-built map picture, street names, lines and relief |
| [Publisher packs](PUBLISHER_PACKS.md) | Signed and protected content packs |
| [Plugins](PLUGINS.md), [Plugin API](PLUGIN_API.md) | Lua and compiled plugins |
| [Trolleybuses](TROLLEYBUS.md), [Trolleybus mods](TROLLEY_MODS.md) | Overhead wire and poles |
| [Rail](RAIL.md) | Trams, metros and trains |
| [Freetex sync](FREETEX_SYNC.md) | `[matl_freetex]` pictures in multiplayer |
| [Mods](MODS.md) | Installing and ordering mods |
