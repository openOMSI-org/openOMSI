# Graphics

How openOMSI draws the picture: the graphics modes, the quality presets, what each option costs,
upscaling with DLSS, FSR and MetalFX, the graphics API, and the mirror panels. Every setting with
its default is listed in [Launcher and settings](LAUNCHER.md#graphics-settings).

## Graphics modes

Choose the mode in **Settings → Graphics → Graphics** (`graphics` in `settings.cfg`).

| Mode | What it draws |
| --- | --- |
| **Vanilla** (`vanilla`) | as OMSI 2 draws it: no sun shadows, ambient occlusion, detail grain, snow cover or rain drops of openOMSI's own |
| **Vanilla+** (`vanilla_plus`, default) | the same renderer with those extras |
| **Enhanced** (`enhanced`) | openOMSI's physically based renderer |
| **Enhanced + Real-Time Ray Tracing** | Enhanced with hardware ray tracing (the old name: Enhanced+) |

`--enhanced` and `--enhanced-plus` choose a mode for one game from the command line. A settings
file with `graphics=enhanced_plus` reads as Enhanced with ray tracing on.

### Enhanced

Enhanced uses high-range lighting with energy-conserving diffuse light and GGX reflections
(roughness from `[matl_envmap]`). It adds:

- a computed atmosphere that also lights the scene: Rayleigh scattering, ozone, haze that changes
  with the weather, and light scattered many times over. It gives the blue hour its depth and the
  sunset its colour, different every evening;
- clouds lit by the sun at their own height, so they glow pink after sunset; high veils dim the
  sun, and a passing cumulus takes it away from the street;
- the moon where it really is, with its real phase; stars where the sky is dark enough; a city's
  lamps lighting its own haze and clouds;
- contact-hardening sun shadows, aerial perspective and height fog;
- automatic exposure like a camera, a glow only real highlights produce, and a photographic tone
  curve with FXAA (`post_aa`).

The bus's own screens (IBIS, ticket and HTML terminals, LCDs) dim at night as a real dashboard's
do. Gauges and LED destination matrices are left as they are. A mod can mark a material as its
own light with `[matl_glow]` (see [Content formats](FORMATS.md)).

### Real-Time Ray Tracing

**Real-Time Ray Tracing** (`ray_tracing=1`, Enhanced only, from the next start) lets the
graphics card trace rays. It needs Apple M3/M4 or newer, or an NVIDIA RTX / AMD RDNA 2 card or
newer through Vulkan or DirectX 12. Elsewhere, or when the driver refuses, the game draws
Enhanced.

Every solid mesh within 420 m goes into an acceleration structure each frame. The picture then
traces:

- the sun's shadow per pixel: soft away from its caster, crisp at the contact (cut-out leaves and
  fences keep the shadow map);
- the sky's occlusion within two metres;
- reflections on wet roads, water, glass and lacquered paint, including what is off screen.

Its shadows, occlusion and reflections cannot be switched off one by one.

## Quality presets

**Quality preset** sets the mode and most options at once. A preset that matches your settings
is shown; anything else reads *Custom*.

| Preset | Mode | Main changes |
| --- | --- | --- |
| **Low** | Vanilla | no MSAA, 75 % render scale, 512 shadows, 128 mirrors (economical), 300 m view, few small objects, low rain, no puddles |
| **Medium** | Vanilla+ | the defaults: 4x MSAA, 8x anisotropic, 2048 shadows, 256 mirrors |
| **High** | Enhanced | 2x MSAA, street lamps and vehicles cast shadows, low puddles |
| **Extra** | Enhanced | 4x MSAA, 512 mirrors, cloud shadows, glow of lights, street light Auto, full puddles |
| **Max** | Enhanced | 16x anisotropic, 4096 shadows, every light casts shadows, mirrors as the picture, 2000 m view and 1500 m objects, street light On |
| **Ultra** | Enhanced | Max plus Real-Time Ray Tracing |

The video memory is left as it is.

