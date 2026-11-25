# History

This is the record of how openOMSI was built, round by round, from the first vehicle to release
0.1.8 (September 2026). Each entry says what landed. Later releases are in `CHANGELOG.md` in the
repository; the current design is in [Architecture](ARCHITECTURE.md).

## Roadmap

The original plan, in five steps. All are done; later work is listed under the rounds below.

| Step | Scope | State |
| --- | --- | --- |
| 1. Player vehicle | Script-driven model animations, cameras, input, sound | Done |
| 2. AI traffic and the world | `[path]` networks, AI cars, lights, timetable buses, collisions, passengers, physics | Done |
| 3. Day and night | Sun from date and time, `envir.cfg` light colours, night maps, `[maplight]`, coronas, weather, seasons, reflections, shadows | Done |
| 4. Cockpit | HOF and IBIS, text and script textures, coupled vehicles, humans, ticket selling, the player's timetable | Done |
| 5. Interface | Start menu and launcher, situations and chrono events, plugin API, Lua plugins | Done |

## Before the rounds

The first passes built the game up, roughly in this order:

- **AI traffic**: lanes from splines and objects linked by proximity; kinematic AI cars from
  `ailists.cfg` with car following, speed limits, blinkers and `{frame_ai}` scripts (`--traffic N`).
  Then light programs and lamp objects, CTC paint schemes, timetable buses with stops
  (`--schedule --time`), lane changes, turn lanes, AI buses pulling into bays, parked cars,
  aircraft on flight paths, AI collisions and yielding to the player.
- **Collisions**: oriented obstacle boxes. As in `Omsi.exe`, only `[fixed]` and
  `[crashmode_pole]` objects are solid (a collision mesh if they have one, else their
  `[boundingbox]`), and none with `collision_objects=0`.
- **Passengers**: skinned `.hum` models posed procedurally, waiting at `[busstop]` objects,
  boarding through the open door into `[passpos]` seats, walking the cabin path network, queues,
  stop requests, ticket selling (`GivenTicket`, cash desk money), passengers on AI buses,
  pedestrians on the pavements.
- **Trains**: AI trains from `.zug` files on rail tracks.
- **World**: rigid-body physics, seasons, hourly densities, scenery sounds, `[worldcoordinates]`
  maps, chrono events, situations and `.osn` saves, water from `.map.water`, wet roads from
  `[moisture]`, `[crossing_heightdeformation]`, the painted ground (`[groundtex]` alpha masks).
- **Graphics**: `[matl_change]` variants, envmaps, light maps, LODs, precipitation, blob shadows,
  real-time mirrors, interior lights, clouds, terrain night maps, a sun shadow map (later two
  cascades), `[matl_noZwrite]`, angle-weighted glass reflections, `[spline_terrain_align]` and
  `[terrainhole]`.
- **Speed**: per-draw buffer writes merged into runs (9 000 single writes a frame had capped it at
  14 fps), fog culling, parallel culling.
- **Cockpit and driving**: `[newanim]` composed as the original does, `Wheel_Rotation` and
  `Axle_Steering` in radians with OMSI's sign, the switch under the cursor wins the click, the
  mouse wheel turns it, Shift+U puts the bus into service, WASD driving, the passenger side of the
  bus script (`PAX_Entry<i>_Req` and friends), depot services (`veh_tank`, `veh_wash`,
  `malfunction_*`).
- **Driver record**: the `.odr` personnel file with stops, punctuality, distance, crashes,
  takings and OMSI's three ratings (`--driver`, F9).
- **Tools**: a build stamp in the log and HUD; `OMSI_ROAD_PHOTO` photographs the road network
  from above and finds where ground shows instead of road (0 of about 400 points on both stock
  maps).

### The "problems" pass

A first user report brought: keyboard steering that swings back like a damped spring; a braked
bus that stays still (static friction); `Axle_Suspension_*` with the right sign; Z/X/C indicators;
the outside camera clipped against ground and obstacles; a hand cursor over switches; SSAO and
5×5 PCF shadows; weather that changes the light; `~/.openomsi/settings.cfg`; the first
navigator (N); `--export-glb`; session summaries in `~/.openomsi/sessions/`; the first Enhanced
path; and the Tauri launcher (profile, bus with 3D preview and liveries, depot, line, tour,
roadbook, time, date, weather, settings).

