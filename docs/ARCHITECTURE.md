# Architecture

This page maps the openOMSI code for contributors: which crate does what, how a frame runs, and
how content files become a picture. It then covers the deeper topics: memory, coordinate frames,
the layering rules, the start guard and the known debt.

openOMSI follows the unit structure of the original Delphi program, so every subsystem has an
obvious counterpart. It replaces the original's architecture where that was limited: 64-bit,
loading and tessellation on a worker pool, a modern renderer, and (as the aim) no global mutable
state. How a behaviour-preserving change is checked is in [Refactor checks](REFACTOR_CHECKS.md);
the history is in [History](HISTORY.md).

## Crates

The workspace is layered. `scripts/arch-guard.sh` keeps each crate off the layers above it (rules
in `scripts/arch-rules.txt`, see [Target layering](ARCHITECTURE.md#target-layering)). Bottom up:

### Base

| Crate | What it does |
| --- | --- |
| `omsi-cfg` | OMSI's `[keyword]` text files and code pages; the virtual file system (content roots, mounted `.zip` archives, `omsi_cfg::vfs`); the `OMSI_*` flag registry (`omsi_cfg::flags`); finding the OMSI 2 install; the panic guard |
| `omsi-pack` | Signed and optionally encrypted publisher packs (no game dependencies; see [Publisher packs](PUBLISHER_PACKS.md)) |
| `omsi-ui` | Interface drawing: Inter text, Material Symbols icons, anti-aliased shapes, and the immediate-mode widget kit (`omsi_ui::kit`, `theme`) for the pause menu, photo mode and developer tools |
| `omsi-settings-ui` | Grouping and filtering for settings interfaces |
| `omsi-trolley` | Trolleybus overhead wire: network, sections and switches, pole mechanics, rheostat and asynchronous traction, arcs. Pure state and physics ([Trolleybuses](TROLLEYBUS.md)) |

### Formats and content

Files in, plain data out.

| Crate | What it does |
| --- | --- |
| `omsi-o3d` | `.o3d` meshes (v4/v5 unscrambling) and DirectX `.x` meshes |
| `omsi-texture` | Texture loading (bmp, tga, dds, jpg, png) with OMSI's lookup rules and `.cfg` sidecars; seasonal and `_LOW` variants; BC1/BC3 encoding (`bc`); PBR maps (`pbr`) |
| `omsi-model` | `model.cfg`: meshes, materials, animations, lights, text textures |
| `omsi-scenery` | Scenery objects (`.sco`) and splines (`.sli`) |
| `omsi-map` | `global.cfg`, tiles, terrain, calendar, AI lists, chrono |
| `omsi-vehicle` | `.bus`, `.ovh`, `.zug`, HOF files, passenger cabins, paths, sound configs |
| `omsi-timetable` | TTData (stops, trips, tracks, lines) and `car_use` files |
| `omsi-transit` | A map's transport folder: stations, routes, overhead wire, third rail, switches, depots, signals ([Transport folder](TRANSPORT_FORMATS.md)) |
| `omsi-content` | The smaller formats: weather, environment, fonts, languages, money, tickets, humans, drivers, situations, options, input maps |
| `omsi-script` | Compiler and stack VM for `.osc` scripts |
| `omsi-geometry` | Spline curves and extrusion, terrain meshes, object placement, the terrain cut under flush surfaces and `[terrainhole]` |
| `omsi-world` | The map-wide index of splines and objects; where attached objects and spline rows go |
| `omsi-mapdraw` | The map's own picture (ground, water, woods, paving, tracks, trees, houses) and the navigator's roads, route and street names, as triangles |

### Simulation and models beside it

| Crate | What it does |
| --- | --- |
| `omsi-sim` | The simulation: vehicles (`vehicle/` by subsystem), the scripts' host (`host/`), AI traffic, people, timetable runtime, physics, rail, the fixed step. No renderer, audio, texture decoder or network: pictures and HTML pages come through hooks the game registers (`image_decode`, `htmltex::set_backend`) |
| `omsi-weather` | The weather as state and physics: weather model and cycle, season phases, road wetness, cabin air, drops on the glass |
| `omsi-career` | The driver's personnel file (ratings, stops, tickets, crashes) and real-time mode |
| `omsi-htmlengine` | The small built-in `[htmltexture]` engine (HTML/CSS, block layout) ([HTML textures](HTMLTEXTURE.md)) |
| `omsi-htmlpage` | The full `[htmltexture]` engine: HTML, CSS with flexbox, JavaScript in Boa, `window.omsi` ([HTML textures: modern engine](HTMLTEXTURE_MODERN.md)) |

### Presentation

| Crate | What it does |
| --- | --- |
| `omsi-render` | The renderer (wgpu): `gpu/`, `resources/`, `scene/`, `lighting/`, `frame_plan`, shaders, upscaling (`temporal/`), ray tracing (`rt`) |
| `omsi-scene` | Loaded content turned into renderer data: tiles and tile streaming (`tiles`), vehicles, materials, lights, the trolley view; holds `World` |
| `omsi-audio` | The mixer (cpal), WAV, the OMSI sound-config runtime, 3D voices |
| `omsi-soundscape` | Ambience recordings and rules, scenery sounds, the bus radio |
| `omsi-photo` | Photo mode: the lens (depth of field, motion samples) and the darkroom |
| `omsi-navigator` | The route map and the full-screen city map, drawn with `omsi-ui` into a renderer texture |
| `omsi-plugin-ui` | The Lua plugins' panels and notifications |
| `omsi-multiplayer-ui` | The multiplayer player, world and device-ban panels |
| `omsi-mt` | Offline machine translation of untranslated interface text (NLLB-200 through CTranslate2) |

### Devices and services

| Crate | What it does |
| --- | --- |
| `omsi-input` | Game controllers and mappings, force feedback, head tracking, key names |
| `omsi-xr` | PC VR: the OpenXR session (Windows) and the cockpit navigator's placement ([Virtual reality](VR.md)) |
| `omsi-net` | The network transport: UDP session, bit-packed vehicle states, chat, the traffic table, the voice relay |
| `omsi-lan` | Multiplayer on top of `omsi-net`: the game-side session (`omsi_lan::game`), host mods for joining players, the public gateway, sync tables ([Playing together](MULTIPLAYER.md)) |
| `omsi-voice` | Voice chat: microphone, Opus, jitter buffer, radio and distance effects ([Voice chat](VOICE.md)) |
| `omsi-serverlist` | The public server list: moderation, registration and heartbeat, the launcher's view |
| `omsi-admin` | A dedicated server's administration: rights, admin sessions, operations, the audit log ([Dedicated server](SERVER.md)) |
| `omsi-remote` | The remote cockpit: a page on a phone or tablet that shows and works the bus's switches |
| `omsi-platform` | Updates and recovery, Discord and presence, the version stamp, quitting on signals (`quit`) |
| `omsi-guard` | The start guard (see [Start guard](ARCHITECTURE.md#start-guard)) |
| `omsi-plugin` | OMSI plugins (`.opl` and their DLLs, the 32-bit host) and Lua plugins ([Writing plugins](PLUGINS.md)) |

### Launcher

| Crate | What it does |
| --- | --- |
| `omsi-launcher-core` | The launcher's data side: maps, buses, lines, profiles, settings, mod installs, running games; the `openomsi-launcher` terminal program |
| `omsi-launcher-api` | The page's commands, JSON in and JSON or pictures out (desktop launcher and Android's web view) |
| `omsi-launcher-model` | The window-free model the Swift launcher uses; asks the game through `omsi_launcher_model::host::Host` |

The launcher window itself is `launcher/` (Tauri, a workspace of its own). The Swift app is
`apple/`.

### App and tools

| Crate | What it does |
| --- | --- |
| `omsi-app` | The glue: `App` and its field groups, the frame stages, view sync, the adapters that hold the crates' state, startup, the offscreen run. Also a library (`openomsi_game`) for Android and Apple |
| `omsi-srcscan` | A source scan used by tests that check stage declarations against code |
| `tools/omsi-check` | Loads every content file of an install and reports failures |
| `crates/omsi-plugin/demo` | A plugin with OMSI's plugin interface, for `omsi-plugin`'s tests |

### The `-mx` crates

Every crate that draws has a second copy on mX, openOMSI's own graphics library: `omsi-app-mx`,
`omsi-render-mx`, `omsi-scene-mx`, `omsi-ui-mx`, `omsi-navigator-mx`, `omsi-mapdraw-mx`,
`omsi-photo-mx`, `omsi-plugin-ui-mx`, `omsi-multiplayer-ui-mx`, `omsi-settings-ui-mx`,
`omsi-mt-mx` and `omsi-xr-mx`. The **mX Graphics** setting (`mx_graphics`, or `OMSI_MX` for one
run) picks one stack at start: `omsi_app::run` hands over to `openomsi_game_mx::run`, and the
other stack is never started. A change to a drawing crate must go into both copies. Status and
plan: [Porting the renderer to mX](MX_PORT.md).

## Mapping to the original

| Original unit | Crate |
| --- | --- |
| File readers (`TTextfile`) | `omsi-cfg` |
| `mc_exprcalc` (scripts) | `omsi-script` (all 245 stock script sets compile) |
| `mc_o3dfiles`, `.x` meshes | `omsi-o3d` |
| `mc_complobj`, `mc_MatlMan` | `omsi-model` |
| `mc_complMapObj`, `mc_splines`, `mc_path` | `omsi-scenery` |
| `mc_mapclass`, `mc_terrain_2`, `mc_chrono`, `mc_fahrstrasse` | `omsi-map` |
| `mc_roadvehicle`, `mc_vehicle`, `mc_train`, `mc_passcabin`, `mc_sound` | `omsi-vehicle` |
| `mc_timetable`, `mc_station` | `omsi-timetable`; runtime in `omsi_sim::timetable_run` |
| `mc_weather`, `mc_himmel`, `mc_font`, `mc_language`, `mc_money`, `mc_human`, `mc_driver`, `mc_situation`, `mc_input`, options | `omsi-content` |
| `mc_texMan` | `omsi-texture` |
| `mc_sound` runtime, DirectSound | `omsi-audio` |
| `mc_font` text textures | `omsi_sim::texttex` |
| Tessellation, spline geometry | `omsi-geometry` |
| `mc_d3d_classes` (Direct3D 9) | `omsi-render` |
| `mc_Form_Main`, main loop | `omsi-app` |
| Vehicle runtime (`TRoadVehicleInst`) | `omsi-sim` |
| AI (`mc_path`, `mc_pathrule`), humans, physics (ODE) | `omsi-sim`: `ai_traffic`, `ai_motion`, `human`, `crowd`, `rigid`, `physics` |
| Options and the start dialogs | `launcher/` and the `omsi-launcher-*` crates |
| No counterpart (OMSI 2 uses a plugin) | `omsi-trolley` |
| No counterpart | `omsi-net`, `omsi-lan` (multiplayer) |

The vanilla picture reproduces Direct3D's: one-sided meshes, envmap with mask, bump map,
`noZcheck`/`Zbias`, dynamic slots, night maps, reflections, mirrors, shadow cascades. The
Enhanced renderer is a physically based renderer of its own ([Graphics](GRAPHICS.md)).

## The frame loop

The simulation runs on a **fixed step**: `omsi_sim::tick::RATE_HZ` is 60 ticks a second, always.
A frame runs at most 4 ticks, so the simulation keeps wall-clock pace down to 15 frames a second.

The window's frame is three declared lists of stages (`app_events/frame/stages.rs`, run by
`ticked.rs`):

1. `FRAME_BEFORE`, once a frame: housekeeping and the player's input.
2. `FRAME_TICK`, once per tick: traffic, the player's bus and its scripts, people, the duty, the
   clock and the weather. What only the picture and sound need (view syncs, the AI's sounds) runs
   in the frame's last tick alone (`SimTicks::view_due`).
3. `FRAME_AFTER`, once a frame: the view, sounds, multiplayer, the interface and the picture.

The player's bus and the camera are drawn between the last two ticks. The offscreen run's step
is one tick (`offscreen::OFFSCREEN_STEP`), so `--offscreen` exercises the same stages as the game.

Two things still run per frame, not per tick: the tile streaming's handover and the population's
camera. So the AI traffic is not yet identical at every frame rate; the player's bus is.

### Stages

One runner (`omsi-app/src/frame_stages.rs`) runs both the window's list
(`app_events::WINDOW_FRAME`) and the offscreen list. Each stage names the field groups of `App` it
reads and writes and the values it hands on. Tests pin both orders and the three places where the
two paths knowingly differ. `OMSI_FRAME_ORDER=1` logs them.

`App`'s state is grouped by subsystem (`omsi-app/src/app/groups.rs`). `App::parts`
(`app/parts.rs`) splits `App` into one borrow per group and names every field of `App` and
`SessionState`; a new field does not build until it has a group.

### The simulation thread

With the `sim_thread` setting on (off by default, read once at start; `OMSI_SIM_THREAD=0`/`1`
overrides it), part of the simulation runs on a thread of its own while the main thread draws and
presents:

- The main thread keeps the window, the input and the surface.
- The job runs the AI traffic's steps (`TrafficSim::advance`, one per tick), then the scenery
  objects' scripts, then the people's steps.
- The traffic stage finishes the rest of the traffic from what the last step left. The stepped
  traffic is back before the frame is counted.
- The scripted stage applies the commands of the step made during the last picture and hands the
  next step its inputs owned (`OwnedSceneryInputs`).
- The people read the player's bus from a copy (`omsi_sim::people::PlayerBusView`). The humans
  stage applies the last step's result (`steps::humans_after_step`).
- The timetable's step stays in its stage (its fleet uploads sit between its two halves).

The AI cars are then one frame behind the rest. The player's vehicle and input stay in their
stage, so there is no added input latency. The offscreen run never reads the switch.

## Threads

| Work | Where it runs |
| --- | --- |
| Tile loading (`.map` parse, terrain, spline tessellation, object types) | rayon pool; GPU upload on the main thread |
| Tile streaming (`omsi_scene::tiles::Streamer`) | Its own thread: picks the tiles around the camera and the player's bus and stages them on rayon |
| Texture decoding and BC1/BC3 compression | Worker threads through `TextureCache` |
| Timetable fleet vehicle sets (meshes, textures) | Made on the GPU by the worker; the drawing thread only adds them to the scene |
| People's posing and skinning | Parallel (`par_iter_mut`) over those due this frame |
| AI vehicle scripts | Parallel, after the sequential lane and obstacle planning |
| Scenery object scripts | Parallel (`frame_each`) |
| Publisher pack checks | rayon, one asset per task, one 64 KiB chunk at a time (`VerifiedPack::verify_all`) |
| Culling, render bundles, command buffer finishing | Parallel / helper threads |
| Enhanced sky | A helper thread; taken in when ready |
| Audio mixing | The audio device's callback; the cpal stream on the "audio device" thread |
| LAN socket | Its own thread in `omsi-net` |
| Quitting on signals (`omsi_platform::quit`) | Signal handler sets an atomic; a watcher thread wakes the event loop |

Details worth knowing:

- **Streaming** places and uploads only a little of one tile per frame, and unloads within the
  frame budget, so streaming never shows as a stutter.
- **Object and spline types** are shared through `Arc` caches guarded by mutexes.
- **Lanes**: a 50 m grid (`Network::grid`) answers nearest-lane queries without scanning the
  network.
- **Renderer updates**: only the per-draw entries of instances that changed are rewritten
  (`Scene::changed`). A full rebuild happens only when instances are added or the render origin
  moves.
- **Audio**: `AudioEngine` is `Send + Sync`. The game hands the mixer commands (play,
  parameters, stop, radio effects) through a lock-free single-producer ring (`omsi_audio::spsc`)
  and gets ended voices back through another. The callback takes no lock and allocates nothing.
  The internet radio's frames come through a ring too (`stream::StreamBuf`).
- **LAN**: the game's side is its own stage in `OMSI_PROFILE` ("lan").

### Scenery scripts

The scripts and variables of the loaded tiles' scripted objects are one owned list in the
simulation (`omsi_sim::scenery::SceneryState`, held by the session). Their view half (render
instances, materials, texts, sounds) stays in `World` (`omsi_scene::ScriptedSet`) under the same
key.

- The tile loader hands over placed objects, removed tiles and particle emitter owners through a
  `SceneryInbox`. The list takes it at the start of its step.
- Only objects near the camera's 50 m cell are candidates; the exact 800 m cut picks among them.
- Scripts run side by side on inputs gathered before (light phases, boards, clock).
- The step ends in commands in object order (`SceneryCmd`) that carry values. `World::apply_scripted`
  applies them without reading the simulation's list.

## From content files to the screen

1. **Files.** Every loader reads through `omsi_cfg::vfs`. Content roots and mounted `.zip`
   archives look the same, and file names resolve as Windows resolves them.
2. **Parsing.** The format crates turn files into plain data: the map (`omsi-map`), its objects
   and splines (`omsi-scenery`), models (`omsi-model`, `omsi-o3d`), vehicles (`omsi-vehicle`),
   timetables, scripts (compiled by `omsi-script`).
3. **The map index.** `omsi-world` indexes the whole map's splines and objects.
4. **Tiles.** The streamer loads tiles around the camera and the player's bus. On the rayon pool
   each tile is parsed, its terrain and splines tessellated (`omsi-geometry`) and its object
   types loaded. Textures decode and compress on workers.
5. **The world the simulation reads.** Tile ground, stop lists, obstacles and map facts are
   published as an immutable snapshot (`omsi_sim::world_data::WorldData`). Lanes and light
   programs go to the traffic through `omsi_sim::handover::Inbox`.
6. **Simulation.** Each tick, `omsi-sim` advances vehicles, scripts, traffic, people and the
   timetable.
7. **View sync.** `omsi-app/src/view_sync/` turns simulation state into renderer instances
   (positions, animation matrices, materials, script textures, lights).
8. **Scene and render.** `omsi-scene` holds the renderer-side world. `omsi-render` culls, records
   and draws, then the interface (`omsi-ui`) is drawn over it and the frame is presented.

## Memory

On a Mac the CPU and the GPU share memory, so both count against one budget.

### Textures

- **Kept compressed on the GPU.** DXT files go up as blocks with their mip chains. Other pictures
  are compressed on the loader threads (`omsi_texture::bc`, about 1 µs a block) when the result
  stays close. See [Content formats](FORMATS.md).
- **Late upgrades.** Pictures read on the drawing thread go up as RGBA and are swapped for their
  compressed copy by `World::apply_texture_upgrades`.
- **The texture budget** (OMSI's `[texmemlimit]`): an eighth of the machine's memory
  (`OMSI_TEXTURE_MEMORY` for tests). Once a second, while over it, scenery textures used only by
  tiles beyond 150 m lose their finest mip level (down to 64 texels a side), farthest first. With
  a tenth of the budget free, those within 400 m are read again and swapped back.
  `OMSI_BUDGET_FROM=x,y[,MB]` checks the way back offscreen.
- **Video memory.** All GPU memory (textures, meshes, render targets, shadow maps, the driver's
  own) stays within the **Video memory** setting (`video_memory`, `OMSI_VIDEO_MEMORY`; default nine
  tenths of a discrete card's memory) and nine tenths of the driver's budget. The driver reports
  it on DirectX 12, Vulkan (`VK_EXT_memory_budget`) and Metal (`Renderer::gpu_memory`, asked once a
  second). On OpenGL it is worked out from target sizes.
- **Shared pictures.** `[matl_freetex]` pictures are shared by all vehicles showing them. An AI
  vehicle beyond 50 m shows its script textures as one texel of their mean colour, and gets them
  back within 40 m.

### Vehicles

- AI vehicle types load without their CPU meshes (`VehicleType::load_ai`).
- A tour keeps one vehicle all day, so the vehicles of the next 25 minutes of departures are
  known. They are read on workers and uploaded one at a time ahead of departure.
- A vehicle set nobody has drawn for 90 s and no coming departure wants leaves the GPU
  (`World::trim_vehicle_sets`).
- A removed AI vehicle returns its instances, textures and materials to the free lists
  (`World::release_vehicle`).
- Materials made in one frame with the same textures and values share their GPU resources (a C2
  set: 965 materials, 60 ms before, about 4 ms now).

### Other

- **Scene slots** are handed out lowest first. After unloads, a free tail of the arrays is cut
  off (`World::compact_slots`).
- **Sound clips** nobody holds and nobody asked for in a minute leave the cache
  (`AudioEngine::trim_clips`).
- **Object positions**: the map index hands its positions to `World::object_positions` instead of
  keeping a second copy.
- **Surface rasters** are kept in 16×16-texel blocks, and finished tiles drop height layers that
  repeat the top surface (`TileSurface`): about 0.6 MB a tile instead of 5 MB.
- **macOS allocator.** The game restarts itself once at start with `MallocLargeCache=0` (exec,
  same pid; `OMSI_KEEP_ALLOCATOR=1` skips it), because macOS keeps freed large blocks dirty. The
  small-allocation zones return free pages on a thread of their own after tile changes and fleet
  trims.

### Measuring

`OMSI_PROFILE` logs GPU memory by kind and the CPU side every ten seconds. `OMSI_DEBUG_TEXTURES`
lists every vehicle texture and all textures by format and size. See
[Environment variables](DEBUG_FLAGS.md).

Results when this work landed (1280×720 window, 1x MSAA):

| Scene | Peak memory before | After |
| --- | --- | --- |
| Ahlheim V5 main station, traffic 30, passengers, timetable, 60 s | 8.75 GB | 1.93 GB |
| Flight over Ahlheim | 8.54 GB | 2.05 GB |
| Spandau with the EN92, traffic 30, passengers, timetable | 3.45 GB | 1.20 GB |

## Coordinate frames

| Frame | Axes |
| --- | --- |
| World | x east, y north, z up, metres; headings in degrees clockwise from north |
| `.cfg` files (cameras, positions, animations) | x right, y forward, z up |
| Mesh files (`.o3d`, `.x`) | Direct3D frame, converted on load (`omsi_geometry::mesh_from_o3d`) |
| Vehicle | The `.cfg` frame, origin at the model's z = 0 |
| Map splines / objects | Splines store x, height, y; objects store x, y, z |

- The vehicle origin is the plane the tyres touch with the springs **unloaded**, so a standing
  bus's origin is 10-16 cm above the road. `coll_pos_*` is in that frame, at bumper height.
- A DirectX `.x` `FrameTransformMatrix` is row-major for row vectors. Read column by column it is
  already glam's column-vector matrix (no transpose). Normals use the inverse transpose.
- `Wheel_Rotation_*` and `Axle_Steering_*` are radians, positive to the right.
- `articulation_<n>_alpha` is the joint's yaw in clockwise degrees (`beta` its pitch); `n` is the
  coupled part's position in the train.

See [Content formats](FORMATS.md) for the file formats.

## Target layering

New code follows this layering, and refactors move old code towards it:

1. **Parsers** (`omsi-cfg`, `omsi-script`, `omsi-o3d`, `omsi-model`, `omsi-scenery`, `omsi-map`,
   `omsi-vehicle`, `omsi-timetable`, `omsi-content`): files in, plain data out. No GPU, no clock,
   no global state. Each is testable on byte buffers.
2. **Simulation** (`omsi-sim`: vehicles, `ai_traffic` (`TrafficSim`), `people` (`PeopleSim`),
   `timetable_run` (`ScheduleSim`), physics, scripts): advances the world by a time step. It knows
   nothing of the GPU or the window, so it runs headless, in tests and on a server. Two runs with
   the same seed and inputs give the same states.
3. **View sync**: the one place that turns simulation state into renderer instances and tells the
   renderer what changed. Simulation code does not call the renderer, and the renderer does not
   read simulation types.
4. **Render** (`omsi-render`, `omsi-texture`, `omsi-geometry` for meshes): draws what it is given.
   It owns the GPU and nothing of the game.

`omsi-app` runs these as phases in a fixed order (input, simulation step, view sync, render,
present) for both the window and the offscreen run. A phase is a function with explicit inputs
and outputs.

Debug flags (`OMSI_*`) are read once through one registry (`omsi_cfg::flags`) that names each flag,
its meaning and its type. [Environment variables](DEBUG_FLAGS.md) lists them.

### Enforced rules

- `scripts/arch-guard.sh` checks `scripts/arch-rules.txt` on every pull request. Examples:
  nothing depends on `omsi-app`; `omsi-sim` depends on no renderer, UI, audio, texture, network,
  plugin or launcher crate; `omsi-render` depends on no simulation, script, audio, network or
  vehicle crate.
- `allow` lines record edges that still break a rule. When such an edge is gone, the check asks
  for its line to be deleted, so it cannot come back.
- `app-growth 300`: a pull request may add at most 300 net lines of Rust to `crates/omsi-app`,
  tests aside. A new subsystem gets a crate of its own.
- `scripts/size-budget.sh` limits file and function length (1500 and 300 lines; recorded
  larger ones may not grow more than 5 %).

Details: [Refactor checks](REFACTOR_CHECKS.md).

## Start guard

A release that breaks the start must not strand anyone. (It happened: the updater lives in the
launcher, and a launcher that never drew its window never updated.) `crates/omsi-guard` runs
before anything that could break. It needs no graphics card, window or game state; the arch rules
forbid it the renderer, UI, simulation, scene, audio and launcher UI.

How it works:

1. `main` asks the guard first. It handles `--self-test` (the updater runs a new program with it
   before installing; a build that cannot load is refused), `--guard-watch` (the watcher) and safe
   mode's switches.
2. For a launcher start, `omsi_guard::start` starts the watcher: the same program again, without a
   window. If this version already failed here, it heals right away.
3. The watcher waits for `omsi_guard::healthy` (called once the desktop launcher runs, and by a
   game session after 60 frames), for the process to end, or for `OMSI_GUARD_DEADLINE` (120 s,
   then it ends the process).
4. A failed start of a version that never ran here, or two failed starts in a row, is healed
   (`rescue`): a newer release, else the previous version (`omsi_platform::recovery::rollback`;
   the updater keeps replaced files until the new version has started well), else safe mode, else
   a message.
5. Meanwhile the watcher reads the project's list of broken releases (`release/health.json`). A
   listed version keeps its way back and goes back at the next start.

Files in the data folder:

- `guard/starts.json` (the journal) also records game sessions started by the launcher. A start
  that never reached its first frames and whose process is gone counts as failed.
- `guard/bad-versions` lists the versions that failed here. The updater does not offer them again.

To test it, `scripts/guard-e2e.sh <old build> <new build>` runs real release builds in a
throwaway install against fake releases and deliberately broken versions
(`OMSI_GUARD_TEST_BREAK`).

## Known debt

### What is in place

- The frame is a declared list of stages run by one runner (see [Stages](ARCHITECTURE.md#stages)).
- AI traffic and people simulate in `omsi-sim`. `omsi-app::traffic::Traffic` and
  `omsi-app::humans::Humans` are thin wrappers that dereference to them.
- Every `OMSI_*` switch goes through `omsi_cfg::flags`.
- `App`'s state is grouped by subsystem, and `OMSI_PROFILE`'s keys are array slots
  (`profile_keys::ProfKey`; the names in logs are unchanged).

### Stage declarations

A stage given a context made by `stage_ctx!` can touch only its declared groups; the compiler
holds it to its `READS`/`WRITES`. These stages have one: placement, streaming, traffic, view_keys,
humans, after_humans, on_foot, player_tick, scripted, lights, wheel_surface, audio, photo,
lighting, trace_look. A helper shared with the rest of the game takes a narrower context of its
own (for example `shift_clock` takes a `ShiftClockCx`, made with `narrow!` or `XCx::of`).

The other window stages still take the whole `App`. They call `App` methods of many groups, and
several write what `Common` lends read-only (the controllers, duty and weather stages write the
command line's `Args`; the view camera writes the seat in `Settings`). For them,
`frame_stages/access_tests.rs` scans the source (`omsi-srcscan`) and fails when a stage reaches a
field of a group it does not declare.

What the declarations show: the whole-`App` stages each reach most of the sixteen groups, so no
two are independent, and the converted ones cost a fraction of a millisecond. No stage runs in
parallel yet. A stage becomes a candidate once its context is narrow and it costs enough.

### Timetable

`omsi_sim::timetable_run` holds the trip times, the IBIS, the player's duty and `ScheduleSim`
(`timetable_run/sim/`): the day's departures and their state, the tours' vehicles (`choose`),
routes on the lanes, where a due bus is on its route (`place_departure`), the departure boards. It
reads the map through `TimetableWorld` (implemented by `WorldData`) and the traffic as
`TrafficSim`.

`omsi-app::schedule::Schedule` holds it as the GPU adapter: `fleet` reads and uploads the vehicle
sets of the next departures; `dispatch` and `spawn` put buses on the road and take them off.
Still in the adapter: putting the bus itself on the road (trailers, cars, start place, whether it
may appear), which needs the app's `Traffic` (`trailer_chain`, `may_appear`, `blocked`) and the
GPU.

### The world snapshot

`omsi_sim::world_data::WorldData` is an immutable snapshot of `Arc`'d parts:

- `GroundTiles`: terrain and road faces, answering wheel, walking, camera and ground questions.
- `TileLists`: stops, waiting places, obstacles, parked cars' boxes, petrol stations, echoing
  places.
- `ObjectPlaces`: where the map's objects stand.
- `MapFacts`: the map index's stop facts, the only copy of the AI lists, the date and the chrono
  folders.

`omsi_scene::World` holds it in a `WorldCell` (arc-swap) and publishes a new one when a tile's
ground comes or goes or the tile lists are rebuilt. Readers take it without a lock (`peek` for one
look, `snapshot` for a step; the wheels through `LiveGround`). Loading hands the traffic its
`TrafficArrival`s (light programs, a tile's lanes and parked cars) through
`omsi_sim::handover::Inbox`.

Still in `World` and locked: the GPU side, the loading caches, the light programs' numbering by
owner object, the scripted objects' view halves and the lamps.

### LAN play

The session is `omsi_lan::game` and runs on values: a `LanConfig` from the arguments, a `LanInput`
each frame, and the player's bus as a `VehicleInstance`. It asks the game through `LanHost` (the
world as `WorldData`, lanes, making another player's vehicle, destination displays). A frame is
`tick_send`, the shared world, `tick_receive`, then `view_sync::apply_lan` draws and sounds the
others from `LanGame::drawn`. The app's side is `omsi-app/src/lan_play.rs`.

Still in the app: the shared world's mirror (`omsi-app/src/lan_world.rs`), which drives `Traffic`
from the traffic table ([The traffic table](TRAFFIC_TABLE.md)), `Humans` and their view directly.
It moves to `omsi-lan` once it is simulation state plus events for the view.

### View sync

`omsi-app/src/view_sync/` turns the traffic's and people's state into renderer instances:
`traffic.rs` (AI vehicles, their drivers, traffic lamps) and `people.rs` (meshes, posing and
skinning, coins and ticket blocks). The entry point is
`view_sync::sync(ViewSync { traffic, people }, sim_view, world, renderer, scene)`, traffic first.
Its state, `view_sync::SimView`, is held apart from the simulation (in `GfxState::sim_view` for
the window).

It runs where each part was synced before, so the renderer sees the same calls in the same order:

- **Window**: the traffic at the end of `frame_traffic`, the people at the end of `frame_people`.
- **Offscreen**: the traffic after each population, the people every step while
  `OMSI_TRACE_PAX` traces, and both before each snapshot and in the closing reports.

The simulation hands the view data, never renderer calls:

- Traffic changes (a car added or removed, parts coupled, a train rebuilt, parked cars leaving or
  parking) are `omsi_sim::view_events::TrafficEvent`s (`Traffic::events`).
- People appearing, leaving, renamed, and coins on the desk are `omsi_sim::people::BodyOp`s.
- `view_sync::apply_traffic` and `apply_people` replay them in order right after each call that can
  make them. `OMSI_SCENE_DIGEST` checks offscreen that the renderer sees the same calls.
- AI cars are drawn from an owned `FrameSnapshot` taken at the sync, with their looks
  (`TrafficSim::snapshot_looks`, `CarLooks`) and the timetable drivers (`snapshot_drivers`). Their
  sounds play from a `SoundFrame`; people are posed from a `PeopleFrame`.
- `Traffic::tick` is `TrafficSim::advance` plus `Traffic::after_step`, so the step can run on
  another thread.

Still read from simulation state during the sync (so the simulation stands still there): the
snapshots themselves; the traffic lamps' scripts (`LampScripts`); the player's vehicle
(`view_sync/vehicle.rs`, through `LookVars`); the bus the player rides on foot; coins and ticket
blocks. The sync also hands AI vehicles `Envir_Brightness` and swaps their poses between the
traffic's last two steps.

### Global mutable state

What is left no longer carries one session's state into the next. Game state that lived in
statics moved into its owners, for example:

- The weather model and the classic sky's cloud type: `weather_setup::SkyState`.
- The duty start's opened world and the real-time start: `Args`.
- Map lights near the camera, spotlight cookies, spline-check ends, season-mix counts: `World`.
- Passengers' voice lines, photo-mode settings, settings-window flags: `App`'s groups.
- The `/admin` password: `LanGame`. The drive's extremes: `Offscreen`. Corona pictures: the
  `Renderer` (`pictures_taken`).

Settings are passed down instead of broadcast: `Args::session_settings`
(`startup::SessionSettings`), `World::settings` (`scene::host::SceneSettings`), and plain arguments
for the traffic's volume, rain quality and the lamps' cone weather (`lights::ConeWeather`).

What remains: in `omsi-app` 20 module-level statics with interior mutability and 35 more inside
functions and `thread_local!` blocks (9 of those); in `omsi-scene` 14 and 15, no `thread_local!`.
They are install-once hooks, platform handles and process resources, memos of file contents and
switches, log rate limits, and the picture-file numbering. Count them with:

```bash
grep -rnE '^\s*(pub(\([a-z:]+\))? )?static ' crates/omsi-app/src | grep -E 'Mutex|Atomic|OnceLock|RwLock|Cell|Lazy|Once\b'
grep -rn 'thread_local!' crates/omsi-app/src
```

### Long functions and files

`scripts/size-budget.sh` keeps them from growing. `scripts/size-budget.txt` lists those over the
limits (1500 lines a file, 300 a function) with their recorded sizes. The baseline only goes down.