**Optimise for my PC** measures your next drive for about a minute and adjusts the graphics to
reach the frame rate you aim at (*Automatic* is the screen's rate, or 60 to 144 fps). Your
settings are saved first; **Restore my previous settings** brings them back.

## Light and shadows

### Light has shadows

**Light has shadows** (`light_shadows`, Enhanced, default *Off*):

- **Street lamps and vehicles** (`street`): the twenty street lamps, floodlights and platform
  lights that light the view most each get a shadow map looking down from their head. The
  headlamps of your bus and of the cars in front of the camera get one along the road. Your own
  saloon's lamps get sixteen more, drawn in the bus's frame (seats, rails and panels cast shadows;
  moving parts are left out until they stop).
- **Every light** (`all`): spots that are no headlamp look along their own axis, and the two
  strongest lamps that shine every way get a cube map, so a wall far to their side also blocks
  them.

A pole, a car or a wall then keeps a lamp's light off what lies behind it, and the lamps' beams
in fog, rain and snow are cut by the same maps. In Enhanced the ground is lit by these lamps
only; the baked lamp pools of the map belong to Vanilla and Vanilla+. The maps are kept while
nothing in them moves and take about 8 MB at shadow size 2048 (33 MB at 4096).

### Street light, clouds and fog

| Option | What it adds | Cost |
| --- | --- | --- |
| **Street light** (`street_light`) | the shade of houses and trees over a street, and the light surfaces send each other | about 0.5 ms a frame (M4, 1080p) |
| **Cloud quality** (`cloud_quality`) | Low: the same clouds, a little grainier at the edges | Low saves about 0.8 ms |
| **Cloud shadows and light shafts** (`cloud_shadows`) | the clouds' shadow over a 9 km square, moving with the wind; sun shafts through gaps out to 7 km | |
| **Glow of lights** (`light_glow`) | lens glare round bright lights, halos in the night air | |
| **Volumetric fog** (`volumetric_fog`) | fog you can see the light in: shafts, mist banks, cloud shadows in the air | off saves about 1 ms |

**Street light** *Auto* turns it on except on a small or shared graphics chip, and the optimiser
turns it off when the graphics card is the limit.

### Other light options

- **Road glare** (`road_glare`, 0–200 %): highlights of the sun, headlights and street lamps on
  roads and ground in Enhanced, wet roads included. Daylight is not changed.
- **Traffic headlight brightness** (`traffic_headlights`, 0–200 %): other vehicles' headlamp
  glow and beams on the road, in every mode. Your bus and street lamps are not changed.
- **Night brightness** (`night_brightness`): lifts the night in Enhanced.
- **LED glow** (`led_glow`, 0–15): how bright an LED destination matrix's dots burn, with a halo.
- **LED mip strength** (`led_mips`, 0–4): how sharp an LED matrix stays. 0 gives the sharpest
  dots and the worst shimmer; 4 is calm; 1.3 keeps the dots a couple of pixels across.
- **OMSI's shadow meshes** (`shadow_blobs`): the flat blob under a vehicle (`[isshadow]`), laid
  on the road. Off, only the sun shadow map shades under a vehicle.
- **Shadows cast by** (`shadow_casters`): every solid mesh, or only the `[shadow]` meshes as in
  OMSI 2.

## Rain, puddles and snow

### Rain quality

| Rain quality | Drops on the glass | Spray | Falling streaks |
| --- | --- | --- | --- |
| **High** (default) | painted 30 times a second | every vehicle | full |
| **Medium** | 15 times a second | your bus only | half |
| **Low** | OMSI 2's texture rain (from the next bus loaded) | none | a quarter |

Rain and snow stay outside your bus's bounding box. Heavy rain darkens the day enough for the
saloon lights to matter.

### Spray

On a wet road every vehicle's tyres throw up spray, in every mode: a fan of water behind each
wheel and a mist that drifts and settles. It grows with the square of the speed and the depth of
the puddle, and a bus throws more than a car. Spray shows within 100 m of the camera, at most
1400 puffs at once, and never inside the bus you are in. Some stock vehicles also spray through
their own `[smoke]` on a road wet through.

### Puddles and Real-Time Reflections

Wet roads mirror what stands round them when **Reflective objects reflect** is above 0. The
setting is called **Puddles**, or **Real-Time Reflections** in Enhanced (`puddles`):

- **High** (default): traced up to 100 m at half the picture's size; your own bus (up to three
  coupled parts) is mirrored as real geometry out to 60 m.