## The big round (Sept 2026)

One user report of about thirty problems, worked on parallel tracks and merged one at a time.

- **Content and mods**: `.zip` files laid out like OMSI 2 are mounted, not unpacked
  (`omsi_cfg::vfs`). Keyword lines, `-<DISABLED>-` blocks and file names are matched as OMSI
  matches them. The script VM follows the original where mods depend on it. Mod buses find their
  start-up and IBIS log-in in their own scripts. A mod map with missing content loads as far as it
  can and says what is missing.
- **Streaming**: tiles load and unload around the camera and the bus, matching a whole-map load.
  Everything a tile owns comes and goes with it. `OMSI_CHURN` checks recycled GPU slots.
- **Memory**: compressed textures, the fleet read ahead and trimmed, the texture budget, the
  allocator restart. Ahlheim V5 main station: 8.75 → 1.93 GB peak.
- **Vanilla picture**: meshes one-sided as in Direct3D, envmap and bump map as D3DX does them,
  `null.bmp`, `noZcheck`/`Zbias`, `[isshadow]` blobs. Spandau's frame rate doubled (instanced
  batches, render bundles, a render scale with upscaling).
- **Enhanced renderer**: now physically based (GGX, computed sky, spherical-harmonic sky light,
  contact-hardening shadows, physical light units, aerial perspective, auto exposure, PBR
  Neutral, FXAA). OMSI's envmap is a tint on the reflection, not a picture.
- **AI motion**: a bicycle model with pure pursuit, Ackermann steering, wheels and body on
  springs. Trains ride on their outer axles.
- **AI rules**: light programs with stock state codes, IDM following, right of way from `[rule]`
  priority, gap acceptance, deadlock breaking, passing standing vehicles on the oncoming lane.
  Spandau: mean AI speed 21.1 → 25.0 km/h, buses stuck at stops 51 → 12.
- **Physics**: rigid bodies with impulse collisions, wheels on `[heightprofile]`, kerb climbing,
  pusher articulated buses that drive.
- **People**: agents with queues, crowd avoidance, double-decker stairs, a procedural gait with IK
  legs, paying, holding on, sitting, balance. `[smoothskin]` bellows.
- **Timetable and cockpit**: the duty starts with the trip that fits the time, typed into whatever
  IBIS the bus has. The HUD names the switch under the cursor.
- **LAN play**: protocol 3 (bit-packed states, a host that owns the world, rate limits), session
  codes, chat on **V**.
- **Launcher**: mods install as background jobs; any number of games started and watched from
  the Sessions page. SIGTERM, SIGINT, SIGHUP and Cmd+Q end the game cleanly.
- **Left open**: parallel scenery scripts, LAN timetable sync, real window reflections, parked
  cars setting off. Three reports could not be reproduced.

## September 2026 rounds

### Mod bus compatibility (Sept 22)

Ten broken mod buses (LiAZ 5292, PAZ 32051, Procity, Lion's City A21, C2, Citaro Facelift,
Sprinter 412D/312D, Scania), each traced to a general cause read off OMSI:

- Script VM: division by zero gives 0; `random` as the exe does it.
- Sounds: triggered entries play once; `[noloop]` plays on the rising edge (the endless hiss);
  articulated rear sections play their own sound config.
- Code pages detected per file (UTF-8, 1251, 1250, 1252).
- Two installed versions of a pack no longer mix files.
- Protected v7 meshes, named `.x` objects, odd bitmaps and DDS formats load.
- `AutoClutch` defaults to 1; `Weather_Temperature` is known before `{init}`; Shift+U waits for
  the engine.
- `examples/bus_audit` loads and starts every vehicle (445: all buses start).

### Round 6 (Sept 23): shadows, online interface, traffic, clouds, rain

A close shadow cascade (3 cm texels); a Roblox-like chat, tooltips and name tags (settings
`chat`, `tooltips`, `name_tags`, `show_fps`, `clouds`); dormant cars that keep driving off-screen;
cloud types from `Weather/clouds.cfg` and volumetric cumulus in Enhanced; procedural raindrops on
windows; `discard` only in alpha-tested pipelines (main pass −8 %).

