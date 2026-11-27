# Changelog

Releases are started by hand from `main` as the beta `MAJOR.MINOR.PATCH`, and tested betas are
promoted to stable releases (see [docs/VERSIONING.md](docs/VERSIONING.md)); the downloads are on the
[Releases](https://github.com/openOMSI-org/openOMSI/releases) page.

## Unreleased

### Fixes

- **Mirror refresh settings cap every mirror picture again**: a mirror shown in another mirror is
  drawn with the mirror that shows it, instead of also taking its own redraw turn. The pictures
  drawn for another mirror now count against the Economical, Full and VR redraw budgets, so
  mirrors no longer refresh faster than the setting allows, and the driver's view keeps its frame
  rate with mirrors enabled
  ([#4341](https://github.com/openOMSI-org/openOMSI/issues/4341)).
- **Articulated buses' third axle now springs with the rest of the rear section**: a two-axle
  rear section samples the road under both axles, instead of only the axle farthest from the
  joint, so a bump under the third axle moves the body instead of leaving that spring rigid
  ([#2945](https://github.com/openOMSI-org/openOMSI/issues/2945)).
- **Mouse steering works the pedals again after it is switched off and on**: a cursor report
  queued before the steering point was centred stays stale until the echo of that move arrives,
  while the first deliberate move after it is counted - and a refused cursor move no longer
  pretends the physical cursor reached the middle
  ([#4346](https://github.com/openOMSI-org/openOMSI/issues/4346)).
- **Android: buses and maps with HTML displays no longer crash the game**: the displays' scripts
  now run on phones that tag their memory pointers, where the game stopped with "this platform
  is not compatible with a nan-boxed `JsValueInner`"
  ([#4382](https://github.com/openOMSI-org/openOMSI/issues/4382)).

## 0.3.59 - 2026-11-26

### New and improved

- **Settings in the launcher by topic**: each category has short topic pages, a search over all
  settings (by name or by the English key), favourite settings with a star, and a Help button
  beside each setting. Graphics profiles have their own page
  ([#3856](https://github.com/openOMSI-org/openOMSI/pull/3856)).
- **Cruise control on Num0**: it holds the speed the bus has when it is switched on, by working
  the pedals the bus's scripts read; the brake pedal or key switches it off
  ([#4043](https://github.com/openOMSI-org/openOMSI/pull/4043)).
- **Passengers add their weight to the bus** (Gameplay > Passengers, off by default): a full bus
  pulls away, brakes and climbs more slowly and sits lower on its springs, and a crowd at the
  back loads the rear axle. `omsi.bus.mass()` follows it
  ([#4296](https://github.com/openOMSI-org/openOMSI/pull/4296)).
- **Rear door boarding** (off by default): passengers get in at the rear and middle doors too,
  each at the nearest door on their side of the bus
  ([#4076](https://github.com/openOMSI-org/openOMSI/pull/4076)).
- **Which doors passengers get in through**: as the bus says, the front door only, the first
  two, or the rear door only ([#4112](https://github.com/openOMSI-org/openOMSI/pull/4112)).
- **The IBIS's stop follows the duty** (off by default), for the buses whose IBIS the game types
  itself (Shift+U, autostart): no "next stop" key to press at every stop
  ([#4113](https://github.com/openOMSI-org/openOMSI/pull/4113)).
- **Automatic announcements say why they are quiet** (a depot file with one recording per stop,
  a bus without an IBIS key or stop counter), and the departure announcement comes as the bus
  pulls out instead of a fixed four seconds after the doors close
  ([#4115](https://github.com/openOMSI-org/openOMSI/pull/4115),
  [#4288](https://github.com/openOMSI-org/openOMSI/pull/4288)).
- **HOF templates**: a depot file's `*.tchof.xml` template beside it (HOF Suite) is read, so its
  display strings are known by name ([#4290](https://github.com/openOMSI-org/openOMSI/pull/4290)).
- **"Reload this vehicle" shows a rebuilt mesh**: a modder who rebuilds an `.o3d` and reloads the
  bus sees the new mesh, not the copy already on the graphics card
  ([#4282](https://github.com/openOMSI-org/openOMSI/pull/4282)).
- **The game can draw the mouse cursor itself** (an option): with G-SYNC on Windows the picture no
  longer stutters while the mouse moves ([#4289](https://github.com/openOMSI-org/openOMSI/pull/4289)).
- **Stop labels from the timetable**: `--export-stop-labels <file>` writes a CSV of every stop's
  name with the lines that serve it and where they go
  ([#4283](https://github.com/openOMSI-org/openOMSI/pull/4283)).
- **Plugins**: the `key` event carries the key combination (`"Ctrl+KeyH"`), as
  `omsi.input.hotkey` takes it, and a chat line starting with `/` and a letter or digit goes to
  the plugins that listen for `chat_command` instead of the chat
  ([#4096](https://github.com/openOMSI-org/openOMSI/pull/4096)).
- **The launcher's add-on list** is read from the add-ons-list repository each time the page
  opens, so a new add-on shows up without a new release
  ([#4260](https://github.com/openOMSI-org/openOMSI/pull/4260)).
- **Android**: "Copy server code" puts the code on the clipboard, and the keyboard opens for the
  join code and the chat ([#4204](https://github.com/openOMSI-org/openOMSI/pull/4204)).

### Graphics

- **Ray-traced global illumination (experimental)**: with Real-Time Ray Tracing on, the new
  `rt_gi` option traces the light surfaces send to each other, frame after frame. The sky's
  light and reflection are dimmed by how much of the sky each point sees, and the saloon is lit
  by what comes in through its windows. Off by default; see
  [Graphics](docs/GRAPHICS.md#ray-traced-global-illumination-experimental)
  ([#4301](https://github.com/openOMSI-org/openOMSI/pull/4301)).
- **Road dirt on the vehicles** (Enhanced, `procedural_dirt`): spray behind the wheels and on the
  lower sides on wet roads, film on the back, dust when dry, slush in winter; the rain washes it
  off. It lies on up to 64 bodies near the camera; the vehicles past them keep their own dirt
  ([#4297](https://github.com/openOMSI-org/openOMSI/pull/4297),
  [#4318](https://github.com/openOMSI-org/openOMSI/pull/4318)).
- **Materials without textures**: `[matl_basecolor]`, `[matl_metallic]`, `[matl_roughness]` and
  the normal and occlusion strengths set PBR values directly or scale the maps, as glTF does
  ([#4294](https://github.com/openOMSI-org/openOMSI/pull/4294)).
- **Emissive materials**: `[matl_emissive]` and its companions give a slot a light of its own, in
  HDR, with a colour, a temperature (a filament warms up as it comes on) and a fade, switched by
  the scripts of a vehicle or of a scenery object; see [PBR](docs/PBR.md#emissive-materials)
  ([#4295](https://github.com/openOMSI-org/openOMSI/pull/4295),
  [#4318](https://github.com/openOMSI-org/openOMSI/pull/4318)).
- **Enhanced lighting and colour grade**: curved clouds in Vanilla, `[light_emitter_interior]`
  (a lamp mesh's light map lights the saloon), height maps for PBR materials, and inverted normal
  maps drawn the right way round ([#4241](https://github.com/openOMSI-org/openOMSI/pull/4241)).
- **The real moon in the night sky**, with its phases and without glare, in Enhanced, Vanilla and
  Vanilla+ ([#4272](https://github.com/openOMSI-org/openOMSI/pull/4272),
  [#4291](https://github.com/openOMSI-org/openOMSI/pull/4291)).
- **Snow on the bus's glass**: soft flakes and an even layer on every pane, without seams, instead
  of rain drops on the doors in a snowfall
  ([#4310](https://github.com/openOMSI-org/openOMSI/pull/4310),
  [#4311](https://github.com/openOMSI-org/openOMSI/pull/4311)).
- **Altocumulus** in fields of soft cloudlets instead of flat flecks
  ([#4312](https://github.com/openOMSI-org/openOMSI/pull/4312)).
- **DLSS sharpness** is a setting (25 % as before)
  ([#4307](https://github.com/openOMSI-org/openOMSI/pull/4307)).
- **Mirrors show the other mirrors**: a door mirror seen in the interior mirror is drawn, and
  before it, instead of standing still ([#4279](https://github.com/openOMSI-org/openOMSI/pull/4279)).
- **Cleaner pictures**: puddle reflections no longer shimmer on the move, the clouds' shaded parts
  have no grid of diamonds, far lamps are round points of light instead of torn squares, rain on
  the windscreen in Original+ has no blocks or black drops, fences and railings are lit by the
  sun on their facing side only, and bus windows in Enhanced are no longer grey
  ([#4261](https://github.com/openOMSI-org/openOMSI/pull/4261),
  [#4262](https://github.com/openOMSI-org/openOMSI/pull/4262),
  [#4259](https://github.com/openOMSI-org/openOMSI/pull/4259),
  [#4269](https://github.com/openOMSI-org/openOMSI/pull/4269),
  [#4205](https://github.com/openOMSI-org/openOMSI/pull/4205),
  [#4224](https://github.com/openOMSI-org/openOMSI/pull/4224)).
- **Reflective AI car bodies** are no longer drawn as see-through glass
  ([#3965](https://github.com/openOMSI-org/openOMSI/pull/3965)).
- **Ground textures in the map's own folder** are found, with their surface sound
  ([#3966](https://github.com/openOMSI-org/openOMSI/pull/3966)).
- **Clouds under an upscaler** are traced at the window's size, not at the smaller picture DLSS or
  FSR is given, and are sharp again ([#4306](https://github.com/openOMSI-org/openOMSI/pull/4306)).

### Multiplayer

- **A conductor for a friend's bus**: the driver gives another player the conductor's role; the
  conductor sees what ticket a passenger asks for, sells it at the cash desk and gives the change
  ([#4280](https://github.com/openOMSI-org/openOMSI/pull/4280)).
- **Other players' buses in less detail far away** (an option): beyond 120 m, or with the frame
  rate under 20, their driver figures and destination displays are left out
  ([#4095](https://github.com/openOMSI-org/openOMSI/pull/4095)).
- **The host knows what the others have served**: a stop another player's bus has stood at and
  left no longer fills up again for that trip, and the timetable's buses near the other players
  are loaded ahead as near the host
  ([#4309](https://github.com/openOMSI-org/openOMSI/pull/4309)).
- **Emissive materials, rail points and taking over a bus online**: the `[matl_emissive]`
  variables reach the other games, the host sets points thrown only by a player whose vehicle is
  near them, and a host cannot take over an AI bus its session does not allow
  ([#4309](https://github.com/openOMSI-org/openOMSI/pull/4309)).
- **The public server list keeps its listings** when Cloudflare moves or restarts the list
  between two heartbeats ([#4206](https://github.com/openOMSI-org/openOMSI/pull/4206)).

### As in OMSI 2

What Omsi.exe does, read from the game itself, where openOMSI did something of its own:

- **Weather**: `.owt` files are read with Omsi.exe's defaults and its short `[precip]` block;
  the wind blows from its direction at Omsi.exe's angle; `PrecipRate` is the strength whatever
  falls; the windscreen wets at Omsi.exe's rate in rain and dries in twenty seconds after it;
  the sun stands where Omsi.exe puts it
  ([#4218](https://github.com/openOMSI-org/openOMSI/pull/4218),
  [#4231](https://github.com/openOMSI-org/openOMSI/pull/4231),
  [#4232](https://github.com/openOMSI-org/openOMSI/pull/4232),
  [#4233](https://github.com/openOMSI-org/openOMSI/pull/4233),
  [#4220](https://github.com/openOMSI-org/openOMSI/pull/4220),
  [#4217](https://github.com/openOMSI-org/openOMSI/pull/4217)).
- **Street lamps, the AI's lights and `Envir_Brightness`** follow Omsi.exe's daylight sum under
  the weather: under an overcast deck, in fog, rain or snow the lamps come on by day
  ([#4219](https://github.com/openOMSI-org/openOMSI/pull/4219)).
- **Traffic lights**: AI cars stop for red, red-and-yellow and yellow alike and go on only once
  their front is over the line; a light without `[approachdist]` is asked from 10 m; lights on
  rails are asked as far ahead as the path search runs; a program point on a light number the
  crossing does not have waits on light 0; `if_request` is set by any line but "0"
  ([#4235](https://github.com/openOMSI-org/openOMSI/pull/4235),
  [#4209](https://github.com/openOMSI-org/openOMSI/pull/4209),
  [#4246](https://github.com/openOMSI-org/openOMSI/pull/4246),
  [#4210](https://github.com/openOMSI-org/openOMSI/pull/4210),
  [#4234](https://github.com/openOMSI-org/openOMSI/pull/4234)).
- **AI traffic**: cars spawn on the lanes that run past the player, not only on those that start
  near, and spread over the ways on at a fork by the map's densities, the emptiest first
  ([#4200](https://github.com/openOMSI-org/openOMSI/pull/4200),
  [#4243](https://github.com/openOMSI-org/openOMSI/pull/4243)).
- **Physics**: `[mass]` is tonnes however large; the body has no air drag of its own and no
  rolling resistance unless the file gives `[rollwiderstand]` (the scripts add both, as in
  OMSI 2); `n_Wheel` is the mean of the driven wheels, an AI vehicle's its speed over the first
  wheel; `Velocity` is 0 without a driven axle; AI vehicles' and coupled parts' wheels turn at
  their own speed over their own size; a pusher articulated bus's driven rear wheels can spin on
  ice ([#4238](https://github.com/openOMSI-org/openOMSI/pull/4238),
  [#4207](https://github.com/openOMSI-org/openOMSI/pull/4207),
  [#4208](https://github.com/openOMSI-org/openOMSI/pull/4208),
  [#4237](https://github.com/openOMSI-org/openOMSI/pull/4237),
  [#4236](https://github.com/openOMSI-org/openOMSI/pull/4236),
  [#4247](https://github.com/openOMSI-org/openOMSI/pull/4247),
  [#4244](https://github.com/openOMSI-org/openOMSI/pull/4244)).
- **Scripts**: a block without `{end}` runs on into the next; a macro call finds only a macro
  defined below it; only the first `{init}`, `{frame}` and `{frame_ai}` run; `(T.F.x)` leaves the
  file name on the string stack, an empty name plays the entry's own file again and a blank one
  stops the entry
  ([#4215](https://github.com/openOMSI-org/openOMSI/pull/4215),
  [#4214](https://github.com/openOMSI-org/openOMSI/pull/4214),
  [#4242](https://github.com/openOMSI-org/openOMSI/pull/4242),
  [#4216](https://github.com/openOMSI-org/openOMSI/pull/4216),
  [#4240](https://github.com/openOMSI-org/openOMSI/pull/4240),
  [#4248](https://github.com/openOMSI-org/openOMSI/pull/4248)).
- **Sound**: `[onlyone]` sounds play once, from the nearest object; `[sound_maxcount]` (from
  OMSI's options.cfg, 200 by default) silences whole sound packs, the farthest first, and far
  sounds no longer restart over and over under it
  ([#4213](https://github.com/openOMSI-org/openOMSI/pull/4213),
  [#4239](https://github.com/openOMSI-org/openOMSI/pull/4239),
  [#4265](https://github.com/openOMSI-org/openOMSI/pull/4265)).
- **Passengers**: stops fill bit by bit as a scheduled bus draws near and stop filling for a bus
  that has passed; passengers choose their entry door, pay with coins and notes drawn as
  Omsi.exe draws them, and count the change to half the currency's unit; the cash desk's change
  keys give the coin the script names
  ([#4211](https://github.com/openOMSI-org/openOMSI/pull/4211),
  [#4230](https://github.com/openOMSI-org/openOMSI/pull/4230),
  [#4249](https://github.com/openOMSI-org/openOMSI/pull/4249),
  [#4245](https://github.com/openOMSI-org/openOMSI/pull/4245),
  [#4212](https://github.com/openOMSI-org/openOMSI/pull/4212),
  [#4198](https://github.com/openOMSI-org/openOMSI/pull/4198)).
- **Rail switches with more paths than `[switchdir]`s** (the NF6D's map): the tram takes the
  path that goes on from where it is, instead of jumping
  ([#4225](https://github.com/openOMSI-org/openOMSI/pull/4225)).
- **Full Tank charges an electric bus's battery too**
  ([#4199](https://github.com/openOMSI-org/openOMSI/pull/4199)).

### Fixes

- **Loading large maps on 8 GB cards**: the timetable's vehicles are preloaded only near the
  player and within the texture budget, the first textures stay in the disk cache from start to
  start, and after the card runs out of memory the renderer is rebuilt whole, with its shadow
  maps, sky and clouds, instead of drawing without them for the rest of the session
  ([#4304](https://github.com/openOMSI-org/openOMSI/pull/4304),
  [#4305](https://github.com/openOMSI-org/openOMSI/pull/4305),
  [#4302](https://github.com/openOMSI-org/openOMSI/pull/4302),
  [#4318](https://github.com/openOMSI-org/openOMSI/pull/4318)).
- **A Direct3D 12 device lost to ray tracing** no longer ends the game, and the Vulkan device the
  game goes on with keeps its full pipelines ([#4226](https://github.com/openOMSI-org/openOMSI/pull/4226)).
- **The launcher's settings page opens again**: a quote in one help text stopped the whole page
  ([#4314](https://github.com/openOMSI-org/openOMSI/pull/4314)).
- **Passenger settings work everywhere**: which doors passengers use, paying without a ticket,
  how late the bus may be, free-drive passengers and waiting for the doors to open worked only
  in online rooms; now also in single player, LAN and when changed in the game, with their rows
  in the launcher. A ticket buyer whose chosen doors sell no tickets boards there instead of
  standing at the stop for good ([#4314](https://github.com/openOMSI-org/openOMSI/pull/4314)).
- **The IBIS** keeps its line letter when a destination is picked by hand, and is not typed twice
  after Shift+U on a trip change
  ([#4253](https://github.com/openOMSI-org/openOMSI/pull/4253),
  [#4314](https://github.com/openOMSI-org/openOMSI/pull/4314)).
- **A duty started on today's date keeps its year**
  ([#4251](https://github.com/openOMSI-org/openOMSI/pull/4251)).
- **Binding one controller button** no longer removes the default buttons of the others
  ([#4162](https://github.com/openOMSI-org/openOMSI/pull/4162)).
- **A latching switch flipped in the pause menu** takes effect when the menu closes
  ([#4202](https://github.com/openOMSI-org/openOMSI/pull/4202)).
- **Bus Company Simulator's `--paint`** is kept for a situation in the default paint
  ([#4203](https://github.com/openOMSI-org/openOMSI/pull/4203)).
- **The upscaler (DLSS, FSR) is not switched back on** after the player turned it off while it
  waited to be tried again after running out of memory ([#4314](https://github.com/openOMSI-org/openOMSI/pull/4314)).
- **mX Graphics have everything the standard graphics got** in this release: cruise control,
  named saves, taking over an AI bus, the passenger and IBIS settings, plugin commands in the
  chat, the announcements, mirrors in mirrors, other players' buses in less detail and more
  ([#4267](https://github.com/openOMSI-org/openOMSI/pull/4267),
  [#4314](https://github.com/openOMSI-org/openOMSI/pull/4314),
  [#4318](https://github.com/openOMSI-org/openOMSI/pull/4318)).

## 0.3.58 - 2026-11-24

### New and improved

- **mX Graphics twice as fast on the GPU**: in Enhanced the main pass draws most surfaces with a
  lean pipeline that leaves out the shading they do not use (films, water, LEDs, bump maps, wet or
  snowy roads in dry weather), compiled in the background, and the ground's albedo pass works
  with MSAA and ambient occlusion. In the MAN EN92's cab at Grundorf (1920x1080, Enhanced, 4x
  MSAA, Apple M4) a frame takes 12.6 ms of GPU time instead of 29.5 by day, 15.3 instead of 26.6
  in rain and 13.1 instead of 28.1 at night: 70, 59 and 64 frames a second instead of 32, 36 and
  32. `OMSI_NO_LEAN` draws everything with the full pipelines.
- **mX Graphics are fully bindless**: the renderer, the interface and the VR mirror read every
  texture, sampler and buffer from mX's heap, ray tracing and the multisampled pane depth
  included; no bind groups are left.
- **Name a save in the game and load any save by name** ([#4045](https://github.com/openOMSI-org/openOMSI/pull/4045)).
- **Plugins page in the launcher**: switch plugins on and off, see their errors and change their
  settings ([#4044](https://github.com/openOMSI-org/openOMSI/pull/4044)).
- **Take control of an AI bus** from the map camera, as in OMSI 2
  ([#4047](https://github.com/openOMSI-org/openOMSI/pull/4047)).
- **A dead zone for each controller axis** instead of one for all, set in the settings file
  ([#4108](https://github.com/openOMSI-org/openOMSI/pull/4108)).
- **A stop's own passenger numbers**: a stop's numbers are multiplied before the rounding, so
  small stops get their passengers ([#4101](https://github.com/openOMSI-org/openOMSI/pull/4101)).
- **Signed publisher packs**: a publisher's map can depend on a signed pack under `Packs/`,
  played with the publisher's key in `Publishers.json`; ordinary maps and mods load as before
  ([#3593](https://github.com/openOMSI-org/openOMSI/pull/3593)).
- **Scripts read the track**: `omsi.bus.track()`, a first step towards a rail simulation
  ([#4051](https://github.com/openOMSI-org/openOMSI/pull/4051)).

### Multiplayer

- **A hosted LAN session sets the buses its players drive**
  ([#4100](https://github.com/openOMSI-org/openOMSI/pull/4100)).

- **Rail points lie the same way for every player**: the hosting game's points go to each
  player's game with the traffic lights (those within 600 m of its vehicle, once a second), and
  points a player's tram throws (driving over them, the J key, a click) go to the host and from
  there to the others. Each game used to keep its own points, so a tram that turned off in one
  game drove straight on in the others'. With a host of the previous release each game still
  keeps its own.

- **A long tram is drawn whole in the others' games**: every coupled part of a player's train goes
  to the others (up to 15), not only the first three. The NF6D is made of eleven, so its B car and
  bogies stood off the rails or across the track in the other players' games. A game of the
  previous release still draws the first three.

### Fixes

- **mX Graphics on OpenGL (ANGLE on macOS)**: blending works again - the interface's text and
  icons drew as transparent boxes, smoke and LED layers were wrong - and textures with mip maps
  made on the GPU are no longer black at a distance.
- **Trams follow switches set by traffic lights**
  ([#4183](https://github.com/openOMSI-org/openOMSI/pull/4183)).
- **Dashboard labels on meshes with mixed materials** no longer flicker or disappear
  ([#4185](https://github.com/openOMSI-org/openOMSI/pull/4185)).
- **Indicators with their own bitmap** (`[light_enh_2]`) show the whole bitmap while blinking
  instead of a tiny point ([#4159](https://github.com/openOMSI-org/openOMSI/pull/4159)).
- **Multi-line vehicle menus**: the Volvo Wright dashboard menu keeps its rows aligned, and
  multi-line strings in vehicle scripts compile as one
  ([#4181](https://github.com/openOMSI-org/openOMSI/pull/4181)).
- **Pedestrians stuck walking in place** for two minutes are removed
  ([#4102](https://github.com/openOMSI-org/openOMSI/pull/4102)).
- **The MAN NLC's windscreen no longer fogs up with the heater on**
  ([#4161](https://github.com/openOMSI-org/openOMSI/pull/4161)).
- **mX graphics have the tram changes too**: the rear cab, backing a tram from it and the J
  points key, the tunnel mouths and the sun kept out of tunnels, the rail vehicles' collisions,
  the curves and the body sway came only to the standard graphics. With mX graphics switched on
  the NF6D's B car could not be driven and tunnel mouths stood closed.

## 0.3.57 - 2026-11-23

### New and improved
- **Alt menu redone**: the quick menu's thirty tiles are flat, squarer and have pictograms of their own in the style of OMSI 2's menu (a bus with a plus, a cross or arrows for placing, removing and swapping it, a pump, a clock, a chequered flag...) in place of the generic symbols.
- **Tunnel mouths are open**: a `[terrainhole]` standing upright, as in the Kartoffelphantom tram
  tunnels, is an opening: the ground is cut behind it up to where the hill is over it, and no wall
  stands across it. The walls of a cut in the ground show the ground's texture instead of streaks.
- **No sun inside tunnels**: a spline with a profile as tall as a storey (2 m or more) casts its
  shadow wherever it lies, also when it is buried in the ground.
- **Rail vehicles no longer crash into platforms they clear**: on the rails each car of a tram or a
  train meets scenery with its own box, where that car stands on the track, in bends too. Above the
  top of a platform (0.35 m over the rails for a tram, 1.0 m for a train, 1.15 m for a metro) the box
  is the car's whole width, so a pole or a wall beside the body stops it. Below that only the strip
  over the track's middle counts (0.7 m either side), as the real body overhangs a platform's edge:
  a buffer stop still stops it, the platform no longer does. Other vehicles meet the leading car's
  whole body, side impacts included.
- **An AI tram's rear section follows the bend from the moment it appears**: the track behind a tram
  put on its rails is laid out from the lanes before it, so a KT4D's rear section no longer stands
  across the track until the tram has driven its own length.
- **Trams roll through tight curves**: a street tram's curve resistance is its own (about 10 N/kN
  in a 25 m curve) instead of a main-line car's, and the curve is taken over 12 m of track, so the
  kink where a switch's lanes meet no longer reads as the sharpest curve there is. A contact shoe on
  a car with a pantograph (the NF6D's) no longer makes it a metro car.
- **Mirrors on every car of a train**: a coupled car's `reflexionN` mirrors show its own mirror
  cameras, numbered on from car to car, instead of the first car's.
- **Passengers use every car of a train**: a car coupled turned round, back to back with the car in
  front of it, is joined into the passenger cabin turned round, so people get in and out by its own
  doors and ride in it; and the doors of a car that runs its own scripts (the NF6D's sections)
  open for the passengers and for the player on foot by that car's own `PAX_Entry<n>_Open`.
- **Third rail and curve squeal on trams and trains**: `[contact_shoe]`s read the `[third_rail]`
  under them (`contactshoe_N_volt_rail` and the rest, which the NF6D's PZB takes its codes from),
  and every car's bogies report the curve under them and when the wheels reach the flange
  (`boogie_N_invradius`, `boogie_N_wheel_at_limit`), which is what starts the curve squeal. A
  rail's curve is the map's own, not worked out of the path's samples.
- **`GetHeightAbovePoint` as in Omsi.exe**: the ray starts 3 m over the point, meets a wire or a
  wall top on a narrow height profile, and reads 1000 with nothing below it (it read 0, which held
  the Zweiachser tram's pantograph at the top of its travel).
- **Every car of a train rocks on its own springs**: each car's body leans out of bends by the force
  on it and rocks on the rails, with `[rail_body_osc]`'s lever, frequency and decay where the file
  gives them and the class's springs where it switches them off. Joints go with the cars around
  them; bogies, couplings and wheel sets stay level. **This changes the default**: the lean is now
  always on, also without the realistic rail physics (which still add the dip under braking and
  the bounce), and a car shows half of the lean its springs work out (`ROLL_SHOWN`).
- **Drive a tram from either cab**: in a train with a cab at each end (the NF6D) walk
  to the other cab and take its wheel (G), or switch driving ends from the outside view with the
  interior-camera keys. That cab's own scripts then take the keys, the switches and the indicator,
  and its cameras, driver figure, sounds, roller blinds and destination displays are its own. Every
  car with scripts of its own pulls and brakes, and the cars talk over their control cables as in
  OMSI 2. Camera changes glide inside a coupled part too.
- **Backing through points keeps the train together**: backing, the vehicle follows the track its
  train lies on, and the train's rear end takes the way the points are set (or the rear cab's
  indicator says) and throws them.
- **New key J, set the points**: J sets the first points within 50 m ahead of the train's leading
  end to their next branch (the rear end when driving from the rear cab or backing). It is
  openOMSI's own key; OMSI 2 has none for it.
- **Plugins can lay cards of their own over the navigator's map**: a plugin that fills the navigator's frame (`frame = true`) can put small cards of its own on the map - the next stop, the speed, whatever it shows - with the `map` element's new `overlay` key, at any corner or edge of the map and held inside it (what crosses the map's edge is left out, the card's corners stay rounded); their buttons take clicks as in the rest of the panel (`omsi.ui.version` 8). An example is `docs/examples/plugins/navigator_map.lua`.
- **mX Graphics (Experimental)** on a Mac: Settings → Graphics → Display → *mX Graphics (Experimental)* draws the game on Metal with [mX](https://github.com/openOMSI-org/mX), openOMSI's own graphics library, instead of wgpu, from the next start. The picture is meant to be the same. It is an early step: the game's shaders do not use mX's bindless textures yet, and in the tests so far it spent less processor time on a frame than wgpu and the same time on the graphics chip. Ray tracing and frame generation stay off with it; where mX cannot open the graphics chip, the game draws with wgpu as before. Off by default. mX's Direct3D 12 and Vulkan have not drawn the game on a real graphics card yet, so Windows and Linux do not offer the switch; `OMSI_MX=on` tries them for one run (without DLSS, FSR 3 and VR), and `OMSI_MX=off` turns mX off for a run on a Mac.
- **Plugins see the AI vehicles' size**: `omsi.traffic.list()`, `get`, `nearest` and `ahead`
  give each vehicle's `bounding_box` (`{width, length, height, cx, cy, cz, heading, default}`,
  the .bus/.ovh `[boundingbox]`), so a blind-spot or distance plugin knows where a truck's or an
  articulated bus's body really is, not only its origin (#3936).
- **Cruise control**: Num0 holds the speed the bus drives at (above 5 km/h), working the
  throttle and the brake the bus's scripts read, so a bus with a strong engine brake no longer
  has to be pumped along on the keys. The brake switches it off; pressing the throttle further
  wins while it is held. Buses that use Num0 themselves keep it; `cruise_control` can be bound
  in keyboard.cfg's `[game]`. Plugins get `omsi.bus.cruise()` and `omsi.bus.set_cruise(kmh)`
  (#3976).
- **Plugins know where a train is on its track**: `omsi.bus.track()` gives a tram's, metro's or
  train's track (lane), the metres along it and its length, its direction and the net distance it
  has moved on the rails (less after backing up) - the first piece of a rail simulation with blocks and train
  protection; `docs/RAIL.md` lays out the rest (#3969).
- **Take control of an AI bus, as in OMSI 2**: in the map camera (F4), click a bus of the AI traffic and answer *Yes*: you drive it at once, even while it moves, with its passengers, its switches and displays as the AI left them, and its line and tour from the stop it was driving to. The bus you drove before stays where it is. As in OMSI 2, the bus is not changed while a timetable is active; not for a LAN client (#3974).
- **Plugin catalog in the launcher**: Content → Plugins → Browse lists the plugins of the new
  [openOMSI-Plugins](https://github.com/openOMSI-org/openOMSI-Plugins) repository: open source,
  read by a maintainer before they are added. One click installs a plugin into the plugins
  folder (only when every file is the one the catalog names, by its SHA-256), an update is
  offered when its version goes up and keeps its settings, and *Read the code* opens its source
  (#3920).
- **Plugins page in the launcher**: Content → Plugins lists the Lua, `.oop` and OMSI
  plugins of the game's plugins folders. Each has a switch (a plugin switched off is left out
  of the next game, its files and settings stay where they are), shows whether it ran, did not
  load (with the error, e.g. a syntax error) or was stopped after errors in the last game, and
  a plugin that declares settings with `plugin.settings` gets a settings form there too (#3919).
- **Saves with names of your own, and loaded by name in the game**: Esc → *Save to a slot...*
  → *Save under a name...* lets you type the save's name (Enter saves); it keeps that name when
  you save over it later. *Load the quicksave* became *Load a situation...*: the quicksave and
  every save of the map, by name, to load straight from the game (#3975).
- **"Place a vehicle..." names and pictures as in the launcher**: a maker's buses are grouped into models as the launcher's bus picker groups them (*Citaro* with *C2*, *C2 G*, *C2 K* under it) instead of type names with folder and file beside them; the picture's second line is the bus's description, not its file's path; a maker's line shows its first bus; and the standard paint is called by the name the bus gives it, as in the launcher.
- **Controls profiles**: the launcher's Controls page has profiles, each with every key and
  every controller setting: *Duplicate* makes a new one of the current settings, the list
  switches between them, and they can be renamed and deleted. One can be the *default for every
  bus*, and a bus can have its own (*Use for* the bus chosen on the Drive page): before each
  drive the launcher switches to the bus's profile, or to the default one (#3921).
- **Frame generation on a Mac**: with Apple MetalFX upscaling on, Settings → Graphics → *Frame generation* shows a generated frame between every two drawn ones (Apple's MetalFX frame interpolator; a Mac with Apple silicon on macOS 26 or newer), for smoother motion at the price of a little more input lag. Where fewer than 30 frames a second are drawn with it (Apple's advice), it is turned off for the drive, as SSAO and the shadows are. The frame rate's corner then reads e.g. `90 fps (45 + FG)`. Off by default, experimental.
- **Apple MetalFX upscaling on a Mac**: Settings → Graphics → Upscaling offers *Apple MetalFX* on a Mac with Apple silicon (macOS 13 or newer), with the same quality levels as NVIDIA DLSS and AMD FSR 3 on Windows: the picture is drawn smaller and brought up to the window with the help of the last frames, in place of the anti-aliasing and the render scale, from the next start. It is part of macOS, so there is nothing to install. Where the Mac's graphics chip scales up less than Ultra performance asks for, the picture is drawn at the smallest size it takes.
- **Rename a save**: the pencil beside a save slot in the launcher's *Continue* list gives it a name of your own, without editing its `.osn` file in the map's `Saves` folder. Saving over that slot in the game keeps the name (#3849).
- **AI reaction time**: the new `ai_reaction` setting (25 to 200 %, default 100 %, a slider in the launcher's Traffic settings) scales how long the AI drivers wait before moving off, so a queue at a light pulls away closer together at a lower value. It never goes under 0.4 s, the shortest reaction the junction code is built for (#3793).
- **Force feedback sliders in the game's options**: a device's Force feedback tab in the game's controller settings now shows steering force, vibration, self-centring, damping, parking resistance, minimum force and smoothing as sliders like the launcher's, which you can drag as well as step with Left and Right.
- **What's new in the launcher**: after an update the launcher shows the notes of every
  version since the one last seen, opened, and the others folded below them; *What's new*
  beside *Check now* (Settings) opens them at any time, and the offer of an update shows that
  version's notes before it is installed. The notes come from the releases on GitHub; links in
  them open in the browser, and only https ones.
- **Remote cockpit on a phone or tablet**: the game serves a page on your home network
  (Options > Interface > Remote cockpit, off until switched on); scan the QR code or type the
  address and the 6-digit code, and the phone or tablet shows a dashboard with the map and the
  route, the bus's own buttons (every door, lights, indicators, kneeling...), its real
  destination sign, a touch steering wheel and pedals, and a layout editor per bus. Nothing to
  install, nothing leaves the network.
- **Mouse steering keeps the system cursor in the window**: with *Hold the cursor while steering*
  off, the cursor is the system's own (no frame of delay) and now stays inside the game window
  while the mouse steers, so a hard turn no longer lets it slip onto another screen (Windows and
  Linux; macOS keeps it free as before).
- **Place the bus facing the way you want**: on the city map, Ctrl+drag from the place towards
  the way the bus should face (a plain Ctrl+click keeps the nearest street's direction); in the
  map camera (F4), Ctrl+click puts the bus on the street facing the way the camera looks, as in
  OMSI 2.
- **Go anywhere on foot from the city map**: Alt+click a place on the city map and you stand
  there on foot; from the driver's seat you get up first, and the bus stays where it is.
- **The radio from the Alt menu**: a Radio tile opens a small panel with what plays, the volume,
  on and off, the station before and after, quieter and louder - for buses whose radio has no
  volume knob or no switch of its own.
- **Seat, view and mirrors from the Alt menu**: the quick menu has a tile that opens the panel
  for the driver's eye, view and mirrors of the bus, without going through the options.
- **Cleaner photos**: the photo mode hides the route arrows by itself (they come back when you
  leave it) and its panel has a Hide (H) button at the top.
- **More traffic for strong computers**: the game's Traffic slider (World options) and the launcher's
  *Cars around* go on past 120 to 160, 220 and 300 vehicles, for busy maps and rush hours; more
  cars cost frames and can jam junctions, so 120 stays the top of the usual range. Dedicated
  servers keep their limit of 120.
- **Your depot buses in the game**: the game menu's *Place a vehicle* lists the buses of your depot
  first (*My depot: name | plate*). One taken replaces the bus driven, with its livery, fleet
  number, plate, odometer and wear; the session is written back to it when it ends, and the time
  spent in the bus driven before goes to that one (when it was a depot bus too).
- **Choose which notes show**: under *Notes in the top-left corner* (launcher, game options, Apple
  app) four switches turn off one kind each - why the bus does not move, the bus's own messages
  (the announcements among them), the end of a trip, tickets and change - while the rest stay.
- **Only the buses on the map, if you like**: under "AI vehicles on the map", *Cars on the map too*
  (`nav_ai_cars`, launcher and game options) can be switched off: the navigator and the city map
  then show the traffic's buses and trolleybuses, not the cars.
- **Getting up has a key of its own**: *Get up from the seat (on foot)* is in the Controls'
  list (keyboard and controllers), on Ctrl+Shift+G as before; it can be moved to another key or a
  controller button, and the old built-in Ctrl+Shift+G gives way to it.
- **A flash of the high beam waves the traffic on**: flash the headlights (high beam on for under
  a second and a half) and for six seconds the cars that were waiting for your bus - at a junction,
  or holding back for where the bus is about to go - take their turn first. They still stop for
  the bus itself.
- **The Alt menu stays open**, as in OMSI 2: a tile that does its work in place (route arrows,
  refuel, repair, wash, reset, a screenshot...) leaves the menu open on its page, so the next
  tile is one click away; Alt or Esc closes it. A tile that opens a list, a panel, the city map
  or the photo mode still hands over to it.
- **Set the world to the time it is now**: World options › Traffic and people has *Set to the time it
  is now*: the timetable buses are put out again for the current time and the people waiting at
  the stops are taken off; the stops fill again as people walk up, as many as the hour wants -
  after changing the time by a little, or whenever the stops are fuller than the hour wants.
- **Finer moves in the photo mode**: Alt now slows the turning as well as the moving (to a
  quarter), for the small adjustments of a zoomed-in shot, and the camera rises and sinks at half
  the speed it moves along, so it can be lowered to a kneeling height without overshooting.
- **Several AI vehicles at once**: in Settings › AI Lists, the vehicle picker for adding to a group,
  a depot or a line has a tick box on each vehicle; tick several and *Add the n ticked* adds them
  all in one go (one click still previews, a double click still adds one).
- **Long lists in the launcher filter as you type**: a drop-down with more than twelve entries
  (the liveries, the depot files, the languages...) shows a search line at its top; typing
  narrows it down, Backspace and Esc undo the search.
- **Enhanced has options of its own, and the quality presets cross the modes.** Enhanced+ is no
  longer a mode: it is Enhanced with *Real-Time Ray Tracing* switched on (`ray_tracing=1`; a
  `graphics=enhanced_plus` file reads as that, and `--enhanced-plus` still asks for it for a run).
  Graphics has *Light has shadows* (`light_shadows`: Off, Street lamps and vehicles, Every light),
  *Cloud shadows and light shafts* (`cloud_shadows`), *Glow of lights* (`light_glow`),
  *Volumetric fog* (`volumetric_fog`, on by default),
  *Real-Time Reflections* (the puddles' setting, now with the wet roads' and the water's) and
  *Reflective objects reflect* (the reflection maps), next to the street light and the mirrors. The
  presets are Low, Medium, High, Extra, Max and Ultra, and each sets the mode too: Low is Vanilla,
  Medium Vanilla+ (what the game starts with), High to Max Enhanced with more switched on at each
  step, Ultra Enhanced with ray tracing and everything on. The launcher, the game's options, the
  Apple app and the quality optimiser follow it; every new text is translated into all the
  languages.
- **The clouds' shadow lies over the land** (Enhanced): the sun is no longer one number for the
  whole picture. A field of nine kilometres, worked out from the very heaps the sky shows and kept
  in the sky table's texture, makes patches of sun and shade on the ground that the wind moves, and
  the air scatters the sun's shafts through the gaps of a broken sky as far as seven kilometres.
- **Lights shine and cast shadows in every direction** (Enhanced, *Light has shadows: Every
  light*): a spot that is no headlamp gets a map along its own axis, the two strongest lamps that
  shine every way a cube map of six tiles (fifty-six lamp tiles now), and a lamp in the dark has a
  glare round it that falls as the cube of the distance and a halo in the night air (*Glow of
  lights*).
- **Wet roads and water mirror the street** (Enhanced): the sheen of a wet road and still water
  trace what stands round them in screen space at their Fresnel share, not only the pools - the
  lit windows and lamps of a street stream down a wet road at night.
- **A clean screen (Alt+U)**: the navigator, the virtual mirrors, the notes and the panels go away
  while you drive, for a free view of the road or a quick picture; Alt+U again brings them back.
  The menu, the city map and a plugin's permission question still show over it. It is in the
  Controls' list as *Hide / show the whole interface*, so it can be moved to another key or a
  controller button (`clean_screen` in keyboard.cfg's `[game]`).
- **A flashlight (Alt+F)**: a hand lamp held beside the eye that shines where the view looks, a
  warm white cone reaching 50 m with a soft edge, in the Vanilla and the Enhanced pictures, in
  every view, by night in the dark or by day inside a dim cab. `flashlight` in keyboard.cfg's
  `[game]` (or a controller button) switches it as well.
- **The cab's steering wheel turns as far as yours** (*Cab wheel rotation*, in the launcher's
  Settings › Driving › Game controllers and in the game's Options; `cab_wheel_rotation`): the
  wheel model in the cab turns the set degrees lock to lock, 180° to 1800°, so a 900° or 1080°
  wheel on the desk and the one in the bus move alike. *Automatic* (0, the default) keeps the
  bus's own animation, as before. A bus that turns its wheel from a script of its own keeps its
  own rotation.
- **A modern engine for HTML screens (opt-in)**: a page that puts
  `<meta name="omsi-engine" content="modern">` in its head runs on a second engine with flexbox,
  `position` with `z-index`, borders, `calc()` and custom properties, attribute selectors,
  combinators, gradients, shadows, and a full JavaScript engine (Boa): classes, `Promise`,
  `async`/`await`, ES modules, `fetch` of the page's own files and a fuller DOM. Pages without
  the line keep the classic engine and are not touched; `OMSI_HTML_ENGINE=modern` runs every page
  on the modern one for comparing. See [docs/HTMLTEXTURE_MODERN.md](docs/HTMLTEXTURE_MODERN.md).
- **Transport modes in the launcher**: maps and vehicles show their mode (tram, railway, metro,
  trolleybus), the map and vehicle pickers can filter by it, and a vehicle that does not fit the
  chosen map is marked with the reason and cannot be started (the game refuses it too).
- **Signalling blocks, points and depot gates for the AI trains**: blocks of `signals.cfg` (given by
  where they begin and end) hold AI trains one at a time, and the map's signal objects show
  whether their block is free; the points of `switches.cfg` stay under a train, return to their
  rest position, can be `manual` or locked by a signal; the first departure of a service comes out
  of its depot's gate.
- **The overhead wire and the third rail power the player's rail vehicle**: with a `catenary.cfg` or
  `thirdrail.cfg` in the map's `transport` folder a tram's pantograph or a metro's shoe needs a live
  section under it (dead sections and breaks stop the drive, a sagging line weakens it), a vehicle
  says `[rail_power]` where it gets its current. `network.cfg` sets the class, street, third rail,
  tunnel and speed of the rail lanes, and `switches.cfg` the speed of a diverging branch.
- **AI trams, trains and metro trains by services**: the trips made from a map's `routes.cfg` are
  routed by the game over the whole map's network (the lanes of the mode's kind nearest each
  station, the shortest way through them, with the direction of the lanes respected) and timed
  from the stretch lengths, the segment speeds, acceleration and the standing at stops; their
  departures wait until the track is found, and a trip that cannot be routed is named in the log.
- **Scripts for vehicles and map objects**: a vehicle can name Lua or `.oop` scripts with
  `[vehicle_script]` that start when it becomes the player's and stop when it is left, with the
  permissions of vehicle content (`ui`, `storage`, `vehicle_write`, `audio`). A `.sco` can name
  scripts for the objects placed from it (`[lua_script]`, `[lua_range]`, `[lua_rate]`); they hear
  `object_init`, `object_frame`, `object_click` and `object_gone` and drive their own objects
  through the new `omsi.object.*` functions: variables, strings, `{trigger}`s, and moving,
  turning, scaling or hiding single meshes. Scripts are told apart by their whole path, so two
  folders may each hold a `gate.lua`. See [docs/PLUGINS.md](docs/PLUGINS.md) and
  [docs/OBJECT_SCRIPTS.md](docs/OBJECT_SCRIPTS.md).
- **Realistic rail physics** (with the setting "Realistic road physics", or `OMSI_REALISTIC_ROAD=1`;
  off by default, so no existing tram, train or trolleybus drives differently without it):
  - **Pantographs meet the wire**: a raised pantograph rises until its head touches the contact
    wire and no further (the head's travel is measured on the model's own meshes and animations;
    the script's variable is left alone, only the animation is shown the travel at which the head
    is on the wire), presses it with its static force, and brings current only while it lies on a
    live wire. The wire is the standard height of the class over the rails (kept within the
    pantograph's reach; it is not read from the map yet), staggered across the track, harder at
    forks and rougher in the wet and the frost. Where the wire is uneven at speed the head leaves
    it: a break under current strikes an arc, drawn as sparks and a flash; lowering under load does
    the same. `Panto_Height`, `Panto_Contact`, `Panto_Force`, `Panto_Arc` and `Panto_Powered` are
    written for models and sounds.
  - **Rail bodies ride on their springs**: heave, pitch and roll on the secondary suspension answer
    the lateral acceleration of a bend, the traction and the brake and the unevenness of the rails
    at each bogie, with damping. `rail_body_roll`, `rail_body_pitch`, `rail_body_heave`,
    `rail_accel_side` and `rail_accel_vert` are written for scripts.
  - **AI trams and trains** pull as their class does: the pull falls with speed at the class's power.
  - **Legacy trolleybuses keep their script's traction**, including its gear and door-brake
    interlocks, even with realistic physics enabled. An inferred generic drive stays inactive
    until a script adapter can honor those controls. Explicitly configured trolleybus drives
    and pole simulation remain available.

- **A lost graphics device says why**: on Direct3D 12 the game's log gets DRED's report (the reason, where each unfinished command list stopped and in which pass, a page fault's address and the resources there or just freed there), on Vulkan the driver's `VK_EXT_device_fault` report where it has one. `OMSI_MX=off` starts a run on wgpu, `OMSI_MX=on` on mX, whatever the setting says.

### Fixes

- **"Place a vehicle..." names and pictures as in the launcher**: a maker's buses are grouped into models as the launcher's bus picker groups them (*Citaro* with *C2*, *C2 G*, *C2 K* under it) instead of type names with folder and file beside them; the picture's second line is the bus's description, not its file's path; a maker's line shows its first bus; and the standard paint is called by the name the bus gives it, as in the launcher.
- **mX Graphics no longer leaves a grey screen or a few frames a second**: a graphics device lost under mX (Direct3D 12 removing it, a lost Vulkan device) is recognised as lost - every frame was dropped before and the window stood grey - and the game starts again on the other graphics interface, and on wgpu if it is lost there too; where no graphics chip opens on mX, or only a software renderer (WARP, llvmpipe: three or four frames a second), the game starts again on wgpu by itself. The setting stays on. On Direct3D 12 the bindless heap's free slots hold empty views instead of the views of textures already freed, which a stale index read and the driver answered by removing the device ([#4157](https://github.com/openOMSI-org/openOMSI/pull/4157)).
- **Rain on the glass looks like rain again** ([#4153](https://github.com/openOMSI-org/openOMSI/pull/4153)): the windscreen shows drops instead of a grey mist, the runners no longer cut wide clear stripes, the mirrors show no dense noise of drops, and the falling rain is see-through again in Enhanced.
- **Clouds without rings and stripes**: the Enhanced sky no longer slices into rings round the zenith or bands near the horizon; the cloud pass is skipped when the sky is clear or clouds are off.
- **Rain and snow fall on phones**: Android and iOS never drew the falling rain or snow; they do now (a phone whose driver fails on them keeps without them from the next start).

## 0.3.56 - 2026-11-22

This beta brings a new sky and weather system, trams that can be driven from either cab, passengers waiting in free drive, and parked buses on dedicated servers. It also adds in-game vehicle previews and opens the experimental mX renderer to more platforms. Online play uses LAN protocol 18, so players and servers need matching versions.

### New and improved

- **🍎 Mac only: Apple MetalFX upscaling and frame generation.** Apple MetalFX exists only on a Mac with Apple silicon (M1 or newer): on Windows, Linux, Android and Intel Macs it does not appear in the menus. Windows players keep NVIDIA DLSS and AMD FSR 3 as before. Settings → Graphics → Upscaling offers *Apple MetalFX* on a Mac with Apple silicon (macOS 13 or newer), with the same quality levels as DLSS and FSR 3 on Windows. It is part of macOS, so there is nothing to install, and the picture is sharpened so gauges and leaves stay crisp. On macOS 26 or newer, *Frame generation* adds a generated frame between every two drawn ones for smoother motion; it is experimental and off by default ([#3863](https://github.com/openOMSI-org/openOMSI/pull/3863), [#3867](https://github.com/openOMSI-org/openOMSI/pull/3867), [#3943](https://github.com/openOMSI-org/openOMSI/pull/3943), [**@realmaitreal**](https://github.com/realmaitreal)).
- **Vehicle lamp halos stay compact** in Enhanced with *Glow of lights* on, so brake lights and indicators read clearly; street lights keep their broad glow ([#4030](https://github.com/openOMSI-org/openOMSI/pull/4030), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Graphics profiles can be exported and imported** as a file from Settings → Graphics → Profiles ([#3855](https://github.com/openOMSI-org/openOMSI/pull/3855), [**@shamough1792**](https://github.com/shamough1792)).
- **Picture:** Pixel XC list rows on the SOR NBG 12 show whole, layered instrument displays lose their black strips, dashboard highlights run across spaces, and ticket blocks and coins on the cash desk take the cab's light at night ([#3897](https://github.com/openOMSI-org/openOMSI/pull/3897), [#3923](https://github.com/openOMSI-org/openOMSI/pull/3923), [#3915](https://github.com/openOMSI-org/openOMSI/pull/3915), [#3967](https://github.com/openOMSI-org/openOMSI/pull/3967), [**@rafaelkaiz**](https://github.com/rafaelkaiz), [**@shamough1792**](https://github.com/shamough1792), [**@szambonurek-88**](https://github.com/szambonurek-88)).
- **Phones and tablets:** big maps load on chips without BC textures (large textures drawn at half size there), and the game has about 250 MB more while driving because the launcher's bus preview lets go ([#3911](https://github.com/openOMSI-org/openOMSI/pull/3911), [#3910](https://github.com/openOMSI-org/openOMSI/pull/3910), [**@fsalexandre**](https://github.com/fsalexandre)).
- **Maps can give their own climate:** an `openomsi_climate.cfg` beside `global.cfg` with twelve monthly mean temperatures makes natural weather follow the real climate of the place ([#3842](https://github.com/openOMSI-org/openOMSI/pull/3842), [**@trolley813**](https://github.com/trolley813)).
- **AI drivers follow their vehicle's `rowdy_factor`**, so a car's file sets how fast and boldly it drives ([#3961](https://github.com/openOMSI-org/openOMSI/pull/3961), [**@szambonurek-88**](https://github.com/szambonurek-88)).
- **Trams and trains grow up.** A tram with a cab at each end, such as the NF6D, drives from either cab (walk there and press G, or switch ends from the outside view), backs through points without coming apart, and the new **J** key sets the points ahead. Passengers use every car, each car rocks on its own springs, mirrors work on every car, and trams no longer crash into platforms they clear ([#3995](https://github.com/openOMSI-org/openOMSI/pull/3995), [#4061](https://github.com/openOMSI-org/openOMSI/pull/4061), [#4024](https://github.com/openOMSI-org/openOMSI/pull/4024), [#4025](https://github.com/openOMSI-org/openOMSI/pull/4025), [#4026](https://github.com/openOMSI-org/openOMSI/pull/4026), [**@ImperialGaming4**](https://github.com/ImperialGaming4)).
- **Controls profiles.** The launcher's Controls page keeps profiles with every key and controller setting: duplicate, rename and delete them, pick one as the default for every bus, and give a bus its own. The launcher switches to the right profile before each drive ([#4046](https://github.com/openOMSI-org/openOMSI/pull/4046), [**@turbo-devv**](https://github.com/turbo-devv)).
- **The outside view (F3) starts straight behind the bus**, a little above it, as in OMSI 2 ([#3824](https://github.com/openOMSI-org/openOMSI/pull/3824), [**@thibautgnt**](https://github.com/thibautgnt)).
- **The clock moves by one minute** with new -1 min and +1 min buttons on the Time page ([#3830](https://github.com/openOMSI-org/openOMSI/pull/3830), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Set a fleet number on the phone** before the start, and on the desktop for every bus, also one with no number list of its own ([#4078](https://github.com/openOMSI-org/openOMSI/pull/4078), [**@szambonurek-88**](https://github.com/szambonurek-88)).
- **Rail:** trams follow points thrown by IBIS or map scripts, an AI tram's rear section follows the bend from the start, street trams roll through tight curves, AI trains show tail lights only at their ends and find their destination textures ([#4090](https://github.com/openOMSI-org/openOMSI/pull/4090), [#4024](https://github.com/openOMSI-org/openOMSI/pull/4024), [#3876](https://github.com/openOMSI-org/openOMSI/pull/3876), [#3914](https://github.com/openOMSI-org/openOMSI/pull/3914), [**@szambonurek-88**](https://github.com/szambonurek-88), [**@ImperialGaming4**](https://github.com/ImperialGaming4), [**@Shiko215**](https://github.com/Shiko215)).
- **Cab:** with self-cancelling indicators off, no phantom cancel click plays, and the pause menu's Line and shift page draws its next arrow ([#3872](https://github.com/openOMSI-org/openOMSI/pull/3872), [#4012](https://github.com/openOMSI-org/openOMSI/pull/4012), [**@szambonurek-88**](https://github.com/szambonurek-88), [**@rafaelkaiz**](https://github.com/rafaelkaiz)).
- **Driving rating:** harsh braking and turns count only with passengers aboard ([#4000](https://github.com/openOMSI-org/openOMSI/pull/4000), [**@szambonurek-88**](https://github.com/szambonurek-88)).
- **See the bus before you place it.** The in-game bus chooser shows the launcher's 3D preview when placing or swapping a vehicle and when changing livery, and the Alt menu's thirty tiles are flatter and squarer, with pictograms of their own in OMSI 2's style ([#3814](https://github.com/openOMSI-org/openOMSI/pull/3814), [#4121](https://github.com/openOMSI-org/openOMSI/pull/4121), [**@shamough1792**](https://github.com/shamough1792), [**@isaacsa2**](https://github.com/isaacsa2), [**@rafaelkaiz**](https://github.com/rafaelkaiz)).
- **Deleting a map frees its objects and splines** that no other map uses, and keeps the ones another map still needs ([#3848](https://github.com/openOMSI-org/openOMSI/pull/3848), [**@thibautgnt**](https://github.com/thibautgnt)).
- **A button keeps the phone's screen on**, and on Windows a one-click button lets the phone through the firewall ([#3823](https://github.com/openOMSI-org/openOMSI/pull/3823), [#3803](https://github.com/openOMSI-org/openOMSI/pull/3803), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Alt menu redone**: the quick menu's thirty tiles are flat, squarer and have pictograms of their own in the style of OMSI 2's menu (a bus with a plus, a cross or arrows for placing, removing and swapping it, a pump, a clock, a chequered flag...) in place of the generic symbols.
- **Tunnel mouths are open**: a `[terrainhole]` standing upright, as in the Kartoffelphantom tram tunnels, is an opening: the ground is cut behind it up to where the hill is over it, and no wall stands across it. The walls of a cut in the ground show the ground's texture instead of streaks.
- **No sun inside tunnels**: a spline with a profile as tall as a storey (2 m or more) casts its shadow wherever it lies, also when it is buried in the ground.
- **Passengers use every car of a train**: a car coupled turned round, back to back with the car in front of it, is joined into the passenger cabin turned round, so people get in and out by its own doors and ride in it; and the doors of a car that runs its own scripts (the NF6D's sections) open for the passengers and for the player on foot by that car's own `PAX_Entry<n>_Open`.
- **`GetHeightAbovePoint` as in Omsi.exe**: the ray starts 3 m over the point, meets a wire or a wall top on a narrow height profile, and reads 1000 with nothing below it (it read 0, which held the Zweiachser tram's pantograph at the top of its travel).
- **Every car of a train rocks on its own springs**: each car's body leans out of bends by the force on it and rocks on the rails, with `[rail_body_osc]`'s lever, frequency and decay where the file gives them and the class's springs where it switches them off. Joints go with the cars around them; bogies, couplings and wheel sets stay level. **This changes the default**: the lean is now always on, also without the realistic rail physics (which still add the dip under braking and the bounce), and a car shows half of the lean its springs work out (`ROLL_SHOWN`).
- **Drive a tram from either cab**: in a train with a cab at each end (the NF6D) walk to the other cab and take its wheel (G), or switch driving ends from the outside view with the interior-camera keys. That cab's own scripts then take the keys, the switches and the indicator, and its cameras, driver figure, sounds, roller blinds and destination displays are its own. Every car with scripts of its own pulls and brakes, and the cars talk over their control cables as in OMSI 2. Camera changes glide inside a coupled part too.
- **Backing through points keeps the train together**: backing, the vehicle follows the track its train lies on, and the train's rear end takes the way the points are set (or the rear cab's indicator says) and throws them.
- **New key J, set the points**: J sets the first points within 50 m ahead of the train's leading end to their next branch (the rear end when driving from the rear cab or backing). It is openOMSI's own key; OMSI 2 has none for it.
- **Plugins can lay cards of their own over the navigator's map**: a plugin that fills the navigator's frame (`frame = true`) can put small cards of its own on the map - the next stop, the speed, whatever it shows - with the `map` element's new `overlay` key, at any corner or edge of the map and held inside it (what crosses the map's edge is left out, the card's corners stay rounded); their buttons take clicks as in the rest of the panel (`omsi.ui.version` 8). An example is `docs/examples/plugins/navigator_map.lua`.

### Fixes

- **Sharper and steadier on full graphics memory.** Textures near the bus no longer turn blurry for good on a card near its limit, the game no longer closes when the shadow maps do not fit (the shadows get coarser instead), and graphics built into the processor no longer run out of memory on big maps ([#3844](https://github.com/openOMSI-org/openOMSI/pull/3844), [#3890](https://github.com/openOMSI-org/openOMSI/pull/3890), [#4050](https://github.com/openOMSI-org/openOMSI/pull/4050), [**@thibautgnt**](https://github.com/thibautgnt), [**@turbo-devv**](https://github.com/turbo-devv)).
- **mX Graphics (experimental).** Settings → Graphics → Display → *mX Graphics (Experimental)* runs the whole game from the next start on [mX](https://github.com/openOMSI-org/mX), openOMSI's own graphics library, instead of wgpu. It is offered on every system and off by default; with it off, nothing of mX is started ([#3973](https://github.com/openOMSI-org/openOMSI/pull/3973), [#4080](https://github.com/openOMSI-org/openOMSI/pull/4080), [**@turbo-devv**](https://github.com/turbo-devv)).
- **Detailed trees:** leaf pictures no longer squeeze into green streaks when seen side-on, and tree crowns are shaded like a volume ([#3826](https://github.com/openOMSI-org/openOMSI/pull/3826), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Fogged-up windows stay on the glass on a slope** instead of sliding up or down with the hill ([#4049](https://github.com/openOMSI-org/openOMSI/pull/4049), [**@turbo-devv**](https://github.com/turbo-devv)).
- **Start-up and crashes:** the shadow pipeline builds on OpenGL ES and ANGLE, bad material numbers in a model no longer end the game, and a lost graphics device no longer crashes the lamp checks ([#4091](https://github.com/openOMSI-org/openOMSI/pull/4091), [#3998](https://github.com/openOMSI-org/openOMSI/pull/3998), [#3924](https://github.com/openOMSI-org/openOMSI/pull/3924), [**@szambonurek-88**](https://github.com/szambonurek-88)).
- **A new sky and new weather.** Rain and snow on the glass are one model in Vanilla, Vanilla+ and Enhanced, with drops, runners and snow on the panes. Enhanced gets volumetric clouds that no longer jump from frame to frame and volumetric fog (on by default, *Volumetric fog*), and the far rain and snow veil stays at the horizon. The weather menu's visibility goes down to 50 m again, as in OMSI 2 ([#4140](https://github.com/openOMSI-org/openOMSI/pull/4140), [#4142](https://github.com/openOMSI-org/openOMSI/pull/4142), [#4015](https://github.com/openOMSI-org/openOMSI/pull/4015), [**@turbo-devv**](https://github.com/turbo-devv), [**@ServerBot-code**](https://github.com/ServerBot-code)).
- **Scripts:** scripts with several distinct `{frame}` blocks work again, Volvo Wright doors follow their script's timing, a held brake no longer locks the bus with a stop-brake interlock, and more gear lever names are found ([#4088](https://github.com/openOMSI-org/openOMSI/pull/4088), [#3889](https://github.com/openOMSI-org/openOMSI/pull/3889), [#4087](https://github.com/openOMSI-org/openOMSI/pull/4087), [#3791](https://github.com/openOMSI-org/openOMSI/pull/3791), [**@szambonurek-88**](https://github.com/szambonurek-88), [**@shamough1792**](https://github.com/shamough1792), [**@NeyvanSantos**](https://github.com/NeyvanSantos)).
- **Destinations and money:** route numbers such as R56 or CE19 reach the destination display, night and express lines match their HOF codes, the Euro cash desk's change levers give the right coins, and switching vehicles no longer carries over the old plate and fleet number ([#3874](https://github.com/openOMSI-org/openOMSI/pull/3874), [#3870](https://github.com/openOMSI-org/openOMSI/pull/3870), [#3963](https://github.com/openOMSI-org/openOMSI/pull/3963), [#3858](https://github.com/openOMSI-org/openOMSI/pull/3858), [**@szambonurek-88**](https://github.com/szambonurek-88), [**@shamough1792**](https://github.com/shamough1792)).
- Xbox and other XInput pads register their buttons in the launcher, the Linux D-pad is no longer upside down, a rebound or cleared pause key is respected, all mirrors show in the developer tools' list, DLSS installs in a OneDrive folder, and the route map of a big map no longer hangs the launcher ([#3877](https://github.com/openOMSI-org/openOMSI/pull/3877), [#3962](https://github.com/openOMSI-org/openOMSI/pull/3962), [#3869](https://github.com/openOMSI-org/openOMSI/pull/3869), [#3871](https://github.com/openOMSI-org/openOMSI/pull/3871), [#4093](https://github.com/openOMSI-org/openOMSI/pull/4093), [#4089](https://github.com/openOMSI-org/openOMSI/pull/4089), [**@szambonurek-88**](https://github.com/szambonurek-88)).
- **OmniNavigation without its config file** no longer freezes loading or stops passengers from boarding ([#4084](https://github.com/openOMSI-org/openOMSI/pull/4084), [**@szambonurek-88**](https://github.com/szambonurek-88)).
- **A broken sound file is named in the log** with its path, size and first bytes ([#3978](https://github.com/openOMSI-org/openOMSI/pull/3978), [**@turbo-devv**](https://github.com/turbo-devv)).
- **Passengers fill the stops in free drive**, as in OMSI 2, instead of leaving every stop empty without a duty. Passengers on board are still in the bus after loading a save. Maps and players also get new passenger settings, all keeping OMSI 2's behaviour by default: wait until the bus stands with a door open (`pax_board_when_stopped`), pay in cash without a ticket (`pax_no_ticket`) and how late the bus may be before they complain (`pax_late_tolerance_min`) ([#4114](https://github.com/openOMSI-org/openOMSI/pull/4114), [#4111](https://github.com/openOMSI-org/openOMSI/pull/4111), [#4110](https://github.com/openOMSI-org/openOMSI/pull/4110), [#4109](https://github.com/openOMSI-org/openOMSI/pull/4109), [#3831](https://github.com/openOMSI-org/openOMSI/pull/3831), [**@szambonurek-88**](https://github.com/szambonurek-88), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Traffic lights:** vehicle-actuated lights no longer stick on yellow, AI no longer slips past a red light through a short lane, and green with yellow lets traffic go ([#3859](https://github.com/openOMSI-org/openOMSI/pull/3859), [#3866](https://github.com/openOMSI-org/openOMSI/pull/3866), [#3882](https://github.com/openOMSI-org/openOMSI/pull/3882), [**@websterzh**](https://github.com/websterzh), [**@ponponjo**](https://github.com/ponponjo), [**@dima-au**](https://github.com/dima-au)).
- **AI buses:** they no longer lock each other out at stop berths, and a timetable bus waiting out its layover is no longer removed as stuck ([#3960](https://github.com/openOMSI-org/openOMSI/pull/3960), [#3903](https://github.com/openOMSI-org/openOMSI/pull/3903), [**@szambonurek-88**](https://github.com/szambonurek-88), [**@Lincoln-Muller**](https://github.com/Lincoln-Muller)).
- **Passengers:** right-hand-drive buses pick up at ordinary kerb stops again, passengers wait when the sign shows another service, and pass-through stations are skipped as the next stop ([#4085](https://github.com/openOMSI-org/openOMSI/pull/4085), [#3805](https://github.com/openOMSI-org/openOMSI/pull/3805), [#3833](https://github.com/openOMSI-org/openOMSI/pull/3833), [**@szambonurek-88**](https://github.com/szambonurek-88), [**@FakeShxp3**](https://github.com/FakeShxp3), [**@Tom021212**](https://github.com/Tom021212)).
- **Release notes in What's new show tables and pictures** instead of their markup ([#3810](https://github.com/openOMSI-org/openOMSI/pull/3810), [**@kizule**](https://github.com/kizule)).
- Large maps and missing depot files no longer hang the timetable, the launcher's timetable shows the Chrono events of the chosen date, a save remembers the exact `.hof` file, a save with no player vehicle loads, and the timetable editor's line list no longer collapses on a short window ([#3959](https://github.com/openOMSI-org/openOMSI/pull/3959), [#3875](https://github.com/openOMSI-org/openOMSI/pull/3875), [#3873](https://github.com/openOMSI-org/openOMSI/pull/3873), [#3868](https://github.com/openOMSI-org/openOMSI/pull/3868), [#3845](https://github.com/openOMSI-org/openOMSI/pull/3845), [**@szambonurek-88**](https://github.com/szambonurek-88), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Ground:** tunnel mouths are open, the inside of tunnels gets no sun, and the end of a road at Falkensee Bahnhof no longer raises towers of ground ([#4023](https://github.com/openOMSI-org/openOMSI/pull/4023), [#3773](https://github.com/openOMSI-org/openOMSI/pull/3773), [**@ImperialGaming4**](https://github.com/ImperialGaming4), [**@rafaelkaiz**](https://github.com/rafaelkaiz)).
- **Scenery:** the static buses of Praha 200 show their outer skin, and people waiting at a stop stand on the pavement instead of in it ([#3902](https://github.com/openOMSI-org/openOMSI/pull/3902), [#3908](https://github.com/openOMSI-org/openOMSI/pull/3908), [**@rafaelkaiz**](https://github.com/rafaelkaiz)).
- The speed and speed limit stay on the minimap with a destination set, the route line keeps up with the bus, and the speed-limit sign looks the same in the navigator and the information bar ([#3933](https://github.com/openOMSI-org/openOMSI/pull/3933), [#3785](https://github.com/openOMSI-org/openOMSI/pull/3785), [#3950](https://github.com/openOMSI-org/openOMSI/pull/3950), [#3940](https://github.com/openOMSI-org/openOMSI/pull/3940), [**@shamough1792**](https://github.com/shamough1792)).
- **Rail vehicles no longer crash into platforms they clear**: on the rails each car of a tram or a train meets scenery with its own box, where that car stands on the track, in bends too. Above the top of a platform (0.35 m over the rails for a tram, 1.0 m for a train, 1.15 m for a metro) the box is the car's whole width, so a pole or a wall beside the body stops it. Below that only the strip over the track's middle counts (0.7 m either side), as the real body overhangs a platform's edge: a buffer stop still stops it, the platform no longer does. Other vehicles meet the leading car's whole body, side impacts included.
- **An AI tram's rear section follows the bend from the moment it appears**: the track behind a tram put on its rails is laid out from the lanes before it, so a KT4D's rear section no longer stands across the track until the tram has driven its own length.
- **Trams roll through tight curves**: a street tram's curve resistance is its own (about 10 N/kN in a 25 m curve) instead of a main-line car's, and the curve is taken over 12 m of track, so the kink where a switch's lanes meet no longer reads as the sharpest curve there is. A contact shoe on a car with a pantograph (the NF6D's) no longer makes it a metro car.
- **Mirrors on every car of a train**: a coupled car's `reflexionN` mirrors show its own mirror cameras, numbered on from car to car, instead of the first car's.
- **LAN: timetable buses no longer stay invisible on a client**: a scheduled bus or trolleybus with a long line (several pages of timetable) could stay "waiting for a context" on a client for good, because its pages started over each time its schedule state changed. Only the trip itself is paged now, every page of a new trip goes out quickly, and the client keeps the last complete trip until the new one is whole. Both players need this version (LAN protocol 18) ([#3957](https://github.com/openOMSI-org/openOMSI/pull/3957)).

### Performance

- **Lighter settings for small computers:** 300 m view and object distances and 256 and 512 shadow maps; the Low preset uses 300 m and a 512 shadow map ([#4126](https://github.com/openOMSI-org/openOMSI/pull/4126), [**@isaacsa2**](https://github.com/isaacsa2)).

### Multiplayer

- **Parked buses on servers.** A dedicated server can keep the buses players sent standing parked in its world, also after a restart, for any player to walk up to and take (`parked_buses` in `server.cfg`, off by default). The launcher's public server list loads again, and servers with `public_server = 1` can list themselves ([#3899](https://github.com/openOMSI-org/openOMSI/pull/3899), [#4053](https://github.com/openOMSI-org/openOMSI/pull/4053), [**@fsalexandre**](https://github.com/fsalexandre), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Timetable buses on long lines show on clients** instead of waiting for good; this needs LAN protocol 18 on both sides ([#4042](https://github.com/openOMSI-org/openOMSI/pull/4042), [**@turbo-devv**](https://github.com/turbo-devv)).

### Project

- **Plugins can lay cards of their own over the navigator's map** with the `map` element's `overlay` key (`omsi.ui.version` 8); see `docs/examples/plugins/navigator_map.lua` ([#4068](https://github.com/openOMSI-org/openOMSI/pull/4068), [**@Luc-nbr**](https://github.com/Luc-nbr)).
- **Plugins see the AI vehicles' size:** `omsi.traffic.list()`, `get`, `nearest` and `ahead` give each vehicle's `bounding_box` ([#3979](https://github.com/openOMSI-org/openOMSI/pull/3979), [**@turbo-devv**](https://github.com/turbo-devv)).
- **Plugins can ask where a vehicle's body and a point of its meshes are** with `omsi.body_frame`, `omsi.mesh_point` and their `other_` versions for nearby vehicles ([#3737](https://github.com/openOMSI-org/openOMSI/pull/3737), [**@dima-au**](https://github.com/dima-au)).
- **GPS screens in the cockpit get more options** in `[navigatortexture]`: map scale, north up, no navigation overlay, centred bus and the trip so far, for screens such as passenger information panels ([#3892](https://github.com/openOMSI-org/openOMSI/pull/3892), [**@georgefung892**](https://github.com/georgefung892)).
- **Trolleybus scripts can react to wire crossings and frogs** the pole's shoe passes ([#3941](https://github.com/openOMSI-org/openOMSI/pull/3941), [**@GrenManSK**](https://github.com/GrenManSK)).
- **A trolleybus can keep the game's pole gear off its poles** with an empty `[trolley_none]` block, for mods whose own script moves them ([#3813](https://github.com/openOMSI-org/openOMSI/pull/3813), [**@dima-au**](https://github.com/dima-au)).
- **Third rail and curve squeal for rail vehicles:** contact shoes read the `[third_rail]` under them, and bogies report the curve and the flange (`boogie_N_invradius`, `boogie_N_wheel_at_limit`) ([#4026](https://github.com/openOMSI-org/openOMSI/pull/4026), [**@ImperialGaming4**](https://github.com/ImperialGaming4)).
- Formatting checks, translations in one file per language, tidier code and tests, steadier CI and release builds without compiler warnings, and an up-to-date README and website ([#3930](https://github.com/openOMSI-org/openOMSI/pull/3930), [#3934](https://github.com/openOMSI-org/openOMSI/pull/3934), [#4060](https://github.com/openOMSI-org/openOMSI/pull/4060), [#4064](https://github.com/openOMSI-org/openOMSI/pull/4064), [#4066](https://github.com/openOMSI-org/openOMSI/pull/4066), [#4094](https://github.com/openOMSI-org/openOMSI/pull/4094), [#3935](https://github.com/openOMSI-org/openOMSI/pull/3935), [#3820](https://github.com/openOMSI-org/openOMSI/pull/3820), [#3865](https://github.com/openOMSI-org/openOMSI/pull/3865), [#3799](https://github.com/openOMSI-org/openOMSI/pull/3799), [#4129](https://github.com/openOMSI-org/openOMSI/pull/4129), [#4143](https://github.com/openOMSI-org/openOMSI/pull/4143), [#3992](https://github.com/openOMSI-org/openOMSI/pull/3992), [**@trolley813**](https://github.com/trolley813), [**@paulbmueller**](https://github.com/paulbmueller), [**@ImperialGaming4**](https://github.com/ImperialGaming4), [**@Luc-nbr**](https://github.com/Luc-nbr), [**@shamough1792**](https://github.com/shamough1792), [**@thibautgnt**](https://github.com/thibautgnt), [**@isaacsa2**](https://github.com/isaacsa2), [**@turbo-devv**](https://github.com/turbo-devv)).

### Credits

- [**@szambonurek-88**](https://github.com/szambonurek-88), for passengers in free drive, the new passenger settings, a phone fleet number, and more than twenty fixes to scripts, rail switches, destinations, cash desks, controllers, timetables and the launcher.
- [**@ImperialGaming4**](https://github.com/ImperialGaming4), for trams and trains that drive from either cab, the J key, passengers in every car, body sway, third rail and curve squeal, rail collisions and open tunnel mouths.
- [**@turbo-devv**](https://github.com/turbo-devv), for the weather overhaul, mX Graphics, controls profiles, the integrated-graphics memory cap, condensation on slopes, AI vehicle sizes for plugins, sound-file logging and keeping the release builds green.
- [**@shamough1792**](https://github.com/shamough1792), for in-game bus and livery previews, graphics profile import and export, navigator and speed-limit sign fixes, display and door fixes.
- [**@rafaelkaiz**](https://github.com/rafaelkaiz), for the Alt menu redesign, the OMSI 2 driver seat, terrain, mesh, display and pavement fixes.
- [**@isaacsa2**](https://github.com/isaacsa2), for restoring the Alt menu redesign, 300 m distances and small shadow maps, compact lamp halos and the mX build fixes.
- [**@fsalexandre**](https://github.com/fsalexandre), for parked buses on servers and more memory on phones and tablets.
- [**@Luc-nbr**](https://github.com/Luc-nbr), for plugin cards over the navigator's map and the code split behind it.
- [**@dima-au**](https://github.com/dima-au) and [**@GrenManSK**](https://github.com/GrenManSK), for vehicle poses for Lua plugins, `[trolley_none]`, green-with-yellow lights and wire-crossing triggers.
- [**@trolley813**](https://github.com/trolley813) and [**@paulbmueller**](https://github.com/paulbmueller), for map climates, formatting checks and one translation file per language.
- [**@georgefung892**](https://github.com/georgefung892), [**@czyun44**](https://github.com/czyun44), [**@kizule**](https://github.com/kizule) and [**@NeyvanSantos**](https://github.com/NeyvanSantos), for GPS screen options, force feedback sliders, release notes that render, and gear lever names.
- [**@websterzh**](https://github.com/websterzh), [**@ponponjo**](https://github.com/ponponjo), [**@Lincoln-Muller**](https://github.com/Lincoln-Muller), [**@Tom021212**](https://github.com/Tom021212), [**@FakeShxp3**](https://github.com/FakeShxp3), [**@Shiko215**](https://github.com/Shiko215) and [**@ServerBot-code**](https://github.com/ServerBot-code), for traffic-light, AI lane, layover, pass-through stop, boarding, AI train and visibility fixes.
- Thibaut ([**@thibautgnt**](https://github.com/thibautgnt)), for Detailed trees, sharp textures and steadier shadows on full graphics cards, renaming saves, freeing map space, the public server list, the remote cockpit fixes, the F3 camera, passengers kept in saves.
- **Full changelog:** [0.3.55 → 0.3.56](https://github.com/openOMSI-org/openOMSI/compare/v0.3.55...v0.3.56) · [All changes since stable 0.3.52](https://github.com/openOMSI-org/openOMSI/compare/v0.3.52...v0.3.56).

## 0.3.55 - 2026-11-21

Mac players can try the experimental mX renderer or use MetalFX frame generation on supported hardware. The launcher can switch controller profiles for each bus. This beta also addresses texture memory on integrated GPUs, condensation on hills and several scenery issues.

### New and improved

- **A sharper picture with Apple MetalFX upscaling**: the textures are read one level sharper and the picture is sharpened a little more, so bark, grass and the numbers of the gauges come closer to the picture drawn at the window's size, without halos around the lamp posts at night.
- **Controls profiles**: the launcher's Controls page has profiles, each with every key and every controller setting: *Duplicate* makes a new one of the current settings, the list switches between them, and they can be renamed and deleted. One can be the *default for every bus*, and a bus can have its own (*Use for* the bus chosen on the Drive page): before each drive the launcher switches to the bus's profile, or to the default one ([#3921](https://github.com/openOMSI-org/openOMSI/pull/3921)).
- **Force feedback sliders in the game's options**: a device's Force feedback tab in the game's controller settings now shows steering force, vibration, self-centring, damping, parking resistance, minimum force and smoothing as sliders like the launcher's, which you can drag as well as step with Left and Right.
- **The driver can sit as in OMSI 2**: Settings → Camera → Driver's view (and the options menu in the game) has *Driver sits as in OMSI 2*, off by default. On, the driver sits back on his seat with his hands off the wheel, as in OMSI 2; off, he moves up and leans forward to hold the wheel, as before. It can be switched while driving.
- **The next arrow shows in the pause menu's Line and shift page**: the button beside the start time (and the next arrow of the vehicle pages) was an empty square, as the game had no `arrow_forward` icon to draw on it.

### Fixes

- **mX Graphics (Experimental)** on a Mac: Settings → Graphics → Display → *mX Graphics (Experimental)* draws the game on Metal with [mX](https://github.com/openOMSI-org/mX), openOMSI's own graphics library, instead of wgpu, from the next start. The picture is meant to be the same. It is an early step: the game's shaders do not use mX's bindless textures yet, and in the tests so far it spent less processor time on a frame than wgpu and the same time on the graphics chip. Ray tracing and frame generation stay off with it; where mX cannot open the graphics chip, the game draws with wgpu as before. Off by default. mX's Direct3D 12 and Vulkan have not drawn the game on a real graphics card yet, so Windows and Linux do not offer the switch; `OMSI_MX=on` tries them for one run (without DLSS, FSR 3 and VR), and `OMSI_MX=off` turns mX off for a run on a Mac.
- **Graphics in the processor (Intel UHD, AMD APUs) no longer run out of memory on big maps**: their texture allowance came from what the driver may hand out in all, up to 30 % of it, which on a 16 GB machine was 2.2 GB of textures beside a big map and the game itself, and the device was lost. It is now a tenth of the machine's memory at most (at least 1000 MB) ([#2720](https://github.com/openOMSI-org/openOMSI/pull/2720)).
- **Fogged-up windows stay on the glass on a slope**: the condensation on the bus's panes was laid out level with the world, so going up or down a hill it slid down or up the glass, and the defroster's cleared fan with it. It now follows the bus's pitch and roll ([#3958](https://github.com/openOMSI-org/openOMSI/pull/3958)).
- **Pixel XC list rows no longer cut off**: on the MZKP SOR NBG 12, the direction and line lists of the Pixel XC display showed each row's text sliced along its middle. The text strip of every row is skewed a tenth of a millimetre over its tile, and was not taken as printed on it, so its upper half lost the depth test; such a strip is now recognised.
- **People waiting at a stop no longer sink into the pavement**: a waiting place that the map puts below the footpath (the standing people of MZKP Strzelce's "Starostwo Powiatowe" are at z 0, the pavement is 23 cm up) now has its person lifted onto the pavement, up to a step above the place; the floor is never lowered, and somebody on a seat keeps its bench's height.
- **Static buses of Praha 200 no longer lose their outer skin**: a mesh from the Blender exporter was turned round as a mirrored one when its matrix (the quarter turn about x that thousands of ordinary meshes carry) was positive and its normals faced against its faces, so the IVECO buses at Letnany station showed their inside; Omsi.exe draws such a mesh as wound.

### Performance

- **Frame generation on a Mac**: with Apple MetalFX upscaling on, Settings → Graphics → *Frame generation* shows a generated frame between every two drawn ones (Apple's MetalFX frame interpolator; a Mac with Apple silicon on macOS 26 or newer), for smoother motion at the price of a little more input lag. Where fewer than 30 frames a second are drawn with it (Apple's advice), it is turned off for the drive, as SSAO and the shadows are. The frame rate's corner then reads e.g. `90 fps (45 + FG)`. Off by default, experimental.

### Project

- **Plugins see the AI vehicles' size**: `omsi.traffic.list()`, `get`, `nearest` and `ahead` give each vehicle's `bounding_box` (`{width, length, height, cx, cy, cz, heading, default}`, the .bus/.ovh `[boundingbox]`), so a blind-spot or distance plugin knows where a truck's or an articulated bus's body really is, not only its origin ([#3936](https://github.com/openOMSI-org/openOMSI/pull/3936)).
- **A broken sound file is named in the log**: when no sound can be read from a file under a sound's name (a cut-off download, a web page), the warning now gives its path, its size and its first bytes. Before, the log only had the line `probe reach EOF at n bytes` with no file name, which read like the cause of a crash ([#3956](https://github.com/openOMSI-org/openOMSI/pull/3956)); that line is still logged just before ours.

### Credits

- [**@czyun44**](https://github.com/czyun44), for the in-game force feedback sliders.
- [**@turbo-devv**](https://github.com/turbo-devv), for keeping condensation aligned with the windscreen on hills.
- **Full changelog:** [0.3.54 → 0.3.55](https://github.com/openOMSI-org/openOMSI/compare/v0.3.54...v0.3.55).

## 0.3.54 - 2026-11-11

Apple silicon Macs can use MetalFX upscaling. You can rename saves in the launcher, and the F3 camera starts behind the bus. This beta also fixes traffic lights that could stick on yellow and tall terrain spikes at the end of a road.

### New and improved

- **Rename a save**: the pencil beside a save slot in the launcher's *Continue* list gives it a name of your own, without editing its `.osn` file in the map's `Saves` folder. Saving over that slot in the game keeps the name ([#3849](https://github.com/openOMSI-org/openOMSI/pull/3849)).
- **No more towers of ground at the end of a road**: where the ground is fitted to a road, the side of a kerb - a nearly vertical face - gave the blend a slope of tens of thousands, and the ground rose more than a kilometre in thin strips at the end of the road (Berlin-Spandau, Hansastrasse near Falkensee Bahnhof). The slope a rim hands to the ground is now limited to about 72 degrees.

### Fixes

- **The outside view (F3) starts straight behind the bus**, a little above it, as in OMSI 2, instead of three quarters from the side. It still keeps the angle you leave it at, and Space puts it back behind the bus.
- **Vehicle-actuated traffic lights wait before the stop, not in the yellow**: a stop whose request is present now holds the clock at the phase before it, so a queue can no longer leave a light stuck on yellow; an exact stop time still holds, and a jump at the same time is taken first. This fixes the traffic light at Ahlheim V5's Eichenhöhe Bahnhof ([#2947](https://github.com/openOMSI-org/openOMSI/issues/2947)).

### Performance

- **Apple MetalFX upscaling on a Mac**: Settings → Graphics → Upscaling offers *Apple MetalFX* on a Mac with Apple silicon (macOS 13 or newer), with the same quality levels as NVIDIA DLSS and AMD FSR 3 on Windows: the picture is drawn smaller and brought up to the window with the help of the last frames, in place of the anti-aliasing and the render scale, from the next start. It is part of macOS, so there is nothing to install. Where the Mac's graphics chip scales up less than Ultra performance asks for, the picture is drawn at the smallest size it takes.

### Credits

- [**@realmaitreal**](https://github.com/realmaitreal), for Apple MetalFX upscaling.
- [**@thibautgnt**](https://github.com/thibautgnt), for renaming saves in the launcher and the outside camera's starting position.
- [**@websterzh**](https://github.com/websterzh), for fixing the traffic-light stop phase.
- **Full changelog:** [0.3.52 → 0.3.54](https://github.com/openOMSI-org/openOMSI/compare/v0.3.52...v0.3.54).

## 0.3.52 - 2026-11-07

Stable release: the tested betas 0.3.46 to 0.3.51 promoted. Their changes are listed under each beta below; the list here is the roll-up as it was published with this release.

<img width="1600" alt="What's new in openOMSI 0.3.52" src="https://github.com/user-attachments/assets/560783ae-7534-49b4-bf5e-980b0271618e" />

**Stable release.** This is the version the launcher now offers every player. It brings together everything from the 0.3.46 to 0.3.51 betas and a few last additions: DLSS and FSR 3 upscaling, a large frame-rate recovery, a phone or tablet as part of the cockpit, snow that builds up on roads and roofs, your depot buses in the game, and a long list of multiplayer, traffic and vehicle fixes. Online play uses a new protocol: players and servers need this version to play together. openOMSI needs an original OMSI 2 installation to run; it contains no game content.

### New and improved

- **NVIDIA DLSS and AMD FSR 3 upscaling (experimental, Windows).** Settings → Graphics → Upscaling offers DLSS on RTX cards and FSR 3 on any card, on DirectX 12 and Vulkan. FSR 3 ships with the game; DLSS is installed from the launcher (Settings → Graphics → **Install NVIDIA DLSS**) after accepting NVIDIA's licence. Both are off in VR and with triple screens ([#3681](https://github.com/openOMSI-org/openOMSI/pull/3681), [**@m3rlinek01**](https://github.com/m3rlinek01), [**@isaacsa2**](https://github.com/isaacsa2)).
- **More frames per second.** Integrated graphics get the frame rate of 0.2.30 back, the game draws faster from the driver's seat with MSAA, Enhanced runs faster on AMD cards with Vulkan, and on Windows with an AMD Radeon the Auto setting now opens Vulkan. Rain no longer halves the frame rate on integrated graphics ([#3521](https://github.com/openOMSI-org/openOMSI/pull/3521), [#3713](https://github.com/openOMSI-org/openOMSI/pull/3713), [#3663](https://github.com/openOMSI-org/openOMSI/pull/3663), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Remote cockpit.** A phone or tablet on the home Wi-Fi becomes part of the cockpit: a dashboard with the map, next stops, speed and delay, the bus's own buttons, its destination sign, a touch steering wheel and pedals, and a layout editor. Switch it on in Esc → Options → Interface → Remote cockpit and scan the QR code; nothing to install. The map can look exactly like the game's navigator, and the phone's screen stays on. Meant for trusted home networks only ([#3695](https://github.com/openOMSI-org/openOMSI/pull/3695), [#3770](https://github.com/openOMSI-org/openOMSI/pull/3770), [#3753](https://github.com/openOMSI-org/openOMSI/pull/3753), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Snow builds up, and traffic leaves tracks.** Roads and roofs go from a dusting to full cover, wheels press tracks into fresh snow, traffic leaves slushy ruts, and roof snow blows off and thaws. Off by default: switch on **Built-up snow on roads and roofs** in the Graphics settings ([#3237](https://github.com/openOMSI-org/openOMSI/pull/3237), [#3481](https://github.com/openOMSI-org/openOMSI/pull/3481), [**@Bulekoo**](https://github.com/Bulekoo), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Passengers look and move like everyday people.** With **Character models: High quality**, everyday clothes are the most common, and passengers walk, stand and sit with real motion-capture poses, each with their own pace and stance. The updated avatar pack downloads by itself ([#3764](https://github.com/openOMSI-org/openOMSI/pull/3764), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Trolleybus poles by hand, and radio-controlled switches.** From outside, take a pole by its rope and put the shoe on the wire; Ctrl+R puts both poles up or down. Add-ons get radio-controlled switches with cab requests, route codes and state displays, and the pole's shoe can swivel along the wire ([#3710](https://github.com/openOMSI-org/openOMSI/pull/3710), [#3624](https://github.com/openOMSI-org/openOMSI/pull/3624), [#3670](https://github.com/openOMSI-org/openOMSI/pull/3670), [#3742](https://github.com/openOMSI-org/openOMSI/pull/3742), [**@GUMOLIT**](https://github.com/GUMOLIT)).
- **Go anywhere from the city map, and find a street by name.** Alt+click a place on the city map to stand there on foot, or Ctrl+drag to place the bus facing the way you drag. The game menu has **Find a street...**, which opens the city map centred on it ([#3575](https://github.com/openOMSI-org/openOMSI/pull/3575), [#3564](https://github.com/openOMSI-org/openOMSI/pull/3564), [#3645](https://github.com/openOMSI-org/openOMSI/pull/3645), [#3653](https://github.com/openOMSI-org/openOMSI/pull/3653), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Graphics changes apply while driving**, with 20 seconds to confirm before the old values come back. New sliders: **Road glare** (Enhanced, also tones down night-time highlights) and **Traffic headlight brightness** ([#3535](https://github.com/openOMSI-org/openOMSI/pull/3535), [#3658](https://github.com/openOMSI-org/openOMSI/pull/3658), [**@isaacsa2**](https://github.com/isaacsa2), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Interface themes from mods and plugins:** Options → Interface → **Theme pack** can recolour the menus, show a logo, turn panels into frosted glass and style the loading screen ([#3278](https://github.com/openOMSI-org/openOMSI/pull/3278), [**@Luc-nbr**](https://github.com/Luc-nbr)).
- **Each graphics profile is a file of its own**, so it can be copied or shared by itself ([#3539](https://github.com/openOMSI-org/openOMSI/pull/3539), [**@rafaelkaiz**](https://github.com/rafaelkaiz)).
- **Waiting passengers stay visible farther down the road**, about twice as far as before ([#3693](https://github.com/openOMSI-org/openOMSI/pull/3693), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Bus mods can have a GPS screen in the cockpit**: a screen declared in `model.cfg` shows the navigator's map while the bus is powered ([#3706](https://github.com/openOMSI-org/openOMSI/pull/3706), [**@fsalexandre**](https://github.com/fsalexandre)).
- **A new livery without a new bus:** Vehicle options → Livery repaints the bus where it stands, keeping passengers, trip and damage ([#3644](https://github.com/openOMSI-org/openOMSI/pull/3644), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Zoom on a key** to read a far sign or the dashboard, held or toggled, 1.25x to 5x ([#3623](https://github.com/openOMSI-org/openOMSI/pull/3623), [**@thibautgnt**](https://github.com/thibautgnt)).
- **The flashlight can follow the cursor** (Options → Camera, off by default) ([#3648](https://github.com/openOMSI-org/openOMSI/pull/3648), [#3655](https://github.com/openOMSI-org/openOMSI/pull/3655), [#3687](https://github.com/openOMSI-org/openOMSI/pull/3687), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Mouse steering keeps the cursor in the window** on Windows and Linux ([#3594](https://github.com/openOMSI-org/openOMSI/pull/3594), [#3635](https://github.com/openOMSI-org/openOMSI/pull/3635), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Getting up from the seat has its own key**, Ctrl+Shift+G by default ([#3600](https://github.com/openOMSI-org/openOMSI/pull/3600), [**@thibautgnt**](https://github.com/thibautgnt)).
- **The view editor turns the head to the mirror being set** ([#3646](https://github.com/openOMSI-org/openOMSI/pull/3646), [#3657](https://github.com/openOMSI-org/openOMSI/pull/3657), [**@thibautgnt**](https://github.com/thibautgnt)).
- **F4 "Move here" faces the camera**, as in OMSI 2 ([#3421](https://github.com/openOMSI-org/openOMSI/pull/3421), [#3564](https://github.com/openOMSI-org/openOMSI/pull/3564), [**@rafaelkaiz**](https://github.com/rafaelkaiz), [**@thibautgnt**](https://github.com/thibautgnt)).
- **The Alt menu stays open** after a tile, as in OMSI 2, until Alt or Esc ([#3589](https://github.com/openOMSI-org/openOMSI/pull/3589), [**@thibautgnt**](https://github.com/thibautgnt)).
- **New Alt menu tiles:** Radio (station, volume, on/off) and Seat, view and mirrors ([#3580](https://github.com/openOMSI-org/openOMSI/pull/3580), [#3583](https://github.com/openOMSI-org/openOMSI/pull/3583), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Clean screen (Alt+U)** hides the whole interface while you drive ([#3558](https://github.com/openOMSI-org/openOMSI/pull/3558), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Choose which notes show** in the top-left corner ([#3599](https://github.com/openOMSI-org/openOMSI/pull/3599), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Only buses on the maps:** switch off *Cars on the map too* to see only AI buses and trolleybuses ([#3588](https://github.com/openOMSI-org/openOMSI/pull/3588), [**@thibautgnt**](https://github.com/thibautgnt)).
- **One Light/Dark theme** for the launcher and every game screen ([#3309](https://github.com/openOMSI-org/openOMSI/pull/3309), [**@shamough1792**](https://github.com/shamough1792), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Route arrows:** the dynamic arrows and the arrows placed by the map author have separate settings ([#3797](https://github.com/openOMSI-org/openOMSI/pull/3797), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Photo mode:** route arrows hide by themselves, the panel has a Hide (H) button, and Alt gives finer moves ([#3586](https://github.com/openOMSI-org/openOMSI/pull/3586), [#3598](https://github.com/openOMSI-org/openOMSI/pull/3598), [**@thibautgnt**](https://github.com/thibautgnt)).
- **More traffic for strong computers:** the Traffic slider goes up to 300 vehicles ([#3596](https://github.com/openOMSI-org/openOMSI/pull/3596), [**@thibautgnt**](https://github.com/thibautgnt)).
- **AI reaction time** setting (Settings → Traffic): a queue at a light can pull away closer together ([#3794](https://github.com/openOMSI-org/openOMSI/pull/3794), [**@czyun44**](https://github.com/czyun44)).
- **Optional passenger behaviour**, all off by default: passengers wave at approaching buses or turn away from full ones, change lines at nearby stops, and stand by a door on short rides ([#3427](https://github.com/openOMSI-org/openOMSI/pull/3427), [#3428](https://github.com/openOMSI-org/openOMSI/pull/3428), [#3429](https://github.com/openOMSI-org/openOMSI/pull/3429), [#3613](https://github.com/openOMSI-org/openOMSI/pull/3613), [**@isaacsa2**](https://github.com/isaacsa2), [**@thibautgnt**](https://github.com/thibautgnt)).
- **What's new:** after an update the launcher shows the notes of every version since the last one seen, and the update offer shows them before installing ([#3783](https://github.com/openOMSI-org/openOMSI/pull/3783), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Long drop-down lists filter as you type** ([#3590](https://github.com/openOMSI-org/openOMSI/pull/3590), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Add several AI vehicles at once** in Settings → AI Lists ([#3591](https://github.com/openOMSI-org/openOMSI/pull/3591), [**@thibautgnt**](https://github.com/thibautgnt)).
- **The launcher keeps its disk caches in check**, with a clean-up button in Troubleshooting ([#3535](https://github.com/openOMSI-org/openOMSI/pull/3535), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Keep today's year**, even outside the map's years (Settings → Gameplay → Simulation) ([#3658](https://github.com/openOMSI-org/openOMSI/pull/3658), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Disable plugins** (Settings → General) starts the game without plugins, for troubleshooting. It is on for brand-new installs for now; existing settings keep plugins as they were ([#3379](https://github.com/openOMSI-org/openOMSI/pull/3379), [#3481](https://github.com/openOMSI-org/openOMSI/pull/3481), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Xbox pads over Bluetooth LE work on Windows** ([#3499](https://github.com/openOMSI-org/openOMSI/pull/3499), [**@teamstarwars-dev**](https://github.com/teamstarwars-dev), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Force-feedback road rumble follows the road surface**: gentler on asphalt, stronger on cobbles and gravel ([#3658](https://github.com/openOMSI-org/openOMSI/pull/3658), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Each ticket stamper can have its own sound and script trigger** ([#3388](https://github.com/openOMSI-org/openOMSI/pull/3388), [**@tottichad**](https://github.com/tottichad), [**@turbo-devv**](https://github.com/turbo-devv)).
- **Trolley switches can start in a random position** with `initial random` in `trolley.txt` ([#3786](https://github.com/openOMSI-org/openOMSI/pull/3786), [**@GUMOLIT**](https://github.com/GUMOLIT)).
- **Maps can tune trolley sparks**, and built-in trolley sounds are muffled in a closed cab ([#3670](https://github.com/openOMSI-org/openOMSI/pull/3670), [**@GUMOLIT**](https://github.com/GUMOLIT)).
- **A plugin can load its DLL from beside its `.opl`** in a subfolder ([#3688](https://github.com/openOMSI-org/openOMSI/pull/3688), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Start-up:** DirectX 12 starts without DXC, OpenGL without compute shaders keeps its device, and unrelated updates keep the texture cache ([#3424](https://github.com/openOMSI-org/openOMSI/pull/3424), [#3663](https://github.com/openOMSI-org/openOMSI/pull/3663), [#3360](https://github.com/openOMSI-org/openOMSI/pull/3360), [**@VlastikYoutubeKo**](https://github.com/VlastikYoutubeKo), [**@thibautgnt**](https://github.com/thibautgnt), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Textures and text:** script-switched textures, grain-free cabs, Brazilian window films and CJK text on Fedora show correctly ([#3475](https://github.com/openOMSI-org/openOMSI/pull/3475), [#3572](https://github.com/openOMSI-org/openOMSI/pull/3572), [#3783](https://github.com/openOMSI-org/openOMSI/pull/3783), [#3687](https://github.com/openOMSI-org/openOMSI/pull/3687), [**@georgefung892**](https://github.com/georgefung892), [**@thibautgnt**](https://github.com/thibautgnt), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Scenery:** rivers flow, escalators move, far mountains and attach-row objects show, panoramas give way up close, `[isshadow]` meshes and Turkish file names work, and maps in other time zones get the right sun ([#3687](https://github.com/openOMSI-org/openOMSI/pull/3687), [#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [#3620](https://github.com/openOMSI-org/openOMSI/pull/3620), [#3425](https://github.com/openOMSI-org/openOMSI/pull/3425), [#3574](https://github.com/openOMSI-org/openOMSI/pull/3574), [#3573](https://github.com/openOMSI-org/openOMSI/pull/3573), [**@thibautgnt**](https://github.com/thibautgnt), [**@VlastikYoutubeKo**](https://github.com/VlastikYoutubeKo)).
- **Signals and arrows:** route arrows only where the route has a choice, scripted red-and-green lights, countdown lights that neither stutter nor run while paused ([#3584](https://github.com/openOMSI-org/openOMSI/pull/3584), [#3604](https://github.com/openOMSI-org/openOMSI/pull/3604), [#3611](https://github.com/openOMSI-org/openOMSI/pull/3611), [#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [#3663](https://github.com/openOMSI-org/openOMSI/pull/3663), [**@thibautgnt**](https://github.com/thibautgnt)).
- Dense maps stay whole with less video memory, editor-only roads and stop loops are followed, and the next stop matches the info bar ([#3481](https://github.com/openOMSI-org/openOMSI/pull/3481), [#3662](https://github.com/openOMSI-org/openOMSI/pull/3662), [#3671](https://github.com/openOMSI-org/openOMSI/pull/3671), [#3576](https://github.com/openOMSI-org/openOMSI/pull/3576), [#3612](https://github.com/openOMSI-org/openOMSI/pull/3612), [#3663](https://github.com/openOMSI-org/openOMSI/pull/3663), [#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [**@isaacsa2**](https://github.com/isaacsa2), [**@shamough1792**](https://github.com/shamough1792), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Scripts:** Valiant Citybus doors ([#1606](https://github.com/openOMSI-org/openOMSI/issues/1606), [**@turbo-devv**](https://github.com/turbo-devv)), double-speed `{frame}` blocks, `Wheel_Rotation`, pedals without the program folder, loud engine sounds and launcher options over repaints all work again ([#3663](https://github.com/openOMSI-org/openOMSI/pull/3663), [#3567](https://github.com/openOMSI-org/openOMSI/pull/3567), [#3658](https://github.com/openOMSI-org/openOMSI/pull/3658), [#3501](https://github.com/openOMSI-org/openOMSI/pull/3501), [**@thibautgnt**](https://github.com/thibautgnt), [**@Luc-nbr**](https://github.com/Luc-nbr)).
- **Articulated buses:** the Citelis 18 rear rides on its wheels, and walking in a rear section follows it ([#3661](https://github.com/openOMSI-org/openOMSI/pull/3661), [#3626](https://github.com/openOMSI-org/openOMSI/pull/3626), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Destinations and money:** route numbers with letters or signs, the Likino 677 blinds, hand-set destinations and change keys (crowns included) work ([#3616](https://github.com/openOMSI-org/openOMSI/pull/3616), [#3661](https://github.com/openOMSI-org/openOMSI/pull/3661), [#3538](https://github.com/openOMSI-org/openOMSI/pull/3538), [#3663](https://github.com/openOMSI-org/openOMSI/pull/3663), [#3511](https://github.com/openOMSI-org/openOMSI/pull/3511), [#3647](https://github.com/openOMSI-org/openOMSI/pull/3647), [**@thibautgnt**](https://github.com/thibautgnt), [**@insus100**](https://github.com/insus100)).
- **Cab:** the hand finds the gear lever, the paper timetable survives a resume, mirror cameras show, hidden parts can't be clicked, the radio key plays internet radio, and IVECO liveries with a leading space load ([#3582](https://github.com/openOMSI-org/openOMSI/pull/3582), [#3571](https://github.com/openOMSI-org/openOMSI/pull/3571), [#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [#3687](https://github.com/openOMSI-org/openOMSI/pull/3687), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Trams and trolleybuses:** two-cab trains drive from either end, rail wheels and switches behave, tram brakes and pantographs follow their scripts, retrieved poles go back on the wire, and charging trolleybuses leave frogs alone ([#3687](https://github.com/openOMSI-org/openOMSI/pull/3687), [#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [#3658](https://github.com/openOMSI-org/openOMSI/pull/3658), [#3480](https://github.com/openOMSI-org/openOMSI/pull/3480), [#3602](https://github.com/openOMSI-org/openOMSI/pull/3602), [**@thibautgnt**](https://github.com/thibautgnt), [**@m3rlinek01**](https://github.com/m3rlinek01)).
- **The world:** late buses find passengers, distant roads and car parks fill with cars, mirrors show cars up to 250 m, and an AI bus's inside is drawn and heard when you ride it ([#3658](https://github.com/openOMSI-org/openOMSI/pull/3658), [#3585](https://github.com/openOMSI-org/openOMSI/pull/3585), [#3663](https://github.com/openOMSI-org/openOMSI/pull/3663), [#3629](https://github.com/openOMSI-org/openOMSI/pull/3629), [#3661](https://github.com/openOMSI-org/openOMSI/pull/3661), [#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Timetables:** night tours, trip names with trailing spaces and duty entry points are handled correctly ([#3592](https://github.com/openOMSI-org/openOMSI/pull/3592), [#3658](https://github.com/openOMSI-org/openOMSI/pull/3658), [**@thibautgnt**](https://github.com/thibautgnt)).
- Driving layouts apply without a restart, the MOZA R5 gets force feedback, centred wheels are centred, arrow keys, PS4/PS5 triggers on Linux, fast mice and UDP head trackers behave, and the pause calendar picks dates ([#3630](https://github.com/openOMSI-org/openOMSI/pull/3630), [#3494](https://github.com/openOMSI-org/openOMSI/pull/3494), [#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [#3578](https://github.com/openOMSI-org/openOMSI/pull/3578), [#3617](https://github.com/openOMSI-org/openOMSI/pull/3617), [#3661](https://github.com/openOMSI-org/openOMSI/pull/3661), [#3627](https://github.com/openOMSI-org/openOMSI/pull/3627), [**@isaacsa2**](https://github.com/isaacsa2), [**@Luc-nbr**](https://github.com/Luc-nbr), [**@thibautgnt**](https://github.com/thibautgnt), [**@shamough1792**](https://github.com/shamough1792)).
- **AI reaction time**: the new `ai_reaction` setting (25 to 200 %, default 100 %, a slider in the launcher's Traffic settings) scales how long the AI drivers wait before moving off, so a queue at a light pulls away closer together at a lower value. It never goes under 0.4 s, the shortest reaction the junction code is built for ([#3793](https://github.com/openOMSI-org/openOMSI/pull/3793)).
- **No more slivers of sky at crossings of raised roads** (Nova Friburgo, since 0.3.42): where the ground cannot be fitted to roads it should follow, only the roads the ground covers (a ditch) are cut out of it, not those lying on it.

### Fixes

- **Coloured signs at night** and less sun-glare flicker behind the visor in Enhanced ([#3693](https://github.com/openOMSI-org/openOMSI/pull/3693), [**@thibautgnt**](https://github.com/thibautgnt)).
- **AI drivers have more varied personalities**, with patience at the horn and at amber lights of their own; they no longer drive through a red light they could stop for ([#3498](https://github.com/openOMSI-org/openOMSI/pull/3498), [#3797](https://github.com/openOMSI-org/openOMSI/pull/3797), [**@trolley813**](https://github.com/trolley813), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Tickets issued by hand lie on the desk again**, and the passenger takes the one they accepted ([#3525](https://github.com/openOMSI-org/openOMSI/pull/3525), [**@insus100**](https://github.com/insus100), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Timetable buses wait at their set-down stop** between trips instead of blocking the first stop ([#3315](https://github.com/openOMSI-org/openOMSI/pull/3315), [**@yu213272-coder**](https://github.com/yu213272-coder)).
- **Video memory:** closing the city map frees its memory, scenery and the player's bus keep their texture detail under pressure, advert displays no longer fill the card, Vulkan no longer freezes when memory runs out, and a texture with no memory left no longer ends the game ([#3756](https://github.com/openOMSI-org/openOMSI/pull/3756), [#3546](https://github.com/openOMSI-org/openOMSI/pull/3546), [#3551](https://github.com/openOMSI-org/openOMSI/pull/3551), [#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [#3661](https://github.com/openOMSI-org/openOMSI/pull/3661), [**@thibautgnt**](https://github.com/thibautgnt), [**@FakeShxp3**](https://github.com/FakeShxp3)).
- **Picture:** no coloured speckles on AMD, no tearing with vsync and a frame limit, no white discs on signs seen from behind, and a fixed render scale is sharp again at once ([#3499](https://github.com/openOMSI-org/openOMSI/pull/3499), [#3512](https://github.com/openOMSI-org/openOMSI/pull/3512), [#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [#3691](https://github.com/openOMSI-org/openOMSI/pull/3691), [#3687](https://github.com/openOMSI-org/openOMSI/pull/3687), [**@teamstarwars-dev**](https://github.com/teamstarwars-dev), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Lights:** calmer Enhanced headlights, no huge glow at Lower Marsh, and Vanilla headlights no longer light the bus's own front ([#3430](https://github.com/openOMSI-org/openOMSI/pull/3430), [#3426](https://github.com/openOMSI-org/openOMSI/pull/3426), [#3487](https://github.com/openOMSI-org/openOMSI/pull/3487), [**@FakeShxp3**](https://github.com/FakeShxp3), [**@rafaelkaiz**](https://github.com/rafaelkaiz), [**@georgefung892**](https://github.com/georgefung892)).
- **Ground:** tunnels, underpasses and box cuts cut the ground again, with no sky through Nova Friburgo's crossing and no pit near Falkensee Bahnhof ([#3565](https://github.com/openOMSI-org/openOMSI/pull/3565), [#3783](https://github.com/openOMSI-org/openOMSI/pull/3783), [#3772](https://github.com/openOMSI-org/openOMSI/pull/3772), [**@thibautgnt**](https://github.com/thibautgnt), [**@isaacsa2**](https://github.com/isaacsa2), [**@rafaelkaiz**](https://github.com/rafaelkaiz)).
- **Sound and displays:** map sounds are muffled in the cab, and missing font characters no longer shift a display ([#3563](https://github.com/openOMSI-org/openOMSI/pull/3563), [#3406](https://github.com/openOMSI-org/openOMSI/pull/3406), [**@thibautgnt**](https://github.com/thibautgnt), [**@trolley813**](https://github.com/trolley813)).
- **Stability:** a restart or F9 keeps the traffic, and a bad AI collision no longer throws the bus out of the world ([#3512](https://github.com/openOMSI-org/openOMSI/pull/3512), [#3397](https://github.com/openOMSI-org/openOMSI/pull/3397), [**@thibautgnt**](https://github.com/thibautgnt), [**@m3rlinek01**](https://github.com/m3rlinek01)).
- **Junctions:** AI cars no longer lock up at signals, stop for the bus in right-hand bends or stop for roundabout entries, and go round parked cars ([#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [#3661](https://github.com/openOMSI-org/openOMSI/pull/3661), [#3512](https://github.com/openOMSI-org/openOMSI/pull/3512), [**@thibautgnt**](https://github.com/thibautgnt)).
- **AI buses:** they serve their first stop, no longer vanish at lane changes, use every repaint and their own horn, show no line on depot runs, and no longer twin the player's bus ([#3663](https://github.com/openOMSI-org/openOMSI/pull/3663), [#3618](https://github.com/openOMSI-org/openOMSI/pull/3618), [#3557](https://github.com/openOMSI-org/openOMSI/pull/3557), [#3603](https://github.com/openOMSI-org/openOMSI/pull/3603), [#3579](https://github.com/openOMSI-org/openOMSI/pull/3579), [#3687](https://github.com/openOMSI-org/openOMSI/pull/3687), [#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Passengers and stops:** AI bus passengers press the stop button, stops refill gradually, waiting people no longer jump, BRT doors and stop departure boards work, and start announcements play once ([#3442](https://github.com/openOMSI-org/openOMSI/pull/3442), [#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [#3663](https://github.com/openOMSI-org/openOMSI/pull/3663), [#3422](https://github.com/openOMSI-org/openOMSI/pull/3422), [**@jay23606**](https://github.com/jay23606), [**@thibautgnt**](https://github.com/thibautgnt), [**@m3rlinek01**](https://github.com/m3rlinek01)).
- **Frame rate:** no drop near fences, and passengers and traffic no longer scan everybody each frame ([#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [#3615](https://github.com/openOMSI-org/openOMSI/pull/3615), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Plugins:** a stuck SuperRadio host, a plugin that never starts and 32-bit plugins that print output no longer freeze or drop ([#3423](https://github.com/openOMSI-org/openOMSI/pull/3423), [#3566](https://github.com/openOMSI-org/openOMSI/pull/3566), [#3569](https://github.com/openOMSI-org/openOMSI/pull/3569), [**@VlastikYoutubeKo**](https://github.com/VlastikYoutubeKo), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Edge cases:** cleaner errors for damaged sounds, small crash fixes, one bus scan at a time, Android start-up and route keyboard fixes, and security updates for compression and ZIP ([#3391](https://github.com/openOMSI-org/openOMSI/pull/3391), [#3457](https://github.com/openOMSI-org/openOMSI/pull/3457), [#3658](https://github.com/openOMSI-org/openOMSI/pull/3658), [#3561](https://github.com/openOMSI-org/openOMSI/pull/3561), [#3577](https://github.com/openOMSI-org/openOMSI/pull/3577), [#3367](https://github.com/openOMSI-org/openOMSI/pull/3367), [#3486](https://github.com/openOMSI-org/openOMSI/pull/3486), [**@turbo-devv**](https://github.com/turbo-devv), [**@isaacsa2**](https://github.com/isaacsa2), [**@thibautgnt**](https://github.com/thibautgnt), [**@shamough1792**](https://github.com/shamough1792)).
- **Window films and tinted glass of Brazilian buses show again**: a number written in an `[alphascale]` in place of a variable's name (`[alphascale] 0.9`, as the LFD Caio Apache Vip V's window film, its outer and its frosted glass have it) is that opacity, as a number already is for lamps and interior lights. Read as a variable that does not exist it was 0, and those panes were not drawn at all. Panes named in Portuguese (`vidro`, `janela`, `parabrisa`) are now known as glass, as those named in the other languages.
- **A junction left deep under the ground no longer digs a pit**: a `[terrainhole]` cutter that lies wholly more than ten metres under the ground cuts nothing now. Berlin-Spandau has a stray copy of `Einm_Poststr_Poststr` at height zero in the corner of a tile near Falkensee Bahnhof; its cutter opened a pit thirty metres deep in the field, walled with tall strips of ground. Tunnels and underpasses, whose cutters reach the ground or lie a few metres under it, cut as before.

### Multiplayer

- **Multiplayer grows up.** Servers can offer their own buses and paints for download (only with your consent), simultaneous joins wait their turn in a queue, and you can switch rooms from **Esc → Multiplayer** without restarting. The pause menu shows the player list, shared world settings and bans in tabs, and timetable buses show the host's fleet numbers, plates and destination signs ([#3535](https://github.com/openOMSI-org/openOMSI/pull/3535), [#3488](https://github.com/openOMSI-org/openOMSI/pull/3488), [#3725](https://github.com/openOMSI-org/openOMSI/pull/3725), [#3682](https://github.com/openOMSI-org/openOMSI/pull/3682), [**@isaacsa2**](https://github.com/isaacsa2), [**@thibautgnt**](https://github.com/thibautgnt), [**@shamough1792**](https://github.com/shamough1792)).
- **New online protocol:** players and servers need this version to play together ([#3540](https://github.com/openOMSI-org/openOMSI/pull/3540), [#3682](https://github.com/openOMSI-org/openOMSI/pull/3682), [**@thibautgnt**](https://github.com/thibautgnt), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Each server owner gets its own hardware identifier for bans**, and players can switch it off in Settings → General ([#3540](https://github.com/openOMSI-org/openOMSI/pull/3540), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Solo driving and inviting friends share the Drive page**, and Join friends opens a code-only dialog ([#3676](https://github.com/openOMSI-org/openOMSI/pull/3676), [**@shamough1792**](https://github.com/shamough1792)).
- **Dedicated servers** can allow teleports and bus recovery, set an optional RP speed limit, set their own AI limits, let AI drive through parked cars as OMSI 2 does, and keep their clock in step with real time ([#3481](https://github.com/openOMSI-org/openOMSI/pull/3481), [#3682](https://github.com/openOMSI-org/openOMSI/pull/3682), [#3637](https://github.com/openOMSI-org/openOMSI/pull/3637), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Joining is quicker and steadier:** loading slots free up when a game stops answering, a server still loading lets players in once ready, and tunnel joins try the direct address first ([#3682](https://github.com/openOMSI-org/openOMSI/pull/3682), [**@isaacsa2**](https://github.com/isaacsa2)).
- **The launcher shows a server's date**, and says why a game could not host ([#3682](https://github.com/openOMSI-org/openOMSI/pull/3682), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Discord Rich Presence works again** and can show the line, next stop and delay, players online and links for friends, each part chosen in Settings → General. It stays off unless you switch it on ([#3798](https://github.com/openOMSI-org/openOMSI/pull/3798), [#3545](https://github.com/openOMSI-org/openOMSI/pull/3545), [**@thibautgnt**](https://github.com/thibautgnt), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Hosting and joining:** hosting works on more Windows PCs and around busy ports, WebSocket addresses need no `/ws`, join by code waits longer for tunnel hosts, and fleet-bus joins wait to be let in ([#3752](https://github.com/openOMSI-org/openOMSI/pull/3752), [#3365](https://github.com/openOMSI-org/openOMSI/pull/3365), [#3402](https://github.com/openOMSI-org/openOMSI/pull/3402), [#3661](https://github.com/openOMSI-org/openOMSI/pull/3661), [#3637](https://github.com/openOMSI-org/openOMSI/pull/3637), [**@thibautgnt**](https://github.com/thibautgnt), [**@shamough1792**](https://github.com/shamough1792), [**@alecks**](https://github.com/alecks), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Servers:** tours can be changed again, people near players stay, bus downloads no longer overload the server, the web port opens at once, the clock moves timetable buses, and admin passwords stay out of chat ([#3417](https://github.com/openOMSI-org/openOMSI/pull/3417), [#3682](https://github.com/openOMSI-org/openOMSI/pull/3682), [#3637](https://github.com/openOMSI-org/openOMSI/pull/3637), [#3634](https://github.com/openOMSI-org/openOMSI/pull/3634), [#3457](https://github.com/openOMSI-org/openOMSI/pull/3457), [**@isaacsa2**](https://github.com/isaacsa2), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Sound:** returning players no longer jam others' sound, sound events no longer flood a server, and another player's rear engine is heard inside ([#3661](https://github.com/openOMSI-org/openOMSI/pull/3661), [#3637](https://github.com/openOMSI-org/openOMSI/pull/3637), [#3658](https://github.com/openOMSI-org/openOMSI/pull/3658), [**@thibautgnt**](https://github.com/thibautgnt), [**@isaacsa2**](https://github.com/isaacsa2)).

### Project

- **A full privacy policy** at [openomsi.org/privacy](https://openomsi.org/privacy/), linked from the launcher ([#3536](https://github.com/openOMSI-org/openOMSI/pull/3536), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Crash reports leave out names and chat**, and logs mask public addresses ([#3545](https://github.com/openOMSI-org/openOMSI/pull/3545), [#3549](https://github.com/openOMSI-org/openOMSI/pull/3549), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Server logs no longer grow for ever** and mask player addresses ([#3545](https://github.com/openOMSI-org/openOMSI/pull/3545), [**@isaacsa2**](https://github.com/isaacsa2)).
- **A local diagnostics history** helps explain crashes and stuttering; it stays on your computer and goes into the support package only if you attach it ([#3697](https://github.com/openOMSI-org/openOMSI/pull/3697), [#3701](https://github.com/openOMSI-org/openOMSI/pull/3701), [**@thibautgnt**](https://github.com/thibautgnt)).
- Serbian translations completed and polished ([#3395](https://github.com/openOMSI-org/openOMSI/pull/3395), [#3787](https://github.com/openOMSI-org/openOMSI/pull/3787), [**@kizule**](https://github.com/kizule)).

### Credits

- [**@m3rlinek01**](https://github.com/m3rlinek01), for DLSS and FSR 3 upscaling, stable AI suspension, start announcements and trolleybus poles.
- [**@GUMOLIT**](https://github.com/GUMOLIT), for trolley poles by hand, radio-controlled switches, swivelling shoes, random switch positions and trolley sounds.
- [**@shamough1792**](https://github.com/shamough1792), for the shared Light/Dark theme, room switching, reliable host ports, the multiplayer pause page, the Drive and invite flow, searchable pause-menu settings, navigator road detail and the calendar.
- [**@Bulekoo**](https://github.com/Bulekoo), for snow that builds up on roads and roofs, with tyre tracks.
- [**@FakeShxp3**](https://github.com/FakeShxp3), for sharp textures under memory pressure, bounded display memory and calmer Enhanced headlights.
- [**@trolley813**](https://github.com/trolley813), for AI driver personalities, display spacing and information-bar temperatures.
- [**@fsalexandre**](https://github.com/fsalexandre) and [**@dima-au**](https://github.com/dima-au), for GPS screens in the cockpit and cables drawn by Lua plugins.
- [**@turbo-devv**](https://github.com/turbo-devv), [**@tottichad**](https://github.com/tottichad), [**@alecks**](https://github.com/alecks) and [**@kizule**](https://github.com/kizule), for script compatibility, stamper sounds, WebSocket addresses, Serbian and reviews.
- [**@Bazingania**](https://github.com/Bazingania) and [**@Petrxlovs12**](https://github.com/Petrxlovs12), for testing DLSS and FSR 3 on five graphics cards and the cab-click fix, with numbers and screenshots.
- Thibaut ([**@thibautgnt**](https://github.com/thibautgnt)), for the frame-rate recovery, the Remote cockpit, depot buses, the city map and street search, the bug-board fixes, Discord Rich Presence, passenger looks and motion, in-game liveries, diagnostics and reviews.
- **Full changelog:** [0.3.45 → 0.3.52](https://github.com/openOMSI-org/openOMSI/compare/v0.3.45...v0.3.52).

## 0.3.51 - 2026-11-07

A small beta with three changes. Hosting a multiplayer session works again on PCs where it failed on every port. Closing the full-screen city map gives its video memory back, so the scenery no longer stays blurry after a look at the map on a big map. Trolleybus models can now turn the shoe at the end of each pole along the wire.

### New and improved

- **Closing the city map gives its video memory back to the scenery.** The full-screen city map (Shift+M) kept its ground, houses and picture on the graphics card after it was closed, and the texture budget made the scenery pay for them for the rest of the session. They are now freed once the map has been closed for 10 seconds, and the textures that shrank for them come back, even while the bus stands still. On Berlin-Spandau with a 700 MB video memory limit (Mac M4), the game held 521 MB on the card 20 s after closing the map instead of 590 MB without the fix, against 551 MB before it was opened. Reopening the map more than 10 seconds later builds its ground again, a short stall on a very large map. Tested on macOS (Metal) only ([#3756](https://github.com/openOMSI-org/openOMSI/pull/3756), [**@thibautgnt**](https://github.com/thibautgnt)).
- **The pole's shoe swivels along the wire.** A model can animate the head of each pole with `Trolley_Pole<k>_Shoe_Yaw` and `Trolley_Pole<k>_Shoe_Pitch`: on the wire the shoe lies along it, in curves and with the bus beside the wire, and off the wire it swings back straight. Nothing changes for a model that does not read these variables. Tested with a modified Trollino model in game; a model needs the right axes, animation order and pivot, or the shoe turns across the wire ([#3742](https://github.com/openOMSI-org/openOMSI/pull/3742), [**@GUMOLIT**](https://github.com/GUMOLIT)).

### Multiplayer

- **Hosting a multiplayer session no longer fails on some PCs.** On some Windows PCs the game started alone and `game.log` showed `LAN: cannot host on port 27015: TCP loopback probe failed: ... (os error 10035)` for all ten ports. The port check now waits for its own test connection to close, and a late close no longer fails the port. Each candidate port can take up to 250 ms longer to check on Windows and macOS. Diagnosed from the player's log and tested on macOS; not tested on Windows here ([#3752](https://github.com/openOMSI-org/openOMSI/pull/3752), [**@thibautgnt**](https://github.com/thibautgnt)).

### Credits

- [**@GUMOLIT**](https://github.com/GUMOLIT), for the swivelling pole shoe, and the test with the Trollino model.
- [**@isaacsa2**](https://github.com/isaacsa2), for reviewing the pull requests in this release.
- [**@thibautgnt**](https://github.com/thibautgnt), for the multiplayer hosting fix and the city map's memory.
- **Full changelog:** [0.3.50 → 0.3.51](https://github.com/openOMSI-org/openOMSI/compare/v0.3.50...v0.3.51).

## 0.3.50 - 2026-11-06

**This beta should bring a big performance gain.** The frame-rate fixes from the bisect, Vulkan by default on AMD Radeon cards, and the new DLSS and FSR 3 upscalers together should make the game run much faster for many players. Tell us in an issue how it runs on your machine, with your card and settings.

Windows players can try **NVIDIA DLSS** (RTX cards, installed optionally from the launcher) and **AMD FSR 3** (any card) upscaling on DirectX 12 and Vulkan, both marked experimental. The game draws faster from the driver's seat with MSAA, and on Windows with an AMD Radeon, Auto now opens Vulkan. A phone or tablet on the home Wi-Fi can become part of the cockpit with the new **Remote cockpit**. Trolleybus poles go on the wire by hand on their rope, and Lua plugins can draw live cables. The pause-menu settings are grouped into cards and can be searched. Around 30 bug fixes cover AI traffic at junctions, trams with two cabs, flowing water on scenery, far mountains, a Vulkan picture freeze, white discs on signs, the player's texture detail under memory pressure, and diagnostics on phones.

**Performance note:** in the St-Servan measurements on an AMD RX 7900 XT (AB O560 M from the driver's seat, Max preset without ray tracing, 4x MSAA, 2560x1440), DirectX 12 went from **90.6 to 100.0 fps** (about 10 %) and Vulkan from **108.8 to 138.5 fps** (about 27 %) against the previous main. A Radeon player who keeps Auto moves from DirectX 12 to Vulkan, so **90.6 to 138.7 fps** in that scene. 0.3.48 made 84.9 / 109.5 fps there and 0.2.30 made 114.0 / 161.0, so 0.2.30 is still faster; the remaining steps are being bisected. These figures come from one card under Windows: NVIDIA, Intel, Linux AMD and hybrid laptops were not measured. DLSS and FSR 3 figures are in their own bullets below; with the upscalers off, the measured frame rate is the same as without them.

### New and improved

- **What DLSS and FSR 3 do not cover yet.** Both are Windows only, and both are off in VR and with triple screens; photos and other off-screen pictures are drawn without them. Choosing one switches the game to DirectX 12, unless Vulkan is chosen and its DLLs are beside the game. Not tested with VR, online play, mods or Intel cards. Frame generation is not part of this release ([#3681](https://github.com/openOMSI-org/openOMSI/pull/3681), [**@m3rlinek01**](https://github.com/m3rlinek01), [**@isaacsa2**](https://github.com/isaacsa2)).
- **The player's bus keeps its texture detail, and passenger displays stop piling up adverts.** Under memory pressure, the player's bus and cab keep their loaded detail, while AI and timetable buses still fit the room left. Script-switched pictures, such as an advert display cycling through frames, are bounded in number and size and release old frames, instead of filling the card until the game dropped MSAA. Tested in drives of 70 minutes (AVBWU Red, line 171) and 53 minutes (ADL E500 MMC, line 112) on Cross Harbour with an RTX 4050 Laptop, Vulkan, Enhanced+ and 4x MSAA under constrained budgets: no allocation failure and MSAA kept. Memory still grew during those drives, so this is not a fix for every long-session memory report. Integrated GPUs, other APIs, VR and online play were not checked ([#3551](https://github.com/openOMSI-org/openOMSI/pull/3551), [**@FakeShxp3**](https://github.com/FakeShxp3)).
- **No white discs on signs seen from behind, in Enhanced.** A surface turned away from the camera gets no sun highlight (map water excepted), nor a lamp or headlight highlight. Two-sided content with backwards normals also loses its lamp highlight ([#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [#3691](https://github.com/openOMSI-org/openOMSI/pull/3691), [**@thibautgnt**](https://github.com/thibautgnt)).
- **The optimiser can turn texture compression back on.** When the graphics card is the limit and the card's own texture compression is on, one of its steps turns texture compression on at the next start ([#3691](https://github.com/openOMSI-org/openOMSI/pull/3691), [**@thibautgnt**](https://github.com/thibautgnt)).
- **A phone or tablet becomes part of the cockpit.** Switch it on in Esc, Options, Interface, Remote cockpit, Open, Switch on, then scan the QR code or type the address and 6-digit code. The phone shows a dashboard with the map, route, next stops, speed, delay and fuel; the bus's own buttons (every door, lights, indicators, brakes, engine start, kneeling, camera views); the bus's live destination sign and the line and destination picker; a touch steering wheel and pedals; and a layout editor saved per bus or for every bus. Nothing to install on the phone and no account. A USB wheel or pad takes over again as soon as the phone's wheel is let go. Off until switched on. **For trusted home networks only:** the page and its commands travel unencrypted, so on a shared Wi-Fi someone listening could replay a paired device's token; the game refuses devices outside private address ranges. Measured cost with a phone connected: about 0.07 ms per frame on the main thread. Not tested on a real phone against Windows (firewall prompt) nor in VR ([#3695](https://github.com/openOMSI-org/openOMSI/pull/3695), [**@thibautgnt**](https://github.com/thibautgnt)).
- **The navigator's next stop matches the info bar for a duty picked in game.** It learns the places of the stops beyond the loaded tiles, as a duty started from the launcher already did ([#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Departure displays that are the stop object itself list the buses due**, not only the clock ([#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [**@thibautgnt**](https://github.com/thibautgnt)).
- **A wheel back in the middle is centred.** Leftover key steering is cleared once a wheel is turned. While the wheel is off centre, keys and wheel cannot steer together ([#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [**@thibautgnt**](https://github.com/thibautgnt)).
- **The radio key plays the internet radio in any bus whose own radio is off**, including buses without one and the stock MAN NL202. While the bus's own radio plays, it wins ([#3687](https://github.com/openOMSI-org/openOMSI/pull/3687), [**@thibautgnt**](https://github.com/thibautgnt)).
- **No more frame-rate drop walking near fences.** A fence's gate is looked for only in the paths within reach, not along every footpath under a long fence every frame ([#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Riding an AI bus sounds like riding inside.** Sitting in an AI bus, the street, the rain and the scenery are heard from inside it, as in the player's own bus ([#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Rivers flow and escalators move.** `[texcoordtransX]` / `[texcoordtransY]` now scroll the textures of scripted scenery objects, as on vehicles: the river on Bad Hügelsdorf and the escalators of Berlin-Spandau's U-Bahn stations ([#3687](https://github.com/openOMSI-org/openOMSI/pull/3687), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Trolley poles go on the wire by hand, on their rope.** From outside, click a pole, move it by its rope with the mouse and let go just under the wire for its spring to put the shoe on. A pole that comes off stays loose, stopping at 45 degrees unless the bus sets `max_angle`, until the driver puts it back. Ctrl+R puts both poles on or takes them down from anywhere; in the cab, plain R and T no longer move the poles (R is the gearbox's reverse) ([#3710](https://github.com/openOMSI-org/openOMSI/pull/3710), [**@GUMOLIT**](https://github.com/GUMOLIT)).
- **Buses can script their own pole gear and sounds.** A vehicle's script can drive its lifting gear (`Trolley_Pole<k>_Gear`), be the hand on the rope, and play its own recordings for each pole event, as OMSI 2 does for doors. Sound triggers from 0.3.49 are renamed the OMSI way (`trolley_snap` is now `ev_trolleyarc`, and so on): a vehicle using the old names hears the built-in sounds. Tested on the Lublin map with the Trollino add-ons; not tested on touch screens, in VR, online, or saving with a pole in hand ([#3710](https://github.com/openOMSI-org/openOMSI/pull/3710), [**@GUMOLIT**](https://github.com/GUMOLIT)).
- **Lua plugins can draw cables in the world.** `omsi.cable` draws thin round tubes (wires, ropes) from points the plugin gives, in the world or tied to the bus, so a trolleybus plugin can show wires that sag and swing under the shoe. Nothing changes without a plugin that uses it. Tested in game on Windows only ([#3718](https://github.com/openOMSI-org/openOMSI/pull/3718), [**@dima-au**](https://github.com/dima-au)).
- **Pause-menu settings are grouped into cards and can be searched.** Search finds a setting by its name or description; the field sits beside the title in wide windows and below it in narrow ones. Settings and saved values are unchanged. Not checked in VR, online or with the mobile keyboard at runtime ([#3694](https://github.com/openOMSI-org/openOMSI/pull/3694), [**@shamough1792**](https://github.com/shamough1792)).

### Fixes

- **Auto opens Vulkan on an AMD Radeon.** On Windows, when the first discrete card is an AMD one, the Graphics API setting Auto opens Vulkan on that card instead of DirectX 12; other cards keep DirectX 12 first. If Vulkan loses the device in game, the game restarts on DirectX 12 and writes `~/.openomsi/vulkan-lost`, so Auto keeps to DirectX 12 for that card until the next version (deleting the file also resets it). An explicit Graphics API setting or `OMSI_BACKEND` is unchanged ([#3713](https://github.com/openOMSI-org/openOMSI/pull/3713), [**@thibautgnt**](https://github.com/thibautgnt)).
- **No more frozen picture on Vulkan when the card runs out of memory.** An out-of-memory error no longer rebuilds the renderer without MSAA, a rebuild that ran out of memory again and froze the picture while the sound went on ([#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [**@thibautgnt**](https://github.com/thibautgnt)).
- **A fixed render scale is sharp again at once.** Choosing a fixed render scale in game after the automatic one had lowered the picture now takes it back to full size within a few seconds, instead of at the next start ([#3687](https://github.com/openOMSI-org/openOMSI/pull/3687), [**@thibautgnt**](https://github.com/thibautgnt)).
- **AI cars at signalled junctions no longer hold lights or lock up.** A car asks only for the lights it would stop at, not for a light inside the junction its turn joins. Cars locked in a circle at a big junction are let go, the longest waiter first, when the only red light is one past the junction. Not tested with the reporters' maps (Eichenhöhe, Tver, Aachen) ([#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Oncoming cars no longer stop for the bus in right-hand bends.** The bus is looked for along its turn only, instead of a straight box reaching over the oncoming lane ([#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [**@thibautgnt**](https://github.com/thibautgnt)).
- **BRT screen doors work again.** A light program without a green phase, such as a platform's screen doors, holds its stop point again; the rule for the changeover to green now applies only to real signals ([#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Stops refill one passenger at a time.** After a bus took everybody, a stop gets one more passenger every 10 to 15 s instead of filling up all at once. Crowds at terminals are smaller as a result ([#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [**@thibautgnt**](https://github.com/thibautgnt)).
- **AI buses on depot runs show no line**, as in OMSI 2, instead of the name of their timetable file ("EITAG" on Waldhofen 2013). The depot-exit jam of the same report is not addressed ([#3687](https://github.com/openOMSI-org/openOMSI/pull/3687), [**@thibautgnt**](https://github.com/thibautgnt)).
- **IVECO liveries with a leading space in their name are found**, and the engine and gearbox chosen with them apply again ([#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Trams and trains with a cab at each end can be driven from either.** Control cables now carry signals from the rear cars to the front one too, so a car behind can tell the leading car something (the NF6D's cabs could not be switched on). Not driven with the NF6D or other cabled trains here; a test covers the cables. The report's other NF6D points are not addressed ([#3687](https://github.com/openOMSI-org/openOMSI/pull/3687), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Tram wheels turn at the right speed, and automatic switches are not thrown back.** Rail wheels turned twice as fast as the vehicle went. The player's tram no longer throws a switch back onto its old branch while its script is moving it ([#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Mirror-camera screens show their picture.** A camera is redrawn when the screen showing it is in view, not only when the camera is, so cameras mounted high outside the cab no longer leave their screens black ([#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Parts the current view does not draw can no longer be clicked.** From the cab, outside-only parts can no longer be opened through the windscreen, and from outside, cab-only switches can no longer be reached while door openers and the battery switch still can. An outside part that is also drawn from the cab, such as the PAZ 32054's hood, can still be clicked from inside; that case is still open ([#3687](https://github.com/openOMSI-org/openOMSI/pull/3687), [**@thibautgnt**](https://github.com/thibautgnt)).
- **The flashlight shines ahead when there is no live cursor.** With "Flashlight follows the cursor" on, the beam points straight ahead between taps on a touch screen, and on foot once a controller is used, until the mouse moves again ([#3687](https://github.com/openOMSI-org/openOMSI/pull/3687), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Objects up to 25 km away are no longer cut off.** Backdrop mountains beyond 6 km show; what is drawn is still chosen within 6 km, so no more objects are drawn ([#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Caxias City is no longer dark at 4 PM.** A map with a time zone other than UTC+1 and no location gets the sun of its own zone, while keeping Berlin's latitude for day lengths and seasons. This applies to every such map; whether OMSI 2 does the same was not verified ([#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Countdown traffic lights no longer stutter every second.** Script texts are redrawn only within 250 m. A text first seen farther away keeps how it looked until it comes within 250 m. The report's stutter while paused is not explained by this ([#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Attach rows keep their object at tile borders.** A row's last object due a little past the end of its spline chain is placed at the end instead of dropped, and an object that fell between two tiles' records is drawn. Stock maps look the same; the report's custom map was not loaded ([#3687](https://github.com/openOMSI-org/openOMSI/pull/3687), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Chinese, Japanese and Korean text shows on Fedora.** The game also finds Noto Sans CJK where Fedora installs it, so menus and the HUD no longer show boxes ([#3687](https://github.com/openOMSI-org/openOMSI/pull/3687), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Pull requests pass the architecture guard again** after the 2026-11-04 bug-fix merges, and reloading a vehicle type in game clears a cached cable list from the two-cab fix ([#3717](https://github.com/openOMSI-org/openOMSI/pull/3717), [**@thibautgnt**](https://github.com/thibautgnt)).

### Performance

- **NVIDIA DLSS upscaling, experimental, on Windows with an RTX card.** Settings, Graphics, Upscaling offers DLSS on DirectX 12 and Vulkan once NVIDIA's DLLs are beside the game. openOMSI does not ship them: on an RTX card, **Install NVIDIA DLSS** under Settings, Graphics in the launcher downloads them from NVIDIA after the player accepts NVIDIA's licences, and checks their signature. While DLSS is chosen it takes the place of Anti-aliasing and Render scale, which are hidden; the HUD is still drawn at full size on top. The choice applies from the next start. If it cannot start or stops working, the game goes on at full size with the player's MSAA and `game.log` says why. In the PR's still-standing run on an RTX 5080 (3264x1836, Enhanced), DLSS Quality gave 332.1 fps against 303.6 with 4x MSAA on DirectX 12, and 343.5 against 313.6 on Vulkan, where memory after 2 minutes rose from 1655 to 1938 MB; the upscaled picture is drawn at 2177x1225, so the two are not the same picture. A tester measured 89 fps without DLSS, 127 with Quality and 175 with Balanced on an RTX 5070 Ti at 4K, and found it working on an RTX 3060 Laptop and an RTX 2070. The picture comes out about 1 to 2 % darker on the road, and a DLSS screenshot lacks the light sharpening the window gets ([#3681](https://github.com/openOMSI-org/openOMSI/pull/3681), [**@m3rlinek01**](https://github.com/m3rlinek01), [**@isaacsa2**](https://github.com/isaacsa2)).
- **AMD FSR 3 upscaling, experimental, on Windows with any card.** FSR 3 works on DirectX 12 and Vulkan, and its DLLs ship in the Windows x64 package, so nothing needs installing. Like DLSS, it replaces Anti-aliasing and Render scale while chosen. On the RTX 5080, FSR 3 Quality gave 357.3 fps against 303.6 on DirectX 12 and 394.4 against 313.6 on Vulkan. A tester at 1920x1080 on the Max preset measured 53 fps off and 68 to 120 fps from Quality to Ultra Performance on an RX 580, and 26 fps off and 38 to 80 fps on a GTX 1050 2 GB (with some stutter once its 2 GB were full). Where the processor is the limit it gives no gain: on an RX 7900 XT in a CPU-bound scene, Quality was 8 to 10 % slower than off. On the bus's dash FSR 3 looks softer than DLSS ([#3681](https://github.com/openOMSI-org/openOMSI/pull/3681), [**@m3rlinek01**](https://github.com/m3rlinek01), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Faster frames from the driver's seat with MSAA.** Most of the player's own bus had been left out of the MSAA depth prefill since 0.2.46 and 0.3.34, so the world behind the cab's walls, roof and dashboard was shaded in full before the bus covered it. The bus goes back into the prefill, except behind see-through covers, which keep their earlier behaviour. This was worth about 10 fps on DirectX 12 and 12 on Vulkan on the RX 7900 XT. It needs multisampled shading and a 17th texture slot; Android and Apple devices keep the previous prefill. It costs one extra multisampled depth texture, about 59 MB at 2560x1440 with 4x MSAA. Nothing changes on screen ([#3713](https://github.com/openOMSI-org/openOMSI/pull/3713), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Enhanced runs faster on Vulkan with an AMD card.** The enhanced shader reads occlusion and bounced light in a form AMD's Vulkan compiler handles better, worth about 17 fps on the RX 7900 XT; other cards and APIs keep the previous form. Linux AMD also gets this form and was not measured ([#3713](https://github.com/openOMSI-org/openOMSI/pull/3713), [**@thibautgnt**](https://github.com/thibautgnt)).

### Multiplayer

- **No AI twin of the player's bus at the terminus.** A tour picked for a day it does not run on also takes today's same trip at the same minute from the AI. LAN players' tours are not covered ([#3690](https://github.com/openOMSI-org/openOMSI/pull/3690), [**@thibautgnt**](https://github.com/thibautgnt)).

### Project

- **Every game on a phone or tablet gets its own diagnostics record.** Before, only the first game since the app started was recorded. The support package now keeps only the day of each game, not its exact times, and time spent minimised no longer counts as slow frames. Phones often close the app without the game's exit, so many mobile records still end as open, like a crash ([#3701](https://github.com/openOMSI-org/openOMSI/pull/3701), [**@thibautgnt**](https://github.com/thibautgnt)).
- **`game.log` names an unreadable font file** and the meshes whose winding is kept, to help explain missing display digits and mirrored adverts ([#3691](https://github.com/openOMSI-org/openOMSI/pull/3691), [**@thibautgnt**](https://github.com/thibautgnt)).

### Credits

- A big thank you to everyone who made this release: the authors, the reviewers and the players who tested and sent numbers. This one was a team effort.
- [**@m3rlinek01**](https://github.com/m3rlinek01), for DLSS and FSR 3 upscaling, and for the careful rounds of fixes and measurements behind them.
- [**@isaacsa2**](https://github.com/isaacsa2), for the RTX check, Vulkan for both upscalers, the upscaling test runs, the Windows packaging of AMD's DLLs and the optional NVIDIA installer, and for reviewing nearly every pull request in this release.
- [**@FakeShxp3**](https://github.com/FakeShxp3), for keeping the player's textures sharp, bounding display memory, and the long test drives on Cross Harbour.
- [**@GUMOLIT**](https://github.com/GUMOLIT), for trolley poles by hand on their rope and scriptable pole gear and sounds.
- [**@dima-au**](https://github.com/dima-au), for `omsi.cable` and live wires and ropes from Lua plugins.
- [**@shamough1792**](https://github.com/shamough1792), for grouped and searchable pause-menu settings, in every language.
- Thibaut ([**@thibautgnt**](https://github.com/thibautgnt)), for the frame-rate bisect and the fixes that brought back most of the lost frames, Auto on Vulkan for Radeon cards, the Remote cockpit, about 30 bug-board fixes, per-game diagnostics, and the reviews that got DLSS and FSR 3 ready for this beta.
- [**@Bazingania**](https://github.com/Bazingania), for testing DLSS and FSR 3 on an RTX 5070 Ti, RTX 3060 Laptop, RTX 2070, RX 580 and GTX 1050, with numbers.
- [**@Petrxlovs12**](https://github.com/Petrxlovs12), for testing the cab-click fix on the PAZ 32054 and the screenshots that showed what is still open.
- **Full changelog:** [0.3.49 → 0.3.50](https://github.com/openOMSI-org/openOMSI/compare/v0.3.49...v0.3.50).

## 0.3.49 - 2026-11-05

Online play moves to **LAN protocol 17**: players and servers need this version to play together. Timetable buses show their fleet numbers, plates and the server's destination signs, and passengers near players are protected when a server reuses its people pool. Dedicated servers gain their own AI limits, a clock that catches up with real time, steadier joins and less repeated work during bus downloads. Solo driving and inviting friends share Drive. You can repaint a bus in place and bind a head-tracking recentre button. The navigator uses smaller video-memory buffers and preserves more road detail; distant passengers, night-time signs, trolleybus radio switches, AI driver personalities and local diagnostics also improve.

**Performance note:** the navigator's interface vertex buffers fell from **110 MB to 66 MB** in the contributor's Berlin-Spandau measurements on a Radeon 680M. This is a buffer-allocation reduction, not a 40% reduction in total game VRAM. Dense-map rebuild measurements improved, but the unmatched driving sessions do not establish an FPS gain. The multiplayer changes were not validated with a full busy-server, 64-player run.

### New and improved

- **The navigator's map uses smaller vertex buffers, with the same picture.** Packed interface vertices reduce the Berlin-Spandau map's buffers from 110 MB to 66 MB; all reported game allocations fell from 873 MB to 829 MB, while the driver's reserved VRAM fell less. Measured on Linux Vulkan, with buffer/pipeline tests on macOS Metal. DirectX 12 and Android GLES were not tested in that PR ([#3671](https://github.com/openOMSI-org/openOMSI/pull/3671), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Road glare also controls night-time highlights.** The Enhanced slider, renamed from Road sunlight glare, now tones down headlight and street-lamp highlights on roads and terrain, wet roads included. Puddles are unchanged, and far lamps at grazing angles can retain some glare even at 0% ([#3658](https://github.com/openOMSI-org/openOMSI/pull/3658), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Solo driving and inviting friends share Drive.** Map, bus and duty stay selected when switching modes; a running host's code remains on the same page. Join friends opens a code-only dialog; server addresses and saved/public lists stay separate under Servers. Preparing a connection does not start the game or save a server implicitly. Two Windows PCs successfully hosted/joined in the contributor's test; other native platforms and VR were not manually exercised ([#3676](https://github.com/openOMSI-org/openOMSI/pull/3676), [**@shamough1792**](https://github.com/shamough1792)).
- **The launcher shows a server's date** beside its time and weather (in the list and before joining), from the server's status page (`date`); an older server shows none ([#3682](https://github.com/openOMSI-org/openOMSI/pull/3682), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Choose whether today's date keeps the real year.** Settings → Gameplay → Simulation gains Keep today's year, even outside the map's years, shown with Start on today's date. With it off, the year is still fitted to the map's years as before. The web and Apple launchers follow the choice ([#3658](https://github.com/openOMSI-org/openOMSI/pull/3658), [**@thibautgnt**](https://github.com/thibautgnt)).
- **A new livery without a new bus.** Vehicle options → Livery offers a searchable repaint list and applies the choice where the bus stands, keeping passengers, trip, odometer, fuel, doors, displays and damage. Variables the old repaint alone set return to their unpainted values, and explicit player options stay. Old render resources are released; a situation saved in default paint loads in default paint. LAN games still choose the livery before placement, and loading textures can cause a short hitch ([#3644](https://github.com/openOMSI-org/openOMSI/pull/3644), [**@thibautgnt**](https://github.com/thibautgnt)).
- **AI drivers have more varied personalities.** Gaussian draws make ordinary values more common and extremes rarer for several traits, with new horn-patience and amber-light factors. Some drivers can rarely proceed through amber or red. The capped 60 FPS benchmark was close to its base and does not establish an uncapped frame-rate improvement ([#3498](https://github.com/openOMSI-org/openOMSI/pull/3498), [**@trolley813**](https://github.com/trolley813)).
- **Force-feedback road rumble follows the front wheels' surface.** Smooth asphalt is gentler; cobblestones, gravel and dirt are stronger. The existing road-vibration setting still scales it. A separate impact-strength control is not included, and hardware testing was not made in the PR ([#3658](https://github.com/openOMSI-org/openOMSI/pull/3658), [**@thibautgnt**](https://github.com/thibautgnt)).
- **A plugin can load its DLL from beside its `.opl` in a subfolder.** The top-level `plugins` path remains first, then the `.opl` folder. Duplicate copies with the same `.opl` and DLL file names load from the first folder only; a failed first copy does not fall through to another. A 32-bit DLL still needs `omsi-plugin-host32.exe`. A native plugin and a separate Lua port can both run if both are installed ([#3688](https://github.com/openOMSI-org/openOMSI/pull/3688), [**@thibautgnt**](https://github.com/thibautgnt)).
- **A new livery without a new bus.** Vehicle options › *Livery* lists the bus's liveries (with a search, as the launcher's list); the one chosen is painted onto the bus you drive where it stands, with its passengers, trip, odometer and fuel as they were ([#3638](https://github.com/openOMSI-org/openOMSI/pull/3638)).
- **The map's timetable buses show their fleet number, plate and the right destination for every player**, not only on the server or the hosting game: the host now tells each player the number and plate it drew for a bus and the depot file it reads its signs from. A player's copy showed no number and read the destination in the depot file that player chose for their own bus, so it showed another sign.
- **The map's buses show the server's signs on every player.** The server sends each player the depot file (`.hof`) its AI buses use (else the one of that name in `HOFs`), and the player's game draws those buses with it, whatever depot files it has itself.

### Fixes

- **Dense navigator and city maps keep road shapes and markings at close zoom.** Roads receive the existing geometry budget before terrain, and off-view triangles no longer waste it. Bounded fallback remains, so exceptionally dense or broad views can still coarsen roads. Automatic driving zoom alone no longer forces ground rebuilds. Windows GreatGrundorf2 LHT driving and images support the detail change; unmatched-duration performance figures do not prove a speed-up. Other platforms and VR were not exercised ([#3662](https://github.com/openOMSI-org/openOMSI/pull/3662), [**@shamough1792**](https://github.com/shamough1792)).
- **Waiting passengers remain visible farther down the road.** Each person is culled as a whole object instead of separate small body parts: with the default minimum-object-size setting, the reported range increases from about 130 m to about 265 m. Busy stops can therefore draw more triangles ([#3693](https://github.com/openOMSI-org/openOMSI/pull/3693), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Less sun-glare flicker behind the visor, and coloured signs at night.** Enhanced checks the close shadow map first for the off-screen sun, and caps unswitched night-map brightness so signs and windows keep their colour. Those windows and lamp heads are dimmer and bloom less; script-switched night maps are unchanged. The visor limitation with ray tracing remains ([#3693](https://github.com/openOMSI-org/openOMSI/pull/3693), [**@thibautgnt**](https://github.com/thibautgnt)).
- **The launcher says why a game could not host.** When a game started to host by code could not open its session (its port taken by another program, or refused by a firewall or an antivirus: since 0.3.47 it reserves a TCP port for its mods beside the UDP one, and checks it reaches itself there), Invite friends waited for a code that never came, without a word. It now shows the game's reason ([#3682](https://github.com/openOMSI-org/openOMSI/pull/3682), [**@isaacsa2**](https://github.com/isaacsa2)).
- **The launcher scans buses once at a time.** A content change during a scan requests one follow-up instead of starting several scans side by side and making the counter jump. This does not by itself resolve reports of a full rescan at every start ([#3658](https://github.com/openOMSI-org/openOMSI/pull/3658), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Late buses still find waiting passengers, and distant roads receive AI cars.** Stops consider services up to 15 minutes late instead of roughly one minute. Cars can appear in plain view beyond 350 m, including oncoming traffic. Short nearby side streets and splines starting beyond the spawn radius remain limitations; distant cars can pop in ([#3658](https://github.com/openOMSI-org/openOMSI/pull/3658), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Trips with trailing spaces in their file names run again.** Timetable, editor and route lookups match the trimmed trip names. Saving such a trip in the editor can create a file without the trailing space ([#3658](https://github.com/openOMSI-org/openOMSI/pull/3658), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Duty entry points keep their indices.** A map with some missing entry-point objects no longer shifts a valid high-numbered selection down. A selected point whose own object is missing still falls back to the timetable start ([#3658](https://github.com/openOMSI-org/openOMSI/pull/3658), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Buses answer the pedals without the OMSI program folder.** Built-in road-vehicle variables are available when `program` is missing or spelt `Program`, also restoring the affected temperatures, lights and outside-sound variables. Native Android testing was not performed in the PR ([#3658](https://github.com/openOMSI-org/openOMSI/pull/3658), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Loud engine recordings play cleanly when the game volume gives them room.** Sound entries over 100% account for game volume before the buffer limit, as in OMSI, instead of fluttering around that limit. Those entries can sound louder than before at game volumes below 100% ([#3658](https://github.com/openOMSI-org/openOMSI/pull/3658), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Scripted tram cars contribute their own brakes, and unbound pantographs keep their scripted pose.** Each car's own brake force is counted once and cleared each frame; copied lead-car force is not counted again. Pantographs without a raising trigger no longer receive an extra simulated displacement. Live tram testing was not made in the PR ([#3658](https://github.com/openOMSI-org/openOMSI/pull/3658), [**@thibautgnt**](https://github.com/thibautgnt)).
- **A key or a wheel's button can recentre head tracking.** Head tracking: centre again takes the current pose as the middle for opentrack, native TrackIR/freetrack and Tobii without changing F1's behaviour. A pre-existing `head_tracking_recenter` controller binding now works. The action has no default key; the live tracker-and-wheel check was still outstanding in the PR ([#3636](https://github.com/openOMSI-org/openOMSI/pull/3636), [**@Luc-nbr**](https://github.com/Luc-nbr)).
- **One press can toggle a radio switch.** A cab button sending `Trolley_Switch_Request = 2` flips receivers on its channel in range once per press. Held manual requests no longer change switches merely by entering their range; a nonzero route code still sets its listed switches on arrival. Tested with Lublin and Trollino add-ons; the route-code entry path has regression coverage but was not used by those add-ons ([#3670](https://github.com/openOMSI-org/openOMSI/pull/3670), [**@GUMOLIT**](https://github.com/GUMOLIT)).
- **Fewer spurious sparks and closer trolley sounds.** Wire roughness fades at joints, so the shoe no longer arcs merely because the pattern restarted. Maps can tune frog/insulator arc chance, strength and minimum current. Built-in sounds fade over shorter distances and are muffled in a closed cab; vehicle sound entries keep their settings. Maps own frog/switch recordings, while hiss and arc crack belong to the vehicle. VR, online play and Windows/Linux in-game tests were not made in the PR ([#3670](https://github.com/openOMSI-org/openOMSI/pull/3670), [**@GUMOLIT**](https://github.com/GUMOLIT)).
- **Plugin permission tests keep completed result messages during loading.** The fixture starts consent requests from frame IO, and a completed denial is reported once after reload. This is a test-only fix without player-visible behaviour changes ([#3667](https://github.com/openOMSI-org/openOMSI/pull/3667), [**@shamough1792**](https://github.com/shamough1792)).
- **A key or a wheel's button can recentre head tracking**: *Head tracking: centre again* takes where your head is now as the middle (opentrack, native TrackIR and a Tobii eye tracker), without putting the view straight as F1 does. It is on the keys list without a key; a button that already had it in `gamectrler.cfg` did nothing until now.

### Multiplayer

- **Multiplayer protocol 17.** A game of this version and a server of this version play together; older ones are turned away at the door, as at every protocol change ([#3682](https://github.com/openOMSI-org/openOMSI/pull/3682), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Timetable buses show the host's numbers, plates and destination signs.** The host sends each bus's identity and depot-file name, and its `.hof` is fetched for the session. A player's different depot choice no longer changes the AI bus's sign. Downloads are kept per session, so joining another host starts afresh. A full real-player check with the map's buses was still outstanding in the PR ([#3682](https://github.com/openOMSI-org/openOMSI/pull/3682), [**@isaacsa2**](https://github.com/isaacsa2)).
- **People no longer vanish in front of the players of a server.** When the pool of people (`ai_max_humans`) is full, somebody walking or standing out of sight is taken to make room, as in OMSI 2. A dedicated server judged "out of sight" from its own camera, where nobody plays, so it took people standing beside the players - passengers who had just got off a bus among them. Now nobody near any player is taken ([#3682](https://github.com/openOMSI-org/openOMSI/pull/3682), [**@isaacsa2**](https://github.com/isaacsa2)).
- **A server sets its own AI limits** in `server.cfg`: `ai_unsched_factor` (the random traffic, percent of the map's density), `ai_max_scheduled`, `ai_max_parked` and `ai_max_humans`, as the game's options have them. Left empty, the machine's `settings.cfg` says, as before. They count for the whole server, so a busy one can raise them ([#3682](https://github.com/openOMSI-org/openOMSI/pull/3682), [**@isaacsa2**](https://github.com/isaacsa2)).
- **A server that cannot keep up keeps the day.** Its world clock (the day, the timetable, what the players are told) moved with its physics steps, so a server short of CPU fell behind the day for good, silently: at half its steps, half a day's timetable in a day. It follows the wall now, at most twice as fast while it takes back what it lost; the physics still take their fixed steps, and the log says when the machine cannot keep up ([#3682](https://github.com/openOMSI-org/openOMSI/pull/3682), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Joining a busy server is quicker.** A player loading holds one of the eight loading slots only while their game is heard from: one closed or stuck no longer keeps the others waiting for two minutes ([#3682](https://github.com/openOMSI-org/openOMSI/pull/3682), [**@isaacsa2**](https://github.com/isaacsa2)).
- **A server still loading its world lets players in once it is ready**, telling them it is loading (an older game is told to try again); before, one let in met an empty world ([#3682](https://github.com/openOMSI-org/openOMSI/pull/3682), [**@isaacsa2**](https://github.com/isaacsa2)).
- **A game joining through a server's tunnel tries its direct address first**, which a quick tunnel's breaking connections do not touch, and keeps the tunnel if it does not answer within three seconds. The server finds the address by itself: its own internet address, else the one the internet sees if it reaches itself there - never behind a carrier's NAT (CGNAT), so nobody waits on an address that cannot answer. `public_address` in `server.cfg` names another, or `off` tells none ([#3682](https://github.com/openOMSI-org/openOMSI/pull/3682), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Bus downloads repeat less packing work and limit concurrent compression.** Every piece of a bus was packed the moment it was asked for, on the thread of its connection (up to 160 of them), and once more for every player who asked: players downloading together packed on every core, and the server's traffic and people fell behind. A quarter of the cores pack at most (at least one worker), the others' downloads wait their turn, and a piece packed is kept for the next player. A bus a player sends a dedicated server is packed at its smallest at once, in the background, so the others fetch it ready ([#3682](https://github.com/openOMSI-org/openOMSI/pull/3682), [**@isaacsa2**](https://github.com/isaacsa2)).
- **A server's fleet bus is listed once, not once per player.** A player asking for a bus of the server's fleet had the server load it and read every file of every paint again (the seconds a bus takes in the fleet check), once for each paint asked for: many players at once took every core. The list the fleet check made at the start is given instead, while the bus's files are as they were; a list to make again is made by one request at a time ([#3682](https://github.com/openOMSI-org/openOMSI/pull/3682), [**@isaacsa2**](https://github.com/isaacsa2)).
- **The rear engine is heard inside another player's articulated bus.** Rear-section sound sets now run when riding inside too, instead of only in the outside view. A live multiplayer run with the reported vehicle was not made in the PR ([#3658](https://github.com/openOMSI-org/openOMSI/pull/3658), [**@thibautgnt**](https://github.com/thibautgnt)).

### Project

- **A local history helps explain crashes and stuttering.** The game keeps small, privacy-filtered records of frame times, how a game ended and its last error in `~/.openomsi/diagnostics/`, for 30 days and at most the newest 100. Settings → General → Support → Support package includes the history for the player to inspect and attach; nothing is uploaded automatically. A killed process can leave an open record without an error, which is not proof of a crash. This is the initial recorder: later games in the same mobile process can miss their own record; the subsequent per-game fix is not in 0.3.49 ([#3697](https://github.com/openOMSI-org/openOMSI/pull/3697), [**@thibautgnt**](https://github.com/thibautgnt)).

### Credits

- [**@isaacsa2**](https://github.com/isaacsa2), for multiplayer bus identities and depot files, people-pool protection, server AI limits and clock, joining and download fixes, and hosting-error messages.
- [**@shamough1792**](https://github.com/shamough1792), for the shared Drive/invite flow, clearer join dialogs, preserved navigator road detail and plugin-test stability.
- [**@Luc-nbr**](https://github.com/Luc-nbr), for a bindable head-tracking recentre action.
- [**@GUMOLIT**](https://github.com/GUMOLIT), for trolleybus radio toggles, wire-joint arcs and sound balance.
- [**@trolley813**](https://github.com/trolley813), for AI driver personality and randomness improvements.
- [**@thibautgnt**](https://github.com/thibautgnt), for in-game liveries, navigator memory savings, graphics and vehicle fixes, plugin lookup, local diagnostics and integration.
- **Full changelog:** [0.3.48 → 0.3.49](https://github.com/openOMSI-org/openOMSI/compare/v0.3.48...v0.3.49).

## 0.3.48 - 2026-11-03

A big one: about 60 pull requests merged since 0.3.47, with around 30 new features and some 50 bug fixes, many of them from the community bug board ([#3556](https://github.com/openOMSI-org/openOMSI/pull/3556)).

### New and improved

- **Your depot buses in the game.** *Place a vehicle* lists your depot first. The bus you take keeps its livery, fleet number, plate, odometer and wear, and the drive is written back to it.
- **Go anywhere from the city map.** Alt+click a place to stand there on foot. Ctrl+drag places the bus facing the way you drag. In the map camera (F4), Ctrl+click places it facing the camera's way, as in OMSI 2.
- **Find a street by its name.** The game menu has *Find a street...*: pick one and the city map opens centred on it.
- **Graphics that fit your computer on the first start.** A new install picks Low, Medium, High or Extra from your graphics card. Existing settings are never touched.
- **Radio-controlled trolleybus switches** for add-ons, with dashboard requests, route codes, state displays and custom sounds (thanks @GUMOLIT).
- **The Alt menu stays open**, as in OMSI 2. Tiles that act in place (route arrows, refuel, repair, wash, screenshot...) leave it open. Alt or Esc closes it.
- **Radio tile**: what plays, volume, on/off, previous and next station. Useful on buses without their own knob.
- **Seat, view and mirrors tile**: the driver's eye, view and mirror settings without going through the options.
- **Clean screen (Alt+U)**: hides the navigator, virtual mirrors, notes and panels. Rebindable as *Hide / show the whole interface*.
- **Choose which notes show**: four switches under *Notes in the top-left corner* turn off one kind each.
- **Only buses on the map**: switch off *Cars on the map too* (`nav_ai_cars`) to see only AI buses and trolleybuses on the navigator and the city map.
- **Zoom on a key**: *Zoom while held* (Controls › Views) narrows the view to read a far sign or the dashboard. Strength 1.25x to 5x, hold or toggle ([#3114](https://github.com/openOMSI-org/openOMSI/pull/3114)).
- **Flashlight follows the cursor** (Options › Camera, off by default) while the cursor is free ([#3641](https://github.com/openOMSI-org/openOMSI/pull/3641)).
- **Flash the high beam to wave cars on**: under 1.5 s of high beam lets waiting cars go first for six seconds.
- **Getting up has its own key**: *Get up from the seat (on foot)*, Ctrl+Shift+G by default, rebindable on keyboard and controllers.
- **The view editor looks at the mirror being set**: moving a mirror's slider turns the head to it ([#3640](https://github.com/openOMSI-org/openOMSI/pull/3640)).
- **More traffic for strong computers**: the Traffic slider and the launcher's *Cars around* go past 120 to 160, 220 and 300 vehicles. Dedicated servers stay at 120.
- **Set the world to the time it is now** (World options › Traffic and people): timetable buses and waiting passengers reset to the current time.
- **Short rides stand by a door** (`pax_short_rides_stand`, off by default): passengers riding under about 3 km stand near an exit ([#2242](https://github.com/openOMSI-org/openOMSI/pull/2242)).
- **Trip summary stop status**: each stop reads On time, Delayed, Left early or Not served, in colour ([#2336](https://github.com/openOMSI-org/openOMSI/pull/2336)).
- Route arrows hide by themselves, and the panel has a **Hide (H)** button.
- **Finer moves**: Alt slows turning as well as moving, and the camera rises and sinks at half speed.
- **Search every setting at once** (Ctrl+F in Settings), in your language or in English.
- **Long drop-down lists filter as you type** (liveries, depot files, languages...).
- **Add several AI vehicles at once** in Settings › AI Lists, with tick boxes.
- The **realistic road physics** switch is in the launcher's Settings too.
- The **phone launcher** can set the number plate ([#2770](https://github.com/openOMSI-org/openOMSI/pull/2770)).
- Textures and meshes with Turkish letters in their names are found.
- Route-arrow objects with a script (stop displays) hide with the route arrows.
- Route arrows stand only where the route has a choice, as in OMSI 2.
- Scripted traffic lights that keep red on beside green show both, as in OMSI 2.
- The map's own sounds (birds, bells, fountains) are muffled through the bus in a cab view.
- Scenery scripts (light countdowns, clocks) stand still while paused ([#3143](https://github.com/openOMSI-org/openOMSI/pull/3143)).
- The navigator shows roads of maps built from editor-only paths over road objects.
- The navigator follows loops that serve the next stop, on the way to the route and on it ([#3244](https://github.com/openOMSI-org/openOMSI/pull/3244)).
- The Citelis 18 rear section rides on its own wheels ([#3482](https://github.com/openOMSI-org/openOMSI/pull/3482)). Rear sections of articulated buses follow their section when you walk in them.
- `Wheel_Rotation` script variables count up continuously, as in OMSI 2.
- A destination set by hand on a bus without a roller blind stays set (Ikarus 280 TEETA, [#3268](https://github.com/openOMSI-org/openOMSI/pull/3268)).
- The driver's hand goes to the gear lever, Russian levers included.
- AI buses use all of a bus's repaints, not only the first four.
- AI buses honk with their own horn, not OMSI's lorry horn.
- AI buses serve their first stop before changing lane ([#2427](https://github.com/openOMSI-org/openOMSI/pull/2427)).
- Inside an AI bus on foot or seated, its interior is drawn and heard, rear sections included.
- Parked cars fill car parks reached later in a drive.
- AI cars behind the bus show in the mirrors up to 250 m ([#2554](https://github.com/openOMSI-org/openOMSI/pull/2554)).
- Yesterday's night tours are offered only for their trips past midnight.
- Driving layout changes apply without a restart (thanks @isaacsa2).
- On Linux, PS4/PS5 triggers are axes, not random buttons.
- Head tracking smoothing and dead zone also work for AItrack and UDP trackers ([#2590](https://github.com/openOMSI-org/openOMSI/pull/2590)).
- The pause menu calendar lets you pick a date (thanks @shamough1792).
- **The radio from the Alt menu**: a Radio tile opens a small panel with what plays, the volume, on and off, the station before and after, quieter and louder - for buses whose radio has no volume knob or no switch of its own.
- **Cleaner photos**: the photo mode hides the route arrows by itself (they come back when you leave it) and its panel has a Hide (H) button at the top.
- **Only the buses on the map, if you like**: under "AI vehicles on the map", *Cars on the map too* (`nav_ai_cars`, launcher and game options) can be switched off: the navigator and the city map then show the traffic's buses and trolleybuses, not the cars.
- **A flash of the high beam waves the traffic on**: flash the headlights (high beam on for under a second and a half) and for six seconds the cars that were waiting for your bus - at a junction, or holding back for where the bus is about to go - take their turn first. They still stop for the bus itself.
- **Several AI vehicles at once**: in Settings › AI Lists, the vehicle picker for adding to a group, a depot or a line has a tick box on each vehicle; tick several and *Add the n ticked* adds them all in one go (one click still previews, a double click still adds one).
- **The map's own sounds are quieter inside the bus**: birds, church bells, fountains and the other sounds of the scenery are heard through the bus's bodywork in a cab view, louder as the doors and windows open, as the AI traffic already was; outside they keep their full volume.
- **Yesterday's night tours are offered only for their trips past midnight**: a Friday tour that runs into the night was listed and drivable all Saturday, and a Sunday one all Monday; now it counts by Friday's (Sunday's) clock, so it shows only until its last trip after midnight ends.
- **Short rides stand by a door.** With *Short rides stand by a door* on (Options › Gameplay and the launcher's passenger settings, `pax_short_rides_stand`, off by default), a passenger with no set stop who rides under about three kilometres takes the free standing place nearest an exit of their car instead of a seat; with no standing place free, they board as before ([#2242](https://github.com/openOMSI-org/openOMSI/pull/2242)).
- **Graphics that fit the computer from the first start.** On a first start (no settings file, no drive and no map loaded before), the game picks a quality preset from its best graphics chip before it opens the window's renderer: Low for software renderers, Intel HD / UHD chips and cards under 3 GB, Medium for other integrated chips and cards of 3 to 6 GB, High for 6 GB cards and Apple chips, Extra from 10 GB - and says so once. A player's own settings are never touched ([#3643](https://github.com/openOMSI-org/openOMSI/pull/3643)).

### Fixes

- **Tunnels, underpasses and box cuts are back.** Aligned splines cut the ground again where their `.sli` says.
- **Mouse steering keeps the system cursor in the window** on Windows and Linux, so a hard turn no longer slips onto another screen.
- **Aligned splines cut the ground again**: box cuts, tunnel ramps and underpasses.
- Attach-row objects on a tile joint are no longer left out.
- Solid meshes tagged `[isshadow]` (bikes by a bike trailer) are drawn again.
- Waiting passengers no longer jump every two seconds ([#2995](https://github.com/openOMSI-org/openOMSI/pull/2995)).
- Scripts with two `{frame}` blocks no longer run doors and timers at double speed ([#1101](https://github.com/openOMSI-org/openOMSI/pull/1101)).
- **Route numbers with a letter show on roller blinds.** Route numbers with signs (`&`, `N5>`) stay on matrix displays ([#2960](https://github.com/openOMSI-org/openOMSI/pull/2960)).
- The paper timetable is no longer blank after resuming a saved drive.
- Cab textures no longer crawl with grain when the interior lamp code is exactly 1.
- A charging trolleybus no longer flips power frogs while coasting.
- The cash desk's 10, 20 and 50 crown keys give those coins again.
- A timetable bus changing lane at a spline joint no longer vanishes.
- **Roundabouts**: cars on the ring no longer stop for cars at entries ([#3507](https://github.com/openOMSI-org/openOMSI/pull/3507)).
- Arrow keys bound to a bus action no longer turn the head with a wheel.
- A fast mouse is no longer taken for a graphics tablet ([#2809](https://github.com/openOMSI-org/openOMSI/pull/2809)).
- The flashlight no longer follows a cursor hidden for a controller.
- Rain no longer halves the frame rate on integrated graphics (puddle reflections are off there, [#2966](https://github.com/openOMSI-org/openOMSI/pull/2966)).
- Passengers and AI traffic no longer scan everybody for every bus each frame.
- Under graphics memory pressure, scenery keeps a lower-resolution texture instead of turning grey (thanks @FakeShxp3).
- A texture with no memory left to convert is skipped instead of ending the game ([#3219](https://github.com/openOMSI-org/openOMSI/pull/3219)).
- OpenGL drivers without compute shaders no longer lose the device ([#2580](https://github.com/openOMSI-org/openOMSI/pull/2580)).
- **A plugin that never finishes starting no longer blocks map loading.** A 32-bit plugin that prints to its output is no longer dropped.
- **Android**: starting a drive while the window is still held no longer crashes, and typing a route number opens the keyboard.
- **A clean screen (Alt+U)**: the navigator, the virtual mirrors, the notes and the panels go away while you drive, for a free view of the road or a quick picture; Alt+U again brings them back. The menu, the city map and a plugin's permission question still show over it. It is in the Controls' list as *Hide / show the whole interface*, so it can be moved to another key or a controller button (`clean_screen` in keyboard.cfg's `[game]`).
- **The flashlight no longer follows a hidden cursor**: with *Flashlight follows the cursor* on, the cursor hidden while a controller drives still steered the beam; now the beam shines straight ahead until the mouse brings the cursor back.
- **`ai_drive_through_parked`** (`server.cfg`, `--ai-drive-through-parked` for a game): the AI traffic drives on through a parked car standing in its lane, as OMSI 2's does, instead of stopping behind it and going round it when it can. For maps where a parked car stands in the lane with no way round and holds the traffic behind it for good. Off by default: nothing changes unless it is set.
- **A bus taken from the server's fleet is known by the server's version of it**, a copy of a font file beside the original no longer makes a bus another version, and a refused upload names the files that differ.

### Multiplayer

- A dedicated server opens its web port at once and checks its fleet in the background, much faster (thanks @isaacsa2).
- A game taking its bus from the server's fleet waits to be let in, instead of starting without the bus.
- Sound events no longer flood a server through a tunnel.
- A returning player no longer jams the others' sound ([#2858](https://github.com/openOMSI-org/openOMSI/pull/2858)).
- Join by code waits longer for hosts only reachable through the tunnel ([#3353](https://github.com/openOMSI-org/openOMSI/pull/3353)).
- On a server, setting the clock moves the timetable buses to the new time.
- New option `ai_drive_through_parked` (`server.cfg`, `--ai-drive-through-parked`), off by default: AI traffic drives through parked cars in its lane, as OMSI 2 does.
- **A dedicated server opens its web port at once, before checking its fleet.** With a `vehicles` list, the server read and hashed every file of every bus and paint before the WebSocket gateway opened, so for minutes `/status`, the players and the admin page were refused. The gateway now opens first and the fleet is announced when the check is done (players who join meanwhile see a server without one, as before 0.3.47). The check itself is faster: four buses at a time, each `.bus` loaded once, the hash cache read once, a bus's files gathered once for all its paints and each folder read once (on a real server a list of 15 buses with 113 paints took about 26 s). The log tells per bus its paints and where its time went, and names every bus it could not offer and why.

## 0.3.47 - 2026-11-02

Integrated graphics get the frame rate of 0.2.30 back, with an unchanged picture. Online play moves to LAN protocol 16: everyone needs this version to play together. Servers can offer their own buses for download with the player's consent, admit simultaneous joins through a queue, and each server owner now gets its own hardware identifier, which players can switch off. Players can change rooms from the pause menu, and dedicated servers can allow teleports and set an optional RP speed limit. Mods and plugins can bring interface themes, graphics options apply live with a confirmation, and the launcher limits its disk caches. Manually issued tickets lie on the desk again, change keys hand out the right coins, passengers can wave at buses and change lines (both optional), and Xbox pads over Bluetooth LE work on Windows. A full privacy policy, masked addresses in logs, redacted crash reports and Discord status off for new installs complete the release.

**Performance note:** on a Radeon 680M (Linux, Vulkan, Berlin-Spandau in the cab), the frame rate goes from **147.6 fps to 154.6 fps** (0.2.30: 155.3 fps), with byte-identical screenshots and half as many frames over 50 ms. Built-up snow and Enhanced light shadows are now **off unless a player has chosen them**, including settings files that were written before without these keys. A glass interface theme costs about **0.24 ms a frame** while the pause menu is open on the contributor's RTX 4070 SUPER. The server fleet, room switch and optional passenger features were not benchmarked. See the details below.

### New and improved

- **Graphics changes apply while driving, with a confirmation:** compatible options apply in the running game and must be confirmed within 20 seconds, or the previous values come back. Renderer mode and device formats still need a restart. New sliders: **Road sunlight glare** (Enhanced, 100% is the previous look) and **Traffic headlight brightness** (all modes, other vehicles only). The upscaler is now labelled **AMD FSR 1.0** ([#3535](https://github.com/openOMSI-org/openOMSI/pull/3535), [**@isaacsa2**](https://github.com/isaacsa2)).
- **The navigator keeps dense maps whole:** ground and buildings are drawn exactly up to a much larger budget (2 million ground vertices, 750,000 each for shadows, walls and roofs); past it, the ground is simplified rather than later roads being dropped. These are upper bounds, not preallocated memory. Before/after screenshots on Berlin-Spandau were not provided ([#3481](https://github.com/openOMSI-org/openOMSI/pull/3481), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Interface themes for mods and plugins:** Options → Interface → **Theme pack** lists themes brought by a mod (`Themes/<id>/theme.json`) or a plugin. A theme can recolour the pause menu, settings, photo mode, developer tools and plugin panels, show its logo, turn the panels into frosted glass over the game, and style the loading screen, where its logo can serve as the loading bar. openOMSI's own look stays the default and is unchanged. A broken theme falls back to it with a line in `game.log`. Glass was tested on DX12 only; without window copy support, panels are see-through without blur, and VR keeps the theme's colours without glass. Plugins get `ui.themes`, `ui.theme` and `ui.set_theme` (`omsi.ui.version` 7); the format is in `docs/THEMES.md` ([#3278](https://github.com/openOMSI-org/openOMSI/pull/3278), [**@Luc-nbr**](https://github.com/Luc-nbr)).
- **Each graphics profile is a file of its own:** profiles live in `~/.openomsi/graphics_profiles/`, so a damaged file loses only its own profile, and a profile can be copied or shared by itself. An old `graphics_profiles.json` is moved into files on first use and kept as `graphics_profiles.json.migrated`. Not tested on macOS or Linux ([#3539](https://github.com/openOMSI-org/openOMSI/pull/3539), [**@rafaelkaiz**](https://github.com/rafaelkaiz)).
- **Plugins are off on brand-new installs, for now:** a new settings file starts with **Disable plugins** on, as a temporary safety measure while plugin freezes are investigated. Existing settings files keep plugins as they were, an explicit choice is always respected, and normal OMSI bus scripts still run ([#3481](https://github.com/openOMSI-org/openOMSI/pull/3481), [**@isaacsa2**](https://github.com/isaacsa2)).
- **The information bar shows temperatures to one decimal** (for example 22.0 °C), as OMSI 2 does, so heating and cooling can be followed. The navigator is unchanged ([#3454](https://github.com/openOMSI-org/openOMSI/pull/3454), [**@trolley813**](https://github.com/trolley813)).
- **Passengers can wave at approaching buses, or turn away from full ones (optional):** **Passengers signal approaching buses** adds a raised arm and a look towards a compatible bus; **Passengers react to full buses** adds a short glance with the arm lowered. Both are off by default, affect Natural animation only and never change who boards. Remote players' passengers do not show it. Gameplay screenshots and low-end performance figures were not provided ([#3427](https://github.com/openOMSI-org/openOMSI/pull/3427), [#3428](https://github.com/openOMSI-org/openOMSI/pull/3428), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Passengers can change lines in single player (optional):** **Passenger connections between lines**, off by default with a 20% chance once enabled, lets some alighting passengers walk to a nearby stop (within 80 m) for an onward service, at most twice each. Walks are checked conservatively, not with full pavement pathfinding. LAN and dedicated servers always keep it at 0% ([#3429](https://github.com/openOMSI-org/openOMSI/pull/3429), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Passengers on AI timetable buses press the stop button:** the bus's script now receives the stop request, so rear doors that wait for it (such as on `NSAC_City_2Door.bus`) open. Not checked in a live run with that bus ([#3442](https://github.com/openOMSI-org/openOMSI/pull/3442), [**@jay23606**](https://github.com/jay23606)).
- **Manual roller blinds on the Likino 677 follow the chosen destination:** their triggers are recognised, so the blind turns to the selected terminus when the bus is switched on and passengers board for it. The Ikarus i200, which uses the same triggers, was not tested in the game ([#3538](https://github.com/openOMSI-org/openOMSI/pull/3538), [**@insus100**](https://github.com/insus100)).
- **Driven wheels can spin on ice from a standstill,** so ASR scripts have something to react to. Wheel locking and snow grip are not covered, and some scripts may react differently at standstill ([#3512](https://github.com/openOMSI-org/openOMSI/pull/3512), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Automatic start-up handles more ignition systems:** separate battery-master switches, multi-stage keys and stages that start the engine without a starter trigger, with recovery after a stall. Reverse and engine-stop triggers are never pressed as start-up actions ([#3481](https://github.com/openOMSI-org/openOMSI/pull/3481), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Launcher bus options win over the repaint's:** a `--setvar` option (door type, mirrors, fog lamps) now stays even when the chosen repaint sets the same variable. AI copies of that repaint are unchanged; what other online players see was not checked ([#3501](https://github.com/openOMSI-org/openOMSI/pull/3501), [**@Luc-nbr**](https://github.com/Luc-nbr)).
- **Xbox pads over Bluetooth LE work on Windows:** a pad seen through DirectInput drives the bus like a wired one (steering, combined triggers, buttons, D-pad). Only real Xbox pads are taken (Microsoft vendor id, known pad product ids, no force feedback), so wheels such as the G920 or a SideWinder keep their setup and force feedback. The axis layout comes from the author's pad; other Xbox models over DirectInput are untested ([#3499](https://github.com/openOMSI-org/openOMSI/pull/3499), [**@teamstarwars-dev**](https://github.com/teamstarwars-dev), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Routine dependency updates:** the game and launcher crate groups, `parking_lot` 0.13, `sevenz-rust2` 0.24.1, `unrar-rs` 0.14.1 and `resvg` 0.49 ([#3527](https://github.com/openOMSI-org/openOMSI/pull/3527), [#3528](https://github.com/openOMSI-org/openOMSI/pull/3528), [#3529](https://github.com/openOMSI-org/openOMSI/pull/3529), [#3530](https://github.com/openOMSI-org/openOMSI/pull/3530), [#3531](https://github.com/openOMSI-org/openOMSI/pull/3531), [#3532](https://github.com/openOMSI-org/openOMSI/pull/3532)).
- **Optional RP speed limits for dedicated servers**: `speed_limit_kmh=60`, `75` or `80` smooth throttle/braking near the limit, and `0` disables it (default). This is cooperative client-side roleplay, not server-enforced anti-cheat. Policy is fetched on join/reconnect, not periodically ([#3481](https://github.com/openOMSI-org/openOMSI/pull/3481)).
- **Safer defaults and plugin compatibility**: accumulated snow and Enhanced light shadows default off when their keys are absent (including older settings files), but explicit choices remain selectable and saved. Optional OMSI plugins require opt-in only on a *brand-new* settings file; existing files without the key keep plugins enabled, and explicit preferences remain respected ([#3481](https://github.com/openOMSI-org/openOMSI/pull/3481)).
- **Dense navigator keeps dense maps intact**: exact ground and buildings render until a pathological geometry budget is exceeded; approximately 4.25 million vertices in total (2M ground and 750k each for shadows, walls and roofs). Later roads are composited instead of dropped if the ground is reduced; the launcher's map remains uncapped ([#3249](https://github.com/openOMSI-org/openOMSI/pull/3249), [#3481](https://github.com/openOMSI-org/openOMSI/pull/3481)).
- **The map camera (F4) puts the bus down facing where it looks.** Moving the bus there kept the way it faced before, whichever way the camera was turned; it now stands facing the camera's direction.

### Fixes

- **The frame rate of 0.2.30 is back on integrated graphics:** the main pass is drawn in one piece again, the navigator's map goes with the frame's own submit, and the ground's detail grain skips what a pixel cannot show. Measured on a Radeon 680M (Linux, Vulkan, 1280×720, 4× MSAA, 4 runs of 120 s): **147.6 → 154.6 fps**, p99 12.44 → 12.22 ms, frames over 50 ms 6.0 → 2.8 per run; 0.2.30 reached 155.3 fps. Not tested on Windows, Android, iOS or in VR; `OMSI_MAIN_SPLIT=1` brings back the old split for comparison ([#3521](https://github.com/openOMSI-org/openOMSI/pull/3521), [**@thibautgnt**](https://github.com/thibautgnt)).
- **No more coloured speckles on AMD in Ultra with MSAA:** shader values AMD left uninitialised are cleared, and near-zero normals no longer produce invalid reflection directions ([#3499](https://github.com/openOMSI-org/openOMSI/pull/3499), [**@teamstarwars-dev**](https://github.com/teamstarwars-dev)).
- **Panorama backdrops give way as you approach:** an object's level of detail is chosen from its distance, as in OMSI, even when the camera is inside its bounding sphere. On ProjektKameniste, the blurred hill photo no longer stands across the road in front of the map's own scenery. Objects whose top LOD threshold is high may now switch levels up close; only Kameniště was checked in the game ([#3425](https://github.com/openOMSI-org/openOMSI/pull/3425), [**@VlastikYoutubeKo**](https://github.com/VlastikYoutubeKo)).
- **Vanilla headlights no longer light the bus's own front** on some vehicles, such as the ADL E400 MMC ([#3487](https://github.com/openOMSI-org/openOMSI/pull/3487), [**@georgefung892**](https://github.com/georgefung892)).
- **Script-switched textures work again on some models:** models combining `[matl_change]` with a free texture, such as the ADL E500 MMC's monitor, show the right texture when powered, a regression since 0.3.41. Tested on Windows only ([#3475](https://github.com/openOMSI-org/openOMSI/pull/3475), [**@georgefung892**](https://github.com/georgefung892)).
- **F4 "Move here" faces the camera:** the bus is put down facing the direction the map camera looks, instead of keeping its old heading ([#3421](https://github.com/openOMSI-org/openOMSI/pull/3421), [**@rafaelkaiz**](https://github.com/rafaelkaiz)).
- **Tickets issued by hand lie on the desk again:** on buses without a ticket printer, clicking a tear-off block puts that single ticket on the desk, several can lie there at once, and the passenger takes only the one they accepted; a wrong ticket stays. The `ev_ticketsale_giveticket` and `ev_ticketsale_taketicket` sounds play. Ticket packs without `Ticket_<n>_single.o3d` models cannot show the ticket ([#3525](https://github.com/openOMSI-org/openOMSI/pull/3525), [**@insus100**](https://github.com/insus100), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Change keys hand out the coin they name:** on cash desks with a coin changer (such as the Heuliez GX327), the 0.50, 1.00, 2.00 and 5.00 keys give that coin or note again on maps whose money system has an extra 0.20 coin, such as Grand Paris-Moulon. Only `cashdesk_changer_*` and `cashdesk_change_*` triggers change; non-euro money systems were not tested ([#3511](https://github.com/openOMSI-org/openOMSI/pull/3511), [**@thibautgnt**](https://github.com/thibautgnt)).
- **The line's start recording plays once at the first stop:** with arrival announcements on, it no longer repeats every second while the bus stands there, and an announcement the IBIS did not take is retried instead of lost. Only buses with auto-announce on (off by default) are affected ([#3422](https://github.com/openOMSI-org/openOMSI/pull/3422), [**@m3rlinek01**](https://github.com/m3rlinek01)).
- **A restart keeps the traffic and passengers:** after the game restarts itself because the graphics card was lost, or on a quick load with F9, AI traffic, waiting passengers, the driver file and the depot bus are kept instead of an empty world ([#3512](https://github.com/openOMSI-org/openOMSI/pull/3512), [**@thibautgnt**](https://github.com/thibautgnt)).
- **A trolleybus pole pulled down by the retriever can go back on the wire:** each raise now behaves like the first instead of the pole snapping back to the hook. Verified by a test, not in the game ([#3480](https://github.com/openOMSI-org/openOMSI/pull/3480), [**@m3rlinek01**](https://github.com/m3rlinek01)).
- **Smaller stability fixes:** recovery logs, completed duties on the schedule paper, empty photo buffers, controller names and mirror sliders no longer crash or misbehave on edge cases, and passenger profiling history stays bounded ([#3457](https://github.com/openOMSI-org/openOMSI/pull/3457), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Force feedback reaches the MOZA R5's wheel again:** forces go to the lowest axis with a motor (steering before pedals), whatever order the driver lists them in. Only devices with several force axes listed out of order change; other wheels were not tested in the game ([#3494](https://github.com/openOMSI-org/openOMSI/pull/3494), [**@Luc-nbr**](https://github.com/Luc-nbr)).

### Performance

- **Lighter defaults for new and untouched settings:** built-up snow on roads and roofs and Enhanced light shadows are off when the player never chose them, older settings files included. Both stay in the Graphics settings, and an explicit choice is kept. With snow off, snowy roads use the season's winter textures as in OMSI 2 ([#3481](https://github.com/openOMSI-org/openOMSI/pull/3481), [**@isaacsa2**](https://github.com/isaacsa2)).
- **No tearing on Vulkan with vsync and a frame limit:** with a limit below the screen's refresh rate (for example 40 fps at 60 Hz), Vulkan and OpenGL now present every frame in order. The picture is steadier but can judder slightly; DX12, Metal and "Unlimited" are unchanged ([#3512](https://github.com/openOMSI-org/openOMSI/pull/3512), [**@thibautgnt**](https://github.com/thibautgnt)).

### Multiplayer

- **This beta changes the online protocol (LAN protocol 16):** players and servers must run the same version to play together. Hardware bans made under the previous protocol no longer match anybody; a server logs a warning at start when it finds such bans ([#3540](https://github.com/openOMSI-org/openOMSI/pull/3540), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Each server owner gets its own hardware identifier, and players can switch it off:** a host with a ban list sends a random key, and the game derives an identifier from the machine and that key. All sessions of one owner still share it, so "ban for all my sessions" keeps working, but two owners can no longer match a player by it. **Settings → General → Send servers an identifier for bans** (`hardware_id`) is on by default; switched off, servers with `require_hardware_id = 1` turn the player away, and their message says where to turn it back on. The Swift launcher changes were not built in the PR ([#3540](https://github.com/openOMSI-org/openOMSI/pull/3540), [**@thibautgnt**](https://github.com/thibautgnt)).
- **The multiplayer screens say who runs a session:** joining, hosting and the Servers tab explain that sessions are run by players, not by openOMSI, and what the host receives (driver name, network address, chat and voice). Every saved server and public-list entry is tagged **Community server** or **Run by openOMSI**, and a dedicated server says the same to its operator at start ([#3540](https://github.com/openOMSI-org/openOMSI/pull/3540), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Servers can offer their own buses and paints:** the launcher lists the server's fleet, and a bus or repaint the player lacks is downloaded only after consent, then verified (fingerprint, size and hash) and cached for the session without replacing installed mods. When the server copy cannot be fetched, the game joins with the installed bus if the player has it. Tests on a live server and with real GPUs were still pending at merge ([#3535](https://github.com/openOMSI-org/openOMSI/pull/3535), [**@isaacsa2**](https://github.com/isaacsa2), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Simultaneous joins wait their turn:** a host admits a bounded number of loading players at once; the others are told to wait for a loading slot and retry automatically, in order. Older clients without this answer are asked to try again in a minute ([#3535](https://github.com/openOMSI-org/openOMSI/pull/3535), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Switch rooms without restarting:** **Esc → Multiplayer...** enters or pastes another connection code or address, reconnects to the current room, or leaves and continues offline. The current room stays active until the player confirms the new one; a different map is loaded after confirmation, and if it cannot load, the old world stays playable offline rather than reconnecting. A host leaving or switching is warned that the other players will be disconnected, and hosts cannot join their own room. A fresh two-PC run of the final version was still outstanding at merge ([#3488](https://github.com/openOMSI-org/openOMSI/pull/3488), [**@shamough1792**](https://github.com/shamough1792)).
- **Hosting no longer trips over a busy port:** the host reserves UDP and TCP content transfers together and skips a port another application already holds. LAN search and joins by bare IP or hostname try all ten automatic ports (27015 to 27024), while an explicit `ip:port` or a session code still targets one port. A refusal from the host (full, banned, wrong version) is shown at once instead of after the join timeout. A fresh cold-transfer test across two computers was not repeated on the final version ([#3365](https://github.com/openOMSI-org/openOMSI/pull/3365), [**@shamough1792**](https://github.com/shamough1792)).
- **Dedicated servers can allow teleports and recovery:** F4, city-map and entry-point relocation go through the server, which checks the authenticated player and the destination. Administrators can go to a player, bring one or all players, and recover overturned buses. `allow_player_teleport = 0` stays the default; `1` lets ordinary players move only their own bus. Player-hosted LAN sessions keep the host-only rule ([#3481](https://github.com/openOMSI-org/openOMSI/pull/3481), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Optional RP speed limit for dedicated servers:** `speed_limit_kmh = 60`, `75` or `80` eases off the throttle near the limit and helps brake above it, without clamping the speed; `0` (default) turns it off. It is a cooperative limit applied by the client, not anti-cheat ([#3481](https://github.com/openOMSI-org/openOMSI/pull/3481), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Admin passwords stay out of public chat:** `/admin` commands are recognised whatever their case, and a command missing its password stays local. Moderators no longer see administrator addresses in individual operation records ([#3457](https://github.com/openOMSI-org/openOMSI/pull/3457), [**@isaacsa2**](https://github.com/isaacsa2)).
- **The launcher keeps its disk caches in check:** the local cache has a soft limit of 1 GB and multiplayer downloads 4 GB; active downloads may go over for a while, then older files are removed when safe. Troubleshooting shows the usage and offers **Clean to limits** and confirmed clearing, which needs every game closed and protects server vehicle files. Desktop only ([#3535](https://github.com/openOMSI-org/openOMSI/pull/3535), [**@isaacsa2**](https://github.com/isaacsa2)).
- **AI timetable buses go round a car parked in their lane** on roads with two lanes each way, then return to their route. Not tested on the reported Novi Sad map or in LAN play ([#3512](https://github.com/openOMSI-org/openOMSI/pull/3512), [**@thibautgnt**](https://github.com/thibautgnt)).

### Project

- **A full privacy policy:** [openomsi.org/privacy](https://openomsi.org/privacy/) now covers the game, multiplayer, optional services, crash reports, the hardware identifier, Discord, the website and its third parties. It is linked from the website footer and from a **Privacy** button in the launcher's Settings → General (not built for the Swift launcher in the PR) ([#3536](https://github.com/openOMSI-org/openOMSI/pull/3536), [**@thibautgnt**](https://github.com/thibautgnt)).
- **Server logs no longer grow for ever:** the server scripts start `server.log` anew at every start and past `log_max_mb` (20 MB by default), keeping `log_keep` older logs (5 by default). Public player addresses are cut to their network in `server.log` and in a hosting game's `game.log`; local addresses stay whole, and `log_addresses = 1` keeps them for an abuse report. New `server.cfg` files describe what the server keeps, and Docker logs are capped. `run-server.ps1` was reviewed but not run on Windows ([#3545](https://github.com/openOMSI-org/openOMSI/pull/3545), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Crash reports leave out names and chat:** the home folder, account names in paths, the driver profile name, other players' names and LAN chat lines are removed before a report is copied or opened as a GitHub issue, on desktop and mobile. Names typed freely elsewhere in a log line are not recognised. Android and iOS game logs now mask public addresses too, and names with apostrophes are fully removed; native device runs were not made in the PR ([#3545](https://github.com/openOMSI-org/openOMSI/pull/3545), [#3549](https://github.com/openOMSI-org/openOMSI/pull/3549), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Discord status is off for new installs:** Rich Presence now defaults to off in the game, the launcher and the Apple settings. A settings file that already has it on keeps it ([#3545](https://github.com/openOMSI-org/openOMSI/pull/3545), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Patched ZIP handling and quieter host logs:** ZIP extraction uses `zip` 9.0.5 for a reported advisory. The host startup log no longer prints the session identity, host name or join code, and a failed dedicated-server start no longer prints the configuration path ([#3486](https://github.com/openOMSI-org/openOMSI/pull/3486), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Android NDK download resilience**: the Release workflow retries interrupted SDK/NDK installations. Windows networking tests retry ephemeral ports excluded for TCP as well as already-occupied ports ([#3481](https://github.com/openOMSI-org/openOMSI/pull/3481)).

### Credits

- [**@isaacsa2**](https://github.com/isaacsa2), for the server fleet and join queue, disk budgets, live graphics options, server teleports and speed limits, optional passenger gestures and connections, privacy work on logs, crash reports and Discord, and many stability fixes.
- [**@Luc-nbr**](https://github.com/Luc-nbr), for interface themes, the MOZA force-feedback fix and launcher options that win over repaints.
- [**@shamough1792**](https://github.com/shamough1792), for switching rooms in game and reliable host ports.
- [**@insus100**](https://github.com/insus100) and [**@jay23606**](https://github.com/jay23606), for tickets on the desk, roller blinds and stop requests from AI bus passengers.
- [**@teamstarwars-dev**](https://github.com/teamstarwars-dev), [**@georgefung892**](https://github.com/georgefung892), [**@VlastikYoutubeKo**](https://github.com/VlastikYoutubeKo) and [**@rafaelkaiz**](https://github.com/rafaelkaiz), for AMD and Xbox fixes, vehicle lighting and textures, backdrops, graphics profiles and F4 placement.
- [**@m3rlinek01**](https://github.com/m3rlinek01) and [**@trolley813**](https://github.com/trolley813), for start announcements, trolleybus poles and information-bar temperatures.
- **Full changelog:** [0.3.46 → 0.3.47](https://github.com/openOMSI-org/openOMSI/compare/v0.3.46...v0.3.47).

## 0.3.46 - 2026-10-29

Snow settles gradually on roads and vehicle roofs, with tyre tracks and slushy ruts that fill in as it snows. The launcher and the game's screens share one Light/Dark theme, and a launcher switch helps test a game without plugins. AI vehicles with very stiff suspension no longer send the player's bus into a black, silent view through an invalid collision. Buses wait at their set-down stop between trips, server players can change or end their own tour again, and WebSocket addresses work without adding `/ws` by hand. Enhanced headlights are less washed out, London's damaged garage lamp loses its huge yellow glow, DirectX 12 starts without DXC again, and a stuck SuperRadio host no longer freezes the game.

**Performance note:** built-up snow is on by default on supported render paths. In the contributor's GPU-limited Enhanced test, it reduced **185.2 fps to 166.2 fps (about 10%)** and added **16 MB of VRAM** once the roads carried snow. **Settings → Graphics → Built-up snow on roads and roofs** switches the effect off. See the measurements below.

### New and improved

- **One Light/Dark theme for the launcher and the game:** the navigator and city map, Shift+Y information bar, timetable, performance panel, trip summary, pause menu, photo mode, developer tools and plugin screens follow the same choice. A change in the pause menu carries back to the launcher, and restarting no longer overwrites the saved game theme with an old launcher preference. Existing `nav_light=1` settings migrate to Light ([#3309](https://github.com/openOMSI-org/openOMSI/pull/3309), [**@shamough1792**](https://github.com/shamough1792), [**@isaacsa2**](https://github.com/isaacsa2)).
- **Start without plugins, without moving their files:** Settings → General → **Disable plugins**, off by default, saves the choice for new duties, saved situations and tutorials. It takes effect on the next game start; turn it off and restart to restore plugins. Normal OMSI bus scripts still run, but plugin-supplied bus, map and global features are unavailable while it is enabled. This is a troubleshooting switch, not a diagnosis of every freeze ([#3379](https://github.com/openOMSI-org/openOMSI/pull/3379), [**@isaacsa2**](https://github.com/isaacsa2)).
- **The effect can be switched off:** **Built-up snow on roads and roofs** is in the game's and launcher's Graphics settings and is saved across restarts. OpenGL, classic Vanilla and unsupported storage-buffer paths keep seasonal snow as before. Tyre tracks do **not** change local grip yet; maps with their own WinterSnowfall road textures can look different with the effect on ([#3237](https://github.com/openOMSI-org/openOMSI/pull/3237)).
- **Measured cost, not a free effect:** on Grundorf in heavy snow, Windows 11 / i9-11900K / RTX 3060 / DX12 / 1920×1080 / 4× MSAA, the contributor's matched Enhanced standing view went from **185.2 to 166.2 fps**, and the GPU main pass from **3.65 to 4.20 ms**. Capped Vanilla+ stayed at 200 fps; these figures do not establish the cost on a weaker GPU. The track field uses **16 MB of VRAM once snow needs it**, and retained tracks are capped at **64 MB**. Disabling the effect restores the previous snow behaviour, although the published breakdown still measured about **0.09 ms** from the added shader code on that system ([#3237](https://github.com/openOMSI-org/openOMSI/pull/3237)).
- **Enhanced headlights wash out the road less:** generic road-headlamp intensity is reduced to one third for AI traffic and the player's bus, reducing bright road pools and fog cones. Dipped/full-beam patterns, reach, light counts and rendering passes stay the same. Authored cookie lights, downward work lights and classic rendering keep their intensity. Mods calibrated around the old generic Enhanced brightness also become dimmer ([#3430](https://github.com/openOMSI-org/openOMSI/pull/3430), [**@FakeShxp3**](https://github.com/FakeShxp3)).
- **Each ticket stamper can have its own sound and script:** mod authors get indexed `ev_StamperN` sounds and `PAX_StamperN_Used` script triggers, plus `PAX_Stamper_Used` for any stamper. Indices follow cabin order, starting at zero. The old `ev_Stamper` event remains for compatibility, so mods adopting new sounds should account for both events ([#3388](https://github.com/openOMSI-org/openOMSI/pull/3388), [**@tottichad**](https://github.com/tottichad), [**@turbo-devv**](https://github.com/turbo-devv)).
- **No more huge yellow glow at Lower Marsh (London).** The map's `GarageLights.sco` has a light with no height in its file, so every value after it was read a line late: a yellow glow 120 m across. A `[light_enh_2]` larger than 50 m (the largest in the stock content is under 10 m) is a damaged one and is no longer drawn.

### Fixes

- **Snow builds up, and traffic leaves tracks:** roads go from patches and dusted edges to full cover instead of switching white at once. Wheels press tracks into fresh snow, traffic leaves slushy ruts along the lanes, and snow fills them when the road is left alone. Tracks are kept when you drive away and return, within a bounded cache. Roof snow builds up too, blows off while driving and thaws with the weather. A remote player's bus keeps the weather's roof snow where no local accumulation is tracked ([#3237](https://github.com/openOMSI-org/openOMSI/pull/3237), [**@Bulekoo**](https://github.com/Bulekoo)).
- **A bad AI collision no longer takes the player's bus out of the world:** vehicles with very stiff OMSI axle values, including trams or ships routed over street paths, could develop invalid suspension coordinates and pass them into the player's collision response. Suspension integration is bounded, articulated sections recover from invalid pivots, and invalid contacts are rejected. This addresses the black view, missing sound and empty navigator reported with a NaN player position; ordinary road vehicles remain below the new suspension limits ([#3397](https://github.com/openOMSI-org/openOMSI/pull/3397), [**@m3rlinek01**](https://github.com/m3rlinek01)).
- **Timetable buses wait where they set down:** at a terminus with separate set-down and pick-up stops, a bus waits at the set-down stop, then drives to the first stop shortly before departure. It no longer occupies the first stop for its entire layover and makes arriving buses queue in the road. Same-stop termini and rail handovers keep their existing behaviour. Drive-up time remains an estimate, so a long or congested approach can still make a bus late ([#3315](https://github.com/openOMSI-org/openOMSI/pull/3315), [**@yu213272-coder**](https://github.com/yu213272-coder)).
- **Valiant Citybus doors 2 and 3 work again:** an `{end}` inside an open `{if}` now leaves the script block only when execution reaches it. The compiler keeps the following `{else}` instead of discarding the door logic ([#1606](https://github.com/openOMSI-org/openOMSI/issues/1606), [**@turbo-devv**](https://github.com/turbo-devv)).
- **Missing font characters no longer shift the rest of a display:** an unsupported character takes the width of a space, keeping fixed-width and dot-matrix text aligned. A missing letter appears as a space; the font itself is not replaced ([#3406](https://github.com/openOMSI-org/openOMSI/pull/3406), [**@trolley813**](https://github.com/trolley813)).
- **No huge yellow glow at Lower Marsh, London:** a malformed garage light parsed as 120 m across is no longer drawn. The guard rejects `[light_enh_2]` sprites larger than 50 m; a mod deliberately using such an oversized lamp also loses its glow. The separate whitish halo of the valid garage lights in Enhanced is outside this fix ([#3426](https://github.com/openOMSI-org/openOMSI/pull/3426), [**@rafaelkaiz**](https://github.com/rafaelkaiz)).
- **DirectX 12 starts when DXC is missing:** a shared ray-traced sample is read once, avoiding an FXC optimiser stack overflow during pipeline creation. Vulkan and DX12-with-DXC comparison captures were pixel-identical. Normal frames use the same or fewer lookups; a puddle capture with ray tracing can do a small extra lookup on some pixels. Hardware ray tracing on DX12 was not exercised locally in that PR ([#3424](https://github.com/openOMSI-org/openOMSI/pull/3424), [**@VlastikYoutubeKo**](https://github.com/VlastikYoutubeKo)).
- **Unrelated updates no longer discard the compressed-texture cache:** changing a dependency that cannot affect textures no longer makes the next start recompress every cached texture. Texture code and decoder changes still invalidate it. This avoids unnecessary loading work after updates; it is not a measured driving-FPS improvement ([#3360](https://github.com/openOMSI-org/openOMSI/pull/3360), [**@isaacsa2**](https://github.com/isaacsa2)).
- **A stuck SuperRadio host no longer freezes the game:** 32-bit plugin round trips run on their own thread. A frame waits at most about half a second initially; a plugin that was late gets a much shorter wait afterwards, and a host silent for 20 seconds is stopped for the session. Slow plugins can run a frame behind and miss events from skipped frames. Shutdown no longer waits forever for a stuck host ([#3423](https://github.com/openOMSI-org/openOMSI/pull/3423), [**@VlastikYoutubeKo**](https://github.com/VlastikYoutubeKo)).
- **Cleaner errors for damaged sounds and missing texture-change files:** empty or truncated sounds are rejected before probing, and a missing `chtex_*.cfg` path is reported once instead of flooding the log. This addresses log noise, not the separate menu watchdog stall in the original report ([#3391](https://github.com/openOMSI-org/openOMSI/pull/3391), [**@turbo-devv**](https://github.com/turbo-devv)).

### Multiplayer

- **Server players can change and end their own tour again:** Esc → Driving and routes, Alt → Timetable and the second-page **End the tour** tile offer the existing actions. Ending a tour clears only that player's duty; the server still controls time and weather. The picker requires a bus, and ending a tour stays unavailable without an active duty. This restores menu access without changing the wire protocol ([#3417](https://github.com/openOMSI-org/openOMSI/pull/3417), [**@isaacsa2**](https://github.com/isaacsa2)).
- **WebSocket joins work without typing `/ws`:** `ws://host` and `wss://host` select the gateway's `/ws` endpoint automatically. Explicit custom paths and prefixes are preserved, hostname checks use the parsed host, and ordinary bare LAN addresses continue through UDP. This does not fix every query-string or mod-transfer URL case ([#3402](https://github.com/openOMSI-org/openOMSI/pull/3402), [**@alecks**](https://github.com/alecks)).
- **Patched compression libraries for content transfers:** the existing zstd dependency chain is updated for the reported RustSec advisories. Content fingerprints and the online protocol are unchanged; cross-decoding and a two-computer missing-file transfer were checked in the PR. Port-reservation work in [#3365](https://github.com/openOMSI-org/openOMSI/pull/3365) is separate and not included in this beta ([#3367](https://github.com/openOMSI-org/openOMSI/pull/3367), [**@shamough1792**](https://github.com/shamough1792)).

### Project

- **Serbian wording is polished further**, including last-stop labels and launcher texts ([#3395](https://github.com/openOMSI-org/openOMSI/pull/3395), [**@kizule**](https://github.com/kizule)).
- **GPU diagnostics distinguish desktop, mirrors and both headset eyes:** opt-in `OMSI_PROFILE=1` and `OMSI_GPU_TIMERS=1` reports use separate timing/readback slots. `OMSI_BENCH_STEREO=1` adds an offline stereo benchmark. The figures exclude OpenXR UI, compositor reprojection and headset display time; this improves diagnosis, with no FPS-gain claim ([#3214](https://github.com/openOMSI-org/openOMSI/pull/3214), [**@NotSourLimes**](https://github.com/NotSourLimes)).

### Credits

- [**@Bulekoo**](https://github.com/Bulekoo), for gradual road and roof snow, retained tyre tracks, fallback support and published performance measurements.
- [**@shamough1792**](https://github.com/shamough1792) and [**@isaacsa2**](https://github.com/isaacsa2), for the shared theme, plugin troubleshooting, cache reuse, restored server tour controls, dependency fixes and integration reviews.
- [**@m3rlinek01**](https://github.com/m3rlinek01) and [**@yu213272-coder**](https://github.com/yu213272-coder), for stable AI suspension and terminus buses that leave room for one another.
- [**@VlastikYoutubeKo**](https://github.com/VlastikYoutubeKo), [**@rafaelkaiz**](https://github.com/rafaelkaiz) and [**@FakeShxp3**](https://github.com/FakeShxp3), for plugin-host, DirectX and lighting fixes.
- [**@alecks**](https://github.com/alecks), [**@trolley813**](https://github.com/trolley813), [**@tottichad**](https://github.com/tottichad), [**@kizule**](https://github.com/kizule), [**@NotSourLimes**](https://github.com/NotSourLimes) and [**@turbo-devv**](https://github.com/turbo-devv), for gateway URLs, display spacing, stampers, Serbian, diagnostics, script compatibility and reviews.
- **Full changelog:** [0.3.45 → 0.3.46](https://github.com/openOMSI-org/openOMSI/compare/v0.3.45...v0.3.46).

## 0.3.45 - 2026-10-27

### New and improved

- **The cab's steering wheel turns as far as yours** (*Cab wheel rotation*, in the launcher's Settings › Driving › Game controllers and in the game's Options; `cab_wheel_rotation`): the wheel model in the cab turns the set degrees lock to lock, 180° to 1800°, so a 900° or 1080° wheel on the desk and the one in the bus move alike. *Automatic* (0, the default) keeps the bus's own animation, as before. A bus that turns its wheel from a script of its own keeps its own rotation.
- **Controls**
- **Force feedback can be tuned per wheel**: under *Fine tuning*, next to *Steering force* and *Vibration*, a wheel has its own *Self-centring* (0-200 %: how strongly the moving bus turns it back to straight; it never pulls at a standstill), *Damping* (0-300 %: a released wheel settles at the centre instead of swinging past it), *Parking resistance* (0-200 %: how heavy it is while the bus stands or crawls), *Minimum force* (0-20 %: lifts the smallest forces past the dead centre of belt and gear wheels; 0 on direct drive) and *Smoothing* (0-50 ms: softens grainy steps on strong wheels at low frame rates), with **Reset tuning** to go back. They are on the launcher's Game controllers page and in the device's *Force feedback* tab of the game's controller menu, where a change applies at once, and are kept in `gamectrler.cfg` under `[openOMSI.FFTuning]`, which OMSI 2 ignores. The defaults keep today's feel, and a wheel left at them is saved without the section ([#2103](https://github.com/openOMSI-org/openOMSI/issues/2103), [#3289](https://github.com/openOMSI-org/openOMSI/issues/3289)).

### Fixes

- **Train cars that run their own scripts take the paint's variables and the cables' signals.** A tram's second car, whose script is its own (the Konstal 105Na), started with the script's defaults and never got the paint's `[setvar]`s, so its destination boards, number plates and roof parts were missing in every livery but the plainest ([#3343](https://github.com/openOMSI-org/openOMSI/issues/3343)). It now wears the paint of the car in front, also when the paint is changed later. The cars' `[control_cable_*]` plugs now carry what a car's script writes down the train, car by car (a coupler or bogie without a script of its own passes it on), so the second car is powered, lit and braked with the first instead of waiting for its own start ([#3342](https://github.com/openOMSI-org/openOMSI/issues/3342)). Each car also measures the wire above its own pantograph (`GetHeightAbovePoint`), not the leading car's ([#3341](https://github.com/openOMSI-org/openOMSI/issues/3341)).

## 0.3.44 - 2026-10-26

### New and improved

- **The restart after a lost graphics device is lighter** ([#3323](https://github.com/openOMSI-org/openOMSI/pull/3323), [#3328](https://github.com/openOMSI-org/openOMSI/pull/3328)): the Enhanced extras (ray tracing, every-light and lamp shadows, cloud shadows, the glow of lights) are switched off at the first restart, and Enhanced itself gives way to Vanilla+ at the second.

### Fixes

- **A game whose graphics device is gone starts again by itself instead of sitting on the loading screen** ([#3232](https://github.com/openOMSI-org/openOMSI/pull/3232)): when the driver stops answering without saying so, every graphics resource becomes invalid and hundreds of errors a second follow while nothing is drawn. That rate is now taken for a lost device, and the game restarts on its last situation with lighter graphics, as it already did when the driver reported the loss.
- **The navigator's map no longer ends with an error report when the graphics device is lost**: its background builder stops quietly and the restart goes on.
- **The log says where the game is stuck**: a part of a frame that has run for ten seconds is named in `game.log` (and again every thirty seconds), so a report of a game that stays at a part of the loading screen shows what it was doing ([#3326](https://github.com/openOMSI-org/openOMSI/pull/3326)).

## 0.3.43 - 2026-10-26

Stable release: 0.3.42 promoted after testing, with two further changes. The changes since the previous stable release (0.3.37) are those of 0.3.38 to 0.3.42 below.

### New and improved

- **The launcher starts as it did in 0.3.41.** 0.3.42 made the launcher write a pointer file beside its program and copy data folders at start, and some antivirus programs and browsers took that for harmful behaviour and blocked the download or the program. That change (the data folder choice in Setup, [#3243](https://github.com/openOMSI-org/openOMSI/pull/3243)) is taken out again, so the launcher keeps its data in `~/.openomsi` as before; it can return later in a form that leaves the program folder alone. Players who were blocked can update to this version.
- **On foot, the player walks wherever the passengers' paths lead**: a staircase, a ramp, a gate through a fence or a doorway the map gives the AI passengers is now walkable by hand too, at the path's height (the step and slope limits and the walls no longer stop the walker on a people's path), instead of only where the ground happens to allow it.
- **Docs**: 4-character route matrix pads with spaces ([#3320](https://github.com/openOMSI-org/openOMSI/pull/3320))
- **Feat(navigator)**: add optional lightweight classic 2D mode ([#3267](https://github.com/openOMSI-org/openOMSI/pull/3267))
- On foot, the player walks wherever the AI passengers' paths lead ([#3303](https://github.com/openOMSI-org/openOMSI/pull/3303))
- Restore money sound effects, manually take money before next sale ([#3310](https://github.com/openOMSI-org/openOMSI/pull/3310))

### Fixes

- **Fix**: 4-character route displays (LiAZ 5292 etc.) now pad with spaces instead of zeroes ([#3319](https://github.com/openOMSI-org/openOMSI/pull/3319))

## 0.3.42 - 2026-10-25

### New and improved

- **Lights shine and cast shadows in every direction** (Enhanced, *Light has shadows: Every light*): a spot that is no headlamp gets a map along its own axis, the two strongest lamps that shine every way a cube map of six tiles (fifty-six lamp tiles now), and a lamp in the dark has a glare round it that falls as the cube of the distance and a halo in the night air (*Glow of lights*).
- **Wet roads and water mirror the street** (Enhanced): the sheen of a wet road and still water trace what stands round them in screen space at their Fresnel share, not only the pools - the lit windows and lamps of a street stream down a wet road at night.
- **A flashlight (Alt+F)**: a hand lamp held beside the eye that shines where the view looks, a warm white cone reaching 50 m with a soft edge, in the Vanilla and the Enhanced pictures, in every view, by night in the dark or by day inside a dim cab. `flashlight` in keyboard.cfg's `[game]` (or a controller button) switches it as well.
- **Choose where openOMSI keeps its data** (Setup › `.openomsi` folder): settings, profiles, logs and caches can live in another folder, on another disk. A game and its launcher find the folder through a pointer file beside the program (beside the `.app` on a Mac, so an update of the bundle keeps it); a first start, or an installation that cannot be written, uses `~/.openomsi` as before. Moving copies the folder first, in the background (leaving the page does not stop it), and switches only when the copy is whole; a copy that fails leaves the old folder as it was. The default `~/.openomsi` is kept after a move, because other copies of openOMSI may still use it.
- **A modern engine for HTML screens (opt-in)**: a page that puts `<meta name="omsi-engine" content="modern">` in its head runs on a second engine with flexbox, `position` with `z-index`, borders, `calc()` and custom properties, attribute selectors, combinators, gradients, shadows, and a full JavaScript engine (Boa): classes, `Promise`, `async`/`await`, ES modules, `fetch` of the page's own files and a fuller DOM. Pages without the line keep the classic engine and are not touched; `OMSI_HTML_ENGINE=modern` runs every page on the modern one for comparing. See [docs/HTMLTEXTURE_MODERN.md](docs/HTMLTEXTURE_MODERN.md).
- **Transport modes in the launcher**: maps and vehicles show their mode (tram, railway, metro, trolleybus), the map and vehicle pickers can filter by it, and a vehicle that does not fit the chosen map is marked with the reason and cannot be started (the game refuses it too).
- **Signalling blocks, points and depot gates for the AI trains**: blocks of `signals.cfg` (given by where they begin and end) hold AI trains one at a time, and the map's signal objects show whether their block is free; the points of `switches.cfg` stay under a train, return to their rest position, can be `manual` or locked by a signal; the first departure of a service comes out of its depot's gate.
- **The overhead wire and the third rail power the player's rail vehicle**: with a `catenary.cfg` or `thirdrail.cfg` in the map's `transport` folder a tram's pantograph or a metro's shoe needs a live section under it (dead sections and breaks stop the drive, a sagging line weakens it), a vehicle says `[rail_power]` where it gets its current. `network.cfg` sets the class, street, third rail, tunnel and speed of the rail lanes, and `switches.cfg` the speed of a diverging branch.
- **AI trams, trains and metro trains by services**: the trips made from a map's `routes.cfg` are routed by the game over the whole map's network (the lanes of the mode's kind nearest each station, the shortest way through them, with the direction of the lanes respected) and timed from the stretch lengths, the segment speeds, acceleration and the standing at stops; their departures wait until the track is found, and a trip that cannot be routed is named in the log.
- **Scripts for vehicles and map objects**: a vehicle can name Lua or `.oop` scripts with `[vehicle_script]` that start when it becomes the player's and stop when it is left, with the permissions of vehicle content (`ui`, `storage`, `vehicle_write`, `audio`). A `.sco` can name scripts for the objects placed from it (`[lua_script]`, `[lua_range]`, `[lua_rate]`); they hear `object_init`, `object_frame`, `object_click` and `object_gone` and drive their own objects through the new `omsi.object.*` functions: variables, strings, `{trigger}`s, and moving, turning, scaling or hiding single meshes. Scripts are told apart by their whole path, so two folders may each hold a `gate.lua`. See [docs/PLUGINS.md](docs/PLUGINS.md) and [docs/OBJECT_SCRIPTS.md](docs/OBJECT_SCRIPTS.md).
- **Realistic rail physics** (with the setting "Realistic road physics", or `OMSI_REALISTIC_ROAD=1`; off by default, so no existing tram, train or trolleybus drives differently without it):
  - **Pantographs meet the wire**: a raised pantograph rises until its head touches the contact wire and no further (the head's travel is measured on the model's own meshes and animations; the script's variable is left alone, only the animation is shown the travel at which the head is on the wire), presses it with its static force, and brings current only while it lies on a live wire. The wire is the standard height of the class over the rails (kept within the pantograph's reach; it is not read from the map yet), staggered across the track, harder at forks and rougher in the wet and the frost. Where the wire is uneven at speed the head leaves it: a break under current strikes an arc, drawn as sparks and a flash; lowering under load does the same. `Panto_Height`, `Panto_Contact`, `Panto_Force`, `Panto_Arc` and `Panto_Powered` are written for models and sounds.
  - **Rail bodies ride on their springs**: heave, pitch and roll on the secondary suspension answer the lateral acceleration of a bend, the traction and the brake and the unevenness of the rails at each bogie, with damping. `rail_body_roll`, `rail_body_pitch`, `rail_body_heave`, `rail_accel_side` and `rail_accel_vert` are written for scripts.
  - **AI trams and trains** pull as their class does: the pull falls with speed at the class's power.
  - **Legacy trolleybuses keep their script's traction**, including its gear and door-brake interlocks, even with realistic physics enabled. An inferred generic drive stays inactive until a script adapter can honor those controls. Explicitly configured trolleybus drives and pole simulation remain available.
- **Rail traction, brakes and couplings**: `[rail_traction]` (a curve of pull against speed, a power limit, an electric brake), `[rail_brake]` (air with a brake pipe wave and filling cylinders, electro-pneumatic or direct; the electric brake first, the friction brake the rest) and `[rail_coupler]` (free play, draft gear, strength; a train pulled apart breaks in two) for pedal-driven trams, metro and railway vehicles. Without the blocks nothing changes. See [docs/RAIL.md](docs/RAIL.md#traction-brakes-and-couplings-of-a-vehicle-openomsi-blocks).
- **Scripts that belong to a vehicle**: `[vehicle_script]` in a `.bus` or `.ovh` names a Lua or `.oop` script that runs as a plugin while that vehicle is the player's, with the plugin API and the permissions of vehicle content only (`ui`, `storage`, `vehicle_write`, `audio`). See [docs/PLUGINS.md](docs/PLUGINS.md#scripts-that-belong-to-a-vehicle).
- **Suspension types**: an axle can say `[suspension]` with `type coil | leaf | air | progressive`. Leaf springs stiffen and rub (dry friction between the leaves); air bellows hold the ride height of the empty vehicle whatever the load, through a levelling valve; axles without the block are OMSI 2's strut, unchanged. See [docs/SUSPENSION.md](docs/SUSPENSION.md).
- **The `transport` folder of a map**: stations with platforms, **routes without tours** (a route is segments from station to station and the services that run over them: a line with a branch, a short-turn or an express is more segments and another service), the overhead wire, a third rail, switches, depots, signals and blocks, in plain text files. Check them with the `transit_check` example. The data is read when the map opens and the problems are logged, and every service runs as a timetable line (one tour per departure and direction) in the launcher, the duty and the AI. See [docs/TRANSPORT_FORMATS.md](docs/TRANSPORT_FORMATS.md).
- **Transport modes**: a vehicle and a map can say what kind of transport they are for with `[bus_transport]`, `[tram_transport]`, `[railway_transport]`, `[metro_transport]` and `[trolleybus_transport]`. A bus does not start on a tram map and says why; a map with none of them takes every vehicle, and a vehicle with none is judged by its shape as before, so nothing that works now changes. The launcher lists the modes of maps and vehicles; timetable vehicles and the AI pool respect them. See [docs/TRANSPORT_MODES.md](docs/TRANSPORT_MODES.md).
- **AI Lists** (Settings, the new *AI Lists* tab): choose, map by map, which AI cars, lorries and buses the traffic is made of and which buses run on which line - only from what is installed (the AI vehicles and the buses, with every repaint), nothing new is added. *Traffic* changes the weighted lists of the random traffic (replace a car by another, add or remove one, set how often it appears); *Depot buses* changes a depot's types, fleet numbers, plates and repaints; *Lines* makes a line run only, or mostly, the types you choose (a type the line's depot lacks is added to it with a few buses). Every list and the picker show the vehicle in the 3D preview of the Drive page. The map's own files are never touched: the changes are kept in the content folder (`ailists_#user.cfg` and `car_use/!openomsi-ai-lists-*.ocu` of the map's folder, the format OMSI itself reads), a group or a line can be reset on its own, and *Reset the map* deletes them, which gives the map's defaults back. Translated into every language of the launcher.
- **Every light casts a shadow** (Enhanced and Enhanced+): the lamps' shadow maps were four, now there are forty-four, tiles of an eighth of the far shadow map's width (256 texels at the default 2048) in six rows under it - only where the picture is an Enhanced one, so Vanilla and Vanilla+ need no more than the far map itself - and smaller where the graphics card's textures are too low for them. Twenty are the world's: the street lamps, floodlights and platform lights lighting the view most keep their map while they stay among the strongest, and a lamp new to the set takes a free one (six a frame at most). Eight look along the road from the headlamps of the player's bus and of the cars in front of the camera (a car behind it that drives away has none), a pair of lamps sharing a map, taken for a place to half a metre so that a car driving by draws it every few frames, not every one; the car's own parts cast nothing into it. Sixteen are the lamps of the player's saloon: they hang over the seats in the bus's own frame, so that the maps hold still while the bus drives. They hold what stands still in the bus - the seats, the rails, the panels, a shut door; the wheels, the steering wheel, a swinging door, the driver and the passengers are left out until they have stood still for half a second - so that driving the SD77 at night draws none of them again (a tile with a tree in the wind in it is drawn every third frame, on its own turn). A handrail, a seat or a stanchion shades the floor, the seats and the panels behind it - also where the model's light maps (`[matl_lightmap]`) lift a surface to its texture's colour, which now stops where the lamps' light does not reach. The lamps name their tile in their record, no more a search through four, and the beams in fog, rain and snow of every lamp with a tile (the point lamps' too, not only the headlamps') are cut by it. `OMSI_NO_CABIN_SHADOWS=1` leaves the saloon's lamps without shadows; `OMSI_DEBUG_DRAWS=1` logs the tiles drawn each frame. A free camera high over the street chooses the lamps by their distance along the ground, as the lamps round it are the ones that light what it sees.
- **Mirrors drawn as the Enhanced picture show the sun's shadows too** (Graphics: Mirror quality: *As the picture (Enhanced)*, which stays opt-in, *Standard* the default): they used to need *Shadows in the mirrors* as well. They look up the window's shadow maps, no pass of their own.
- **Shadows in the headlamps' beams** (Enhanced): the light the fog, rain or snow scatters from a headlamp or a street lamp's beam is cut where a pole, a car or a wall stands in the beam, as the sun's shafts are cut by the shadow maps: the beam looks up the lamp's shadow tile along its way (one filtered tap a step, no pass of its own). `OMSI_NO_BEAM_SHADOWS=1` draws the old picture. `OMSI_DEBUG_PAD=1` logs every game controller button event (the ones the assignment menu waits for), for a report that a pad's buttons cannot be assigned.
- **On foot**
- **Trolleybuses** ([docs/TROLLEYBUS.md](docs/TROLLEYBUS.md))
- A map can carry a real overhead wire (`trolley.txt` beside its `global.cfg`): two wires of opposite polarity, sections that are live or dead with an insulator between them, frogs (switches) and the feeder's voltage drop. A vehicle's `.bus` can carry real poles (`[trolley_pole]`) and a modelled drive (`[trolley_rheostat]`, `[trolley_async]`). OMSI 2 has no wire of its own (its trolleybuses run a plugin), so these formats are openOMSI's.
- **A trolleybus only moves on current**: both poles must be on live wire of opposite polarity. The poles are real booms with a spring, a mass and a shoe that holds the wire: the shoe leaves the wire where the wire rises faster than the boom can follow, and the pole comes off when the wire lifts out of the shoe, when the bus stands too far off the wire's line (also at a switch set the wrong way), when a bend is taken too fast, or when the wire ends. On a dead section the bus rolls on and stops.
- Out of the cab, near the back of the bus (on foot, outside or free camera), hold R to take a pole's rope and aim the boom at the wire, let go to put it on; T pulls a pole down; Shift takes both. Modern poles with a motor go up and down from the cab with the same keys.
- The rheostat drive solves its series motor against the resistor stack with a stepping controller, resistor heat and dynamic braking; the asynchronous drive is an inverter with regenerative braking into the line where the section takes it, a brake chopper and an optional battery for manoeuvring away from the wire. A vehicle with a scripted drive of its own keeps it, and loses its torque without current.
- Frogs are set by the current the bus draws at the contact plate before them (power takes the branch, coasting goes straight), or by its indicator.
- **New environment variables**: `OMSI_TROLLEY_DEAD_AT`, `OMSI_TROLLEY_FILE`, `OMSI_TROLLEY_DEAD`, `OMSI_TROLLEY_UP`, `OMSI_TROLLEY_HANDS`, `OMSI_DEBUG_TROLLEY`.
- **Phones and tablets**
- **The free camera (F4) on a touch screen**: the cab panel's new "Free camera" button flies out of the cab, a stick at the bottom left moves the camera (the further it is pushed, the faster: fine control near the middle), a finger dragged on the picture looks round, two fingers zoom, the buttons at the bottom right rise, sink and halve or double the speed, and "Exit free camera" (always on the screen, also with the controls hidden, and in the cab panel) goes back to the driver's seat.
- **Photo mode for fingers**: its own buttons (the shutter, leave, the settings sheet, hide the panel, a stick and rise / sink / fast), a drag turns the camera, a tap focuses there, two fingers zoom the lens. The settings come in a sheet from the bottom of an upright screen or a panel at the right of a wide one, inside the screen's cut-outs, scrolled with a finger.
- **Weather and light (Enhanced)**
- **The eye adapts the way an eye does**: a picture much darker than the light model expects (a dark cab, an underpass) is made up for over some three seconds - five-sixths of it, across six stops, where a bright surplus (snow, the sun in view) still only darkens the picture a little - and a dazzle on coming out passes in about a second. This is for the day: through the dusk it gives way to the old gentle metering, so the blue hour and the twilight of a summer night are not shown brighter than the sun under the horizon makes them (a white night comes only where the sun stays a few degrees below it). The Enhanced+ picture takes the same metering.
- The sky's occlusion by houses, trees and hills is worked out on the street's scale (18 m), not only the first two metres: a narrow street or the shade under a crown is darker than a square, and the surfaces round a point light it with what they send back (the last frame's picture, colour included: a sunlit wall lights the one across the street, lit windows light the street at night). The light is read where the surface was in the last frame's picture, so it follows a turning head and a moving bus, and not at all after a cut (another view, a jump) or where a surface was hidden then; the cab's own dashboard and pillars do not shade the street. It is the Enhanced picture's with the AO on: the other pipelines pay nothing for it (two-channel targets, no history picture, the old blur). A lamp or a lit sign among the surfaces does not stand in the picture as a bright square. The `street_light` setting (Graphics: Street light: Auto, On, Off) switches it. It is off by default; On enables it, and Auto enables it except on a small or shared graphics chip. The optimiser turns it off when the GPU is the limit. `OMSI_WIDE_AO=reach,strength,gain,max` tunes it.
- The physical weather model keeps the water of the grey deck (its optical depth, a thin stratus to a raining nimbostratus), so a rainy or winter day is as bright or as dark as its cloud is thick. It also gives the cumulus layer its own base (the condensation level) and depth (a shallow sheet under a high, towers in moist rising air), altocumulus, and thunderstorms.
- **A thunderstorm has lightning**: flashes of several strokes light the clouds from inside and the street with a cold light, a cloud-to-ground stroke draws its crooked channel, and the thunder is heard after the distance's delay, from the flash's direction. The flashes are worked out for every pipeline from the day's seed (a replay of a day repeats them), so the thunder of a storm is there without the Enhanced picture. `OMSI_STORM=activity[,azimuth]` shows one.
- The sun's shafts between houses and trees are followed up to 420 m: the first 180 m in the same steps as before, the rest in a few wider ones.
- The cumulus layer's height follows the ground under the player, so a map in the hills keeps its clouds.
- **Road markings on the map when zoomed in**: lane lines and their dashes, edge lines, stop lines, crossings and arrows, white and yellow, read off the road textures themselves (a spline's texture laid by its own scale, an object's flat faces by their texture coordinates), so a map maker's own textures work without a list. Light all-over textures (concrete, cobbles), worn blotches and flecks are not taken for paint.
- The dark rim of a carriageway on the map is a kerb's breadth (it was a pavement's, and ate the outer lanes of a road without pavements, so the lane lines seemed to drift towards its edge); pavements stay dark all over. Paint that reaches past the paving is left out.
- **Bends of wide roads on the map are round**: a road of many profiles was cut in chords of ten metres or more on a turn (the game's own limit on a spline type's cross-sections), now at least every degree. Painted patches of ground (car parks, fields, sand) are read at about a metre and a half instead of five, so their outlines are finer.
- **Houses on the map are the models themselves**: zoomed in (within 1500 m of the view, the nearest 200 000 triangles; the in-game panel 30 000), a house is drawn in 3D from its model's own faces without textures (gabled and ridged roofs, wings, stepped towers, canopies on posts - a canopy or a bridge over a street is a roof in the air, no longer a solid block that shuts the street), each model kept once however often it stands; farther out, and past the budget, a house is its footprint with walls. Their flat outline is traced four times finer. Hairline gaps between pieces of paving no longer grow dark beads along a street's rim.
- **More houses on the map**: stations, railway halls and other buildings in folders with `road` in their name were never taken for houses; blocks of many buildings in one object (the tower blocks, a swimming pool complex) were dropped for their yards and are now drawn, each house with its own height; garages and greenhouses count too.
- **Model test** (Troubleshooting's second tab, the launcher's former Model test page): any content is loaded the way the game loads it and every warning of its files is listed - a bus with the parts coupled behind it, a whole map (every tile, object and spline it places, what of it is not installed, its ground textures and AI lists; optionally every model, mesh and texture of it), an installed mod (picked from a searchable list), or any file or folder: a scenery object, a spline, a human, a weather, a mesh, a mod not installed yet (folder or zip). A folder's counts filter what it holds by kind. From the command line:
- **Machine bans**: the game sends a hash of the machine's own id (never the id itself, no serial numbers) when it joins, and a host turns away a banned machine whatever name, address or game it comes back with. The session's list is `bans.json` beside `server.cfg`; "all my sessions" is `~/.openomsi/bans-owner.json`, read by every session that account hosts (`owner_bans` in `server.cfg` to share one list between servers). The web panel has the bans with the button to lift them. A player can switch the identifier off (`OMSI_NO_HARDWARE_ID=1`), a server can turn such players away (`require_hardware_id = 1`). This stops the player who comes back, not one who works at it (docs/SERVER.md says how).
- **Head tracking**
- Tobii eye trackers (the Eye Tracker 5, the 4C, the ones built into laptops) track the head natively on Windows, without opentrack: the game reads the Tobii Experience's own Stream Engine (`Head tracking with`: Automatic takes opentrack while it sends, else the Tobii). F1 takes where the head is as the middle. The tracker's trembling is smoothed away while big turns still follow at once (the `Smoothing` and `Dead zone` settings), and a look at the screen's left or right edge can turn the view further (`Look further at the screen's edges`). Every tracker's poses are drawn frame by frame, and a lost tracker glides the view home instead of snapping it there.
- An **Add-ons** page (Content > Add-ons; on a phone under More) lists projects others make for openOMSI, each with its author, a short description and a button that opens its website in the browser after saying which site it goes to. It is a list of links only: nothing is downloaded or installed. A developer adds an add-on with a pull request of one entry, see [docs/ADDONS.md](docs/ADDONS.md).
- **Launcher**
- **The timetable editor works on the map**, as Drive does: every stop of the timetable stands on the map (the depots as amber squares), a stop clicked says which trips call there and puts one into the tour, and a stop found by name flies the map to it. A new trip is made by clicking its stops: only the stops the map has a station link to are ringed, the route it would drive is drawn as they are chosen, and it is saved as a `.ttp` beside the line (the changed trip of a stop list is saved as a new one, so the map's own stay as they are). The tours show their trips with a departure field and a minute up and down; copies, repeats and the reset moved into small menus.
- **Trams, metros and trains** ([docs/RAIL.md](docs/RAIL.md))
- Rail vehicles have a class (tram, metro, train), from `[rail_class]` in their file or from what it declares (a `[contact_shoe]` is a metro, a car over 45 t a train), and rail lanes a track kind (street or railway, third rail, tunnel, speed), from a map's new `openomsi-rail.cfg`, from the names of its spline profiles (`tram_*`, `metro_*`, `rail_*`) or from where the track lies. A road vehicle is never taken for a rail one.
- **Pantographs.** A rail vehicle with a pantograph moves only with it raised. The game finds it from `[pantograph_var]` / `[pantograph_trigger]` in the vehicle's file or, for a mod without them, from its pantograph meshes and the variable they turn on (the Tatra T6B5's is `door_0`, which is then no longer a door: no door key, no boarding, no "door open while driving" penalty). Shift+P raises and lowers it.
- The speed limit of a rail lane is brought down to what its curve allows (a tram loop, the diverging branch of a turnout).
- **Road vehicles' physics** (new setting **Realistic road physics**, off by default; Esc menu, Settings)
- With it on the weather's wind leans and pushes a bus and a denser or thinner air changes the drag (the vehicle's scripts already have their still-air drag on the brakes, so only the difference is added); grip and rolling resistance depend on what each tyre rolls on, wet or dry (cobbles, dirt, grass, gravel, snow), rolling resistance also rises with speed and load; and the road is as uneven as ISO 8608 says its surface is, which shakes a bus on cobbles, gravel and the joints of a concrete-slab road ([#3069](https://github.com/openOMSI-org/openOMSI/pull/3069)).
- **The headlights light the road as in OMSI 2 in Vanilla and Vanilla+**: the player's `[spotlight]` is Omsi.exe's Direct3D spot light - from the spotlight's own place, 0.3 x its colour as diffuse and 0.1 x as ambient light, attenuation 1 / (0.2 + d / R + 40 d² / R²), its cone with a falloff of 2 - lit at the vertices as Direct3D lights them. The old fit (full strength out to an eighth of the range, never below 15 % at a grazing angle) left a flat sheet reaching far down the road with some buses and lit kerbs, hedges and trees hardly at all. Measured against OMSI 2 with the Kajosoft Citaro, headlights on less off: the road +6 / +7 / +8 / +7 / +6 from the bumper out (OMSI 2: +5 / +4 / +5 / +7 / +6), the kerb +26 / +15 (+21 / +15). AI vehicles' spots light the road no longer, as in OMSI 2.
- **The people boarding a timetable bus walk in to their seats**: in any bus but the player's they stood in their seats the moment they stepped through the door, a jump in the saloon of every bus that passed a stop. They now walk along the bus's own paths to the place reserved, as in the player's bus (a bus without paths to the place keeps the jump).
- **Destination displays light up at night in Enhanced and Enhanced+**: a script's screen is never taken for a pane. The EMT Citaro's Krueger matrix (Mallorca) is blended, writes no depth and draws its letters through a `\S:n` transparency map - the layout of a window - so the Enhanced pictures shaded it as glass, which gives no light of its own: its light map was left out and its letters stayed grey at night, where Vanilla+ lights them ([#2927](https://github.com/openOMSI-org/openOMSI/issues/2927)).
- **A much lighter Enhanced frame by day**: since 0.2.42 the AI cars were lent headlamp beams whenever their lights were on, and in a grey winter noon (cars' lights on, no street lamp lit) those twelve cars' beams cost 13 ms of a 25 ms frame on an M4 - any light at all in the scene makes every pixel look through the lamp grid and has the weather's fog work the lamps' air light out. They are lent only while the scene is dark enough for them to show (the sun's height and the weather's gloom, the larger, is 0.12 or more): in a grey or rainy day the three nearest cars within 60 m, in the dark the twelve within 200 m as before. In a clear day, 27.4 ms -> 13.0 ms of GPU a frame; in a heavy grey snow day 27.6 ms -> 19 ms; the night picture is as before.
- **AI cars and buses are drawn with their model's lower levels of detail** (`[LOD]`, as Omsi.exe picks them by the vehicle's size on the screen, and as the scenery's levels are): only the first level was loaded, so each AI vehicle was all its meshes at any distance. A model with interior lamps, displays, switched textures or a lit or animated lower level (a bus, a tram) keeps its full meshes. `OMSI_FORCE_LOD=1` draws the lower levels at any size, for a look at them.
- **Enhanced**: no more snow, or any ground, glowing orange from a small light far away. The ground took the map's baked lamp pools on top of the lamps themselves, three times over, through walls and roofs and with a tail sixteen radii long: the snow of a courtyard behind a building lit by the depot's floodlights glowed yellow at night, and so did the ground round a lamp whose pool ended long before. The ground is lit by the lamps alone now, with their shadows (Vanilla and Vanilla+ keep OMSI's baked light maps). The floodlights and the canopies of petrol stations (declared 10 to 20 m, held to 15 m and dimmed to a fifth as if they were a building's window glow) are lights as declared in the Enhanced picture, with a light of their own beside the held one the plain pictures keep.
- **A trolleybus's poles are taken with the mouse**: a click on the cab's pole switch, or on a pole from outside the cab, takes it in hand, it follows the cursor (the tip goes to the wire the cursor points at, or where it points when there is none), and the next click lets it go and it springs on to the wire. A pole on the wire can be taken off again the same way. A plain trolleybus shows its own pole mesh turning with the pole (the raised mesh is turned about its hinge by the pole's angles) instead of a drawn boom, and from the driver's seat one click on the switch is enough: the motor takes the pole to the wire nearest its tip.
- **Rain on the glass follows the rain**: a drizzle leaves small beads, a downpour full drops, and the runners of a downpour are heavier, wider and quicker, with wider cleared tracks; the drops on an upright side window are smaller than on the sloping windscreen ([#3076](https://github.com/openOMSI-org/openOMSI/issues/3076)).
- A passenger on foot in a bus (another player's too) looks the way the bus does: the view turns with the bus through a bend and keeps its own look as an offset ([#3093](https://github.com/openOMSI-org/openOMSI/issues/3093)).
- Winsenburg's Hauptbahnhof 4 crossing (win-4.sco): a bus asking at the second bus light alone gets its green. A point the program had just jumped to counted as a second visit and sent the clock round for good ([#3092](https://github.com/openOMSI-org/openOMSI/issues/3092)).
- Objects attached to a spline stand where the editor put them on maps with `[worldcoordinates]` (Shanghai and others): the chain distance of a spline was not scaled with the tile like the lengths and the rows' start distances were, so the first object of a row - and the whole row after it - stood off by that difference on tiles away from the map's middle row ([#3080](https://github.com/openOMSI-org/openOMSI/issues/3080)).
- `OMSI_PRESENT_MODE=fifo|fifo_relaxed|mailbox|immediate|auto` picks the swapchain's present mode for a test, when the surface has it (vsync stays `AutoVsync`, so a game just under the refresh rate does not drop to half of it): Vulkan reports of vsync that tears ([#2838](https://github.com/openOMSI-org/openOMSI/issues/2838)) can try `fifo`.
- **Misted windows follow the bus's own cabin**: a bus whose scripts keep their cabin air (`heizung.osc` of the stock buses and their descendants) fogs its glass from that air's temperature and humidity and clears it by the heating, the fans and the vent windows, as its gauges say - they were misting from a second, guessed cabin (always 20 degrees, three air changes an hour) that no control of the bus touched, so at 17 inside and 7 outside the panes of a bus with a few people aboard stayed foggy for good. The windscreen's defroster blows the heater's own air when its fan runs. Buses without such scripts keep the built-in cabin, now with the fresh air a city bus brings in for each person (15 m3/h).
- Snow on windows is laid out in metres of glass, not in the layer's own texture coordinates: it came out as long streaks on the rear windows of double-deckers and articulated buses (the rear window shared a mask with the side windows and lay edge-on in it) and sparse on the small tilt windows and the doors (their coordinates squeezed the snow picture into a few centimetres). Every window of a bus now holds the same snow, in Vanilla, Vanilla+ and Enhanced.
- **Behaviour change:** a bus that cannot be had as its driver has it (the server takes no uploads, the upload failed, it is too large to fetch) is drawn with another bus — the viewer's copy of that path, else their own — and **labelled** under the player's name. Before, the viewer saw their own copy of that path without being told it was not the driver's. While a bus is still arriving, a neutral box of its size stands in.
- Voice chat in sessions through an outside voice program (replaced since by the game's own, see Unreleased), players heard where they are [#1102](https://github.com/openOMSI-org/openOMSI/pull/1102) - with a

### Fixes

- **Enhanced has options of its own, and the quality presets cross the modes.** Enhanced+ is no longer a mode: it is Enhanced with *Real-Time Ray Tracing* switched on (`ray_tracing=1`; a `graphics=enhanced_plus` file reads as that, and `--enhanced-plus` still asks for it for a run). Graphics has *Light has shadows* (`light_shadows`: Off, Street lamps and vehicles, Every light), *Cloud shadows and light shafts* (`cloud_shadows`), *Glow of lights* (`light_glow`), *Real-Time Reflections* (the puddles' setting, now with the wet roads' and the water's) and *Reflective objects reflect* (the reflection maps), next to the street light and the mirrors. The presets are Low, Medium, High, Extra, Max and Ultra, and each sets the mode too: Low is Vanilla, Medium Vanilla+ (what the game starts with), High to Max Enhanced with more switched on at each step, Ultra Enhanced with ray tracing and everything on. The launcher, the game's options, the Apple app and the quality optimiser follow it; every new text is translated into all the languages.
- **The clouds' shadow lies over the land** (Enhanced): the sun is no longer one number for the whole picture. A field of nine kilometres, worked out from the very heaps the sky shows and kept in the sky table's texture, makes patches of sun and shade on the ground that the wind moves, and the air scatters the sun's shafts through the gaps of a broken sky as far as seven kilometres.
- **Walking is paced like a person's**: 1.4 m/s walking and 3.8 m/s running, reached over about a second and stopped over a few steps instead of at once, turns are curves, and the eyes glide over kerbs and steps instead of following every footfall (`OMSI_FOOT_WALK` and `OMSI_FOOT_RUN` set the paces).
- Arcs where current is broken (a shoe leaving the wire, a pole coming off, an insulator or a frog crossed under power): a flash on the wire and the roof, sparks, a crack; the shoes hiss on the wire with the current and the speed; contactors click, shoes and poles thud. New variables for models: `Trolley_Pole<k>_Elev`, `_Azim`, `_State`, `Trolley_Volts`, `Trolley_Amps`, `Trolley_Powered`, `Trolley_Notch`, `Trolley_Arc`, `Trolley_Hiss`.
- The bus preview of the launcher draws two pictures ahead while it turns, the JPEG is made on its own thread, and a turning bus is drawn smaller on a phone (the picture at rest is sharp), so the preview keeps up with the screen instead of about 20 pictures a second.
- **The city map closes with its cross or Escape only**: a click beside it no longer closes it.
- **Troubleshooting** in the launcher's Help, below Tutorials: what is wrong with the installed content, by the bus or pack it concerns - packs waiting for their bus, buses taking parts from packs that are not installed, map HOF files that could not be added, failed installs. "Scan again" looks afresh after installing what was missing and says how many problems are gone; the menu entry shows how many are left.
- **The route preview no longer runs doubled**: the timetable lists the lanes of a road side by side (three lanes of the same stretch, one after another), which were all drawn, and a change of lane showed as a step sideways. The route is one line now, with a smooth curve over where it changes lane, as the game's own map draws it.
- No text of the launcher is set in capitals any more (the section titles, the menu's groups, the destination sign); the stops under a trip in the roadbook have a rail and dots of their own that no longer run over their times, a trip's depot-run label no longer sits on its stops, the panels of the Drive page keep their width on a small window, and the menu folds to its icons on a narrow one.
- A rail vehicle moves by the forces of railway engineering instead of the road tyres of a bus and a made-up pull: running, air and curve resistance, the grade under it, wheel-rail adhesion (less on wet rails, leaves, snow and ice), drive and brakes limited by it - the driven wheels spin, the braked ones slide - and a tram's magnetic track brake. A vehicle scripted for the AI only is driven by the pedals with the typical performance of its class.
- **Bogie frames of mods** (the Tatra T6B5's `_trail.bus`: no `[boogies]`, one axle, one coupling named) hang by their origin on the coupling of the car they belong to, as the mod is made for OMSI 2, and stand on the track by their own wheelbase: they stood two to three metres out of place, both under the middle of the car.
- Cars of a train no longer jump back and forth by a few centimetres in bends: a part just behind the vehicle was placed from the last 10 cm sample of the track behind it, a part just ahead from the track itself.
- **Tyres hold across by slip angle**, as real tyres do, instead of OMSI's rule that the body follows its wheels exactly until the grip is passed: the contact patch lags behind the wheel and pulls back by the cornering stiffness, up to the grip shared with the longitudinal force by the friction ellipse. A bus follows its wheels at walking pace, turns in over 0.15-0.2 s at 50 km/h, understeers a little at 80 km/h (yaw rate 0.92-0.98 of the geometry's), loses its hold across on a locked or spinning wheel. `cargo run -p omsi-sim --example handling -- <file.bus>... --kmh 50 [--realistic]` compares the two models; `OMSI_REALISTIC_ROAD=1` turns it on for one run.
- **The navigator's map and the city map draw the whole ground again** ([#3266](https://github.com/openOMSI-org/openOMSI/pull/3266), [#3301](https://github.com/openOMSI-org/openOMSI/pull/3301)): the cap on the map's geometry that came in with the out-of-memory fix ([#3249](https://github.com/openOMSI-org/openOMSI/pull/3249)) cut the ground off on large maps, so the in-game map showed street names over an empty background. The cap is gone from the panel and the city map; [#3249](https://github.com/openOMSI-org/openOMSI/pull/3249) is open again for another fix.
- **The launcher's bus preview shows what the repaint chooses**: the showroom gives the bus its paint scheme as the game does, so the scheme's `[setvar]`s are set again after the scripts every frame ([#3183](https://github.com/openOMSI-org/openOMSI/pull/3183)). Set once when the bus was made, a script that writes them itself overwrote them: the Kajosoft Jelcz M125M's JGSP Novi Sad repaint showed the standard doors' white frames in the preview instead of the glazed doors.
- **AI traffic: a bus waiting to move over a lane no longer jams the street for good**: a car standing in the lane a bus (or a car whose lane closes) waits to move over into took that vehicle for its lead, so it stayed put even at a green light while the bus waited for it to move - both stood for minutes, and the queue behind them with them (Spandau, the four-lane approach of the Altstadt junction). The standing car now drives on after a few seconds. And a route's lane change that the bus missed its window for (the two lanes no longer run side by side) is made all the same after 12 s of waiting instead of never. `OMSI_DEBUG_STUCK` also says why a lane change waits.
- **Timetable buses no longer stand at the stops that have no time of their own**: a bus that came to such a stop early waited for the timetable's estimate (the running time shared out by distance) up to two minutes, so a third of the buses stood over a minute with nobody getting on or off. They now wait the length of a boarding at the most; the stops the timetable gives a time, the first and the last stop keep their wait. `OMSI_DEBUG_STOPS` also logs how long each bus stood and why.
- **The launcher's bus preview is no longer misted over**: the bus is made for it (the showroom and `--export-glb`) on a mild day (22 °C) instead of at 0 °C, the temperature a vehicle starts at without a weather. The Kajosoft Citaro, Citaro Facelift and 628 fog every window up in their {init} below 10 °C, so the preview always showed them misted; in the game the map's weather still decides.
- **A bus's light cone on the road is a soft pool again in Vanilla and Vanilla+**: a self-lit blended layer (emissive white, such as the KVB Citaro's `lichtkegel.o3d`) is given the alpha that matches Omsi.exe's mix of gamma-encoded colours. Mixed in linear light on the sRGB target, the faint cone (alpha 0.14 on average) was a flat beige sheet over the road, about 40 % brighter than in OMSI 2.
- **Köln's houses on Engeldorfer Straße are back**: a binary `.x` string ended by a WORD token, as PAD-Labs' exporter writes it, no longer swallows the `}` after it. Read as the documented DWORD, the house's material never closed, the model failed to load and its 25 binary houses were missing since 0.3.38.
- **Bus shelters show through a bus's windows** instead of a white shape in Enhanced and Enhanced+, or nothing at all in Vanilla. Grundorf's shelter is drawn "after vehicles" (`[rendertype] 4`) and the bus's panes write depth, so it failed the depth test behind them, and the houses behind it were already gone. The blended draws of `[rendertype]` 3 and 4 are now sorted far to near with the normal objects', after the opaque draws of all of them ([#2852](https://github.com/openOMSI-org/openOMSI/issues/2852)).
- **London's road markings and red bus lanes stay put** when the camera moves away or around the bus ([#2853](https://github.com/openOMSI-org/openOMSI/issues/2853)). A cut-out strip lying just over the driving surface its spline declares (`[heightprofile]`) is paint, as one lying just over the spline's line already was: Road-hog123's British markings lie 5 mm over a 10 cm surface. A spline laid on an earlier spline of its tile at the same height is drawn over it, as Omsi.exe draws a tile's splines in order (the red lanes over the road fought it for every pixel). And a spline laid on a `[surface]` object - Crystal Palace's carriageway is one, its own tile's or the next one's - is pulled towards the eye as the object is, which covered it from a few metres away or from above. `OMSI_NO_DRAW_ORDER_DEPTH=1` turns the last two off.
- **The MAN DN95 is no longer a mirror**: a texture a model asks for as `.dds` that is not there is missing, as in OMSI, instead of being stood in for by a `.bmp` or another file of the same name. The DN95's body asks for an `envmap.dds` it does not ship; its `envmap.bmp` (the windows' sphere map) at the body's factor of 10 drew the paint over with the reflection, where OMSI shows the paint without one.
- With the new setting **Realistic road physics** on, road damage shakes a vehicle again: cobbles, gravel, dirt, grass, snow and the joints of a concrete-slab road lift each tyre by what ISO 8608 gives such a road, where only the map's geometry did before ([#3069](https://github.com/openOMSI-org/openOMSI/issues/3069)).
- **Timetable buses no longer stall the frame when they appear**: a departure whose vehicle was not on the graphics card yet (the clock set far ahead, a type nothing read ahead) had its meshes and textures built on the frame it appeared on - 120 to 300 ms; it is tried again once the vehicle is read ahead. A parallel loop's caller sleeps while it waits for the items a helper has started instead of spinning.
- **Review of the round**: `hwban 3 session é` (a multi-byte last character of a duration) no longer stops a dedicated server; the hardware bans are given to every session a game hosts, not only the first of a run; the host passes voice frames only to players that have voice chat on (they ask `voice?`), the TACKs of the traffic table are rate limited and a player's open microphone sends nothing while an administrator has silenced them; a `server.cfg` without `voice` has voice chat off (the new file says `voice = 1`); the table of the AI traffic is written only while the game hosts, and a bus's sign in it once a second; the seated people's seat-back shift is worked out when the pose changes; the wire of a trolleybus map is read once per map, drawn once per scene, and a bus's poles and frogs leave the scene with it; a microphone that is gone is not opened again every few milliseconds; the beam shadows look up their map only where a beam shines; a plain bus is a trolleybus only if its "pole" meshes are long and thin; the server list does not let a registration take over another server's address, counts an IPv6 sender by its /64, and compares a status page's name as the listing has it; the glass of a dented body keeps the snow's coordinates; new texts in the options and the player list have translations.
- **Trolleybus poles stay on the wire over bumps and joints**: a shoe that a bump lifts for a moment no longer comes off (it has to stay out of its groove for 0.15 s, or leave it by more than three groove depths), the groove is deeper and bends are taken faster.
- **Trolleybus poles no longer come off at the map's frogs whatever the bus does**: a map's junctions are drawn with the three wires up to a metre apart or a branch started from the middle of another wire, and only one frog in seventeen was found (on Bronitsyn), so the shoe ran off a wire that simply ended. The ends of a junction within 1.2 m are now one point, a branch from the middle of a wire cuts it into a frog, and the contact plate before a frog is found over short spans and joints too (a toe cut shorter than 9 m never set its frog). Power on takes the branch, coasting goes straight.
- The windows of Russian and other mod buses whose texture names are not plain ASCII (the ZiU-682G, the Trolza Optima) are see-through again: the `.o3d` keeps a texture's name in the file's own bytes and the `.cfg` as text, so a mod's `[matl_alpha]` for its glass never found the slot and the windows were opaque. Both names are now compared as the bytes Omsi.exe sees.
- The snow on the windscreen and the cab's windows no longer comes out in swirls and bands, with huge flakes beside tiny ones: the film's coordinates in metres were laid out by a normal worked out at every corner, which turned the plane a little at each vertex of a curved pane. A pane (the film triangles that meet) is unrolled on one plane now.
- Road markings no longer disappear and come back as the bus drives towards them ([#3085](https://github.com/openOMSI-org/openOMSI/issues/3085)). The depth a road is pulled towards the eye (so that it wins over the ground beside it) stopped growing 12 m out, and across the long triangles of a straight road that turned its plane into a bowl: from a low eye the road came in front of the lines on it from 25 m on. A flat surface keeps the same share of the way to the eye all the way now; upright faces (a pole a mod tagged as a road) keep the cap. A new graphics test lays a marking on a road of 200 m triangles and looks at it from 4 to 100 m.
- The mirrors show the sun's shadows and the smoke and spray again ([#3051](https://github.com/openOMSI-org/openOMSI/issues/3051)): they look up the shadow maps the window last drew instead of drawing their own, and draw the particles the window has. Settings `mirror_shadows` and `mirror_particles` (both off until switched on: they cost a little in every frame, most in snow) turn them on for a slower machine, as OMSI 2's "No PS in reflections" did.
- A bus that declares air pressure variables it never sets (a shared varlist) no longer says "Air pressure is low" and "keep the engine running" ([#3030](https://github.com/openOMSI-org/openOMSI/issues/3030)): only a script that assigns the spring brake or tank pressures counts as a bus with air brakes.
- Passengers drawn with the high quality avatars no longer sit with their backs through the seat back (robes and long hair on women's avatars): a seated avatar is moved forward on the seat as far as its back needs. Waiting passengers stand with a phone in hand and arms crossed less often.
- On a phone held upright an open page of the game menu (Options and the rest) takes the whole width: beside the menu it was a strip of switches with no names. Its tabs go on a second row instead of being cut off, and the cab panel's buttons fit a narrow screen (they ran off both sides). The top row of buttons shrinks to fit instead of running into itself in the free camera and on foot, and the cab panel's backdrop covers all its buttons.
- Photo mode no longer lies under the driving controls (the wheel and pedals were drawn over its panel) and its pages are no longer a 40-pixel strip on a phone held on its side.
- **Headlights and lamps in fog and rain**: a bright beam no longer carries a pattern of stripes (the soft light the fog scatters is upsampled with a weight that did not tell a road's rows from a step in depth), and the picture of a beam's cone and of a lamp's glare is smoothed between its texels, so a large cone has no stair of small squares along its edge. The classic picture's lamp glare ends softly at its sprite's edge, whatever the bitmap does there.
- **Tyre spray comes only from puddles**: the stock cars' and buses' wheel spray no longer sprays on every wet road from the first drops, and it stops as the puddles dry out after the rain instead of lingering until the road is quite dry. In snow the scripts' own slush is kept.
- **The frame rate no longer collapses after half an hour on DirectX 12 and Vulkan:** the game kept its own total under the video memory setting while the card filled up anyway - the room wgpu's allocator keeps free in its 128-256 MB blocks was left out of the bound, and a block goes back to the driver only once it is empty. Far textures made smaller left holes, the textures coming back needed bigger ones, so new blocks opened: on a 12 GB card set to 6 GB the driver held 11.5 GB after 18 minutes (5.7 GB of it holes), the card paged, 10 fps. The holes now count against the bound, so textures come back only into room the card really has, and every card gets the allocator's small blocks (8-64 MB), which empty and go back to the driver ([#2295](https://github.com/openOMSI-org/openOMSI/issues/2295)).

### Multiplayer

- **LAN protocol 15**: a game of this version cannot join a session of an older one, nor the other way round (the AI traffic goes as a table, see below). Update the game on both sides.
- **Multiplayer: public servers, the player list, machine bans**
- **A server can list itself in a **public server list****: `public_server = 1` in `server.cfg`. It registers with a list service (`services/serverlist`, a Cloudflare Worker, which reads the server's own status page for a challenge before listing it, forgets it three minutes after its last word and limits abuse) and says it is alive every minute; Multiplayer -> Servers in the launcher shows the **Public servers** with name, map, players, version and message, and joins or adds one. The name, message and icon are checked by the same moderation on the server before it registers (a name or message that is flagged is refused, with the reason in the log) and in every launcher before it shows them (a flagged text is covered). A custom icon is shown only when the viewer's own image check cleared it; none is bundled yet (the library planned for it cannot be shipped, see docs/SERVER.md), so the public list shows the default icon for now.
- **Voice chat, built in** - off until switched on (Settings → Sound → *Voice chat*, or the game menu's Sound page). Hold a talk key to speak: *nearby* (V) is heard by the players within the session's range (75 m by default) from where you stand, quieter with the distance and muffled when one of you is in a bus and the other is not; the *radio* (B) is heard by the whole session, in a small speaker's voice with a click when the key goes down and up. Both keys are in Controls (and bindable on a controller button; on a touch screen the cab panel has two talk buttons). The microphone is opened only while a key is held (or with the open microphone setting), the system asks for it the first time voice chat is switched on, and nothing is recorded or stored. Opus at 24 kbit/s with a jitter buffer and loss concealment; the host only relays the frames (LAN datagram `0xBC`, session-bound, rate limited) and needs no audio device, so dedicated servers carry voice too. The screen shows when you transmit and who talks; every other player can be muted in the game menu's *Player list* (kept by name), or all voices at once; an admin can silence a player for the whole session (`vmute` / `vunmute`, a button on the player's page). New `server.cfg` keys `voice`, `voice_radio` and `voice_range`, see [docs/VOICE.md](docs/VOICE.md) and [docs/SERVER.md](docs/SERVER.md). **Removed:** the GreenTeaSpeak plugin and its settings (`voice_chat`, and `voice_server_uid`, `voice_channel`, `voice_channel_password` in `server.cfg`, which an old file may keep: they are ignored and the log says so).
- The AI traffic goes over the LAN as a table of vehicles under ids of their own (LAN protocol 15, [docs/TRAFFIC_TABLE.md](docs/TRAFFIC_TABLE.md)): the host's traffic writes it after each step and every player is told the cars within 650 m of it as changes of what it acknowledged, the nearer ones more often, within 40 KB/s. A car that stands costs nothing, a moving one 6-7 bytes ten times a second (the old frames took 17 and told every car in range every time), lost datagrams are told again, and the clients draw the cars between and past the host's poses. A car's id is not reused for 30 s. A game of the older protocol is turned away with the usual message. The script context of the host's cars goes to each client often at first and then now and then, not every tenth of a second for every car in range.
- **Every player sees another player's bus exactly as its driver has it** — the same files, the same paint — fetched from the server by its content when they lack it; a dedicated server keeps every version its players send (`accept_uploads`, `uploads_keep_days`) and may leave the map for the players to install (`share_maps`: a new `server.cfg` says 0; one without the key goes on passing the map on, as before).

## 0.3.41 - 2026-10-24

An optional **AMD FSR 1** upscaler joins the graphics settings, the daytime Enhanced picture does less work, and the video-memory bound counts the room the driver really holds. BRT buses serve their scheduled platform and passengers choose doors on its side. The launcher gets a timetable editor on the map and a Troubleshooting page; the game gets a pause menu in categories, native Tobii head tracking on Windows, rail dynamics and optional realistic road physics. Georgian joins the languages, and more of the interface is translated.

This is a large beta since 0.3.40. The new lighting and physics need testing on more maps and machines; the known limits below matter, particularly for night driving in Enhanced.

### New and improved

- **Less CPU work for traffic out of sight:** a car outside the view and beyond 150 m runs its script a few times a second; timetable buses keep their usual beat. Distant, unseen cars' lamps and smoke leave the frame's work, and placing dormant traffic no longer scans every lane for every car ([#3228](https://github.com/openOMSI-org/openOMSI/pull/3228), @turbo-devv).
- **Malformed texture data is refused before upload:** an incomplete mip chain uses the stand-in texture and leaves a bounded diagnostic in the log, with fault counters in the status report. This closes one route to invalid textures; it does not establish the cause of every colourful-artifact report ([#3087](https://github.com/openOMSI-org/openOMSI/pull/3087), @isaacsa2).
- **Daylight adapts to a dark cab or underpass**, then settles after coming back into the light. The stronger correction gives way through dusk so a night is not brightened into a day. Enhanced adds wider street-scale occlusion and bounced light: a lit wall can light the street across from it. **Street light is off by default**; On enables it and Auto avoids small or shared graphics chips. The optimiser can turn it off when the GPU is the limit. Cloud layers follow the weather model, and thunderstorms bring flashes and thunder heard from their direction after the distance's delay ([#2925](https://github.com/openOMSI-org/openOMSI/pull/2925), @turbo-devv).
- **Trips already under way appear when a map loads**, including yesterday's trips still running after midnight, and departure boards show a night duty in clock time. Free textures stay with the material item they belong to, and font selection follows Windows' order more closely ([#2910](https://github.com/openOMSI-org/openOMSI/pull/2910), @turbo-devv).
- **Fleet number and plate can be typed while playing** in the vehicle options ([#3108](https://github.com/openOMSI-org/openOMSI/pull/3108), @trolley813). A launcher option starts the cab at a comfortable **22 °C** ([#3184](https://github.com/openOMSI-org/openOMSI/pull/3184), @wmalt). Maps with separate arrival recordings play that half of the announcement on reaching the stop ([#3049](https://github.com/openOMSI-org/openOMSI/pull/3049), @anonchan958).
- **Realistic road physics is an option, off by default:** wind, air density, tyre grip, load and the road surface affect the bus. Cobbles, gravel and concrete joints shake it, and tyres build lateral force by slip angle rather than holding the body's direction exactly to the wheels. Rail vehicles gain tram, metro and train classes, track-dependent resistance and adhesion, and pantograph control with **Shift+P**. Bogie frames and coupled cars sit more consistently on the track ([#3150](https://github.com/openOMSI-org/openOMSI/pull/3150), @turbo-devv).
- **Tobii head tracking works natively on Windows**, through the installed Tobii Experience, without opentrack. Automatic uses opentrack while it sends, otherwise Tobii. F1 centres the view; smoothing and a dead zone calm small movements, and a lost tracker glides the view home. Looking towards the screen's edges can turn the view further ([#3025](https://github.com/openOMSI-org/openOMSI/pull/3025), @Luc-nbr).
- **Build a timetable on the map:** click stops to make a trip, see the route it would drive, search for a stop and inspect the trips that call there. Edited stop lists become new trip files, keeping the map's own. Route previews draw one smooth line instead of every parallel lane; roadbook stops and times have room of their own ([#3199](https://github.com/openOMSI-org/openOMSI/pull/3199), @turbo-devv).
- **The map moves and zooms more smoothly on large maps:** smaller ground data and detail chosen for the current zoom replace repeated large requests. On the author's St-Servan test, zooming went from 19–22 fps to 50–53 fps, and the first ground arrived in 0.13 s instead of 2.2 s ([#3239](https://github.com/openOMSI-org/openOMSI/pull/3239), @ThiBot77). French road texture names are recognised too, restoring missing streets on maps such as HCM ([#3240](https://github.com/openOMSI-org/openOMSI/pull/3240), @ThiBot77).
- **The desktop pause menu has categories and action cards**, with keyboard navigation and the original actions behind them. Returning from a submenu keeps the category it came from. Settings headings are easier to find, scrolling over a slider no longer changes its value, and dragging interface scale keeps the pointer steady ([#3192](https://github.com/openOMSI-org/openOMSI/pull/3192), [#3234](https://github.com/openOMSI-org/openOMSI/pull/3234), @shamough1792).
- **Each roadbook trip shows its duration** ([#3032](https://github.com/openOMSI-org/openOMSI/pull/3032), @isaacsa2), and tours sharing a number on different days can be selected independently ([#3136](https://github.com/openOMSI-org/openOMSI/pull/3136), @wmalt). **Add-ons**, under Content, lists community projects and links to their websites; it does not download or install them ([#2875](https://github.com/openOMSI-org/openOMSI/pull/2875), @Luc-nbr).
- **MileZero radio gets a card over the navigator**, or inside the information bar when the navigator is off: cover, song, artist, playback and a song-request field. Stream quality adapts to the connection. The integration is off by default and playback starts only when Play is pressed ([#2978](https://github.com/openOMSI-org/openOMSI/pull/2978), @turbo-devv).
- **Enhanced at night can cost more with the new lamp shadows.** The [#3204](https://github.com/openOMSI-org/openOMSI/pull/3204) review measured 79.8 → 71.6 fps on a Radeon 680M in Spandau at 22:00. Daytime savings do not establish a night-time improvement. AI headlamp beams in tunnels and covered depots by day in Enhanced also remain a known limit.
- **Rail changes apply beyond the optional road-physics setting.** A Tatra T6B5 still needs its pantograph raised with Shift+P, and noisy tight track joins can make AI trams' speed limits too low.
- **The final VRAM change needs 40–60-minute driving tests**, especially on Windows with DirectX 12 and Vulkan and an 8–16 GB card. Report dedicated/shared GPU memory, distant texture recovery and frame times.
- **FSR needs broader GPU and headset testing.** Compare it with Standard at the same internal resolution, not only different quality presets. Report the backend, card, resolution, scene and sharpening.
- **BRT needs real-map retesting:** left and right platforms, adjacent opposite-direction stops, articulated buses, doors shut and open, and tile streaming.
- Please include **0.3.41**, the map and bus, graphics preset and API, hardware, steps to reproduce and the relevant log in an issue. Existing reports about low frame rate or artifacts are not all resolved by this beta.

### Fixes

- **Less GPU work in Enhanced by day:** AI cars no longer lend the scene all their headlamp beams whenever their lights are on. Clear daylight needs none; gloomy daylight keeps the nearest few, and night keeps the previous range. On the author's Apple M4, the GPU time of a clear-day frame fell from 27.4 to 13.0 ms, and a heavy grey snow day from 27.6 to about 19 ms. These are measurements of those scenes, not a gain every PC should expect. AI vehicles use suitable lower levels of detail from their models, and a timetable bus waits for its vehicle to be read ahead instead of building it on the frame it appears on ([#3187](https://github.com/openOMSI-org/openOMSI/pull/3187), @turbo-devv).
- **More lamps cast shadows in Enhanced and Enhanced+:** street lamps, headlamps and the player's saloon lights shade the surfaces behind seats, rails and other parts. Ground no longer adds the map's baked light pools on top of the physical lamps, which made snow glow orange through buildings. Mirrors stay **Standard** by default; *As the picture (Enhanced)* also shows the sun's shadows using the window's maps ([#3151](https://github.com/openOMSI-org/openOMSI/pull/3151), @turbo-devv).
- **Follow-up fixes keep the lighter presets light:** Vanilla and Vanilla+ no longer allocate the Enhanced lamp-shadow atlas, AI headlamp beams return in the plain pictures, and trolleybus sparks no longer displace street lamps' shadow maps ([#3204](https://github.com/openOMSI-org/openOMSI/pull/3204), @thibautgnt). Bright bounced-light spots no longer show as squares ([#2928](https://github.com/openOMSI-org/openOMSI/pull/2928), @turbo-devv).
- **BRT buses serve the platform of their scheduled stop**, rather than a nearby stop on the opposite carriageway. Passengers choose entrances facing that platform, including while those doors are shut, instead of crossing the bus to chase an open door on the other side. The vehicle's scripts still control its physical doors. Real-map docking, articulated buses and passenger paths need further testing ([#3227](https://github.com/openOMSI-org/openOMSI/pull/3227), @isaacsa2).
- **Trolleybus poles follow more legacy models**, including animated poles and poles on an articulated rear section. Interior light meshes no longer turn a diesel bus into a trolleybus. Loosely drawn wire junctions and branches in the middle of a span are recognised as frogs, and their contact plates work across short spans: power takes the branch, coasting goes straight ([#3165](https://github.com/openOMSI-org/openOMSI/pull/3165), @thibautgnt; [#3201](https://github.com/openOMSI-org/openOMSI/pull/3201), [#3196](https://github.com/openOMSI-org/openOMSI/pull/3196), @turbo-devv).
- **Other corrections:** Ctrl+T puts all the change still owed on the tray again ([#3218](https://github.com/openOMSI-org/openOMSI/pull/3218), @thibautgnt); cabin condensation reads the weather's humidity in the right units ([#3188](https://github.com/openOMSI-org/openOMSI/pull/3188), @turbo-devv); and the MAN DN95 shows its paint instead of a mirror body when its requested DDS reflection texture is missing ([#3246](https://github.com/openOMSI-org/openOMSI/pull/3246), @rcasaucao).
- **Troubleshooting is in Help:** missing bus parts, packs waiting for their bus, HOF problems and failed installs are grouped by the content they concern. Scan again after a repair; Model test is the second tab, with a searchable installed-mod list ([#3175](https://github.com/openOMSI-org/openOMSI/pull/3175), @paulbmueller).

### Performance

- **AMD FSR 1, when you choose it:** Settings > Graphics > Upscaling offers *AMD FSR 1 (experimental)* alongside the standard Catmull-Rom upscaler. Choose Ultra Quality, Quality, Balanced, Performance, Automatic, or the render scale, and adjust FSR sharpening. It changes while the game runs; the HUD stays at the window's resolution. Automatic follows the game's render-scale governor. The FSR passes are built only when first needed, and a driver that refuses them falls back to the standard upscaler. Sharpening at zero skips its extra pass. **A gain in frame rate depends on the card and the scene; FSR is not a promise of more fps.** The renderer covers triple screens and VR eyes, but headset testing is still needed ([#3147](https://github.com/openOMSI-org/openOMSI/pull/3147), @isaacsa2, with @thibautgnt).
- **Video memory counts the allocator's empty room too:** on DirectX 12 and Vulkan, textures made smaller could leave holes in large allocation blocks that the driver still held. Those holes were left out of the game's bound, so memory could keep growing until the card paged and the frame rate collapsed. They now count, and all cards use smaller allocation blocks. This targets the long-drive slowdown in [#2295](https://github.com/openOMSI-org/openOMSI/pull/2295); **long driving tests of the final change are still needed, and the setting is not a hard ceiling once textures reach their smallest size** ([#3198](https://github.com/openOMSI-org/openOMSI/pull/3198), @filiphajduch420).

### Project

- **Rain and snow fall again on OpenGL and ANGLE**, photo mode on ANGLE no longer switches multisampling off, and the city map is bounded more carefully so it does not exhaust weaker cards. Chrono events from both map locations count, and the timetable paper shows the next trip's departure ([#3121](https://github.com/openOMSI-org/openOMSI/pull/3121), @thibautgnt).
- **VR's timetable restores stop details, time arrows and trip-start controls** where the panel is wide enough ([#3216](https://github.com/openOMSI-org/openOMSI/pull/3216), @NotSourLimes). Map opacity fades its labels and map as well as its background ([#3141](https://github.com/openOMSI-org/openOMSI/pull/3141), @wmalt).
- **Georgian is a new interface language.** German, Spanish, French, Brazilian and European Portuguese and Russian gain complete catalogue translations, and more previously hard-coded notices and launcher text use the selected language ([#3126](https://github.com/openOMSI-org/openOMSI/pull/3126), [#3132](https://github.com/openOMSI-org/openOMSI/pull/3132), @thibautgnt). Serbian Latin is expanded and polished ([#3149](https://github.com/openOMSI-org/openOMSI/pull/3149), [#3185](https://github.com/openOMSI-org/openOMSI/pull/3185), @kizule), with missing Traditional Chinese controls and wording corrected ([#3130](https://github.com/openOMSI-org/openOMSI/pull/3130), @shamough1792). Please report wording and layout problems in your language.
- Plugins can read full tours, loaded HOF data and destination identifiers ([#3102](https://github.com/openOMSI-org/openOMSI/pull/3102), [#3186](https://github.com/openOMSI-org/openOMSI/pull/3186), @Burst99325), and leave the IBIS to the driver or type the current trip into it ([#2877](https://github.com/openOMSI-org/openOMSI/pull/2877), @Luc-nbr).
- Native helpers require the player's explicit approval for the plugin content; Lua can wait without blocking the game. Externally changed free textures reload in the background while an incomplete write keeps the last good image ([#3139](https://github.com/openOMSI-org/openOMSI/pull/3139), [#3103](https://github.com/openOMSI-org/openOMSI/pull/3103), @Burst99325).
- Legacy 32-bit OMSI plugins on Windows no longer open console windows; OMSI Presentation Tools is left out with a log message ([#3163](https://github.com/openOMSI-org/openOMSI/pull/3163), @thibautgnt). Closing the game stops frame stages instead of letting plugins load again on exit ([#3159](https://github.com/openOMSI-org/openOMSI/pull/3159), @shamough1792).
- The code moves to Rust 2024 ([#3137](https://github.com/openOMSI-org/openOMSI/pull/3137), @thibautgnt), and startup tests isolate Steam and temporary folders more reliably ([#3209](https://github.com/openOMSI-org/openOMSI/pull/3209), @NotSourLimes).

### Credits

- Thanks to **@turbo-devv**, **@thibautgnt**, **@isaacsa2**, **@Burst99325**, **@Luc-nbr**, **@shamough1792**, **@wmalt**, **@trolley813**, **@anonchan958**, **@kizule**, **@filiphajduch420**, **@ThiBot77**, **@rcasaucao** and **@NotSourLimes** for the work, fixes, translations and reviews in this beta, and to everyone testing and reporting what breaks.

## 0.3.40 - 2026-10-21

Trolleybuses arrive: a map can carry a real overhead wire, the poles are put up and taken down by hand, come off in a bend taken too fast and spark where the current breaks, and existing OMSI 2 trolleybus maps and add-ons run as they are. Online play gets built-in voice chat (nearby and radio, off until switched on), a player list in the game menu with kicks and machine bans, and a public server list. **Online play changes protocol (LAN 15): every player and the server must update to 0.3.40.** The game takes much less memory again, back under 0.2.30 on Grand Paris-Moulon. Trams can be driven again, the city map takes waypoints with numbered pins, and everything over the picture while driving looks like the navigator and resizes at its corner. The launcher lists the games that ended and how, shows the roadbook trip's stops, offers weather that fits the season and asks before sharing a map's HOF files; the iPhone, iPad and Mac launcher catches up with the desktop one. 32-bit OMSI 2 plugins such as BCS load again on Windows.

### New and improved

- **The roadbook shows the chosen trip's stops:** the selected trip opens into a timeline of its stops with their times, the first and last stops in bold, and every trip row shows its distance in km ([#3039](https://github.com/openOMSI-org/openOMSI/pull/3039), @isaacsa2).
- **Weather presets that fit the season:** in Drive, Day & weather, only the map's presets that fit the season are offered (no summer shower in the snow, no snow in spring), with a toggle to show the others; changing the season clears a preset that no longer fits and says so. Every tile shows its temperature, rain or snow and visibility ("15 °C - rain 68% - 200 m") with an icon for its kind ([#3052](https://github.com/openOMSI-org/openOMSI/pull/3052), @isaacsa2, @thibautgnt).
- **Trolleybuses on a real overhead wire:** a map can carry a wire (`trolley.txt` beside its `global.cfg`) with live and dead sections, insulators, frogs and the feeder's voltage drop, and a bus can carry real poles and a rheostat or asynchronous drive. A trolleybus only moves on current: a pole comes off where the wire lifts out of the shoe, when the bus strays too far from the wire's line or takes a switch the wrong way, in a bend taken too fast or where the wire ends; on a dead section the bus rolls on and stops. Frogs follow the current the bus draws at the contact plate, or its indicator. Breaking the current flashes, sparks and cracks, the shoes hiss on the wire and the contactors click. See `docs/TROLLEYBUS.md` ([#3099](https://github.com/openOMSI-org/openOMSI/pull/3099), @turbo-devv).
- **Poles by hand:** from the cab, a click on the pole switch takes a pole, it follows the cursor and the next click lets it spring onto the wire; from outside near the back of the bus, hold R to take a pole's rope and aim it, T pulls a pole down, Shift takes both. Poles stay on the wire over bumps and joints ([#3099](https://github.com/openOMSI-org/openOMSI/pull/3099), @turbo-devv).
- **OMSI 2 trolleybus content runs as it is:** a map's wire splines (as the Bronitsyn map draws them) become the wire network, and a trolleybus add-on that only shows folded and raised pole meshes gets real poles from those meshes and keeps its own drive, cut without current. Checked with the ZiU-682G and the Trolza Optima. See `docs/TROLLEY_MODS.md` ([#3099](https://github.com/openOMSI-org/openOMSI/pull/3099), @turbo-devv).
- **The cards over the picture look like the navigator, and resize at their corner:** the Shift+Y information bar, the timetable (Insert), the performance panel, the notifications, the notes, the frame rate, the tooltip and the chat box take the navigator's look, dark or light. The information bar, the timetable and the performance panel grow or shrink (0.6x to 2x) by the grip at their bottom right corner and keep that size, and "Stop requested" shows a bus icon ([#3065](https://github.com/openOMSI-org/openOMSI/pull/3065), @thibautgnt).
- **Winsenburg's Hauptbahnhof 4 crossing:** a bus asking at the second bus light alone now gets its green ([#3099](https://github.com/openOMSI-org/openOMSI/pull/3099), @turbo-devv).
- **Shadows in the headlamp beams (Enhanced):** the light that fog, rain or snow scatter from a headlamp or a street lamp is cut where a pole, a car or a wall stands in the beam ([#3099](https://github.com/openOMSI-org/openOMSI/pull/3099), @turbo-devv).
- **Smoother headlight beams in fog and rain:** no more stripes in a bright beam and no stair of small squares along a large cone's edge ([#3099](https://github.com/openOMSI-org/openOMSI/pull/3099), @turbo-devv).
- **Waypoints on the city map:** click anywhere on the Shift+M map, or on an orange entry point marker, to build a route through numbered pins in order, on the city map and the small navigator. Hover a pin to delete it, or clear them all; the timetable route comes back afterwards ([#3074](https://github.com/openOMSI-org/openOMSI/pull/3074), @shamough1792).

### Fixes

- **Games that ended stay listed, and say how they ended:** the Running games page lists under **Ended** the games of the last half hour: when each ended, and whether it quit normally, ended on an error (with its exit code) or was killed by Stop with its run not saved, with its log still there to open. **Stop** now says when it had to kill the game instead of "The game was stopped." every time ([#3033](https://github.com/openOMSI-org/openOMSI/pull/3033), @isaacsa2).
- **A map's HOF files are shared only when you say so:** the Mods page asks first ("Share with every bus" / "Not for this map") instead of copying a map's HOF files to every bus, which gave wrong destinations on buses with their own IBIS on maps like Grand Paris-Moulon with one HOF per company. When you choose a line, "Automatic" picks the HOF that carries it, from the bus or from the map's folder, and the game finds a map's HOF in its folder without any copy ([#3068](https://github.com/openOMSI-org/openOMSI/pull/3068), @thibautgnt).
- **The iPhone, iPad and Mac launcher has everything the desktop one has:** the settings it was missing (touch pedals, navigator widget and size, puddles, trip summary, mirror panels, seat position, radio stations, Optimise for my PC, Reset all settings and more), the Depot and Timetable pages, the key editor (with a hardware keyboard on iPad), notes and starred buses, line and tour search with next departures, the custom weather editor, METAR airports, a calendar, traffic up to 120 %, the guided multiplayer join and invite, the mods contents tree and the crash dialog's hints. Deleting a mod or a save now asks first ([#3054](https://github.com/openOMSI-org/openOMSI/pull/3054), @thibautgnt).
- **Android writes the support package:** Settings, Support package works on Android instead of failing with "the game program was not found"; the ZIP is saved in the openOMSI storage folder ([#3041](https://github.com/openOMSI-org/openOMSI/pull/3041), @isaacsa2).
- **The launcher's bus preview keeps up on phones:** it draws ahead while the bus turns and makes its picture on its own thread, so it follows the screen instead of about 20 pictures a second ([#3099](https://github.com/openOMSI-org/openOMSI/pull/3099), @turbo-devv).
- **Trams can be driven again:** a tram that starts at an underground stop (or under a roof or a bridge) stands on its track instead of floating out of sight, and trams whose cab sends its commands through control cables, such as the freeware Tatra KT4D, pull away when the driver accelerates instead of staying at 0 km/h, as on Übersee and Gråkallbanen ([#3045](https://github.com/openOMSI-org/openOMSI/pull/3045), @thibautgnt).
- **Walking paces like a person:** on foot you walk at 1.4 m/s and run at 3.8 m/s, speed up and stop over a few steps, turn in curves, and your eyes glide over kerbs and steps instead of following every footfall. A passenger on foot in a moving bus turns with the bus through a bend ([#3099](https://github.com/openOMSI-org/openOMSI/pull/3099), @turbo-devv).
- **Gauges and selectors no longer light up symbols that should stay dark:** a model that refers to a variable the bus never declares behaves as in OMSI 2 (its mesh stays shown, its material stays the base one), which removes an extra lit glyph over D/R on a gear selector ([#2951](https://github.com/openOMSI-org/openOMSI/pull/2951), @dev-Younte).
- **Layered dashboard displays no longer hide what is behind them:** on the ADL Enviro500MMC Facelift's dashboard display, an opaque strip no longer covers the front-door graphic when the door opens ([#3100](https://github.com/openOMSI-org/openOMSI/pull/3100), @shamough1792).
- **AI motorcycles no longer crawl:** light motorcycles such as the EX:Honda CG 125 keep normal traffic speeds instead of the bicycle limit of 15 to 21 km/h; bicycles are recognised by their file and folder name ([#3022](https://github.com/openOMSI-org/openOMSI/pull/3022), @isaacsa2).
- **No more false "Air pressure is low":** a bus that declares air pressure variables it never sets no longer asks you to keep the engine running ([#3099](https://github.com/openOMSI-org/openOMSI/pull/3099), @turbo-devv).
- **The game takes much less memory again:** the navigator's map no longer keeps its ground twice, and its layers are read from their cache file as needed instead of held in memory. Measured on a Linux PC with a Radeon 680M, on Grand Paris-Moulon with the map cache already built, the game's private memory went from 1058 MB in 0.3.39 to 824 MB, below 0.2.30's 845 MB. The navigator and the city map look the same ([#3066](https://github.com/openOMSI-org/openOMSI/pull/3066), [#3070](https://github.com/openOMSI-org/openOMSI/pull/3070), @thibautgnt).
- **Road markings stay on the road:** they no longer disappear and come back as the bus drives towards them ([#3099](https://github.com/openOMSI-org/openOMSI/pull/3099), @turbo-devv).
- **Mirrors can show shadows and smoke again:** two new settings, mirror shadows and mirror particles, both off by default since they cost a little every frame ([#3099](https://github.com/openOMSI-org/openOMSI/pull/3099), @turbo-devv).
- **Rain, snow and mist on the glass look right:** a drizzle leaves small beads and a downpour full, heavier runners; snow on the windscreen no longer comes out in swirls and bands, and every window holds the same snow instead of streaks on the rear windows of double-deckers and articulated buses. Misted windows follow the bus's own heating and fans, so they no longer stay foggy for good ([#3099](https://github.com/openOMSI-org/openOMSI/pull/3099), @turbo-devv).
- **See-through glass on Russian mod buses:** the windows of buses whose texture names are not plain ASCII, like the ZiU-682G and the Trolza Optima, are no longer opaque ([#3099](https://github.com/openOMSI-org/openOMSI/pull/3099), @turbo-devv).
- **Spline objects where the editor put them:** on maps with `[worldcoordinates]` (Shanghai and others), objects attached to a spline no longer stand off on tiles away from the map's middle ([#3099](https://github.com/openOMSI-org/openOMSI/pull/3099), @turbo-devv).
- **Seated passengers no longer sink into the seat back:** high quality avatars sit forward as far as their robe or hair needs, and waiting passengers hold a phone or cross their arms less often ([#3099](https://github.com/openOMSI-org/openOMSI/pull/3099), @turbo-devv).
- **The free camera by touch:** the cab panel's "Free camera" button flies out of the cab, a stick at the bottom left moves the camera, a finger drag looks round, two fingers zoom, and "Exit free camera" stays on screen ([#3099](https://github.com/openOMSI-org/openOMSI/pull/3099), @turbo-devv).
- **Photo mode for fingers:** its own shutter, stick and settings sheet, a drag turns the camera, a tap focuses and two fingers zoom; it no longer lies under the driving controls ([#3099](https://github.com/openOMSI-org/openOMSI/pull/3099), @turbo-devv).
- **The game menu fits an upright phone:** an open page takes the whole width instead of a strip of nameless switches, its tabs wrap to a second row, and the cab panel's buttons fit a narrow screen ([#3099](https://github.com/openOMSI-org/openOMSI/pull/3099), @turbo-devv).
- **32-bit OMSI 2 plugins load again on Windows:** `.opl` plugins with a 32-bit DLL, BCS among them, run again: the Windows zip and installer now contain `omsi-plugin-host32.exe`, the small program the game runs them in ([#3122](https://github.com/openOMSI-org/openOMSI/pull/3122), @thibautgnt).

### Multiplayer

- **A new online protocol (LAN 15):** 0.3.40 cannot join a session of an older version, nor the other way round. Update the game on both sides. AI traffic now goes as a table of vehicles: a standing car costs nothing and a moving one 6 to 7 bytes ten times a second instead of 17, and clients draw the cars smoothly between the host's updates ([#3099](https://github.com/openOMSI-org/openOMSI/pull/3099), @turbo-devv).
- **Voice chat, built in:** off until switched on (Settings, Sound, Voice chat). Hold V to speak to the players nearby (75 m by default, quieter with distance and muffled between inside and outside a bus) or B for the radio heard by the whole session. The microphone is open only while a key is held, nothing is recorded, dedicated servers carry voice too, and every player can be muted. It replaces the GreenTeaSpeak plugin, whose settings are now ignored ([#3099](https://github.com/openOMSI-org/openOMSI/pull/3099), @turbo-devv).
- **A player list in the game menu:** everybody's name, what they drive and who is host, admin or moderator; the host and a server's admins and moderators can kick a player with a reason, ban their machine for the session, a day, a week or for good, and lift bans. A machine ban follows a hash of the machine's id (never the id itself), whatever name or address the player comes back with ([#3099](https://github.com/openOMSI-org/openOMSI/pull/3099), @turbo-devv).
- **A public server list:** with `public_server = 1` in `server.cfg`, a server lists itself, and Multiplayer, Servers in the launcher shows the Public servers with name, map, players, version and message, ready to join. Names and messages are checked by moderation before they are shown; custom icons stay the default one for now ([#3099](https://github.com/openOMSI-org/openOMSI/pull/3099), @turbo-devv).

### Project

- A new feature's pull request now shows its frame rate and memory next to the current release and 0.2.30 ([#3071](https://github.com/openOMSI-org/openOMSI/pull/3071), @thibautgnt).
- `OMSI_PRESENT_MODE` picks the swapchain's present mode for testing Vulkan vsync tearing reports, and the TLS libraries are updated for two security advisories ([#3099](https://github.com/openOMSI-org/openOMSI/pull/3099), @turbo-devv, @thibautgnt).

### Credits

- **@turbo-devv**, for trolleybuses on a real wire, built-in voice chat, the player list with machine bans, the public server list, beam shadows and a long list of fixes to glass, snow, rain and phones.
- **@isaacsa2**, for the ended games list, the roadbook's stops, weather that fits the season, the Android support package and AI motorcycles at their real speed.
- **@shamough1792**, for waypoints with numbered pins on the city map and a dashboard display fix confirmed with its reporter.
- **@dev-Younte**, for another careful, well-tested match with OMSI 2, this time on undefined model variables.

## 0.3.39 - 2026-10-20

The launcher gets back much of what the game's own launcher did: a server list that refreshes itself, mods that say what they hold, a start bar with the first trip and the start place, METAR for any airport and a depot file that stays when you change buses. The roadbook now reads from the first stop to the last, and the IBIS box marks the route code to type. The left, right and front mirror panels are chosen in the options and cost less to draw, the Shift+Y information bar is redesigned, and a new option chooses which vehicles show smoke. The city map gets a compass to turn it north up or heading up. At night in fog the headlamps no longer draw a grid of squares, raindrops on the glass are bigger, and destination displays keep their text from afar. Plugins can close stops for a diversion, add lines to the pause menu and read the navigator's distance to the next stop.

### New and improved

- **A mod's row says what it holds:** on the Mods page, each mod lists the game folders it put things into and how many ("Vehicles 1, Fonts 2, Texture 1"), and "found in the content folder" for one copied there by hand. Opened, its contents are grouped by those folders ([#3035](https://github.com/openOMSI-org/openOMSI/pull/3035), @isaacsa2).
- **The start bar says what you are about to drive:** the Drive start bar shows when the duty's first trip runs and how long it is ("09:27 - 10:00 | 11.0 km"), where the bus is put down and the parts the bus lacks. Starting while a game already runs asks for a second click within 6 s, since its window may be hidden behind the launcher. An entry point with no name is shown as "Entry point N" instead of a blank ([#3037](https://github.com/openOMSI-org/openOMSI/pull/3037), @isaacsa2).
- **METAR for any airport:** in Drive, Day & weather, Current weather, any four-letter ICAO code can be typed, even when the airport is not in `ICAO.txt` or the list is empty ([#3038](https://github.com/openOMSI-org/openOMSI/pull/3038), @isaacsa2).
- **A hand-picked depot file stays when you change buses,** as long as the new bus has a file of the same name, and an expanded bus family shows how many paints each version has ([#3040](https://github.com/openOMSI-org/openOMSI/pull/3040), @isaacsa2).
- **The roadbook reads from the first stop to the last:** each trip is drawn as a journey line, its first stop with an open ring above its last stop with a filled dot, with the arrival time under the departure time; the route map marks the ends the same way. The IBIS box highlights and lists first the route code that fits the chosen trip, so you no longer type the code of the other direction. Narrow windows of every layout and the phone trip list are laid out better too ([#2936](https://github.com/openOMSI-org/openOMSI/pull/2936), @thibautgnt).
- **A duty date outside the map's years is kept:** a date you type or pick in Date stays when you change line, tour or trip, even if the map's `[years]` end earlier (for example 2025 on a map that stops in 2000). Dates taken from the computer's clock still fit the map's years ([#2979](https://github.com/openOMSI-org/openOMSI/pull/2979), @FakeShxp3).
- **Mirror panels in the options, cheaper to draw:** the Camera page of the options and of the launcher has **Mirror panels on the screen** (Off, Left, Right, Left and right, Left, right and front) and an **Arrange the mirror panels** button, so Ctrl+M is no longer needed to find them. The new front panel shows the bus's front mirror. The mirrors now take 0.5 to 0.6 ms a frame with the side panels on instead of 0.7 ms (1.0 ms in 0.2.30) ([#3004](https://github.com/openOMSI-org/openOMSI/pull/3004), @thibautgnt).
- **A new Shift+Y information bar:** a compact, rounded bar of two rows, route, time, speed, speed limit (in red) and delay on the first, distance, odometer, temperatures, passengers and fuel on the second, with its labels in every language ([#2997](https://github.com/openOMSI-org/openOMSI/pull/2997), @shamough1792).
- **Choose which vehicles show smoke:** a new **Vehicle smoke** option, in the launcher and in the game's Graphics menu, shows smoke from all vehicles (the default), only your own bus with its articulated sections, or none. Scenery smoke and glowing effects stay as they are ([#2982](https://github.com/openOMSI-org/openOMSI/pull/2982), @isaacsa2).
- **Bigger raindrops on the glass:** fewer, larger drops replace fine specks on the windows, so the wipers' sweep shows clearly ([#3024](https://github.com/openOMSI-org/openOMSI/pull/3024), @FakeShxp3).
- **Articulated buses cost less per frame:** the bellows are bent from weights worked out once, about 0.17 ms less a frame with the MAN Lion's City G ([#2988](https://github.com/openOMSI-org/openOMSI/pull/2988), @ThiBot77).
- **A shorter hitch when a very detailed object comes into view:** Grand Paris-Moulon's LED street lamps went from 25 to 10 ms of one frame ([#2990](https://github.com/openOMSI-org/openOMSI/pull/2990), @ThiBot77).
- **A compass on the city map:** a compass bottom right points to real north, and a click turns the map: **Map** (as drawn, the default), **North** up (where the map's package says which way north lies) or **Heading** up, turning smoothly with the bus. Street names, stops, vehicles and the route turn with it ([#2896](https://github.com/openOMSI-org/openOMSI/pull/2896), @lmonteirotech).
- **The city map's chips are drawn once:** the city name and the "You are on" street chip were drawn twice a frame; their backdrop is now as see-through as meant and the map does a little less work ([#3002](https://github.com/openOMSI-org/openOMSI/pull/3002), @thibautgnt).
- **Plugin lines in the pause menu:** `omsi.ui.set_menu_item()` puts up to 3 lines of a plugin, each with an icon, in their own group of the Esc menu, reachable by mouse, keyboard and controller; choosing one sends a `menu_item` event ([#2878](https://github.com/openOMSI-org/openOMSI/pull/2878), @Luc-nbr).
- **The navigator's distance to the next stop:** `omsi.info()` has `next_stop_distance_nav`, the distance along the navigator's route in metres, next to the straight-line `next_stop_distance` ([#2992](https://github.com/openOMSI-org/openOMSI/pull/2992), @georgefung892).

### Fixes

- **Android shows the last crash:** after a crash, the next start of the app opens the crash dialog with its report and GitHub issue buttons, which the crash used to take down with the launcher ([#3042](https://github.com/openOMSI-org/openOMSI/pull/3042), @isaacsa2).
- **Only hands and forearms in the cab view:** with "Driver's hands in the cab view" on, the driver's arms end at the elbows, and the upper arms no longer stick out into the cockpit ([#2933](https://github.com/openOMSI-org/openOMSI/pull/2933), @NeyvanSantos).
- **Buses no longer wait for ever at some lights:** at a crossing that skips a phase "when nobody asks" at two lights at once, a bus waiting at only one of them now gets its green, as at the modified Winsenburg Hauptbahnhof ([#3009](https://github.com/openOMSI-org/openOMSI/pull/3009), @thibautgnt).
- **Big date changes no longer stall the game:** moving the date by months or years while driving rebuilds the timetable once instead of once for every day in between (594 times for one reported jump) ([#2980](https://github.com/openOMSI-org/openOMSI/pull/2980), @isaacsa2).
- **No more grid of squares in night fog:** the headlamps' light in fog or mist is smooth again instead of covered by a fixed checker of small squares, at the same cost ([#3044](https://github.com/openOMSI-org/openOMSI/pull/3044), @thibautgnt).
- **Destination displays keep their text from afar:** the route text no longer disappears when the camera moves a few metres away from the bus, as on the LiAZ 5293.53 and many other buses ([#2975](https://github.com/openOMSI-org/openOMSI/pull/2975), @thibautgnt).
- **Lit gauges show their needles again:** instruments whose artwork lies only in their night texture, like those of the Elec City Town, show their needles and rims in Classic, Vanilla+ and Enhanced ([#2942](https://github.com/openOMSI-org/openOMSI/pull/2942), @dev-Younte).
- **Plugins can close stops for a diversion:** `omsi.duty.set_closed_stops()` closes stops of the duty; the navigator strikes them through, the next-stop card says "Closed" with the way round, and driving past a closed stop no longer counts as missing it ([#2876](https://github.com/openOMSI-org/openOMSI/pull/2876), @Luc-nbr).

### Multiplayer

- **The server list refreshes itself:** in Multiplayer, Servers, the player counts, time and weather update every 15 s while the list is open (and not while the window is hidden). A server shows the name you gave it first, one that cannot be reached shows its address under the error, and the map is shown by its name instead of its raw path ([#3034](https://github.com/openOMSI-org/openOMSI/pull/3034), @isaacsa2).
- **Hosting with many add-ons uses less memory:** the host's file list is built by streaming each file instead of loading several whole files at once, and the hash cache is saved safely every few seconds ([#2981](https://github.com/openOMSI-org/openOMSI/pull/2981), @isaacsa2).
- **Tunnel addresses are recognised by their real host:** `abc.trycloudflare.com:443/ws` or a name in capitals now join as tunnels too, and an address that only carries a tunnel name in its path is no longer mistaken for one ([#2932](https://github.com/openOMSI-org/openOMSI/pull/2932), @thibautgnt).

### Project

- **Brazilian Portuguese:** the cockpit tooltips are translated, 139 control names such as "Controle de temperatura do aquecimento", the outside and second-button variants included ([#2937](https://github.com/openOMSI-org/openOMSI/pull/2937), @NeyvanSantos).
- **Traditional Chinese:** the online play notices are translated: connecting, the player list, joining, leaving and reconnecting, with player names and chat left as written ([#2941](https://github.com/openOMSI-org/openOMSI/pull/2941), @shamough1792).
- **Serbian (Latin):** the newer interface texts and the online session messages are translated, and the wording is polished, with the driving term "retrovizor" for vehicle mirrors ([#2976](https://github.com/openOMSI-org/openOMSI/pull/2976), [#3008](https://github.com/openOMSI-org/openOMSI/pull/3008), [#3048](https://github.com/openOMSI-org/openOMSI/pull/3048), @kizule).
- The README, the website and the user docs describe the game as it is now, with a table of every `server.cfg` key ([#2915](https://github.com/openOMSI-org/openOMSI/pull/2915), @turbo-devv).
- Windows builds retry the NSIS install when the Chocolatey site is briefly down ([#3010](https://github.com/openOMSI-org/openOMSI/pull/3010), @thibautgnt).

### Credits

- **@isaacsa2**, for bringing back the old launcher's server list, mod contents, start bar, METAR and depot file, for the vehicle smoke option, the Android crash dialog, a lighter host and quick date changes.
- **@Luc-nbr**, for closed stops and pause menu lines, two plugin additions that let a plugin run a service around the timetable.
- **@lmonteirotech**, for the compass that turns the city map north up or heading up.
- **@shamough1792**, for the new Shift+Y information bar and the Traditional Chinese online notices.
- **@NeyvanSantos**, for a tidier driver in the cab view and the Brazilian Portuguese cockpit tooltips.
- **@FakeShxp3**, for raindrops you can see and a duty date that stays as chosen.
- **@ThiBot77**, for two performance fixes on articulated buses and very detailed scenery.
- **@dev-Younte**, for a careful, well-tested fix that brings back the needles of lit gauges.
- **@georgefung892**, for the navigator's distance to the next stop in the plugin API.
- **@kizule**, for three rounds of Serbian translations and the careful choice of words in them.
- **@turbo-devv**, for bringing the README, the website and the docs up to date.

## 0.3.38 - 2026-10-18

The launcher gets a **Depot** of your own buses, each keeping its plate, odometer, fuel and wear from one drive to the next. Passengers now run for the bus and pay with banknotes too. In online play, every player sees another player's bus exactly as its driver has it, and a session hosted by a player's game offers the joining players their own buses again. The navigator and city map draw road markings, houses from their models and, with a map package, the relief. Plugins can put a panel in the navigator, build richer cards and take a click directly. Driving into new areas hitches less, the launcher's bus preview turns at up to 60 pictures a second, and tyres only throw spray through puddles.

### New and improved

- **A depot of your own buses:** a new **Depot** page under Play. Add a bus with its livery, a name, a French plate and a fleet number; it keeps its odometer, fuel, dirt, battery and collision damage between sessions, and counts its distance, hours, runs, stops, tickets, takings, fuel and crashes. **Refuel**, **Wash** and **Repair** prepare it for its next drive, and **Drive this bus** takes it to Drive. Each bus lists its last 30 drives, and the wear of the stock scripts (bulbs, door cycles, engine, electrical and drivetrain failures) now carries over as in OMSI's "Maintenance", with a **Repair** per broken part and **Repair all**. Buses can be sorted into depots you name ([#2868](https://github.com/openOMSI-org/openOMSI/pull/2868), @ThiBot77).
- **Passengers run for the bus:** now and then, someone walking towards the stop starts to run as your bus comes, along the pavement, and gets in if you wait. Leave without them and they walk back to wait for the next one. A timetable AI bus waits up to 12 s for a runner; your own bus is never held. `OMSI_RUNNER_CHANCE` sets how often (5 % by default, 0 turns it off) ([#2073](https://github.com/openOMSI-org/openOMSI/pull/2073), @JDCustard).
- **Passengers pay with banknotes too:** the money handed over now uses the coins and the banknotes of the map's currency, so on maps that define banknotes the amount tendered and the change differ from before. Lua plugins can set how much is handed over with `payment` in `omsi.people.set_ticket()`, without changing the price ([#2952](https://github.com/openOMSI-org/openOMSI/pull/2952), @insus100).
- **No random pedestrians on footpaths with zero density,** as the map asks, such as the traffic island near Duba on map 400. People already walking can still use those paths ([#2968](https://github.com/openOMSI-org/openOMSI/pull/2968), @VlastikYoutubeKo).
- **Road markings and real houses:** the navigator and the city map draw lane lines, edge and stop lines, crossings and arrows, read from the paint of the road textures themselves, so a map maker's own textures work too. Houses are drawn from their own models (towers, ridged roofs, wings), canopies and bridges as roofs over an open street, and many buildings that were missing now show. The dark rim of a road is a kerb wide and no longer eats the outer lanes, and bends of wide roads are smooth. The city map now closes with its cross or Escape only ([#2929](https://github.com/openOMSI-org/openOMSI/pull/2929), @turbo-devv).
- **The relief on the city map:** with a map package that has contour lines, a **Terrain** button shows the ground's contour lines with their heights in metres, the land use left out. A package without contour lines looks as before ([#2895](https://github.com/openOMSI-org/openOMSI/pull/2895), @lmonteirotech).
- **A plugin's panel in the navigator:** with `anchor = "navigator"`, a panel sits under the navigator's map, where the stop list is, and moves, scales and hides (Shift+N) with it. `omsi.ui.dock()` tells its size ([#2880](https://github.com/openOMSI-org/openOMSI/pull/2880), @Luc-nbr).
- **Richer panels:** tinted boxes, stop lists with planned and expected times, bars of several coloured parts, bold badges, and tables with right-aligned columns and coloured cells ([#2881](https://github.com/openOMSI-org/openOMSI/pull/2881), @Luc-nbr).
- **A card that takes a click directly:** with `grab = true`, a small card's buttons work on the first click, as the game's own cards do, without giving the panels the mouse first. `omsi.ui.version` is now 5 ([#2883](https://github.com/openOMSI-org/openOMSI/pull/2883), @Luc-nbr).
- **A little less work per frame while driving,** about 0.4 to 0.6 ms on a dense part of Grand Paris-Moulon at 1440p ([#2958](https://github.com/openOMSI-org/openOMSI/pull/2958), @ThiBot77).

### Fixes

- **The bus preview turns smoothly:** in the bus picker, the depot and the Drive page, the 3D preview now follows the mouse at up to 60 pictures a second instead of 30, with each picture made in about a third of the time ([#2873](https://github.com/openOMSI-org/openOMSI/pull/2873), @ThiBot77).
- **Fewer hitches driving into new areas:** the painted ground textures of new tiles are decoded in the background again instead of on the frame (on a 3 minute drive on Grand Paris-Moulon, 38 textures decoded on the frame before, 2 after) ([#2957](https://github.com/openOMSI-org/openOMSI/pull/2957), @ThiBot77).
- **No more quarter-second freeze north of Massy** on Grand Paris-Moulon: a tile with a very long AI path now links its lanes in 3 to 4 ms instead of about 170 ms ([#2956](https://github.com/openOMSI-org/openOMSI/pull/2956), @ThiBot77).
- **Tyre spray only through puddles:** wheels throw as much spray as the puddle under them, and stop as the puddles dry after the rain, instead of spraying on every wet road. The haze on wet asphalt is gone ([#2931](https://github.com/openOMSI-org/openOMSI/pull/2931), @turbo-devv).
- **Binary `.x` models:** a new reader for binary `.x` files and their vertex declarations, written from the DirectX file format documentation. Broken files give an error instead of a crash, and all stock `.x` files load exactly as before ([#2886](https://github.com/openOMSI-org/openOMSI/pull/2886), @turbo-devv).

### Multiplayer

- **Other players' buses drawn as their drivers have them:** a bus you lack is fetched from the server by its content, every file checked, and mounted on its own. A neutral box of its size, with the download progress, stands in while it arrives. A bus that cannot be had is drawn with another bus, always labelled as a stand-in under the player's name, and the real one replaces it as soon as it comes. Buses arriving no longer freeze the game (on a heavy map, the worst frame went from 4.8 s to 0.27 s) ([#2667](https://github.com/openOMSI-org/openOMSI/pull/2667), @fsalexandre).
- **A session hosted by a player offers your own buses again:** joining players were limited to the buses the host had installed; now they can choose their own, which the host takes as a dedicated server does ([#2938](https://github.com/openOMSI-org/openOMSI/pull/2938), @fsalexandre).
- **Tools for server owners:** a Docker image for the dedicated server, `--pack-map <map> --pack-to <folder or .zip>` to pack a map with everything it uses for a server that lacks it, load-test bots and an end-to-end check script ([#2668](https://github.com/openOMSI-org/openOMSI/pull/2668), @fsalexandre).
- **The chat hint names the right key:** "press <key> to chat" shows the key chat is really bound to (`/`, or the key it moved to when OMSI's keyboard.cfg gives `/` to the gearbox), and the launcher's Running games page points to Controls instead of saying V ([#2916](https://github.com/openOMSI-org/openOMSI/pull/2916), @turbo-devv).

### Project

- **Russian:** 680 texts that were still in English are translated ([#2905](https://github.com/openOMSI-org/openOMSI/pull/2905), @dcmanmwem).
- **Traditional Chinese:** the remaining launcher and game texts are translated, including the navigator's units, the destination form, the developer tools and the camera notices ([#2961](https://github.com/openOMSI-org/openOMSI/pull/2961), @shamough1792).
- **The macOS disk images are packaged reliably again**: the image is detached by its device and its conversion retried when the disk is still busy ([#2972](https://github.com/openOMSI-org/openOMSI/pull/2972), [#2974](https://github.com/openOMSI-org/openOMSI/pull/2974), @isaacsa2).

### Credits

- **@ThiBot77**, for the Depot, a smoother bus preview and three fixes that take hitches and freezes out of Grand Paris-Moulon.
- **@fsalexandre**, for finishing the players' buses: drawn as their drivers have them, own buses in player-hosted sessions, and the server tools.
- **@JDCustard**, for passengers running for the bus, with a running animation of their own.
- **@Luc-nbr**, for three plugin API additions that let plugins look and feel like the game's own panels.
- **@turbo-devv**, for road markings and real houses on the navigator, puddle-only spray, a new binary `.x` reader and the right chat key.
- **@lmonteirotech**, for the relief on the city map.
- **@insus100**, for banknotes and the `payment` option of the ticket API.
- **@VlastikYoutubeKo**, for a careful fix, with before and after shots, of pedestrians on zero-density paths.
- **@dcmanmwem** and **@shamough1792**, for the Russian and Traditional Chinese translations.
- **@isaacsa2**, for unblocking the macOS packaging of this beta and for reviewing much of it.

## 0.3.37 - 2026-10-18

<img width="4000" height="2250" alt="What's new in openOMSI 0.3.37" src="https://github.com/user-attachments/assets/7eb6b83c-4803-4529-a121-328eb19f88ed" />

Changes since 0.3.33, the previous stable release:

### New and improved

- **A new navigator, in the style of Apple Maps.** Every map is drawn from the map itself, no map package needed: green land, fields, water, woods and trees, rail and tram lines, paved roads and yards, and the houses standing in 3D with lit walls and soft shadows. The navigator is a rounded map with floating cards: the next turn and its street, the time and temperatures, your speed with the limit sign (red when too fast), and the next stop with its line, distance, time and delay. It can be a widget you drag anywhere and resize.
- **The Drive page on one screen.** The map, the line and tour, the roadbook, the bus and the start together, in the layout the community chose: **Big Map** (the default), with **Control Room**, **Classic+** and **Classic** in Settings → Launcher layout. The menu on the left is in parts (Play, Content, Options, Help) and folds down to its icons.
- **Installers.** A Windows installer (per user, no admin rights needed) and a macOS disk image to drag onto Applications, beside the zip files.
- **Wheels work like in OMSI 2.** One button can do several actions at once (all doors, a light and its switch); a new **Continuous** switch repeats an action while the button is held; one click makes the hat look around; **Keep driving in the background** keeps the wheel, pedals and force feedback working when another window has the focus; and **Radio: on / off** plays the internet radio in a bus without a radio of its own.
- **Rain made anew, and snow that builds up.** A quarter of a million fine drops worked out on the GPU, splashing where they land and staying put when the camera turns. Rain and snow stop at roofs, bridges and shelters. Snow settles on buses, cars, houses and verges, and thaws above zero and in the rain.
- **Driving off the road soils the bus**: mud in the wet in a few minutes, dust in the dry more slowly.
- Tomorrow.io weather that follows the map's area while you drive (with your own Tomorrow.io key and the map's `openomsi_weather.cfg`).
- An option to start new buses with a full fuel tank.
- A Gameplay option to keep passengers from reacting to bad driving (on by default).
- The timetable window shows the expected arrival time.
- The fleet number can be typed in, and picking a bus chooses a random fleet number.
- A map's HOF file is shared with every bus automatically.
- **Line timetables in the lines panel**: departures by day and direction, and the time at each stop on the map. Read only when **Timetable** is pressed.
- **A map's package can name its places**: icons, names, regions, and the street and municipality under the bus. On maps without street name signs, an add-on's own name plates and the stops name the streets.
- The city map tilts into 3D when zoomed in and lies flat from far (its own 2D/3D button and the **3D buildings on the map** setting).
- **Traffic colours follow how fast traffic flows**: cars waiting at a red light are no jam; slow, heavy and standing traffic show amber, red and dark red.
- The navigator's map moves smoothly, shows only the buses really on the road near you, and zooms in for a turn.
- The sun is back in the sky in Vanilla and Vanilla+, drawn as OMSI 2 draws it.
- A road's painted lines are drawn over the crossings and junction plates laid over it, as in OMSI 2.
- **Textures stay sharp on a graphics card of its own**: their share grows to half of the video memory.
- Puddle reflections have a Low and a High quality.
- The resolution set in the settings now sizes the 3D picture in fullscreen too.
- The Drive page's map moves and zooms smoothly on large maps.
- The settings warn when a choice can cost many frames per second.
- **Current time** and **Current date** buttons beside the start time and date.
- The bus preview follows your graphics settings and stands in a photo studio, and it fills its side of the bus picker for every bus.
- **Content test** in Settings replaces the Model test page: any bus, map, mod or file is loaded the way the game loads it and every warning is listed. Its bus list can be narrowed to one maker. From the command line: `openomsi --check <file or folder> [--deep]`.
- Several `[visible]` conditions of a mesh combine, with `[visible_range]`, `!` and `||` (see docs/MODDING.md).
- Script init blocks get a larger instruction budget than a frame.
- Mip maps, lamp effect bitmaps, trees past their tile, coupled train cars' lamps, traffic light requests, level crossings, timetable buses at their last stop and passengers' stop rules all behave closer to OMSI 2.
- Updates and downloaded packs unpack more safely.

### Fixes

- The simulation runs on its own thread by default, and AI cars no longer shake back and forth when it does.
- A way back to the route is the quickest to the route's end, counted in time with turns, and no longer leads back to a part already driven.
- In Enhanced+, a bus's lit switches and gauges keep their texture at night instead of glowing as flat white patches.
- Parked cars no longer look like see-through ghosts in Enhanced and Enhanced+.
- Car and bus windows no longer wear a grey sheen in Enhanced on overcast and foggy days.
- From the cab, other vehicles' lamps no longer light the inside of your bus or the road ahead through its body.
- Some street lamps no longer draw a dark disc on the ground round their pole.
- Grass and earth no longer shine like wet asphalt in the rain.
- Tinted bus windows that share their texture with the body are blended instead of drawn black.
- A bus's invisible cover hides its sun blind again instead of showing a light blue patch.
- It rains again on computers that fell back to the basic pipelines after a graphics failure.
- The launcher's route map opens fast and follows the mouse smoothly on big maps (Berlin-Spandau's preview in about a second instead of 88 s), with the map's own green around it.
- The route map draws the game's own city map, with a light and a dark look after Apple Maps, and flies to a new trip instead of jumping.
- The line list no longer covers the tour list on maps with more than eight lines, and a long bus name no longer makes the drive panel scroll sideways.
- **Scripts**: `{else}` chains and `$SetLength` work as in OMSI 2 (the DBC Chicago Citybus farebox and the Kruger++ matrix display are fixed), and the latest `$msg` of your bus's scripts shows as a note.
- OMSI 1 AI lists are read as OMSI 1 writes them (OMSI 1 Velbert has traffic again).

### Performance

- **Less CPU and GPU time per frame**: a shorter ambient occlusion prepass, AI cars that keep the bend speeds ahead, late passes finished on helper threads, lighter culling, and a navigator that draws only what its camera sees.
- **Above 60 fps, the Enhanced sky costs less GPU time.** Night in the city costs less: each place is lit by its strongest lamps, as OMSI 2 did.

### Multiplayer

- **Multiplayer with a code.** Multiplayer has **Join friends** and **Invite friends**: type a code, check the host, map, players, time and weather, pick a bus, then **Join and drive**. Hosting goes to the Drive setup in one click. Each server also gets a protocol choice (Auto, UDP or WebSocket).
- **Dedicated server administration.** An admin and a moderator role, an Administration menu in the game (Esc) and a web panel at `/admin/` with players, search, send away, chat, clock, weather, traffic and passenger density, plus an audit log. New `run-server.cmd` / `run-server.sh` start scripts ask for the OMSI 2 folder and settings the first time.
- **Every bus, tram and train of the timetable is on the maps, gliding along its route.** Other players of a LAN session or server show as named markers.
- LAN players see each other's vehicle variables, texts, script textures, sounds and AI timetable behaviour, with smoother door and model animations. On foot, Ctrl+Alt+S asks a nearby bus to open its door.
- **Players' buses go to the server by their content**: a server that lacks a player's bus gets just the files of that variant and paint, and keeps every version its players send.
- An `admin_password` or `moderator_password` shorter than 12 characters now lets nobody in (the log says so; the start scripts ask for a new one).
- A `server.cfg` without `admin_web` has the web panel off until it says `admin_web = local`.
- **New keys**: `passenger_density`, `accept_uploads`, `uploads_keep_days` and `share_maps` (a new `server.cfg` says 0; one without the key keeps passing the map on). See [docs/SERVER.md](https://github.com/openOMSI-org/openOMSI/blob/main/docs/SERVER.md).

### Project

- Serbian Latin added and polished, French in-game messages, tips and menus, and complete Traditional Chinese hints and controller notices.

## 0.3.36 - 2026-10-17

An **Optimise for my PC** button tunes the graphics to your own PC while you drive, and puts your settings back in one click. Multiplayer gets a guided way to invite and join friends by code, a UDP/WebSocket choice per server, and the first part of players' buses travelling to the server by their content. The city map shows a map package's lines and their timetables, with the time at each stop. The interface speaks Serbian, French players get 131 more texts in their language, and AI cars no longer shake with the simulation on its own thread nor wear a grey sheen on their windows in Enhanced.

### New and improved

- **A Lines panel on the city map:** on a map whose package (`Maps/<Map>/openomsi-realmap`) has a `lines.lin`, the city map gets a **Lines** button. It lists the map's lines in tabs by kind (bus, tram, metro, rail, and bus groups such as BRT, express, feeders) with each line's length, stops, trip time, days and vehicle. A ticked line is drawn on the map in its own colour with its stops, and the vehicles running it get a ring of that colour. Maps with eras show the lines as they were on the session's date ([#2829](https://github.com/openOMSI-org/openOMSI/pull/2829), @lmonteirotech).
- **The content test's bus list can be narrowed to one maker,** as on the Drive page: Settings > Content test > Bus has a maker list, and the buses are sorted by maker, then name. No more scrolling through 100+ buses to find the one you are working on. Asked for on Discord by a modder ([#2844](https://github.com/openOMSI-org/openOMSI/pull/2844), @thibautgnt).

### Fixes

- **AI cars no longer shake back and forth with Simulation on its own thread:** at high frame rates they jumped about 20 cm forward and back every frame; now they glide like your bus. With the setting off (the default), nothing changes ([#2846](https://github.com/openOMSI-org/openOMSI/pull/2846), @thibautgnt).
- **Car and bus windows no longer wear a milky grey sheen in Enhanced** on overcast and foggy days: they mirror the sky softly, as real glass does, and you can see inside. Enhanced+, Vanilla and Vanilla+ do not change ([#2848](https://github.com/openOMSI-org/openOMSI/pull/2848), @thibautgnt).
- **The Drive page's map moves and zooms smoothly on large maps:** releasing the mouse after a drag used to freeze the launcher for up to 1.6 s on Grand Paris-Moulon; now it doesn't, and dragging runs at about 54 frames a second instead of 35 ([#2871](https://github.com/openOMSI-org/openOMSI/pull/2871), @ThiBot77).

### Performance

- **Optimise for my PC:** a new block in the launcher's Settings > Graphics, and a button in the game's Options > Graphics. Pick a frame rate to aim at (Automatic, 60, 90, 120 or 144 fps); during your next drive the game measures for about a minute, finds out whether the graphics card or the processor holds you back, and tries lighter settings one at a time, keeping only those that really bring frames. A PC with room to spare gets a finer picture instead. Your settings are saved first: **Restore my previous settings** brings them back in one click, and the launcher shows the frame rate before and after with every setting changed. On one player's PC, the kind of changes it makes took Grand Paris-Moulon from about 50 to over 140 fps ([#2847](https://github.com/openOMSI-org/openOMSI/pull/2847), @thibautgnt).

### Multiplayer

- **Join friends and Invite friends, step by step:** Multiplayer > Connect by Code now has two clear paths. **Join friends** takes a code, shows the room before you commit (host, map, players, time and weather), then lets you choose a bus; the Drive page says the session is selected but not joined until you press **Join and drive**. A wrong code gets a short explanation, with the technical details folded away. **Invite friends** sets up a hosted duty in one click, straight to the Drive page, with the connection prepared in the background. Servers stays next to it, and a folded **First time here?** explains the steps ([#2865](https://github.com/openOMSI-org/openOMSI/pull/2865), @shamough1792).
- **Choose UDP or WebSocket per server:** each server in the list has a protocol drop-down. On **Auto**, an address without `http://` or `https://` joins over UDP (lower latency, and changing line or tour in game works), and one with it joins over WebSocket. A server's status is also looked for on port 443, where its certificate usually is ([#2338](https://github.com/openOMSI-org/openOMSI/pull/2338), @alecks).
- **Your bus goes to the server by its content:** your game knows your bus by a fingerprint of the files it uses, and sends a server that lacks it only those files (the variant and paint you drive, not the whole pack), in pieces that resume after a break. The server keeps every version its players send, side by side. This is part 2 of 4: other players still draw their own copy of your bus until the next part lands. Server owners get three new `server.cfg` keys: `accept_uploads` (default 1), `uploads_keep_days` (default 30) and `share_maps` (a new `server.cfg` says 0, so players install the map themselves; a file without the key still passes the map on, as before). Older games keep working with it ([#2564](https://github.com/openOMSI-org/openOMSI/pull/2564), @fsalexandre).
- **A line's timetable:** a **Timetable** button on a line's card shows its departures by direction and day (Mon-Fri, Sat, Sun), only for the session's school term or holidays. Click a departure to see each stop of that trip with its time, in the list and next to each stop on the map. The times are the ones the game's AI buses run by, from the map's own timetable files ([#2837](https://github.com/openOMSI-org/openOMSI/pull/2837), @lmonteirotech).

### Project

- **Serbian (Latin)** is a new interface language: choose it in the language settings or set `language=sr` ([#2761](https://github.com/openOMSI-org/openOMSI/pull/2761), @kizule), then polished with clearer wording, proper diacritics and better context-specific labels ([#2898](https://github.com/openOMSI-org/openOMSI/pull/2898), @kizule).
- **131 more texts in French:** the hints over the dashboard (such as the low air pressure warning), the loading screen tips, the controller and head-tracking options, the placing, kneeling and refuelling messages, the developer tools and several error messages ([#2872](https://github.com/openOMSI-org/openOMSI/pull/2872), @ThiBot77).
- Updates, avatar packs and soundscape packs are unpacked more safely (no special permission bits, nothing written through a link, no link reaching outside its folder), and the security check on main is green again with zip 9 ([#2887](https://github.com/openOMSI-org/openOMSI/pull/2887), @thibautgnt).
- More of the game moves out of its main crate, with no change for players: the camera geometry to `omsi-scene`, the bus diagnostics to `omsi-input`, the system code (memory, support package) to `omsi-platform` and starting the launcher to `omsi-guard` ([#2818](https://github.com/openOMSI-org/openOMSI/pull/2818), [#2820](https://github.com/openOMSI-org/openOMSI/pull/2820), @fsalexandre).

### Credits

- **@shamough1792**, for a Multiplayer page that finally walks players through inviting and joining friends.
- **@alecks**, for the per-server UDP/WebSocket choice that brings lower latency to online play.
- **@fsalexandre**, for the big players' buses work, built with a careful security review, and for two more clean refactors.
- **@lmonteirotech**, for the Lines panel and the line timetables on the city map.
- **@kizule**, for bringing the whole interface to Serbian, and polishing it right after.
- **@ThiBot77**, for 131 more texts in French and a smooth Drive map on large maps.

## 0.3.35 - 2026-10-17

A smaller beta that polishes 0.3.34: the navigator's map moves smoothly and zooms in for turns, the launcher's route map opens fast on big maps, parked cars no longer look like ghosts in Enhanced, and the rain is back on computers running the basic pipelines after a graphics failure.

### New and improved

- **Current time and Current date buttons** sit inside the start time and date fields in every layout (Big Map, Control Room, Classic+), not only in the Day & weather dialog: one click sets the duty to now or today. When Settings locks a field to the real time or today's date, its button stays visible but greyed out ([#2840](https://github.com/openOMSI-org/openOMSI/pull/2840), @thibautgnt).
- **Only buses really on the road show near you:** the small map shows the traffic's own buses, the ones you can actually meet; the city map still shows the timetable's buses far away ([#2831](https://github.com/openOMSI-org/openOMSI/pull/2831), @thibautgnt).
- **The map zooms in for a turn** of the route, some 200 m before it and while the bus turns, then back out on the straight, as a phone's navigation app does ([#2831](https://github.com/openOMSI-org/openOMSI/pull/2831), @thibautgnt).

### Fixes

- **The route map opens fast and follows the mouse smoothly on big maps:** on Berlin-Spandau the roads and the route show in about a quarter of a second instead of 1.4 s, the surfaces (houses, paved roads, land) a moment later, and dragging stays smooth. Choosing another tour or trip no longer reloads the whole map, and the land around the map's tiles has the map's own green instead of a grey that looked as if it were still loading ([#2824](https://github.com/openOMSI-org/openOMSI/pull/2824), @thibautgnt).
- **The bus picker's preview fills its side for every bus:** a bus with a short description no longer leaves an empty strip left of its preview ([#2834](https://github.com/openOMSI-org/openOMSI/pull/2834), @thibautgnt).
- **The navigator's map moves smoothly:** it is redrawn every frame instead of at most 30 times a second, unevenly, and the game no longer hitches for 50-100 ms when the map's houses and paving come in or as the bus drives on ([#2831](https://github.com/openOMSI-org/openOMSI/pull/2831), @thibautgnt).
- **Parked cars no longer look like see-through ghosts in Enhanced and Enhanced+:** cars whose models carry poor normals (the Grand Paris-Moulon Peugeot 308, among others) keep their paint colour, as in Vanilla+. Since 0.3.34 it was worse in the rain ([#2832](https://github.com/openOMSI-org/openOMSI/pull/2832), @thibautgnt).
- **It rains again after a graphics failure:** on computers that fell back to the reduced set of render pipelines (`~/.openomsi/gpu-fallback.cfg`), the new rain of 0.3.34 was left out, and an Apple M1 that fell back once had no rain for the rest of the version. Buses and shelters keep the rain out there as well ([#2836](https://github.com/openOMSI-org/openOMSI/pull/2836), @thibautgnt).

### Credits

- **@isaacsa2**, for reviewing every change of this beta, quickly and carefully.

## 0.3.34 - 2026-10-16

The launcher's Drive page puts the map, the duty, the bus and the start on one screen, in the Big Map layout the community chose on Discord, and big maps' route previews show in about a second. The navigator and the city map are rebuilt in the style of Apple Maps, with houses standing in 3D and every timetable bus on the map. Rain is made anew from a quarter of a million fine drops and stops at roofs, snow now builds up while it falls, and the sun is back in the sky in Vanilla and Vanilla+. A wheel button can do several things at once and repeat while held as in OMSI 2, new buses start with a full tank, and LAN games share far more of each bus (displays, sounds, animations), so every player of a session needs this version.

### New and improved

- **The Drive page on one screen:** the map, the line and tour, the roadbook, the bus and the start are all on one page. **Big Map**, the layout chosen by a community vote on Discord, is the default: the route map fills the page, the rest floats over it, and the route is framed in the part the panels leave free. **Control Room**, **Classic+** and **Classic** (the steps, as before) are in Settings > Launcher layout. The page with cards, where the map came far down, is gone; its tour buses, next departures, map note, date calendar and save deletion are on every layout ([#2812](https://github.com/openOMSI-org/openOMSI/pull/2812), @thibautgnt).
- **The route map has a light and a dark look after Apple Maps,** following the launcher's theme ([#2812](https://github.com/openOMSI-org/openOMSI/pull/2812), @thibautgnt).
- **Each tour says how long its trips take** ("32 min per trip") ([#2812](https://github.com/openOMSI-org/openOMSI/pull/2812), @thibautgnt).
- **The bus preview stands in a photo studio,** on a lit podium, and is drawn with the player's own graphics settings (mode, anti-aliasing, render scale, textures), taking over @isaacsa2's work from [#2732](https://github.com/openOMSI-org/openOMSI/pull/2732). Moving previews in the launcher are capped at 30 FPS and pause while the window is hidden; phones keep the plain preview ([#2798](https://github.com/openOMSI-org/openOMSI/pull/2798), @thibautgnt).
- **Any fleet number can be typed,** in a field next to the list, and shows on the bus and its plate ([#2802](https://github.com/openOMSI-org/openOMSI/pull/2802), @trolley813).
- **Content test in Settings,** in place of the Model test page: a bus with the parts coupled behind it, a whole map, an installed mod, or any file or folder (a scenery object, a spline, a human, a weather, a mesh, a mod not installed yet as a folder or zip) is loaded the way the game loads it, and every warning of its files is listed. From the command line: `openomsi --check <file or folder> [--deep]`. Desktop only ([#2768](https://github.com/openOMSI-org/openOMSI/pull/2768), @turbo-devv).
- **The navigator and the city map are drawn in the style of Apple Maps, from the map itself,** with no map package needed: green land, fields, water, woods and single trees, rail and tram tracks, and every paved surface as it lies, so a car park is one grey yard. On a map's first visit its layers are read off its tiles (some seconds on a big map), then kept in a cache ([#2782](https://github.com/openOMSI-org/openOMSI/pull/2782), @turbo-devv).
- **Houses stand in 3D** with lit walls and soft shadows: always on the navigator, and on the city map when zoomed in, where it tilts into 3D (the "3D buildings on the map" setting and the city map's 2D/3D button). The route shows faintly through a house that hides it ([#2782](https://github.com/openOMSI-org/openOMSI/pull/2782), @turbo-devv).
- **The navigator is a rounded map with floating cards** (the next turn and its street, the time and temperatures, the speed with the limit sign, the next stop with its delay), and a widget you can drag anywhere and resize from its bottom right corner (the "Navigator as a widget" setting) ([#2782](https://github.com/openOMSI-org/openOMSI/pull/2782), @turbo-devv).
- **A map's package can name its places:** icons (terminals, stations, stops), names of districts and towns, municipalities as faint watermarks on the city map, and a row under the next stop with the municipality, the street and its speed limit. A package without these files looks as before ([#2763](https://github.com/openOMSI-org/openOMSI/pull/2763), @lmonteirotech).
- **A map's depot (HOF) file is shared with every bus automatically:** when an installed map needs a depot file found in its own folder or in any bus folder, the launcher copies it once into the shared `HOFs/` folder, so stock buses can use it too. The Mods page warns about a depot file nothing installed provides, and its **Add HOF files to buses** button adds chosen `.hof` files for every bus ([#2082](https://github.com/openOMSI-org/openOMSI/pull/2082), @dxox57).
- **Rain and snow stop at roofs:** no more rain or snow inside buses and cars, under bridges, shelters and eaves, also from the outside view ([#2768](https://github.com/openOMSI-org/openOMSI/pull/2768), @turbo-devv).
- **Snow builds up while it falls,** on roofs of buses, cars and houses, the verges and the ground (white in about a quarter of an hour of heavy snow), and thaws above zero and in the rain; roads stay clear unless the weather lays snow on them ([#2768](https://github.com/openOMSI-org/openOMSI/pull/2768), @turbo-devv).
- **Driving off the road soils the bus:** earth, grass or gravel throw dirt onto the body, mud in the wet within minutes, dust in the dry more slowly ([#2768](https://github.com/openOMSI-org/openOMSI/pull/2768), @turbo-devv).
- **Live weather from Tomorrow.io, opt-in:** on a map that ships an `openomsi_weather.cfg` (made with the community editor), and with your own Tomorrow.io key in `~/.openomsi/tomorrow-api-key.txt`, choose **Tomorrow.io** in the day's weather settings and the weather follows the nearest configured area while you drive. Only the area's coordinates and your key are sent, never your position; without a key or offline, the current weather stays ([#1960](https://github.com/openOMSI-org/openOMSI/pull/1960), @VlastikYoutubeKo).
- **One wheel button, several actions:** a wheel or button-box button can close every door, or switch a light together with its switch, in one press. In Controls > Game controllers > Buttons, assigning a button that already does something adds the action instead of replacing it; the *By button* tab gets a **+** and an **x** per extra action ([#2791](https://github.com/openOMSI-org/openOMSI/pull/2791), @thibautgnt).
- **Continuous buttons as in OMSI 2:** a new switch per button, next to *Latching*; held down, its action repeats until released (a retarder lever goes through its steps, *Gear up* keeps shifting). `gamectrler.cfg` files from OMSI 2 that set this flag now repeat as they did there ([#2791](https://github.com/openOMSI-org/openOMSI/pull/2791), @thibautgnt).
- **The hat switch** stays listed in the *By button* tab, and one click (**Look around with it**) makes its four directions look around ([#2791](https://github.com/openOMSI-org/openOMSI/pull/2791), @thibautgnt).
- **Keep driving in the background** (off by default; Settings > Wheel and pedals, and the in-game controller menu): with two screens, the wheel, pedals, buttons and force feedback keep working while another window has the focus ([#2791](https://github.com/openOMSI-org/openOMSI/pull/2791), @thibautgnt).
- **Radio: on / off,** a new action for a key or button that switches on the internet radio in a bus whose scripts have no radio of their own ([#2791](https://github.com/openOMSI-org/openOMSI/pull/2791), @thibautgnt).
- **New buses start with a full tank** (on by default; "Start with a full tank" in Options > Gameplay and the launcher's Settings > Gameplay), filled through the bus's own fuel script; a saved situation keeps its own fuel ([#2805](https://github.com/openOMSI-org/openOMSI/pull/2805), @isaacsa2).
- **The timetable window shows an Expected column:** each coming stop's time with your current delay added, when the window has room for it ([#2797](https://github.com/openOMSI-org/openOMSI/pull/2797), @rawin187).
- **Save slots:** the pause menu's "Save to a slot..." saves to a new slot or over one of the map's slots, and the launcher's Continue card deletes saves ([#2768](https://github.com/openOMSI-org/openOMSI/pull/2768), @turbo-devv).
- **Touch pedals** get a rest zone at their foot where a finger presses nothing, a softer start of their travel, and an adjustable length ([#2768](https://github.com/openOMSI-org/openOMSI/pull/2768), @turbo-devv).
- **The steering wheel comes back after a turn the way a bus's does:** gently at town speed and damped, without swinging past the centre ([#2768](https://github.com/openOMSI-org/openOMSI/pull/2768), @turbo-devv).
- **Simulation on its own thread, opt-in:** the AI traffic, the people and the scenery scripts can run beside the drawing on another processor core (the AI cars one frame later). Off by default; switch it on in Display > "Simulation on its own thread" (from the next start) or with `OMSI_SIM_THREAD=1` ([#2807](https://github.com/openOMSI-org/openOMSI/pull/2807), @isaacsa2).
- **The sun is in the sky in Vanilla and Vanilla+,** drawn as OMSI 2 draws it, under the haze and the clouds; Enhanced is unchanged ([#2778](https://github.com/openOMSI-org/openOMSI/pull/2778), @thibautgnt).
- **A wet bus mirrors the street** in Enhanced ([#2768](https://github.com/openOMSI-org/openOMSI/pull/2768), @turbo-devv).
- **Passengers can be kept from reacting to bad driving** ("Passengers react to bad driving", Options > Gameplay and the launcher's Settings > Gameplay, on by default). Switched off, nobody complains about hard braking or fast bends, and nobody gets off early because of it: for modded buses whose physics upset passengers at the slightest bump ([#2810](https://github.com/openOMSI-org/openOMSI/pull/2810), @thibautgnt).
- **`[visible]` conditions combine:** `[visible_range] var min max`, a `!` after the variable turns a condition round, and a `||` line between blocks gives either-or. Meshes with only plain `[visible]` blocks keep OMSI 2's rule (docs/MODDING.md) ([#2768](https://github.com/openOMSI-org/openOMSI/pull/2768), @turbo-devv).
- **The navigator is a widget**: drag it anywhere and size it at its bottom right corner (the `Navigator as a widget` setting; the size is kept, and in the launcher's settings).
- **The launcher's route preview draws the same picture**: ground, water, rail, paved surfaces and houses, the route in blue, the stops on white cards.
- **Rain and snow stop at roofs**: a map of what stands over the ground round the camera, seen from above, keeps them out of every bus and car, under bridges, shelters and eaves - from the outside view as well, where it rained and snowed in the saloon.
- **Enhanced**: a wet bus mirrors the street - the water on its paint is as smooth as glass.
- **Textures stay sharp on a graphics card of its own**: their share grows to half of the video memory (about 3.7 GB on an 8 GB card on Automatic), where it stayed at an eighth of the computer's memory (2 GB with 16 GB) whatever the setting, and the far textures lost their finest levels even at the highest settings. The video memory still bounds everything on the card; integrated graphics and the safer settings after a lost device keep the old share.

### Fixes

- **Big maps preview in about a second instead of over a minute:** Berlin-Spandau with its paved surfaces took 88 s before; when zoomed in, only the part in view is drawn ([#2812](https://github.com/openOMSI-org/openOMSI/pull/2812), @thibautgnt).
- **The bus preview is always lit as at midday under a clear sky,** whatever the duty's start time and weather: a night duty no longer hides the bus, and changing the time or the weather no longer redraws it ([#2759](https://github.com/openOMSI-org/openOMSI/pull/2759), @ThiBot77).
- **Picking a bus gives it a random fleet number** instead of always the first of the list ([#2190](https://github.com/openOMSI-org/openOMSI/pull/2190), @Burst99325).
- **Traffic colours follow how fast the traffic flows:** busy streets that move are no longer amber, and two cars at a red light or a bus at its stop are no jam. **A way back to the route** now leads to the end of the route by the quickest way, counted in time with turns, instead of back to a part already driven ([#2782](https://github.com/openOMSI-org/openOMSI/pull/2782), @turbo-devv).
- **Rain, made anew:** about a quarter of a million drops worked out on the graphics card each frame instead of a few thousand sprites, so a downpour is a veil of short, fine, faint lines. Drop sizes and fall speeds follow how heavy the rain is, small splashes land on the road, the kerb and roofs, and the rain stays put in the world: moving the camera round the bus no longer swings it ([#2768](https://github.com/openOMSI-org/openOMSI/pull/2768), @turbo-devv).
- **Screenshots on Android show up in the phone's gallery at once,** and the pause menu has its Screenshot line again, next to Photo mode, on every platform ([#2789](https://github.com/openOMSI-org/openOMSI/pull/2789), @thibautgnt).
- **More accurate GPU timings** for the passes and the mirrors, and dedicated servers no longer work out spray for every AI car in the rain ([#2807](https://github.com/openOMSI-org/openOMSI/pull/2807), @isaacsa2).
- **Invisible covers hide the sun blind again:** no more light blue patch over the windscreen (Karosa B951/B961, SUNWIN V and others), broken since 0.3.0. **Textures stay sharp on a graphics card with its own memory:** their share is now half of the video memory (about 3.7 GB on an 8 GB card on Automatic) instead of 2 GB with 16 GB of RAM ([#2787](https://github.com/openOMSI-org/openOMSI/pull/2787), @thibautgnt).
- **Grass and earth no longer shine like wet asphalt in the rain** (the stock grass strips glowed in every lamp at night) ([#2768](https://github.com/openOMSI-org/openOMSI/pull/2768), @turbo-devv).
- **Tinted bus windows are no longer drawn black** when they share their texture with the body (Ychebniy LiAZ 5292.65, NefAZ 5299 2024) ([#2768](https://github.com/openOMSI-org/openOMSI/pull/2768), @turbo-devv).
- **A road's painted lines stay on top of crossings and junction plates,** as in OMSI 2, instead of vanishing with the camera's height and distance ([#2768](https://github.com/openOMSI-org/openOMSI/pull/2768), @turbo-devv).
- **Night in the city costs less:** the classic picture lights a place with its eight strongest lamps, as OMSI 2 does, and headlights at a junction no longer push the street lamps out ([#2768](https://github.com/openOMSI-org/openOMSI/pull/2768), @turbo-devv).
- **Buses whose scripts do a long lookup when they start work again:** a script's start-up gets a larger budget than a frame. On Grand Paris-Moulon, the Citaro C2's destination sign, IBIS, screens and heating work again ([#2773](https://github.com/openOMSI-org/openOMSI/pull/2773), @ThiBot77).
- A bus's invisible cover hides its sun blind again instead of showing a light blue patch over the windscreen and the street behind it, in Enhanced, and in Vanilla+ with ambient occlusion and MSAA off: since 0.3.0 the parts listed after the cover still went into the depth the picture is drawn against, also when the cover is a mesh of its own. The ambient occlusion still sees them ([#2678](https://github.com/openOMSI-org/openOMSI/issues/2678), [#2178](https://github.com/openOMSI-org/openOMSI/issues/2178)).

### Performance

- **A Puddles setting** (Graphics, High by default): Low traces the wet roads' reflections only near the bus, much cheaper; Off draws none, while paint and glass keep theirs ([#2807](https://github.com/openOMSI-org/openOMSI/pull/2807), @isaacsa2).

### Multiplayer

- **The menu on the left is in parts:** Play (Drive, Multiplayer, Running games), Content (Mods, Timetable), Options (Settings, Controls, Setup) and Help (Tutorials); the driver at its bottom opens the profile. The arrow at its bottom folds it to its icons, and it stays folded at the next start ([#2812](https://github.com/openOMSI-org/openOMSI/pull/2812), @thibautgnt).
- **Every bus, tram and train of the timetable is on the maps** where the timetable has it, gliding along its route, and the other players of a LAN session or server show with their names. Street names show on maps without the stock street name signs, from an add-on's own name plates and stops named after their street ([#2782](https://github.com/openOMSI-org/openOMSI/pull/2782), @turbo-devv).
- **LAN games share far more of each bus:** script variables and texts, script textures (displays), sounds, and AI buses' timetable behaviour arrive on every client, and doors and other animations move smoothly. While walking, Ctrl+Alt+S (set in Controls) asks a nearby bus for a door. The LAN protocol is now 14: every player of a session needs this version ([#2190](https://github.com/openOMSI-org/openOMSI/pull/2190), @Burst99325).

### Project

- **Traditional Chinese** for the remaining loading tips, pause-menu descriptions, controller notices and the head-tracking description ([#2771](https://github.com/openOMSI-org/openOMSI/pull/2771), @shamough1792).
- **1046 translations completed** for the launcher's Drive page and new texts, in all 27 languages ([#2812](https://github.com/openOMSI-org/openOMSI/pull/2812), @thibautgnt).
- **No update in your launcher?** The home, Download and Betas pages now point to the release on GitHub, the beta on the Betas page ([#2811](https://github.com/openOMSI-org/openOMSI/pull/2811), @thibautgnt).
- More of the game moves out of its main crate, with no change for players: the navigator and the machine translation into crates of their own, the journey log and trip summary, tile streaming, the paper timetable, the content test, the glTF export and six other modules into the crates they belong to ([#2800](https://github.com/openOMSI-org/openOMSI/pull/2800), [#2795](https://github.com/openOMSI-org/openOMSI/pull/2795), [#2801](https://github.com/openOMSI-org/openOMSI/pull/2801), [#2796](https://github.com/openOMSI-org/openOMSI/pull/2796), [#2794](https://github.com/openOMSI-org/openOMSI/pull/2794), @fsalexandre).
- The desktop launcher builds again after [#1960](https://github.com/openOMSI-org/openOMSI/pull/1960), which had stopped every build and beta ([#2803](https://github.com/openOMSI-org/openOMSI/pull/2803), @fsalexandre).
- CodeQL leaves the test files out of its analysis ([#2804](https://github.com/openOMSI-org/openOMSI/pull/2804), @isaacsa2).
- Every pull request that changes the launcher's Drive page is checked in every layout, both themes and many window sizes, so no option can go missing ([#2812](https://github.com/openOMSI-org/openOMSI/pull/2812), @thibautgnt).
- A new test covers buses behind a bus standing long at a shared stop ([#2768](https://github.com/openOMSI-org/openOMSI/pull/2768), @turbo-devv).

### Credits

- **@turbo-devv**, for the Apple-Maps-style navigator with 3D houses, the new rain and snow, the Content test and a long list of fixes: a huge beta.
- **@Burst99325**, for the big LAN sync work that makes displays, sounds and doors match on every screen.
- **@isaacsa2**, for the simulation thread, the Puddles setting, the full tank and CodeQL, and for the bus preview work in [#2732](https://github.com/openOMSI-org/openOMSI/pull/2732).
- **@fsalexandre**, for five careful refactors and for the quick hotfix that got every build going again.
- **@VlastikYoutubeKo**, for live Tomorrow.io weather, built with care for privacy.
- **@dxox57**, for ending the copying of HOF files into every bus folder by hand.
- **@lmonteirotech**, for giving map packages their place names, icons and municipalities.
- **@ThiBot77**, for the midday bus preview and for bringing the Citaro C2 of Grand Paris-Moulon back to life.
- **@trolley813**, for letting players type any fleet number.
- **@rawin187**, for the Expected column in the timetable window: welcome, and thank you for your first merged contribution.
- **@shamough1792**, for completing the Traditional Chinese.

## 0.3.33 - 2026-10-16

Heavy rain no longer holds the frame in the cab, the Video memory setting now bounds everything the game keeps on the graphics card, and a card that runs out of memory no longer loses the whole picture. Dashboard instruments and sign plates stop flickering against the surface they lie on, headlight beams leave from the headlamps, reflections get a strength slider, and the launcher speaks French and offers two smaller sizes.

### New and improved

- **Heavy rain costs less in the cab:** the drops on the windows are painted on helper threads and only the rows that changed are sent to the card. On a Radeon 680M, in a downpour, the panes went from 10-12 ms of every frame to 2-3 ms ([#2766](https://github.com/openOMSI-org/openOMSI/pull/2766), @thibautgnt).
- **Video memory bounds everything on the card:** "Texture memory" becomes "Video memory" and now covers all textures, meshes, render targets and the driver's own memory, as the driver counts them on DirectX 12, Vulkan and Metal. Before, the game could use gigabytes more than set, and 8 GB cards paged to system memory. Automatic is nine tenths of the card; 8 and 12 GB can be chosen ([#2669](https://github.com/openOMSI-org/openOMSI/pull/2669), @turbo-devv), and 16 GB for 20-24 GB cards ([#2766](https://github.com/openOMSI-org/openOMSI/pull/2766)).
- **Enhanced+ draws 8x MSAA at 4x:** the traced passes cost half as much and their targets take half the memory, for little visible difference; `OMSI_FULL_GPU=1` keeps 8x ([#2766](https://github.com/openOMSI-org/openOMSI/pull/2766)).
- **Headlight beams start at the bus's own headlamps,** not under the bumper or at the fog lamps (Van Hool AG280, MB O305, MAN SD200, Enviro400 MMC...) ([#2756](https://github.com/openOMSI-org/openOMSI/pull/2756), @thibautgnt).
- **Reflection maps get a 0-100 % strength slider** (paint, chrome, glass) in the game, the launcher and the Apple app; changes show at once ([#2764](https://github.com/openOMSI-org/openOMSI/pull/2764)).
- **Two smaller sizes, Very small and Smallest,** so more of each page fits on big screens ([#2758](https://github.com/openOMSI-org/openOMSI/pull/2758), @ThiBot77).
- **The video memory setting is kept:** "Texture memory" is now "Video memory", and it bounds everything the game holds on the graphics card - textures, meshes, render targets and shadow maps, the driver's own - not the textures alone. The game took gigabytes more than it was set to, and an 8 GB card could page to the system's memory and drop to a fraction of its frame rate. Where the driver says how much it really hands out (DirectX 12, Vulkan, Metal), the game stays under that as well, whatever the setting; when it is over, the farthest textures give up their finest levels first, and come back when there is room again. "Automatic" is nine tenths of the card's own memory. Choices of 8 and 12 GB were added, 500 MB was dropped. A texture memory set before (and OMSI's `texmemlimit`) was a texture budget and is not carried over: the video memory starts on Automatic. After a lost graphics device the safer settings take two thirds of what the card holds, not a fixed 800 MB.
- A video memory of 16 GB can be chosen, for cards with more than 16 GB of their own.

### Fixes

- Render targets of a size not drawn for 5 s are freed, so a large photo no longer leaves them on the card ([#2766](https://github.com/openOMSI-org/openOMSI/pull/2766)).
- **A card out of memory no longer loses the whole picture:** meshes go into smaller buffers, or only those that cannot be made are left out, instead of the main pass, shadows and picture being thrown away every frame ([#2764](https://github.com/openOMSI-org/openOMSI/pull/2764), @thibautgnt).
- **No more flicker between surfaces lying on each other:** instruments printed on a dashboard, a plate on a pole or a timetable on a stop show steadily, the later one on top as in OMSI 2 ([#2603](https://github.com/openOMSI-org/openOMSI/pull/2603), @turbo-devv).

### Project

- **French** for the whole desktop launcher ([#2311](https://github.com/openOMSI-org/openOMSI/pull/2311), @ThiBot77).

### Credits

- **@turbo-devv**, for making the Video memory setting mean what it says and for ending the z-fighting on dashboards and signs.
- **@ThiBot77**, for the French launcher and the smaller sizes: two contributions in one beta.

## 0.3.32 - 2026-10-15

The departure displays at the stops show their buses again, high-quality passengers no longer fill 6-8 GB graphics cards after a few minutes, and a street lamp hidden behind a house no longer darkens the whole picture at night. Timetable buses appear when their tour runs as in OMSI 2, articulated buses steer their rear axle, VR headsets that stayed black now show the game, and a force-feedback wheel no longer pulls itself to the centre when the bus stands still. Plus complete Catalan and Spanish, new German texts, and plugins that can choose the passengers' tickets.

### New and improved

- **VR:** headsets that stayed black (PICO Neo3 Link with SteamVR) now show the game, even without input focus ([#2589](https://github.com/openOMSI-org/openOMSI/pull/2589), @Klyntch).
- The texture cache keeps its new entries on Linux ([#2704](https://github.com/openOMSI-org/openOMSI/pull/2704), @thibautgnt).
- From the 14 October triage, with Omsi.exe's own behaviour as the reference ([#2657](https://github.com/openOMSI-org/openOMSI/pull/2657), @turbo-devv):
- **Articulated buses:** the steered rear axle steers and reaches the scripts, and the rear section's driver camera comes after the front's cab views.
- The driver's eye moves with the seat's suspension; a vehicle key in keyboard.cfg only overrides a game shortcut where the bus uses it.
- **Plugins can see and choose the ticket each passenger buys** (reduced fares, passes...), with two Lua examples; without a plugin nothing changes ([#2708](https://github.com/openOMSI-org/openOMSI/pull/2708), @Spindlerkoo).
- The launcher is a little less zoomed in, and grows more gently on big screens ([#2733](https://github.com/openOMSI-org/openOMSI/pull/2733), @thibautgnt).
- Chrono scenarios switch the money system, the AI lists and the parking lists as in OMSI 2: a scenario's full AI list replaces the map's, an update replaces only the groups it names, and parking lists come from the newest scenario that has them ([#2315](https://github.com/openOMSI-org/openOMSI/issues/2315)).
- The driver's eye moves with the seat's suspension, one movement as in OMSI 2; the camera had a spring of its own that dipped the other way under braking ([#2550](https://github.com/openOMSI-org/openOMSI/issues/2550)).
- GetTicketName gives the ticket's display string, what ticket printers print ([#2483](https://github.com/openOMSI-org/openOMSI/issues/2483)).
- **Controls**
- DirectInput axes are scaled from the range the driver really gives them, and an axis with its own range is logged (Fanatec pedals reading 50-100 %, [#521](https://github.com/openOMSI-org/openOMSI/issues/521)).
- **Graphics and sound**
- The paper timetable keeps Courier New's spacing whatever font draws it, and Linux finds Courier-compatible fonts (Liberation Mono, Cousine, Nimbus Mono) ([#1878](https://github.com/openOMSI-org/openOMSI/issues/1878)).
- Shader tests cover OpenGL ES 3.1/3.2 per entry point and catch the nested constant-array lookups the Adreno 710 refused ([#2009](https://github.com/openOMSI-org/openOMSI/issues/2009)).

### Fixes

- **Departure displays at the stops show their buses again:** since 0.2.39, boards such as HafenCity Hamburg's HVV displays showed the stop name but no lines, destinations or times, and other scripted scenery near the camera could lose its texts, sounds and animations ([#2737](https://github.com/openOMSI-org/openOMSI/pull/2737), @thibautgnt).
- **High-quality passengers take a third of the graphics memory**, so cards with 6-8 GB no longer run out after a few minutes ([#2711](https://github.com/openOMSI-org/openOMSI/pull/2711), @thibautgnt).
- **Enhanced at night:** a street lamp hidden behind a house, a tree or the ground no longer darkens the whole picture ([#2659](https://github.com/openOMSI-org/openOMSI/pull/2659), @thibautgnt).
- Buses whose display fonts share one bitmap decode it once instead of once per font ([#2730](https://github.com/openOMSI-org/openOMSI/pull/2730), @isaacsa2).
- **Timetable buses appear when their tour runs:** the first trip of a tour comes at its departure instead of a quarter of an hour early, and a tour under way has its bus waiting at its first stop between trips.
- An AI bus that served a stop from behind the bus in front pulls up and boards again once the stop is free; a traffic light's approach request follows the indicator.
- **Windows**: Num4 + Left held together no longer leave the steering stuck; DirectInput and Linux controller axes are read from their real range.
- Scenery sounds carry as far as their range allows (up to 2 km) and fade out instead of stopping at 300 m; the paper timetable keeps Courier New's spacing.
- **Force feedback:** the wheel no longer pulls itself to the centre while the bus is stopped; the self-aligning force comes back as it moves ([#2185](https://github.com/openOMSI-org/openOMSI/pull/2185), @YanisK33).
- **A traffic light's approach request follows the indicator**: a path marked left or right asks only with that indicator, so a junction no longer requests every stage at once ([#2532](https://github.com/openOMSI-org/openOMSI/issues/2532)).
- The rear section's driver camera comes after the front's first and last cab views, as in OMSI 2, instead of one key press away from the standard view ([#2503](https://github.com/openOMSI-org/openOMSI/issues/2503)).
- Another add-on's key in keyboard.cfg no longer takes Ctrl+Shift+M (mirror editor) and similar game shortcuts from buses that have no use for that key ([#2577](https://github.com/openOMSI-org/openOMSI/issues/2577)).
- **Linux**: only real controller axes are used, and a free X axis steers counted from where it rests, so a macro keypad's knob no longer steers the bus ([#868](https://github.com/openOMSI-org/openOMSI/issues/868)).
- A remembered MSAA fallback is tried at the chosen MSAA again once with each new version (written down before the try, so a failing or crashing retry is not repeated at every start), instead of keeping a computer at 1x for good ([#2597](https://github.com/openOMSI-org/openOMSI/issues/2597)).
- Scenery sounds carry as far as their range lets them, up to the 800 m within which scenery objects run, and fade out over the last part instead of stopping dead at 300 m ([#2534](https://github.com/openOMSI-org/openOMSI/issues/2534)).

### Multiplayer

- Server admin commands are read by the session and carried out by the game; a host's own game now also carries out `tell`, `duty` and `duty-off`, and a kick passes its reason on ([#2725](https://github.com/openOMSI-org/openOMSI/pull/2725), @fsalexandre).
- Bus downloads through the web gateway have a ceiling of their own, so they can no longer crowd out players joining ([#2663](https://github.com/openOMSI-org/openOMSI/pull/2663), @fsalexandre).

### Project

- **Catalan** for the whole 0.3 interface ([#2734](https://github.com/openOMSI-org/openOMSI/pull/2734), @juanjopuntcat).
- **Spanish** completed and corrected ([#2746](https://github.com/openOMSI-org/openOMSI/pull/2746), @juanjopuntcat).
- **German** for the new texts ([#1987](https://github.com/openOMSI-org/openOMSI/pull/1987), @flows-c).
- resvg 0.48.1 ([#2689](https://github.com/openOMSI-org/openOMSI/pull/2689)); the architecture guard's limit for omsi-app follows main again ([#2721](https://github.com/openOMSI-org/openOMSI/pull/2721)).

### Credits

- **@turbo-devv**, for a whole triage day of fixes checked against Omsi.exe itself: timetables, articulated buses, cameras, controls and scenery sounds.
- **@juanjopuntcat**, for bringing two languages fully up to date in one day: Catalan and Spanish players get the whole 0.3 interface.
- **@Klyntch**, for tracking down the black VR headset and fixing it the way the OpenXR spec says.
- **@Spindlerkoo**, for the ticket hook: plugin authors can now build fare systems, and it costs nothing without a plugin.
- **@YanisK33**, for a wheel that finally rests when the bus does.
- **@fsalexandre**, for moving multiplayer out of the game's glue code command by command, and the gateway ceiling.

## 0.3.31 - 2026-10-15

This beta is mostly about smoothness: driving with timetable traffic no longer hitches every few seconds, rain no longer stutters on weak PCs, and the game starts faster after an update on Vulkan. On OpenGL and DirectX 11, street lamps and headlights light their surroundings, and a dedicated graphics card gets its full graphics settings. Maps made with the OMSI 1 editor, such as Great Grundorf 2, now have their whole road network, AI cars stop driving into articulated buses on servers, and the launcher's route map shows the city around the roads. Plus Brazilian Portuguese, safer online play and many smaller fixes.

### New and improved

- **No more short hitch every few seconds with timetable traffic:** since 0.2.46 the game froze for 13 to 20 ms about every 6 seconds while it looked up the coupled cars of the coming trains on disk. They are now looked up once; the traffic schedule costs less than in 0.2.30 ([#2654](https://github.com/openOMSI-org/openOMSI/pull/2654), @thibautgnt).
- **The route map shows the map's water, green areas, rivers, rail and borders** under the roads when the map has a package ([#2561](https://github.com/openOMSI-org/openOMSI/pull/2561), @lmonteirotech).
- **Light theme:** off toggles, key badges and selected buttons are easier to see ([#2358](https://github.com/openOMSI-org/openOMSI/pull/2358), @Equinity).
- **What another machine sends is checked for what it is** — a host's mod file a joining player fetches must be what its name says (a picture of a size a texture can have, a sound, a mesh the reader takes, a text), or it is left out and reported; a vehicle script that would never end is cut short instead of stopping the game.

### Fixes

- **Rain is steady on weak PCs:** the bus windows take turns to repaint their drops instead of all in the same frame, which caused a regular stutter of about 20 ms ([#2654](https://github.com/openOMSI-org/openOMSI/pull/2654)).
- **Faster first start after an update on Vulkan:** the shaders are compiled on several cores; on Windows the map was ready in 12.9 s instead of 22.4 s on a cold start ([#2654](https://github.com/openOMSI-org/openOMSI/pull/2654)).
- **Scenery scripts run on helper threads again, as in 0.2.34**, and AI traffic costs less in the frames that run a traffic step, with identical results ([#2654](https://github.com/openOMSI-org/openOMSI/pull/2654)).
- **Enhanced without anti-aliasing:** PCs no longer pay for the extra depth pass under excavations, which only helps Apple graphics ([#2654](https://github.com/openOMSI-org/openOMSI/pull/2654)).
- **Dedicated servers use less memory:** they no longer keep a copy of every mesh of the map ([#2662](https://github.com/openOMSI-org/openOMSI/pull/2662), @fsalexandre).
- **OpenGL and DirectX 11:** a graphics card of its own keeps the player's MSAA, shadows and texture memory instead of being taken for a small shared chip, and street lamps, headlights and interior lamps light their surroundings as on DirectX 12 ([#2654](https://github.com/openOMSI-org/openOMSI/pull/2654)).
- **Motion-blur photos:** passengers, the driver, the money and the ticket blocks move with the bus instead of being left behind as ghosts ([#2652](https://github.com/openOMSI-org/openOMSI/pull/2652), @isaacsa2).
- Compiled shader data is saved again when it changes, so later starts can reuse it ([#2651](https://github.com/openOMSI-org/openOMSI/pull/2651), @isaacsa2).
- **Maps made with the OMSI 1 editor, such as Great Grundorf 2:** the whole road network and all start points are there; before, most roads were missing and there was nothing to start from ([#2670](https://github.com/openOMSI-org/openOMSI/pull/2670), @turbo-devv).
- **Legacy mouse steering:** the crosshair, steering and pedals stay aligned with the cursor after mouse steering is switched off and on again ([#2094](https://github.com/openOMSI-org/openOMSI/pull/2094), @FakeShxp3).
- The Discord links open the openOMSI server again ([#2681](https://github.com/openOMSI-org/openOMSI/pull/2681)).

### Multiplayer

- **Dedicated server and LAN host:** AI cars see the whole articulated bus of every player and no longer drive into its rear section ([#1968](https://github.com/openOMSI-org/openOMSI/pull/1968), @veryinsanee).
- **Safer content from other machines:** every file a host sends must be what its name says, and no script can run for ever ([#2666](https://github.com/openOMSI-org/openOMSI/pull/2666), @fsalexandre).

### Project

- **Brazilian Portuguese** for 222 texts that had none ([#2665](https://github.com/openOMSI-org/openOMSI/pull/2665), @fsalexandre).
- The end-of-trip notice and the other in-game messages in every language of the game ([#2655](https://github.com/openOMSI-org/openOMSI/pull/2655)).
- Proxy logins no longer appear in the logs ([#2650](https://github.com/openOMSI-org/openOMSI/pull/2650), @isaacsa2).
- Dependency updates (rand, sevenz-rust2, unrar-rs) and both dependency audits keep running after a failed check ([#2680](https://github.com/openOMSI-org/openOMSI/pull/2680), @isaacsa2).

### Credits

- **@turbo-devv**, for bringing the OMSI 1 maps back to life: Great Grundorf 2 players finally get their roads and their 49 start points.
- **@fsalexandre**, for three careful pull requests split out at review: content checks that make online play safer, Brazilian Portuguese for 222 texts, and a lighter dedicated server.
- **@lmonteirotech**, for the map packages under the launcher's route map: the Drive page now looks like the real city.
- **@veryinsanee**, for measuring exactly how far the AI cars drove into articulated buses on servers, and fixing it.
- **@FakeShxp3**, for chasing the mouse steering offset through every way of switching it off and on.
- **@Equinity**, for making the launcher's light theme readable.
- **@VlastikYoutubeKo**, for making a test pass on Czech Windows, so every contributor can run the whole suite whatever their language ([#1982](https://github.com/openOMSI-org/openOMSI/pull/1982)).
- **@isaacsa2**, for the photo, shader cache and proxy fixes, the audit fix, and for reviewing the performance pull request.
- And thanks to everyone who tests the betas and reports what breaks: this is how a beta becomes a good stable release.

## 0.3.30 - 2026-10-15

### New and improved

- **Rain on the windows is much smoother on weak PCs:** the drops are painted on two threads of their own ([#2643](https://github.com/openOMSI-org/openOMSI/pull/2643), @thibautgnt).
- **Less graphics memory:** about a fifth less texture memory for the same picture, so graphics cards with 2 to 8 GB stop running short on big maps ([#2643](https://github.com/openOMSI-org/openOMSI/pull/2643)).
- **Graphics settings do what they say:** phones keep their own defaults, presets keep reflections, rows that do nothing in the running mode are hidden, and every graphics text is translated ([#2643](https://github.com/openOMSI-org/openOMSI/pull/2643)).

### Fixes

- **The game no longer turns blurry and slow after a while** when the graphics card refuses one texture: the texture budget is no longer cut for it ([#2643](https://github.com/openOMSI-org/openOMSI/pull/2643)).
- **A frame limit (or V-sync off) costs less processor:** the limiter sleeps instead of spinning, and wakes on time at high frame limits ([#2643](https://github.com/openOMSI-org/openOMSI/pull/2643)).
- **Maps and buses load faster from the second start:** compressed textures are kept in `~/.openomsi/cache/textures` instead of being compressed again on every start ([#2643](https://github.com/openOMSI-org/openOMSI/pull/2643)).
- **Enhanced: bus windows look like glass:** they show the cabin and the street instead of a bright sky mirror ([#2643](https://github.com/openOMSI-org/openOMSI/pull/2643)).
- On weak PCs, rain can still show a regular stutter of about 20 ms; a fix is on the way.

### Performance

- **Enhanced, 1× MSAA: higher frame rates where excavations are in view** (49 → 58 fps at a Thüringer Wald spot on an M4 Pro) ([#2644](https://github.com/openOMSI-org/openOMSI/pull/2644), @SpicySpaceman).

### Project

- rustls 0.23.46 in the game and the launcher, for the four RustSec advisories of October 14 (updates, downloads, online play), and the security check now audits the launcher's own lockfile too ([#2648](https://github.com/openOMSI-org/openOMSI/pull/2648), @isaacsa2).
- The Download page keeps the betas apart from the stable release, with the release history on its own ([#2646](https://github.com/openOMSI-org/openOMSI/pull/2646), @isaacsa2).

### Credits

- **@SpicySpaceman**, for tracking down why Enhanced at 1× MSAA shaded hidden terrain under excavations, and for measuring the gain before and after. Players with excavations in view get those frames back thanks to you.
- **@isaacsa2**, for jumping on the rustls advisories the day they came out and making sure the launcher's lockfile is audited from now on, for the clearer Download page, and for a careful review of the optimisation pull request: you caught the texture cache going over its disk budget when several textures are compressed at once, before any player hit it.
- And thanks to everyone who tests the betas and reports what breaks: this is how a beta becomes a good stable release.

## 0.3.0 - 2026-10-14

A stabilization release: the slowdowns and breakages that came with the updates since 0.2.20,

traced back to the changes that caused them and fixed.

### New and improved

- **Vanilla and Vanilla+ sky:** no more clouds stretched into layers and streaks towards the horizon; past the edge of the cloud dome there are no clouds, as in OMSI 2.
- Articulated AI buses' rear sections follow smoothly between frames.
- **Phones**: the Mods page has its own storage browser to install a folder or a .zip, .7z or .rar archive; "Add folder" and ZIP did nothing ([#2513](https://github.com/openOMSI-org/openOMSI/issues/2513)).
- The bus preview shows the chosen paint in the classic Drive page too ([#2585](https://github.com/openOMSI-org/openOMSI/pull/2585)).
- **iPhone and iPad**: a first start welcomes the player, shows how to copy OMSI 2 onto the device, finds the copy by itself and says to turn the phone and drive; the welcome page shows the openOMSI logo ([#2500](https://github.com/openOMSI-org/openOMSI/pull/2500)).

### Fixes

- **Big maps with a lot of AI traffic run smoothly again:** the AI vehicles' scripts run once a frame, as before 0.2.28, instead of in every 20 ms traffic step. At low frame rates they ran up to four times a frame, which made the frame slower still; articulated buses paid it for every section. Fewer "Simulation" stutters and no more slide to a few frames a second ([#2246](https://github.com/openOMSI-org/openOMSI/issues/2246), [#2388](https://github.com/openOMSI-org/openOMSI/issues/2388), [#2208](https://github.com/openOMSI-org/openOMSI/issues/2208), [#2464](https://github.com/openOMSI-org/openOMSI/issues/2464), [#2579](https://github.com/openOMSI-org/openOMSI/issues/2579)).
- Passengers, pedestrians, the timetable and the weather move on once a frame again, not in every tick of the fixed step (since 0.2.39).
- With a wheel or joystick plugged in (Windows), the game no longer looks for controllers every 300 ms: it waits longer and longer while nothing changes, and looks again as soon as something is plugged in or out. No more regular small hitches.
- openOMSI starts again on older processors without POPCNT ("this app can't run on your PC", 0xC000001D) ([#2484](https://github.com/openOMSI-org/openOMSI/issues/2484)).
- Ambient occlusion is as strong as before 0.2.46 again, and the sun's glare and the fog lamps no longer shine through roofs and bus ceilings; invisible covers still hide what is behind them ([#2558](https://github.com/openOMSI-org/openOMSI/issues/2558), [#2605](https://github.com/openOMSI-org/openOMSI/issues/2605), [#2059](https://github.com/openOMSI-org/openOMSI/issues/2059)).
- **Enhanced**: glossy lacquer is sharp and shiny again, while paint that reflects little stays satin ([#2563](https://github.com/openOMSI-org/openOMSI/issues/2563), [#2301](https://github.com/openOMSI-org/openOMSI/issues/2301)).
- **AI traffic no longer stands still for good at big junctions and roundabouts** ([#2373](https://github.com/openOMSI-org/openOMSI/issues/2373), [#2562](https://github.com/openOMSI-org/openOMSI/issues/2562), [#2592](https://github.com/openOMSI-org/openOMSI/issues/2592), [#2257](https://github.com/openOMSI-org/openOMSI/issues/2257)):
  - cars drive into a junction unless they would stop inside the path of another car, instead of waiting at the edge of a big crossing whenever anyone stood inside it;
  - cars that wait for each other in a circle sort it out after a few seconds: the one that has waited longest goes (never through another vehicle, a red light or a person);
  - a car waiting at its give-way line no longer takes the right of way from the cars going round a roundabout or along a priority road;
  - a car waiting at a red light inside a junction no longer holds up the cross traffic that has green.
- **AI trains and trams stay on their tracks:** every car is laid along the track on its two bogies at the couplings its files declare, instead of being pushed behind the car ahead as a rigid block (since 0.2.24 and 0.2.28).
- The ground under splines and spline attachments laid with `[spline_terrain_align]` is cut to the right one again: no more broken terrain beside or under roads ([#2004](https://github.com/openOMSI-org/openOMSI/issues/2004)).
- **Start sounds and other triggered sounds are heard again**: a sound started at zero volume takes the volume its curves give later ([#2552](https://github.com/openOMSI-org/openOMSI/issues/2552)).
- The in-game vehicle lists offer **Original paint** again, beside Random livery ([#2598](https://github.com/openOMSI-org/openOMSI/issues/2598)).
- The timetable sheet in the bus shows the timetable again ([#2575](https://github.com/openOMSI-org/openOMSI/issues/2575)).
- Passengers read the destination off what the bus's display shows again.
- Street lamps' shadows no longer switch between lamps at night after the mirrors are drawn (Enhanced) ([#2498](https://github.com/openOMSI-org/openOMSI/pull/2498)).
- **Passengers**: the original OMSI animations are the default again; the natural poses no longer raise a hand or put arms through the body, and people out of sight take fewer poses ([#2588](https://github.com/openOMSI-org/openOMSI/pull/2588)).
- Switching the update channel while the launcher is still looking for an update no longer leaves (or installs) the previous channel's result ([#2623](https://github.com/openOMSI-org/openOMSI/pull/2623)).
- If the launcher crashes, its log now says where, so a report can be traced ([#2574](https://github.com/openOMSI-org/openOMSI/issues/2574)).
- When the graphics driver resets the card and the game restarts with lower graphics, the game now says so instead of quietly looking worse.

### Project

- **Releases are started by hand:** a push to `main` no longer publishes a release by itself. Maintainers run the release from Actions, and promote a tested beta to stable as before (see [docs/VERSIONING.md](docs/VERSIONING.md)).
- Promotion runs from `main` only and reads the release policy from `main` ([#2620](https://github.com/openOMSI-org/openOMSI/pull/2620)).
- Governance and pull request rules, CODEOWNERS, a secret scan on every pull request and Dependabot for the launcher's own crates ([#2622](https://github.com/openOMSI-org/openOMSI/pull/2622), [#2633](https://github.com/openOMSI-org/openOMSI/pull/2633)).

## 0.2.60 - 2026-10-14

### New and improved

- **Beta and Stable releases:** pushes to `main` publish Beta releases for testers. Maintainers can promote a tested Beta to Stable without rebuilding or replacing its files. Releases include `SHA256SUMS`, and an already published version is never replaced ([#2565](https://github.com/openOMSI-org/openOMSI/pull/2565)).
- **Update channel in the launcher:** Settings → General → Updates offers Stable (the default, including for existing settings) and Beta. The Beta channel offers the newest compatible release, including prereleases, while skipping releases listed as broken. The version picker marks Beta releases, and Stable releases remain visible behind pages of newer Betas ([#2571](https://github.com/openOMSI-org/openOMSI/pull/2571)).

### Project

- The pull-request template asks authors to describe risks and check translations, compatibility and the absence of original OMSI 2 content ([#2566](https://github.com/openOMSI-org/openOMSI/pull/2566)).
- CodeQL analyses the project's Rust, JavaScript/TypeScript and GitHub Actions workflows; the custom scan excludes vendored launcher patches ([#2568](https://github.com/openOMSI-org/openOMSI/pull/2568)).
- The source distribution uses GNU AGPL v3 only and includes `TRADEMARKS.md`, retaining historical MIT and third-party notices ([#2599](https://github.com/openOMSI-org/openOMSI/pull/2599)).
- Switching update channels while a lookup is already running can leave a result from the previous channel. The approval of [#2571](https://github.com/openOMSI-org/openOMSI/pull/2571) accepts this as a follow-up; this release does not claim to fix that race.
- Dispatch Beta promotion from current `main`: running the promotion workflow from an older ref can read an older broken-release policy. This remains a follow-up from the review of [#2565](https://github.com/openOMSI-org/openOMSI/pull/2565).

### Credits

- The release workflow, launcher channels, review template and CodeQL changes were authored by **Thibaut (@thibautgnt)**. The original openOMSI codebase was created by **usonskyyyy**. The official organization is [openOMSI.org (`openOMSI-org`)](https://github.com/openOMSI-org).

## 0.2.50 - 2026-10-14

Additional security hardening and verification for mods, maps and online sessions (thanks to @thibautgnt); the details are in the list below. **Online play needs 0.2.48 on every computer** (the session protocol moves from 6 to 7; 0.2.47 was not released; an older game is refused cleanly when it joins), and a dedicated server's `admin_password` needs at least 8 characters.

### Fixes

- **Hardening**: the launcher's glib (GTK3, Linux) has the fix of RUSTSEC-2024-0429: its `VariantStrIter` wrote through a pointer it did not own mutably.
- **PR checks pass again**: the vendored glib is not counted by the size budget, three overgrown spots are split without changing what they do, and the simulation no longer depends on omsi-texture

### Project

- **Hardening**: ruzstd 0.9.1, the zstd decoder of plugin packages (`.oop`): a tiny crafted file could make it ask for all the memory (RUSTSEC-2026-0344).

## 0.2.48 - 2026-10-13

Additional security hardening and verification for mods, maps and online sessions (thanks to @thibautgnt); the details are in the list below. **Online play needs 0.2.48 on every computer** (the session protocol moves from 6 to 7; 0.2.47 was not released; an older game is refused cleanly when it joins), and a dedicated server's `admin_password` needs at least 8 characters.

### New and improved

- **Model test** in the launcher, for modders: a bus is loaded exactly as for a drive, without a window, and every warning and error of its `.bus`, model, meshes, scripts, constfiles and paint schemes is listed, with the textures that are not there; the running game's warnings show live beside it (`openomsi --check-vehicle`) ([#2524](https://github.com/openOMSI-org/openOMSI/pull/2524), by @icacar98-dot).
- **The navigator and the city map show the land under the roads** on a map that brings a package (`Maps/<Map>/openomsi-realmap`): water, parks and woods, rivers, rail and borders, in a dark or a light look; two new settings (Navigator surface, Light navigator). See docs/MAP_PACKAGE.md ([#2529](https://github.com/openOMSI-org/openOMSI/pull/2529), by @lmonteirotech).
- **The launcher grows with its window** on a big or wide screen, and has a **Launcher size** setting; **Launcher layout: Classic** brings back the Drive page in steps (Bus, Day & weather, Map & duty); the bus preview can be looked at from the **passenger's and the driver's seat**; the launcher asks less of a weaker computer and rests while you drive ([#2493](https://github.com/openOMSI-org/openOMSI/pull/2493), by @thibautgnt; it supersedes [#1631](https://github.com/openOMSI-org/openOMSI/pull/1631)).

### Fixes

- **Hardening** (by @thibautgnt): mod archives can no longer write outside the mod being installed; texture and other content names cannot point at another computer (`\\server\share`) or climb out of the content folders, nor can another player's roller blind and script strings; online sessions: the host is signed (a per-session key), so nobody can pose as it on the relay or for its mods, a player's place cannot be taken with an overheard hello, the session is not told to strangers from outside the network, and a dedicated server's admin login uses PBKDF2 instead of one hash over plain UDP; bans hold the player's address; the mods download, the WebSocket gateway and files that claim billions of entries can no longer exhaust memory or hang the game; crafted glTF, `.x`, TGA and zip files no longer crash it; Android: other apps can no longer fake an update's answer; the website shows issue text through an allowlist with a Content-Security-Policy.
- **Hardening**: the launcher's glib (GTK3, Linux) has the fix of RUSTSEC-2024-0429: its `VariantStrIter` wrote through a pointer it did not own mutably.
- **Hardening**: ruzstd 0.9.1, the zstd decoder of plugin packages (`.oop`): a tiny crafted file could make it ask for all the memory (RUSTSEC-2026-0344).
- **The loading screen has its plain dark background again**, without the map's picture (0.2.46 put it there, [#1383](https://github.com/openOMSI-org/openOMSI/pull/1383)).
- **Passengers read the destination from the IBIS**: the depot row the IBIS was set to, not the display script's own row - nobody boarded a bus showing a code typed past the list ([#2508](https://github.com/openOMSI-org/openOMSI/pull/2508), by @icacar98-dot).
- **With one door open, nobody waits at a shut door with a button**: they walk to the open one ([#2509](https://github.com/openOMSI-org/openOMSI/pull/2509), by @icacar98-dot).
- **People who get off where there is no pavement spread out beside the bus** instead of crowding the doorway, before they go ([#2510](https://github.com/openOMSI-org/openOMSI/pull/2510), by @icacar98-dot).
- **The map's drivers stay out of the passengers** also when its humans.txt names nobody installed ([#2511](https://github.com/openOMSI-org/openOMSI/pull/2511), by @icacar98-dot).
- **A repaint's folder no longer paints over the details of the other repaints in it**: only what its `.cti` names comes from there ([#2515](https://github.com/openOMSI-org/openOMSI/pull/2515), by @icacar98-dot).
- **A paint scheme's `[setvar]`s are written before every frame of the scripts**, as Omsi.exe does - the IK-201's display went back to Latin letters only so ([#2519](https://github.com/openOMSI-org/openOMSI/pull/2519), by @icacar98-dot).
- **Serbian and Croatian text in Windows-1250** (č, ć, đ with š or ž) is read as such, no longer as Western letters ([#2521](https://github.com/openOMSI-org/openOMSI/pull/2521), by @icacar98-dot).
- **Mac**: the app is no longer overwritten by the game program beside it ([#2522](https://github.com/openOMSI-org/openOMSI/pull/2522), by @thibautgnt).
- **iPhone**: the touch buttons answer where they are drawn from the first drive ([#2491](https://github.com/openOMSI-org/openOMSI/pull/2491), by @thibautgnt).
- **Dependencies**: the crates' minor updates, pollster 1.0 and sha2 0.11 ([#2544](https://github.com/openOMSI-org/openOMSI/pull/2544), [#2545](https://github.com/openOMSI-org/openOMSI/pull/2545), [#2546](https://github.com/openOMSI-org/openOMSI/pull/2546)); Dependabot now comes once a month with the major versions grouped.

## 0.2.46 - 2026-10-13

### New and improved

- Setting "The mouse works the pedals" - off, mouse steering only steers and the pedals stay on the keys ([#1704](https://github.com/openOMSI-org/openOMSI/issues/1704)).
- Setting "Pedal keys speed" for a finer throttle and brake on the keyboard ([#1371](https://github.com/openOMSI-org/openOMSI/issues/1371)).
- Keys for the radio - next / previous station, louder / quieter - for the keyboard, a wheel or a pad ([#1293](https://github.com/openOMSI-org/openOMSI/issues/1293), [#1462](https://github.com/openOMSI-org/openOMSI/issues/1462)).
- The mouse's side buttons (Mouse 4, Mouse 5) can be bound like keys ([#2281](https://github.com/openOMSI-org/openOMSI/issues/2281)).
- **Key "Free camera keys on/off": the free view's W, A, S, D...** can work the bus instead ([#1779](https://github.com/openOMSI-org/openOMSI/issues/1779)).
- What passengers say is shown as OMSI's hints (too cold, too little change, 'Pardon?') ([#1806](https://github.com/openOMSI-org/openOMSI/issues/1806)).
- Setting "People at once" (Settings, Gameplay; OMSI's AI humans limit, 50-1000) in the launcher and the game menu ([#2087](https://github.com/openOMSI-org/openOMSI/issues/2087)).
- **Launcher**: a calendar for the start date, and the mouse wheel turns the start time and date ([#2351](https://github.com/openOMSI-org/openOMSI/issues/2351), [#1625](https://github.com/openOMSI-org/openOMSI/issues/1625)).
- **Launcher**: it says that it rests while a game runs, and draws nothing meanwhile ([#2362](https://github.com/openOMSI-org/openOMSI/issues/2362)).
- **Information bar**: the road's speed limit and a stop request ([#2476](https://github.com/openOMSI-org/openOMSI/issues/2476), [#1767](https://github.com/openOMSI-org/openOMSI/issues/1767)).
- **Launcher**: a tour that does not run on the chosen date can be driven on its next day with one click ([#1626](https://github.com/openOMSI-org/openOMSI/issues/1626)).
- **Quick menu**: the destination form offers the depot file's destinations ([#2219](https://github.com/openOMSI-org/openOMSI/issues/2219)).
- **Touch controls**: an announcement button (Q) ([#2107](https://github.com/openOMSI-org/openOMSI/issues/2107)).
- Screenshots leave the interface out, as in OMSI ([#1265](https://github.com/openOMSI-org/openOMSI/issues/1265)).
- The loading screen shows the map's picture ([#1383](https://github.com/openOMSI-org/openOMSI/issues/1383)).
- Updates and downloads use the system's proxy ([#1000](https://github.com/openOMSI-org/openOMSI/issues/1000)).
- **Game menu**: choose which placed vehicle to drive ([#2026](https://github.com/openOMSI-org/openOMSI/issues/2026)).
- **Launcher**: the map's next departures over all lines ([#1885](https://github.com/openOMSI-org/openOMSI/issues/1885)).
- **Launcher**: a tour offers the buses its timetable gives it ([#1886](https://github.com/openOMSI-org/openOMSI/issues/1886)).
- **Launcher**: notes of your own on maps and buses ([#1791](https://github.com/openOMSI-org/openOMSI/issues/1791)).
- **Launcher**: the R and N keys say which gearbox they are for ([#1691](https://github.com/openOMSI-org/openOMSI/issues/1691)).

### Fixes

- **AI traffic no longer stands at green lights** near a vehicle whose script claims priority (the player's bus on some maps, ambulances and fire engines with the siren on): a junction is kept for an emergency vehicle only while it comes, and only against the cars whose way meets its; a left turn gives way to the oncoming traffic even when it got there first ([#2431](https://github.com/openOMSI-org/openOMSI/issues/2431), [#2307](https://github.com/openOMSI-org/openOMSI/issues/2307), [#2373](https://github.com/openOMSI-org/openOMSI/issues/2373)).
- **Cars no longer vanish in the middle of the road** where a junction's paths have no traffic density of their own ([#2466](https://github.com/openOMSI-org/openOMSI/issues/2466)).
- **AI traffic gives way as in OMSI 2**: where a map sets no priorities, whoever gets to the junction first goes first - no more main-road cars stopping for every car out of a side road on their right ([#2058](https://github.com/openOMSI-org/openOMSI/issues/2058), [#1452](https://github.com/openOMSI-org/openOMSI/issues/1452)).
- **AI cars no longer pass over a solid centre line** (`[rule] overtaking_prohib`, as OMSI reads it): the queue behind a bus at its stop waits ([#2048](https://github.com/openOMSI-org/openOMSI/issues/2048), [#1931](https://github.com/openOMSI-org/openOMSI/issues/1931)).
- **No road spray from aircraft and trains** of the AI traffic over a wet road ([#1330](https://github.com/openOMSI-org/openOMSI/issues/1330)).
- **Free drive**: a line and tour can be picked again from the pause menu with the timetable buses switched off ([#2016](https://github.com/openOMSI-org/openOMSI/issues/2016)).
- **Night tours run on past midnight**: their timetable buses come after 24:00, a duty picked after midnight is the night's tour under way, and the launcher keeps the trip picked after midnight ([#2166](https://github.com/openOMSI-org/openOMSI/issues/2166)).
- **Automatic IBIS**: a trip without a line number that the depot file runs as line 000 is typed itself, not the tour's next numbered trip, and its passengers board ([#2425](https://github.com/openOMSI-org/openOMSI/issues/2425)).
- **On foot during a duty, the navigator stays with the bus**: walking round it no longer recalculates the route or moves the stops on ([#2426](https://github.com/openOMSI-org/openOMSI/issues/2426)).
- The next trip of a duty begins as soon as the terminus is served, as in OMSI 2: the people at the terminus board for it ([#2148](https://github.com/openOMSI-org/openOMSI/issues/2148), [#2060](https://github.com/openOMSI-org/openOMSI/issues/2060)).
- AI buses queue close up at a stop, and the one right behind the bus at the stop takes people on at the same time, as in OMSI 2 ([#2429](https://github.com/openOMSI-org/openOMSI/issues/2429)).
- A timetable bus waiting at the first stop of its next trip keeps its doors open for the passengers ([#2433](https://github.com/openOMSI-org/openOMSI/issues/2433)).
- Timetable buses leave a terminus on time even while the player watches it ([#2235](https://github.com/openOMSI-org/openOMSI/issues/2235)).
- An AI bus whose script never shuts its doors no longer stands at the stop for good ([#1801](https://github.com/openOMSI-org/openOMSI/issues/1801), [#1921](https://github.com/openOMSI-org/openOMSI/issues/1921)).
- Door hit sounds (SD200/SD202, Kajosoft buses) play to their end instead of a short click ([#611](https://github.com/openOMSI-org/openOMSI/issues/611), [#1268](https://github.com/openOMSI-org/openOMSI/issues/1268)).
- An AI car that glitches out of sight no longer blasts its engine sound at full volume ([#2416](https://github.com/openOMSI-org/openOMSI/issues/2416)).
- The personnel file keeps the kilometres of every bus driven ([perbusinfo]), as OMSI does ([#2475](https://github.com/openOMSI-org/openOMSI/issues/2475)).
- Buses can be refuelled and charged in depots whose building is marked [depot], as in OMSI ([#1847](https://github.com/openOMSI-org/openOMSI/issues/1847)).
- In LAN play the AI buses show the repaint their AI list gives them on every player's screen ([#2018](https://github.com/openOMSI-org/openOMSI/issues/2018)).
- **Another player's articulated bus**: its rear section keeps the lean their game gives it, and its wheels stand still when the bus does ([#1177](https://github.com/openOMSI-org/openOMSI/issues/1177)).
- The game opens on the screen the launcher stands on again ([#1959](https://github.com/openOMSI-org/openOMSI/issues/1959), [#2025](https://github.com/openOMSI-org/openOMSI/issues/2025)).
- **Place a vehicle**: "Random livery" really picks a random livery ([#2410](https://github.com/openOMSI-org/openOMSI/issues/2410)).
- **Launcher**: the bus picker shows the whole name of a bus ([#2355](https://github.com/openOMSI-org/openOMSI/issues/2355)).
- **Android**: Change the keys and Set up a wheel open the Controls page ([#2308](https://github.com/openOMSI-org/openOMSI/issues/2308)).
- **Linux**: when the launcher cannot start (no WebKitGTK 4.1, e.g. SteamOS) the game says why in a dialog, with the steps for SteamOS ([#2458](https://github.com/openOMSI-org/openOMSI/issues/2458)).
- **Game menu**: moving the seat, head pitch or field of view shows the view behind the menu ([#2359](https://github.com/openOMSI-org/openOMSI/issues/2359)).
- **Textures**: "Compress textures on loading" no longer changes the colour of small colour swatches in texture atlases (handrails, buttons) ([#2292](https://github.com/openOMSI-org/openOMSI/issues/2292)).
- **Transparency**: a fully transparent [matl_alpha] 2 mesh hides what its model draws behind it again, as in OMSI (masks such as scroll boxes), in Enhanced as well ([#2059](https://github.com/openOMSI-org/openOMSI/issues/2059)).
- **Enhanced**: bus and car paint that mirrors only a little is satin again, no longer shining like glass against the light ([#2474](https://github.com/openOMSI-org/openOMSI/issues/2474)).
- **Enhanced**: the headlamps' beam tilts with the bus on ramps and hills instead of staying level ([#1897](https://github.com/openOMSI-org/openOMSI/issues/1897)).
- **Enhanced**: displays and panels with a light map are as bright by day as in Vanilla+ ([#2051](https://github.com/openOMSI-org/openOMSI/issues/2051)).
- **Vanilla+**: nights, dusk and shade are as dark as in OMSI 2; the light is laid on the texture the way OMSI does ([#300](https://github.com/openOMSI-org/openOMSI/issues/300), [#1612](https://github.com/openOMSI-org/openOMSI/issues/1612)).
- Ctrl+Shift+U (and other chords with Ctrl or Alt) no longer also runs the game's own Shift+U, Shift+R, Shift+M, Shift+N or Shift+number; those and Ctrl+Shift+D/E/G, Ctrl+M step aside where keyboard.cfg binds the chord ([#2478](https://github.com/openOMSI-org/openOMSI/issues/2478)).
- Num+ and Num- no longer zoom the outside camera when they are bound (OMSI's file has the kickdown and a door on them); new keys "Zoom in" / "Zoom out" (view_zoom_in, view_zoom_out) ([#2469](https://github.com/openOMSI-org/openOMSI/issues/2469), [#2322](https://github.com/openOMSI-org/openOMSI/issues/2322)).
- Shift, Ctrl and Alt can be bound on their own, also OMSI's "Shift/Ctrl/Alt (both)" entries ([#2003](https://github.com/openOMSI-org/openOMSI/issues/2003)).
- A gamepad lying idle no longer pulls the wheel back while the keys steer (the wheel went to and fro) ([#1895](https://github.com/openOMSI-org/openOMSI/issues/1895)).
- Gamepad steering turns the wheel slower at speed, not only less far ([#2014](https://github.com/openOMSI-org/openOMSI/issues/2014)).
- Switching cab cameras, the zoom glides to the new camera's along with the view instead of jumping ([#2306](https://github.com/openOMSI-org/openOMSI/issues/2306)).
- With a wheel the arrow keys switch the cab cameras by default, as in OMSI (the glance stays as a setting) ([#2164](https://github.com/openOMSI-org/openOMSI/issues/2164)).
- Head tracking turns the view smoothly at every frame, also with a 30 Hz webcam tracker ([#2216](https://github.com/openOMSI-org/openOMSI/issues/2216), [#1983](https://github.com/openOMSI-org/openOMSI/issues/1983)).
- A turn of the mouse wheel zooming the cab view no longer stops when a switch comes under the cursor ([#1224](https://github.com/openOMSI-org/openOMSI/issues/1224)).
- **AI trains**: every car of a train is read and uploaded ahead, so trains (X10 S-Bahn and others) no longer cause half-second stutters when they appear or turn at a terminus ([#2365](https://github.com/openOMSI-org/openOMSI/issues/2365), [#2464](https://github.com/openOMSI-org/openOMSI/issues/2464)).
- **Graphics**: when the graphics card runs out of memory, a texture that cannot be made is drawn plain and the game keeps fewer textures, instead of a black picture for the rest of the session ([#2472](https://github.com/openOMSI-org/openOMSI/issues/2472)).
- **Passenger avatars**: their pictures are read in the background before anybody is drawn with them, so walking or driving past new people no longer stutters ([#2463](https://github.com/openOMSI-org/openOMSI/issues/2463)).
- Ticket blocks no longer stay floating over the road after changing or removing the bus, and the cursor shows the hand over them and over the change ([#2340](https://github.com/openOMSI-org/openOMSI/issues/2340), [#2399](https://github.com/openOMSI-org/openOMSI/issues/2399)).
- A coin of the change on the tray is picked up again with a click, as in OMSI ([#1354](https://github.com/openOMSI-org/openOMSI/issues/1354)).
- Passengers who get off where there is no pavement walk away from the bus and go instead of standing at the doors ([#1290](https://github.com/openOMSI-org/openOMSI/issues/1290)).
- Passengers press the stop button in good time also on buses whose doors stay released ([#2346](https://github.com/openOMSI-org/openOMSI/issues/2346)).
- Passengers get off where the map's stops say they do, also at stops far from the bus ([#1583](https://github.com/openOMSI-org/openOMSI/issues/1583), [#2418](https://github.com/openOMSI-org/openOMSI/issues/2418)).
- Bus drivers from the map's driver list no longer appear among the passengers ([#2223](https://github.com/openOMSI-org/openOMSI/issues/2223)).
- Passengers wait at the stops round the camera, so a timetable bus watched along its route takes people on and lets them off at every stop ([#2319](https://github.com/openOMSI-org/openOMSI/issues/2319)).
- A stop whose object stands at the start of a long bay is served where the bus stops with its doors open: the IBIS, navigator and information bar go back to it ([#2468](https://github.com/openOMSI-org/openOMSI/issues/2468)).
- The quick menu follows its key in keyboard.cfg (open_menue, Alt by default): moved to another key, Alt no longer opens it ([#2392](https://github.com/openOMSI-org/openOMSI/issues/2392)).

## 0.2.45 - 2026-10-13

### New and improved

- **An `[htmltexture]` page that says `lit` is a surface on a bus too**, not only on a scenery object: it takes the sun, the street lamps and the fog instead of glowing at its own brightness. With a dot mask and a white light map such a page is an LED matrix whose dots burn and glow in Enhanced like the Krueger and K++ panels. Pages without the word look exactly as before ([#2460](https://github.com/openOMSI-org/openOMSI/pull/2460), by @NACHN).
- **Traditional Chinese** for the rest of the launcher and the pause menu: Drive, Settings, Controls, Profile, the head tracking and ambience settings, the controller descriptions and the status messages ([#2459](https://github.com/openOMSI-org/openOMSI/pull/2459), by @shamough1792). The launcher's "Light" theme is now named as a theme in every language, no longer as the light of the photo mode.

### Fixes

- **The driver no longer flickers in front of the cab view while the bus drives fast** (from about third gear, sooner at low frame rates such as on phones). The figure is still seen in the mirrors, and in the photo mode once its camera leaves the driver's eyes ([#2449](https://github.com/openOMSI-org/openOMSI/issues/2449), [#2461](https://github.com/openOMSI-org/openOMSI/pull/2461), by @thibautgnt).
- **AI buses sound like they pull away and brake.** Their scripts now get the throttle and brake pedals, so engines rev up under load and brake sounds play, instead of idling all the way (Hamburg's HHA buses and others) ([#2456](https://github.com/openOMSI-org/openOMSI/pull/2456), by @SpicySpaceman).
- **On foot, getting up under a building** (an indoor terminus, a canopy, a covered platform) leaves you on the floor under the roof instead of on top of the building ([#2201](https://github.com/openOMSI-org/openOMSI/pull/2201), by @KYChung1287).

## 0.2.44 - 2026-10-13

### New and improved

- **Character models: High quality (improved)** (Settings, Gameplay, Passengers) draws the people with the 91 everyday [Microsoft Rocketbox avatars](https://github.com/microsoft/Microsoft-Rocketbox) (MIT) in place of the content's own, on your computer only: each person an avatar of the same kind, posed by OMSI's animation or the procedural one, with a living face from the avatars' ARKit blend shapes - blinks, glances, a mood of their own, passing smiles, raised brows, frowns and yawns, a smile at the cash desk, and lips that move with what they say. Servers and other players keep their own people. The avatars are downloaded once, like the ambience (about 310 MB). **Original** stays the default.
- **glTF / GLB, FBX and OBJ models** wherever OMSI reads a mesh (sceneries, splines' objects, vehicles, humans), with their materials, textures beside the model or packed into it, skins and blend shapes; the format is told by content.
- **Every picture format**: DDS with BC4-BC7, the DX10 header's 10-bit, float and packed formats, ATI1/ATI2 and DX10 cube maps; KTX 1 / KTX 2 (BC, ETC, ASTC); WebP, GIF, TIFF, QOI, OpenEXR, Radiance HDR, ICO, PNM and farbfeld; 16-bit PNGs. A texture is read by its content, whatever its name says, and found under all these extensions.
- **Every sound format**: WAV with extensible floats, 64-bit float, A-law / mu-law, 20- and 12-bit samples, RF64, ADPCM and the truncated headers old tools wrote; AIFF, CAF, M4A (AAC, ALAC), Matroska audio and MP2 besides Ogg, FLAC and MP3, all told by content. Surround is mixed down to stereo, and a sound missing under its name (`horn.wav`) is taken in another format of the same name (`horn.ogg`).

### Fixes

- **Higher frame rates below 120 frames a second, mostly on big maps with heavy AI lists.** Since 0.2.39 the whole simulation ran 120 times a second however fast the picture was. Each frame below 120 fps ran it two to four times (at 30 fps four times), up to 13 times in a slow frame, and every run brought the AI cars' meshes, lamps, drivers and sounds and every person's pose up to date again. A slower frame held more ticks and got slower still. Now:
  - the simulation steps 60 times a second;
  - a frame runs at most four steps, so the game keeps real time down to 15 fps;
  - what only the picture and the sound need of the traffic and the people is brought up to date once a frame, in its last step. At 30 fps the simulation's share of the frame is about half what it was ([#2246](https://github.com/openOMSI-org/openOMSI/issues/2246), [#2388](https://github.com/openOMSI-org/openOMSI/issues/2388), [#2295](https://github.com/openOMSI-org/openOMSI/issues/2295), [#2365](https://github.com/openOMSI-org/openOMSI/issues/2365)).
- **The frame rate no longer collapses after half an hour on 8 GB graphics cards.** The texture budget counted only the scenery's and the vehicles' textures. All the others (the people's, the scripts' and texts', the sky's and the terrain's) grew past it until the card ran out of its own memory: from 140 to 25 fps on a long drive. Textures beyond half of the card's memory are now taken from the scenery and the vehicles ([#2295](https://github.com/openOMSI-org/openOMSI/issues/2295), [#2121](https://github.com/openOMSI-org/openOMSI/issues/2121), [#1314](https://github.com/openOMSI-org/openOMSI/issues/1314)).
- **The automatic render scale no longer swings between 85 and 100 % every ten seconds** on a card at its limit. Each swing made new targets and was a stutter of its own. A step up that does not hold now waits twice as long before the next try, up to two minutes ([#2295](https://github.com/openOMSI-org/openOMSI/issues/2295), [#2121](https://github.com/openOMSI-org/openOMSI/issues/2121)).
- **Night in Enhanced is lighter on the graphics card.** Only the 12 AI cars nearest the camera, within 200 m, light the road with their headlamps; every car on the street did, a hundred lights and more on a busy road ([#1774](https://github.com/openOMSI-org/openOMSI/issues/1774), [#1467](https://github.com/openOMSI-org/openOMSI/issues/1467)).
- **Macs with Apple M4 draw the full picture again.** 0.2.43's far-map shadow for Enhanced+ made the scene shader too big for Apple's shader compiler. It crashed on every scene pipeline, and the game fell back to the basic picture or closed. The same shadow now takes one call instead of two.

## 0.2.43 - 2026-10-13

### New and improved

- **Photo mode: depth of field without ghost copies.** Out of focus things were drawn as so many separate copies side by side (one for each point of the aperture the photo was taken through), worst wide open. Every picture of the shot now reads back its depth and is softened by what lies between two copies, so they run together into one smooth blur even at 16 samples. Lamps burnt out in the picture spread into bright bokeh discs.
- **Photo mode focuses where you click**, and focusing "On the bus" finds the bus's side the camera looks at instead of its middle a couple of metres behind it.
- **Photo mode, Advanced (for photographers)**, chosen in the launcher (Settings, Interface & online, Photo mode): a camera body with a sensor size (full frame, APS-C, Micro Four Thirds, medium format, 1-inch) and real focal lengths; P/A/S/M exposure in third stops (f/1 - f/32, 30 s - 1/8000, ISO 50 - 51200) with Auto ISO that keeps the time hand-held and exposure compensation; matrix, centre-weighted or spot metering of the scene's light (some EV 15 at noon, EV 4 at night) with an exposure meter; sensor noise that grows with the ISO, a sensor that burns out over its white and diffraction when closed down; the lens as it comes (distortion, vignetting wide open, colour fringes); tilt (the plane of focus leaned about its hinge line, for a miniature look or a whole street sharp) and shift (verticals kept upright); white balance by preset or in kelvin, picture profiles, split toning; histogram, clipping warning (zebra) and focus peaking in the live view; the depth of field's limits and the hyperfocal distance; exposure bracketing; 16-bit PNG; the camera's settings written into every photo as XMP metadata.
- **Photo mode, Simple** keeps its few settings and gains motion frozen, panned with the bus or from a tripod (the bus blurs past a sharp street), bright bokeh of lights, "everything sharp" (the hyperfocal distance), clarity, borders (white, black, instant print, gallery mat) and a spirit level.
- **Passengers move by their own poses by default** (Settings, Passengers: Natural; Original OMSI stays to choose). They wait in ways of their own - arms hanging, hands clasped or behind the back, arms crossed, a hand on the hip, looking at a phone - and each walks a little differently.
- **Units** for the game's own texts: metric, British (mph, miles and yards) or American (mph, miles and feet, °F) - the navigator and its speed limit signs, the information bar, the touch speedometer, LAN players' distances and the trip summary.
- **Time menu**: the date with its weekday, moved by a day, a week or a month with the time of day kept; the timetable's day, the season and the map's chrono follow it.
- **Plugins can draw on a bus's script textures** and react to clicks on them (`[scripttexturelist]`, `AccessScriptTexture`, `AccessScriptTexturePointer`, see docs/PLUGINS.md), so ticket machines and driver terminals written for OmsiHook can run ([#2367](https://github.com/openOMSI-org/openOMSI/pull/2367), by @Spindlerkoo).
- **The launcher's timetable editor is back**: lines, tours and trips of a map, saved as `.ttl` files in the content folder, never into OMSI 2's own ([#2448](https://github.com/openOMSI-org/openOMSI/pull/2448), by @thibautgnt).
- **A custom weather can be made again** on the launcher's Drive page ([#2446](https://github.com/openOMSI-org/openOMSI/pull/2446), by @thibautgnt).
- **The Drive page shows what to type into the bus's IBIS** for the chosen line again ([#2445](https://github.com/openOMSI-org/openOMSI/pull/2445), by @thibautgnt).
- **The bus radio's stations can be edited in the launcher again** (Settings, Sound) ([#2444](https://github.com/openOMSI-org/openOMSI/pull/2444), by @thibautgnt).

### Fixes

- **Nothing that goes with a moving vehicle shakes any more**: the mirrors, the bus's lamps and their glow, the passengers in every bus, the coins on the change tray, the highlighted switch, AI cars beside the bus and the view on foot aboard a bus are drawn where the bus is drawn ([#2439](https://github.com/openOMSI-org/openOMSI/pull/2439), by @thibautgnt).
- **Key bindings and game controllers are kept across updates** and new install folders: they live in `~/.openomsi/Inputs` now, and the first start brings over those an earlier version saved beside the program ([#2443](https://github.com/openOMSI-org/openOMSI/pull/2443), by @thibautgnt).
- **An add-on's `Maps` folder next to the original `maps`** on a case-sensitive file system no longer hides the stock maps ([#2450](https://github.com/openOMSI-org/openOMSI/issues/2450), [#2451](https://github.com/openOMSI-org/openOMSI/pull/2451), by @Bluscream).
- **Passengers' legs stay under them when they turn**: the body turned to a new heading at once over feet that stayed put, the legs ran diagonally and the person seemed to slide.
- **Passengers no longer pop onto their seats**: the jump from the aisle onto the seat (as Omsi.exe places them) is drawn as a short glide while they sit down, and a foot no longer leaps up a step as the body climbs into the bus.
- **People sit on a stop's bench** as on a bus seat; they sat 11 cm too low and 31 cm too far back, with their legs straight down as if kneeling.
- **People no longer shine in the sun** in Enhanced: their cut-out hair and clothes were lit as leaves, from every side with the sun through them.
- **Enhanced+ shadows**: where no traced shadow is found for a surface (a pane, a step's edge, a thin handrail) the far shadow map still shades it, so there are no bright specks and stair-stepped lines under a roof; the sun no longer lays stripes of shadow on faceted ground and a road's camber where it grazes; reflections in the panes take their colour from the face turned to them.
- **The clock past New Year's Eve** goes on into the next year instead of starting the same year again.
- **Map: Esc -> Move** lets the city map be dragged again, and the bus goes where a click lets go.
- **Start points** show each name once, sorted as OMSI sorts them; a name whose places are all taken is left out, and choosing a name again goes on to its next free place.
- **Tip-up seats fold down under a player on foot** or a LAN player's figure sitting on them, as under a passenger.
- **A depot file's destination with the route in front of it** ("402 Apsley Station") is the timetable's stop to the people waiting, and the bus empties there.
- **A trip without a line of its own** shows none on the AI buses and the departure boards, as in OMSI.
- **Buses no longer sink into a street** with a road face buried under drawn ground (Ahlheim & Laurenzbach).
- **Text textures** written with 255 as the full colour flag are no longer yellow.
- **.x models**: texture coordinates and normals from DeclData (houses and trees drawn in one flat colour) and binary .x files (25 PAD-Labs houses were missing).
- **Train cars that run their own scripts take the paint's variables and the cables' signals.** A tram's second car, whose script is its own (the Konstal 105Na), started with the script's defaults and never got the paint's `[setvar]`s, so its destination boards, number plates and roof parts were missing in every livery but the plainest ([#3343](https://github.com/openOMSI-org/openOMSI/issues/3343)). It now wears the paint of the car in front, also when the paint is changed later. The cars' `[control_cable_*]` plugs now carry what a car's script writes down the train, car by car (a coupler or bogie without a script of its own passes it on), so the second car is powered, lit and braked with the first instead of waiting for its own start ([#3342](https://github.com/openOMSI-org/openOMSI/issues/3342)). Each car also measures the wire above its own pantograph (`GetHeightAbovePoint`), not the leading car's ([#3341](https://github.com/openOMSI-org/openOMSI/issues/3341)).

## 0.2.42 - 2026-10-12

### New and improved

- **Mirror quality**: "As the picture (Enhanced)" draws the mirrors with the Enhanced shading, its light and its sky, instead of the plain light (Settings, Graphics, and the pause menu).
- **Real-time reflections: Every frame** redraws every mirror in the picture each frame. In turns, each mirror's picture was a step behind, and the steps grew with the speed, so the street seemed to shake in them.
- **AI cars light the road with their headlamps** in Enhanced and Enhanced+: the stock cars have no spotlight of their own, so their lit front lamps now give them a pair of beams.

### Fixes

- **Windows: the WebView2 installer is offered only when the runtime is really missing.** Any launcher that failed in its first two seconds was taken for one without WebView2, and Microsoft's installer, answering "already installed", was offered at every start. With the runtime installed the launcher is now tried again with a fresh web view profile, and its own error is shown; a runtime whose files are missing is explained, with how to repair it.
- **The sun is its real size** (0.27 degrees) in Enhanced and Enhanced+, with a camera lens' glare round it instead of the eye's, which drew a white blot ten degrees round it ([#2400](https://github.com/openOMSI-org/openOMSI/issues/2400)).
- **The sun's glare no longer switches off and on as the camera turns**: with the sun just off the picture, whatever stood at the picture's edge decided whether the glare was there, and a sheet of light over the picture vanished and came back.
- **Paint and windows keep their shine at night** in Vanilla and Vanilla+, where the lamps light them, as in OMSI: the reflection was darkened twice by the night ([#2284](https://github.com/openOMSI-org/openOMSI/issues/2284)).
- **Bus and car bodies have a lacquer's shine in Enhanced**: a body that mirrors a few per cent (the MAN NL's 15 of 255) was drawn as matte plastic ([#2067](https://github.com/openOMSI-org/openOMSI/issues/2067)).
- **Photo mode keeps the driver at the wheel** when it is opened from the driver's seat and the camera moves away; every photo taken from there showed the bus without its driver.

## 0.2.41 - 2026-10-12

### Fixes

- **On a phone the on-screen pedals and wheel drive the bus again**: since 0.2.39 the accelerator, the brake, the clutch and the steering wheel lit up under the finger, but the bus never felt them - the fixed simulation step took the controllers' axes before the touch controls were added to them. The buttons and the menus were not affected ([#2442](https://github.com/openOMSI-org/openOMSI/pull/2442), by @thibautgnt).

## 0.2.39 - 2026-10-12

### New and improved

- **The simulation runs at a fixed 120 steps a second** on every computer and at every frame rate: the bus, the traffic, the passengers and the map's scripts move by the same steps at 30 and at 144 frames a second, and the bus and the camera are drawn between two steps, so the drive is the same however fast the picture is. The old step of one frame's length is gone (and with it `OMSI_FIXED_STEP`). A dedicated server steps 120 times a second too.
- **The desktop launcher is the only launcher**: the game's own launcher window is gone. A plain start opens the desktop launcher; where it is missing or cannot open its window (no WebView2 on Windows, which is offered to be installed, no WebKitGTK on Linux) the game says why and where it should be. On iPhone and iPad the "Cross-Platform Launcher" option is gone with it.
- **The launcher shows its Discord status again** ("In launcher"), while the setting is on, and leaves it to the game while a game starts or runs.
- **The pause menu shows the game behind it**, dimmed, on its first page; Settings and the other pages keep a solid background. The row over the game (Paused, the sign, the buttons, the bus and the driver) is gone; the sidebar stays.
- **Reflections on Enhanced+ have textures**: the windscreen, the rear window and other glass reflect the buildings, the trees and the buses with their real textures, not in plain colours.
- **Choosing the line and the tour is a pair of lists with a search again**, a tour found by any stop of its trips ([#2414](https://github.com/openOMSI-org/openOMSI/pull/2414), by @thibautgnt).
- **The Mods page gets back** cancelling an install, the archive's size check, filters, the content folder and dropping a mod anywhere ([#2412](https://github.com/openOMSI-org/openOMSI/pull/2412), by @thibautgnt).
- **Swedish (Svenska)** is one of the interface's languages ([#2411](https://github.com/openOMSI-org/openOMSI/pull/2411), by @Equinity).
- **A joined server sets the map, the clock, the weather and the buses** on the Drive page again ([#2408](https://github.com/openOMSI-org/openOMSI/pull/2408), by @thibautgnt).
- **Settings, Updates, Choose a version** lists the releases and installs the one picked; an older version stays until the latest is chosen again ([#2393](https://github.com/openOMSI-org/openOMSI/pull/2393), by @thibautgnt).
- **A door release action for any bus** (`doors_release`), beside the buses' own ([#2390](https://github.com/openOMSI-org/openOMSI/pull/2390), by @ThiBot77).
- **Add binding lists every action again** (the installed buses' triggers, OMSI's names) and a controller's buttons can be given any of them ([#2387](https://github.com/openOMSI-org/openOMSI/pull/2387), by @thibautgnt).
- **HTML textures can be lit surfaces**: `lit` after a page's path ([#2423](https://github.com/openOMSI-org/openOMSI/pull/2423), by @NACHN).
- **The core was rebuilt without a rewrite**: the map's data the simulation reads is one snapshot read without locks (52 locked reads per frame are gone), the map objects' scripts run only near the bus and in parallel, traffic, passengers and the timetable tell the picture what changed instead of drawing it themselves, the frame's stages get only the parts of the game they declare, and the sound's callback takes no locks and allocates nothing. With `OMSI_SIM_THREAD=1` traffic, passengers and the map's scripts run on their own thread beside the drawing: 0.5-1 ms less on the main thread per frame, 6-10 % more frames on a busy processor (off by default for now).

### Fixes

- **Far headlights glow round**: lights seen from afar had a square halo, from the light's sprite and the bloom's square blur.
- **The traffic's headlight glow stays on the car**: it was placed where the car was at the last simulation step, not where it is drawn, so it slid into the body of some cars and looked like flickering lamps.
- **The desktop launcher looks for updates, offers them and installs them again** ([#2401](https://github.com/openOMSI-org/openOMSI/pull/2401), by @thibautgnt, fixes [#2309](https://github.com/openOMSI-org/openOMSI/issues/2309)).
- **Joining a multiplayer game starts on the host's map** ([#2406](https://github.com/openOMSI-org/openOMSI/pull/2406), by @thibautgnt).
- **A game that ends on an error or is sent away by its server says so again** in the launcher ([#2404](https://github.com/openOMSI-org/openOMSI/pull/2404), by @thibautgnt).
- **A controller file that cannot be read is no longer replaced** by a device set up by itself ([#2398](https://github.com/openOMSI-org/openOMSI/pull/2398), by @thibautgnt).
- **A force-feedback wheel pushes back again** beside a shifter or a button box set up by itself to steer ([#2421](https://github.com/openOMSI-org/openOMSI/pull/2421), by @thibautgnt).
- **Steering keys at speed**: without Steering linearity a tap no longer swings the bus; past about 12 km/h the keys turn the wheel slower the faster it goes ([#2422](https://github.com/openOMSI-org/openOMSI/pull/2422), by @thibautgnt).
- **Setup no longer saves an incomplete OMSI 2 folder** over the one that works ([#2397](https://github.com/openOMSI-org/openOMSI/pull/2397), by @thibautgnt).
- **The profile's bar shows the progress within the level** ([#2396](https://github.com/openOMSI-org/openOMSI/pull/2396), by @thibautgnt).
- **Current weather, Weather cycle and custom weather stay chosen** when the launcher opens again and for the next duties ([#2395](https://github.com/openOMSI-org/openOMSI/pull/2395), by @thibautgnt; [#2382](https://github.com/openOMSI-org/openOMSI/pull/2382), by @ThiBot77).
- **Settings changed in the game are no longer put back by the launcher** ([#2394](https://github.com/openOMSI-org/openOMSI/pull/2394), by @thibautgnt).
- **The bus picker's maker list shows the makers' whole names again**, and every drop-down's list is as wide as its longest choice ([#2420](https://github.com/openOMSI-org/openOMSI/pull/2420), by @thibautgnt).
- **On a phone the pause menu's sliders follow the finger** and take a tap again ([#2381](https://github.com/openOMSI-org/openOMSI/pull/2381), by @thibautgnt), and **the trip summary can be closed** ([#2375](https://github.com/openOMSI-org/openOMSI/pull/2375), by @thibautgnt).
- **Door keys no longer pair two different doors** (GPM C2, Urbanway, Citelis) ([#2380](https://github.com/openOMSI-org/openOMSI/pull/2380), by @ThiBot77).
- **A quick menu tile or a failed quick load no longer pauses or resumes the game** on its own ([#2356](https://github.com/openOMSI-org/openOMSI/pull/2356), by @thibautgnt).

### Multiplayer

- **Running games shows the session code, the players and the chat** of a multiplayer game again ([#2407](https://github.com/openOMSI-org/openOMSI/pull/2407), by @thibautgnt).

## 0.2.38 - 2026-10-12

### New and improved

- **Wheels, pedals and joysticks set themselves up**: a device nobody has set up is set up by itself as soon as it is used (a force-feedback wheel at once), in the game and on the launcher's Game controllers page: its X axis steers, every axis resting at an end of its travel is a pedal - throttle, brake and clutch in the order the device lists them, turned round when it reads the other way - and a joystick or a wheel with combined pedals drives with its Y axis. The set-up is saved like one made by hand and a notification says what was set up; before, such a device only steered and the pedals had to be set up by hand. Gamepads keep their ready-made layout.
- **The game controllers page is back and redesigned**: live axes, buttons given by action, the step-by-step setup and the force feedback test ([#2347](https://github.com/openOMSI-org/openOMSI/pull/2347), by @thibautgnt).
- **A live performance panel**, compact or detailed (Settings, Interface): where each frame's time goes, what holds the frame rate back, the processor's load and the last stutter ([#2329](https://github.com/openOMSI-org/openOMSI/pull/2329), by @thibautgnt).
- **The Alt quick menu is a compact grid of icon tiles** off the screen's edge, see-through and in the game's language; Time and weather share one tile, Refuel and Wash have their own ([#2327](https://github.com/openOMSI-org/openOMSI/pull/2327), by @thibautgnt).
- **The bus picker lists one row per model** with its versions folded under it ([#2326](https://github.com/openOMSI-org/openOMSI/pull/2326), by @ThiBot77).
- **Current weather (METAR) and the weather cycle can be chosen again** in the launcher, with the airport ([#2313](https://github.com/openOMSI-org/openOMSI/pull/2313), by @ThiBot77).
- **Buttons that set the start to the current time and date** are back, on the computer and on the phone ([#2334](https://github.com/openOMSI-org/openOMSI/pull/2334), by @thibautgnt).
- **On a wide screen the Drive page spreads over two columns** and the bus preview is taller ([#2325](https://github.com/openOMSI-org/openOMSI/pull/2325), by @thibautgnt).
- **Drop-down lists drawn by the launcher's page**, no longer as wide as their longest entry ([#2314](https://github.com/openOMSI-org/openOMSI/pull/2314), by @ThiBot77).
- **The end-of-trip notice and other in-game messages in French** ([#2330](https://github.com/openOMSI-org/openOMSI/pull/2330), by @ThiBot77).

### Fixes

- **A line shows its two ends**: the launcher names a line by the first and last stop of its longest trip ("137 - Stadtgrenze - Falkensee Bhf"); it listed the termini of every short working and depot run one after another, which read as all its stops.
- **Mod buses have their 3D preview again** (the EN92, the GN92 and other big buses): a bus whose preview was still loading when the launcher closed or a game started was taken for one that hung the launcher and left out for good. The launcher now lets the preview helper end in order and clears the mark itself; previews left out by an older version are tried again after every update ([#2352](https://github.com/openOMSI-org/openOMSI/pull/2352), by @SpicySpaceman); a bus whose preview fails no longer shows the bus chosen before ([#2349](https://github.com/openOMSI-org/openOMSI/pull/2349), by @SpicySpaceman).
- **The bus preview no longer takes graphics memory from a game** started from the launcher on small graphics cards ([#2343](https://github.com/openOMSI-org/openOMSI/pull/2343), by @thibautgnt).
- **The version is right on macOS and Linux too**: the launcher's About window and bundle, multiplayer games, the dedicated server's listing, plugins and the radio's requests said 0.1.0 or 0.2.33.
- **The launcher's sliders**: the knob sits on the end of the coloured track; it stood beside it, by up to half its width.
- **The Drive page follows the computer's clock** when the real time or date is on ([#2312](https://github.com/openOMSI-org/openOMSI/pull/2312), by @ThiBot77).
- **The Time and Weather windows opened from the Alt quick menu** take the keys and the mouse again ([#2354](https://github.com/openOMSI-org/openOMSI/pull/2354), by @thibautgnt).
- **A livery's tinted windows on an alpha-tested layer are blended**, not drawn black (GPM C2) ([#2339](https://github.com/openOMSI-org/openOMSI/pull/2339), by @ThiBot77).

### Performance

- **The roundabout search is 2-5 times faster** at loading a map, finding the same roundabouts ([#2331](https://github.com/openOMSI-org/openOMSI/pull/2331), by @ThiBot77).

## 0.2.37 - 2026-10-12

### New and improved

- **WebView2 installed from the launcher**: on a Windows without Microsoft WebView2 the new launcher cannot open its window; the game's own launcher then offers to install it with Microsoft's own installer, and opens the new launcher once it is there.
- **The clock moves by a tap**: World options has -1 h, -10 min, +10 min and +1 h buttons beside the hour and minute, so the time can be moved on a phone too (typing the time needs Enter, holding it needs Ctrl+Shift+Page Up).

### Fixes

- **The new launcher shows the right version** on every system; it said 0.1.0.
- **A tour can be picked and started on a phone**: in the pause menu's Line and tour the timetable with its Start button is under the list on a narrow screen; it was left out there, so a tour could only be marked.

## 0.2.36 - 2026-10-11

(0.2.35 was not released: its changes come with this version.)

### New and improved

- **A new desktop launcher** (on Windows, macOS and Linux; on phones the same page in the app): the duty on one page - the map with its picture, the bus turning in a preview the game itself draws (drag to turn, wheel to zoom, sharp while it moves), paint, fleet number and depot, line, tour and trips with the route and its stops on the city map, time, date, season, weather and traffic - and Multiplayer, the driver's profile and runs, Mods, Settings, Controls, the running games and Setup. Calm graphite surfaces without outlines, one amber accent, Material Symbols icons, English unless another language is chosen. It stays open while a game runs and knows when a game has ended. Built on the organisation's fork of Tauri 2 ([tauri-openomsi](https://github.com/openOMSI-org/tauri-openomsi)). A plain start of openOMSI opens it; it needs WebView2 on Windows (part of Windows 10 and 11) and WebKitGTK 4.1 on Linux (`libwebkit2gtk-4.1-0`), and where it cannot start the game's own launcher opens instead.
- **A launcher made for phones**: a rail of icons, the bus with the destination sign and Start, the duty as a list whose rows open full-screen pickers (maps as pictures, buses, lines, tours, trips beside the route map, time and date by steps, weather). On Android the game's own launcher is no longer made under it, so the app starts faster.
- **openOMSI for iPhone, iPad and Mac** (built from the source with Xcode, see docs/APPLE.md): the game on iOS with touch controls inside the safe area, and a SwiftUI launcher in the same design.
- **Inter instead of Roboto** everywhere: the game's interface, pause menu, navigator, chat, photo mode, the html pages and the destination signs' fallback text.
- **A new loading screen**: the openOMSI logo large in the middle, the map's name under it, a striped bar along the bottom with tips on the left and how far on the right. The bar moves evenly along the time the last loads of the map took and the screen fades over the first pictures. The window opens dark, not blue.
- **Notifications as cards** in the launcher's look, with an icon in their kind's colour; the notes in the top left corner are gone: what the driver has to do (engine, gearbox, parking brake, air, stop brake, doors, change, a passenger's wish) comes as a to-do card.
- **Pause menu, navigator, city map and timetable window (Insert) redesigned** in the launcher's look: the pause menu with its sidebar, Material icons and a summary card; the navigator and city map with rounded corners and no dark halos; the timetable with the line on an amber sign and the stops as a table.
- **Weather in the interface's language**: the presets are named as their file names them in that language, else in English with a translation, instead of in German everywhere.
- **Photo mode moves with W A S D**, Space up, Ctrl down, Shift faster; the keys are shown in the panel.
- **No dashes or raised dots in the interface**: a plain hyphen, a colon or a comma instead.
- **Less work per frame**: street lamps' shadow tiles kept while nothing in them changes, mirrors keep their draw bundles, smoke written only when it changed, sky and shadow passes spread evenly, Enhanced passes that cannot show are left out, tiles streamed in batches and a season change read a few tiles a frame.
- **Frame limiter**: no sleep on top of V-sync and a real limit; mimalloc as the desktop allocator; the autosave written on a thread of its own.

### Fixes

- **Steadier frames**: the scenery scripts, traffic step and people run on the frame's thread with workers joining in instead of waiting for them (4.5 ms to 0.24 ms a frame for the scripts standing still on Spandau, and no more 20-70 ms frames from them); the population, timetable and people fill the map in different frames; far traffic light lamps update in turns.
- **Pedestrians no longer walk round and round a roundabout's pavement** or pile up there: they can turn into a path that meets theirs midway ([#2294](https://github.com/openOMSI-org/openOMSI/pull/2294), by @thibautgnt).
- **Cars behind the bus at a red light wait with it**; after 10 s they went round it on the oncoming lane ([#2293](https://github.com/openOMSI-org/openOMSI/pull/2293), by @thibautgnt).
- **Vanilla lighting no longer washes out** where lights overlap, **large textures no longer whiten** from their transparent pixels when they are scaled or compressed, and **invisible transparent faces no longer hide** what is behind them ([#2272](https://github.com/openOMSI-org/openOMSI/pull/2272), by @isaacsa2).
- **Enhanced clouds**: a deck's underside shows its heaps instead of one smooth grey, and the clouds no longer jump every few minutes.
- **Bus physics** take as many substeps as the frame's time needs instead of at least four a frame.

## 0.2.34 - 2026-10-11

### New and improved

- **The trip summary rates the trip**: five stars, a line on how the passengers felt, the trip at a glance (stops served, stops on time, distance, time driven, passengers) and gauges for driving, comfort and ticket selling above the table of stops, all in the game's language. It opens 5 seconds after the bus stops, so there is time to open the doors first ([#2263](https://github.com/openOMSI-org/openOMSI/pull/2263), by @thibautgnt).

### Fixes

- **Bus signals no longer freeze at the start of their cycle**: a traffic-light jump that lands where it started (Grand Paris-Moulon's TCSP17) is done once, not again every frame, so the bus signal reaches its next green ([#2265](https://github.com/openOMSI-org/openOMSI/pull/2265), by @YanisK33; [#2224](https://github.com/openOMSI-org/openOMSI/issues/2224), [#2266](https://github.com/openOMSI-org/openOMSI/issues/2266)).
- **A scripted bus signal never shows red and green together**: when its script lights both, it stays red ([#2264](https://github.com/openOMSI-org/openOMSI/pull/2264), by @YanisK33).
- **"Save the situation" can be continued**: the quicksave (Ctrl+S) is offered in the launcher's Continue on the map it was made on, beside the last situation and the slots; before, only a slot seemed to save ([#1746](https://github.com/openOMSI-org/openOMSI/issues/1746)).
- **Winter textures without snow as in OMSI**: a map's winter of kind 4 ("winter with snow") shows its winter pictures like kind 3; the snowy ones come only with snow in the weather - they showed under a clear sky ([#2131](https://github.com/openOMSI-org/openOMSI/issues/2131)).
- **Text textures with '@' line breaks are laid out as in OMSI**: the lines a font height apart and centred as a block, cut at both edges when it is taller than the texture; a text beginning with '@' (the New Lion's City's settings list) stood a line too low ([#1387](https://github.com/openOMSI-org/openOMSI/issues/1387)).
- **The map's depot file stays assigned** to a bus that lacks it when it carries the line chosen; a depot of the bus's own folder with a line of the same number was taken instead ([#1837](https://github.com/openOMSI-org/openOMSI/issues/1837)).
- **On foot without a bus, the navigator and the city map (Shift+M) work**, following the walker ([#2118](https://github.com/openOMSI-org/openOMSI/issues/2118)).

### Multiplayer

- **The launcher's start button says which multiplayer game it joins**: "Join Spandau 1" or "Join Thibaut's game" after joining a server or pasting a session code, instead of "Drive" ([#2262](https://github.com/openOMSI-org/openOMSI/pull/2262), by @thibautgnt).

## 0.2.33 - 2026-10-11

### New and improved

- **Sun blinds work as in OMSI**: one pushed up by hand drops back to where it was pulled down to once it is let go; only its cord rolls it up. Before, every drag moved the blind's catch with it and it stayed wherever it was dragged ([#2146](https://github.com/openOMSI-org/openOMSI/issues/2146)).

### Fixes

- **Each cab camera keeps its own zoom**, as in OMSI, instead of one zoom for every camera of the view ([#2183](https://github.com/openOMSI-org/openOMSI/issues/2183)).
- **Single railcars no longer crash the game**: a railcar with nothing coupled behind it (Thüringer Wald's AI trains) is put on its track without a panic ([#2259](https://github.com/openOMSI-org/openOMSI/issues/2259), [#2229](https://github.com/openOMSI-org/openOMSI/issues/2229)).
- **The clutch, the throttle and the brake on a controller button work**: a button or trigger set to a pedal works it as the pedal's key does; the clutch on a wheel button did nothing ([#2029](https://github.com/openOMSI-org/openOMSI/issues/2029)).
- **No more hitch every 5 seconds without a controller (Windows)**: with none plugged in, DirectInput is looked for again only in the first 30 seconds and then when Windows reports a new device, instead of every 5 seconds for the whole session ([#2213](https://github.com/openOMSI-org/openOMSI/issues/2213), [#2068](https://github.com/openOMSI-org/openOMSI/issues/2068), [#2159](https://github.com/openOMSI-org/openOMSI/issues/2159)).
- **Parked buses stay where they were left**: a placed bus keeps its place while its tile is not loaded, and one saved under the ground is put back on it; they fell through the world when the player drove off and were saved kilometres down ([#2132](https://github.com/openOMSI-org/openOMSI/issues/2132)).
- **Photo mode gives the time back**: a clock moved for a photo returns when photo mode is left, so the duty goes on at the time it stood at ([#2116](https://github.com/openOMSI-org/openOMSI/issues/2116)).
- **The free camera keeps the horizon level** when taken over from the driver's view of a bus leaning on a slope; it went on with the bus's roll ([#2220](https://github.com/openOMSI-org/openOMSI/issues/2220)).
- **The automated manual shifts past first gear** on buses whose script keeps the gear in a variable of its own (the gear up/down keys too): such a bus was seen in neutral after every shift and put into first again and again. The gate put in last is now taken as the gear when the script's variable does not follow it ([#1832](https://github.com/openOMSI-org/openOMSI/issues/1832); reports on the O305 Schaltwagen and the Citybus i280 are welcome).
- **Articulated buses' rear sections sit higher**: a rear section on one axle hangs the share of its weight ahead of that axle on the joint, which presses the front section down there, as OMSI's joined bodies do; its axle carried all of it and sat that much deeper on its springs (0.6 of the GN92 rear section's 7.2 t). Right after the start the GN92's rear section still stands lower until its level control has filled its bellows, as the stock script has it ([#1835](https://github.com/openOMSI-org/openOMSI/issues/1835)).
- **The NL/NG wheelchair lift and ramp work at petrol stations**: the scripts' ground sensor (`GetHeightAbovePoint`) measures down to the ground under the point, not to the highest face there - under a canopy that was its roof, so the lift would not go down and the ramp turned on its side and vanished ([#2205](https://github.com/openOMSI-org/openOMSI/issues/2205)).

### Multiplayer

- **Timetable buses vary from session to session**: a tour's bus is drawn from its depot with the session's seed, so a tour gets another of the depot's buses each time, as in OMSI (alike on every machine of a LAN game); the same bus ran each tour every time ([#2250](https://github.com/openOMSI-org/openOMSI/issues/2250)).

## 0.2.32 - 2026-10-11

### New and improved

- **Emergency vehicles**: the cars making way keep waiting 8 s (all of them 30 s) as before - let go after 3 s and 8 s, a car in front drove on while the other lane's traffic still came and the siren never got past it; the faster drive (1.5 times the limit, at most 30 km/h over it) stays, its test follows it
- **Emergency vehicles**: a car standing aside that the siren cannot get past drives on after 3 s (every one after 8 s, was 8 and 30: a van and an ambulance waited for each other for 40 s), and the drive itself faster - 1.5 times the limit up to 30 km/h over it, 8 m/s through a red light kept for it, harder pulling away and bends
- **Traffic**: brake lights with hysteresis and a minimum time lit, gentle braking eased in and bends taken at 1.2 m/s² from further back, AI buses steering into turns with a shorter look-ahead and a quicker wheel, and the AI horn heard (OMSI's own Sounds/AI_Cars horn where the car's sounds have nothing on ev_AI_Horn)
- **Updater**: the version an update replaced is kept until the new one has started well, the new program must pass --self-test before it replaces anything, and a version that broke here (or that the project lists in release/health.json) is never installed again
- **Scene tests**: the fixtures' folders carry a counter besides the clock - it ticks in microseconds on macOS, and two tests running side by side shared a folder now and then and failed
- **Size budget**: ui.rs's 350-line draw gets its own entry - it was covered by the navigator's draw under the same crate and name, whose entry went when that one was cut up
- **Statics that stay**: a line on why each is process-wide Comments only. The statics left after the game state moved into its owners are install-once hooks (the launcher's Host, the scene's settings reader), process resources (the translation worker, the settings file's write-behind, the LAN mods' content root, the phone's tilt sensor), read-only tables, rate limits, and the settings and weather the game sets again at every session start for loading code that has no game state at hand. Each now says which.
- **Settings window**: its page titles, kept for a moment, belong to the menu state, not to a thread-local The titles of the open settings window's pages are asked for every frame and building them is the work of all their rows, so the answer is kept for up to five seconds. It was kept in a thread_local!; it is now MenuState::page_titles (a RefCell, as page_titles reads the app by reference), forgotten by open_list as before.
- **OpenXR**: the cursor, the desktop window and the frame statistics out of render
- **Launcher controllers**: the device list and the force feedback rows drawn by functions of their own
- **Launcher timetable**: the tour's trips panel drawn by a function of its own
- **Sky**: the classic sky's cloud type is kept with the scene it was set up for, not in a static setup_sky picks the [cloudtype] entry the classic sky draws its cloud layer with, and weather_lighting reads it while the weather names that type. It was a process-wide static written by every setup_sky, so the window's session, the offscreen run and the launcher's showroom (one process on Android) overwrote each other's. It is now weather_setup::SkyClouds, kept by the owner of the scene - SessionState::sky_clouds, Offscreen::sky_clouds, the showroom's Shown - and handed to setup_sky, weather_lighting and picture_lighting. Every caller sets it up and reads it in the same order as before.
- **Script compat**: the Aachen IBox route pad patch in steps of their own
- **Humans**: Pose::bones' foot targets, swing pitch, arm target and hand as helpers, 437 -> 282 lines
- **Humans**: Pose::advance's self-contained steps as helpers of their own, 441 -> 209 lines
- **Offscreen**: the drive's extremes are kept by the run, not in a static The worst pitch and bank of an offscreen drive, the origin's highest and lowest point over the ground and where the worst pitch was are gathered every step and logged in the drive report. They were a process-wide static; they are now Offscreen::drive_extremes, starting from the same values, so the report line is the same.
- **Map tiles**: the spline and spline attachment records read by functions of their own
- **Model**: the material keywords of model.cfg parsed by a function of their own
- **Game menu**: the switches' match in two halves
- **Game menu**: the timetable pane beside the list drawn by a function of its own
- **Spline check**: the ends of the staged splines are kept by the world that staged them With OMSI_CHECK_SPLINES every staged spline's ends go into a table the offscreen road check reads to report chained ends that differ in height. The table was a process-wide static that only ever grew. It is now World::spline_ends, and the check is handed the world; its report is unchanged.
- **Rigid body**: step_slice's self-contained parts as helpers of their own, 496 -> 278 lines
- **Season mix**: the count of the second look's textures is kept by the world, not by the process A season's phase between two looks logs how many textures and object types the plants' other look adds. The two counters were process-wide statics, so a later drive in the same process went on counting from the earlier one's totals. They are now World::season_extra. Only the totals in those log lines can differ, and only where two worlds are opened in one process; what is loaded and drawn is unchanged.
- **Sound sets**: a recording counts a sound's seconds played by the sound it mixed, not the wall clock
- **Spawn**: the player's bus set up in steps of their own
- **Lights**: the fading spotlight cookies are kept by the world, not in a process-wide static A [spotlight_cookie] lamp with a fade time comes to its brightness over time, kept per vehicle (by its address), part and lamp. The table was a process-wide static: a vehicle of a later drive in the same process at the address of an earlier one started from that lamp's old brightness instead of its target. It is now World::cookie_fades, handed to lights::vehicle_lights by lights::collect; the fading itself is unchanged.
- **Navigator**: the panel's marks, top bar and bottom bar drawn by functions of their own

### Fixes

- 0.2.31 was never published: its Windows x64 build stopped at a check of the shaders that read Windows' line endings (not the game itself). 0.2.32 is 0.2.31 for every platform - see below for what it brings.
- 0.2.32: Windows x64 built again - 0.2.31's shader check compared the parts with Windows' line endings; everything of 0.2.31 for every platform
- 0.2.31: openOMSI heals a broken start by itself (a watcher before anything that could break: a newer release, else the version before, else safe mode), mirror panels every frame ([#2209](https://github.com/openOMSI-org/openOMSI/pull/2209)), smoother AI traffic and faster emergency vehicles, the trip summary waits for the bus to stop ([#2240](https://github.com/openOMSI-org/openOMSI/pull/2240)) and fits a phone, spline rows as Omsi.exe draws them ([#2249](https://github.com/openOMSI-org/openOMSI/pull/2249), [#2238](https://github.com/openOMSI-org/openOMSI/pull/2238)), model-only display variables ([#2217](https://github.com/openOMSI-org/openOMSI/pull/2217)), and the program split into crates with clear layers
- **Trip summary**: the card shrinks to fit a small screen - a phone - instead of spilling past its edges
- **Start guard**: its end-to-end proof in macOS' app bundles too, the start where nothing helps (safe mode once, then a message, no endless restarts), and its documents - what players see (USER_GUIDE), how a broken release is listed (VERSIONING), how it works (ARCHITECTURE)
- **Start guard**: the way back is dropped by the watcher, once it knows the project does not list the version as broken (a listed version goes back at the next start), and an update whose program fails its self-test is never offered again; scripts/guard-e2e.sh proves it end to end
- **Start guard**: before anything that could break, a launcher start gets a watcher without a window; a start that crashes or hangs before its first frames is healed on its own - a newer release installed, else the version before put back, else safe mode, else the player told what to do - in a message box that needs no graphics card
- **Refactor checks**: audio traces, so that a change that must not alter what is heard can show it did not
- **Settings window**: "a value really changed" is a flag of the menu state, not a static A slider dragged in the game's settings window sends the same value many times over; the list is built again only when option_do (or a drop-down, a graphics profile, the VR navigator's reset) really changed something. That flag was a process-wide static; it is now MenuState::list_dirty, cleared and read by App::list_adjust as before.
- **LAN**: the WebSocket's reconnections are counted with the connection, not in a static of their own ws_came_back tells the joining game that its WebSocket was made again (it says hello to the host once more). The count it compares with was a static that outlived the connection: the first frames of a later join in the same process (Android's launcher) saw the previous connection's count, took it for a reconnection and said hello twice. The count is now kept in the WsPath it belongs to, which goes with the session. WS_PATH itself stays process-wide and gets a line on why.

### Multiplayer

- **Weather model**: a test that a run steps as the session statics did The old start/stop/refresh, kept in the test as they were (statics turned into a local struct), and ModelRun go through two sessions side by side: frames of a sixtieth of a second and of a second, minutes over midnight, a jump of more than a day, one back, a new year, a map at another place and back, a stop. Weather and sky must agree bit for bit at every one of the 1800-odd steps (a changed step threshold in ModelRun makes it fail).
- **Architecture**: the global mutable state as it stands after the game state moved into its owners The "Known debt" bullet lists where the session state that lived in statics went, how many statics are left in omsi-app and omsi-scene with the grep that counts them, what kinds they are, and which ones are to go next (the settings and weather broadcast to loading code).
- **Weather model**: the physical model's run is kept by the session that runs on it, not in two statics omsi-weather kept the model of the `natural` weather in a static (SESSION) and the sky it shows in another (CURRENT, read by weather_lighting). Every load_weather started it again or stopped it - also the ones that only wanted to know the weather (the season of a map being opened on its own thread, a LAN host's season, the launcher's showroom) - and the window's session, the offscreen run and the showroom (one process on Android) all ran on the one model.
- **Photo mode**: its settings kept between photos live in the app, not in a thread-local The photo mode keeps its settings (lens, exposure, size, ...) from one photo to the next in a session. They were kept in a thread_local!, which outlived the session: the next drive in the same process (Android's launcher) started with the last one's settings. They are now App::photo_settings, starting empty with each App as the thread-local did with the process.

## 0.2.31 - 2026-10-11

### New and improved

- **openOMSI heals a broken start by itself.** A release that crashes or hangs before the launcher's window used to strand everyone - the updater lives in the launcher, so a launcher that never opened never updated (0.2.28 on Windows had to be downloaded again by hand). Now the program's very first act is a small watcher without a window; if the launcher crashes, ends on an error or has not drawn its first frames after two minutes, it is healed on its own: a newer release is installed (its program must start before it replaces anything), else the version before the last update - kept until the new one has started well - is put back, else it starts once more in safe mode (basic graphics, no plugins), else a message says what to do. Each step is said in a message box in the game's language. A version that failed is never installed again on that computer, and the project can list a broken release (`release/health.json`) so that it is replaced at the next start even when it starts. A crash in the middle of a drive changes nothing. `OMSI_NO_GUARD=1` switches it off (see the user guide, "When a version does not start").
- **Mirror panels every frame**: a switch in Settings → Graphics (off by default) draws the mirror panels in the cab (Ctrl+M) every frame, as ETS2 draws its screen mirrors, so the street no longer jerks past in them ([#2209](https://github.com/openOMSI-org/openOMSI/pull/2209), by @m3rlinek01).
- **Emergency vehicles on a drive are faster**: 1.5 times the limit, at most 30 km/h over it, pulling away harder and taking bends harder.

### Fixes

- **AI traffic drives more smoothly**: brake lights with hysteresis and a minimum time lit (no flicker), gentle braking eased in, bends taken at 1.2 m/s² and slowed for from further back, AI buses steer into turns with a shorter look-ahead and a quicker wheel, and the AI horn is heard (OMSI's own `Sounds/AI_Cars` horn where a car's sounds have none on `ev_AI_Horn`).
- **The trip summary waits until the bus stops**, so it no longer takes the mouse and the pedals while the bus pulls into the last stop ([#2240](https://github.com/openOMSI-org/openOMSI/pull/2240), by @Dasteee; [#2227](https://github.com/openOMSI-org/openOMSI/pull/2227), [#2236](https://github.com/openOMSI-org/openOMSI/pull/2236)), and **fits a phone's screen** instead of spilling past its edges.
- **Spline rows as Omsi.exe draws them**: the objects of a row that reach past their tile's edge are drawn again (the far side of the parked cars along a Berlin-Spandau street crossing a tile border; [#2249](https://github.com/openOMSI-org/openOMSI/pull/2249)), and a row whose first object lies before its own spline is not drawn, whatever its lateral offset ([#2238](https://github.com/openOMSI-org/openOMSI/pull/2238); both by @NACHN, checked against Omsi.exe).
- **Destination displays that only their model names a variable for show again** (the Enviro500's `DPIP_8words`): a variable a model's `[visible]` uses is registered even when no varlist declares it ([#2217](https://github.com/openOMSI-org/openOMSI/pull/2217), by @shamough1792).
- **A second drive in the same program** (Android starts one from the launcher, a lost graphics device restarts the renderer) no longer inherits the last one's state: corona and spotlight pictures were missing on the second renderer, a duty after one begun on the real time could not move the clock to its first trip, a map opened for a duty start could be taken over with its old state, the map lights and fading lamps of the previous map, the photo mode's settings, passengers' lines and a LAN `/admin` password carried over, and a LAN rejoin said hello twice.
- **With mirrors on, the window's draw lists are reused again** from frame to frame (a mirror pass threw them away every frame).
- **Offscreen recordings (`OMSI_RECORD`)** fade sounds in by the film's time, not the wall clock, so a recording comes out the same every time.

### Multiplayer

- **Under the hood**: the program is split into 33 parts with clear layers - the launcher, input devices, VR, LAN play, platform services, weather, career, soundscape, the world, the scene, photo mode and the HTML displays each in their own crate, the renderer's 10,000-line core cut up by concern, the vehicle and the script host by subsystem, the frame as one declared list of stages for the window and the offscreen run. What is drawn, heard and simulated is unchanged (checked picture by picture, sound by sound and with every test); a check on every pull request keeps the layers apart.

## 0.2.30 - 2026-10-11

### Fixes

- **Clouds: an overcast sky is a dark deck with its own heaps in it** instead of a flat pale sheet. A few fair-weather clouds keep the light bases they had; as the cover rises past about 55 % their undersides go darker, towards the grey of a storm, and a shut sky or rain is drawn by the deck itself (Enhanced; [#2198](https://github.com/openOMSI-org/openOMSI/pull/2198), by @NACHN).
- **All platforms again**: 0.2.29 came out for Windows only; 0.2.30 is built for Windows, macOS and Linux (x64 and ARM64), Android (arm64) and the dedicated servers.
- **Spline rows no longer draw objects Omsi.exe drops**: a row's objects that fall outside their own tile, and a row written along the carriageway that starts before its chain - on Grundorf Island 4's interchange, bridge piers no longer stand in the road. Rows of `[absheight]` objects stay wherever they fall, as single objects do ([#2194](https://github.com/openOMSI-org/openOMSI/pull/2194), by @KYChung1287; [#787](https://github.com/openOMSI-org/openOMSI/pull/787)).
- **Objects a map tags as a surface stay behind what stands beside them at a distance**: the pull towards the eye that keeps road markings over the road stops growing at 5 cm, so a traffic light's pole no longer draws across its signal head from tens of metres away; markings, crossings and kerbs keep their depth near the camera ([#2197](https://github.com/openOMSI-org/openOMSI/pull/2197), by @NACHN; [#1694](https://github.com/openOMSI-org/openOMSI/pull/1694), [#1196](https://github.com/openOMSI-org/openOMSI/pull/1196)).

## 0.2.29 - 2026-10-10

### Fixes

- **Windows: the launcher opens again.** In 0.2.28 its window stayed hidden until its first picture, and Windows never asks a hidden window to draw, so the launcher never appeared. If you installed 0.2.28 on Windows, download 0.2.29 by hand: that launcher cannot update itself.
- **Release**: a manual run can build and release one platform alone, for a fix only that platform needs
- 0.2.29: the launcher opens on Windows again (its window, hidden until its first picture, is drawn from the event loop: Windows asks no hidden window to paint); the release workflow's one-platform rebuild taken out again, since a published release keeps its files
- **Release**: a manual run can build one platform again into the release of the same version, the others keeping their files

## 0.2.28 - 2026-10-10

### New and improved

- **AI traffic reworked.**
  - It runs in fixed steps of 20 ms whatever the frame rate, and the cars are drawn between the steps, so they glide at 144 Hz as at 30 and decide the same at any frame rate. Each session's traffic differs (OMSI_SEED=n repeats one).
  - Junctions: a car keeps the way it took until its tail is through, room on an exit is promised to one car at a time, nobody follows a standing queue into a junction, a left turn at a signal pulls in to wait for the oncoming traffic, a priority sign beats an early claim from the side road, roundabouts give the ring the right of way where the map sets none, and `[crossingproblem]` works as in OMSI. Cars waiting for each other in a circle are found and let go safely instead of by timers.
  - Lane changes: one decision per car with the whole move checked against cars, people and the scenery, one car into a lane at a time, no back-and-forth, cars leave a lane that turns into a bus lane, and passing on the other half of the road gives up if somebody comes.
  - Bodies and braking: a separate emergency braking channel, the car's plan follows where its body really is, and cars stop before the scenery's walls and steer round corners instead of driving into them.
  - Bus stops: berths taken in turn, docking within tolerances, missed stops recorded, the doors close with the script's answer, and a bus serves a stop from behind one on its layover.
  - Fewer emergency brakes, close calls and stuck cars than before; random cars give up only after 200 s standing, as in OMSI.
  - **Emergency vehicles** (a car whose script sets `TrafficPriority`, the player's bus or a LAN player's too): the cars on its way pull over or open a corridor, it gets the junction and crosses a red light at walking pace, never through cars, people or a full exit. `TrafficPriorityWarningNeeded` follows OMSI's rule.
  - **AI cars honk** when somebody stops sharply in front of them or blocks them for long, and sometimes get honked back (`ev_AI_Horn`).
- **Developer tools** (Ctrl+Shift+D): watch, set and hold a vehicle's script variables, overlays for paths, seats, boxes, axles, lights, mirrors and views, a mirror editor, and hot reload of `.bus`, paths, `sound.cfg`, `model.cfg` and constfiles.
- **Quick menu** on a tap of Alt, as in OMSI 2 (12 tiles, the game keeps running).
- **Trip summary** at the end of a trip: every stop with the times planned and driven and the odometer, to copy or save (Settings → Gameplay → *Trip summary*).
- **View editor** (Options → Camera → *Edit the view and mirrors*): eye position, direction, field of view and each mirror, saved for the vehicle.
- **Destination form**: a route number and two lines of free text for script-drawn displays.
- **F4 map**: a click asks whether to move the vehicle there, anywhere on the map.
- **Bus radio** sounds from the cab, with optional small-speaker, FM hiss and dropout effects (Settings → Sound), and the scripts can read the station and the song (`GetRadioName`, `GetRadioSong`).
- **Headlamp shadows**: the bus's own headlights cast shadows (Enhanced).
- **`[spotlight_cookie]`**: a vehicle lamp can carry a picture of its beam; **`[led_glow_effect]`**: a model sets its LED glow per material. Both are documented in FORMATS.md and MODDING.md.
- **Configurable door buttons on phones** ([#2134](https://github.com/openOMSI-org/openOMSI/pull/2134), by @isaacsa2).
- **Faster starts.** The game's window shows its loading screen at once and stays responsive while the shaders are built; the launcher draws its page from the first frame and loads the 3D preview after it.
- **Less work per frame**: the script engine, the AI cars' sounds and materials, the timetable paper, departure boards, scenery scripts, the light grid, ray tracing (Enhanced+, about half the CPU time), render bundles and texture lookups; the settings file is read once.
- **Smooth view changes as in OMSI**, and the steering follow only in the standard driver's view.
- **`Velocity` is the driven wheels' speed** (wheelspin and locked wheels show in it), `Velocity_Ground` the body's, as in OMSI.
- **Clouds**: a deck's underside keeps the shape of the heaps it is made of ([#2171](https://github.com/openOMSI-org/openOMSI/pull/2171), by @NACHN).
- **Integrated GPUs get the texture memory their driver reports** ([#2115](https://github.com/openOMSI-org/openOMSI/pull/2115), by @anonchan958).
- **Outside sounds are muffled in the closed cab**, and the cab and street sounds crossfade as you walk in or out.
- **Rail vehicles follow tracks and grades more precisely** ([#2122](https://github.com/openOMSI-org/openOMSI/pull/2122), by @Maarceeli).
- **Text fields** in the launcher: selection, word-wise moves, copy, cut and paste.

### Fixes

- **Choosing a duty no longer types into the IBIS or the roller blind**, as in OMSI 2 (Shift+U and `--autostart` still do). `GetTTDelay` is whole seconds.
- **Low sun**: rough roads no longer glare; Enhanced paint reads its reflection map as OMSI does, and windows with one show the lamp-lit street at night.
- **Photo mode: a steering wheel no longer turns the camera.** While it is open the wheel, the pedals and a joystick do nothing; its camera listens to a gamepad only.
- **Scenery drawn as a surface layer keeps its cut-out picture** instead of being eaten by the road under it, the parked buses of the Hong Kong maps for one ([#2142](https://github.com/openOMSI-org/openOMSI/pull/2142), by @KYChung1287).
- **Windows: no white rectangle at a session's start.** The window is shown only once it has a picture.
- **Android: the launcher starts much faster** and no longer shows the wallpaper before it; a start cut short still saves the shader cache.
- **DirectX 11 (ANGLE): the launcher draws again** instead of a black window ([#2140](https://github.com/openOMSI-org/openOMSI/pull/2140), by @isaacsa2).
- **Crossed-card trees face the way OMSI turns them.**
- **LED matrices with `[matl_alpha]` after `[matl_change]`** no longer show every dot lit.
- **Clear glass**: no grey veil from lamps, and displays behind it no longer ghost ([#2161](https://github.com/openOMSI-org/openOMSI/pull/2161), by @SpicySpaceman).
- **Instrument backlights** are readable at night with the lights on ([#2127](https://github.com/openOMSI-org/openOMSI/pull/2127), by @shamough1792).
- **Bus stop displays show the timetable's destination** ([#2125](https://github.com/openOMSI-org/openOMSI/pull/2125), by @SpicySpaceman).
- **Tunnel entrances** are no longer walled off by stretched terrain ([#2130](https://github.com/openOMSI-org/openOMSI/pull/2130), by @juanjopuntcat; [#1472](https://github.com/openOMSI-org/openOMSI/pull/1472)).
- **A mod's lowercase `texture` folder no longer hides the original's textures** on Linux ([#2119](https://github.com/openOMSI-org/openOMSI/pull/2119), by @artemix1508).
- **Linux force feedback for wheels without a gilrs mapping** ([#2168](https://github.com/openOMSI-org/openOMSI/pull/2168), by @YanisK33).
- **Android: the pause menu scrolls by touch again** ([#2139](https://github.com/openOMSI-org/openOMSI/pull/2139), by @isaacsa2; [#2108](https://github.com/openOMSI-org/openOMSI/pull/2108)).
- **`[illumination_interior]` lists** ignore -1, out-of-range and repeated lamps.
- **Wheel spray** no longer shows inside the cab.
- **The "playing now" service and the PR build comments are hardened against bad input** ([#2151](https://github.com/openOMSI-org/openOMSI/pull/2151), by @isaacsa2).

## 0.2.27 - 2026-10-10

### New and improved

- **Ambience from real recordings** (Settings → Sound → *Ambience*, on by default, with its own volume).
  - The place you drive through: town, courtyard, suburb and village, the motorway and the railway in the distance, tunnels.
  - Nature by season and hour: the dawn chorus, woods, fields, crickets, water and frogs, and 22 species of birds.
  - The weather: rain by how hard it falls and what it falls on, drips after it, snow and blizzards, wind in leaves, conifers, bare branches and wires, thunder over a warm shower (none in a snowfall).
  - The life of the town: sirens, aircraft, doors, the bin lorry, lawnmowers, and fireworks from five to twelve on New Year's Eve.
  - Churches, schools, shops, stations, farms and other places the map's authors marked as such, each at its own hours. OMSI 2's own sound objects keep their sounds.
  - The recordings (about 300 MB, all CC0, CC BY or CC BY-SA, credited in their `CREDITS.md`) are downloaded once by themselves the first time the launcher or the game starts with the ambience on, into `~/.openomsi/ambience`. They play from the next drive on. Not on Android yet.
- **The Mods page shows what each mod holds.** The arrow on a mod's row opens its contents folder by folder (Vehicles, Fonts, Texture, ...), and the row lists how many items each folder has.

### Fixes

- **Poles, signs, street lamps, traffic lights and fences no longer sway in the wind.** OMSI's street objects are grouped as "German Street Side", and the word "street" was read as holding "tree". They also no longer take the plants' autumn and winter looks.
- **AI traffic no longer jams at junctions without traffic lights.** A car standing third in a queue took the right of way as if it waited at the line, the car across the junction waited for it while the queue waited for that car, and nothing moved until a car was taken away after 200 s (Spandau).
- **A mod's fonts and textures belong to it.** What a mod puts into a folder all mods share (a pack's fonts in Fonts, its textures in Texture) is now noted as its own, so it shows in the Mods page and goes when the mod is switched off or deleted. Fonts and textures put there by hand are listed together, one entry per folder.
- **The "playing now" count is the same on the website and in the README.** The count is taken once every ten minutes, on the clock, and both show that one. The website picks up the next count as soon as it is there, and the README badge within five minutes more because of GitHub's cache.

## 0.2.26 - 2026-10-10

### New and improved

- **Photo mode** (pause menu → *Photo mode*, Ctrl+F12 or the `photo_mode` key). It replaces the menu's Screenshot.
  - The game stops and a camera of its own flies freely or orbits the bus (WASD, Q/E, mouse or right stick to look, wheel to zoom, Z/X to roll).
  - A real lens: focal length from 10 to 400 mm, depth of field from the aperture and the focus (on the bus or set by hand), round or bladed bokeh.
  - A rolling shot: the shutter blurs the street and turns the wheels while the bus stays sharp.
  - A darkroom (exposure, contrast, highlights, shadows, saturation, temperature, tint), film looks, grain, vignette, frames of any aspect ratio with composition guides, and the clock moved for another light.
  - Photos are saved as PNG into `Screenshots`, at the window's size or double. The note that says where it went opens the folder with the photo selected when clicked (Finder, Explorer, the file manager). On Android the photo goes straight into the gallery.
- **The pause menu is drawn like the launcher**: its rail with Resume first, and the settings and lists as launcher pages with the same fields, switches and sliders. The keyboard works it as before.
- **Procedural passenger animation as an option** ([#2101](https://github.com/openOMSI-org/openOMSI/pull/2101), by @isaacsa2). Settings → Gameplay → Passengers → *Passenger animations*: *Procedural (experimental)* plants the feet on the floor and the steps, seats people on their seats and lets standing riders hold the rails. *Original OMSI* stays the default.
- **Clouds cover the sky by a continuous amount** ([#1360](https://github.com/openOMSI-org/openOMSI/pull/1360), by @NACHN). A cloud cover slider in the launcher, the phone layout and the game's weather page, carried smoothly across weather changes.
- **Plain text in the interface**: separators are `|`, dashes `-` and ellipses `...`. The photo mode's line of keys and the pause menu's key hint are gone.
- **Passengers take a free seat first** and stand only by chance (Settings → Gameplay → *Standing passengers*, 5 % by default) or once the seats are full ([#2063](https://github.com/openOMSI-org/openOMSI/pull/2063), by @SuSeu1414).
- **Logitech G923 force feedback** pushes the right way, and a gentle centring spring holds the wheel while parked ([#2065](https://github.com/openOMSI-org/openOMSI/pull/2065), by @atzock).
- **The destination menu shows the HOF's names**, as OMSI's own list does ([#2081](https://github.com/openOMSI-org/openOMSI/pull/2081), by @SpicySpaceman).

### Fixes

- **Looking round with the mouse is smooth.** While the right (or middle) button is held, the cursor stays where it is and shows OMSI's four arrows, and the view turns by the mouse's own movement. It no longer stops at the edge of the screen, jumps when the button comes up, or loses the steering cross with mouse steering on. The photo mode's camera turns the same way.
- **Less GPU work on slow machines**: the launcher no longer redraws an unchanged picture while idle ([#2097](https://github.com/openOMSI-org/openOMSI/pull/2097)), and the navigator is drawn every other frame at most where frames are slow ([#2096](https://github.com/openOMSI-org/openOMSI/pull/2096), both by @isaacsa2).
- **DirectX 11 (ANGLE) draws again** ([#2095](https://github.com/openOMSI-org/openOMSI/pull/2095), by @isaacsa2). Shadows without `textureGather` on OpenGL ES 3.0 class devices, the pipelines compiled on a thread of their own, colours no longer too dark, and the card's memory read from DXGI.
- **Snowfall on DirectX 12.** The snowfall, the lamps in the fog and the street lamps' shadows are now each made on their own: a driver that fails on one of them leaves out only that one (one of them took the snowfall off DirectX 12). A reduced set remembered for a graphics card is tried again with every new version, so snow that went missing comes back.
- **Bus reflections follow the light.** The `[matl_envmap]` reflection photo is lit by the light that is there now, so at night, at dusk and under a closed sky the bus no longer mirrors a sunny day (Vanilla, Vanilla+).
- **Photo mode no longer flashes black**, and the photo is shown only once it is of the camera as it stands.
- **Passengers who got off where there is no pavement walk on** from the nearest pavement instead of standing still for good ([#2102](https://github.com/openOMSI-org/openOMSI/pull/2102), by @isaacsa2).
- **Timetable buses no longer jump route gaps** or block the stops in between ([#1995](https://github.com/openOMSI-org/openOMSI/pull/1995), by @pcy200409).
- **Street lamps with a built-in light no longer shadow themselves** ([#2050](https://github.com/openOMSI-org/openOMSI/pull/2050), by @SpicySpaceman).
- **LED destination displays are bright again** ([#2045](https://github.com/openOMSI-org/openOMSI/pull/2045), by @SpicySpaceman).
- **A destination chosen while the display is still changing** waits for it, and the newest choice wins ([#2071](https://github.com/openOMSI-org/openOMSI/pull/2071), by @SpicySpaceman).

## 0.2.25 - 2026-10-09

### Fixes

- **Snow chosen while driving now covers the ground in Vanilla and Vanilla+.**
  - Switching to a snowy weather mid-game already brought the falling snow and the winter textures on the roads, objects and trees.
  - The plain ground kept its summer grass, because the material all tiles share was only made once. It is now made again for the new season's textures.
  - Enhanced hid this under its own snow cover.
- **Destination displays no longer show every variant at once.**
  - Some buses stack several versions of a sign (other fonts or letter styles) as separate meshes and switch them by a variable they never declare.
  - Omsi.exe creates such a variable at 0 when it reads the model, so only the variant for 0 shows. openOMSI used to leave all of them visible, faded over the correct one.
  - Undeclared `[visible]` and `[alphascale]` variables now read 0 on vehicles and scenery objects, and `Colorscheme` is always available to a vehicle's model.
- **AI cars no longer back up at junctions made of very short road pieces** ([#2041](https://github.com/openOMSI-org/openOMSI/pull/2041), by @SpicySpaceman). Two short paths linked both ways could send a car back onto the one it had just left. The cars now never choose that return. Seen on TH_Wald and Hamburg109.

## 0.2.24 - 2026-10-09

### New and improved

- **Rain quality setting.** Rain is expensive to draw, so Graphics → *Rain quality* (in the launcher and in the game) now sets how much of it is drawn.
  - **High** (the default) is as before.
  - **Medium** paints the drops on the glass 15 times a second instead of 30, keeps only your own bus's spray and draws half the falling streaks.
  - **Low** shows OMSI 2's own texture rain on the glass (from the next bus you load), so there are no drops to simulate and no extra copy of the picture. It throws no spray and draws a quarter of the streaks.
- **Trams** ([#1793](https://github.com/openOMSI-org/openOMSI/pull/1793), by @Maarceeli).
  - Multi-car trams and trains follow the track on both bogies, and each car of a consist hangs where it should.
  - A tram is put on the track running its own way, not the one beside it.
  - Driving against the track's direction no longer flips the vehicle round.
  - Rail vehicles' shadows lie flat.
  - Drivable rail vehicles with a front coupler appear in the vehicle list.
  - Consists that come back round to their first part (the eleven-part NF6D) are loaded whole.
- **Clickable scenery objects** ([#1793](https://github.com/openOMSI-org/openOMSI/pull/1793)). Switches and buttons on scenery objects (`[mouseevent]`) can be clicked, dragged and turned with the wheel.
- **More for html pages** ([#1992](https://github.com/openOMSI-org/openOMSI/pull/1992), by @pcy200409).
  - Departures now say how many stops away each bus is, how full it is, how late it runs and whether it is the last trip of the day.
  - Pages can read the weather and the depot's destination strings.
- **Korean, Chinese and Japanese text on html pages** ([#1991](https://github.com/openOMSI-org/openOMSI/pull/1991), by @pcy200409). Characters the built-in font lacks are drawn with Malgun Gothic on Windows or AppleGothic on macOS, or with any font set in `OMSI_HTML_FALLBACK_FONT`.
- **The ambience is removed.** The synthesised wind, leaves, raindrops, thunder, birds, crickets, town hum and tyre sounds added in 0.2.21 are gone, together with their settings. OMSI 2's own sounds play as they always did, and the bus scripts still learn what each wheel rolls on (`Axle_SurfaceID`).

### Fixes

- **Objects placed along a spline** stand where the map puts them, without gaps or overlaps where a row crosses into the next tile ([#2032](https://github.com/openOMSI-org/openOMSI/pull/2032), by @KYChung1287). This covers bridge piers, street lamps, catenary masts, fences and road markings.
- **Scenery shows its own textures** ([#2006](https://github.com/openOMSI-org/openOMSI/pull/2006), by @KYChung1287). A texture missing from an object's folders is no longer borrowed from another folder (a gantry wearing the pack's stock sign, a plate wearing asphalt). Fully opaque sign pictures no longer fade into the background.
- **Steam overlay** opens with Shift+Tab, because Steam now starts before the graphics and the window ([#2001](https://github.com/openOMSI-org/openOMSI/pull/2001), by @shamough1792).
- **The French menu title** is plain "openOMSI" ([#2024](https://github.com/openOMSI-org/openOMSI/pull/2024), by @Showtc).

## 0.2.23 - 2026-10-09

### New and improved

- **The Mods page of 0.2.22 lives on in it.** It still lists every installed mod and every item found in the content folder, with its kind, size and install date. You can search, filter by buses, maps, archives, other and switched-off mods, switch each mod off and on, and delete it after a confirmation. The list follows installs as they finish.

### Fixes

- **The previous launcher is back.** The launcher rebuilt in 0.2.22 is withdrawn; openOMSI opens the launcher of 0.2.21 again, with every page, the phone layout and the settings as they were (the ambience's switch and volume stay under Settings → Sound).
- **The ambience no longer crackles.** Raindrops on the ground, drips from the trees after rain and drops into puddles were single sharp clicks that hit the layer's limiter, so the ambience sounded like eating crisps. Each drop is now a soft tick, and the limiter recovers in 80 ms instead of 0.4 s, so it no longer ducks the whole layer after each one.
- **Leaves rustle instead of crunching.** A tree's leaves knock against each other thousands of times a second, but there were far too few, too loud clicks. Autumn's dry leaves (from late September) made a crisp-packet crackle. The rustle is now dense and soft, and still grows with the wind and the trees around.
- **Ending the launcher while a bus's preview is being placed** no longer leaves that bus out of the previews from then on.

## 0.2.22 - 2026-10-09

### New and improved

- **A new launcher.** Every page is built anew on `egui_retained`, the retained-mode interface of the org's egui fork, in the look of the Development Tools: a header, a sidebar, cards and a status bar, dark or light. The Drive page has three steps (the bus, the day and the weather, the map and the duty) with the bus turning on its stage and the map with its stops and times. Settings come as six tabs, and the Controls page holds the keyboard bindings with their action picker, the game controllers with live axes, the set-up assistant and the force-feedback test. Multiplayer, Profile, Sessions, Tutorials, Timetable and Setup are rebuilt too, and so are the update, crash, disconnect and reset dialogs. On a phone or in a narrow window it has a tab bar, a Play screen and full-screen sheets; lists scroll with a finger and a pinch zooms the bus.
- **The Mods page lists every mod.** Each one can be switched off and on, or deleted after a question; search and filters cover buses, maps, archives and switched-off mods. Each install is noted with the folders it made, and anything in the content folder without a note is listed by itself. Installing an archive or a folder, dropping it on the window and following the installs stay on the page.
- **Ambience.** Wind and gusts, rustling leaves, rain drops on the ground, in puddles and on the glass, thunder, birds, crickets and the town's far hum are synthesised live from the weather, the time, the season and the place. The tyres sound by the surface under each wheel (asphalt, concrete, cobbles, gravel, dirt and mud, grass, snow, wet road, puddles), and the trees drip after the rain. It is heard muffled through the bodywork inside a bus. Switch it off and set its volume under Settings → Sound.
- **Plugins.**
  - **Lua API.** One API registry serves every plugin language, with 271 functions and 59 events: the bus, the AI traffic and the people; the duty, the timetable and the map; time, weather, camera, input and sound; the game and the LAN.
  - **Plugin panels.** Panels get checkboxes, sliders, text fields, tabs, charts, tables, pictures and dragging.
  - **WebAssembly plugins.** They run sandboxed, with fuel and memory limits.
  - **Compiled `.oop` plugins.** They are encrypted, signed and sandboxed, and are made with the new [openOMSI Development Tools](https://github.com/openOMSI-org/openOMSI-Development-Tools) (the plugin workbench with the API documentation and the compiler, for Windows, Linux and macOS).
  - **Docs.** The plugin docs cover every API group, with three complete example plugins.
- **AI traffic jams far less.**
  - No car drives into a junction it cannot leave.
  - Rings of cars waiting on each other are broken.
  - A driver who has waited long at a busy main road gets across.
  - Cars take turns where two lanes become one.
  - Lane changes look out for the player's and the LAN players' buses.
  - Cars that gave up count towards the population, so it no longer grows until the town locks.
  - In open country, where nothing hides a car, the traffic no longer dies out. On Spandau with 120 cars, the cars standing for over a minute fell from 41-77 to 10-21.
- **Vanilla and Vanilla+ sky as in Omsi.exe.** The weather's own cloud texture lies on its cloud cone, the haze comes from the fog range and the cloud height, and the dome's pictures blend by the sun's height.

### Fixes

- **Sun shadows.**
  - A tent filter reads every texel under it, so a lit gap no longer shows as an X of magnified texels ("Minecraft" squares) and the edges stop shimmering.
  - The cascades snap to their own map's texels, so the shadows round the bus no longer tremble while it moves.
  - Enhanced+ soft shadow edges no longer crawl with a checker pattern.
- **Sky**: the clouds keep their outline from one mip level to the next; something white no longer vanishes at the line where the sky goes over to a smaller level.
- **Enhanced, wet roads**: the headlamps are reflected by the road's film of water - a short gleam on the asphalt and their image in puddles - instead of an endless white line down to the camera; drops in the bus's own beams are no longer a hundred times too bright.
- **Double-deckers**: the player walks the upper deck instead of standing stuck at the top of the stairs.
- **LAN**: a player who gets out of the bus keeps the figure they drove in instead of a random passenger's.
- **Faster on the CPU**:
  - The vehicle lights take no lists, keys or locks per lamp per frame.
  - The smoke is sorted by its keys, with the light grid and the sorting lists kept between frames.
  - Shadow casters ask which maps they fall into before their materials.
  - The scenery scripts find their emitters' objects in one pass.
- **The launcher**: a bus's 3D preview is no longer left out for good when the launcher was closed while the preview loaded.
- **Apple M4**: the scene shaders build again, so 4x MSAA, the snowfall, the lamps in the fog and the street lamps' shadows are back.
- **Links**: the project moved to [github.com/openOMSI-org](https://github.com/openOMSI-org/openOMSI) and [openomsi.org](https://openomsi.org/).

## 0.2.21 - 2026-10-08

### New and improved

- **Mouse steering**: a switch to leave the cursor free while the mouse steers - the system's crosshair without the frame's delay, the window's edges as the lock, as in OMSI [#1948](https://github.com/openOMSI-org/openOMSI/issues/1948).
- **Pipeline cache**: the graphics driver's compiled pipelines are kept between starts on Vulkan and OpenGL, so the game starts faster from the second run on.
- The game opens on the screen the launcher stands on, fullscreen too, instead of always on the main screen [#1959](https://github.com/openOMSI-org/openOMSI/issues/1959).

### Fixes

- **Timetable**: school runs run on school days again - bit 8 of a tour's days is the school days and bit 9 the school holidays, as in Omsi.exe; they ran only in the holidays [#1883](https://github.com/openOMSI-org/openOMSI/issues/1883).
- **Timetable**: the delay shown on the way between two stops follows where the bus is, as OMSI's IBIS shows it, instead of standing still until the next stop [#1898](https://github.com/openOMSI-org/openOMSI/issues/1898), [#735](https://github.com/openOMSI-org/openOMSI/issues/735).
- **Sound**: an entry with a `[volume]` over 1 plays at its full volume from its first moment, no longer held at what its curve gave it while it was silent [#1851](https://github.com/openOMSI-org/openOMSI/issues/1851), [#611](https://github.com/openOMSI-org/openOMSI/issues/611), [#1268](https://github.com/openOMSI-org/openOMSI/issues/1268), [#1311](https://github.com/openOMSI-org/openOMSI/issues/1311), [#1515](https://github.com/openOMSI-org/openOMSI/issues/1515), [#1900](https://github.com/openOMSI-org/openOMSI/issues/1900), [#1418](https://github.com/openOMSI-org/openOMSI/issues/1418).
- **Sound**: riding in an AI bus on foot it is heard from the inside, and in another player's bus the outside comes in through what that bus has open [#1286](https://github.com/openOMSI-org/openOMSI/issues/1286), [#1759](https://github.com/openOMSI-org/openOMSI/issues/1759).
- **AI buses**: passengers at the doors hold a bus at most a minute past its departure; somebody standing at a door the bus never opens no longer keeps it at the stop for good [#1801](https://github.com/openOMSI-org/openOMSI/issues/1801), [#1697](https://github.com/openOMSI-org/openOMSI/issues/1697), [#1544](https://github.com/openOMSI-org/openOMSI/issues/1544).
- **Automated manual gearbox**: it shifts up below the speed the governor holds the engine at, so a low-revving diesel no longer stays in first gear [#1832](https://github.com/openOMSI-org/openOMSI/issues/1832).
- **Enhanced+ reflections**: no hard seam where the reflected street leaves the screen, leaves seen through their gaps without specks, and a smooth pane's rays evened out with their neighbours' [#1907](https://github.com/openOMSI-org/openOMSI/issues/1907), [#1720](https://github.com/openOMSI-org/openOMSI/issues/1720), [#1795](https://github.com/openOMSI-org/openOMSI/issues/1795).
- **Weather**: snow and wetness stay out of the rear section of the player's articulated bus as well as the front [#1967](https://github.com/openOMSI-org/openOMSI/issues/1967).
- **Rain on the glass**: the washer wets only the panes a wiper blade reaches, not every window of the bus [#1884](https://github.com/openOMSI-org/openOMSI/issues/1884).
- **Situations**: the bus's script textures (destination displays, IBIS, ticket printer) are saved with a situation and come back when it is loaded, as in OMSI; they came back empty [#1559](https://github.com/openOMSI-org/openOMSI/issues/1559).
- **Scripts**: an instruction that reaches the wrong dispatch is logged and skipped instead of stopping the game [#1812](https://github.com/openOMSI-org/openOMSI/issues/1812).
- **Mouse steering**: a pointer the window cannot move back (a graphics tablet's pen) steers by where it stands instead of locking the wheel at full lock [#1945](https://github.com/openOMSI-org/openOMSI/issues/1945).
- **Controllers**: a latching switch stays in while the pause menu is open or the window is in the background [#1876](https://github.com/openOMSI-org/openOMSI/issues/1876); on Linux a device with only `BTN_TRIGGER_HAPPY` buttons counts them from 1, without sixteen buttons it does not have [#1879](https://github.com/openOMSI-org/openOMSI/issues/1879).
- **Mirror panels** (Ctrl+M) are drawn under the notes and the pause menu, not over them [#1880](https://github.com/openOMSI-org/openOMSI/issues/1880).
- **Launcher**: a tour's row names its first and last trip with passengers, not the depot runs it starts and ends with [#1891](https://github.com/openOMSI-org/openOMSI/issues/1891).
- **Android**: the folder browser finds an SD card from the mounted volumes (since Android 11 `/storage` itself cannot be listed) [#1306](https://github.com/openOMSI-org/openOMSI/issues/1306).
- **Renderer**: a device that cannot bind 4-byte storage arrays reads the scene's arrays from textures, so the Xclipse/ANGLE shadow pipeline works [#1857](https://github.com/openOMSI-org/openOMSI/issues/1857).
- **Graphics drivers** (wgpu fork): backports from wgpu 30 - the Vulkan acquire fence, out-of-memory checks, DX12 `textureNumLevels`, 3D textures and naga loop variables; a discarded surface no longer crashes the game, and shader errors are logged on one line.
- **Apple M4 / Metal**: the scene shader is back to the 0.2.20 one, whose bigger version the Metal compiler could not build.

## 0.2.20 - 2026-10-08

### New and improved

- **Gamepads**: ready-made Xbox, PS4 and PS5 profiles with default buttons, a Gamepad settings page (stick dead zone, steering speed), and the Menu/Options button opens the main menu [#1939](https://github.com/openOMSI-org/openOMSI/pull/1939); a controller action to open and close the main menu [#1594](https://github.com/openOMSI-org/openOMSI/pull/1594).
- **Key bindings**: "Add binding" on the Controls page lists every action - the installed buses' own script triggers too, with how many buses use each - searchable [#1437](https://github.com/openOMSI-org/openOMSI/pull/1437).
- **Platform screen doors**: light paths marked as a turn work as detectors for that indicator, for every vehicle, so BRT platform doors open for the bus that stops there [#1889](https://github.com/openOMSI-org/openOMSI/pull/1889).
- **HTML LED destination signs** glow at night like the LED panels, with the same LED glow setting [#1905](https://github.com/openOMSI-org/openOMSI/pull/1905).
- **Lua plugins**: a `service` event (refuel, wash, repair, reset, teleport) [#1882](https://github.com/openOMSI-org/openOMSI/pull/1882), and `trip_done` with the trip's ratings, `jolt` and `ticket_sold` events [#1937](https://github.com/openOMSI-org/openOMSI/pull/1937).
- `telemetry.json` for outside tools (the player's bus, the duty and its stops, the AI buses): written only while the file exists, see `docs/PLUGINS.md` [#1909](https://github.com/openOMSI-org/openOMSI/pull/1909).
- **Translations**: Dutch completed [#1944](https://github.com/openOMSI-org/openOMSI/pull/1944), German reviewed [#1936](https://github.com/openOMSI-org/openOMSI/pull/1936), Chinese [#1587](https://github.com/openOMSI-org/openOMSI/pull/1587), French [#1713](https://github.com/openOMSI-org/openOMSI/pull/1713), the game menu in every language [#1540](https://github.com/openOMSI-org/openOMSI/pull/1540), the stock weather names [#1598](https://github.com/openOMSI-org/openOMSI/pull/1598); messages with a number or a name in them are translated too [#1724](https://github.com/openOMSI-org/openOMSI/pull/1724).

### Fixes

- **Dedicated server and LAN host**: AI cars no longer appear or vanish in plain view of the other players [#1893](https://github.com/openOMSI-org/openOMSI/pull/1893).
- **Enhanced at night**: no more black disc under street lamps whose light sits inside the lamp (the Ufo lamps of Spandau), and the lamp shadows got faster [#1915](https://github.com/openOMSI-org/openOMSI/pull/1915).
- **Faster in scenes with excavations (U-Bahn entrances)**: the MSAA depth prepass is kept, about 1.5 ms less per frame there [#1614](https://github.com/openOMSI-org/openOMSI/pull/1614).
- A Shift, Ctrl or Alt whose release was missed (after a screenshot shortcut) no longer sticks [#1901](https://github.com/openOMSI-org/openOMSI/pull/1901).
- Vehicle descriptions are shown in full, and the phone launcher's vehicle sheet scrolls [#1932](https://github.com/openOMSI-org/openOMSI/pull/1932).
- The German tutorial pages show their special characters [#1552](https://github.com/openOMSI-org/openOMSI/pull/1552).
- Documentation fixes [#1917](https://github.com/openOMSI-org/openOMSI/pull/1917), [#1918](https://github.com/openOMSI-org/openOMSI/pull/1918), [#1919](https://github.com/openOMSI-org/openOMSI/pull/1919); a deprecated call replaced [#1947](https://github.com/openOMSI-org/openOMSI/pull/1947).

## 0.2.19 - 2026-10-08

### Fixes

- Mouse steering shows its "+" again: since 0.2.17 the cursor is held while the mouse steers (so the wheel reaches its full lock past the screen's edge), and the crosshair went with it. The cross is now drawn at the point that steers and takes your clicks.
- **Mouse steering**: the cross shows where the mouse steers again while the cursor is held

## 0.2.18 - 2026-10-08

### New and improved

- **Quality presets for each graphics mode**, the same in the launcher and the in-game options; the mode is chosen first and a preset never switches it. Your current settings stay as they are [#1873](https://github.com/openOMSI-org/openOMSI/pull/1873).
- **Cloud quality** (Settings → Graphics): "Low" draws the same physical clouds with fewer steps, about 0.8 ms faster on an M4; the cloud noise is kept on disk so later starts make it no more [#1872](https://github.com/openOMSI-org/openOMSI/pull/1872).
- **"DXT/BC textures stay compressed on the GPU"** can be switched off for drivers that draw compressed textures wrongly (the textures then take about twice the video memory) [#1873](https://github.com/openOMSI-org/openOMSI/pull/1873).
- **Searchable vehicle lists**, and placing, swapping or reloading a vehicle loads it in the background instead of freezing the game (1.4 s less on an articulated bus) [#1867](https://github.com/openOMSI-org/openOMSI/pull/1867).
- **Support package** (Setup → Export diagnostics): a ZIP for a bug report with the versions, the graphics card and driver, the graphics settings, the controllers and the last map and bus - no folders, names, chat, LAN codes or addresses. It stays on your computer and its folder opens, so you can look inside before attaching it [#1870](https://github.com/openOMSI-org/openOMSI/pull/1870).
- **Mods**: `[matl_glow] <texture> <value>` lets a script make a material glare (an openOMSI extension; stock content is unchanged) [#1853](https://github.com/openOMSI-org/openOMSI/pull/1853).
- **Performance captures**: an `OMSI_PROFILE` run now logs the frame-time percentiles, `OMSI_PROFILE_JSON=<file>` saves them, and `scripts/compare-performance.py` compares two runs [#1869](https://github.com/openOMSI-org/openOMSI/pull/1869).

### Fixes

- A duty started from the game menu starts at the current time, not at the first trip of the day [#1864](https://github.com/openOMSI-org/openOMSI/pull/1864).
- **Articulated buses**: every exit follows its own door, passengers on foot find the nearest section's doors [#1863](https://github.com/openOMSI-org/openOMSI/pull/1863).
- **LAN**: a weather change by the host reaches the other players at once [#1865](https://github.com/openOMSI-org/openOMSI/pull/1865).
- **Launcher**: a key binding keeps the Shift, Ctrl or Alt it was pressed with [#1866](https://github.com/openOMSI-org/openOMSI/pull/1866).
- **HafenCity**: the harbour backdrop no longer stands across the road at Landungsbrücken [#1861](https://github.com/openOMSI-org/openOMSI/pull/1861).
- **Enhanced at night**: lamp shadows are no longer looked up for surfaces facing away from the lamp (same picture, a little faster) [#1875](https://github.com/openOMSI-org/openOMSI/pull/1875).
- **The launcher opens faster**: its bus preview no longer prepares resources it doesn't use [#1871](https://github.com/openOMSI-org/openOMSI/pull/1871).
- **Build**: no compiler warnings left, and two examples no longer share an output name [#1862](https://github.com/openOMSI-org/openOMSI/pull/1862); more content tests [#1868](https://github.com/openOMSI-org/openOMSI/pull/1868).

## 0.2.17 - 2026-10-08

A big one: early, mid and late seasons with trees that turn one by one, mouse steering to full

lock, the wipers really clearing the glass, collisions with real momentum and dents, encrypted

LAN mods, a DirectX 11 option for old graphics cards, and the game's code reorganised from the

ground up so that it stays easy to change.

### New and improved

- **Early, mid and late seasons.** Pick a season and under it Early, Mid or Late. The phase sets a real date in that part of the season (the map's hemisphere decides which months), so the sun's path, the length of the day, the natural weather's temperatures and snow, the street lamps, the map's dated changes and the timetable all follow it by themselves. "By date" works as before. Also on the command line: `--season autumn-late`.
- **Wipers clear the rain and snow only where the blade has passed**, along its sweep, pushing the water ahead of it; drops merge and run off, and in snow the flakes settle on the glass and build up at the edge of the wiped area [#1828](https://github.com/openOMSI-org/openOMSI/pull/1828).
- **Collisions with AI vehicles**: a car that is hit recoils by momentum and finds its way back onto its lane, crash energy follows the masses and speeds of both vehicles, both the bus and the AI cars get dents where they were hit, and glass shows cracks, lit like the pane it is in [#1819](https://github.com/openOMSI-org/openOMSI/pull/1819). Mouse pedal strength is a new setting [#1819](https://github.com/openOMSI-org/openOMSI/pull/1819).
- **"ANGLE (DirectX 11)" graphics API on Windows**, for graphics chips whose DirectX 12, Vulkan and OpenGL drivers don't run the game (Intel HD Graphics 2000-4000, Radeon HD 5000/6000). It is tried by itself when the others fail, or chosen in Settings. The DLLs are built from Google's ANGLE source in [angle-openomsi](https://github.com/openOMSI-Project/angle-openomsi) and checked by their SHA-256.
- The information bar shows how many passengers fit next to how many are aboard [#1826](https://github.com/openOMSI-org/openOMSI/pull/1826).
- **Plugins**: `omsi.info()` names the stops on either side of the bus with their IDs, the distance to each, and whether the bus stands at a stop [#1588](https://github.com/openOMSI-org/openOMSI/pull/1588).
- **New option (Settings → Gameplay → Traffic, off by default)**: timetable buses running ahead of time wait for their departure only at the stops the timetable times itself, instead of at every stop as in OMSI [#1773](https://github.com/openOMSI-org/openOMSI/pull/1773).

### Fixes

- **Mouse steering reaches full lock both ways**: the wheel used to stop as soon as the cursor hit the edge of the screen (on macOS often at a quarter of the lock). The cursor is now held while you steer and comes back when you look around, open a menu or switch windows; the mouse pedals reach full throttle and full brake too.
- **Dedicated server**: people walk the pavements and wait at the stops around every player again, and board the timetable buses there [#1817](https://github.com/openOMSI-org/openOMSI/pull/1817).
- The offscreen pictures (and so the game's own picture checks) are taken exactly as the window draws the game: the clock goes on, the AI's lights follow the daylight, the collision settings count, the wet roads and the cabin air develop, the lighting is built the same way.
- No more crashes when the graphics driver fails in some ways it used to take the game down with: running out of video memory while recording, a driver that can't compile a shader, adapter checks that fail, closing an OpenGL context. Shader compile failures now say what the driver said.
- **OpenGL**: no more "Could not lock adapter context" crashes while another part of the game waits for the GPU ([#843](https://github.com/openOMSI-org/openOMSI/pull/843), [#898](https://github.com/openOMSI-org/openOMSI/pull/898), [#1110](https://github.com/openOMSI-org/openOMSI/pull/1110)).
- The LAN tests no longer fail now and then.

### Multiplayer

- **Trees that turn one by one.** In between two seasons the trees no longer change all together: each tree takes one of the two looks, the same every time and for every LAN player - early autumn is green with the first yellow trees, late autumn mostly bare with a few still in leaf, early spring bare with the first green, late summer partly dry.
- **LAN mods are encrypted** on the network and on disk: the files a host sends can't be read off the network, and copying them out of the game's folders gives you scrambled data, while the game reads them normally. (This stops casual copying, not a determined person: the game is open source and decrypts on the player's computer.) Hosts and players need 0.2.17 both.

### Project

- The game's code was reorganised without changing what it does (checked frame by frame against the previous version): the frame is a sequence of named steps shared by the window and the offscreen run; the AI traffic, the people and the timetable simulate in `omsi-sim` with no graphics in them, and one place hands their state to the renderer; the renderer's passes and pipelines each have a module of their own; the game's state is grouped by subsystem; every `OMSI_*` switch is listed in `docs/DEBUG_FLAGS.md` and read through one registry. No function is longer than 500 lines any more (there were ten), and a check on every pull request keeps files and functions from growing past their size again.
- openOMSI now builds on [wgpu-openomsi](https://github.com/openOMSI-Project/wgpu-openomsi), the project's fork of wgpu 29.0.4 with the graphics fixes above (and a video-memory query, which the log now shows).
- `docs/ARCHITECTURE.md` describes the architecture; its history moved to `docs/HISTORY.md`.

## 0.2.16 - 2026-10-07

### New and improved

- Lua plugins can show panels (texts, icons, progress bars, buttons) and sliding notifications of their own in the game's look (`omsi.ui`); F10 gives the mouse to the panels, Esc back to the bus. See `docs/PLUGINS.md` and the `trip_panel.lua` example [#1788](https://github.com/openOMSI-org/openOMSI/pull/1788).
- Native TrackIR head tracking on Windows (NaturalPoint's NPClient), with sensitivity and inversion per axis in the launcher; opentrack keeps working as before [#1763](https://github.com/openOMSI-org/openOMSI/pull/1763).
- **Night brightness (Settings → Graphics)**: brightens the Enhanced picture after sunset only, Off by default [#1799](https://github.com/openOMSI-org/openOMSI/pull/1799).
- **Plugins**: `trip_done` in `omsi.info()` [#1810](https://github.com/openOMSI-org/openOMSI/pull/1810).

### Fixes

- Refuelling and the bus wash take their time, litre by litre, and stop when the bus drives off [#1785](https://github.com/openOMSI-org/openOMSI/issues/1785).
- Pedestrians run over are counted once each, within the bus's real box [#1805](https://github.com/openOMSI-org/openOMSI/issues/1805).
- **Enhanced+**: the buildings of a depot or other surface object cast their shadow up close too [#1783](https://github.com/openOMSI-org/openOMSI/issues/1783).
- Smoke, exhaust and other particles drift with the wind [#1798](https://github.com/openOMSI-org/openOMSI/issues/1798).
- The automated manual gearbox can be switched on in the launcher's driving settings [#1780](https://github.com/openOMSI-org/openOMSI/issues/1780).
- A traffic light no longer reacts to a bus on the neighbouring road or bay: the depot-gate request applies only off the lanes [#1790](https://github.com/openOMSI-org/openOMSI/issues/1790).
- A duty started at the real time keeps the clock instead of jumping to ten minutes before its first trip [#1792](https://github.com/openOMSI-org/openOMSI/pull/1792).
- Puddles are smaller, with sharp irregular edges [#1804](https://github.com/openOMSI-org/openOMSI/pull/1804).
- **Dedicated server**: AI traffic and pedestrians move smoothly on the players' screens and at their real speed; the server's clock keeps to real time [#1807](https://github.com/openOMSI-org/openOMSI/pull/1807).
- **Hong Kong maps**: AI buses of a plain `[aigroup_2]` group show their destination again [#1757](https://github.com/openOMSI-org/openOMSI/pull/1757), and bus stop signs show their route numbers and pictures [#1765](https://github.com/openOMSI-org/openOMSI/pull/1765).
- AI vehicles wait before a junction when a queue on the short road pieces beyond it leaves no room, instead of blocking it [#1761](https://github.com/openOMSI-org/openOMSI/pull/1761).

## 0.2.15 - 2026-10-07

### New and improved

- Tear-off ticket blocks on buses without a printer ([#1413](https://github.com/openOMSI-org/openOMSI/pull/1413))
- **Enhanced**: a material's emissive colour shines at the screen's white ([#1228](https://github.com/openOMSI-org/openOMSI/pull/1228), [#1236](https://github.com/openOMSI-org/openOMSI/pull/1236))
- **Passengers**: nobody stands at a bus's shut doors for good
- **On foot**: up and down a double-decker's stairs (invisible walls)
- Self-lit text fields keep their light; a map named Wald does not sway ([#1384](https://github.com/openOMSI-org/openOMSI/pull/1384), [#1775](https://github.com/openOMSI-org/openOMSI/pull/1775))
- **Snow**: roads stay clear when the weather keeps snow off them ([#1362](https://github.com/openOMSI-org/openOMSI/pull/1362))
- Timetable buses rest at their stops with the engine off ([#1404](https://github.com/openOMSI-org/openOMSI/pull/1404))
- **Model**: a [matl_item] is a copy of the material before it ([#1419](https://github.com/openOMSI-org/openOMSI/pull/1419), [#738](https://github.com/openOMSI-org/openOMSI/pull/738))

### Fixes

- Fixes from the second look at the issues (0.2.15)

## 0.2.14 - 2026-10-07

### New and improved

- **Traffic lights**: a jump back while nobody asks loops, as the program's rest ([#1722](https://github.com/openOMSI-org/openOMSI/pull/1722))
- Shadows from what stands on a surface object; route arrows left out of the glow ([#1503](https://github.com/openOMSI-org/openOMSI/pull/1503), [#1615](https://github.com/openOMSI-org/openOMSI/pull/1615))
- **Enhanced**: a street lamp keeps its shadow map until another is clearly stronger ([#1613](https://github.com/openOMSI-org/openOMSI/pull/1613))
- **Timetable**: yesterday's night tour can be taken after midnight ([#1576](https://github.com/openOMSI-org/openOMSI/pull/1576))
- **Launcher**: a bus whose 3D preview hung the launcher is shown without one ([#1478](https://github.com/openOMSI-org/openOMSI/pull/1478), [#1635](https://github.com/openOMSI-org/openOMSI/pull/1635))
- **Update**: a file is put aside under a new name when the last one set aside is still held ([#1753](https://github.com/openOMSI-org/openOMSI/pull/1753))
- **Enhanced**: the mirrors show the window's sky ([#1754](https://github.com/openOMSI-org/openOMSI/pull/1754), [#1449](https://github.com/openOMSI-org/openOMSI/pull/1449))
- Getting up stands the driver beside the seat, wherever the camera looks ([#1725](https://github.com/openOMSI-org/openOMSI/pull/1725))
- **Duty start**: a run out of the depot starts at the depot ([#1642](https://github.com/openOMSI-org/openOMSI/pull/1642))
- **Enhanced**: an untextured slot keeps its colour ([#1737](https://github.com/openOMSI-org/openOMSI/pull/1737))
- Timetable buses switched off stay off when a line is chosen ([#1762](https://github.com/openOMSI-org/openOMSI/pull/1762))
- **Lua plugins**: require("os") hands out the clock only, load reads text only ([#1715](https://github.com/openOMSI-org/openOMSI/pull/1715))
- **Release**: the APK also as openOMSI-android.apk, a link that is always the newest

### Fixes

- Fixes from the issues of 5 to 7 October (0.2.14)

## 0.2.13 - 2026-10-07

### New and improved

- **Android**: an update is looked for at once after a start that never reached the launcher (0.2.13)

## 0.2.12 - 2026-10-07

### New and improved

- **Pause menu**: navigator and info bar hidden under the rail, phone cutout margin (0.2.12)

## 0.2.11 - 2026-10-07

### Fixes

- Android S24/S25 start again, pause menu as the launcher's rail (0.2.11)

## 0.2.10 - 2026-10-07

### New and improved

- Headlamp beams in the fog without the checker round the lamps (0.2.10)
- **Content folder**: Sounds, as in OMSI 2, not Sound

## 0.2.9 - 2026-10-06

### New and improved

- Clear glass of painted window layers fully cut with multisampling (0.2.9)

## 0.2.8 - 2026-10-06

### New and improved

- Black windows on buses with a paintable glass layer (0.2.8)

## 0.2.7 - 2026-10-06

### Fixes

- Android launcher opens again, lamps fade in fog, invisible covers, nearest exit, silent spawn (0.2.7)

## 0.2.6 - 2026-10-06

### Multiplayer

- Passengers for the trips due soon, crossings rung from afar, per-bus seat, LAN rear sections on slopes and more (0.2.6)

## 0.2.5 - 2026-10-06

### New and improved

- Plates from the list, free-drive passengers, timetable after a clock change, route helper lines and more (0.2.5)

## 0.2.4 - 2026-10-06

### New and improved

- Black windows on painted glass, Android black screen, AI bus gridlocks (0.2.4)

## 0.2.3 - 2026-10-06

### New and improved

- **Enhanced**: no overexposed nights among lamps, envmaps as strong as in the vanilla picture
- Follow-up to [#1677](https://github.com/openOMSI-org/openOMSI/pull/1677): its test with the module's others

## 0.2.2 - 2026-10-06

### New and improved

- **Enhanced**: windy trees, condensation on the bus's windows, clear-coat paint, night adaptation (0.2.2)
- Follow-ups to [#1640](https://github.com/openOMSI-org/openOMSI/pull/1640), [#1609](https://github.com/openOMSI-org/openOMSI/pull/1609), [#1617](https://github.com/openOMSI-org/openOMSI/pull/1617): test doc comment in place, duty terminus only without a destination index, object path gradients in FORMATS.md

## 0.2.1 - 2026-10-06

### New and improved

- **Android**: recover from a device lost while the pipelines are made (0.2.1)
- **Presence**: deploy the counter [skip actions]
- **Presence**: keep the sessions in the counter's memory

### Fixes

- **Android**: a black screen instead of the game since 0.2.0 ([#1633](https://github.com/openOMSI-org/openOMSI/pull/1633), [#1597](https://github.com/openOMSI-org/openOMSI/pull/1597)) on phones whose graphics driver cannot compile one of 0.2's new shaders (the snowfall, the lamps in the fog, the street lamps' shadow maps): the driver's error lost the whole graphics device. The game now notices this while it starts, opens the device again without those three and draws as before (snow then falls without flakes). The log names the shaders it was compiling when the device was lost.

## 0.2.0 - 2026-10-06

A release about light and weather: the Enhanced and Enhanced+ pictures worked out from the physics of the eye, the air and the lamps, and a new snowfall for every graphics mode.

### New and improved

- **Street lamps cast real shadows**: shadow maps for the four lamps lighting the camera's surroundings most (the bus, poles, signs and trees throw their shadows on the street).
- **A street lamp's light goes down and out, not into the sky**: tree crowns and upper floors above the lamps stay dark.
- **Moonlit nights**: the moon is a directional light with its own shadows; on a dark country road under a full moon the eye takes to the moonlight.
- The night sky's glow comes from the lamps round the camera (Walker's law): a village keeps its stars, a city glows orange-grey.
- Fog and mist light up round every lamp and in front of every headlight (single scattering by the droplets, a forward peak and back-scatter): halos, cones under the lamps, the glow of a bus's own beams in the fog. Rain does not do this (its drops scatter too narrowly), so headlights keep a single glow in a drizzle.
- Shafts of sunlight between the shadows of houses and trees in haze, mist and fog.
- Falling snow takes the view as a real snowfall does (heavy snow: some 400 m).
- Wet porous surfaces (soil, paving, plaster, bark) turn darker and deeper in colour in the rain, not only the asphalt.
- **An overcast deck lets through light by its thickness**: a raining nimbostratus is darker, a grey day over snow brighter.
- **Looking at the sun blinds**: the eye's scattered light round it (CIE 146 glare function) with a faint ciliary corona and lenticular halo.
- **Eye adaptation**: the picture darkens when the sun comes into view and brightens in a dark cab or an underpass, quickly towards the light and slowly towards the dark.
- Lit windows and lights have a light glow; only what is brighter than the screen's white glows.

### Fixes

- **The night is dark again**: the eye's adaptation follows a lightness-perception model (Krawczyk et al. 2005) instead of nearly full adaptation, the automatic metering no longer brightens a night, and the eye adapts to the lamps and headlights actually in view (their log-average) rather than to a fixed city level.
- **The lit ground throws light back up**: a bus's flank or a facade beside a lit street is no longer black; much more so over snow.
- Raindrops and snowflakes are lit by what is round them - the sky, the sun, the lamps - instead of glowing at one level in the dark.
- **A new snowfall**: up to 150 000 flakes worked out on the graphics card (nothing per flake on the processor), fixed in the world so the bus drives through the snow, falling gently at about a metre a second, swaying and drifting with the wind; a light fall is fewer, smaller crystals. No snow falls inside the player's bus.

## 0.1.1740 - 2026-10-05

### Project

- **Headlamps**: the grass and hedges beside the road are no longer washed out at night, the beam keeps to the lane, and a full beam reaches far down the road [#1563](https://github.com/openOMSI-org/openOMSI/pull/1563); a spot declared ahead of the bus's lamps shines from both headlamps, not from the middle [#1565](https://github.com/openOMSI-org/openOMSI/pull/1565).
- **Enhanced lighting uses a material's own ambient colour**: depot interiors with black diffuse and white ambient (Thüringer Wald, OVR Lichtentanne) are no longer black [#1556](https://github.com/openOMSI-org/openOMSI/pull/1556).
- Raised floors, markings and rails stay visible above the road surfaces at a distance, without a depth bias that moved with the camera angle [#1557](https://github.com/openOMSI-org/openOMSI/pull/1557).
- **Linux**: wheels such as the Logitech G29 are no longer taken for gamepads: linear steering and native force feedback again [#1566](https://github.com/openOMSI-org/openOMSI/pull/1566).
- **Performance**: no more regular 30-45 ms hitches near heavy collision meshes (Grand Paris-Moulon's street lamps) [#1561](https://github.com/openOMSI-org/openOMSI/pull/1561).

## 0.1.1727 - 2026-10-05

### New and improved

- **Text**: Czech and Slovak content is read as Windows-1250, so stop names keep their ř, ě and ů
- **Weather**: a .owt's second [temp] value is the dew point

### Fixes

- The Linux build of the release no longer runs out of disk space after the workspace tests (the release of 0.1.1726 did not build).
- Fix Vulkan OOM render fallback
- **Controllers**: an idle joystick nobody set up no longer takes the arrow keys for looking; it steers once its X axis moves
- **Controllers**: a gilrs panic on a controller's first event no longer ends the game
- **Fix**: improve legacy HOF destination compatibility

### Project

- CI: the Linux x64 release job frees disk space and drops the tests' debug build before building the game

## 0.1.1726 - 2026-10-05

### Project

- **Controllers**: a gilrs panic on a controller's first event no longer ends the game on Windows [#1537](https://github.com/openOMSI-org/openOMSI/pull/1537); an idle joystick nobody set up no longer takes the arrow keys for looking [#1548](https://github.com/openOMSI-org/openOMSI/pull/1548).
- Czech and Slovak content is read as Windows-1250, so stop names keep their ř, ě and ů [#1542](https://github.com/openOMSI-org/openOMSI/pull/1542).
- A `.owt` weather's second `[temp]` value is read as the dew point: winter weathers give the scripts a real humidity (exhaust steam in the frost, the heating's misted panes) [#1533](https://github.com/openOMSI-org/openOMSI/pull/1533).
- Destinations of legacy depot files show their names in the menu and on the displays, and a destination picked by hand runs the scripts' own trigger, as in OMSI 2 [#1535](https://github.com/openOMSI-org/openOMSI/pull/1535).
- **Vulkan**: running out of graphics memory on a demanding map frees the old render targets before falling back, and far textures are reduced right after streaming [#1551](https://github.com/openOMSI-org/openOMSI/pull/1551).

## 0.1.1711 - 2026-10-05

### Fixes

- The launcher's download of a server's map and buses before joining ([#1486](https://github.com/openOMSI-org/openOMSI/pull/1486), with [#1511](https://github.com/openOMSI-org/openOMSI/pull/1511)) is taken out again for now.
- The switches for sharing and downloading mods in multiplayer ([#1432](https://github.com/openOMSI-org/openOMSI/pull/1432)) are taken out again for now.

## 0.1.1707 - 2026-10-05

### Project

- Headlamps in Enhanced and Enhanced+ light the road as real ones do: evenly from the bumper on, wide, with a low beam's cut-off, instead of one bright pool where the lamp's axis meets the road; lamps pointing down (over a door) keep their cone [#1522](https://github.com/openOMSI-org/openOMSI/pull/1522).
- **Performance**: meshes share pages of vertex and index buffers, about a third less drawing work on the CPU; Enhanced+ keeps one ray tracing structure per mesh in its page [#1340](https://github.com/openOMSI-org/openOMSI/pull/1340).
- Catalan as an interface language [#1521](https://github.com/openOMSI-org/openOMSI/pull/1521); Portuguese (Brazil and Portugal) complete again [#1501](https://github.com/openOMSI-org/openOMSI/pull/1501).
- **Launcher**: buttons that set the start to the current time and date [#1517](https://github.com/openOMSI-org/openOMSI/pull/1517); no automatic update from a server while downloading missing mods is off [#1511](https://github.com/openOMSI-org/openOMSI/pull/1511).
- The personnel file stores late arrivals before early departures, as OMSI 2 does: the counts no longer swap between the two games [#1499](https://github.com/openOMSI-org/openOMSI/pull/1499).
- Dependency updates [#1510](https://github.com/openOMSI-org/openOMSI/pull/1510) [#1509](https://github.com/openOMSI-org/openOMSI/pull/1509) [#1508](https://github.com/openOMSI-org/openOMSI/pull/1508) [#1507](https://github.com/openOMSI-org/openOMSI/pull/1507) [#1506](https://github.com/openOMSI-org/openOMSI/pull/1506); a plugin DLL that does not load says the system's reason again.

## 0.1.1674 - 2026-10-05

### New and improved

- Prevent rail vehicles from overtaking

### Project

- AI trams no longer leave their rails to overtake over the oncoming lane: they wait behind what blocks them [#1512](https://github.com/openOMSI-org/openOMSI/pull/1512).

## 0.1.1671 - 2026-10-05

### Fixes

- Fix junction deformation outside height fields

### Project

- Junction plates are raised by their height field only where the field covers them: the road connection at U Ruhleben (Berlin-Spandau) is level again, without the gap and the bump, and AI lanes follow the field there too [#1493](https://github.com/openOMSI-org/openOMSI/pull/1493).

## 0.1.1668 - 2026-10-05

### Project

- **Controls**: keyboard, wheels and gamepads can be set up while playing [#1382](https://github.com/openOMSI-org/openOMSI/pull/1382); DirectInput wheels recover after a failed read or a replug, retrying a device that will not open at growing intervals [#1489](https://github.com/openOMSI-org/openOMSI/pull/1489); H-pattern shifters can return to neutral when a gear is let go [#1490](https://github.com/openOMSI-org/openOMSI/pull/1490); stale gamepad events after a focus change are ignored [#1388](https://github.com/openOMSI-org/openOMSI/pull/1388); the menu shows the default keys of the timetable (Insert) and the information bar (Shift+Y) [#1425](https://github.com/openOMSI-org/openOMSI/pull/1425).
- VR: the head's pitch is applied [#1373](https://github.com/openOMSI-org/openOMSI/pull/1373).
- **Launcher**: the roadbook keeps a picked trip when the start follows the real time [#1460](https://github.com/openOMSI-org/openOMSI/pull/1460); the tour list filters by route or stop, starts at the trip found and hides ended tours [#1461](https://github.com/openOMSI-org/openOMSI/pull/1461); a shared `HOFs` folder serves every vehicle, listed in the depot too [#1491](https://github.com/openOMSI-org/openOMSI/pull/1491); the map zooms smoothly on big maps [#1139](https://github.com/openOMSI-org/openOMSI/pull/1139).
- **Multiplayer**: joining players get the map's ground textures [#1485](https://github.com/openOMSI-org/openOMSI/pull/1485); the launcher downloads a server's map and buses before joining, and only changed files update by themselves [#1486](https://github.com/openOMSI-org/openOMSI/pull/1486); controls for mod transfers [#1432](https://github.com/openOMSI-org/openOMSI/pull/1432); a game the server sends or turns away ends and says why, while a refused reconnect plays on [#1429](https://github.com/openOMSI-org/openOMSI/pull/1429); traffic light cycles no longer stall on simultaneous inactive events [#1398](https://github.com/openOMSI-org/openOMSI/pull/1398) and stay under the host's clock [#1399](https://github.com/openOMSI-org/openOMSI/pull/1399); a host's natural weather or weather cycle reaches the clients.
- **Lua plugins**: crash, pedestrian and skipped-stops events, trip, stop and vehicle details in `omsi.info()` [#1447](https://github.com/openOMSI-org/openOMSI/pull/1447); `omsi.send` sends UDP messages (up to 8 KB) to other programs on this computer [#1448](https://github.com/openOMSI-org/openOMSI/pull/1448); `.opl` plugins read the game's values through `openomsi_*` variables [#1361](https://github.com/openOMSI-org/openOMSI/pull/1361).
- **Graphics**: trees' pictures are no longer repeated and have no line along their top [#1480](https://github.com/openOMSI-org/openOMSI/pull/1480); no invisible road walls from thin triangles [#1454](https://github.com/openOMSI-org/openOMSI/pull/1454); the high cloud layer's noise no longer jitters [#1357](https://github.com/openOMSI-org/openOMSI/pull/1357); transparent terrain costs less [#1426](https://github.com/openOMSI-org/openOMSI/pull/1426); MSAA depth prepass for mixed-material views on Apple GPUs [#1465](https://github.com/openOMSI-org/openOMSI/pull/1465); lamps light the walls next to them softer and a doubled maplight counts once [#1372](https://github.com/openOMSI-org/openOMSI/pull/1372); OpenGL no longer deadlocks prefetching vehicles [#1416](https://github.com/openOMSI-org/openOMSI/pull/1416).
- **Performance**: a new pose rescans only the instances drawing reshaped meshes [#1466](https://github.com/openOMSI-org/openOMSI/pull/1466); draws of materials that look alike are batched together [#1456](https://github.com/openOMSI-org/openOMSI/pull/1456).
- **Translations**: Traditional Chinese expanded [#1445](https://github.com/openOMSI-org/openOMSI/pull/1445).
- **Build**: the whole workspace's tests run on Windows and Linux before a release [#1431](https://github.com/openOMSI-org/openOMSI/pull/1431); shared crates declared once, an unused patch fails the audit, Dependabot waits a week [#1403](https://github.com/openOMSI-org/openOMSI/pull/1403) and updates wgpu with naga [#1370](https://github.com/openOMSI-org/openOMSI/pull/1370); dependency updates [#1349](https://github.com/openOMSI-org/openOMSI/pull/1349) [#1352](https://github.com/openOMSI-org/openOMSI/pull/1352) [#1347](https://github.com/openOMSI-org/openOMSI/pull/1347) [#1353](https://github.com/openOMSI-org/openOMSI/pull/1353).

## 0.1.1554 - 2026-10-05

### Fixes

- Enhanced+ ray tracing works on Windows and Linux again (RTX and RDNA 2 cards and newer, Direct3D 12 and Vulkan), not only on Apple silicon: on Direct3D 12 its shaders were refused by the shader compiler, so every frame was thrown away and the picture stood still on the loading screen. Cut-out leaves and fences also cast their full traced shadows on Vulkan and Direct3D 12 now.
- Should a graphics driver refuse the ray tracing all the same, Enhanced+ falls back to Enhanced instead of freezing.
- **Enhanced+**: ray tracing on Direct3D 12 and Vulkan again

## 0.1.1553 - 2026-10-05

### New and improved

- **Enhanced+**: ray tracing on Metal only for now

### Fixes

- Enhanced+ no longer stops drawing at the end of the loading screen on Windows and Linux: its ray tracing is used on Apple silicon (Metal) only for now, and elsewhere Enhanced+ draws as Enhanced. The graphics library's ray tracing on Vulkan and Direct3D 12 is still experimental; `OMSI_RT=1` tries it.

## 0.1.1552 - 2026-10-05

### New and improved

- **Presence**: fit the counter into the free plan's daily requests

### Fixes

- "Playing now" stays up all day: the game reports every ten minutes instead of every three and waits half an hour when the counter is busy, and the website asks every five minutes - the counter had run out of its daily requests and answered nobody until midnight UTC.

## 0.1.1551 - 2026-10-05

### New and improved

- Enhanced+ graphics (Settings → Graphics, `--enhanced-plus`): hardware ray tracing on Apple M3/M4 and newer, RTX and RDNA 2 cards - ray-traced sun shadows (soft away from their caster, crisp at the contact), ambient occlusion and reflections: wet roads, puddles, water, glass and chrome mirror what really stands around them, off the screen too. Elsewhere it draws as Enhanced.
- **Natural light in Enhanced and Enhanced+**: the sky is computed from the physics of the atmosphere (multiple scattering, ozone, the day's haze and its particle size, a stratospheric layer), so the blue hour, the twilight's purple and every sunset look their own. Clouds glow after the sun has set for the street, a veil of high cloud gives a milky sun with soft pale shadows, passing cumulus take the sun away and bring it back, the moon stands at its real place with its real phase and lights the night, stars show in a dark sky and a city lights its own clouds. A camera's tone curve and exposure, street lamps as bright points with a little glare (wide halos in mist and rain).
- Natural weather (the default when no weather is chosen, `--weather natural`): a physical weather model instead of one fixed state. Highs and lows pass through, cumulus grows in the afternoon and dissolves in the evening, calm clear nights leave morning fog, fronts bring a grey deck and rain, snow lies in winter and thaws in a mild spell, rain leaves clear air behind - grey days, blue evenings and sunny mornings follow from the days before, as the season and the map's latitude allow.

### Fixes

- Vehicle shadow blobs no longer flicker against the road, and in Enhanced and Enhanced+ they darken the ground under the bus as in OMSI 2.

## 0.1.1541 - 2026-10-04

### Project

- **Performance**: vehicle scripts run about a quarter faster [#1328](https://github.com/openOMSI-org/openOMSI/pull/1328), and finding the ground under the wheels costs about a quarter less CPU [#1334](https://github.com/openOMSI-org/openOMSI/pull/1334), with the same results.
- **Texture memory**: on Linux with NVIDIA's driver the card's memory is read from Vulkan, and cards over 6 GB get more automatic texture memory [#1320](https://github.com/openOMSI-org/openOMSI/pull/1320).
- Automatic start-up no longer releases the starter too early (Volvo 7900H, HH109, WSW C2), the stock buses unchanged [#1147](https://github.com/openOMSI-org/openOMSI/pull/1147).
- **Trains**: cars whose bogies are declared reversed face the right way, and cars are spaced by their declared couplings as in OMSI 2 [#1186](https://github.com/openOMSI-org/openOMSI/pull/1186).
- **Camera**: an optional precision curve for zooming with both mouse buttons, and a zoom cursor while the right button zooms [#903](https://github.com/openOMSI-org/openOMSI/pull/903); the driver's eye is back on the authored seat point [#1337](https://github.com/openOMSI-org/openOMSI/pull/1337).
- **Repository**: new issues are sorted by topic and milestone, repeated crash reports point to the existing issue, and pull requests get translation and file size checks; dependency updates and a weekly security audit [#1329](https://github.com/openOMSI-org/openOMSI/pull/1329).
- Code clean-ups after review [#1322](https://github.com/openOMSI-org/openOMSI/pull/1322).

## 0.1.1518 - 2026-10-04

### New and improved

- **The chat has a size of its own**: Ctrl + the mouse wheel over it, or Settings → General → Chat size (50-300 %), for large and 4K screens.

## 0.1.1517 - 2026-10-04

### Fixes

- The README's "playing now" badge keeps its label while the counter cannot be reached.

## 0.1.1515 - 2026-10-04

### New and improved

- The launcher looks for updates again every 30 minutes while it is open and after a game ends, not only when it starts.

### Fixes

- Updates no longer fail with "Timeout" on a slow or unsteady connection: requests are tried again, a broken download goes on where it stopped, and github.com is asked when the GitHub API does not answer.

### Multiplayer

- **Multiplayer**: every variable of the other players' buses (floats and strings: gearbox, displays, IBIS, ticket printer, plugin values) is synced, so their buses look and behave as they do for their drivers.
- **Updates during a session**: the game looks for a new version, downloads it in the background and says so over the navigator; it is installed when the session ends. The cards can be switched off (Settings → General → "Tell me about a new version during a session").
- **"Playing now": the website and the README show how many people play openOMSI right now.** The game sends only a random per-session id, its version and the kind of system; switch it off under Settings → General.

## 0.1.1512 - 2026-10-04

### New and improved

- **Launcher**: choose how Join connects (Auto, UDP, WebSocket)
- Back to the single-character forms (一, 二, 三, etc.).

### Fixes

- **Fix(i18n)**: refine Traditional Chinese labels
- **Launcher**: a server joined earlier is not joined again at the next launch

### Project

- **Mirror panels**: only the player's bus notes its mirrors' glass, so the panels no longer take another bus's mirrors [#1171](https://github.com/openOMSI-org/openOMSI/pull/1171).
- **Translations**: Simplified Chinese numbers in their single-character forms [#1282](https://github.com/openOMSI-org/openOMSI/pull/1282), Traditional Chinese labels refined [#1264](https://github.com/openOMSI-org/openOMSI/pull/1264).

## 0.1.1502 - 2026-10-04

### Project

- **Force feedback**: the wheel vibrates with the road surface and the engine, and jolts fade out instead of stopping dead, with three new sliders in the driving settings [#867](https://github.com/openOMSI-org/openOMSI/pull/867).
- The navigator shows buses, trolleybuses and trams as pictograms pointing the way they go, not as dots [#1315](https://github.com/openOMSI-org/openOMSI/pull/1315).
- **Camera**: the driver's head pitch can be set (-45 to +45 degrees) under Camera → Seat position [#1179](https://github.com/openOMSI-org/openOMSI/pull/1179); the view can glide to where the mouse turned it (Look smoothing, off by default) [#998](https://github.com/openOMSI-org/openOMSI/pull/998); the head can sway a little while the bus waits (Head sway at a standstill, off by default) [#1204](https://github.com/openOMSI-org/openOMSI/pull/1204); a snappier F1 camera switch and an eased Space return [#870](https://github.com/openOMSI-org/openOMSI/pull/870).
- **Controllers**: the gamepad's right stick turning the view can be switched off [#1170](https://github.com/openOMSI-org/openOMSI/pull/1170), and an idle device nobody set up no longer stops the gamepad stick from steering [#1165](https://github.com/openOMSI-org/openOMSI/pull/1165).
- Mouse steering switched on with O starts from the centre of the window [#1301](https://github.com/openOMSI-org/openOMSI/pull/1301).
- Mirror panels (Ctrl+M) show each mirror as its glass does, and the panel editor lists its keys and resizes panels [#1171](https://github.com/openOMSI-org/openOMSI/pull/1171).
- Passengers press the stop button at a random distance before their stop instead of all at the same point [#1191](https://github.com/openOMSI-org/openOMSI/pull/1191).
- **Timetable**: after serving the terminus, driving off starts the next trip early when it is due within five minutes [#1199](https://github.com/openOMSI-org/openOMSI/pull/1199).
- **Servers**: the status page counts the shared world (AI cars, buses, people) and an admin can set the AI traffic [#1206](https://github.com/openOMSI-org/openOMSI/pull/1206); the dispatch can take a player's duty back [#1207](https://github.com/openOMSI-org/openOMSI/pull/1207); the world options have a button to clear the AI traffic [#1234](https://github.com/openOMSI-org/openOMSI/pull/1234).
- The launcher's Join can be told to connect over UDP or WebSocket [#958](https://github.com/openOMSI-org/openOMSI/pull/958).
- Lua plugins can see and drive the vehicles round the bus (omsi.others) [#894](https://github.com/openOMSI-org/openOMSI/pull/894).
- Player screenshots leave the touch controls out [#1188](https://github.com/openOMSI-org/openOMSI/pull/1188).
- Texture memory is no longer capped at 2 GB on cards with more, and the VRAM of AMD cards on Linux is read [#1137](https://github.com/openOMSI-org/openOMSI/pull/1137).
- **Android**: a black screen on Adreno 6xx, 7xx and 8xx chips is avoided by rebuilding their shader cache [#1310](https://github.com/openOMSI-org/openOMSI/pull/1310).
- **Translations**: Simplified Chinese [#1282](https://github.com/openOMSI-org/openOMSI/pull/1282), Traditional Chinese [#1264](https://github.com/openOMSI-org/openOMSI/pull/1264), Polish [#1262](https://github.com/openOMSI-org/openOMSI/pull/1262) and French [#1251](https://github.com/openOMSI-org/openOMSI/pull/1251).

## 0.1.1423 - 2026-10-04

### Fixes

- **Build**: the small-BAR fix for NVIDIA cards ([#905](https://github.com/openOMSI-org/openOMSI/pull/905)) now comes from the project's own copy of gpu-allocator, not a personal fork.

## 0.1.1422 - 2026-10-04

### Project

- **Performance**: script names are looked up without allocating and debug switches read once [#1292](https://github.com/openOMSI-org/openOMSI/pull/1292), a wheel's ground probe walks its cell's faces once [#1287](https://github.com/openOMSI-org/openOMSI/pull/1287), a render origin moved sideways rewrites only the models' translations [#1283](https://github.com/openOMSI-org/openOMSI/pull/1283), an instance given another mesh is updated alone [#1280](https://github.com/openOMSI-org/openOMSI/pull/1280), the AI vehicles' heaviest scripts each get a job of their own [#1260](https://github.com/openOMSI-org/openOMSI/pull/1260), traffic light lamps are set again only when what they show changed [#1259](https://github.com/openOMSI-org/openOMSI/pull/1259), and the instances are culled by blocks of 128 before one by one [#1256](https://github.com/openOMSI-org/openOMSI/pull/1256).
- AI cars no longer drive through red lights, and a car put on the road in front of a red light comes in slowly enough to stop [#1253](https://github.com/openOMSI-org/openOMSI/pull/1253).
- The outside camera is no longer trapped under buses whose orbit centre lies below the ground clearance [#1246](https://github.com/openOMSI-org/openOMSI/pull/1246).
- Crash reports carry the computer, its graphics card and the map, and their title is the error alone [#1225](https://github.com/openOMSI-org/openOMSI/pull/1225).
- **Dedicated servers**: a player who drives a bus the `vehicles` list of `server.cfg` does not allow is sent away and told which buses the server has [#1222](https://github.com/openOMSI-org/openOMSI/pull/1222) (the vehicle menu offers only those since 0.1.1382).
- Bus stops put on splines are placed right in the map index before their tiles load, so a duty's stops and the announcements are right from the start [#1208](https://github.com/openOMSI-org/openOMSI/pull/1208).
- **Terrain holes**: the ground's exposed edges along spline and object cuts are closed by walls textured like the ground around them [#1069](https://github.com/openOMSI-org/openOMSI/pull/1069).
- **Linux**: a gear shifter or a button box (a device with buttons only) can be set up [#1059](https://github.com/openOMSI-org/openOMSI/pull/1059).
- **NVIDIA cards without Resizable BAR (Vulkan)**: uploads no longer go to the small BAR heap, which made loading take minutes and the game run at a few fps [#905](https://github.com/openOMSI-org/openOMSI/pull/905).
- [#1223](https://github.com/openOMSI-org/openOMSI/pull/1223): the same OpenGL fix (Intel HD 2500, Mali) was already in 0.1.1382.

## 0.1.1382 - 2026-10-04

Bug Fixes & Improvements

### New and improved

- **Lights**: in Enhanced a full beam throws its light as much further down the road as its [spotlight] range says, where it had lit the road no further than the low beam - both fell off from the same one-metre core, so its longer reach ended in the dark ([#1068](https://github.com/openOMSI-org/openOMSI/pull/1068))
- **Lights**: a [spotlight_2] in the model.cfg is a spotlight of its own, switched by its variable and mirrored onto the other side of the vehicle unless its flag says not, so low beam, main beam, fog and door lamps can light the road at once beside the [spotlight] that Spot_Select picks ([#730](https://github.com/openOMSI-org/openOMSI/pull/730))
- **Seasons**: in snow a texture takes the picture Omsi.exe takes - the map's snowy roads of WinterSnowfall when the weather has snow on the road, else WinterSnow, and its winter or autumn picture when it has no snow one - so a snowy map shows its own winter roads instead of bare asphalt in Vanilla and a flat white one laid over it in Vanilla+ and Enhanced ([#1084](https://github.com/openOMSI-org/openOMSI/pull/1084))
- **Reflections**: Vanilla and Vanilla+ read a [matl_envmap] sphere map where Direct3D's sphere-map texgen puts it, at the vertex with u = Rx/m + 0.5 and m = 2|R - (0, 0, 1)|, so a bus's flank mirrors the middle of the picture zoomed in as in OMSI 2 instead of the whole picture spread across it ([#1193](https://github.com/openOMSI-org/openOMSI/pull/1193))
- **Scenery**: a retexture - a copy of a .sco with its own texture folder whose [model] still points at the original's model file - shows its own textures, as Omsi.exe takes an object's textures from its .sco's folder and not from the model file's ([#978](https://github.com/openOMSI-org/openOMSI/pull/978))
- **Scenery**: a bridge deck or junction plate high over the ground under it is draped by its [crossing_heightdeformation] like one lying on the ground, as Omsi.exe drapes it in the object's own frame wherever it stands - one more than 12 m from the ground was left flat and met neither of its roads ([#961](https://github.com/openOMSI-org/openOMSI/pull/961))
- **Launcher**: each type in a bus family's list says how many liveries it has, as the family says how many types, counted as the Livery list counts them - the phone's list counted the repaints alone ([#717](https://github.com/openOMSI-org/openOMSI/pull/717))
- **Touch**: a finger dragged on the navigator moves it, as the mouse does, and it stays there - on a phone it had to stand in the middle of the screen and a drag on it only turned the view ([#1138](https://github.com/openOMSI-org/openOMSI/pull/1138))
- **Launcher**: Shift, Ctrl and Alt held on an Android device's keyboard count, so a key is bound with them and Ctrl+V pastes - winit's Android backend never says which modifiers are held, and the launcher only listened for that; their own keys tell it now ([#634](https://github.com/openOMSI-org/openOMSI/pull/634))
- **Passengers**: a [passpos] can name a script variable that switches the place on and off, and one the engine writes its occupancy into, so a bus can change its seating layout with a setvar and a tip-up seat can fold down for the person on it ([#721](https://github.com/openOMSI-org/openOMSI/pull/721))

### Fixes

- **Particles**: a puff of exhaust, steam or wheel spray that sinks into the road fades out into it instead of the road cutting it off in a straight line - no more bright bands across the road under the wheels and behind the bus, nor in the puddles' reflection of them - and every puff is turned its own way and set a tenth of a metre nearer in depth, as Omsi.exe draws it
- **Particles**: a puff of exhaust, steam or spray is slowed by its brake factor every twentieth of a second, as Omsi.exe slows it, not every thirtieth - it keeps its speed longer, an exhaust puff rises at up to 1.9 m/s instead of 1.3 and the wheel spray falls as fast as in OMSI
- **Rain**: every vehicle's tyres throw up spray through the puddles - a fan of water behind each wheel and a mist that hangs in the wake and settles, growing with the square of the speed - instead of droplets shot out sideways from the player's bus alone
- **Mirrors**: an [add_camera_reflexion_static] camera looks where it is pointed instead of following the driver's eye, and a rear section's reflection cameras are numbered on after the front's, so a monitor in the cab can show a CCTV picture of a rear door ([#726](https://github.com/openOMSI-org/openOMSI/pull/726))
- **Lighting**: a tile marked [variable_terrainlightmap] has its night light map baked from the lamps of the tiles round it, as Omsi.exe bakes it at load, so the roads light up under lamps that a map's stale or missing .map.LM.bmp does not know ([#951](https://github.com/openOMSI-org/openOMSI/pull/951))
- **Particles**: exhaust smoke, boiling coolant's steam, wheel spray and chimney smoke are drawn - each puff was laid out as a lamp's fog cone of no length and never showed - and a [smoke] reads as Omsi.exe reads it, skipping the line after its alpha so a puff fades out instead of going opaque ([#949](https://github.com/openOMSI-org/openOMSI/pull/949), [#948](https://github.com/openOMSI-org/openOMSI/pull/948), [#852](https://github.com/openOMSI-org/openOMSI/pull/852))
- **Snow**: the snow on the bus windows has its mip levels, so its flakes are soft specks that hold still instead of single pixels glittering over the windscreen with every move of the head ([#946](https://github.com/openOMSI-org/openOMSI/pull/946))
- **Settings**: the game menu offers 16x anisotropic filtering as the launcher does, so a 16x set there no longer shows as a bare 16 that cannot be picked again ([#669](https://github.com/openOMSI-org/openOMSI/pull/669))
- **Enhanced**: the sun lights the street only with what gets through the weather - none from under an overcast deck and next to none through a dense fog, as its disc in the sky already showed - so fog no longer glows white round the sun and an overcast day no longer has sunlight and glints ([#1106](https://github.com/openOMSI-org/openOMSI/pull/1106))
- **Text textures**: a full-colour font whose colour bitmap is not the size of its alpha bitmap draws its glyphs in the colour bitmap's colours, read at the same pixel as Omsi.exe reads them, instead of in the alpha's white ([#829](https://github.com/openOMSI-org/openOMSI/pull/829))
- **Render**: rails, markings and paving laid a millimetre over a road as splines of their own stay on it close to the camera - the road surfaces are pulled towards the eye by the same share at every vertex, which keeps them flat, instead of 2 cm more per vertex, which bowed a road's long triangles over the short ones lying on it ([#1196](https://github.com/openOMSI-org/openOMSI/pull/1196), [#1134](https://github.com/openOMSI-org/openOMSI/pull/1134))
- **Scenery**: paint made as a plain object (a flat mesh a few millimetres over the road, like a depot's bus bays) is drawn over the road as the markings are, instead of going under the road's depth bias ([#1009](https://github.com/openOMSI-org/openOMSI/pull/1009))
- **Lights**: a [light_enh] is drawn as Omsi.exe draws a [light_enh_2], with its own bitmap, z offset, brightness factor and star, so the MAN NL's stop request lamp glows over its dome instead of showing a ring round it ([#1159](https://github.com/openOMSI-org/openOMSI/pull/1159))
- **Launcher**: Settings, Sound lists the bus radio's internet stations to edit - a name and an address each, removed with the bin or added - saved to radio.cfg at once with its comments and volume kept, instead of only by editing the file ([#857](https://github.com/openOMSI-org/openOMSI/pull/857))
- **Timetable**: a duty keeps a journey log in the content folder's Journeys - each trip's stops with their planned and actual arrival and departure, how far off and whether late, early or missed - written again at every stop, as virtual bus companies ask for ([#800](https://github.com/openOMSI-org/openOMSI/pull/800), [#1061](https://github.com/openOMSI-org/openOMSI/pull/1061))
- **Server**: the game menu's Place a vehicle and Swap offer only the buses the server's vehicles list allows, as the launcher does, instead of every bus installed - the game now asks the server's status page for its list on joining ([#1183](https://github.com/openOMSI-org/openOMSI/pull/1183))
- **Touch**: the wheel on the screen, let go, comes back as the bus's wheel does with the keys - slowly standing, brisker rolling, not at all with Old Steering - instead of springing back to the middle in a third of a second at any speed ([#1090](https://github.com/openOMSI-org/openOMSI/pull/1090))
- **On foot**: C (or a button at the bottom right of a phone's screen) kneels, the eyes down to about a metre and the walk a shuffle, for a picture of a bus from low down, and stands up again ([#1148](https://github.com/openOMSI-org/openOMSI/pull/1148))
- **HUD**: the information bar breaks into rows that fit the room it has - on a phone between the buttons along the top, with the navigator below it, where it lay under the buttons and over the navigator - and stays on or off for the next session instead of having to be switched on every time ([#1164](https://github.com/openOMSI-org/openOMSI/pull/1164))
- **Renderer**: OpenGL chips without storage buffers in the vertex shader (Mali phones) keep the scene's textures within wgpu's sixteen OpenGL units and draw vanilla+, instead of panicking with "index out of bounds: the len is 16 but the index is 16" and leaving OpenGL for Vulkan ([#1133](https://github.com/openOMSI-org/openOMSI/pull/1133), [#1154](https://github.com/openOMSI-org/openOMSI/pull/1154), [#1162](https://github.com/openOMSI-org/openOMSI/pull/1162), [#1176](https://github.com/openOMSI-org/openOMSI/pull/1176), [#770](https://github.com/openOMSI-org/openOMSI/pull/770), [#316](https://github.com/openOMSI-org/openOMSI/pull/316))
- **Android**: a panic the renderer catches while it tries a graphics interface (the next one is tried) is no longer logged and written to crash.log as the game's end, so the launcher stops reporting a crash for a game that went on ([#1133](https://github.com/openOMSI-org/openOMSI/pull/1133), [#1154](https://github.com/openOMSI-org/openOMSI/pull/1154), [#1162](https://github.com/openOMSI-org/openOMSI/pull/1162), [#1176](https://github.com/openOMSI-org/openOMSI/pull/1176))
- **LAN**: a host whose OMSI 2 folder lies inside its content folder (a Linux server's openOMSI/OMSI 2) names its vehicles Vehicles/... instead of OMSI 2/Vehicles/..., so the clients draw its AI cars, buses and trains again ([#1097](https://github.com/openOMSI-org/openOMSI/pull/1097))
- **Services**: the game menu's Repair repairs a bus without a repair time (no malfunction_gettime, as most have) at once, as Omsi.exe does, instead of saying it has no repair handling ([#1048](https://github.com/openOMSI-org/openOMSI/pull/1048))
- **Destination display**: a destination picked from the list keeps the route number the bus shows, its letter too, so 92E stays 92E on the IBIS and the matrix instead of becoming 92
- **Destination display**: a destination named by its sign text (another LAN player's bus says so) is the row whose sign reads so, before an earlier row with that text on another line, so U Ruhleben is no longer shown as Machandelweg, whose district line reads RUHLEBEN, and Grundorf's Newtondorf no longer as Bhf Newtondorf
- **LAN**: a host's timetable bus shows on the others' screens the row of the depot file it was given, not the first row whose sign has that text anywhere, so Spandau's buses to U Ruhleben (282) no longer arrive there signed Machandelweg (194)
- **Destination display**: a destination or route number picked from the list turns a hand-cranked roller blind (SD77, SD80, SD81, SD83 RLB) to it, as OMSI 2's line and destination dialog sets the driven bus, instead of reaching only the IBIS, so the blind and the passengers who go by it follow the pick ([#1095](https://github.com/openOMSI-org/openOMSI/pull/1095), [#1098](https://github.com/openOMSI-org/openOMSI/pull/1098))
- **IBIS**: a duty to a destination the depot file has a row for per route, of one name and a code each, types the route the trip's stops take and shows that route's destination, so line 39 to ul. Xutorskaya is no longer typed as destination 092 with no route where its route ends at 120
- **Passengers**: a trailer on a lorry's hitch whose cabin gives no way in from the bus is boarded, ridden in and left by its own doors, instead of being walked into through the bus's back wall ([#718](https://github.com/openOMSI-org/openOMSI/pull/718))

### Multiplayer

- **Navigator**: the other players of a LAN session or server stand on the small map and the city map as purple arrows the way they face, with their names, where they drive or walk ([#1011](https://github.com/openOMSI-org/openOMSI/pull/1011), [#1080](https://github.com/openOMSI-org/openOMSI/pull/1080))

## 0.1.1324 - 2026-10-03

### Fixes

- **Feel the road**: a texture's `.surf` map is found inside mounted archives too.

### Project

- **Pull requests**: a comment links every platform's test build (nightly.link, no account needed); the build says which pull request it is and is never updated, and the Android one installs beside the release.
- **Issues**: the forms ask for the last version that worked, the map, the bus, the graphics settings and what the report is about; an issue not in English is closed until it is translated.

## 0.1.1318 - 2026-10-03

### New and improved

- Separate launcher controller release from FFB test cleanup

### Fixes

- Controller handles are released before the game starts, and outside controller setup; the DirectInput reconnect lifecycle no longer keeps stale controller state; the launcher's controller release is separate from the force-feedback test clean-up.
- Fix DirectInput reconnect lifecycle and stale controller state

## 0.1.1313 - 2026-10-03

### New and improved

- **Triple screen**: the settings code back in its one-line form where the merge only reformatted it
- **Triple screen**: placing and the object editor keep the plain cursor ray when it is off (VR unchanged), and offscreen renders one picture when a headset is asked for
- **Triple screen**: fullscreen and Alt+Enter leave a window spanning three monitors alone, and the log says when spanning overrides the window settings
- **Settings**: the Display page shows the triple screen's own settings only while it is on
- **Traffic, pedestrians**: nobody appears or vanishes on a triple screen's side panels
- **Triple screen**: nothing drawn after a lost device, the HUD's rect buffers kept between frames, and the panel targets freed once it is turned off
- **SSAO**: off-centre projections (triple screen side panels, headset eyes) find their samples where they are drawn
- Add native triple-screen support and centre-screen HUD

### Fixes

- Native triple-screen support with the HUD on the centre screen ([#1127](https://github.com/openOMSI-org/openOMSI/pull/1127)), merged with fixes: the three panels are drawn at one size (the depth, AO and rain targets were made anew twice a frame), SSAO on the angled side panels, traffic and pedestrians no longer appear or vanish on them, fullscreen and Alt+Enter leave a spanned window alone, nothing is drawn after a lost device, and the Display page shows the rig's settings only while it is on. A single screen draws exactly as before.
- **Triple screen**: the three panels are drawn at one size, so the depth, AO and rain targets are no longer rebuilt twice a frame when the width is not divisible by three

## 0.1.1302 - 2026-10-03

### Fixes

- The Drive page's map no longer fills with the interface's words after a game or a lost graphics device: the launcher kept the map's texture number from its old device, and a number past the new device's textures was drawn with the font atlas.
- **The game starts again on every system**: Steam's rich presence ([#996](https://github.com/openOMSI-org/openOMSI/pull/996)) is built only where its library exists (Windows x64, Linux x64, macOS), the library is put beside the program (also for the servers and a Linux build, which could not find it), and Android, Windows ARM64 and macOS build again (0.1.1246 was not released because of it).
- The window losing the focus or being minimised lets go of every key, button and mouse steering held, and takes no input until it is back; a minimised game runs at 30 fps.

### Project

- Voice chat in sessions through an outside voice program (replaced since by the game's own, see Unreleased), players heard where they are [#1102](https://github.com/openOMSI-org/openOMSI/pull/1102) - with a key handshake between the game and the plugin (a web page can no longer drive it), the voice server's id required, and the plugin putting the user's name and channel back.
- Resumed sessions keep the bus displays and the timetable's place, and the IBIS moves on to the next trip [#1087](https://github.com/openOMSI-org/openOMSI/pull/1087).
- The server's notifications over the navigator [#974](https://github.com/openOMSI-org/openOMSI/pull/974), a duty given by the server [#985](https://github.com/openOMSI-org/openOMSI/pull/985), a fresh hello after the WebSocket is made again [#980](https://github.com/openOMSI-org/openOMSI/pull/980), the paint the bus wears now seen by the other players [#1022](https://github.com/openOMSI-org/openOMSI/pull/1022).
- **Controllers**: latching switches [#1060](https://github.com/openOMSI-org/openOMSI/pull/1060), Linux wheels no longer buzz [#1042](https://github.com/openOMSI-org/openOMSI/pull/1042); one key between cabin and outside [#1007](https://github.com/openOMSI-org/openOMSI/pull/1007).
- **Rendering**: the render origin no longer flips while the camera is near it [#1126](https://github.com/openOMSI-org/openOMSI/pull/1126), the ICU400 sign controller shows its text [#993](https://github.com/openOMSI-org/openOMSI/pull/993), fewer meshes turned inside out [#1105](https://github.com/openOMSI-org/openOMSI/pull/1105); Chinese AI car plates [#1002](https://github.com/openOMSI-org/openOMSI/pull/1002); Volvo Wright rear doors [#1125](https://github.com/openOMSI-org/openOMSI/pull/1125).
- Tile loading diagnostics [#1111](https://github.com/openOMSI-org/openOMSI/pull/1111), cloudflared without console windows [#1001](https://github.com/openOMSI-org/openOMSI/pull/1001), Simplified Chinese [#1028](https://github.com/openOMSI-org/openOMSI/pull/1028), the radio documented [#1124](https://github.com/openOMSI-org/openOMSI/pull/1124), the release history filled in [#1130](https://github.com/openOMSI-org/openOMSI/pull/1130).

## 0.1.1246 - 2026-10-03

### New and improved

- AI cars give way to the player's bus by the same rules as to each other - at junctions, where lanes merge, and when the bus is in the junction - and let a bus out of a stop when it indicates towards the road (for 20 s at most).
- Route arrows show street and stop names in Cyrillic and other scripts their font lacks, drawn with the interface font in the arrow's colour.

### Fixes

- A car stopped behind a parked car just past a joint between road pieces pulls out round it: a lane change now carries on across the joint, so cars no longer queue for good behind a parked car on short road pieces ([#1055](https://github.com/openOMSI-org/openOMSI/issues/1055)).
- A car held for a long time by nothing anybody can see is taken away even in view, and a car creeping a few centimetres at a time counts as standing; people and cars on a bridge above or in a subway below no longer stop a car.

## 0.1.1234 - 2026-10-03

### New and improved

- Script, text and HTML textures larger than the chip takes are halved until they fit.

### Fixes

- The Drive page is laid out again, in three steps - the bus, the day and the weather, the map and the duty. The column on the left is a solid panel; the bus or the map has a stage of its own on the right, the roadbook beside the map, and a foot under it sums the choice up with the buttons in a row. Nothing lies over the map or shows through a panel any more, the line and tour rows have room for their text, stop names keep off each other and inside the map, the lists stretch with the window, and the roadbook opens on your first trip.
- OpenGL chips without storage buffers in the vertex shader (Intel HD 2500, Mali on GLES) draw the scene from textures instead of failing to start (`OMSI_GPU_ARRAYS=textures` forces it). [#770](https://github.com/openOMSI-org/openOMSI/issues/770) [#316](https://github.com/openOMSI-org/openOMSI/issues/316)
- The Vulkan and OpenGL interfaces are only started when DirectX 12 cannot draw, so their loader no longer breaks a DirectX 12 start on dual-GPU laptops. [#1058](https://github.com/openOMSI-org/openOMSI/issues/1058) [#1044](https://github.com/openOMSI-org/openOMSI/issues/1044)
- A chip without a DirectX 12 driver draws on its own Vulkan or OpenGL instead of Microsoft's software renderer. [#770](https://github.com/openOMSI-org/openOMSI/issues/770)
- A window the system cannot open ends with a message instead of a panic report.
- **A script value that is not a number no longer turns the bus to NaN and stops the game.** [#1045](https://github.com/openOMSI-org/openOMSI/issues/1045)

## 0.1.1225 - 2026-10-03

### New and improved

- In the cab, Ctrl+M can put copies of the bus's mirrors over the picture; Ctrl+Shift+M opens an editor to move, resize, add, aim and zoom them. Layouts are saved per bus. [#920](https://github.com/openOMSI-org/openOMSI/pull/920)
- **A bus radio display can show the station and song that are actually playing.** A map can supply its own stations in `radio.cfg` and give them frequencies by position on the map. [#945](https://github.com/openOMSI-org/openOMSI/pull/945)
- Route numbers with symbols, such as `-10`, are kept as written from a HOF or manual input and work on destination displays; the free route-number field accepts keyboard symbols. [#967](https://github.com/openOMSI-org/openOMSI/pull/967)

### Fixes

- In Enhanced, a soaked road has puddles in patches instead of becoming one mirror from kerb to kerb. [#950](https://github.com/openOMSI-org/openOMSI/pull/950)
- A missing scenery or spline texture can be found from another folder of the same content type when the installation contains a matching image. [#912](https://github.com/openOMSI-org/openOMSI/pull/912)
- In Vanilla+, terrain under street lamps keeps its ground colour instead of getting a white or beige veil at night. [#935](https://github.com/openOMSI-org/openOMSI/pull/935)

### Multiplayer

- After a LAN connection times out, the client keeps trying the host and can rejoin without restarting the game; `/reconnect` retries immediately. [#926](https://github.com/openOMSI-org/openOMSI/pull/926)
- **Long chat messages wrap onto following rows instead of being cut with an ellipsis.** [#969](https://github.com/openOMSI-org/openOMSI/pull/969)
- A dedicated server behind a local web gateway logs the player's forwarded address, so the operator can distinguish clients that would otherwise all appear as loopback. [#962](https://github.com/openOMSI-org/openOMSI/pull/962)

## 0.1.1223 - 2026-10-03

### New and improved

- **The cab's steering wheel turns as far left as right**: `Axle_Steering_N_L` and `_R` carry the axle's one angle, as Omsi.exe hands it to the scripts, not each tyre's Ackermann angle. [#953](https://github.com/openOMSI-org/openOMSI/issues/953) [#1073](https://github.com/openOMSI-org/openOMSI/issues/1073)
- **Mouse steering switched off in the game menu keeps the brake on, as the O key does.** [#517](https://github.com/openOMSI-org/openOMSI/issues/517) [#760](https://github.com/openOMSI-org/openOMSI/issues/760)
- **Ctrl+click on the ground in the free view (F4) moves the bus to the nearest street there.** [#1039](https://github.com/openOMSI-org/openOMSI/issues/1039)
- **The engine-off hint names the player's own key for the electrics.** [#461](https://github.com/openOMSI-org/openOMSI/issues/461)
- Stops hung on another object (`[attachObj]`) are on the route map, and a duty's stop whose tile was not loaded at the start is reached once it loads. [#1014](https://github.com/openOMSI-org/openOMSI/issues/1014) [#975](https://github.com/openOMSI-org/openOMSI/issues/975)
- A map without `unsched_vehgroups.txt` takes its random traffic from the default AI group alone, as Omsi.exe does. [#1025](https://github.com/openOMSI-org/openOMSI/issues/1025)
- **A signal that names no crossing is still a lamp, and reads phase 0 as in Omsi.exe.** [#988](https://github.com/openOMSI-org/openOMSI/issues/988)
- **Texture names stored in a Korean, Chinese or Japanese code page are found.** [#990](https://github.com/openOMSI-org/openOMSI/issues/990)
- **A mesh borrowed from another vehicle pack keeps the winding of its own pack.** [#977](https://github.com/openOMSI-org/openOMSI/issues/977) [#1054](https://github.com/openOMSI-org/openOMSI/issues/1054)
- **Scroll bars of the game menu's drop-downs and of a line's stop list can be dragged.** [#794](https://github.com/openOMSI-org/openOMSI/issues/794)
- Names macOS stores decomposed (the weather "Eiseskälte") show their accented letters.

### Fixes

- A steering or pedal key let go while Shift is held, or while the game menu is open, lets go instead of turning the wheel on to full lock. [#1040](https://github.com/openOMSI-org/openOMSI/issues/1040) [#1050](https://github.com/openOMSI-org/openOMSI/issues/1050) [#1053](https://github.com/openOMSI-org/openOMSI/issues/1053)
- An early timetable bus waits at its stop until 20 s before its departure (a train 2 min), as in Omsi.exe, instead of leaving after 40 s. [#1012](https://github.com/openOMSI-org/openOMSI/issues/1012)
- A tour that repeats the same trip no longer leaves its bus at the trip's last stop for good. [#976](https://github.com/openOMSI-org/openOMSI/issues/976)
- **People leaving a bus walk onto the pavement at their own pace instead of sliding sideways.** [#1033](https://github.com/openOMSI-org/openOMSI/issues/1033) [#1079](https://github.com/openOMSI-org/openOMSI/issues/1079)
- AI cars whose road is drawn more than 0.3 m under their lane come down onto it instead of driving in the air. [#876](https://github.com/openOMSI-org/openOMSI/issues/876)
- A terrain object lying outside its own tile is not drawn, as in OMSI (leftovers of copied tiles no longer stand in the road). [#787](https://github.com/openOMSI-org/openOMSI/issues/787)
- A `[rendertype] surface` object with `[matl_alpha] 2` on a texture without alpha is opaque, so roads behind it no longer show through. [#1008](https://github.com/openOMSI-org/openOMSI/issues/1008)
- **Enhanced**: the custom weather's brightness no longer darkens the mirrors; a white light map no longer bleaches displays and gauges at night. [#1018](https://github.com/openOMSI-org/openOMSI/issues/1018) [#827](https://github.com/openOMSI-org/openOMSI/issues/827)
- **Vanilla+**: the sphere map is blended on encoded colours as in Vanilla and Omsi.exe, so reflections are no longer twice too strong over dark surfaces. [#780](https://github.com/openOMSI-org/openOMSI/issues/780)
- **A bus whose `[boundingbox]` has a negative size no longer stops the game.** [#986](https://github.com/openOMSI-org/openOMSI/issues/986)
- **macOS**: a fresh openOMSI.app in Applications keeps its content in `~/.openomsi/content` instead of creating OMSI's folders among the applications; content already installed there stays. [#1043](https://github.com/openOMSI-org/openOMSI/issues/1043)

## 0.1.1196 - 2026-10-03

### New and improved

- **The texture fallback index looks into archives and content installed while the game runs.** [#1029](https://github.com/openOMSI-org/openOMSI/pull/1029)
- On the navigator and city map, trolleybuses, buses and trams have their own colours and show their line. [#992](https://github.com/openOMSI-org/openOMSI/pull/992)
- **The Drive page shows the map itself**: its roads, entry points and the chosen line's route. [#943](https://github.com/openOMSI-org/openOMSI/pull/943)
- **A server whose map is not installed here can be joined**: the map comes with the server's mods. [#1076](https://github.com/openOMSI-org/openOMSI/pull/1076)

### Fixes

- Roads are drawn at their own height again and win over the ground by the surfaces' depth bias, as in Omsi.exe; the 8 cm lift is gone, so footways no longer stand over the ground aligned to them and markings no longer sink under the road. [#1077](https://github.com/openOMSI-org/openOMSI/pull/1077)
- The bumps of a road texture's `.surf` map (cobbles, slabs, broken asphalt) are felt under the wheels, on splines and on junction objects alike (`OMSI_NO_SURF=1` for an A/B). [#1056](https://github.com/openOMSI-org/openOMSI/pull/1056)
- Train cars stand end to end by their model bodies, not by their declared coupling points (a CR200J's second car no longer sits 2.6 m inside the first). [#1016](https://github.com/openOMSI-org/openOMSI/pull/1016)
- Wet-road reflections work in Vanilla and Vanilla+ as well as Enhanced, also seen through the bus's own windows; rain films no longer feed the previous frame back into themselves. [#970](https://github.com/openOMSI-org/openOMSI/pull/970)

## 0.1.1169 - 2026-10-03

### New and improved

- **Sound**: over the mixer limit the nearest voices are kept, as in Omsi.exe, and the driven bus's own never cut - ranking by loudness cut voices in and out with every volume curve
- **Pause**: no notice over the picture (the game menu says it when open)

### Fixes

- A declared `[matl_transmap]` whose image is missing still keeps its opaque texture slot, as Omsi.exe does, so traffic bodies that use diffuse alpha as a reflection mask are no longer half transparent.

## 0.1.1166 - 2026-10-02

### New and improved

- A 16-bit TGA keeps its alpha bit (A1R5G5B5, as D3DX reads it).
- Sounds are mixed as in Omsi.exe's sound update (0x750340): the volume is checked against 0 dB after the distance factor (DirectSound keeps the last volume when asked for more), a loop's pitch outside 100-200,000 Hz is refused and the last rate kept (the LiAZ and trolleybus "whine" played 30x too fast), AI vehicles use viewpoint 4, sounds of other vehicles heard from the cab are x(0.2 + Snd_OutsideVol) with no invented filter, only `[3d]` sounds are panned (5 dB at most), Doppler only on loop sounds, conditions compared exactly.
- People come from one pool, as in Omsi.exe: `ai_max_humans` (OMSI's `[AIMaxCountRandom]`, 200 by default), walkers at most half of it. A waiting place another stop's person stands on is not free, so no two people stand inside each other.
- AI wheels each stand on the highest drawn face up to 3 m above them, as Omsi.exe's ground query (0x7a0814): no more cars sunk into cambered roads or hidden inside roads above the lane.

### Fixes

- A vehicle's blended layers write depth unless `[matl_noZwrite]`, exactly as Omsi.exe sets its states (0x7fd6c4), and are drawn in model order with no reordering: stacked panes (door glass with dirt, decals on glass) no longer see through each other. The vehicle the camera is in is drawn after the whole scene and its own lamp flares after it, as in Omsi.exe. [#211](https://github.com/openOMSI-org/openOMSI/issues/211) [#596](https://github.com/openOMSI-org/openOMSI/issues/596)
- Coins lie flat on the change tray at random spots and turns, as Omsi.exe places them, instead of a tower that grew 3 mm a coin.
- A duty moves on to the next trip when the bus stands at its first stop a minute before departure, so the IBIS is no longer left on the old terminus with riders refusing to board.
- A wheel no longer falls through the hair-wide seam between two spline segments (an 18 cm drop).

### Project

- The pause menu uses the launcher's greys, switches, sliders and scroll bars, follows the Interface size, and lists destination codes in a column.
- The website has a light theme (and a theme button), readable colours, and no sideways scrolling on phones.

## 0.1.1147 - 2026-10-02

### New and improved

- In the cab, Ctrl+M can put copies of the bus's mirrors over the picture; Ctrl+Shift+M opens an editor to move, resize, add, aim and zoom them. Layouts are saved per bus. [#920](https://github.com/openOMSI-org/openOMSI/pull/920)
- **A bus radio display can show the station and song that are actually playing.** A map can supply its own stations in `radio.cfg` and give them frequencies by position on the map. [#945](https://github.com/openOMSI-org/openOMSI/pull/945)
- Route numbers with symbols, such as `-10`, are kept as written from a HOF or manual input and work on destination displays; the free route-number field accepts keyboard symbols. [#967](https://github.com/openOMSI-org/openOMSI/pull/967)

### Fixes

- In Enhanced, a soaked road has puddles in patches instead of becoming one mirror from kerb to kerb. [#950](https://github.com/openOMSI-org/openOMSI/pull/950)
- A missing scenery or spline texture can be found from another folder of the same content type when the installation contains a matching image. [#912](https://github.com/openOMSI-org/openOMSI/pull/912)
- In Vanilla+, terrain under street lamps keeps its ground colour instead of getting a white or beige veil at night. [#935](https://github.com/openOMSI-org/openOMSI/pull/935)

### Multiplayer

- After a LAN connection times out, the client keeps trying the host and can rejoin without restarting the game; `/reconnect` retries immediately. [#926](https://github.com/openOMSI-org/openOMSI/pull/926)
- **Long chat messages wrap onto following rows instead of being cut with an ellipsis.** [#969](https://github.com/openOMSI-org/openOMSI/pull/969)
- A dedicated server behind a local web gateway logs the player's forwarded address, so the operator can distinguish clients that would otherwise all appear as loopback. [#962](https://github.com/openOMSI-org/openOMSI/pull/962)

## 0.1.1122 - 2026-10-02

### Fixes

- **In Vanilla+, terrain no longer gets a white veil at night.** [#935](https://github.com/openOMSI-org/openOMSI/pull/935)

## 0.1.1120 - 2026-10-02

### New and improved

- A controller button or a keyboard.cfg key can work door 1-9 front to back, or all doors (`door_1` ... `door_9`, `doors_all`). [#916](https://github.com/openOMSI-org/openOMSI/issues/916)
- **A night or light map named in a paint scheme's `[CTCTexture]` is the scheme's picture.** [#895](https://github.com/openOMSI-org/openOMSI/issues/895)
- **An active chrono event that reshapes a tile brings its own terrain and water.** [#923](https://github.com/openOMSI-org/openOMSI/issues/923) [#925](https://github.com/openOMSI-org/openOMSI/issues/925)
- **A full beam reaches as much further than the low beam as its `[spotlight]` range says.** [#941](https://github.com/openOMSI-org/openOMSI/issues/941)
- **The rear section's wheels of an articulated bus spring on the road under each of them.** [#901](https://github.com/openOMSI-org/openOMSI/issues/901)
- **A bus takes its own depot file of the map's place before another bus's.** [#896](https://github.com/openOMSI-org/openOMSI/issues/896)
- **A four-digit line such as 7110 keeps its number on the IBIS.** [#459](https://github.com/openOMSI-org/openOMSI/issues/459)
- The launcher keeps "Hold manual gear buttons", and the pause menu's switch applies to the bus.
- **The passenger view (F2) turns all the way round.** [#909](https://github.com/openOMSI-org/openOMSI/issues/909)
- **"Camera..." in the pause menu opens the driver's view settings.** [#908](https://github.com/openOMSI-org/openOMSI/issues/908)
- **The minimap can be dragged anywhere on the screen.** [#940](https://github.com/openOMSI-org/openOMSI/issues/940)
- **Ctrl+Up / Ctrl+Down (gear up and down) are keys of the list that can be moved or cleared.** [#907](https://github.com/openOMSI-org/openOMSI/issues/907) [#930](https://github.com/openOMSI-org/openOMSI/issues/930)
- A window size can be chosen (Graphics > Window size); under gamescope the game opens full screen. [#904](https://github.com/openOMSI-org/openOMSI/issues/904)

### Fixes

- A timetable bus waits only for the people walking up to its doors from the stop it serves and those getting off, as Omsi.exe does, so a full bus no longer stands with open doors for ever. [#767](https://github.com/openOMSI-org/openOMSI/issues/767)
- People off a bus walk on along the pavement instead of milling round each other at the stop. [#913](https://github.com/openOMSI-org/openOMSI/issues/913)
- A second Shift+1 shuts both front leaves of the SD202 again.
- A transmapped car body is see-through only where its transmap is: AI cars are no longer half transparent, with wheel arches showing through. [#928](https://github.com/openOMSI-org/openOMSI/issues/928) [#932](https://github.com/openOMSI-org/openOMSI/issues/932)
- A see-through layer drawn in model order (a sticker on a window) is no longer painted over by the opaque parts listed after it. [#918](https://github.com/openOMSI-org/openOMSI/issues/918)
- A scripted child of a crossing that names one of its lights reads that light's phase, so such traffic lights no longer flash yellow. [#922](https://github.com/openOMSI-org/openOMSI/issues/922)
- Textures waiting to be compressed go up at half size meanwhile, so a big articulated bus no longer runs a 3 GB graphics card out of memory while loading. [#921](https://github.com/openOMSI-org/openOMSI/issues/921)
- **On OpenGL no thread polls the GPU beside the one drawing (a crash at start).** [#898](https://github.com/openOMSI-org/openOMSI/issues/898)

## 0.1.1098 - 2026-10-02

### New and improved

- Riders complain about hard braking, fast bends and a jerky foot on the pedals (TooBad_A/B/C of the ticket packs), as in OMSI; the third time they get off at the next stop. [#862](https://github.com/openOMSI-org/openOMSI/issues/862) [#873](https://github.com/openOMSI-org/openOMSI/issues/873)
- **After the bus was removed, the mouse wheel zooms on foot and the field of view holds.** [#837](https://github.com/openOMSI-org/openOMSI/issues/837)
- **The see-through part of a blended layer shows no reflection (stickers).** [#861](https://github.com/openOMSI-org/openOMSI/issues/861)
- **A map's WinterSnow textures show their own snow, with no white laid over them.** [#879](https://github.com/openOMSI-org/openOMSI/issues/879)
- **Falling snow covers the windscreen as rain does, and the wipers clear it.** [#883](https://github.com/openOMSI-org/openOMSI/issues/883)
- **In Enhanced the map's water is drawn as water**: small waves, mirroring the sky. [#841](https://github.com/openOMSI-org/openOMSI/issues/841)
- **Street lamps' light map lights the roads as it lights the ground beside them.** [#847](https://github.com/openOMSI-org/openOMSI/issues/847)
- `gear_up` shifts a gear lever past first gear (VW T3, Peugeot 106, Manta), and a force feedback wheel that is not set up steers without a dead zone. [#866](https://github.com/openOMSI-org/openOMSI/issues/866)
- **An automated manual gearbox (in-game driving settings, off by default).** [#713](https://github.com/openOMSI-org/openOMSI/issues/713)
- `A_Trans_*` is taken over OMSI's thirtieth-of-a-second frames, so rattle scripts rattle on rough roads at any frame rate. [#772](https://github.com/openOMSI-org/openOMSI/issues/772) [#886](https://github.com/openOMSI-org/openOMSI/issues/886)
- **The game menu swaps the driven vehicle for another in its place, or reloads it.** [#728](https://github.com/openOMSI-org/openOMSI/issues/728)
- **"A right click ends the mouse steering" is back in the menu and the launcher.** [#878](https://github.com/openOMSI-org/openOMSI/issues/878)
- **"The launcher rests while a game runs" is a setting.** [#834](https://github.com/openOMSI-org/openOMSI/issues/834)
- **Mouse look sensitivity is a setting (100% = OMSI).** [#859](https://github.com/openOMSI-org/openOMSI/issues/859)
- **Parked cars can be left out altogether (Parked cars**: None). [#864](https://github.com/openOMSI-org/openOMSI/issues/864)
- **"Indicators cancel themselves" can be switched off.** [#451](https://github.com/openOMSI-org/openOMSI/issues/451)
- **The touch wheel turns as far as Wheel rotation and Full lock say.** [#856](https://github.com/openOMSI-org/openOMSI/issues/856)
- **An action can be given another key, and a mod's own trigger can be added to the list.** [#854](https://github.com/openOMSI-org/openOMSI/issues/854)
- **Any route number can be typed (Route number > Type a route number...).** [#836](https://github.com/openOMSI-org/openOMSI/issues/836)
- **A gamepad's right stick turns the driver's head.** [#454](https://github.com/openOMSI-org/openOMSI/issues/454)
- **Buses can be starred in the bus list ("Favourites only").** [#524](https://github.com/openOMSI-org/openOMSI/issues/524)

### Fixes

- **A compressed smooth dark texture keeps its colour, no longer grainy with green and blue.** [#845](https://github.com/openOMSI-org/openOMSI/issues/845)
- A mesh with no matrix or the identity is drawn as wound, as Omsi.exe draws it, so houses are no longer shown inside out. [#874](https://github.com/openOMSI-org/openOMSI/issues/874)
- **Road markings and zebra crossings lie on the road again instead of under it.** [#871](https://github.com/openOMSI-org/openOMSI/issues/871)
- Objects beside a crossing stand on the ground the map gives them: the terrain is no longer pressed into a crossing's height mesh, which Omsi.exe uses for its paths only (`OMSI_CROSSING_DEFORM=1` brings the old way back). [#860](https://github.com/openOMSI-org/openOMSI/issues/860)
- **On OpenGL a wait for the GPU no longer holds the GL context for seconds (a crash).** [#843](https://github.com/openOMSI-org/openOMSI/issues/843)
- **On a phone, a crash long after the shaders were compiled is no longer blamed on Vulkan.** [#848](https://github.com/openOMSI-org/openOMSI/issues/848)

### Multiplayer

- A stop whose waiting people another player's bus took fills up again only once that bus has left, instead of a passenger a frame: no more endless streams of riders. [#842](https://github.com/openOMSI-org/openOMSI/issues/842) [#840](https://github.com/openOMSI-org/openOMSI/issues/840) [#830](https://github.com/openOMSI-org/openOMSI/issues/830)
- The chat line opens on the key left of 1 where '/' is the manual gearbox's gear down.

### Project

- [#892](https://github.com/openOMSI-org/openOMSI/pull/892) fractional `achse_antrieb` drives the axle, [#888](https://github.com/openOMSI-org/openOMSI/pull/888) free camera fly keys, [#885](https://github.com/openOMSI-org/openOMSI/pull/885) manual gearbox detection for touch controls, [#891](https://github.com/openOMSI-org/openOMSI/pull/891) staged ignition start-up, [#881](https://github.com/openOMSI-org/openOMSI/pull/881) Portuguese, [#839](https://github.com/openOMSI-org/openOMSI/pull/839) mobile launcher options, [#789](https://github.com/openOMSI-org/openOMSI/pull/789) custom weather editor, [#792](https://github.com/openOMSI-org/openOMSI/pull/792) real-time reflections setting for the mirrors, [#910](https://github.com/openOMSI-org/openOMSI/pull/910) mirror budget counted once.

## 0.1.1068 - 2026-10-02

### Fixes

- **Mirrors are no longer redrawn twice as often as their refresh budget allows.** [#910](https://github.com/openOMSI-org/openOMSI/pull/910)

## 0.1.1066 - 2026-10-02

### New and improved

- **Mirror reflections have a real-time setting with None, Economical and Full modes.** [#792](https://github.com/openOMSI-org/openOMSI/pull/792)

## 0.1.1061 - 2026-10-02

### New and improved

- Custom weather can be edited from the launcher and in-game, including visibility, wind, temperature, humidity, clouds, precipitation, road wetness and snow. METAR ICAO stations can be entered directly, the controls are available on mobile too, and LAN clients follow a host's custom weather. [#789](https://github.com/openOMSI-org/openOMSI/pull/789)
- Preserve latest upstream app events in weather branch
- Bring mobile launcher setup options in line with desktop
- Expand weather controls in the new UI

## 0.1.1050 - 2026-10-02

### New and improved

- Very small resting speeds count as stopped, so systems that require a stationary vehicle, including passenger boarding, do not get held up by tiny residual motion. [#897](https://github.com/openOMSI-org/openOMSI/pull/897)
- **Vehicles**: treat tiny resting speeds as stopped
- Complete Brazilian and European Portuguese translations

### Fixes

- **Fix**: support staged bus startup controls
- **Fix**: pass zero state to release triggers

## 0.1.1043 - 2026-10-02

### New and improved

- Gearbox tests declare their script variables the same way a bus varlist does, including the shared-cockpit manual gearbox case.

## 0.1.1042 - 2026-10-02

### New and improved

- **An axle with a fractional `achse_antrieb` share is treated as a driven axle.** [#892](https://github.com/openOMSI-org/openOMSI/pull/892)

### Fixes

- **Fix**: an axle with a share of the drive is a driven axle
- Fix manual gearbox detection for shared cockpit scripts

## 0.1.1038 - 2026-10-02

### Fixes

- **Free-camera fly keys no longer fire OMSI `[game]` actions at the same time.** [#888](https://github.com/openOMSI-org/openOMSI/pull/888)

## 0.1.1036 - 2026-10-02

### New and improved

- **Tests**: the graphics profile controls are named once in the settings list
- **Articulated buses**: a rear section turns about its own [rot_pnt_long] line, so a steered rear axle follows the front section through bends ([#322](https://github.com/openOMSI-org/openOMSI/pull/322))
- **Mods**: a bus deleted from Mods/installed is uninstalled, its folder kept in Mods/uninstalled, and leaves the lists ([#819](https://github.com/openOMSI-org/openOMSI/pull/819))
- **Situations**: the game menu saves to new save slots, and the launcher continues from any of them ([#341](https://github.com/openOMSI-org/openOMSI/pull/341))
- **Input**: mouse steering goes on steering in the map camera (F4), as in OMSI ([#516](https://github.com/openOMSI-org/openOMSI/pull/516))
- **Input**: the exit key of keyboard.cfg (Ctrl+Q) ends the game, as the menu's Quit does ([#817](https://github.com/openOMSI-org/openOMSI/pull/817))
- **Updater**: on Linux the new launcher is started after an update, not the old one set aside ([#811](https://github.com/openOMSI-org/openOMSI/pull/811))
- **Traffic**: a car standing behind a vehicle that waits to move over lets it in ([#414](https://github.com/openOMSI-org/openOMSI/pull/414))
- **Scenery**: an object with traffic paths keeps the height the map gives it ([#828](https://github.com/openOMSI-org/openOMSI/pull/828))
- **Timetable buses**: an AI bus passes a stop where nobody gets off or on ([#703](https://github.com/openOMSI-org/openOMSI/pull/703))
- **Traffic lights**: a junction runs its lights for a lamp without [trafficlight] ([#822](https://github.com/openOMSI-org/openOMSI/pull/822))
- **Terrain**: a [terrainhole] cutter cuts the ground along its rim, not by texel, so no grass stands over a junction's carriageway ([#823](https://github.com/openOMSI-org/openOMSI/pull/823))
- **Scenery**: a signal's lenses follow their [alphascale] and [matl_lightmap] variables, and scenery objects get their light maps ([#826](https://github.com/openOMSI-org/openOMSI/pull/826))
- **Vehicles**: Envir_Brightness is light A plus the light map's light on the vehicle, so bus glass stays under street lamps at night ([#624](https://github.com/openOMSI-org/openOMSI/pull/624))
- **Render**: a vehicle's outer skin below the roof is lit as outside, not as cab ([#805](https://github.com/openOMSI-org/openOMSI/pull/805))
- **Passengers**: GetHumanCountOnSeat numbers the seats with the driver's place, as OMSI does
- **Passengers**: getting off, they head for the nearest open exit, not a shut door ([#493](https://github.com/openOMSI-org/openOMSI/pull/493))
- **Driver**: no figure standing in a T-pose in the aisle before it is first posed (EN92 driver T-pose)
- **Feat**: hide the mouse cursor while driving with a game controller
- Cut the ground exactly along a [terrainhole] cutter's rim

### Fixes

- **Road seams no longer kick the steering wheel.** [#821](https://github.com/openOMSI-org/openOMSI/pull/821)
- **Fix**: slow suspension motion jolts the wheel as before, and no jolt on return
- **Scripts**: A_Trans_X/Y/Z are the body's acceleration without gravity, as Omsi.exe's, so A_Trans_Z reads 0 standing or cruising instead of 9.81
- **Articulated buses**: the rear section finds a viaduct's deck again after a gap and no longer drops onto the road beneath it ([#135](https://github.com/openOMSI-org/openOMSI/pull/135))
- **Launcher**: a long list (a tour of hundreds of trips) no longer uses up the layers the rail and the buttons are drawn in, so the sidebar stays ([#666](https://github.com/openOMSI-org/openOMSI/pull/666))
- **Timetable**: the paper's rows are as high as GDI makes a Courier New line (23 px), and stay on the paper's lines ([#629](https://github.com/openOMSI-org/openOMSI/pull/629))
- **Launcher**: brought forward while a game runs, it is drawn and answers again, so the session code can be copied ([#825](https://github.com/openOMSI-org/openOMSI/pull/825))
- **Passengers**: people waiting at a shut exit keep to its point, no longer lifted up in the bus and stuck there ([#709](https://github.com/openOMSI-org/openOMSI/pull/709))
- **On foot**: walking without a bus of one's own no longer flies the camera ahead of the walker ([#807](https://github.com/openOMSI-org/openOMSI/pull/807))
- **LAN**: passengers handed over to a client's bus ride to a stop instead of getting off at once ([#813](https://github.com/openOMSI-org/openOMSI/pull/813))
- **Fix**: hidden cursor drops hover, stays out of menus, VR and the mirror editor

### Multiplayer

- **LAN**: the chat's keys are in the key list (Controls) and can be moved off a key the bus needs ([#130](https://github.com/openOMSI-org/openOMSI/pull/130))

## 0.1.1028 - 2026-10-02

### New and improved

- **Getting off, passengers head for the nearest open exit, not a shut door, as in OMSI.** [#493](https://github.com/openOMSI-org/openOMSI/issues/493)
- `GetHumanCountOnSeat` numbers the seats with the driver's place, as OMSI does (seats were one off for scripts).
- **Timetable AI buses pass a stop where nobody gets off or waits, as in OMSI.** [#703](https://github.com/openOMSI-org/openOMSI/issues/703)
- **An object with traffic paths keeps the height the map gives it (mod road pieces).** [#828](https://github.com/openOMSI-org/openOMSI/issues/828)
- **A car standing behind a vehicle that waits to move over lets it in (a deadlock).** [#414](https://github.com/openOMSI-org/openOMSI/issues/414)
- A `[terrainhole]` cuts the ground along the cutter's rim, so no grass stands over the edges of a junction's carriageway. [#823](https://github.com/openOMSI-org/openOMSI/issues/823) [#672](https://github.com/openOMSI-org/openOMSI/issues/672)
- A bus's outer skin below the roof is lit as outside, not as cab: no seam round the body and its reflections are back in Enhanced. [#805](https://github.com/openOMSI-org/openOMSI/issues/805)
- Signal lenses follow their `[alphascale]` and `[matl_lightmap]` variables, and scenery objects get their light maps. [#826](https://github.com/openOMSI-org/openOMSI/issues/826)
- **A rear section turns about its own `[rot_pnt_long]` line, so steered rear axles work.** [#322](https://github.com/openOMSI-org/openOMSI/issues/322)
- `A_Trans_X/Y/Z` are the body's acceleration without gravity, as in Omsi.exe (`A_Trans_Z` read 9.81 standing, so air suspension scripts never re-levelled).
- **Save slots**: the game menu saves to a new slot, and the launcher continues from any. [#341](https://github.com/openOMSI-org/openOMSI/issues/341)
- **A bus deleted from `Mods/installed` is uninstalled (its folder kept in `Mods/uninstalled`).** [#819](https://github.com/openOMSI-org/openOMSI/issues/819)
- **The exit key of keyboard.cfg (Ctrl+Q) ends the game.** [#817](https://github.com/openOMSI-org/openOMSI/issues/817)
- **Mouse steering goes on in the map camera (F4), as in OMSI.** [#516](https://github.com/openOMSI-org/openOMSI/issues/516)
- **On Linux the new launcher is started after an update, not the old one.** [#811](https://github.com/openOMSI-org/openOMSI/issues/811)

### Fixes

- People getting off who wait at a shut exit keep to its point; they are no longer lifted up inside the bus and stacked there, blocking everyone behind them. [#709](https://github.com/openOMSI-org/openOMSI/issues/709)
- The driver no longer stands in a T-pose in the aisle before he is first seated.
- A junction runs its traffic lights for a lamp without `[trafficlight]`, as Omsi.exe does, so mod traffic lights work again and AI cars stop at them. [#822](https://github.com/openOMSI-org/openOMSI/issues/822) [#818](https://github.com/openOMSI-org/openOMSI/issues/818)
- `Envir_Brightness` is the night light plus the street lamps' light on the vehicle, so bus glass no longer turns clear at night. [#624](https://github.com/openOMSI-org/openOMSI/issues/624)
- **The rear section of an articulated bus finds a viaduct's deck again after a gap.** [#135](https://github.com/openOMSI-org/openOMSI/issues/135)
- **The launcher brought forward while a game runs is drawn and answers again.** [#825](https://github.com/openOMSI-org/openOMSI/issues/825)
- **A long tour no longer makes the Timetable page lose its sidebar.** [#666](https://github.com/openOMSI-org/openOMSI/issues/666)
- **The paper timetable's rows are as high as GDI makes them and stay on the paper's lines.** [#629](https://github.com/openOMSI-org/openOMSI/issues/629)

### Multiplayer

- **Passengers handed over to a client's bus ride to a stop instead of getting off at once.** [#813](https://github.com/openOMSI-org/openOMSI/issues/813)
- **The chat keys are in the key list (Controls) and can be moved off a key the bus needs.** [#130](https://github.com/openOMSI-org/openOMSI/issues/130)

## 0.1.1000 - 2026-10-02

### New and improved

- Interaction text is hidden when the object cannot actually be reached.
- Loading a graphics profile works from the in-game menu, and 8x MSAA is available there.

### Fixes

- The in-game server code can be copied, cursor flicker is fixed, and menu selection/rendering is less distracting. [#846](https://github.com/openOMSI-org/openOMSI/pull/846)

## 0.1.983 - 2026-10-02

### New and improved

- Real-time and real-date options keep the launcher's choice on the system clock.
- Controller actions use readable OMSI names, and Alt+Enter switches between a window and borderless full screen. [#758](https://github.com/openOMSI-org/openOMSI/pull/758)

### Fixes

- A tour can be started from a chosen trip instead of only from the duty's first one.

## 0.1.973 - 2026-10-02

### New and improved

- **The driver's pose and arms are more natural**: the torso sits straighter, shoulders stay visible through turns, and both hands remain on the wheel except while shifting. [#734](https://github.com/openOMSI-org/openOMSI/pull/734)
- Adjust driver parameters for seat and reach dynamics
- Adjust constants for driver ergonomics
- Remove duplicate lever_target call in driver.rs
- Refine driver seat and gear lever parameters

## 0.1.968 - 2026-10-02

### New and improved

- Discord Rich Presence can show the launcher or the current map, bus, line and tour, and can be disabled in Settings. [#779](https://github.com/openOMSI-org/openOMSI/pull/779)
- **Feat**: integrate Discord Rich Presence for game and launcher

### Fixes

- **Fix**: preserve Discord bus labels when vehicle types are empty

## 0.1.965 - 2026-10-02

### New and improved

- **Render**: the headset shows the Enhanced graphics ([#784](https://github.com/openOMSI-org/openOMSI/pull/784))
- **Render**: no rain inside the rear section of articulated buses ([#777](https://github.com/openOMSI-org/openOMSI/pull/777))
- **Vehicles**: AI buses' triggered sounds read their volume curves at the trigger too ([#676](https://github.com/openOMSI-org/openOMSI/pull/676))
- **Vehicles**: a triggered sound reads its volume curves as they stand when the trigger fires ([#676](https://github.com/openOMSI-org/openOMSI/pull/676))
- **Vehicles**: a key bound in [game] and in [vehicles] works the vehicle too ([#745](https://github.com/openOMSI-org/openOMSI/pull/745))
- **Vehicles**: on foot, the switches of an articulated bus's rear section are in reach by that section ([#715](https://github.com/openOMSI-org/openOMSI/pull/715))
- **Vehicles**: a switch let go with the right button held stays where it is ([#769](https://github.com/openOMSI-org/openOMSI/pull/769))
- **Traffic**: the player's bus holds up only the traffic on its own level, not the cars or trains on a bridge above it or a road below ([#753](https://github.com/openOMSI-org/openOMSI/pull/753))
- **IBIS**: a depot file's stop list belongs to the trip written before it, as Omsi.exe reads it; a trip without a list gave every later route the stops of another ([#667](https://github.com/openOMSI-org/openOMSI/pull/667))
- **Text**: on a Chinese, Japanese or Korean Windows, only text that reads in that code page is taken for it, so Russian HOF stop names keep their names ([#785](https://github.com/openOMSI-org/openOMSI/pull/785))
- **Passengers**: test that a stop answers to its label and its timetable name ([#773](https://github.com/openOMSI-org/openOMSI/pull/773))
- **Passengers**: riders know their stop by its timetable name too ([#773](https://github.com/openOMSI-org/openOMSI/pull/773), [#748](https://github.com/openOMSI-org/openOMSI/pull/748), [#736](https://github.com/openOMSI-org/openOMSI/pull/736))
- **Passengers**: a stop is no destination of itself or of a stop of its name ([#795](https://github.com/openOMSI-org/openOMSI/pull/795))
- **Launcher**: typing into an open dropdown searches it ([#747](https://github.com/openOMSI-org/openOMSI/pull/747))
- UI: the city map and the navigator open on foot ([#705](https://github.com/openOMSI-org/openOMSI/pull/705))
- **Input**: the built-in view keys step aside for the player's own ([#701](https://github.com/openOMSI-org/openOMSI/pull/701))
- UI: the game's and the launcher's windows fit the screen and open in its middle ([#771](https://github.com/openOMSI-org/openOMSI/pull/771))
- **Camera**: the ticket desk camera keeps where it was turned ([#733](https://github.com/openOMSI-org/openOMSI/pull/733))
- **Launcher**: dropdown lists scroll by dragging their bar ([#794](https://github.com/openOMSI-org/openOMSI/pull/794))

### Fixes

- **Tests**: the launcher's process tests import what they test again, and the settings list names the graphics profile controls
- **Render**: the loading screen remakes a swapchain that no longer fits ([#776](https://github.com/openOMSI-org/openOMSI/pull/776))
- **Render**: a fleet number or plate no longer glows in the dark ([#698](https://github.com/openOMSI-org/openOMSI/pull/698))
- **Render**: clouds no longer vanish when a sky texture cannot be read ([#749](https://github.com/openOMSI-org/openOMSI/pull/749))
- **Situations**: a saved duty is continued at the trip of the tour it was saved on, with the rest of the tour, and saved again with that trip, not the first leg ([#653](https://github.com/openOMSI-org/openOMSI/pull/653))
- **Traffic**: cars drive into the end of the road at speed and are taken off there, as in OMSI, instead of stopping before it one by one ([#765](https://github.com/openOMSI-org/openOMSI/pull/765), [#598](https://github.com/openOMSI-org/openOMSI/pull/598))
- **Launcher**: sliding an open dropdown no longer scrolls the page behind it ([#774](https://github.com/openOMSI-org/openOMSI/pull/774))
- **Input**: the brake stays on when the mouse steering is switched off ([#517](https://github.com/openOMSI-org/openOMSI/pull/517), [#760](https://github.com/openOMSI-org/openOMSI/pull/760))
- **Camera**: head tracking stays on when its UDP port is taken ([#775](https://github.com/openOMSI-org/openOMSI/pull/775))

### Project

- The changelog was expanded with 29 issue fixes from the preceding development work.

## 0.1.960 - 2026-10-02

### New and improved

- Riders know their stop by its timetable name as well as by its map label, so they get off at intermediate stops where the two names differ, not only at the terminus. [#773](https://github.com/openOMSI-org/openOMSI/issues/773) [#748](https://github.com/openOMSI-org/openOMSI/issues/748) [#736](https://github.com/openOMSI-org/openOMSI/issues/736)
- **The player's bus holds up only traffic on its own level, not cars on a bridge above it.** [#753](https://github.com/openOMSI-org/openOMSI/issues/753)
- **On a Chinese, Japanese or Korean Windows, Russian HOF and map text keeps its stop names.** [#785](https://github.com/openOMSI-org/openOMSI/issues/785)
- Door hit sounds with a `doorSpeed` volume curve are heard: a triggered sound reads its volume curves as they stand when the trigger fires (player's and AI buses). [#676](https://github.com/openOMSI-org/openOMSI/issues/676)
- **A switch let go with the right button held stays where it is.** [#769](https://github.com/openOMSI-org/openOMSI/issues/769)
- **On foot, the switches of an articulated bus's rear section are in reach by that section.** [#715](https://github.com/openOMSI-org/openOMSI/issues/715)
- **A key bound in both [game] and [vehicles] works the vehicle too.** [#745](https://github.com/openOMSI-org/openOMSI/issues/745)
- **The built-in view keys step aside for keys the player bound themselves.** [#701](https://github.com/openOMSI-org/openOMSI/issues/701)
- **The city map (Shift+M) and the navigator (Shift+N) open on foot.** [#705](https://github.com/openOMSI-org/openOMSI/issues/705)
- **The ticket desk camera keeps where it was turned (Home recentres only when unbound).** [#733](https://github.com/openOMSI-org/openOMSI/issues/733)
- **Typing into an open dropdown filters it (entry points, buses, fleet numbers).** [#747](https://github.com/openOMSI-org/openOMSI/issues/747)
- **The game's and the launcher's windows fit the screen and open in its middle.** [#771](https://github.com/openOMSI-org/openOMSI/issues/771)
- **No rain inside the rear section of articulated buses.** [#777](https://github.com/openOMSI-org/openOMSI/issues/777)
- **The VR headset shows the Enhanced graphics.** [#784](https://github.com/openOMSI-org/openOMSI/issues/784)

### Fixes

- A stop is no longer a destination of itself or of a stop of the same name, so riders on circular lines no longer board and get straight off again. [#795](https://github.com/openOMSI-org/openOMSI/issues/795)
- Cars drive into the end of a road at speed and are taken off there, as in OMSI, instead of stopping before it one by one; released AI buses no longer queue at the map's end. [#765](https://github.com/openOMSI-org/openOMSI/issues/765) [#598](https://github.com/openOMSI-org/openOMSI/issues/598)
- A depot file's stop list belongs to the trip written before it, as Omsi.exe reads it; a trip without a list no longer gives every later route another trip's stops in the IBIS. [#667](https://github.com/openOMSI-org/openOMSI/issues/667)
- A continued situation resumes at the trip of the tour it was saved on, with the rest of the tour, and is saved again with that trip. [#653](https://github.com/openOMSI-org/openOMSI/issues/653)
- **The brake stays on when the mouse steering is switched off.** [#517](https://github.com/openOMSI-org/openOMSI/issues/517) [#760](https://github.com/openOMSI-org/openOMSI/issues/760)
- **Head tracking stays on when its UDP port is taken by opentrack.** [#775](https://github.com/openOMSI-org/openOMSI/issues/775)
- Dropdown lists scroll by dragging their bar, and sliding an open list no longer scrolls the page behind it. [#794](https://github.com/openOMSI-org/openOMSI/issues/794) [#774](https://github.com/openOMSI-org/openOMSI/issues/774)
- **Clouds no longer vanish when a sky texture cannot be read; 24-bit bitfield BMPs load.** [#749](https://github.com/openOMSI-org/openOMSI/issues/749)
- Fleet numbers and plates no longer glow in the dark; only text with a light or night map does. [#698](https://github.com/openOMSI-org/openOMSI/issues/698)
- **The loading screen remakes a Vulkan swapchain that no longer fits instead of freezing.** [#776](https://github.com/openOMSI-org/openOMSI/issues/776)

## 0.1.936 - 2026-10-02

### New and improved

- Added 0.1.927 & 0.1.929 changelog
- Stretch spline-joined objects onto the world grid with their tile

### Fixes

- **Text textures no longer break up when viewed from a distance.** [#806](https://github.com/openOMSI-org/openOMSI/pull/806)
- Fix text textures breaking up at distance

## 0.1.929 - 2026-10-02

### New and improved

- VR navigator settings are restored in the new pause menu [#808](https://github.com/openOMSI-org/openOMSI/pull/808)

## 0.1.927 - 2026-10-02

### New and improved

- **A new in-game menu is implemented, together with real-time and METAR synchronisation.** [#679](https://github.com/openOMSI-org/openOMSI/pull/679)
- **Steering wheel hats are read on Linux, and the list scrolls to the pressed button.** [#788](https://github.com/openOMSI-org/openOMSI/pull/788)

### Fixes

- Far stand-in models are drawn only from the tiles OMSI loads together with them, so stand-ins no longer appear for tiles that are not part of the loaded set. [#743](https://github.com/openOMSI-org/openOMSI/pull/743)
- **Scenery sign text is aligned correctly again.** [#764](https://github.com/openOMSI-org/openOMSI/pull/764)
- **Wheels no longer jolt when driving over stacked road surfaces.** [#757](https://github.com/openOMSI-org/openOMSI/pull/757)

## 0.1.864 - 2026-10-02

### New and improved

- **Cars simulated out of range take the same routes that an active car on the road would take.** [#782](https://github.com/openOMSI-org/openOMSI/pull/782)
- Cars out of range take the ways a car on the road would take

## 0.1.862 - 2026-10-02

### New and improved

- **Traffic for the lights only**: cars of a raised and lowered target still go
- AI buses pull into the bay at a stop just behind a junction
- Traffic lights run without AI traffic

### Fixes

- **AI buses can pull into a stop bay immediately after a junction instead of missing it.** [#737](https://github.com/openOMSI-org/openOMSI/pull/737)

## 0.1.857 - 2026-10-02

### New and improved

- Reset script texture colour on renewal

### Fixes

- **Masterbus destination displays work again.** [#768](https://github.com/openOMSI-org/openOMSI/pull/768)
- Fix initial and scaled script displays

## 0.1.853 - 2026-10-02

### New and improved

- Vehicle light sprites draw the right way up and at Omsi.exe's size: an asymmetric `[light_enh_2]` glow bitmap (a light streak such as a bus's `lights_abbl`/`lights_stand`) was drawn upside down - the sprite's UV was built as if the corner vector were a clip-space value, so v = 1 landed at the sprite's top and put the picture's first row along its bottom, though the engine's convention (overlay.wgsl) is that v = 0 is the first, topmost row (flipped at the sample site only, so the fog cone fan keeps its own `fan_uv` mapping); and every glow is now drawn a tenth narrower, the sprite radius carrying a 0.9 measured against Omsi.exe, which draws it a little inside the diameter its `size` names (rain and snow streaks are unaffected - they are not a light)

### Fixes

- **Vehicle glow bitmaps are no longer upside down and use the same sizing as OMSI 2.** [#741](https://github.com/openOMSI-org/openOMSI/pull/741)

## 0.1.851 - 2026-10-02

### New and improved

- **Attached objects (`[attachObj]`) and object labels are read as Omsi.exe reads them.** The records of a tile depend on its `[version]`: before version 9 there is no detail level line, before 6 no IDCodes, before 10 an attached object names its parent by its place in the tile instead of its IDCode, before 8 it has no heading, and before 11 a spline has one line linking it to the spline before it instead of both neighbours. Older tiles were read as version 14 ones, so their objects hung on the wrong parents and pieces of road stood in the air. An object's labels are now exactly as many lines as it says (an empty label or one in brackets no longer cuts the rest off), an attachment whose parent is written after it or that hangs on a later object of a spline attachment row is not loaded, as in OMSI.
- **Maps**: read [attachObj], labels and old tile versions as Omsi.exe does

## 0.1.850 - 2026-10-02

### New and improved

- **Door keys of UK buses**: on Road-hog123's door script (London Citybus 400, Enviro400s and many more) `bus_doorfront0` opens the door and `bus_doorfront1` closes it, and Shift+1 fired both, so the door never opened. Door key triggers are now tried on the scripts first, and when they undo each other only the one that moves the door is fired.
- `OMSI_PROFILE` lists triangles per asset and names the stages of slow people ticks; `OMSI_SKIP_PIPE` and `OMSI_CHECK_GROUND` help measuring.

### Fixes

- Big installations no longer show an empty launcher for minutes: the maps appear at once and the buses as their folders are read, several folders at a time, with the progress in the status line. A poll no longer starts the whole reading over while it runs (the growing cache changed the content stamp, so a large OMSI folder never finished loading). Depot files are read for their name only, and the search of every vehicle folder for a map's depot runs once per session.
- **Mouse steering stays on**: the right button looks round without ending it (also in the pause), the cursor goes back to where it steered when the button is let go, and the game starts with mouse steering as it was left, the cursor in the middle of the window. OMSI's right click that ends mouse steering is a setting (Esc > Options > "A right click ends the mouse steering").
- People standing still are not skinned and uploaded again every frame (2.6 ms to 0.15 ms a frame for thirty waiting passengers).

### Project

- [#661](https://github.com/openOMSI-org/openOMSI/pull/661) shadow blobs lie on the road under a bridge, and can be switched off; [#696](https://github.com/openOMSI-org/openOMSI/pull/696) glass found by its texture's alpha (lamps show through every bus's windows); [#699](https://github.com/openOMSI-org/openOMSI/pull/699) "The game is running" in the launcher; [#704](https://github.com/openOMSI-org/openOMSI/pull/704), [#687](https://github.com/openOMSI-org/openOMSI/pull/687), [#689](https://github.com/openOMSI-org/openOMSI/pull/689), [#675](https://github.com/openOMSI-org/openOMSI/pull/675), [#688](https://github.com/openOMSI-org/openOMSI/pull/688) dedicated server administration (`tell`, weather by name, `/status` weather and clock, `POST /admin` from the same machine - refused through a tunnel or proxy); [#706](https://github.com/openOMSI-org/openOMSI/pull/706), [#710](https://github.com/openOMSI-org/openOMSI/pull/710) outside camera; [#707](https://github.com/openOMSI-org/openOMSI/pull/707) 16x anisotropic filtering; [#711](https://github.com/openOMSI-org/openOMSI/pull/711) 8x MSAA; [#708](https://github.com/openOMSI-org/openOMSI/pull/708) a Windows test; [#714](https://github.com/openOMSI-org/openOMSI/pull/714) changelog.

### Credits

- Passengers get off double-deckers again: the once-a-second check for another open door ran every frame, pulling everyone back to the nearest path point, so people coming down from the upper deck stayed on the stairs. People held up face to face in the aisle or on the stairs squeeze past after two seconds.
- Passengers paying at the cash desk hold the money out to the tray instead of raising the arm up and forward: the arm's reach had its lift and turn the wrong way round.
- Standing passengers keep their feet on the ground at every tick, as Omsi.exe does, not at the height of their waiting place's object.
- A long bus station stop gets the waiting places of objects along its whole length.

## 0.1.810 - 2026-10-02

### Fixes

- Buttons that contain only an icon now centre it correctly instead of leaving the spacing reserved for a missing label; this fixes the livery arrows on Drive > Bus and the mobile file browser's folder-up button ([#678](https://github.com/openOMSI-org/openOMSI/pull/678)).
- **A button with only an icon has it in its middle**: the gap before a label is left out when there is none, so the livery arrows on Drive > Bus (and the mobile browser's folder-up) are no longer 3 px to the left

## 0.1.808 - 2026-10-02

### New and improved

- **VR has an adjustable cockpit navigator attached to the bus.** It can be moved and rotated, its distance and size can be adjusted, and position, rotation, size, opacity and visibility are saved separately for each bus ([#693](https://github.com/openOMSI-org/openOMSI/pull/693)).
- `Ctrl+Shift+M` enters navigator placement mode and `Ctrl+Shift+N` shows or hides it; both actions can be rebound. The same placement settings are available in the VR options menu ([#693](https://github.com/openOMSI-org/openOMSI/pull/693)).
- The VR pause menu is smaller for a more comfortable fit ([#693](https://github.com/openOMSI-org/openOMSI/pull/693)).

### Fixes

- Traffic-light programs that use conditional backwards jumps to extend a phase no longer repeat the same jump indefinitely and get stuck on one signal combination. The extension is replayed once before the normal cycle continues ([#692](https://github.com/openOMSI-org/openOMSI/pull/692)).

## 0.1.800 - 2026-10-02

### Multiplayer

- **A dedicated server can optionally expose `GET /players` for live web maps.** The JSON list contains each player's name, bus, line, destination, tour, position, heading and speed, and latitude/longitude on maps with `[worldcoordinates]` ([#674](https://github.com/openOMSI-org/openOMSI/pull/674)).
- Player position sharing is off by default and must be enabled with `share_positions = 1` in `server.cfg`; otherwise `/players` returns 404 ([#674](https://github.com/openOMSI-org/openOMSI/pull/674)).
- A player on foot is reported at the walker's position, and the walker is preferred over a parked bus when both exist ([#674](https://github.com/openOMSI-org/openOMSI/pull/674)).

## 0.1.796 - 2026-10-01

### New and improved

- **Enhanced graphics can reflect buses, buildings and scenery in wet-road puddles.** Reflections follow the local road height, slope and camber, and nearby articulated bus sections are included ([#686](https://github.com/openOMSI-org/openOMSI/pull/686)).
- Puddle reflections use bounded half-resolution screen-space rendering and are skipped on dry roads, full snow, mirror views, OpenGL and when reflections are disabled, limiting their cost when they are not needed ([#686](https://github.com/openOMSI-org/openOMSI/pull/686)).
- Add bounded puddle reflections with correct road alignment

## 0.1.791 - 2026-10-01

### Fixes

- DirectInput now finds generic controllers and button boxes with no axes, including custom Arduino, Pro Micro and STM32 devices, instead of silently leaving them disconnected. Keyboards, mice and screen pointers remain filtered out ([#71](https://github.com/openOMSI-org/openOMSI/pull/71), [#662](https://github.com/openOMSI-org/openOMSI/pull/662)).
- Holding both opposite steering keys keeps the wheel at its current position, as in OMSI, instead of always giving the left key priority. Releasing either key immediately continues steering in the remaining direction ([#663](https://github.com/openOMSI-org/openOMSI/pull/663)).
- `Toggle game controllers` follows its configured key binding instead of also having a hard-coded `K`. Leaving the action unbound now frees `K`, and rebinding it to another key works as expected ([#649](https://github.com/openOMSI-org/openOMSI/pull/649), [#670](https://github.com/openOMSI-org/openOMSI/pull/670)).

## 0.1.785 - 2026-10-01

### New and improved

- **Updater, site and docs**: the repository's new home in the openOMSI-Project organization
- **Chore**: updated repo urls
- Passengers as in Omsi.exe: tasks, stops, boarding, tickets and animation rewritten

### Multiplayer

- **In LAN multiplayer, a client's bus can take the host's waiting passengers again.** They are claimed from the stop where the client's bus is listed and handed over as that client's waiting passengers.

### Project

- Repository, updater, website, documentation and release links now use the project's new `openOMSI-org/openOMSI` home after the repository moved to the openOMSI-Project organization.
- **README**: links to the openOMSI-Project organization (website, releases, badges)

## 0.1.782 - 2026-10-01

### Credits

- Passengers are rewritten after Omsi.exe: the same tasks (waiting, the bus coming, walking to the bus, to a place, to the exit, sitting), waiting places along the stop's platform by its length and side, a bus taken from 60 m out and only when its terminus is one of theirs, the nearest open door (or one with a button), the cabin's path network walked by its own routing tables, a free place reserved at random (none free: they stay behind), the ticket stamped or bought at the desk with the game's dialogue, and everybody out at the terminus. The made-up queues, door waits and aisle shuffling are gone: people no longer stand at a door for long or crowd into the bus.
- Passengers and people on foot are posed and animated as in OMSI (its joint angles, gait curves, stride and stoop) instead of with leg IK. Pedestrians on the pavements stay as they were.
- Timetable buses no longer set off with a made-up number of riders by the hour.

## 0.1.780 - 2026-10-01

### New and improved

- The left and right turn signals can be toggled by keys or wheel buttons (automatic cancelling still applies), and the driver's view can follow the steering with an adjustable angle and response ([#657](https://github.com/openOMSI-org/openOMSI/pull/657), [#646](https://github.com/openOMSI-org/openOMSI/pull/646)).
- Keyboard steering keeps openOMSI's defaults (self-centring, the non-linear keys): the OMSI defaults of [#488](https://github.com/openOMSI-org/openOMSI/pull/488) are turned back
- **Feat**: configure smooth steering-linked driver camera
- **Feat**: add bindable left and right indicator toggles

## 0.1.774 - 2026-10-01

### New and improved

- The combined throttle/brake axis puts the throttle on the right half, as OMSI does ([#577](https://github.com/openOMSI-org/openOMSI/pull/577)).
- A map's own AI cars load when their `.ovh` leaves the registration affixes out ([#644](https://github.com/openOMSI-org/openOMSI/pull/644)).

### Fixes

- `GetTTTerminusIndex` answers the depot terminus named like the trip's terminus (or -1), as OMSI does: the IBIS's automatic destination showed a wrong or random terminus ([#623](https://github.com/openOMSI-org/openOMSI/pull/623), [#545](https://github.com/openOMSI-org/openOMSI/pull/545)). Switching to free drive clears the bus's timetable values ([#623](https://github.com/openOMSI-org/openOMSI/pull/623)).
- **The cab's paper timetable is OMSI's**: one Courier New text, the stop names cut or dotted to 25 characters, 24 rows a column, one time a stop and the arrival/departure words of the game's language instead of German everywhere ([#629](https://github.com/openOMSI-org/openOMSI/pull/629)).
- A vJoy device no longer crashes the game when vibration starts ([#655](https://github.com/openOMSI-org/openOMSI/pull/655)).

### Credits

- People on foot step only onto surfaces up to 0.5 m higher, as in OMSI, and no longer onto low roofs or benches ([#600](https://github.com/openOMSI-org/openOMSI/pull/600)).
- Busy stops no longer drop the frame rate ([#656](https://github.com/openOMSI-org/openOMSI/pull/656)).

## 0.1.759 - 2026-10-01

### New and improved

- AI vehicles switch their lights on below OMSI's light value of 0.75, before the street lamps, and keep them on after the lamps go out ([#620](https://github.com/openOMSI-org/openOMSI/pull/620)).
- The street lamps come on below a light value of 0.6 as in OMSI, and each scenery object's `NightlightA` at its own threshold (a random 0.3-0.75 with `[NightMapMode]`) ([#620](https://github.com/openOMSI-org/openOMSI/pull/620)).
- The amount of street traffic follows the paths' `trafficdensity`; density-0 and no-car paths stay empty ([#625](https://github.com/openOMSI-org/openOMSI/pull/625), [#161](https://github.com/openOMSI-org/openOMSI/pull/161)).
- Fewer stutters when AI vehicles appear ([#641](https://github.com/openOMSI-org/openOMSI/pull/641)).
- Looking round in a view of the bus follows the cursor and scales with the field of view, as in OMSI ([#622](https://github.com/openOMSI-org/openOMSI/pull/622), [#398](https://github.com/openOMSI-org/openOMSI/pull/398)); the cursor shows up-down arrows while zooming ([#621](https://github.com/openOMSI-org/openOMSI/pull/621)).
- Force feedback direction is detected and saved per wheel ([#637](https://github.com/openOMSI-org/openOMSI/pull/637)).
- A model takes the ambient light with OMSI's white material ambient (or its `[matl_allcolor]`), not its diffuse colour ([#530](https://github.com/openOMSI-org/openOMSI/pull/530)).
- Coloured lamp sprites are blended as OMSI blends them and keep their colour ([#601](https://github.com/openOMSI-org/openOMSI/pull/601)).
- IBIS picks the route by the trip's stops ([#631](https://github.com/openOMSI-org/openOMSI/pull/631)); the information bar (Ctrl+Y) shows the passengers aboard ([#628](https://github.com/openOMSI-org/openOMSI/pull/628)).

### Fixes

- A timetable bus at the end of its tour's last trip leaves the road at once instead of driving on and queueing at the map's end ([#598](https://github.com/openOMSI-org/openOMSI/pull/598), [#536](https://github.com/openOMSI-org/openOMSI/pull/536)).
- The scene shader compiles on OpenGL and GLES again (black screen and crashes on some Android and Linux machines, [#617](https://github.com/openOMSI-org/openOMSI/pull/617), [#610](https://github.com/openOMSI-org/openOMSI/pull/610), [#556](https://github.com/openOMSI-org/openOMSI/pull/556), [#456](https://github.com/openOMSI-org/openOMSI/pull/456)).

### Multiplayer

- A server seeds bus-stop passengers near every player ([#626](https://github.com/openOMSI-org/openOMSI/pull/626)).

## 0.1.737 - 2026-10-01

### New and improved

- A passenger's stop request fires `int_haltewunsch` as in OMSI, not the cab's stop button triggers, which played the driver's switch and brake sounds on some buses ([#569](https://github.com/openOMSI-org/openOMSI/pull/569)).
- A light's sprite keeps its own colours, tinted by the light's colour ([#601](https://github.com/openOMSI-org/openOMSI/pull/601)).
- A weather chosen from the menu comes at once, as in OMSI, and the clouds drift on smoothly while a weather blends in ([#609](https://github.com/openOMSI-org/openOMSI/pull/609)).
- The ground is the floor at the walker's level, not a stop's roof over it ([#600](https://github.com/openOMSI-org/openOMSI/pull/600)).
- The pause notice appears once and is translated ([#616](https://github.com/openOMSI-org/openOMSI/pull/616)).

### Fixes

- A triggered sound keeps its loudest volume while it plays, as in OMSI: door sounds are no longer cut off when the door stops moving ([#473](https://github.com/openOMSI-org/openOMSI/pull/473), [#611](https://github.com/openOMSI-org/openOMSI/pull/611), [#542](https://github.com/openOMSI-org/openOMSI/pull/542)).
- The map's path rules `bus` and `trucks` open a path to AI buses and trucks by their `ai_veh_type`, as in OMSI: trucks no longer avoid exactly the roads marked for them, and bus roads are no longer closed to cars ([#612](https://github.com/openOMSI-org/openOMSI/pull/612)).
- Every camera clips at 0.1 m as in OMSI: a wide field of view no longer cuts into the cab ([#587](https://github.com/openOMSI-org/openOMSI/pull/587)).
- `STLoadTex`/`STNewTex` size a script texture as D3DX does (the bitmap's power of two, stretched), and a resized one no longer keeps the old picture's memory ([#607](https://github.com/openOMSI-org/openOMSI/pull/607)).
- Gamepad triggers on Windows are no longer read as buttons, and the D-pad works ([#602](https://github.com/openOMSI-org/openOMSI/pull/602)).
- No "missing pack" for a part the game finds from the vehicle's own folders ([#582](https://github.com/openOMSI-org/openOMSI/pull/582)).

## 0.1.721 - 2026-10-01

Pull requests from the community: [#455](https://github.com/openOMSI-org/openOMSI/pull/455), [#560](https://github.com/openOMSI-org/openOMSI/pull/560), [#563](https://github.com/openOMSI-org/openOMSI/pull/563), [#566](https://github.com/openOMSI-org/openOMSI/pull/566), [#570](https://github.com/openOMSI-org/openOMSI/pull/570), [#580](https://github.com/openOMSI-org/openOMSI/pull/580), [#583](https://github.com/openOMSI-org/openOMSI/pull/583), [#585](https://github.com/openOMSI-org/openOMSI/pull/585), [#588](https://github.com/openOMSI-org/openOMSI/pull/588),

### New and improved

- AI buses at a stop on the left open their left doors ([#592](https://github.com/openOMSI-org/openOMSI/pull/592)).
- Mods can be imported from .7z and RAR archives as well ([#570](https://github.com/openOMSI-org/openOMSI/pull/570)).
- Better vehicle selection with a livery preview ([#583](https://github.com/openOMSI-org/openOMSI/pull/583)); the fleet number is set before the bus starts up ([#560](https://github.com/openOMSI-org/openOMSI/pull/560)).
- The launcher gives its graphics device up while a game runs ([#599](https://github.com/openOMSI-org/openOMSI/pull/599)).
- Interior and exterior temperatures in the HUD and the navigator ([#605](https://github.com/openOMSI-org/openOMSI/pull/605)), stop requests on the minimap ([#604](https://github.com/openOMSI-org/openOMSI/pull/604)), better roads and stop markers in the navigator ([#563](https://github.com/openOMSI-org/openOMSI/pull/563)).
- Route numbers that start with a letter (X10, M48, N9) on the destination display ([#588](https://github.com/openOMSI-org/openOMSI/pull/588)).
- Scripted LED traffic lamps get their materials, and a same-name DDS texture is preferred ([#566](https://github.com/openOMSI-org/openOMSI/pull/566)); `[matl_freetex]` paths relative to the OMSI folder work ([#580](https://github.com/openOMSI-org/openOMSI/pull/580)).
- VR: the mirrors can refresh faster, up to every frame ([#608](https://github.com/openOMSI-org/openOMSI/pull/608)).
- A map's `humans.txt` may name people in nested pack folders ([#455](https://github.com/openOMSI-org/openOMSI/pull/455)).

### Fixes

- AI cars no longer wait at junctions without traffic lights for no reason ([#591](https://github.com/openOMSI-org/openOMSI/pull/591)).
- Force feedback fixes and improvements ([#585](https://github.com/openOMSI-org/openOMSI/pull/585)); manual gear buttons can be held, letting go returns to neutral ([#570](https://github.com/openOMSI-org/openOMSI/pull/570)).
- **On foot**: walking stays on the bus's right level ([#594](https://github.com/openOMSI-org/openOMSI/pull/594)); a dedicated server updates the passenger density and the time of day every tick ([#597](https://github.com/openOMSI-org/openOMSI/pull/597)).

## 0.1.623 - 2026-10-01

### New and improved

- With the default keys plain Left/Right switch the interior camera as in OMSI (A/D steer; the "Arrow keys only" preset still steers with them), and the camera keys go on into the rear section's driver cameras of an articulated bus ([#519](https://github.com/openOMSI-org/openOMSI/pull/519), [#525](https://github.com/openOMSI-org/openOMSI/pull/525), [#464](https://github.com/openOMSI-org/openOMSI/pull/464)).
- The viewpoint key steps through the driver, passenger, outside and map views as in OMSI ([#519](https://github.com/openOMSI-org/openOMSI/pull/519)).
- **Wheels**: the scripts' shaking (`FF_Vib_Amp`, `FF_Vib_Period`) plays as a real periodic force-feedback effect ([#501](https://github.com/openOMSI-org/openOMSI/pull/501)).
- Head tracking on Windows also reads opentrack's freetrack output (TrackIR protocol) ([#522](https://github.com/openOMSI-org/openOMSI/pull/522)).
- The fleet number can be picked from the bus's number list before driving (also in a duty and with `--number`), as in OMSI's vehicle dialog ([#538](https://github.com/openOMSI-org/openOMSI/pull/538), [#133](https://github.com/openOMSI-org/openOMSI/pull/133)).
- A `[matl_lightmap]` on a variable the bus does not have is always lit, as in OMSI, not always dark (dashboard lamps, [#475](https://github.com/openOMSI-org/openOMSI/pull/475), [#352](https://github.com/openOMSI-org/openOMSI/pull/352), [#231](https://github.com/openOMSI-org/openOMSI/pull/231)).
- `[surface]` objects stand on the terrain like every other object without `[absheight]`, as in OMSI ([#531](https://github.com/openOMSI-org/openOMSI/pull/531), [#453](https://github.com/openOMSI-org/openOMSI/pull/453), [#460](https://github.com/openOMSI-org/openOMSI/pull/460), [#417](https://github.com/openOMSI-org/openOMSI/pull/417)).
- Terrain-mapped textures of a map's own folder win over global seasonal copies ([#548](https://github.com/openOMSI-org/openOMSI/pull/548)).
- On a Windows with a Chinese, Japanese or Korean code page, content text is read in that code page, as OMSI does.

### Fixes

- Passengers see the doors open on buses whose script sets `PAX_Entry`/`PAX_Exit` without listing them in its varlist (stock MAN NL, MAN NLC, many Citaros), and on 3-door buses the middle and rear doors let people out again; `door<i>` without the underscore works too ([#532](https://github.com/openOMSI-org/openOMSI/pull/532), [#535](https://github.com/openOMSI-org/openOMSI/pull/535), [#512](https://github.com/openOMSI-org/openOMSI/pull/512), [#537](https://github.com/openOMSI-org/openOMSI/pull/537), [#179](https://github.com/openOMSI-org/openOMSI/pull/179), from [#551](https://github.com/openOMSI-org/openOMSI/pull/551)).
- They walk to the nearest open door or one with a request button, as OMSI does, and switch when the driver opens a nearer one, instead of all crowding the front door ([#535](https://github.com/openOMSI-org/openOMSI/pull/535)).
- Waiting at a door that is still shut, they press the request and stay, and the bus creeping a metre or two to line up no longer sends them back to the shelter ([#532](https://github.com/openOMSI-org/openOMSI/pull/532), [#512](https://github.com/openOMSI-org/openOMSI/pull/512)).
- Painted ground no longer darkens through stacked layers at shallow angles, and painted edges no longer shift with the texture's mip level ([#552](https://github.com/openOMSI-org/openOMSI/pull/552)).
- Stairs and excavations of presurface objects are no longer filled by the terrain ([#526](https://github.com/openOMSI-org/openOMSI/pull/526)), and declared `[terrainhole]`s cut railway cuttings and underpasses however deep ([#550](https://github.com/openOMSI-org/openOMSI/pull/550)).
- Random AI cars at a dead end or the map edge leave at once, as in OMSI, instead of standing there and blocking the cars behind ([#536](https://github.com/openOMSI-org/openOMSI/pull/536), [#327](https://github.com/openOMSI-org/openOMSI/pull/327)).

## 0.1.592 - 2026-10-01

### New and improved

- As many people wait at a stop as the map's passenger counts for it and the passengers setting give, up to its waiting places, as in OMSI. Before, never more than seven: 200% changed nothing ([#458](https://github.com/openOMSI-org/openOMSI/pull/458), from [#513](https://github.com/openOMSI-org/openOMSI/pull/513)).
- Game controller axes use their characteristic from gamectrler.cfg (progressive, degressive, bi-..., range extension), as in OMSI; the launcher's controller page sets it. Before, a G25 set to bi-progressive steered linearly ([#479](https://github.com/openOMSI-org/openOMSI/pull/479)).
- The hints for a bus that does not move name the player's own keys ([#461](https://github.com/openOMSI-org/openOMSI/pull/461)).
- Sun shadows stay until the sun is about a degree below the horizon, as in OMSI (they went at 4.6°), and overcast weather or fog under 350 m casts none ([#518](https://github.com/openOMSI-org/openOMSI/pull/518), [#506](https://github.com/openOMSI-org/openOMSI/pull/506)).
- Route arrows cast no shadow ([#508](https://github.com/openOMSI-org/openOMSI/pull/508)).

### Fixes

- People getting off wait at the exit they chose when they pressed the stop button, as in OMSI, instead of running to whichever door opens first, usually the front one ([#493](https://github.com/openOMSI-org/openOMSI/pull/493), [#336](https://github.com/openOMSI-org/openOMSI/pull/336)).
- Plain Left/Right switch the interior camera again unless a wheel steers ([#464](https://github.com/openOMSI-org/openOMSI/pull/464), [#519](https://github.com/openOMSI-org/openOMSI/pull/519)).
- Scenery text textures (street signs) are lit like their object and no longer glow at night ([#470](https://github.com/openOMSI-org/openOMSI/pull/470)).
- A crash in the interface's texture cleanup ([#534](https://github.com/openOMSI-org/openOMSI/pull/534)).

### Multiplayer

- P pauses and resumes without opening the menu; a LAN session cannot be paused ([#527](https://github.com/openOMSI-org/openOMSI/pull/527)).

## 0.1.576 - 2026-10-01

### New and improved

- The weather's METAR airport can be typed as an ICAO code ([#492](https://github.com/openOMSI-org/openOMSI/pull/492)).
- In-game interface size and opacity, and the notes in the corner, are settings; sharper text; the pause menu's Options list one line a setting ([#446](https://github.com/openOMSI-org/openOMSI/pull/446)).
- The in-cab schedule sheet shows the player's timetable ([#491](https://github.com/openOMSI-org/openOMSI/pull/491)).
- Dashboard lamps built as `[visible]` meshes with a plain `[matl_nightmap]` light up by day as well, as in OMSI ([#507](https://github.com/openOMSI-org/openOMSI/pull/507)).
- The driver figure's arms and the hand on a manual gear lever ([#465](https://github.com/openOMSI-org/openOMSI/pull/465)).
- Surface objects modelled away from their origin (bridges) stand at the road's height ([#496](https://github.com/openOMSI-org/openOMSI/pull/496)); scenery `[matl_freetex]` works without a script ([#474](https://github.com/openOMSI-org/openOMSI/pull/474)).

### Fixes

- People no longer freeze at an open door before boarding ([#498](https://github.com/openOMSI-org/openOMSI/pull/498)).
- Buses whose scripts set `PAX_Entry0_Open` for the front door only (the SD200, SD202) let people out of the rear doors again: an exit without a `PAX_Exit` variable follows its `door_<i>` ([#486](https://github.com/openOMSI-org/openOMSI/pull/486)).
- Steering wheels (G27, G29, Driving Force GT...): the pedals no longer act as buttons and buttons no longer fire twice ([#480](https://github.com/openOMSI-org/openOMSI/pull/480)).
- AI-only lines are no longer offered for driving ([#483](https://github.com/openOMSI-org/openOMSI/pull/483)).
- **LED panels**: a "LED mip strength" slider (0-4) instead of the on/off switch ([#490](https://github.com/openOMSI-org/openOMSI/pull/490)).
- `[terrainmapping]` takes the map's first ground texture only, as Omsi.exe does; ground lighting and tile seams fixed ([#436](https://github.com/openOMSI-org/openOMSI/pull/436)).
- VR: the bus mirrors are no longer black when looking round ([#487](https://github.com/openOMSI-org/openOMSI/pull/487)).
- AI cyclists ride at 15-21 km/h and no longer show green boxes on the rider ([#502](https://github.com/openOMSI-org/openOMSI/pull/502)).

## 0.1.538 - 2026-10-01

### New and improved

- A TGA picture under a `.png`, `.bmp` or `.jpg` name is read by its content, as OMSI does; it used to leave the slot without a texture ([#414](https://github.com/openOMSI-org/openOMSI/pull/414)).
- New setting "Dynamic steering" (Driving tab), OMSI's `redSteerSpd`: the steering keys and the wheel's return to centre slow down with speed ([#347](https://github.com/openOMSI-org/openOMSI/pull/347)).
- **Camera tab**: "Right mouse button turns the view" switched off gives OMSI's own mouse: the right button zooms (up widens the cab view, backs the outside camera away) and the wheel button turns the view. On (the default) right-drag looks round as before and Shift+right zooms ([#398](https://github.com/openOMSI-org/openOMSI/pull/398), [#104](https://github.com/openOMSI-org/openOMSI/pull/104)).
- Passenger footsteps in a bus use the step sounds its `paths.cfg` gives each walkway: on the SD200 the stairs sound like stairs and the upper deck has its own floor. Before, every step picked from all of OMSI's floor samples ([#311](https://github.com/openOMSI-org/openOMSI/pull/311)).
- The bus's outside sounds (the SD200's exterior engine) are heard from the cab through an open door or window, at the level the bus script gives (`Snd_OutsideVol`), as in OMSI.

### Fixes

- **Enhanced**: flipdot destination displays (the stock MAN NL/NG Krueger matrix, the flipdot variants of the NEOMAN and Sprinter mods), whose light map is a picture of their dots, no longer glow and bloom like LED panels. LED panels (a plain white light map) still glow, and go dark when the bus's power or lights are switched off ([#413](https://github.com/openOMSI-org/openOMSI/pull/413)).
- **Enhanced**: the mirrors are no longer far darker than the view through the windscreen at dusk and in daytime rain; they dim only once the sun is below the horizon ([#432](https://github.com/openOMSI-org/openOMSI/pull/432)).
- The driver figure is lit by the four interior lamps of its seat, as OMSI lights a seated person - coloured, by distance and from the lamp's side - instead of a flat glow of every lamp in the bus that left it overexposed ([#206](https://github.com/openOMSI-org/openOMSI/pull/206)).
- `[matl_texadress_mirror]` and `[matl_texadress_mirroronce]` mirror the textures beyond their edges as in OMSI, instead of smearing the edge ([#145](https://github.com/openOMSI-org/openOMSI/pull/145)).
- A tile has water only when its `.map` says `[water]`: a leftover `.water` file no longer floods a tile whose water was removed ([#414](https://github.com/openOMSI-org/openOMSI/pull/414)).
- The cab no longer plays a made-up low engine rumble under the bus's own sounds: OMSI makes no vehicle sound of its own.
- Script callbacks round their arguments to the nearest whole number as OMSI does, instead of cutting them off: a terminus code of 1100.9999 out of a script's arithmetic now finds terminus 1101, not 1100 ([#312](https://github.com/openOMSI-org/openOMSI/pull/312), [#197](https://github.com/openOMSI-org/openOMSI/pull/197)).

### Credits

- `[walk_param]` is read as OMSI reads it (stride, then arm angle), and people walk at OMSI's 1.1 m/s ± 0.2: the stride was taken for a speed ([#393](https://github.com/openOMSI-org/openOMSI/pull/393)).

## 0.1.523 - 2026-10-01

### New and improved

- **Letting go of a key fires only that key's own `_off` trigger, as in Omsi.exe.** On some mods it fired an alias's release (e.g. `parking_brake_mouse_off`, `kw_blinker_*_off`), which undid the parking brake or the turn signal just set with the keyboard ([#420](https://github.com/openOMSI-org/openOMSI/pull/420)).
- The hand cursor appears over every clickable part of the cockpit, including large ones such as door leaves, the cockpit door and the steering column ([#411](https://github.com/openOMSI-org/openOMSI/pull/411)).
- **The odometer starts at a used bus's reading, as in OMSI**: from the year and the km a year in `[kmcounter_init]` (1980 and 60000 when a bus has none), ±20% from bus to bus. Before, such buses showed 000000; reversing now also takes the counter back ([#305](https://github.com/openOMSI-org/openOMSI/pull/305)).
- **Number plates**: a repaint's own `[registration_list]` plate is used for AI buses whatever the template says afterwards; the player's bus takes it as the vehicle dialog does. Before, they showed the automatic prefix and the fleet number ([#133](https://github.com/openOMSI-org/openOMSI/pull/133)).
- A `[matl_freetex]` declared inside a `[matl_item]` changes the powered material only, and named material references in `.x` models are resolved (from [#436](https://github.com/openOMSI-org/openOMSI/pull/436)).
- Crossings draped over the ground (`[crossing_heightdeformation]`) get normals rebuilt from their faces, as Omsi.exe does: some junctions looked as if their faces were turned inside out ([#428](https://github.com/openOMSI-org/openOMSI/pull/428)).
- `[important]` sounds keep their place when more sounds play than OMSI's `[sound_maxcount]` of 200 ([#434](https://github.com/openOMSI-org/openOMSI/pull/434)).
- **Mirrors can be switched off (Settings, Graphics, Mirrors**: Off) on slower machines ([#433](https://github.com/openOMSI-org/openOMSI/pull/433)).
- **HTML pages**: `omsi.getDepartures(stop)` lists the next departures of a stop, and `omsi.setNextStop(index)` can also go back to an earlier stop ([#437](https://github.com/openOMSI-org/openOMSI/pull/437)).

### Fixes

- In mouse steering the cursor sets the pedals every frame, as in OMSI: a brake held on the keyboard (with "The keyboard brake stays on until the throttle") no longer stays on under the mouse ([#395](https://github.com/openOMSI-org/openOMSI/pull/395)).
- **Articulated buses**: the rear section stops at the joint's maximum angle from `[coupling_front_character]` (52.5° on the GN92) instead of folding through the front section when turning tightly or reversing ([#410](https://github.com/openOMSI-org/openOMSI/pull/410)).
- The passenger cameras of an articulated bus's rear section hang on its body like the front section's: `[add_camera_pax]` distance, pitch and roll ([#174](https://github.com/openOMSI-org/openOMSI/pull/174), [#126](https://github.com/openOMSI-org/openOMSI/pull/126)).
- `[matl_change]`: the first `[matl_item]` is made of its own block only, no longer of the later items' maps (an LED matrix showed its script texture at value 1) ([#210](https://github.com/openOMSI-org/openOMSI/pull/210)).
- The content folders and the OMSI 2 installation get a `.nomedia` file, so the gallery no longer lists thousands of textures as photos - the media scan kept phones busy and people deleted the "pictures", leaving buses white ([#443](https://github.com/openOMSI-org/openOMSI/pull/443)).

### Multiplayer

- Numpad ÷ opens the front door again instead of the chat line ([#130](https://github.com/openOMSI-org/openOMSI/pull/130)).

## 0.1.486 - 2026-10-01

### New and improved

- PBR maps for the textures a tile's preparation brings (nearly all of a map's), and found in every content root's copy of the folder
- **Launcher settings in six tabs**: Graphics, Driving, Camera, Sound, Gameplay, General

### Fixes

- The Settings page is split into six tabs - Graphics, Driving, Camera, Sound, Gameplay, General - each fitting the window, instead of about ninety controls in three long columns; on a phone too ([#430](https://github.com/openOMSI-org/openOMSI/pull/430)).

## 0.1.483 - 2026-10-01

### New and improved

- PBR maps (`_nn`, `_rr`, `_ao`, ...) work on map textures: roads, splines and scenery objects were drawn flat - only the textures loaded on the spot got their maps, not the ones a tile's preparation brought, which are nearly all of a map's. Maps put into the same folder in the openOMSI content folder, beside a texture of the OMSI install, are found too.

## 0.1.481 - 2026-10-01

### New and improved

- Holding both mouse buttons and moving the mouse zooms as in OMSI (its "M_Zoom"): up zooms in on the dashboard, down back out to the seat's view; outside, the camera moves further away or closer. It did nothing - the right button only looked round.

### Fixes

- Pressing the right button while a switch or lever is held with the left one no longer turns the view and stops the drag.
- **Both mouse buttons held**: OMSI's mouse zoom (in the bus) and camera distance (outside); a right press no longer stops a switch drag

## 0.1.479 - 2026-10-01

### New and improved

- **Spline rows**: a tangential object turns in the spline's inclined frame (from [#409](https://github.com/openOMSI-org/openOMSI/pull/409))
- **OpenGL (Android)**: no ambient-occlusion pipelines, whose depth reads GLSL cannot compile ([#422](https://github.com/openOMSI-org/openOMSI/pull/422))

### Fixes

- Objects placed along a spline that follow its slope and cant (railings, posts, signs, lights) turn within the spline's inclined surface: one turned sideways to the road no longer leans across it and off the ground on a sloped or canted street, and one on a chain running backwards no longer tips downhill (from [#409](https://github.com/openOMSI-org/openOMSI/pull/409)).
- No "fs_blur" shader error on OpenGL devices: the ambient occlusion pipelines, which OpenGL ES cannot compile, are left out there - AO stays available on Vulkan ([#422](https://github.com/openOMSI-org/openOMSI/pull/422)).

## 0.1.476 - 2026-10-01

### New and improved

- Mouse steering sensitivity can be set in the pause menu's Options (Mouse steering sensitivity + / -, 10% to 300%; 100% is OMSI's), and the launcher's slider goes as far.
- The log says how each game controller came in (its layout, DirectInput or the system's) and, the first time a stick or axis moves, whether it steers - for reports of sticks that do nothing.

### Multiplayer

- The game and the dedicated server start on a PC without the Visual C++ Redistributable: its runtime DLLs ship beside `openomsi.exe` ("The code execution cannot proceed because VCRUNTIME140_1.dll was not found").

## 0.1.471 - 2026-10-01

### New and improved

- **The wheels roll on the road where it is drawn**: splines and `[surface]` objects are drawn 8 cm over their authored height and the bus now stands on them there, not 8 cm into the asphalt ([#421](https://github.com/openOMSI-org/openOMSI/pull/421)).

### Fixes

- Buses no longer fall through the road at junctions over a buried embankment slope (Cotterell, the junction by the park): a road face lying more than a metre under the drawn ground beneath the wheel is no road there, and the bus stands on the ground, as with Omsi.exe's highest-face query ([#424](https://github.com/openOMSI-org/openOMSI/pull/424), [#423](https://github.com/openOMSI-org/openOMSI/pull/423)).
- An object stored in a neighbouring tile's file, past its own tile's edge, stands on the ground under it instead of the height at its tile's border ([#421](https://github.com/openOMSI-org/openOMSI/pull/421)).

## 0.1.463 - 2026-10-01

### Fixes

- With the door release on, the SD200's automatic rear door no longer opens on its own while riders board at the front: riders no longer press the outside door opener, which Omsi.exe never does - they only ask for a door through `PAX_Entry<n>_Req` ([#416](https://github.com/openOMSI-org/openOMSI/pull/416), [#415](https://github.com/openOMSI-org/openOMSI/pull/415)).

### Project

- Pull requests get a template, and ones opened from a `main` branch or an organisation account are closed with a note ([#418](https://github.com/openOMSI-org/openOMSI/pull/418)).

## 0.1.455 - 2026-10-01

### New and improved

- **Emergency vehicles have right of way at crossings**: a vehicle whose script sets `TrafficPriority` goes before the others and they give way to it, as Omsi.exe does for any vehicle, not only the player's bus ([#356](https://github.com/openOMSI-org/openOMSI/pull/356)).
- Timetable buses stop at the stop on their own side of the road: on a route back along the same street, they stopped at the stop across the road on the way out. Stops are matched in the trip's order, on the lane they stand beside.
- Cards of up to 4 GB use the allocator's small memory blocks, and a texture budget larger than the card holds is taken down to its size: 2 GB cards lost their device to "out of memory" in the first frames ([#332](https://github.com/openOMSI-org/openOMSI/pull/332), [#295](https://github.com/openOMSI-org/openOMSI/pull/295), [#323](https://github.com/openOMSI-org/openOMSI/pull/323)).

### Fixes

- **Enhanced**: the far road and ground no longer go dark at grazing angles - what reflects nothing keeps its light, and a wet road reflects the sky ([#374](https://github.com/openOMSI-org/openOMSI/pull/374)).

## 0.1.451 - 2026-10-01

### New and improved

- **Wheels stay under their hub caps**: the drawn tyres are seated on the physical hub at the point they turn about (the rotation's `origin_trans`), not at the .o3d's own pivot. A tyre without a pivot (the NEOMAN's right front) was measured at a point circling the hub and moved up and down by centimetres as it turned, so its cap seemed to roll off it.
- Smooth camera transitions when changing and entering views, a setting in the pause menu as well ([#408](https://github.com/openOMSI-org/openOMSI/pull/408), by shloooo).
- The map's lamps leave `[tree]`s dark, as in OMSI 2 ([#407](https://github.com/openOMSI-org/openOMSI/pull/407), by Sulamufor).

### Fixes

- The terrain's light map lights the ground instead of glowing over it ([#406](https://github.com/openOMSI-org/openOMSI/pull/406), by Sulamufor).

## 0.1.438 - 2026-10-01

### New and improved

- The driver's hands in the cab view are a setting now (Settings and the pause Options, "Driver's hands in the cab view"), off by default.
- **Vanilla**: the envmap stage lerps in gamma, as the texture x light product does

### Fixes

- **Vanilla**: reflections blend in gamma like the rest of the classic picture; at night the MAN NL/NG instrument glass no longer lies milky white over the unlit gauges ([#401](https://github.com/openOMSI-org/openOMSI/pull/401), by Sulamufor).

## 0.1.434 - 2026-10-01

### New and improved

- Manual gearboxes told from automatics by what their gates do

### Fixes

- **An automatic gearbox is no longer taken for a manual one.** A bus counted as manual when its scripts answered to the gate keys (`kw_s_1`, `kw_s_2`) and read a `Clutch` anywhere - many automatics do both (gear hold keys, a torque converter's own clutch) - and the automatic clutch of the settings then worked their clutch at every stop and pull-away, and the phone showed a manual's gate. Now a gearbox is manual when it has the gates and no automatic's `automatic_D`, or when its first gate itself asks for the clutch, or when it works a clutch of its own through `AutoClutch` (checked on the LiAZ MKPP/GMP, the Sprinter G32/G-tronic, the SD202 and the NEOMAN A23).

## 0.1.433 - 2026-10-01

### New and improved

- **Scripts**: a trigger starts with 1 on its stack, as in OMSI - a trigger guarded by a bare `{if}` did nothing (the S315 UL-GT's ticket printer switch) ([#388](https://github.com/openOMSI-org/openOMSI/pull/388), by hannsadrian).
- The interior camera glides between viewpoints (OMSI's `driverview_smooth`, a setting) ([#388](https://github.com/openOMSI-org/openOMSI/pull/388)).
- The driver's hands are seen in the cab view, the rest of the figure folded away ([#376](https://github.com/openOMSI-org/openOMSI/pull/376), by Neblina666).

### Fixes

- Enhanced graphics are there again on every device and graphics API (0.1.402 left them out on phones and OpenGL). They are built whenever Enhanced is chosen; only a phone or OpenGL device not set to Enhanced skips compiling them, as it never draws them - that compile is what killed Mali and Adreno drivers at the start.

### Project

- The Atron ticket machine shows its stop text and keeps its sales screen ([#390](https://github.com/openOMSI-org/openOMSI/pull/390), by TruckiHD).
- Brazilian Portuguese improved, European Portuguese added ([#396](https://github.com/openOMSI-org/openOMSI/pull/396), by isaacsa2).

## 0.1.402 - 2026-09-30

### New and improved

- A `[matl_change]` with several `[matl_item]`s shows item n at value n, as Omsi.exe does: only the first was kept and shown at 1, so e.g. the MAN New Lion's City's door buttons (2 = lit while the door is open) stayed dark ([#352](https://github.com/openOMSI-org/openOMSI/pull/352)).
- `[animparent]` hangs a mesh on the last mesh before it that carries the name, as Omsi.exe resolves it while reading: door variants reusing their arms' names moved the later variant's leaves with the first one's arm (Solaris Urbino III "Bode new", [#348](https://github.com/openOMSI-org/openOMSI/pull/348)).
- Station displays and other scenery text show a string that arrives after their first frame (they were only redrawn on `Refresh_Strings`, [#367](https://github.com/openOMSI-org/openOMSI/pull/367)).
- A throttle pedal takes off a brake the keyboard holds, as the throttle key does: the bus was driven against its brakes ([#377](https://github.com/openOMSI-org/openOMSI/pull/377)).
- **Phones and OpenGL leave out the Enhanced graphics' pipelines**: they are never drawn there, and compiling the ray-marched clouds' sky killed Mali and Adreno drivers before the first frame ([#364](https://github.com/openOMSI-org/openOMSI/pull/364), [#333](https://github.com/openOMSI-org/openOMSI/pull/333), [#316](https://github.com/openOMSI-org/openOMSI/pull/316), [#371](https://github.com/openOMSI-org/openOMSI/pull/371)).

### Fixes

- **Rain on the windows**: drops no longer run down in lanes of wavy lines all at once. Now and then a single drop breaks loose, slides a few centimetres to a hand's width in jerks, nearly straight with a little drift, and stops again - each at its own moment.
- **Cars change only onto lanes open to their own traffic group**: trucks no longer take the cycle paths beside a road ([#327](https://github.com/openOMSI-org/openOMSI/pull/327)); trucks keep to the speed limit (up to 80-90 km/h, it was 38-47 on every road), bicycles ride at 20-28 km/h.
- In multiplayer the host has traffic round itself again ([#342](https://github.com/openOMSI-org/openOMSI/pull/342)).
- **A crash report takes its title from the run itself**: an error before the game started, or one the game got over, titled reports of games that died much later ([#381](https://github.com/openOMSI-org/openOMSI/pull/381), [#331](https://github.com/openOMSI-org/openOMSI/pull/331)).

## 0.1.400 - 2026-09-30

### New and improved

- Linux builds link with lld ([#384](https://github.com/openOMSI-org/openOMSI/pull/384), by no-felix).
- **Ci(linux)**: use clang and lld linker for release builds

### Fixes

- **Macs and phones**: a fixed render scale keeps to the same pixel budget as the automatic one on high-resolution screens, the navigator draws without multisampling, the picture's depth is not written back where nothing reads it, and two frames are in flight so the graphics chip works while the next frame is prepared (M1 Pro, Thüringer Wald: 20 to 57 fps; one frame more of input delay with V-sync) ([#385](https://github.com/openOMSI-org/openOMSI/pull/385), by hannsadrian).
- Two frames in flight on vsync, the Mac pixel budget caps a fixed render scale, picture depth not stored when nothing reads it, the navigator every other frame at 1x MSAA

## 0.1.394 - 2026-09-30

### New and improved

- **A clutch pedal pushed to the floor is fully in**: a wheel's pedal reads 0.93-0.99 there, and the LiAZ/PAZ gearboxes part the engine from the wheels only above 0.95 and take a gear only at 1 - holding the clutch at a stop still stalled the engine. All pedals' last few per cent count as their ends; the phone's clutch is in from three quarters down.
- Riders who stood up as the bus pulled in get off at their stop: they lost "this is my stop" as the bus came to a stand and stayed at the door ([#336](https://github.com/openOMSI-org/openOMSI/pull/336)).
- Controller buttons can pause, take a screenshot, quicksave and switch mouse steering or the controllers ([#380](https://github.com/openOMSI-org/openOMSI/pull/380), by isaacsa2); DirectInput devices with unusual layouts (button boxes without axes) are taken ([#379](https://github.com/openOMSI-org/openOMSI/pull/379), by isaacsa2).
- Text fields in the launcher can be clicked into, selected and overwritten ([#373](https://github.com/openOMSI-org/openOMSI/pull/373), by XiZyno).
- Translations completed and corrected, Traditional Chinese in full ([#383](https://github.com/openOMSI-org/openOMSI/pull/383), by EFour4).

### Fixes

- Rain drops on the windows are smaller again (the big ones of 0.1.381 were far too big and lumpy), and a drop running down leaves only a cleared track and a few beads - no more thin tail drawn behind it.
- Riders of timetable buses no longer lose their stop whenever the player's bus stops somewhere ([#317](https://github.com/openOMSI-org/openOMSI/pull/317)); somebody held off the exit by a pole gets off from there.
- The dedicated server starts on machines without a graphics card again ([#375](https://github.com/openOMSI-org/openOMSI/pull/375), by no-felix; [#368](https://github.com/openOMSI-org/openOMSI/pull/368)).

## 0.1.381 - 2026-09-30

### New and improved

- Rain on the bus windows looks like real drops, in all three graphics modes (Vanilla had OMSI's sliding texture until now):
  - every drop is a lens: it shows the street behind the glass through itself, small and upside down, with the sky at its bottom (from the last frame's picture);
  - drops come in four sizes up to over a centimetre, with uneven rims, the heavy ones drawn out downwards, and clear glass between them;
  - a runner leaves a thin stream of water in its track;
  - while the bus drives, the airstream takes the runners up and out across the windscreen and back along the side windows, harder the faster it goes. It costs about half a millisecond at 1080p.
- Rain drops on the windows show the street through themselves

## 0.1.380 - 2026-09-30

### New and improved

- **The pause menu's Options are kept**: most switches (collisions, camera collisions, view turning with the steering, force feedback, keyboard brake hold, automatic clutch, head tracking) were written in a form the settings file read as "not set" and came back at their defaults, and LED glow and LED mipmaps were not saved at all.
- Somebody at the front of a queue whom a railing, pole or shelter wall holds off the door boards from where they stand; they stood a metre from the open door until the bus left.

### Fixes

- The launcher takes over the settings a game changed instead of writing its older copy back over them when it saved something of its own.

## 0.1.378 - 2026-09-30

### New and improved

- **Website**: the Your system badge on its own line, buttons aligned; changelog
- A character a font lacks is left out, as Omsi.exe does
- **Perf**: spread the frame's render preparation over the render thread pool

### Fixes

- **Busy maps run much faster**: the frame's render preparation (culling, shadow casters, draw lists, bundle recording) is spread over the render threads instead of one core, mirrors are drawn only when in view and at most 30 times a second, the navigator's map is redrawn at most 30 times a second, and the cab's hover pick tests only the triangles near the cursor. On St-Servan with traffic and passengers: 37 to 60 fps ([#369](https://github.com/openOMSI-org/openOMSI/pull/369), by ThiBot77).
- **Fix(font)**: do not draw glyphs for whitespace ([#360](https://github.com/openOMSI-org/openOMSI/pull/360))

### Project

- **Download**: the "Your system" badge no longer breaks across the card title, and the Download buttons line up.

## 0.1.374 - 2026-09-30

### New and improved

- Turning the wheel can turn the driver's view into the bend, as in OMSI's "look with the steering wheel" ([#363](https://github.com/openOMSI-org/openOMSI/pull/363), by shloooo).
- AI emergency vehicles sound their siren when something holds them up ([#357](https://github.com/openOMSI-org/openOMSI/pull/357), by Sulamufor).
- The game menu's scrollbar can be dragged ([#354](https://github.com/openOMSI-org/openOMSI/pull/354), by XiZyno).
- When the game was closed by the system (out of memory), the launcher says so and points to the settings that help ([#355](https://github.com/openOMSI-org/openOMSI/pull/355), by no-felix).
- A saved situation keeps each vehicle's livery; read-only content folders fall back to a writable place ([#365](https://github.com/openOMSI-org/openOMSI/pull/365), by no-felix).

### Fixes

- Vanilla lights textures through the sRGB curve instead of a plain power of 2.2, so dark colours are no longer crushed ([#343](https://github.com/openOMSI-org/openOMSI/pull/343), by Sulamufor).
- Small dashboard indicator lights are drawn again instead of being dropped as too small ([#346](https://github.com/openOMSI-org/openOMSI/pull/346), by no-felix).
- System gamepads on Windows can have their buttons bound again ([#358](https://github.com/openOMSI-org/openOMSI/pull/358), by EpixIXIx).
- A character a font lacks is left out, as Omsi.exe does (its glyph lookup gives none), instead of being drawn as the font's first glyph; spaces keep their width. The MAN Lion's City's odometer and trip meter lose the `|` in front of them ([#370](https://github.com/openOMSI-org/openOMSI/pull/370), by no-felix; [#360](https://github.com/openOMSI-org/openOMSI/pull/360)).

### Multiplayer

- **LAN protocol 6**: a vehicle's state carries up to 63 values, the rear section's sounds and what is seen first come first, and INFO messages are sent at most four times a second ([#353](https://github.com/openOMSI-org/openOMSI/pull/353), with the updated [#338](https://github.com/openOMSI-org/openOMSI/pull/338) and [#334](https://github.com/openOMSI-org/openOMSI/pull/334), by Jaja80330). Players and servers need this version together.

### Project

- **Download**: the Windows button downloaded the dedicated server (its zip ends the same way). Every build now has its own button - Windows and Windows on ARM, macOS for Apple silicon and Intel, Linux and Linux on ARM, Android - the one for your system first, and the four dedicated servers apart below. Installing on macOS is explained too.

## 0.1.371 - 2026-09-30

### New and improved

- **Website**: every build has its own download button; changelog 0.1.371
- **Feat(launcher)**: clarify silent exit dialog and link to settings
- AI emergency vehicles sound their siren when something holds them up
- Restore button binding for system gamepads on Windows
- **Chore**: adjust steer look degrees from 12 to 30
- **Feat**: implement steer look
- **Vanilla**: multiply in gamma through the sRGB curve, not a power of 2.2

### Fixes

- **Fix(app)**: log LAN offline check as warning ([#314](https://github.com/openOMSI-org/openOMSI/pull/314))
- **Fix(scene)**: allow small dashboard indicator lights ([#164](https://github.com/openOMSI-org/openOMSI/pull/164))
- **Fix(fs)**: fallback to writable path on read-only fs ([#351](https://github.com/openOMSI-org/openOMSI/pull/351))
- **Fix(situation)**: preserve livery on saved vehicles ([#344](https://github.com/openOMSI-org/openOMSI/pull/344))
- Fix game menu scrollbar dragging

### Multiplayer

- **LAN protocol 6**: up to 63 values in a vehicle state
- **LAN**: the rear sections' sound variables last in the values list
- **LAN**: INFO at most four a second; what is seen first in the values list

### Project

- Every build has its own download button instead of matching files only by the end of their names. Windows ARM64, macOS Intel, Linux ARM64 and the dedicated-server builds are listed, the visitor's platform is shown first, and macOS installation is explained.

## 0.1.344 - 2026-09-30

### New and improved

- A wheel in the air stays where it hangs at rest (`Axle_Suspension`), as in OMSI.

### Fixes

- Braking and pulling away pitch the bus as much as in OMSI (the tyres' forces act about the hubs), and it no longer pitches at a standstill.
- **Turning the wheel no longer kicks the body into a roll**: while the tyres hold, the bus leans only by the bend's pull, as Omsi.exe does.

## 0.1.342 - 2026-09-30

### New and improved

- **Busy spline scenes cost far less CPU**: short static kerb, grass and pavement splines are drawn together per 48 m cell, and `[terrainmapping]` spline faces share the tile's ground materials ([#340](https://github.com/openOMSI-org/openOMSI/pull/340), by TruckiHD; for [#284](https://github.com/openOMSI-org/openOMSI/pull/284)).
- Document performance validation after upstream integration
- Optimize CPU rendering of spline-heavy map scenes

### Multiplayer

- Another player's articulated bus has its rear section lit, with its displays and its sounds ([#338](https://github.com/openOMSI-org/openOMSI/pull/338)); its roller blind shows the line number ([#334](https://github.com/openOMSI-org/openOMSI/pull/334)); a passenger in another player's bus hears it from inside ([#330](https://github.com/openOMSI-org/openOMSI/pull/330)) - all by Jaja80330. The network format changed with [#334](https://github.com/openOMSI-org/openOMSI/pull/334): players and servers need this version together.
- **LAN**: an articulated bus's rear section lit and heard in the others' games
- **LAN**: other players see a roller blind's line number
- **LAN**: a passenger in another player's bus hears it from inside

## 0.1.330 - 2026-09-30

### New and improved

- **Mods with physics of their own**: `Brakeforce` and every `Axle_Brakeforce_*` go back to 0 after the physics read them, as Omsi.exe clears them each frame before the scripts run - a script that brakes only now and then (a retarder, a stop brake, custom physics) no longer leaves the brakes on for good.
- **Timetable buses pull into the bay**: they move over to the `[busstop]` box as Omsi.exe moves them - the kerb-side flank 0.3 m past the box's centre, from the stop's docking distance (30 m) out - whether or not a path leads into the bay ([#241](https://github.com/openOMSI-org/openOMSI/pull/241)).

### Fixes

- **The driver's and the passengers' views ride with the bus**: their cameras hang on the body as Omsi.exe's do, pitching under braking and leaning in bends with the cab, the mouse look turned in the bus's frame. The level view with the cab rocking about it was most of the "boat" - the body's own heave, pitch and roll already settle as Omsi.exe's do.

## 0.1.328 - 2026-09-30

### New and improved

- **The wheels stand on the road**: the suspension's spring point is where Omsi.exe puts it, on the model's origin plane under each wheel, measured straight up. Measured from the hub less the `.bus` file's tyre radius, a mod whose tyre mesh is larger than that stood with its wheels sunk a few centimetres into the spline.
- **Enhanced**: chrome and metal parts of a vehicle - opaque, with a sphere map, not the body - are metal by their `[matl_envmap]` factor, as the vanilla picture shows them; their bump maps bend the reflection. The body stays paint unless it has a mask of its own.
- LED destination panels glow in the enhanced picture; "LED glow" and "LED masks keep their mipmaps" are settings ([#324](https://github.com/openOMSI-org/openOMSI/pull/324), by NACHN).

### Fixes

- People walking in the street no longer sound as if they walked on a bus floor (the passengers' step samples, `Sounds\Passengers`, are for passengers aboard), and the stair samples are left out of the steps ([#236](https://github.com/openOMSI-org/openOMSI/pull/236)).
- Teleporting to a street picked on the city map works across a big map: the navigator's lanes of the whole map are taken when the loaded tiles have no street there, and the bus waits at the street's height for its tiles instead of dropping through ([#235](https://github.com/openOMSI-org/openOMSI/pull/235)).

## 0.1.323 - 2026-09-30

Crash reports from phones, a lost graphics device on DirectX 12, the vanilla night, and five

pull requests.

### New and improved

- A phone whose Vulkan driver went down while the shaders were being compiled (the reports that end at "cloud noise made") draws with OpenGL from then on (Settings → Graphics API takes it back).
- The automatic texture budget stays at 2.5 GB: since 0.1.237 a PC with 32-64 GB let the textures take 4-8 GB ([#277](https://github.com/openOMSI-org/openOMSI/pull/277)).
- `[kmcounter_init]` starts the odometer at the bus's years in service times its kilometres a year ([#305](https://github.com/openOMSI-org/openOMSI/pull/305)).
- **Phones**: a manual gearbox whose dashboard answers to the automatic's keys shows the manual gate ([#279](https://github.com/openOMSI-org/openOMSI/pull/279)).

### Fixes

- **Phones**: an app the system ended in the background (or that was swiped away) is no longer reported as a crash at the next start - most of the "closed without a word" reports were that. A report sent to GitHub carries the end of the log, and the whole report is on the clipboard as well; the renderer names each stage it compiles, so a report says where a driver gave up.
- A graphics device lost on DirectX 12 starts the game again on Vulkan, as one lost on Vulkan starts it on DirectX 12. The launcher, too, makes its device again on the other interface instead of drawing on a dead one with thousands of errors ([#274](https://github.com/openOMSI-org/openOMSI/pull/274), an AMD Radeon).
- **Vanilla**: the texture times the light as Omsi.exe multiplies them, in gamma space - nights were several times too bright, a late dusk instead of the dark ([#300](https://github.com/openOMSI-org/openOMSI/pull/300)).
- Night maps switch on with the street lamps, fully, as in Omsi.exe, instead of fading in with the dusk (a clear evening showed lit windows at a fraction, [#276](https://github.com/openOMSI-org/openOMSI/pull/276)).
- **Enhanced**: chrome and other opaque sphere-mapped parts reflect again ([#266](https://github.com/openOMSI-org/openOMSI/pull/266), [#264](https://github.com/openOMSI-org/openOMSI/pull/264)).
- The automatic clutch's help is for gearboxes that read the clutch pedal only: an automatic with number-key gears had its clutch pressed at stops ([#234](https://github.com/openOMSI-org/openOMSI/pull/234)); a script without `engine_n` no longer keeps the clutch down for good ([#260](https://github.com/openOMSI-org/openOMSI/pull/260)).

### Project

- **Merged**: [#298](https://github.com/openOMSI-org/openOMSI/pull/298) (backwards meshes of exporters with a positive determinant: the Citelis' dashboard lamps, by ThiBot77), [#307](https://github.com/openOMSI-org/openOMSI/pull/307) (force feedback on Logitech and Moza wheels, by tistron), [#310](https://github.com/openOMSI-org/openOMSI/pull/310) (a warning when the driver uploads far too slowly, by ThiBot77), [#313](https://github.com/openOMSI-org/openOMSI/pull/313) (all buttons of a Linux wheel in the launcher, by ThiBot77), and [#240](https://github.com/openOMSI-org/openOMSI/pull/240)'s scenery-object support for HTML textures (by shloooo).

## 0.1.307 - 2026-09-30

Passengers, bus physics and light maps checked against Omsi.exe once more, and ten pull

requests.

### New and improved

- A bus that is not in service (no valid destination, or a "$allexit$" one such as Betriebsfahrt) or that stands at its own terminus empties there and takes nobody on, as Omsi.exe does (0x61f3e3). People boarded buses showing nothing; nobody got out at the last stop of a late trip (its stop index started again at 0 with the next trip - riders now go by the stop itself as well).
- Riders get up as the bus pulls in to their stop, not once it stands.
- **The front of a queue stands aside while people get off**: both used the same spot at the door and each waited for the other ([#253](https://github.com/openOMSI-org/openOMSI/pull/253)).
- **An articulated bus's rear section rides on springs**: a bump under its axle is a jolt.
- **Enhanced lays the light maps the same way**: little by day, fully at night.
- Several maps on a slot chain as ADDSMOOTH and switch on at 0.5.
- The view reset restores the zoom and the outside camera's distance ([#244](https://github.com/openOMSI-org/openOMSI/pull/244), with [#281](https://github.com/openOMSI-org/openOMSI/pull/281)).

### Fixes

- Everyone on the way out holds the door request the whole way, as in OMSI: the automatic rear door no longer shuts on the next person walking up and opens again ("the door doesn't know whether people are getting off"). The requests are pulses, cleared after the vehicle's scripts each frame (0x7d6214): a timetable bus out of the passengers' reach no longer keeps its door open for good.
- `[momentofintertia]` on Omsi.exe's axes: roll is the third value, yaw the second (the SD202 rolled on 80 t m² instead of 300 - twice as fast, rocking over every uneven patch: the "boat").
- Speed bumps, cushions, manhole covers, lowered kerbs and slab edges are felt again: only faces under 2 cm over the road count as paint (4.5 cm took them away, and the bottom of every bump's ramp).
- When nothing is found under a wheel the ground is looked for up to 3 m above, as Omsi.exe's ground query does - a bus no longer falls through where it sank into a joint.
- **A light-mapped material is lit as D3D lights it**: the material's own light and colour times every light - the saloon lamps included - clamped, then the light map laid on with ADDSMOOTH. The saloon lamps are no longer counted twice (flat white where both were on).
- On a circular or turn-back route the duty no longer jumps to the stop over the road: stops are told apart by the direction the trip runs through them ([#254](https://github.com/openOMSI-org/openOMSI/pull/254)).
- AI cars follow bends tighter than their model's lock instead of running wide through kerbs and corner houses ([#249](https://github.com/openOMSI-org/openOMSI/pull/249): 851 → 301 moments of a car over 1.5 m off its path in 150 s of Spandau traffic).

### Project

- **Merged**: [#237](https://github.com/openOMSI-org/openOMSI/pull/237) (own number plate), [#240](https://github.com/openOMSI-org/openOMSI/pull/240) (HTML textures, by shloooo), [#257](https://github.com/openOMSI-org/openOMSI/pull/257) and [#263](https://github.com/openOMSI-org/openOMSI/pull/263) (AI bus displays and rear sections, by NACHN), [#281](https://github.com/openOMSI-org/openOMSI/pull/281) (H-pattern `kw_s_*_fest` gates, tour start and end in the chooser, by isaacsa2; [#280](https://github.com/openOMSI-org/openOMSI/pull/280) is the same), [#282](https://github.com/openOMSI-org/openOMSI/pull/282) (road markings and rails near the camera, by TruckiHD), [#287](https://github.com/openOMSI-org/openOMSI/pull/287) (tile light maps on their middle third, by Sulamufor), [#288](https://github.com/openOMSI-org/openOMSI/pull/288) (issue templates, by shloooo), [#290](https://github.com/openOMSI-org/openOMSI/pull/290) (mouse throttle reaches full, so automatic gearboxes kick down, by Sulamufor - taken without its build folder).

## 0.1.238 - 2026-09-30

Manual gearboxes, dashboard lamps, phones that crash or run slowly ([#226](https://github.com/openOMSI-org/openOMSI/pull/226), [#231](https://github.com/openOMSI-org/openOMSI/pull/231), [#229](https://github.com/openOMSI-org/openOMSI/pull/229), [#225](https://github.com/openOMSI-org/openOMSI/pull/225)).

### New and improved

- The automatic clutch's pull-away help (the clutch bites as the throttle goes down, so the engine does not stall) works for these scripts' `antrieb_getr_gang` too, and is left to the scripts that work the clutch themselves.
- "Automatic clutch" can be switched in the launcher (Controls) and in the game menu; with it off, a phone shows its clutch pedal.
- The phone's gear buttons light the gear engaged for either kind of script.
- A `[matl_change]` variant shows as Omsi.exe shows it (0x5fd6xx): the variable rounded to the nearest whole number picks the `[matl_item]` (1 = the first), anything else the plain material - a lamp whose variable stands at 2 with one item is dark. A variable no script declares counts as 0, as the model loader registers it: the stock MANs' spare buttons (switched by `*Noch nicht belegt*`, "not assigned yet") and mods' door button lamps were lit all the time.
- The game's log is written on the phone too (`game.log` in the app's folder, the previous run's as `game-prev.log`), with the device's maker and model. A run that closed in the middle of a drive - a graphics driver taking the app down without a word - is shown by the launcher at the next start, with the end of its log for "Copy report".
- The first drive after such a closing starts with safer graphics, and on OpenGL when the one that closed drew with Vulkan.
- A phone's graphics chip always gets the light picture (no SSAO, no MSAA, small shadow maps), whatever type its driver reports.
- The automatic render scale has a fourth step, 55 %, for a chip that is still too slow at 70 %.

### Fixes

- With the automatic clutch on (the default), a gear chosen with a key, a phone's gear button or a controller comes with the clutch pressed and let up again, as OMSI's clutch key does, for gearbox scripts that only take a gear with the pedal right down and do not work the clutch themselves (the LiAZ and PAZ KPP: `(L.L.clutch) 1 =`). Before, a player without a clutch pedal - every phone - could not put such a bus in gear at all, forwards or backwards. Scripts that read `AutoClutch` (the Sprinters' G32) still do it themselves.

### Project

- `OMSI_DEBUG_VARS` with `OMSI_DEBUG_VARS_EVERY=<s>` logs the variables through an offscreen `--drive`; a manual gate given with `--triggers` comes with the automatic clutch as from the keys.

## 0.1.237 - 2026-09-30

The testers' second round: the crashes with "the graphics device was lost", weak cards and

phones, traffic that stood on free roads and roundabouts, passengers' necks, VR, gamepads,

and six pull requests.

### New and improved

- Windows tries DirectX 12 before Vulkan.
- When the card runs out of memory, the textures are cut down (to 60 % of the budget each time, not below 300 MB) before the driver gives up.
- The interface's vertex buffers are made where a failure can be seen: after the card ran out of memory, one invalid buffer was written to every frame, flooding the log with thousands of GPU errors and taking the frame rate down to 12 fps ([#217](https://github.com/openOMSI-org/openOMSI/pull/217)). A failed one is now made again at the next frame and nothing draws from it meanwhile.
- The automatic render scale moves in three steps (100, 85, 70 %), at most every five seconds. Every 5 % step every two seconds made all the picture's targets anew - hundreds of MB each time - a stutter and memory the driver ran out of. At the smallest scale and still too slow, SSAO and then the shadows go off.
- A small or shared graphics chip (integrated graphics outside a Mac, a phone, a card of up to 2.5 GB, OpenGL) is drawn without SSAO and MSAA; a card of up to 4 GB without SSAO and with at most 2x MSAA. `OMSI_FULL_GPU=1` keeps the settings as they are.
- The status log shows the GPU memory the textures and meshes take; on Windows the machine's memory sets the texture budget.
- A hold of one frame winds a waiting driver's reaction back only a little: a junction "free, not free" by turns kept cars about to go for good, on open roads too.
- A car at the stop line when the light turns green goes; a green of a second let nobody through before.
- Nobody waits for a car of the ring that is itself creeping in a queue.
- `OMSI_DEBUG_STUCK` names the light programs and the hidden reasons a car holds.
- The bus's own head movement is off in the headset (the cab swayed before the eyes).
- An Xbox pad on Windows named in OMSI's `gamectrler.cfg` keeps its sticks and triggers ([#171](https://github.com/openOMSI-org/openOMSI/pull/171)).
- Raindrops on the glass are lenses ([#228](https://github.com/openOMSI-org/openOMSI/pull/228), by Jaja80330): each drop shows the world behind it upside down and mirrors the sky; drops sit in three sizes on turned grids, a mist of droplets greys the pane, and runners slide down in fits and starts, wiping a track and leaving beads behind. Storms are denser and less regular ([#222](https://github.com/openOMSI-org/openOMSI/pull/222), by TruckiHD).
- `[rendertype] presurface` objects draw before the terrain, so excavations under the ground show through their invisible covers ([#218](https://github.com/openOMSI-org/openOMSI/pull/218) by TruckiHD, [#215](https://github.com/openOMSI-org/openOMSI/pull/215)).
- The player's bus's own sounds keep their pitch while the camera follows it: sound and listener were moved at different moments and the Doppler shift made them waver ([#214](https://github.com/openOMSI-org/openOMSI/pull/214), by TruckiHD).
- The timetable chooser shows the chosen trip's duration, in words as OMSI's BBS writes them ([#233](https://github.com/openOMSI-org/openOMSI/pull/233), by tistron).

### Fixes

- **A lost graphics device (the driver reset the card**: "the graphics device was lost", [#219](https://github.com/openOMSI-org/openOMSI/pull/219), [#223](https://github.com/openOMSI-org/openOMSI/pull/223)) no longer ends the drive: the game saves the situation and starts again on it by itself with lighter graphics (no MSAA, no SSAO, smaller shadow maps, mirrors and texture budget; on Windows DirectX 12 when it was Vulkan that was lost), at most twice. The launcher does not report such a restart as a crash.
- Textures shrunk while far away come back whole at once when they are near again: buildings right in front of the bus stayed blurred on a map that filled the budget.
- A roundabout's entry no longer waits at its line for a gap at the far side of the ring: a nine-second gap that never came kept the queue standing for minutes (Westcountry: no car stuck any more, mean speed 13 -> 21 km/h).
- The sphere-map reflections are laid out by the bus's heading, not by each eye's view: they no longer swim with every turn of the head.
- Gamepads ([#200](https://github.com/openOMSI-org/openOMSI/pull/200)): the stick sets where the wheel turns to, on a gentler curve and less the faster the bus goes, and the wheel follows at a hand's pace; the bus no longer swerves with every touch of the stick.
- Force feedback ([#224](https://github.com/openOMSI-org/openOMSI/pull/224), [#230](https://github.com/openOMSI-org/openOMSI/pull/230), by tistron): DirectInput wheels have their own centring spring turned off before they are acquired, and again when they are acquired anew; the steering is lighter while turning, heavier when parking, centres itself under control and follows the bus's sideways acceleration; the front wheels' bumps and kerbs are felt as short vibrations (also in a gamepad's rumble). Steering force and vibration are set per controller under Controls -> Game controllers and kept in `Inputs/gamectrler.cfg`.

### Project

- `OMSI_AUTOPILOT=<km/h>` (offscreen): the player's bus follows the lanes and logs where it stands against the ground, for roundabouts and places buses fall through.
- **Merged**: [#214](https://github.com/openOMSI-org/openOMSI/pull/214), [#218](https://github.com/openOMSI-org/openOMSI/pull/218), [#222](https://github.com/openOMSI-org/openOMSI/pull/222), [#224](https://github.com/openOMSI-org/openOMSI/pull/224) / [#230](https://github.com/openOMSI-org/openOMSI/pull/230) (the same commits), [#228](https://github.com/openOMSI-org/openOMSI/pull/228) (with [#222](https://github.com/openOMSI-org/openOMSI/pull/222)'s hash; its own patches replace [#222](https://github.com/openOMSI-org/openOMSI/pull/222)'s density field), [#233](https://github.com/openOMSI-org/openOMSI/pull/233).

### Credits

- Passengers who look at the bus turn their shoulders with it, and the head turns no more than 45 degrees on them. The people have no neck bone: a head turned 60 degrees on still shoulders twisted the neck.

## 0.1.221 - 2026-09-30

Everything since 0.1.178: the testers' reports from Fikcyjny Szczecin (MAN NL/NG Enhanced),

KS Węglin, Cotterell and The Adstow Project, multiplayer, the trains, ten GitHub issues and

six pull requests. Where OMSI 2 has the behaviour, it was taken from Omsi.exe itself (the

addresses are in the commits).

### New and improved

- The suspension is Omsi.exe's: the body hangs on a spring and damper at each wheel over the ground point under it (`achse_feder`, `achse_daempfer`, `Axle_Springfactor`, capped at `achse_maxforce`), with no tyre spring, wheel mass or bump stop of our own in between. Buses no longer float over the road "like a boat"; every bus drives on its own `.bus` values. (`OMSI_TYRE_SUSPENSION=1` brings the old model back for comparison.)
- **The driver's head moves as in OMSI**: thrown by the body at the eye, up and down always, sideways and fore and aft with `[driverview_moving]`, never more than 10 cm.
- Mouse steering switched off (right click, O, the menu) leaves the wheel where it is; the keys go on from there ([#184](https://github.com/openOMSI-org/openOMSI/pull/184)).
- While the view is turned with the mouse, the cursor shows OMSI's four arrows ([#185](https://github.com/openOMSI-org/openOMSI/pull/185)).
- `[matl_lightmap]` is on or off at its variable's 0.5 and is added to the light before the texture (ADDSMOOTH): lit saloons glow at night and hardly show by day.
- Mirrors and door monitors (the BMC Procity's `camera_TFT` among them) show what they reflect: each frame the ray from the eye to the mirror is reflected in the mirror's face, as Omsi.exe does; left mirrors, kerb-side blind-spot mirrors and middle-door monitors no longer look into the saloon or the sky ([#192](https://github.com/openOMSI-org/openOMSI/pull/192)).
- Attached objects turn as Omsi.exe turns them (own rotation, then the parent's).
- Scenery whose free-texture filename is built in `{frame}` shows its texture ([#198](https://github.com/openOMSI-org/openOMSI/pull/198)).
- `model.cfg` `[item]`/`[setvar]` are paint schemes of the model, as in Omsi.exe, and the chosen scheme's variables are there for the scripts' `{init}` ([#190](https://github.com/openOMSI-org/openOMSI/pull/190)).
- **Checks for road builders**: `OMSI_CHECK_SPIKES`, `OMSI_HOLE_PHOTO`, `OMSI_ROAD_PHOTO_N`; `--cam` takes a field of view.
- Traffic keeps to the middle of its lane beside parked cars (narrow British streets, The Adstow Project).
- Random traffic keeps to its pool's path densities (`unsched_vehgroups.txt` pools with their own `[rule] trafficdensity`), and a positive density as low as 0.001 still lets cars on ([#201](https://github.com/openOMSI-org/openOMSI/pull/201), [#199](https://github.com/openOMSI-org/openOMSI/pull/199), from Aurora Studio). A car whose pool may go nowhere at a junction goes on where cars may instead of standing there.
- OpenXR VR support ([#168](https://github.com/openOMSI-org/openOMSI/pull/168), by EpixXx): stereo rendering with head tracking, a spatial Esc menu and cockpit pointer, right-click zoom, the headset picture on the monitor, its own settings and keys (Ctrl+Shift+R recentre, F7 monitor picture, F8 VR / desktop). See [docs/VR.md](docs/VR.md).
- With `OMSI_TOUCH=1` on a computer, the mouse works the on-screen controls as a finger (from [#202](https://github.com/openOMSI-org/openOMSI/pull/202)).

### Fixes

- `Inputs/keyboard.cfg` is read as Omsi.exe reads it ([#195](https://github.com/openOMSI-org/openOMSI/pull/195)): the third value's bit 1 means "the action follows the key's state" (throttle, brake, steering), 2 is Shift and 4 is Ctrl. The stock driving keys no longer need Shift; parking lights are Shift+L, the quicksave Ctrl+S, the screenshot Ctrl+Shift+P, the information display Shift+Y, the timetable Insert and the ticket desk camera Home. Rebinding a key in the launcher keeps the entry's own "held" bit; an Alt chord of our own is bit 8, which OMSI ignores.
- Material highlights are Direct3D's per-vertex specular term from the sun and the light above, as in OMSI: gear selectors, buttons and screens no longer catch sharp sun spots.
- `[matl_envmap]` on glass and paint blends as Omsi.exe blends it: windows are no longer mirrors of the street; the enhanced picture's glass reflects 4-12 %.
- A lamp's `[light_enh]` lights move with the mesh they belong to: a level crossing's barrier lamps stay on the barrier.
- Parked cars stand on the ground as Omsi.exe puts them, with the map's own pitch and bank, no longer tilted by the slope under them (Cotterell).
- The stop helper (`routearrows_busstop.sco`) stands on the stop object with its rotation; it no longer "cuts" into the bus beside it.
- A far AI bus keeps its destination sign instead of a flat colour beyond 50 m.
- `[trainreverse]` works as in Omsi.exe: a train whose next trip runs the other way is turned round where it stands - its last car leads - and goes on with the trip. The Berlin U-Bahn no longer drives off the end of its siding while another train appears for the trip back.
- Trains stop with their front at the station, as Omsi.exe measures it (half the train's length and the `[ai_brakeperformance]` holding offset): the S-Bahn and U-Bahn no longer stand half a car past the end of the platform. Train cars without `[boundingbox]` take their model's length (18 m, not 12 m).

### Multiplayer

- **No more micro-teleports**: states are stamped with the moment of the frame they show, and the other players' buses and the host's traffic are drawn by a clock that runs smoothly instead of jumping with every datagram. Another player's bus: speed jitter per frame median 11 % -> 1.4 %; the host's cars at a client are drawn within 2 cm of where the host has them.
- A joining player sees the host's traffic and people whatever their own traffic settings ("passengers but no traffic" on a server).
- A server no longer stalls when someone drives a bus it cannot load: it tries again after half a minute, loads only the buses its `vehicles` list allows and shows the first of them for any other.
- Joining by code starts on the host's map.
- Session codes end in a full group of four characters ([#152](https://github.com/openOMSI-org/openOMSI/pull/152)); old codes are still read.
- Each camera keeps where it was turned, as in OMSI 2.
- **A door whose entry point lies on the aisle opens to the kerb**: the left where traffic keeps left.
- [#127](https://github.com/openOMSI-org/openOMSI/pull/127) (an overlay layer drawn opaque), [#151](https://github.com/openOMSI-org/openOMSI/pull/151) (default specular), [#152](https://github.com/openOMSI-org/openOMSI/pull/152) (session code), [#176](https://github.com/openOMSI-org/openOMSI/pull/176) (shiny windows), [#184](https://github.com/openOMSI-org/openOMSI/pull/184) (mouse steering), [#185](https://github.com/openOMSI-org/openOMSI/pull/185) (look cursor), [#187](https://github.com/openOMSI-org/openOMSI/pull/187) (envmap brightness), [#190](https://github.com/openOMSI-org/openOMSI/pull/190) (`[setvar]`), [#192](https://github.com/openOMSI-org/openOMSI/pull/192) (mirrors and door monitors), [#195](https://github.com/openOMSI-org/openOMSI/pull/195) (keyboard.cfg bits).

### Project

- Hungarian refined (from [#143](https://github.com/openOMSI-org/openOMSI/pull/143), by agost4002).
- **Merged**: [#168](https://github.com/openOMSI-org/openOMSI/pull/168) (VR), [#198](https://github.com/openOMSI-org/openOMSI/pull/198), [#199](https://github.com/openOMSI-org/openOMSI/pull/199), [#201](https://github.com/openOMSI-org/openOMSI/pull/201). Taken in part: [#202](https://github.com/openOMSI-org/openOMSI/pull/202) (the mouse as a finger; its fixed gear panel and the committed rustup installer were left out), [#143](https://github.com/openOMSI-org/openOMSI/pull/143) (the Hungarian lines; its edits to the English texts would have dropped those lines in every language).

### Credits

- Passengers get off where Omsi.exe sends them: each rider's stop is drawn among the stops ahead by the stops' "passengers alighting" numbers. They no longer all leave after one or two stops.
- Waiting passengers keep to their nearest door while it opens: a bus whose rear doors open a moment before the front one no longer sends the people at the front to the back.
- People on foot wait for a car or bus standing in their way on a crossing, then go round it, instead of pressing against its side.
- F2 reaches the passenger cameras of an articulated bus's rear section.

## 0.1.182 - 2026-09-30

### New and improved

- Ko-fi, and one Donate button on the site
- **A door whose entry point lies on the aisle opens to the kerb**: the left where traffic keeps left

### Project

- Ko-fi was added alongside Buy Me a Coffee, and the site's corner Donate button opens a menu with both options.

## 0.1.180 - 2026-09-30

### New and improved

- Joining by code starts on the host's map
- Each camera keeps where it was turned, as in OMSI 2

### Multiplayer

- Joining a server by code starts on the host's map even when the server welcome arrives after loading has begun; maps not installed locally can still come with the host's mods.

## 0.1.178 - 2026-09-30

Everything since 0.1.146. Where OMSI 2 has the behaviour, it was taken from Omsi.exe itself.

### New and improved

- Road cant takes its width from the spline's height profiles, as in Omsi.exe.
- Skinned meshes (bellows, levers of mod buses such as the AA-FR Agora) deform as in OMSI 2.
- Roller-blind destination displays (`[texcoordtransY]`, `[matl_freetex]`, borders) work.
- Thüringer Wald buses keep their roof at night.
- Mirrors see closer and further (0.1 m to the objects' range, as Omsi.exe).
- **Trains are put together as in OMSI 2**: every unit with its cars, the last car turned round (Berlin U-Bahn A3, S-Bahn BR 275).
- Timetable buses' door handshake follows Omsi.exe (a trace: `OMSI_DEBUG_DOORS=1`).
- **Server admins**: set any installed weather, switch the cycle on and off, clear jammed traffic.
- **A launcher made for phones**: tabs at the bottom, a Play screen, full-screen choice sheets.
- Manual gearboxes on the touch controls, with a clutch pedal.
- On foot, the own bus answers clicks.

### Fixes

- **Roads no longer disappear under the grass.** Splines the map marks `[spline_terrain_align]` cut their outline out of the ground, as Omsi.exe does: whole stretches of Spandau's roads, the six-lane Falkenseer Chaussee among them, were buried. The cut is exact to a few centimetres: no sky along the kerbs, and narrow medians stay green.
- The ground is no longer taken away under every road in rough 1.5-3 m steps (the "holes in the world" beside kerbs and car parks); only where the map says.
- Bellows of articulated buses bend with the rear section on slopes instead of away from it.
- Headlights in the classic picture shine forward from the lamps, one beam per headlamp, as bright as in OMSI 2, and no longer light up the bus's own saloon and dashboard.
- **AI cars no longer wait for each other for ever**: a long wait at a side road now gets its turn, and two cars that each waited for the other drive on.
- **Passengers at a stop no longer all stare at the driver**: each watches a coming bus on their own, and only the people it takes keep looking once it stands.
- Installing mods works again (it stood at "reading the archive's table of contents").

### Performance

- Less stutter when the camera moves (culling buffers are kept between frames).

### Multiplayer

- Weather cycle (launcher, phone launcher, `weather = cycle` in server.cfg): a new weather every 25-60 game minutes, fitting the month; every weather change blends in over 4 minutes.
- **The official server**: type `openomsi` to join "openOMSI | Official Server"; it is first in the server list.
- **Any server address works**: an IP, a host name, host:port or a link.
- **Parked cars are the same for everybody**: a car that drove off at the host is gone for the other players too (their buses drove through cars only one side had).
- Joining keeps the duty on the host's map; the launcher never hangs on a job that died.

## 0.1.175 - 2026-09-30

### New and improved

- Waiting passengers decide individually whether to watch an approaching bus, and once it stops only the people who intend to board it keep looking at it.
- Waiting passengers watch a bus each on their own, and only their own bus once it stands

## 0.1.174 - 2026-09-30

### New and improved

- Trains and coupled vehicles are put together as Omsi.exe does: each unit's cars, turned round where the coupling says

### Multiplayer

- Train and coupled-vehicle consists are assembled in OMSI's order, including reversed cars and couplings from every unit. The same orientation rules apply to the player, AI and LAN vehicles.

## 0.1.173 - 2026-09-30

### Fixes

- A car that has waited a long time at a junction keeps its turn instead of being starved by every new arrival.
- Two cars that end up treating each other as their lead no longer wait for each other forever.
- A car committed to changing lanes is no longer held back by parked cars in the lane it is leaving.

## 0.1.171 - 2026-09-30

### New and improved

- Main-view culling buffers are reused between frames, reducing allocation and camera-motion overhead without changing visibility or LOD decisions. [#175](https://github.com/openOMSI-org/openOMSI/pull/175)
- Headlights shine from the lamps' face, one spot per headlamp, as bright as in OMSI 2
- **Articulation_<n>_beta**: the part in front's pitch less the rear part's
- **Skinned meshes**: the bones first, the mesh's own motion on top
- Unbound o3d bones keep their share of a [smoothskin] vertex
- An alpha-tested slot is not faded by its [alphascale] (Thüringer Wald bus roofs at night)
- Reuse renderer buffers to reduce camera stutter

### Fixes

- Roller blinds as in Omsi.exe: border addressing, fixed maps, freetex fallback

## 0.1.160 - 2026-09-30

### New and improved

- A spline type's half cant width is derived from its height profiles, as Omsi.exe does; splines without height profiles have no cant.
- Splines laid with [spline_terrain_align] cut the ground out under them, as Omsi.exe does
- **OMSI_DEBUG_DOORS**: timetable buses' door handshake twice a second
- **Any server address is taken**: an IP, a name or host:port is asked where a server answers
- OMSI_CHECK_ROADS also counts road points under the ground, and where
- **Administration**: any weather by name, the weather cycle, clearing the traffic
- A weather cycle, and every change of weather comes in over a few minutes
- **Bellows bend with the rear section**: articulation beta as Omsi.exe counts it
- The ground is taken away only where the map says, as in Omsi.exe
- **A launcher made for phones**: tabs, a Play screen, a sheet for every choice
- **The official server by name**: "openomsi" joins it wherever it is reached
- Joining keeps the duty on the host's map; the launcher never waits for a job that died
- **Phone controls**: a manual gearbox's whole gate, and its clutch pedal
- Headlights shine ahead as a spot in the classic picture too; the own bus answers clicks on foot

## 0.1.146 - 2026-09-29

### New and improved

- **Scripted advertisements can be rendered on scenery objects.** [#147](https://github.com/openOMSI-org/openOMSI/pull/147)

## 0.1.144 - 2026-09-29

### New and improved

- **Linux wheels get force feedback and stable button numbering.** [#146](https://github.com/openOMSI-org/openOMSI/pull/146)
- Reuse launcher content cache after updates

### Fixes

- **Model-order drawing back on**: a blended roof hides the saloon again

## 0.1.139 - 2026-09-29

### Fixes

- **A right click can end mouse steering again.** [#162](https://github.com/openOMSI-org/openOMSI/issues/162)

## 0.1.138 - 2026-09-29

### New and improved

- **OMSI_DEBUG_TRAILERS**: log coupled parts off the level of what pulls them
- [matl_noZcheck] keeps the depth test, as in Omsi.exe

### Project

- `OMSI_DEBUG_TRAILERS` logs coupled parts whose level does not match the vehicle pulling them.

## 0.1.136 - 2026-09-29

### Fixes

- Model-order vehicle drawing is off by default again after it exposed saloons through body panels on several buses; `OMSI_MODEL_ORDER=1` keeps the ordered path available for tests.
- **Bodies whole again**: model-order drawing off by default

## 0.1.135 - 2026-09-29

### New and improved

- `$cutBegin`, `$cutEnd` and `$SetLength` round their counts like Omsi.exe, and `$*` repeats a pattern to fill the requested length.
- Script string functions count as Omsi.exe counts
- $RemoveSpaces trims both ends only, as Omsi.exe does

## 0.1.133 - 2026-09-29

### New and improved

- People keep off the carriageway and off shelter roofs

### Fixes

- Waiting passengers stop at the pavement instead of stepping into traffic, and people under shelters stand on the floor at their own height rather than on the roof above them.

## 0.1.132 - 2026-09-29

### New and improved

- Passengers board the buses that go their way, as Omsi.exe decides it

### Fixes

- Passengers choose buses from the stops and termini served by the timetable, as Omsi.exe does, instead of using the old random "take the next bus" rule.

## 0.1.131 - 2026-09-29

### New and improved

- Text textures drawn with Omsi.exe's font rules
- Fleet numbers, plates and depot repaints as Omsi.exe assigns them

### Fixes

- Text textures follow Omsi.exe's font rules for missing glyphs, clipping and glyph spacing, fixing destination words that could run together or be squeezed into a texture.

## 0.1.129 - 2026-09-29

### New and improved

- Wheels stand on the drawn splines, as Omsi.exe stands them

### Fixes

- Vehicle materials follow Omsi.exe's fixed-function texture stages more closely, including environment maps, transmaps and model-order blend/depth behaviour.
- Materials drawn as Omsi.exe's fixed-function stages draw them
- Parked cars no longer glow at night

## 0.1.126 - 2026-09-29

### New and improved

- Every vehicle one placed is saved in the situation, not just the driven one
- The launcher lists buses in folders inside a vehicle folder

### Fixes

- **Cars no longer brake for the player's bus merely following behind them.** [#139](https://github.com/openOMSI-org/openOMSI/issues/139)
- Cars no longer brake for the player's bus that only follows them
- A continued session has its traffic and its people again
- No crash on a person with small feet; AI trailers keep to their own level

## 0.1.121 - 2026-09-29

### New and improved

- **Offscreen**: OMSI_JOINT_ANGLE holds an articulated bus's rear section at an angle

### Fixes

- [surface] makes an object [fixed], as Omsi.exe sets both

### Project

- `OMSI_JOINT_ANGLE` can hold an articulated bus's rear section at a chosen angle in offscreen tests, making joints and bellows easier to inspect.

## 0.1.119 - 2026-09-29

### New and improved

- The wheels stand on what Omsi.exe's ground query stands them on

### Fixes

- Passengers meeting an arriving bus step only toward the kerb and stay on the pavement instead of standing out in the road. [#123](https://github.com/openOMSI-org/openOMSI/issues/123)
- Passengers meeting a bus that pulls in stay on the pavement

## 0.1.117 - 2026-09-29

### New and improved

- [matl_alpha] is followed on buses: cut-outs and overlay layers stay as declared

### Fixes

- Bus `[matl_alpha]` modes are respected again, so alpha-tested cut-outs and overlay layers are not forced opaque by the body-depth repair. [#127](https://github.com/openOMSI-org/openOMSI/issues/127)

## 0.1.116 - 2026-09-29

### New and improved

- **Windows**: the lists fill after the OMSI 2 folder is chosen
- AI trains and trams keep their cars on the rails
- **Phone**: the indicator and hazard buttons work with OMSI's key layout too

### Fixes

- The outside-view zoom changes in steps again instead of compounding every frame and jumping straight to its minimum or maximum. [#126](https://github.com/openOMSI-org/openOMSI/issues/126)
- The outside view zooms in steps again instead of jumping to its ends
- The bus no longer rocks like a boat at high frame rates
- Bus bodies reflect their envmap again (and with it their bump map)

## 0.1.110 - 2026-09-29

### New and improved

- **Objects attached to splines are placed at their stored chain offsets.** [#124](https://github.com/openOMSI-org/openOMSI/pull/124)

### Fixes

- Fix alpha-tested foliage coverage

## 0.1.101 - 2026-09-29

### New and improved

- **Laptops with a GeForce GT/GTX start**: every graphics interface and adapter is tried
- Random traffic groups drive where the map's paths ask for them (Trabant around Falkensee) ([#121](https://github.com/openOMSI-org/openOMSI/pull/121))

### Fixes

- The game and launcher try the requested graphics backend and then other available Vulkan, DirectX 12 and OpenGL adapters instead of giving up on the first device that cannot open.
- Windows asks switchable-graphics drivers for the high-performance GPU, and a complete failure now reports an error cleanly instead of panicking. [#97](https://github.com/openOMSI-org/openOMSI/issues/97)

## 0.1.99 - 2026-09-29

### New and improved

- English interface strings remain the lookup keys, so renamed text does not silently lose every translation; the Hungarian improvements are kept.
- Follow-up to [#118](https://github.com/openOMSI-org/openOMSI/pull/118): the English texts stay the keys

### Fixes

- Multiple fixes in English and Hungarian translation.

## 0.1.96 - 2026-09-29

### New and improved

- **Main-thread frame time is reduced on busy maps.** [#119](https://github.com/openOMSI-org/openOMSI/issues/119)

## 0.1.95 - 2026-09-29

### New and improved

- Graphics cards with about 2 GB of VRAM give textures a smaller share of memory, avoiding device loss on maps such as Grundorf with MSAA, SSAO and shadows enabled. [#114](https://github.com/openOMSI-org/openOMSI/issues/114)
- **Cards with 2 GB**: textures take a third of it ([#114](https://github.com/openOMSI-org/openOMSI/pull/114))
- Follow-up to [#117](https://github.com/openOMSI-org/openOMSI/pull/117): pitch, then bank, then heading, as Omsi.exe composes them
- Mirrors are drawn 1.6 wide, as OMSI draws them ([#92](https://github.com/openOMSI-org/openOMSI/pull/92))

## 0.1.92 - 2026-09-29

### New and improved

- Map pitch and bank are converted into the world's rotation convention correctly, fixing scenery such as Thüringer Wald rock faces. [#117](https://github.com/openOMSI-org/openOMSI/issues/117)

### Fixes

- Turn the map's pitch and bank into the world's sense of rotation (Thüringer Wald rocks form rock faces again) ([#117](https://github.com/openOMSI-org/openOMSI/pull/117))

## 0.1.91 - 2026-09-29

### New and improved

- When a material slot has several `[matl_change]` records, the item is shown while any of their variables is active, matching Omsi.exe. [#112](https://github.com/openOMSI-org/openOMSI/issues/112)
- Several [matl_change] of one slot: the item shows while any of them is on ([#112](https://github.com/openOMSI-org/openOMSI/pull/112))
- A placed object's map strings are its string variables before {init} ([#112](https://github.com/openOMSI-org/openOMSI/pull/112), [#108](https://github.com/openOMSI-org/openOMSI/pull/108))
- An AI bus's destination is the first depot terminus of its name, as in OMSI ([#110](https://github.com/openOMSI-org/openOMSI/pull/110))

### Fixes

- **Crash fixes**: buffers after running out of video memory, scripted objects with ground slots

## 0.1.87 - 2026-09-29

### New and improved

- **DirectInput**: say why a wheel with force feedback gets no forces ([#99](https://github.com/openOMSI-org/openOMSI/pull/99))
- **Outside view**: Ctrl+wheel narrows the field of view; the pressed wheel pans ([#104](https://github.com/openOMSI-org/openOMSI/pull/104))

### Fixes

- DirectInput now logs why a force-feedback wheel could not get exclusive access or create its constant-force effect instead of silently running without forces. [#99](https://github.com/openOMSI-org/openOMSI/issues/99)
- A lost graphics device ends the session without a crash ([#94](https://github.com/openOMSI-org/openOMSI/pull/94), [#96](https://github.com/openOMSI-org/openOMSI/pull/96), [#98](https://github.com/openOMSI-org/openOMSI/pull/98), [#100](https://github.com/openOMSI-org/openOMSI/pull/100))

## 0.1.84 - 2026-09-29

### New and improved

- Only splines whose profiles are entirely overhead are excluded from the ground surface; walls, embankments and watersides still shape the ground.
- Mesh files are looked up beside their model first, and the vehicle body-depth repair keeps applying where it is needed.
- Follow-up to [#103](https://github.com/openOMSI-org/openOMSI/pull/103): overhead splines only, meshes and bodies as before
- [matl_alpha] given: the test for blocks omsi-model joins ([#95](https://github.com/openOMSI-org/openOMSI/pull/95))
- Support indexed parking lists, scenery free textures and decorative splines ([#103](https://github.com/openOMSI-org/openOMSI/pull/103))

### Fixes

- Fix foliage alpha coverage revealing background ([#105](https://github.com/openOMSI-org/openOMSI/pull/105))
- **Mipmaps**: keep the colour where a 2x2 footprint has alpha 0 (lamps of buses and cars no longer black at a distance) ([#102](https://github.com/openOMSI-org/openOMSI/pull/102))

## 0.1.79 - 2026-09-29

### New and improved

- When several plain `[matl]` blocks address the same slot, a later `[matl_alpha]` can set the slot's alpha mode, matching OMSI's material handling. [#95](https://github.com/openOMSI-org/openOMSI/issues/95)

### Fixes

- Let a later [matl] of the same slot set its alpha mode (Thüringer Wald chain barriers no longer on a white band) ([#95](https://github.com/openOMSI-org/openOMSI/pull/95))

## 0.1.78 - 2026-09-29

### New and improved

- `Cargo.lock` now records the Windows dependency used by `omsi-render` for DXGI.
- Waiting passengers step forward to meet a bus pulling in ([#82](https://github.com/openOMSI-org/openOMSI/pull/82))
- With collisions with objects off, no step stops the wheels ([#79](https://github.com/openOMSI-org/openOMSI/pull/79), [#61](https://github.com/openOMSI-org/openOMSI/pull/61))
- Splines are drawn from the side they face, as OMSI draws them ([#76](https://github.com/openOMSI-org/openOMSI/pull/76))
- FXAA for the plain graphics too, where multisampling is off ([#73](https://github.com/openOMSI-org/openOMSI/pull/73))
- Mirrors look back past the bus, not at its side ([#92](https://github.com/openOMSI-org/openOMSI/pull/92))
- A spline standing clear of the ground casts a sun shadow ([#70](https://github.com/openOMSI-org/openOMSI/pull/70))
- A second [matl] block of one material goes on with it ([#80](https://github.com/openOMSI-org/openOMSI/pull/80))
- Keyboard pedals and steering centring as Omsi.exe works them ([#74](https://github.com/openOMSI-org/openOMSI/pull/74), [#63](https://github.com/openOMSI-org/openOMSI/pull/63), [#75](https://github.com/openOMSI-org/openOMSI/pull/75))

### Fixes

- **Crash fixes**: NaN in a depth sort, readback after a lost device, 2 GB cards

## 0.1.68 - 2026-09-29

### New and improved

- The game's menus draw Chinese, Japanese, Korean and Thai ([#72](https://github.com/openOMSI-org/openOMSI/pull/72))
- [terrainmapping] on spline textures too; one rest mesh per object type
- Splines are cut, skewed and textured as Omsi.exe does it
- Smooth spline corners on tight curves ([#84](https://github.com/openOMSI-org/openOMSI/pull/84))
- Avoid periodic DirectInput device enumeration ([#86](https://github.com/openOMSI-org/openOMSI/pull/86))
- Add Asian interface localisation contributed by Aurora Studio ([#88](https://github.com/openOMSI-org/openOMSI/pull/88))
- Draw [terrainmapping] slots with the ground of the tile ([#90](https://github.com/openOMSI-org/openOMSI/pull/90))

### Fixes

- Controllers plugged in while the game is running are detected again without periodic polling stutter, and DirectInput device names with non-Latin letters match their `gamectrler.cfg` entries. [#71](https://github.com/openOMSI-org/openOMSI/issues/71)
- Controllers plugged in while playing are found again, without the stutter

## 0.1.60 - 2026-09-29

### New and improved

- Document the modding guide and the Android build ([#66](https://github.com/openOMSI-org/openOMSI/pull/66))

### Project

- The README links the modding guide, and the build guide documents the Android toolchain and build command. [#66](https://github.com/openOMSI-org/openOMSI/issues/66)

## 0.1.59 - 2026-09-29

### New and improved

- Blended spline layers cut no ground away (sky through junctions)

### Project

- The format documentation now covers model LOD selection, transmap alpha, entry-point records and map tile numbering.

## 0.1.57 - 2026-09-29

### Fixes

- A car stuck at a dead end leaves sooner when a queue is building behind it.

## 0.1.56 - 2026-09-29

### New and improved

- **UK AI car packs**: visible traffic and their paint; object LOD choice as OMSI

### Fixes

- UK AI car packs are visible again, their paint/transmap layers render correctly, and object LOD selection follows OMSI's model order.

## 0.1.55 - 2026-09-29

### New and improved

- Stacked road markings are ignored as extra steps by the wheel-ground query.
- Stacked road markings are no step for the wheels

## 0.1.54 - 2026-09-29

### New and improved

- Tile numbers count repeated [map] entries; entry points found on their own tile

### Fixes

- Tile numbers include repeated `[map]` entries, as in OMSI, and entry points are resolved on their own tile instead of a different tile with the same object id.

## 0.1.53 - 2026-09-29

### New and improved

- Sound follows the system output device when headphones or a Bluetooth headset are connected or disconnected while the game is running.
- Sound follows the system's output device (headsets connected or disconnected)

## 0.1.52 - 2026-09-29

### New and improved

- A bus is put down on the surface its wheels should stand on near the entry point's height, rather than on the highest nearby surface such as a wall top or bridge deck.
- The bus is put down on what its wheels stand on, not on a wall's top

## 0.1.51 - 2026-09-29

### New and improved

- The player's own key bindings win over the game's conveniences

### Fixes

- Seated passengers on high seats let their feet hang naturally instead of stretching their legs straight down to the floor.
- Seated passengers' feet hang on high seats
- Hazard lights toggle off again

## 0.1.48 - 2026-09-29

### New and improved

- People keep to the floor they are on, not the roof over them

### Fixes

- People keep to the floor at their own height instead of being placed on a roof or deck above them.
- The bus is held while the tile under it is read again (snow weather)
- Shift+U starts a bus whose engine died; a stuck auto-start is begun again

## 0.1.45 - 2026-09-29

### New and improved

- Fourteen interface languages were added, bringing the built-in total to eighteen, with the launcher and game-menu strings filled in and system fonts used for Chinese, Japanese and Hindi text.
- 14 more interface languages, every interface text translated, system fonts for CJK and Devanagari

## 0.1.44 - 2026-09-29

### New and improved

- Launcher bus list built only when it changes

### Fixes

- The bus list is rebuilt only when its search, content or host list changes instead of every frame.
- **Sky cube redrawn sooner**: clouds no longer jump when flying fast

## 0.1.42 - 2026-09-29

### New and improved

- Puddle splashes as a lit mist, not glowing sprites
- Railway signals clear for the player's train

### Fixes

- Puddle splashes are rendered as a widening, scene-lit mist instead of glowing sprite rings.

## 0.1.40 - 2026-09-29

### New and improved

- Pressing a controller button highlights it in the controller list, making bindings easier to identify.
- A pressed controller button is lit in the list
- Radio stations from OMSI's radio plugins
- Reset all settings, with a dialog that asks first

## 0.1.37 - 2026-09-29

### New and improved

- OMSI's held keyboard-pedal behaviour is available as an option: a tapped throttle or brake can keep its position until the opposite pedal is used.
- Held keyboard pedals, as OMSI's option
- Spaces in fonts without a space character
- Force feedback and vibration can be switched off

## 0.1.34 - 2026-09-29

### New and improved

- An entry point whose marker lies below empty space places the bus on the ground above it, while real lower levels are preserved.
- Rear doors close on Shift+2; entry markers under the ground
- Doors-open and low-air notes follow what the bus really says

### Fixes

- Berlin buses' rear doors close on Shift+2 again.

## 0.1.32 - 2026-09-29

### Project

- Buy Me a Coffee links were added to the README and project website.
- Buy me a coffee on the site and in the README

## 0.1.31 - 2026-09-29

### New and improved

- `reflexionN.bmp` camera pictures work wherever a vehicle material refers to them, including switched material items and light/night maps.
- Camera pictures wherever a vehicle material names reflexionN
- **Passengers**: queues stop short of the bus's front, a door shut for a moment is waited out

## 0.1.29 - 2026-09-29

### New and improved

- Barriers can open for the player's bus when it approaches off the traffic lanes, such as in depot yards.
- Barriers open for the player's bus off the lanes
- Scenery the bus is spawned inside is taken away

## 0.1.27 - 2026-09-29

### Fixes

- Missing content report; a detailed game log

### Project

- Missing objects, splines and textures are reported by add-on in `~/.openomsi/missing_content.txt`.
- The game log records the system, launch settings, controls, views, pauses and regular runtime status so broken maps and sessions are easier to diagnose.

## 0.1.26 - 2026-09-29

### New and improved

- **Lower levels**: no rescue onto the roof, teleports keep the level
- **Phones**: 60 fps by default, the view follows the finger

### Fixes

- A bus on a genuine lower level is no longer "rescued" onto a roof, and teleports that specify a height keep the intended level.

## 0.1.24 - 2026-09-29

### New and improved

- Door keys are assigned from the model's actual doorways and the triggers that move their leaves, fixing unusual mod door layouts.
- Door keys by the model's doorways; release notes from the changelog lines since the last release
- **Modding**: any number of interior lamps per mesh, larger PBR maps, more lights per cell; modding docs

### Fixes

- Release notes are generated from changelog lines added since the preceding release instead of falling back to a generic message.

## 0.1.22 - 2026-09-29

### New and improved

- Mouse steering past the window edge, free camera keys and zoom, matte snow

### Fixes

- Mouse steering can keep turning beyond the window edge, free-camera keys no longer drive the bus at the same time, and the mouse wheel zooms in free camera and on foot.
- Snow is matte instead of taking the wet-road gloss.

## 0.1.21 - 2026-09-29

### New and improved

- Height profiles far over the drawn spline are taken at its drawn height; wheel check

### Fixes

- A spline height profile far above the geometry it describes is taken at the drawn height instead of becoming an invisible wall; `OMSI_CHECK_WHEELS=1` can report what wheels meet.

## 0.1.20 - 2026-09-29

### New and improved

- Wall tops stay walls, route numbers can be chosen correctly, and a force-feedback wheel recovers properly after a pause.
- Rear doors close, wall tops are walls, route numbers, FFB wheel after pause

### Fixes

- Automatic rear doors close again instead of having their timer constantly restarted by waiting passengers.

## 0.1.19 - 2026-09-29

### New and improved

- Full-size automatic render scale up to 4K off macOS; spline chain check

### Project

- Automatic render scale stays at full size up to 4K on Windows and Linux.
- `OMSI_CHECK_SPLINES=1` can list spline chains whose ends do not meet.

## 0.1.18 - 2026-09-29

### New and improved

- The clock can be moved forward or back from the game menu, including an option to put a duty back on time with its timetable.
- Discord Rich Presence shows the current bus, map and line.
- Set the clock by hand (on time with the timetable), Discord Rich Presence

## 0.1.17 - 2026-09-29

### New and improved

- A junction claim expires when its car has been crawling outside the junction for too long, preventing chains of traffic from waiting behind a stalled claimant.
- AI: a claim on a junction lapses when its car crawls for seconds outside it
- Smooth cockpit input and traffic on large maps ([#60](https://github.com/openOMSI-org/openOMSI/pull/60))

## 0.1.15 - 2026-09-29

### New and improved

- Wheels prefer the road surface over terrain cutting through it, removing invisible bumps and walls where roads pass through the ground.
- Bus auto-start waits longer for delayed starters and can select D with the brake held.
- Manual gearboxes can be shifted with Ctrl+Up/Ctrl+Down, with an optional automatic clutch.
- Continued situations keep the livery, mouse steering survives a look-around, glance keys return to the road, and the bus can be moved from the map.

### Fixes

- The pause menu no longer flickers its top selection, and the phone wheel follows the bus wheel one to one.

## 0.1.14 - 2026-09-28

### New and improved

- **No hidden dead zone on wheels any more**: gilrs's default filters took 10 % of every axis (90 degrees either side on a wheel of 1800) and held back small movements; Windows: the driver's own DirectInput dead zone and saturation are cleared, as OMSI does (PXN V99).
- **macOS**: a device with sliders or the simulation page's axes is read as a wheel from its HID elements even when SDL's list calls it a gamepad (HORI Truck Control System: accelerator and brake stayed merged and the steering had a gamepad's dead zone).
- **The Controllers page offers the view actions for buttons**: *view_look_left/right/up/down* (look round while held), the interior cameras, the views. OMSI's view actions on buttons work in the game.
- Force feedback in every view of the bus, not only the driver's.
- Mouse steering shows a cross as the cursor, as in OMSI.
- **Phones**: the on-screen wheel turns one and a half turns to the lock, as a bus's does, and comes back by itself when let go.
- The launcher starts the game that came with it, not a path remembered from an older installation (on macOS it kept starting a build of the days before the rename).
- **Esc → More → *Depot file (HOF)...*: choose the bus's depot file by hand.** Placing a vehicle asks for its livery and depot file.
- A street running through an object's `[boundingbox]` (a bridge, a gantry, a hall) makes that box no wall: mod maps' invisible walls across the road.
- **The automatic rear door closes**: the passengers' request button was never let go, so the stop request stayed on. A passenger who cannot get in stops asking after a while.
- Passengers turn their heads about the middle of the neck (aXYZ man02's neck point lies at the back of his neck: the head swung off the collar).

### Fixes

- Esc → More → *Set the clock...*: the clock one, five, fifteen or sixty minutes on or back, and on a duty *On time with the timetable* (early or late by six minutes: the clock is put where the bus is on time).
- Discord shows "Playing openOMSI" with the bus, the map and the line (Rich Presence, over Discord's local connection; `discord_app_id` in the settings, `discord_status=0` turns it off).
- **AI traffic no longer stands for minutes on a free road**: a car crawling in a jam of its own kept its claim on the junction ahead, and the cars that give way to it waited behind it in a chain (one Golf stood 81 s on Spandau; none now in four minutes of 80 cars).
- **On a road the wheels stand on the road, as in OMSI**: the terrain over or through the carriageway (an embankment the road runs under, ground poking through the asphalt) was an invisible wall under bridges and a bump that threw the bus.
- Mod buses with a lamp test after the key (the GX7767 E500 MMC waits four seconds) start with Shift+U: the starter is tried for longer; an automatic gearbox that takes D only with the brake held (ZF, `(L.L.Brake) 0 >`) is put into D by the auto-start.
- **Manual gearboxes**: Ctrl+Up / Ctrl+Down shift up and down (gear levers with a trigger per gate, `kw_s_1`...`kw_s_10`, `kw_s_N`, `kw_s_R`, as the LiAZ MKPP), the clutch let up as OMSI's clutch key lets it; with *automatic clutch* on, the clutch bites by itself when pulling away, as far as the engine keeps its revs. *gear_up* / *gear_down* for buttons.
- A situation loaded with *Continue* keeps the bus's livery.
- Mouse steering keeps the wheel and pedals while the right button looks round, as in OMSI.
- **The arrow keys' look with a wheel is a glance**: held, the head turns (at most 140 degrees); let go, it comes back to the road.
- Esc → More → *Move the bus on the map...*: click a street on the city map and the bus is put there (Ctrl+click on the map does it too, now also on a duty).
- The pause menu no longer flickers its top line while the mouse moves over it.
- **Phones**: the on-screen wheel turns the bus's wheel one to one.
- **Windows and Linux**: the automatic render scale draws at full size up to 4K (it drew a 4K screen at 58 %, and enhanced graphics looked like low-quality textures).
- **Automatic rear doors (SD202, SD200 and the like) close again**: passengers walking to the door or standing at the back of the queue kept asking for it, and the script starts its closing time again on every request. A request now opens a shut door; an open one is held only by somebody in the doorway, as by the light barrier.
- Walls with a height profile on their top (the stone and brick walls of UK maps) are walls to the wheels, not a road: where a wall's top met the road the bus drove up onto it and along it as the road fell away.
- A spline's height profile lying well over everything the spline draws (Westcountry's yellow surface marking: paint 10 cm up, height profile 50 cm) is taken at the drawn height: it was an invisible wall across the road. `OMSI_CHECK_WHEELS=1` with `--offscreen` lists what the wheels meet along the driving lanes (for map makers).
- Mouse steering turns on to the full lock past the window's edge: with the cursor at the edge, moving the mouse on outwards keeps turning the wheel (the width of the window is a smaller part of the lock the faster the bus goes, as in OMSI, and at 30 km/h the edge was a third of it); moving back gives that turn back first.
- **The free camera (F4)**: its keys (W A S D Q E, Space, Shift, the arrows) no longer work the bus as well (W switched the wipers on), and the mouse wheel zooms there and on foot (Ctrl+wheel moves the camera on).
- **Snow is matte**: it no longer takes the rain's gloss and shines like plastic in the lights.
- **Modding**: a mesh can be lit by up to 63 interior lamps (the extra numbers on the lines after the four of `[illumination_interior]`; OMSI 2 reads the first four), PBR maps up to 4096 px, and 32 lights per 25 m of the world instead of 16. What openOMSI allows beyond OMSI 2 is written down in docs/MODDING.md and on the site.
- **Door keys**: Shift+1, Shift+2 ... are the bus's doors front to back, found from the model (where each door leaf sits along the bus and which leaves each door trigger moves). The LiAZ's Shift+1 opened its middle and rear doors together and its front door had no key; a mod door script that mentions a closing variable while opening had its two leaves on two keys (one leaf moved, the other needed its own press). The game's log lists the keys of each bus ("door keys: ...").
- The release notes list what changed since the release before (they said "Small changes and fixes" for every release without a section of its own here).
- **Phones**: 60 frames a second by default (the settings took the PC OMSI's limit of 30 from its options.cfg), and dragging the view turns it the way the finger moves (it was the other way round, left for right and up for down).
- A bus on a lower level (a car park under a building, a road under a bridge) is no longer taken for one fallen through the world and put up on the roof: it has fallen only with nothing under it at all. A teleport to a place with a height lands on that level.
- A map that uses objects or splines that are not installed says so when it loads (how many, and the add-on folders they come from), and every missing object, spline and texture is listed by add-on in `~/.openomsi/missing_content.txt` (written again when the game ends, with the tiles loaded on the way). Holes, bare roads and white objects of such a map are a missing download, not a fault of the game - now one can tell.
- **The game's log records the whole session**: the system (OS, processor, memory), the command line and every setting at the start; then everything said on the screen, each view, pause and resume, every key action and door key, and a status line every minute (frame rate and the worst frame, where the bus is, its speed, the view, the time, the traffic).
- **The bus is no longer put down inside scenery**: an object no taller than a vehicle that stands for a third or more where the bus is put (a mod map's static buses in its depot, a sign) is taken away for the session, as the object editor takes one away (the map's files are not changed).
- Barriers (depot and car park gates on a light program) open for the player's bus off the lanes too: a gate whose lane starts up to 25 m ahead, the way the bus faces, is asked for (in a depot yard the bus stood beside every lane and the barrier stayed down).
- **Passengers**: the queue at a front door no longer goes on round the bus's nose (it stops short of the front and turns out along the kerb - people stood across the road in front of the windscreen, facing the bus), and a door shut for a moment no longer sends the waiting people away: they wait on 25 s after a door of the standing bus was last open (they turned away at once and came back when it opened again).
- **Camera monitors**: `reflexionN.bmp` is camera N's picture wherever a vehicle's material names it (its light map, night map, a `[matl_item]` switched on by the script), not only as the plain texture - monitors that show the camera once switched on were white.
- "Doors are open" follows what the passengers are told is open (`PAX_Entry/Exit<n>_Open`)
  - mods use `door_<n>` for other things, and a bus with its doors shut said they were open; "Air pressure is low" is no longer said with the tanks full (the spring brake is then held by the bus's own parking brake).
- The rear doors of the Berlin buses (SD, NL, EN/GN) close on Shift+2: switching their release off with the doors open shuts them at once, rather than when the passengers' last request has lapsed.
- An entry point whose marker lies under the ground (nothing under its height at all) puts the bus on the ground above it, not in the void under the map; a real lower level (a car park's floor) is kept.
- Settings → Controllers → *Force feedback and vibration* (and Esc → More → Options): switches the wheel's forces and a pad's rumble off altogether (a pad left plugged in shook all the time).
- **Spaces on displays and signs**: a font without a space character (many display fonts have none) leaves the width of a narrow letter between the words; the words of a destination ran into one another.
- **OMSI's held keyboard pedals**: Settings → Controllers → *Keyboard pedals stay where they are* (and Esc → More → Options). Tap the brake and it keeps that pressure until the throttle is tapped, and the other way round.
- Settings → *Reset all settings...*: every setting back to how it came (the language, the drivers, the key bindings and the game folder stay), after a dialog that asks first. The quality presets are under Performance.
- The bus radio also plays the stations of OMSI's radio plugins (SuperRadio's `.opl` and its lists under `plugins`): every stream address found there is a station, after the ones of `~/.openomsi/radio.cfg`.
- **Controls → Game controllers**: a button pressed on the wheel lights its line in the list for a few seconds, and the status line says which button it is and what it does - press it and give it an action right there.
- Railway signals clear for the player's own train as well (driven on the rails): its signals stayed at stop, as only an AI train ever asked for them.
- Puddle splashes are a mist of water - soft, lit by the scene, widening and thinning out as it sinks - instead of rings of glowing light flying off the wheels.
- **Enhanced graphics**: the sky is drawn again after a third of the way it waited before, so the clouds no longer drift and jump back into place when the camera flies fast.
- **Launcher**: the bus list is built again only when the search, the buses or the host's list change (it was rebuilt every frame, every name copied - scrolling it stuttered on phones).
- **14 more interface languages**: Українська, Беларуская, Қазақша, Polski, Čeština, Magyar, Español, Português (Brasil), Italiano, Nederlands, Türkçe, 日本語, 中文 (简体), हिन्दी - with English, German, French and Russian 18 in all (Settings → Language), and every text of the launcher and the game menus the tables lacked now translated in all of them. The tables are in the program, so they work on every system (the machine translation, which runs on Macs with Apple silicon only, is not needed for them). Chinese, Japanese and Hindi are drawn with the system's own fonts. OMSI's own texts (key names, descriptions) show in English where OMSI has no such language.
- **Shift+U after a crash starts the bus again**: a bus under power whose engine had died was taken for a running one and "switched off" round and round ("Shutting down..." for good). An auto-start that has gone on for 20 s is begun again by the next Shift+U.
- The weather turning to snow (Next weather, or the weather file) no longer drops the bus through the world: every tile is read again with the winter textures, and while the one under the bus is away the bus is held where it stands (it fell, was put back in the sky and fell again).
- Passengers in an indoor station stand on its floor, not on its roof: walking, they took any surface over them for a kerb to step up on. They now keep to the floor within a step of where they are (a station's floor under its roof, a car park's level under the deck).
- Keys the player set in `Inputs/keyboard.cfg` are theirs, also when they edited the installation's own file: Z / X / C (the indicators), Shift+number (the doors), W A S D and the arrows no longer take over a key bound to something else. What counts as changed is told from OMSI 2's own assignment, built into the game, with Shift held as well.
- The hazard lights go off again (X, and the phone's hazard button): pressed with them on, the key let go of the indicator lever instead of their own switch.
- Seated passengers on a high seat (on a podium, over a wheel arch) let their feet hang as a sitting body does, instead of stretching the legs straight down through the seat's front to the floor far below.
- **The bus no longer spawns floating on a wall's top**: the place it is put down at is the face its wheels stand on near the entry point's height (a road, a deck, an underground floor), not the highest surface of the map's height raster there, which is a wall's top beside a pavement (London) or a deck over the road.
- **The sound follows the system's output device**: a Bluetooth headset or headphones connected while the game runs take the sound over, and disconnected, the sound comes back on the speakers (it had stayed on the speakers, or stopped for good).
- Maps whose `[map]` list names a tile twice (Westcountry 3 names 33 tiles twice): the tile numbers the map's files use count those entries, as in OMSI. Counted without them, every number after the first repeat named the wrong tile: rows of objects repeated along a road (fences, bollards, lamps) hung from another tile's row and stood across the road or were missing (3 of 1536 rows found their start on Westcountry 3, now 165, 159 of them where the map says), timetable tracks ran over the wrong tiles, and entry points were looked for on the wrong tile.
- **An entry point is found on its own tile**: a map joined from two (two towns you cannot drive between) repeats object ids, and choosing a stop in one town put the bus on the grass of the other, where the other object of that id stands.
- Road markings laid over road markings (where lines cross, a box junction over a lane's arrows) are no step for the wheels: every layer of paint is looked through, not only the first.
- **Traffic of the UK car packs (WH UK AI**: Westcountry, London and others) is no longer invisible, only shadows and lamps driving about: a mesh written before a model's first `[LOD]` belongs to that level, as in OMSI. These cars put their shadow there; as a level of its own it was all a moving car had.
- **Their paint**: a `[matl_transmap]` picture without an alpha channel is opaque, as Direct3D reads it (the cars' paint layer has a black 24-bit `transmap_null.tga` and was invisible), and a layer drawn over another mesh of the same shape keeps its blending (the baked shading over the paint had been made opaque: black cars, black roofs).
- An object's `[LOD]` level is chosen as OMSI does: the first level in the model's order whose size the object reaches, else the last. The stock Sv signals list their detailed level before their low one; sorted by size, the low one stood in close up and the signal vanished in the distance. A model with a single `[LOD]` is drawn at any size.
- A car that has reached a dead end goes after 25 seconds, even in view, when others are waiting behind it: a fire engine at the end of a dead-end street held a queue of fourteen cars for two and a half minutes, and the junctions before it jammed full (Westcountry 3, 38 cars stuck for over a minute in five minutes of traffic, now none).
- No more sky showing through the road in stars and stripes at junctions: a spline made only of blended layers (Westcountry's lane darkeners laid over the junctions' painted ground) no longer cuts the ground away under itself; the ground is what it darkens.
- `OMSI_DEBUG_LAMPS=1` lists every traffic light object, the crossing it belongs to and those that name none (and so stay dark).
- **Esc → Destination display → *Route number***: the route (line) number on the displays, from the depot file's routes and the map's timetable; the destination stays.
- **Windows**: a force feedback wheel (G29) no longer pulls itself to the middle after the pause (taken back by the game, its own centring spring came back on).
- `OMSI_CHECK_SPLINES=1` with `--offscreen` lists the map's spline chains whose ends do not meet (for map makers).
- **The mouse no longer freezes the picture**: a gaming mouse's thousands of moves a second each looked for the switch under the cursor, and no frame was drawn while the mouse moved.
- A bus that could not be loaded is tried once more, and the reason is shown on the screen (it started on foot without a word).
- Parked cars have their paint (the paint scheme's pictures were looked for in the wrong folder and the cars stood white) and lean with an inclined street.
- "RenderBundleEncoder::finish: Validation Error" after the card ran out of memory no longer ends the game: the part of the picture is left out.

### Multiplayer

- P pauses into the pause menu in a LAN session too (the session goes on for the others).

## 0.1.13 - 2026-09-28

### New and improved

- P opens the pause menu and resumes the game from it.
- P pauses into the pause menu

## 0.1.12 - 2026-09-28

### New and improved

- With a steering wheel, the arrow keys look around as in OMSI.
- Ctrl+Alt+arrow turns the mirror being viewed, with the adjustment saved per bus in `mirrors.cfg`.

### Fixes

- A bus spawned inside an obstacle can move out of it instead of being permanently stuck.
- Road paint close above another road face no longer lifts the wheels.

## 0.1.10 - 2026-09-28

### New and improved

- The automatic rear door (MAN SD200, NL202/EN92) closes after the passengers are out: one passenger held up on the way out kept the stop request on for good.
- Add-ons that name their meshes from another folder than their model (Studio Polygon's `Configuration Files`, packs that borrow from the vehicle folder or the game folder) find them.
- The side mirrors show the bus's own flanks, as in OMSI (the outside-only meshes are drawn in the mirrors from the cab).
- Esc → *Destination display...*: choose any destination of the bus's depot file by hand (roller blinds, matrix displays, custom blinds).
- **Wheels and pedals are read from their HID elements**: two axes of the same kind stay two (HORI Truck Control System: the brake pedal moved the accelerator), a 16-bit wheel uses its whole range (no dead zone of a quarter turn), and the simulation page's steering, clutch, accelerator and brake are read on wheels that use it.
- **Windows**: the hat switches (D-pads of wheel rims, Moza among them) can be given keys like buttons (*Hat 1 up* ... on the Controllers page).
- ***Throttle pedal strength* / *Brake pedal strength***: a softer or stronger response of the analog pedals (launcher, and Esc → Options).
- ***Seat position***: the driver's eye forward/back, up/down, left/right, with *Reset the seat position* (launcher, and Esc → Options).
- The field of view applies to the free camera and the view on foot too.
- **Mirrors can be turned**: Ctrl+Alt+arrows in the cab turn the mirror you look at, kept per bus in `~/.openomsi/mirrors.cfg`.
- **Head tracking**: Settings → *Head tracking* takes the head's pose from opentrack's "UDP over network" output (port 4242) - TrackIR, Tobii, webcams and phones through opentrack. `head_tracking_invert=yaw,pitch,roll` in `settings.cfg` turns an axis round.
- **The clock can be changed gradually**: hold Ctrl+Shift+Page Up / Page Down (faster the longer it is held), or Esc → More → *Clock +10 minutes* / *-10 minutes*.
- P pauses into the pause menu (P or *Resume* go on); it was only a line of text before.
- The timetable shows departure times (a stop with a wait shows both), the trip number of the tour and the next trip.
- `omsi.info()` (map, clock, view, speed, delay, line, tour, trip, next stop, ...), `omsi.command(name)` (refuel, wash, repair, screenshot, save, weather, clock ...), `omsi.vars()`, `omsi.clock()`, `omsi.speed()`, `omsi.distance(x, y)`, and the events `key`, `next_stop`, `view` and `duty`. See [Plugins](docs/PLUGINS.md).
- Depot buses wear the repaint of their fleet number when the bus's `[number]` lists name one (a `.org` file per repaint); they were painted at random.
- **The AI vehicles are heard round the camera**: a free camera following an AI bus lost its sound 250 m from the player's bus.

### Fixes

- Big mod maps (Grande Porto, Novi Sad) no longer freeze for up to two seconds while driving: the night copies of object textures (`night\` folder) were decoded on the thread that draws. They are now read with the rest of a tile in the background; the slowest object upload on Novi Sad went from 1760 ms to 24 ms.
- Fixed the crash "Error in Buffer::get_mapped_range: Validation Error" (Windows): the vertex updates of a frame no longer go through one staging buffer that could outgrow the graphics card's buffer limit.
- **Steering no longer eats the engine's power**: both front wheels turned by the same angle and the tyres fought each other, so at 60 % steering a bus barely moved and at full lock not at all. The inner wheel now turns further than the outer one (Ackermann) - buses and cars take tight corners at the speed you give them.
- **Modded maps**: the bus no longer hops over invisible things. Only solid objects (`[fixed]`, not `[nocollision]`) give the wheels a step to climb; the low collision meshes of helper and sensor objects did too.
- **Trains stay on their track**: a track under a bridge counted as a "neighbouring lane", and a train changed lanes down through the viaduct.
- ***Camera collisions***: the outside camera no longer jumps in when something passes behind it (it is pulled in over a tenth of a second); switched off, it goes through everything, as in OMSI.
- A bus put down inside an obstacle (a shelter's or a depot's collision box - GPM) is no longer held there: what it spawned in is left alone until it has driven out of it.
- **Road markings are paint, not steps**: a road face within 4.5 cm over another one (markings made as `[surface]` objects or as splines with a height profile - Horizon) no longer lifts the wheels, so the bus stops hopping over lines at stops and roundabouts. Kerbs stay kerbs.
- With a steering wheel the arrow keys look around again, as in OMSI (a G29's buttons set to the arrow keys turned the view there; here they steered).
- The aXYZ man in the grey jacket no longer looks as if his neck were broken: a head turns about a point under its middle, not about the `[links]` neck point at the back of the neck, which swung the head off the collar whenever he looked to the side.
- The pause menu shows the everyday lines first (*Resume, Options, Line and tour, Destination display, City map, Timetable, Save, ...*); the rest is under *More...*. The mouse wheel scrolls the menu instead of moving the highlight, and only the line under the mouse is lit.
- **Android**: dropdowns no longer close (or pick something) the moment they open.
- A car already in a crossing on its green no longer stops again at a light of a path it joins inside that crossing (the cross traffic's red, an invisible stop line mid-turn).

### Project

- A link to the Discord server on the website and in the README.
- The website shows the releases (with their notes and downloads) and the issues (open and closed, searchable, with their discussion) on pages of its own.

## 0.1.9 - 2026-09-28

### New and improved

- "The graphics device was lost" (RTX 4060 reports, a few seconds into big maps) is the Vulkan driver giving up. On Windows the game can now draw with DirectX 12 instead: Settings → *Graphics API*, or *Use DirectX 12* in the launcher after such a crash.
- Graphics cards without Vulkan (GeForce GT 530 and other older ones) run the game: it asks Vulkan, then DirectX 12 (Windows), then OpenGL, and takes the first that draws. Settings → *Graphics API* chooses one; the Windows download brings DirectX 12's shader compiler.
- **Collisions with objects can be switched off, as in OMSI**: Settings → *Collisions with objects*, or Esc → Options in the game (OMSI's own `no_collision` setting is taken over).
- Mouse steering works in the outside and passenger views too, and the wheel follows the cursor smoothly: it crept on by itself and came back in steps.
- **New settings, off by default**: *Steering linearity* (the steering keys turn the wheel at OMSI's steady pace) and *Old Steering* (the wheel stays where you leave it, as in OMSI 2 - turn it back yourself).
- **The clutch key works as in OMSI**: the pedal goes down at once and comes up slowly.
- **Settings → *Wheel rotation* and *Full lock at***: a wheel of 900° can steer like a real bus (the full lock at, say, 540° of the wheel), *Reset wheel settings* goes back to OMSI's (the whole wheel is the full lock). *Invert force feedback* for wheels that push the wrong way (G29).
- Settings → *Field of view* for the views from the bus (Default: the bus's own cameras); the mouse wheel, = and - still zoom as in OMSI.
- **A steering wheel listed twice (Logitech G29**: once as a wheel, once as a gamepad) is listed once, and *Use this device* on the Controllers page switches any device off.
- **Free roam (no duty)**: people only board a bus that shows a destination, not one showing nothing or "not in service".
- **Drive → *Depot file***: the depot file (HOF) can be chosen by hand; *Automatic* follows the map and the date.

### Fixes

- **A graphics validation error no longer ends the game**: it is written to the log and the game goes on.
- Phones where every mesh came out flat and far away (the bus in the launcher too): the shaders no longer read the objects' matrices in the way some phone GPUs get wrong.
- Cards below the usual limits get a smaller shadow map, and textures larger than the card takes are scaled down instead of stopping the game.
- When a game ends on an error, the launcher says so, with *Copy report*, *Report on GitHub* and, after a lost graphics device on Windows, *Use DirectX 12*.
- **Only the objects OMSI makes solid stop the bus (`[fixed]` ones and poles).** Signs on stop poles, gantries and the bridges of mod maps not marked so were invisible walls - under the bridges of Saint Servant, for example.
- Parked cars, people and objects whose `Texture` or `model` folder is spelt with another case are no longer white or missing on Linux (TH_Zafira and others).
- The warning lamps' glass of the Thüringer Wald buses (S 315 UL, S 317 UL, O 550) and the MB O 407 is see-through again instead of a row of white tiles ("glas" is glass too).
- **A limiter on the whole mix**: many loud sounds at once no longer clip (heard as squeaks and crackles).
- **Icons missing on some phones**: the launcher's picture atlas grows when a screen needs more room than it had (high-resolution phones).
- The README has an installation guide and what to do when something goes wrong.

### Multiplayer

- A joining game plays on the host's map when it is installed, whichever map was chosen before joining. On big add-on maps it often stayed on its own map - where nobody met it:
  - the host lists its mods for the joining players after it starts, which takes a while on a big map, and the joining game gave up waiting for that list after 25 s;
  - a host busy loading a heavy area answered later than the 3 s the joining game waited;
  - a dedicated server answered nobody until it had loaded its whole map.
- The host answers joining players while it loads its world, and a joining game stays in the session while its own map loads.
- A joining game with everything the host uses installed no longer needs 3 GB of free disk.
- Mods installed into openOMSI's content folder inside the OMSI 2 folder are passed on to joining players (they were taken for OMSI's own files).
- A player's info (bus, destination, display texts) always fits one datagram: long paths and texts made it too big to arrive, and the others never saw which bus the player drove.

## 0.1.8 - 2026-09-28

### New and improved

- **Changing a key on the Controls page now takes effect**: the page says which driving keys are in use, and changing any key switches *Driving keys* to *Custom controls* by itself (before, with W A S D the edited keys were silently ignored).
- **Zoom inside the bus**: the mouse wheel, **=** / **-** and a pinch narrow the view in the driver's and passenger views, as in OMSI.
- Mouse steering follows Omsi.exe exactly, now including its pedals (throttle from the middle of the window to the top edge, brake to the bottom edge, no dead zone). Settings → *Mouse steering* makes it more or less sensitive (100 % = OMSI).
- On Windows the game controllers are read through DirectInput, as OMSI reads them: every device Windows lists as a game controller (wheels with their makers' drivers included), up to 128 buttons, and force feedback on wheels - the centring that grows with the speed, the heavy steering of a bus standing still, and the scripts' shaking.
- Buttons are numbered as DirectInput numbers them (they were counted in the order they were first pressed), and every button of a device can be given a key: the list stopped at the ten that fitted on the page (T16000M).
- ***Set up step by step***: turn the wheel to the left, press each pedal - the axes, their direction and combined pedals are found by themselves. A connected device nobody has set up yet steers with its X axis and says where to set it up.
- The OMSI 2 folder is found when openOMSI was unpacked into it (openOMSI keeps its own content in an `openOMSI` folder there), when the path is pasted with quotes, or when `Omsi.exe` or a folder inside the game is chosen; a folder that is not a complete OMSI 2 is reported with what it lacks.
- **Timetable**: changes stay while you move between lines and are saved together (*Save all*); *New line*; *Repeat* makes a whole day of tours (every *n* minutes up to a last departure).
- Settings → Graphics → *Reflection maps* switches the materials' reflections (`[matl_envmap]`) off.
- **Distance as in OMSI**: full volume up to the `[3d]` reference distance, then falling as 1/d (DirectSound's law). It fell much faster, so most sounds were far too quiet.
- Footsteps outside are heard through the bodywork from the cab (and the saloon's from the street); people in the street sounded as if they walked inside the bus.
- Objects put on a road spline (`[splineAttachement]`), such as an entry point or a stop, are found by their id: a Novi Sad start point was "not in the map".
- An entry point whose object comes out on another level than the map recorded (under a bridge) starts the bus at the recorded height.
- The depot file (HOF) follows the date as the map's chrono says: Berlin in 1994 has line 137 where 1986 had 92 - on every map with chrono depot changes.

### Fixes

- **Keys you bind yourself now beat the ready-made layouts**: with W A S D chosen, a D you gave to the gearbox is the gearbox, not "steer right". The layouts' extra keys (Z/X/C for the indicators, I for the saloon lights) no longer apply with Custom controls (C is OMSI's "look ahead" there again).
- **Space** looks ahead again (OMSI's `view_reset_all_directions`); it was swallowed by the W A S D layout. Every view keeps its own direction: turning the outside camera (F3) no longer turns the driver's head (F1).
- **Phones**: the launcher is drawn at least at the system's text size, the settings stand in one column, a finger on a list at its end scrolls the page on, page titles no longer run under the tabs.
- The bus's own sounds are no longer muffled in the cab unless they are other vehicles': interior sounds such as the indicator relay were cut to a quarter and dulled, and only came through with a door or window open.
- **Matrix displays drawn by scripts (script textures as the LED mask, `\S:n`, e.g.** churaPixel/ Krüger++ matrices) show their dots instead of a fully lit panel: a `[matl_change]` ahead of the slot's `[matl]` made it opaque.
- **Phone**: the pause menu scrolls with the finger; a finger put down to scroll no longer picks the line under it.
- **With V-sync one frame waits for the screen instead of two**: less input delay.
- The release notes on GitHub list what changed (this changelog) instead of a link to the code changes.

### Multiplayer

- **Hosting no longer stops working after a while**: the router's port forwarding was asked for two hours and never renewed, and the rendezvous relay was asked every second and refused the host after an hour or two. The forwarding is renewed every 20 minutes, the relay is asked every few seconds with a growing pause after a refusal, and a Cloudflare tunnel that ends is started again.
- **New downloads**: Windows ARM64, macOS Intel, Linux ARM64, and the dedicated server for Windows (x64, ARM64) and Linux ARM64. The launcher updates itself on all of them.

## 0.1.7 - 2026-09-28

### New and improved

- The bus now follows its steering the way OMSI's own physics does: it turns exactly as far as its wheels point and only slides when a bend asks more grip than the road has. Before, every bus turned at 70 % of what its steering asked, 0.8 s late and drifting sideways - the "boat" feeling, and the same for every bus.
- Springs, dampers and their limits act where OMSI takes them (`achse_feder`, `achse_daempfer`, `achse_maxforce`, `achse_minwidth`/`achse_maxwidth`), every axle steers towards `[rot_pnt_long]`, and body pitch and roll are damped as in OMSI. Each bus feels as its author made it: stiffer springs, stronger dampers, a higher centre of gravity all show.
- **Mouse steering (O) as in OMSI**: the whole window width is the full lock, and above 10 km/h the same hand movement turns the wheel less and less (at 50 km/h a fifth as far). No more jumps when the cursor passes the middle.
- **Phone**: the on-screen wheel turns with the finger round it (a third of a turn is the full lock). It used to stop at about half a turn and spring back.
- Mirrors follow the bus's pitch and roll, use the camera distance from the `.bus` file, and only the mirrors in view are redrawn (the radius of `[add_camera_reflexion_2]`).
- **Trees have the width the map gives them**: slim trees such as firs were drawn up to six times too wide.
- Map tiles of older editor versions are read correctly (object tilt and strings).
- **Settings → Updates**: look for updates at the start, install without asking, Check now.

### Fixes

- The bus's own screens (IBIS, matrix displays, dashboard LCDs) are sharp again in Enhanced graphics: FXAA and the glow no longer blur them.
- **The launcher updates itself from the GitHub releases**: when a newer version is out it asks at the start, downloads it (checked against GitHub's SHA-256), replaces the program and starts again. Mods, content and settings stay as they are.
- **On Android the system's installer is used**: Update replaces the app and starts it again, Cancel leaves it as it was.
- **Android**: vibration of the on-screen buttons works (the calls never reached the app).
- **Android**: `openOMSI/env.txt` takes the `OMSI_*` switches a computer takes from its environment (for looking into problems).

### Project

- `cargo run --release -p omsi-sim --example handling -- <file.bus>` prints how a bus handles (yaw response, side slip, body roll and how fast it settles).

## 0.1.6 - 2026-09-27

### New and improved

- **Phone**: the on-screen wheel is drawn cleanly; calmer steering, tilt steering reaches the full lock at 45°.

### Fixes

- A version built again updates its release instead of failing.

### Project

- **Touch wheel**: clean drawing, calmer steering; release workflow updates an existing release

## 0.1.5 - 2026-09-27

### New and improved

- Lua plugins (`plugins/<name>.lua` or `plugins/<name>/main.lua`) on every platform, next to the original DLL plugins: bus variables and triggers, events, timers, saved data, hot reload, sandboxed. See [docs/PLUGINS.md](docs/PLUGINS.md).

## 0.1.4 - 2026-09-27

### New and improved

- **Android**: the launcher and the game on phones and tablets, with on-screen driving controls (wheel or tilt, pedals, gearbox, doors, indicators, cab panel, cameras). See [docs/ANDROID.md](docs/ANDROID.md).

## 0.1.3 - 2026-09-27

### New and improved

- Small changes.

## 0.1.2 - 2026-09-27

### New and improved

- **OpenOMSI**: first public release structure
- Initial commit

### Fixes

- Website and repository improvements and fixes.

## 0.1.1 - 2026-09-27

### Fixes

- Website and repository improvements and fixes.

## 0.1.0 - 2026-09-27

### Multiplayer

- **First public release as openOMSI**: a from-scratch recreation of OMSI 2 in Rust that runs every map and mod (an original OMSI 2 is needed). Builds for Windows, macOS and Linux and a dedicated server, released automatically on every push.