- **Low**: within 40 m, at a quarter of the size, in half the steps.
- **Off**: no puddle reflections. Reflections on paint and glass stay.

From inside the bus they are seen through the windows, dimmed by the glass and the rain on it.
Dry and snowy roads and the mirrors draw none. On OpenGL the puddles show the sky only. Enhanced
also shades shallow rain ripples.

### Vehicle smoke

**Vehicle smoke** (`vehicle_smoke`): *All vehicles*, *Player only* (your bus and its sections) or
*Off*. It hides the vehicles' non-glowing smoke particles only. Scenery smoke, glowing effects
and spray keep their own settings. It works at once.

### Snow

**Built-up snow on roads and roofs** (`road_snow`, off by default, not in Vanilla): while it
snows, snow builds up on roads and roofs, with ruts and tyre tracks, and thaws again. Off, the
weather's own road snow of OMSI 2 is used.

### Road dirt

**Road dirt on the vehicles** (`procedural_dirt`, on by default, Enhanced and Enhanced+ only):
every bus, including stock, DLC and old mod buses, gathers road dirt where it would land, with
no dirt texture of its own needed. It takes the place of the bus's own dirt overlay (OMSI's
`Dirt_Norm` layer): with the setting on the overlay is hidden, off the overlay shows as in OMSI 2,
never both. Vanilla and Vanilla+ always show the bus's own dirt.

- **Wet roads:** spray behind and above each wheel, on the lower sides and on the back, where
  the air behind the bus lays a film that reaches higher up.
- **Dry roads:** a thin dust all over, the roof included. Off the road it builds up fast, and
  wet ground throws up mud.
- **Winter roads:** grey slush and salt.
- **Rain** washes the dust off within minutes and the caked spray slowly; the wheels keep
  laying it on below. Wet dirt is darker and glossy, dry dirt matt.
- **Windows** get dirty too, and a dirty pane is harder to see through from inside as well.
- **The wash** (game menu, or a bus wash) cleans it along with OMSI's own dirt.