### Round 7 (Sept 23): routes

Chrono `[deactivate_lines]` read as a count and names; scenario dates inclusive; tour mask bits 8
and 9 the right way round (see [How routes work](ROUTES.md)). Glass without `[matl_alpha]` is opaque.
One seamless cloud field per weather. Faster shadows and culling hysteresis against popping.

### Round 8 (Sept 23): plugins, lamps, culling, scripts

- **Plugins**: `.opl` files and their DLLs driven as OMSI drives them; 32-bit DLLs in
  `omsi-plugin-host32.exe` (Wine off Windows). See [Writing plugins](PLUGINS.md).
- Saloon lamps as Direct3D point lights, every `[matl_lightmap]` kept.
- One held screen size per object decides culling for all its meshes (flicker gone).
- Night tours picked before midnight start at the right trip.
- `OMSI_DEBUG_NAN`, `OMSI_IBIS_BUDGET`, `OMSI_WATCH_VARS`.

### Round 9 (Sept 24): navigator, launcher, passengers

- **Navigator** rewritten after ETS2's Route Advisor: a tilted 3D map, traffic-coloured route,
  rerouting, street names read from street-sign objects. Shift+N.
- **`omsi-ui`**: Roboto, Material Symbols, an atlas and a shape painter.
- **Launcher** moved into the game's own window for a time (the showroom drew the bus with the
  game renderer). It later returned to Tauri.
- Passengers posed every frame within 45 m, smooth boarding and stepping.
- The LiAZ 5292's line matrix shows one-digit lines (`$SetLengthR` re-read in OMSI).

### Rounds 12-12b (Sept 25)

Sound `[volcurve] -1/-2`; font fallbacks; terrain cells split as OMSI splits them; wheels and feet
on `[surface]` objects; a driver figure at the wheel (setting `driver`); Nubis-style clouds;
drivers from `drivers.txt`. **Timetable buses rewritten as traffic** (`bus_service.rs`), with
riders aboard from the start. People keep out of walls.

### Round 13 (Sept 25)

`[texttexture]` centred as OMSI does. Enhanced clouds rebuilt in the Horizon Zero Dawn style
(Perlin-Worley noise, a 1024² sky cube). AI bodies stay on the path. The driver's fingers curl
round the rim.

### Rounds 14-14b (Sept 25)

- Situations load in the right place and orientation; saving writes weather, timetable and duty.
- Timetable runs past midnight; `car_use/*.ocu` applied.
- Ticket packs, ticket choice by age and time, passenger voices; validators (`[stamper]`).
- `[lht]` left-hand traffic.
- Audio: no gap at loop ends, at most 200 voices.
- Non-finite numbers read as 0, a panic hook. Saloon comfort complaints. A bus driven off the map
  falls.

### Rounds 15-15b (Sept 25)

- F5-F8 are the destination keys again; services moved to the game menu; F4 is the map camera.
- Sound: `[loopsound]` volume, `[next_random]`, outside sound through open doors, reverb in
  underpasses.
- Sun from `timezone.txt`; grip from `StreetCond` and temperature (black ice).
- Nothing is written into the original install.
- `[smoke]` and `[particle_emitter]` particles; coronas, fog cones and `[light_enh_2]` bitmaps as
  OMSI draws them; `[LightMapMapping]`.
- Railways: trains throw switches, signals, coupling.
- Options: maintenance, traffic shares, real clock, collision switches, head movement.
- Placing any vehicle from the game menu, coupling by hand; parked cars pull out.
- LAN protocol 5: tours and riders shared.
- The object editor: move, turn and delete objects, saved as tile copies in the content folder.

### Round 16 (Sept 25)

The launcher opens without OMSI 2 and finds it in more places (Steam libraries, Wine bottles). No
fog inside the bus. The driver drawn in the mirrors. Passenger voices rate-limited as OMSI does.
AI speed planning and parking fixes (`OMSI_TRACE_AI`, `OMSI_DEBUG_CAR`). LAN mods sent to joining
players. Spandau centre 109 → 156 fps (vanilla), 58 → 76 fps (Enhanced).

### Rounds 17-17d (Sept 26)

Glass in fog no longer milky; Urbino headlamps drawn as wound; each wheel has its own mass
(`OMSI_SUSP_TRACE`). Enhanced speed-ups: Spandau cab 45.7 → 66.4 fps. The driver's hands:
built from the forearm, hand-over-hand steering, smoother regrips. The first-person driver
figure was removed again: in the cab view the driver is only in the mirrors, as in OMSI.

> **Tip:** The offscreen `OMSI_BENCH` exaggerates GPU times. Measure in a window with
> `OMSI_GPU_TIMERS=1 OMSI_PROFILE=1`.

### Round 19 (Sept 26)

Rain: texture `.cfg` lookup, `[moisture]` on junctions, puddles spreading as the road soaks.
Traffic fixes (`OMSI_CHECK_OVERLAP`, `OMSI_DEBUG_STUCK`). NaN guards for Vulkan and Direct3D.
Walking inside the bus; a running gait. **The dedicated server** (`openomsi --server server.cfg`)
with WebSocket transport, `/status`, Cloudflare tunnels, and the launcher's Multiplayer page.

### Round 20 (Sept 26)

Multiplayer passengers: entering another player's bus, seats, switches worked through the owner,
smoother remote buses. Server administration (`/admin <password>`, `admin_password`,
`time_speed`, `vehicles`). Offline machine translation (CTranslate2 and NLLB-200). Frame limit at
the screen's refresh rate.

### Round 21 (Sept 27): the audit

- **Security**: signed relay posts, challenge-response admin login, capped connections, pinned
  and checked downloads, downloaded plugins disabled, safe file handling.
- **Crashes and hangs**: the tile loader survives panics; damaged files refused early.
- **As OMSI does it**: cant, `GetTime`, `$IntToStrEnh`, string textures, driver ratings, wheels
  that slip and lock (ABS works), `laststn.osn` with Continue, and more.
- **Simulation**: long frames sliced, articulated sections pitch and lean, gridlock broken.
- **Game**: Options, Line and tour, Driver and Fleet number in the game menu; Ctrl+click on the
  city map places the bus; OMSI 2's `options.cfg` imported on first start.

### Round 22 (Sept 27)

- The date followed at midnight (chrono and season change).
- **Rail vehicles** bound to the track, throwing points, with a plain traction where scripts
  give none (`rail_drive.rs`).
- The launcher's Game controllers tab edits `gamectrler.cfg`.
- The object editor copies objects and shapes the ground; the launcher's Timetable page edits
  lines and saves `.ttl`.
- Cube-map DDS, real reflections (setting `real_reflections`), parallel scenery scripts.

### Round 23 (Sept 27)

Fog cones and headlights on light-mapped roads; mirror brightness at night; no snow or rain under
roofs; walking through doors (`Transit`); many LAN fixes (remote headlights, interpolation, UPnP
cleanup); stuck timetable buses leave; money as notes and coins; key releases go where the press
went; launcher fixes (METAR airport, Reset timetable).

### Round 24 (Sept 27)

- Esc → Remove this vehicle; start on foot (`--on-foot`); G takes a placed vehicle's wheel;
  placing follows the mouse.
- Vans: oversized bounding boxes trimmed, the driver uses the cab door.
- Shift+U off stops the engine first.
- The pause menu scrolls; new admin actions.
- The object editor works with the mouse and syncs over LAN.
- Automatic texture budget; PBR texture maps by suffix (`_nn`, `_rr`, `_mm`, `_aa`, `_orm` and
  the long names; see [PBR materials](PBR.md)).

### Round 25 (Sept 27): Android

The game became a library (`openomsi_game`), built for Android as `libopenomsi_game.so`. One
process runs the launcher and the game in turn. The launcher lays itself out for fingers; the
game has on-screen controls (`touch.rs`). See [Android](ANDROID.md).

## Releases 0.1.5-0.1.8

### 0.1.5 (Sept 27): Lua plugins

`plugins/*.lua` and `plugins/<name>/main.lua` in sandboxed Lua 5.4 states, with the `omsi` table,
saved data, hot reload, a time budget per call and switch-off after 10 errors. See
[Plugin API reference](PLUGIN_API.md).