How fast it builds up: an hour on wet roads leaves the wheel arches and lower sides plainly dirty;
an hour on dry roads barely shows. Only the outside of the body is dirtied: surfaces near the
`.bus` file's `[boundingbox]` that face out of it. Interior panels, seats and the dashboard stay
clean. The `Dirt_Norm` and `DirtRate` variables are kept for the scripts, and the windscreen's
dirt that the wipers clear stays. Dirt is drawn on the six vehicle bodies nearest the camera that
have any. Modders can switch it off for a bus, change how fast it gathers, lay it out on a box of
their own and keep materials clean (see [Road dirt](MODDING.md#road-dirt)).
`OMSI_ROAD_DIRT` sets the dirt on every vehicle for pictures (see
[Debug flags](DEBUG_FLAGS.md)).

### Trees

**Windy trees** makes foliage sway in the weather's wind. **Detailed trees** stops leaf pictures
showing as streaks close up and shades the crowns, for a little graphics time.

## Upscaling

The picture can be drawn smaller and brought up to the window's size. **Settings → Graphics →
Upscaling** (`upscaler`):

| Upscaler | Where | When it applies |
| --- | --- | --- |
| **Standard (Catmull-Rom)** (`default`) | everywhere | at once, with the render scale |
| **AMD FSR 1** (`fsr1`) | everywhere | at once; *FSR quality* and *FSR sharpness* |
| **NVIDIA DLSS** (`dlss`) | Windows, NVIDIA RTX, DirectX 12 or Vulkan | next start |
| **AMD FSR 3** (`fsr3`) | Windows, any card, DirectX 12 or Vulkan | next start |
| **Apple MetalFX** (`metalfx`) | Mac with Apple silicon, macOS 13+ | next start |

The temporal upscalers (DLSS, FSR 3, MetalFX) use the last frames and take the place of the
anti-aliasing and the render scale. Set them with **Upscaling quality** (`upscale_quality`):
*Native* (only smooths edges), *Quality*, *Balanced*, *Performance*, *Ultra performance*. They are
not used with a headset or a triple screen. Where one cannot start, `game.log` says why and the
picture is drawn without it.

### NVIDIA DLSS

DLSS is offered only when NVIDIA's DLLs are beside `openomsi.exe`. They are not part of the
openOMSI download. On Windows with an RTX card:

1. Open **Settings → Graphics**. Under **NVIDIA DLSS runtime (optional)**, read NVIDIA's licences
   (the buttons open them).
2. Tick the box to accept NVIDIA's Streamline and RTX/NGX terms.
3. Press **Install NVIDIA DLSS**.

The launcher downloads NVIDIA's official Streamline SDK 2.14.1 from NVIDIA's GitHub release,
checks its SHA-256 and each DLL's NVIDIA signature, and installs only `sl.interposer.dll`,
`sl.common.dll`, `sl.dlss.dll`, `nvngx_dlss.dll` and their licences. It then restarts to show
DLSS. NVIDIA's runtime still checks the card and driver when it starts.

### AMD FSR 3

The Windows x64 package includes AMD's four FidelityFX SDK 2.4.0 DLLs, which AMD's licence lists
as MIT-licensed. FSR 3 is offered on any vendor's card where the runtime supports it.

### Licensing

openOMSI's own source, DLSS and FSR integration included, is AGPLv3. NVIDIA's and AMD's
binaries keep their own licences; installing NVIDIA's DLLs yourself gives you only the rights
NVIDIA grants. Details: [Upscaler licensing](UPSCALER_THIRD_PARTY_LICENSING.md)
and [NVIDIA DLL license scope](NVIDIA_DLL_LICENSE_SCOPE.md). DLSS and FSR 3 still need testing
on more graphics cards; see [Testing DLSS and FSR 3](UPSCALING_TESTS.md).

### Apple MetalFX

MetalFX is part of macOS: nothing to install. It scales up as far as the Mac's chip allows,
twice a side on most, so *Ultra performance* may be drawn at that size (`game.log` says so).
Glass and rain are passed to it as a mask on macOS 14.4 and newer. It is not offered on an Intel
Mac or on macOS 11 and 12.

### Frame generation

**Frame generation** (`frame_generation=metalfx`; macOS 26 or newer, with MetalFX upscaling on)
shows a generated frame between every two drawn ones, so 45 drawn frames a second show as 90.

- Motion looks smoother. The controls answer a little later (half a frame) and no faster than
  the drawn frames.
- Generated frames cost the chip time too: about 5 ms per million pixels on an M2. At the Mac's
  full resolution it can show fewer frames than without it.
- With V-sync on, the game draws half the display's rate.
- The HUD, menus and pause screen are not interpolated. Screenshots show drawn frames only.
- After 15 s, when two readings in a row find fewer than 30 drawn frames a second, the game turns
  it off for this drive and says so. Switching it off and on in the menu tries again.
- With the frame rate shown, the corner reads e.g. `90 fps (45 + FG)`.

It is switched in the game's menu without a restart.

## Graphics API

**Settings → Graphics → Graphics API** (`graphics_api`, not on a Mac) names the interface the
game asks for first. The others follow, so a machine without it still starts.

| Platform | Automatic tries |
| --- | --- |
| Windows | DirectX 12, Vulkan, OpenGL, ANGLE (Vulkan first with a discrete AMD Radeon) |
| Linux, Android | Vulkan, then OpenGL |
| Mac, iPhone, iPad | always Metal |

After a lost device on Vulkan, *Automatic* keeps to DirectX 12 on that machine until the next
version, or until `~/.openomsi/vulkan-lost` is deleted.

**ANGLE (DirectX 11)** (Windows) runs OpenGL ES through Google's ANGLE on the card's DirectX 11
driver. It is for chips whose DirectX 12, Vulkan and OpenGL drivers are missing or broken (Intel
HD Graphics 2000–4000, AMD Radeon HD 5000/6000). It is tried last, and only when `libEGL.dll` and
`libGLESv2.dll` are beside `openomsi.exe`. The Windows x64 package ships them (there is no ARM64
build). With ANGLE the log names the adapter `ANGLE (…Direct3D11…)`.

### mX Graphics

**mX Graphics (Experimental)** (`mx_graphics`, off by default, from the next start) runs the whole
game on [mX](https://github.com/openOMSI-org/mX), openOMSI's own graphics library, instead of
wgpu. With it on, the graphics API also offers DirectX 11 on Windows, Macs without Metal 3 run on
mX's Metal 2 path, and the upscalers and frame generation are mX's (Intel XeSS among them where its
DLLs are beside the game). mX has not yet run the game on Windows, Linux or Android graphics
cards. If the game does not start with it, switch it off in the launcher.

## Memory and distances

- **Video memory** (`video_memory`): all the game keeps on the graphics card (textures, models,
  the picture). *Automatic* is nine tenths of a discrete card's memory, never more than the
  driver's budget. Big maps stay within it through compressed textures and tiles that free
  everything when they unload. OMSI's `texmemlimit` is not read.
- **Compress textures on loading**: loose pictures compressed to BC1–BC3.
- **DXT/BC textures stay compressed on the GPU**: off decodes them to RGBA, for a driver that
  mishandles block formats, at four to eight times the memory.
- **View distance**: how far round the camera tiles stay loaded (1200 m by default).
- **Object distance** and **Small objects**: how far and how small objects are still drawn.

## Mirrors

**Mirrors** (`mirror_size`) sets the resolution of the bus's mirror pictures (0 turns them off).
**Bus mirror refresh** (`mirror_refresh`) sets how often they are drawn: *None* (a frozen
picture), *Economical*, *Full* or *Every frame*. **Mirror quality** *As the picture* draws the
mirrors of an Enhanced picture with its light, sky and sun shadows; *Standard* uses plain light,
which costs a weak card less. **Shadows in the mirrors** and **Smoke and spray in the mirrors**
add those (both off).

### Mirror panels

Copies of the bus's mirrors can be laid over the picture, so the street behind is in view without
looking at the glass. A panel shows its mirror as the glass in the model does (turned over, or on
its side).

1. In the cab, press **Ctrl+M** to show or hide the panels.
2. Press **Ctrl+Shift+M** to start the editor. Each panel gets a yellow frame.
3. Arrange the panels with the keys below.
4. Press **Esc** (or **Ctrl+Shift+M** again) to end the editor and keep the layout.

| In the editor | What it does |
| --- | --- |
| left drag | move a panel |
| wheel / `Shift+wheel` | taller or shorter / wider or narrower |
| `[` / `]` | narrower / wider (the panel under the cursor) |
| `;` / `'` | shorter / taller |
| arrows | turn that panel's mirror |
| `Alt+arrows`, `Page Up` / `Page Down` | shift the mirror across and up, forward and back |
| `-` / `+` | narrow / widen its field of view |
| `R` / `Shift+R` | put this mirror / every mirror back as the bus has it |
| `Insert` | add a panel (main side mirrors first) |
| `Delete` | remove the panel under the cursor |
| `C` | show another mirror in it |

Outside the editor the panels are only pictures; mouse and keys work as always. The layout is
kept per bus in `~/.openomsi/mirror_hud.cfg`; mirror turns and fields of view per bus in
`mirrors.cfg`. **Settings → Camera → Mirror panels on the screen** gives a bus without a layout
its first panels.

The panels need the mirrors on (`mirror_size` not 0) and are redrawn at the *Bus mirror refresh*
rate. **Mirror panels every frame** (`mirror_hud_every_frame`) draws the panels' mirrors every
frame instead, at the cost of one mirror picture a frame per panel. Panels are not drawn in VR.

## Triple screen and VR

Three monitors as one wide view: [Triple screen](triple-screen.md). OpenXR headsets on
Windows: [Virtual reality](VR.md). The settings for both are in
[Launcher and settings](LAUNCHER.md#outside-views-head-tracking-triple-screen-vr).