### 0.1.7 (Sept 28): the bus as its file makes it

Behaviour reverse engineered from `Omsi.exe` replaced our guesses:

- **Driving physics**: no tyre slip angle; the bus follows its steering geometry until the bend
  asks more than the tyres' grip, then each axle slides. Every axle steers from
  `[rot_pnt_long]` and `[inv_min_turnradius]`. Springs and dampers as OMSI computes them.
  `cargo run --release -p omsi-sim --example handling -- <file.bus>` prints a file's handling.
- **Mouse and keyboard steering** and pedals as OMSI does them.
- **Graphics device**: Vulkan, then DirectX 12, then OpenGL (`graphics_api`, `OMSI_BACKEND`);
  limits for weak cards; validation errors logged, not fatal.
- **Mirrors** in the body's frame, redrawn only when in view.
- **Screens**: FXAA no longer halves the contrast of IBIS letters.
- **Trees**: `[tree]` billboards the right width.
- **Updates**: the updater (GitHub releases, SHA-256, safe swap and undo); Android through
  PackageInstaller.
- **Windy trees** (`windy_trees`), **window condensation** (a cabin air model), snow on glass
  laid out in metres.
- **Enhanced**: clear-coat paint, better night adaptation, envmap strength matching vanilla.
- **Running for the bus**: people run for an arriving bus, and timetable buses wait for them up
  to 12 s (`OMSI_RUNNER_CHANCE`, `OMSI_DEBUG_PAX`).

### 0.1.8 (Sept 28): controls, controllers, multiplayer, phones, builds

- **Keys**: a changed binding switches to Custom controls; Space is OMSI's view reset; a look
  direction and zoom kept per view.
- **Mouse control** as in OMSI; `mouse_sens` scales it.
- **Game controllers**: gilrs plus DirectInput 8 on Windows, stable button numbers, force feedback
  (centring, scrub, road feel, script vibration, `[FFScale]`), a set-up assistant and a
  force-direction test (`[openOMSI.FFInvert]` per device).
- **Multiplayer** that lasts: UPnP renewed, the relay polled gently, cloudflared restarted.
- Depot files follow the chosen date; install detection accepts more paths.
- Phones: launcher scaling and a scrollable game menu.
- Sound: DirectSound's distance law; only foreign vehicles muffled.
- **Builds**: `release.yml` builds Windows x64/ARM64, macOS arm64/x86_64, Linux x64/ARM64 and
  Android, and packs the server for Linux and Windows.

## Details

Facts from the rounds that are useful for debugging but not covered elsewhere:

- `OMSI_INPUT` drives the game from a script (for example `orbit <m>`, `touch` commands);
  `OMSI_TOUCH=1` and `OMSI_MOBILE=1` check the phone controls and launcher on a computer.
- `OMSI_ROAD_PHOTO_SLANT=<m>` photographs the road from a driver's eye height.
- `OMSI_DRIVER_HANDS=<l>,<r>` holds the driver's hands at fixed angles; `OMSI_DEBUG_DRIVER` logs
  their frames.
- `OMSI_CONDENSATION=<min>,<people>[,engine]` and `OMSI_CABIN_LOG=1` check the window
  condensation offscreen.
- `OMSI_RENDER_CLOCK=<s>` starts the animation clock later for offscreen stills.
- `OMSI_AI_RIDERS`, `OMSI_DEBUG_WALLS`, `OMSI_CHECK_WALLS`, `OMSI_TRACE_PAX=<csv>` trace people.
- `openOMSI/env.txt` gives a phone `OMSI_*` switches.
- `crates/omsi-o3d/examples/{bounds,matinfo,panels,sides,turned}` inspect mesh files.
- Known content problems, not openOMSI bugs: Novi Sad's `.ttr` tracks jump between tiles, a
  missing `IK218N` script, missing fonts.
- Deliberately not done: `Snd_Microphone` (the exe toggles the OS microphone line). `relrange`
  has no stock use and its meaning is unclear.

All flags are listed in [Environment variables](DEBUG_FLAGS.md).
