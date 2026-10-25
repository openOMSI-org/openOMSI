# Changelog

Releases are started by hand from `main` as the beta `MAJOR.MINOR.PATCH`, and tested betas are
promoted to stable releases (see [docs/VERSIONING.md](docs/VERSIONING.md)); the downloads are on the
[Releases](https://github.com/openOMSI-org/openOMSI/releases) page.

## Unreleased

### New

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

- **Rail traction, brakes and couplings**: `[rail_traction]` (a curve of pull against speed, a
  power limit, an electric brake), `[rail_brake]` (air with a brake pipe wave and filling
  cylinders, electro-pneumatic or direct; the electric brake first, the friction brake the rest)
  and `[rail_coupler]` (free play, draft gear, strength; a train pulled apart breaks in two) for
  pedal-driven trams, metro and railway vehicles. Without the blocks nothing changes. See
  [docs/RAIL.md](docs/RAIL.md#traction-brakes-and-couplings-of-a-vehicle-openomsi-blocks).
- **Scripts that belong to a vehicle**: `[vehicle_script]` in a `.bus` or `.ovh` names a Lua or
  `.oop` script that runs as a plugin while that vehicle is the player's, with the plugin API and
  the permissions of vehicle content only (`ui`, `storage`, `vehicle_write`, `audio`). See
  [docs/PLUGINS.md](docs/PLUGINS.md#scripts-that-belong-to-a-vehicle).
- **Suspension types**: an axle can say `[suspension]` with `type coil | leaf | air |
  progressive`. Leaf springs stiffen and rub (dry friction between the leaves); air bellows hold
  the ride height of the empty vehicle whatever the load, through a levelling valve; axles
  without the block are OMSI 2's strut, unchanged. See [docs/SUSPENSION.md](docs/SUSPENSION.md).
- **The `transport` folder of a map**: stations with platforms, **routes without tours** (a
  route is segments from station to station and the services that run over them: a line with a
  branch, a short-turn or an express is more segments and another service), the overhead wire,
  a third rail, switches, depots, signals and blocks, in plain text files. Check them with the
  `transit_check` example. The data is read when the map opens and the problems are logged, and
  every service runs as a timetable line (one tour per departure and direction) in the launcher,
  the duty and the AI. See
  [docs/TRANSPORT_FORMATS.md](docs/TRANSPORT_FORMATS.md).
- **Transport modes**: a vehicle and a map can say what kind of transport they are for with
  `[bus_transport]`, `[tram_transport]`, `[railway_transport]`, `[metro_transport]` and
  `[trolleybus_transport]`. A bus does not start on a tram map and says why; a map with none of
  them takes every vehicle, and a vehicle with none is judged by its shape as before, so nothing
  that works now changes. The launcher lists the modes of maps and vehicles; timetable vehicles
  and the AI pool respect them. See [docs/TRANSPORT_MODES.md](docs/TRANSPORT_MODES.md).
- **AI Lists** (Settings, the new *AI Lists* tab): choose, map by map, which AI cars, lorries and
  buses the traffic is made of and which buses run on which line - only from what is installed
  (the AI vehicles and the buses, with every repaint), nothing new is added. *Traffic* changes the
  weighted lists of the random traffic (replace a car by another, add or remove one, set how often
  it appears); *Depot buses* changes a depot's types, fleet numbers, plates and repaints; *Lines*
  makes a line run only, or mostly, the types you choose (a type the line's depot lacks is added to
  it with a few buses). Every list and the picker show the vehicle in the 3D preview of the Drive
  page. The map's own files are never touched: the changes are kept in the content folder
  (`ailists_#user.cfg` and `car_use/!openomsi-ai-lists-*.ocu` of the map's folder, the format OMSI
  itself reads), a group or a line can be reset on its own, and *Reset the map* deletes them, which
  gives the map's defaults back. Translated into every language of the launcher.
- **LAN protocol 15**: a game of this version cannot join a session of an older one, nor the
  other way round (the AI traffic goes as a table, see below). Update the game on both sides.
- **Every light casts a shadow** (Enhanced and Enhanced+): the lamps' shadow maps were four, now
  there are forty-four, tiles of an eighth of the far shadow map's width (256 texels at the
  default 2048) in six rows under it - only where the picture is an Enhanced one, so Vanilla and
  Vanilla+ need no more than the far map itself - and smaller where the graphics card's textures are too low
  for them. Twenty are the world's: the street lamps, floodlights and platform lights lighting
  the view most keep their map while they stay among the strongest, and a lamp new to the set
  takes a free one (six a frame at most). Eight look along the road from the headlamps of the
  player's bus and of the cars in front of the camera (a car behind it that drives away has none),
  a pair of lamps sharing a map, taken for a place to half a metre so that a car driving by draws
  it every few frames, not every one; the car's own parts cast nothing into it. Sixteen are the
  lamps of the player's saloon: they hang over the seats in the bus's own frame, so that the maps
  hold still while the bus drives. They hold what stands still in the bus - the seats, the rails,
  the panels, a shut door; the wheels, the steering wheel, a swinging door, the driver and the
  passengers are left out until they have stood still for half a second - so that driving the
  SD77 at night draws none of them again (a tile with a tree in the wind in it is drawn every
  third frame, on its own turn). A handrail, a seat or a stanchion shades the floor, the seats
  and the panels behind it - also where the model's light maps (`[matl_lightmap]`) lift a surface
  to its texture's colour, which now stops where the lamps' light does not reach. The lamps name
  their tile in their record, no more a search through four, and the beams in fog, rain and snow
  of every lamp with a tile (the point lamps' too, not only the headlamps') are cut by it.
  `OMSI_NO_CABIN_SHADOWS=1` leaves the saloon's lamps without shadows; `OMSI_DEBUG_DRAWS=1` logs
  the tiles drawn each frame. A free camera high over the street chooses the lamps by their
  distance along the ground, as the lamps round it are the ones that light what it sees.
- **Mirrors drawn as the Enhanced picture show the sun's shadows too** (Graphics: Mirror quality:
  *As the picture (Enhanced)*, which stays opt-in, *Standard* the default): they used to need
  *Shadows in the mirrors* as well. They look up the window's shadow maps, no pass of their own.
- **Shadows in the headlamps' beams** (Enhanced): the light the fog, rain or snow scatters
  from a headlamp or a street lamp's beam is cut where a pole, a car or a wall stands in
  the beam, as the sun's shafts are cut by the shadow maps: the beam looks up the lamp's
  shadow tile along its way (one filtered tap a step, no pass of its own).
  `OMSI_NO_BEAM_SHADOWS=1` draws the old picture. `OMSI_DEBUG_PAD=1` logs every game
  controller button event (the ones the assignment menu waits for), for a report that a
  pad's buttons cannot be assigned.

**On foot**

- Walking is paced like a person's: 1.4 m/s walking and 3.8 m/s running, reached over about a
  second and stopped over a few steps instead of at once, turns are curves, and the eyes glide
  over kerbs and steps instead of following every footfall (`OMSI_FOOT_WALK` and `OMSI_FOOT_RUN`
  set the paces).

**Trolleybuses** ([docs/TROLLEYBUS.md](docs/TROLLEYBUS.md))

- A map can carry a real overhead wire (`trolley.txt` beside its `global.cfg`): two wires of
  opposite polarity, sections that are live or dead with an insulator between them, frogs
  (switches) and the feeder's voltage drop. A vehicle's `.bus` can carry real poles
  (`[trolley_pole]`) and a modelled drive (`[trolley_rheostat]`, `[trolley_async]`). OMSI 2
  has no wire of its own (its trolleybuses run a plugin), so these formats are openOMSI's.
- A trolleybus only moves on current: both poles must be on live wire of opposite polarity. The
  poles are real booms with a spring, a mass and a shoe that holds the wire: the shoe leaves the
  wire where the wire rises faster than the boom can follow, and the pole comes off when the
  wire lifts out of the shoe, when the bus stands too far off the wire's line (also at a switch
  set the wrong way), when a bend is taken too fast, or when the wire ends. On a dead section
  the bus rolls on and stops.
- Out of the cab, near the back of the bus (on foot, outside or free camera), hold R to take
  a pole's rope and aim the boom at the wire, let go to put it on; T pulls a pole down;
  Shift takes both. Modern poles with a motor go up and down from the cab with the same keys.
- The rheostat drive solves its series motor against the resistor stack with a stepping
  controller, resistor heat and dynamic braking; the asynchronous drive is an inverter with
  regenerative braking into the line where the section takes it, a brake chopper and an optional
  battery for manoeuvring away from the wire. A vehicle with a scripted drive of its own keeps
  it, and loses its torque without current.
- Frogs are set by the current the bus draws at the contact plate before them (power takes
  the branch, coasting goes straight), or by its indicator.
- Arcs where current is broken (a shoe leaving the wire, a pole coming off, an insulator or a
  frog crossed under power): a flash on the wire and the roof, sparks, a crack; the shoes
  hiss on the wire with the current and the speed; contactors click, shoes and poles thud.
  New variables for models: `Trolley_Pole<k>_Elev`, `_Azim`, `_State`, `Trolley_Volts`,
  `Trolley_Amps`, `Trolley_Powered`, `Trolley_Notch`, `Trolley_Arc`, `Trolley_Hiss`.
- Real OMSI 2 trolleybus content runs as it is ([docs/TROLLEY_MODS.md](docs/TROLLEY_MODS.md)): a
  map's wire splines (two thin strips high over a spline, as the Bronitsyn map draws its wire) are
  the wire network, frogs where three of them meet; a trolleybus add-on that only shows folded and
  raised pole meshes by `cp_SHTANGALEV`/`cp_SHTANGAPRAV` gets real poles from those meshes, keeps
  its own script's drive (cut without current) and is read as rheostat or pulse-controlled from
  its description.
- New environment variables: `OMSI_TROLLEY_DEAD_AT`, `OMSI_TROLLEY_FILE`, `OMSI_TROLLEY_DEAD`, `OMSI_TROLLEY_UP`,
  `OMSI_TROLLEY_HANDS`, `OMSI_DEBUG_TROLLEY`.
**Phones and tablets**

- The free camera (F4) on a touch screen: the cab panel's new "Free camera" button flies out
  of the cab, a stick at the bottom left moves the camera (the further it is pushed, the faster:
  fine control near the middle), a finger dragged on the picture looks round, two fingers zoom,
  the buttons at the bottom right rise, sink and halve or double the speed, and "Exit free
  camera" (always on the screen, also with the controls hidden, and in the cab panel) goes back
  to the driver's seat.
- Photo mode for fingers: its own buttons (the shutter, leave, the settings sheet, hide the
  panel, a stick and rise / sink / fast), a drag turns the camera, a tap focuses there, two
  fingers zoom the lens. The settings come in a sheet from the bottom of an upright screen or a
  panel at the right of a wide one, inside the screen's cut-outs, scrolled with a finger.
- The bus preview of the launcher draws two pictures ahead while it turns, the JPEG is made on
  its own thread, and a turning bus is drawn smaller on a phone (the picture at rest is sharp),
  so the preview keeps up with the screen instead of about 20 pictures a second.

**Weather and light (Enhanced)**

- The eye adapts the way an eye does: a picture much darker than the light model expects
  (a dark cab, an underpass) is made up for over some three seconds - five-sixths of it,
  across six stops, where a bright surplus (snow, the sun in view) still only darkens the
  picture a little - and a dazzle on coming out passes in about a second. This is for the
  day: through the dusk it gives way to the old gentle metering, so the blue hour and the
  twilight of a summer night are not shown brighter than the sun under the horizon makes
  them (a white night comes only where the sun stays a few degrees below it). The
  Enhanced+ picture takes the same metering.
- The sky's occlusion by houses, trees and hills is worked out on the street's scale (18 m),
  not only the first two metres: a narrow street or the shade under a crown is darker than a
  square, and the surfaces round a point light it with what they send back (the last
  frame's picture, colour included: a sunlit wall lights the one across the street, lit
  windows light the street at night). The light is read where the surface was in the last
  frame's picture, so it follows a turning head and a moving bus, and not at all after a
  cut (another view, a jump) or where a surface was hidden then; the cab's own dashboard
  and pillars do not shade the street. It is the Enhanced picture's with the AO on: the
  other pipelines pay nothing for it (two-channel targets, no history picture, the old
  blur). A lamp or a lit sign among the surfaces does not stand in the picture as a bright
  square. The `street_light` setting (Graphics: Street light: Auto, On, Off) switches it.
  It is off by default; On enables it, and Auto enables it except on a small or shared
  graphics chip. The optimiser turns it off when the
  GPU is the limit. `OMSI_WIDE_AO=reach,strength,gain,max` tunes it.
- The physical weather model keeps the water of the grey deck (its optical depth, a thin
  stratus to a raining nimbostratus), so a rainy or winter day is as bright or as dark as
  its cloud is thick. It also gives the cumulus layer its own base (the condensation level)
  and depth (a shallow sheet under a high, towers in moist rising air), altocumulus, and
  thunderstorms.
- A thunderstorm has lightning: flashes of several strokes light the clouds from inside and
  the street with a cold light, a cloud-to-ground stroke draws its crooked channel, and the
  thunder is heard after the distance's delay, from the flash's direction. The flashes are
  worked out for every pipeline from the day's seed (a replay of a day repeats them), so the
  thunder of a storm is there without the Enhanced picture. `OMSI_STORM=activity[,azimuth]`
  shows one.
- The sun's shafts between houses and trees are followed up to 420 m: the first 180 m in
  the same steps as before, the rest in a few wider ones.
- The cumulus layer's height follows the ground under the player, so a map in the hills
  keeps its clouds.

**Navigator and city map**

- The navigator and the city map draw every map in the style of Apple Maps, from the map
  itself: no map package is needed. On a map's first visit its layers are read off its tiles
  (some seconds on a big map, then from `~/.openomsi/cache/mapgen`): green land, painted
  fields, sand and paved yards, water where a tile's water stands over its ground, woods
  where trees stand close and the single trees, rail lines drawn as Google Maps draws them,
  tram tracks, and every paved surface as it lies, from the splines' road profiles and the
  junctions' and car parks' own flat faces, so a car park is one grey yard with a fine
  edge instead of ten road ribbons. Outside the map's tiles the land is green.
- The houses stand in 3D with lit walls and soft shadows, taken from the roofs of the map's
  own models: on the navigator always, on the city map when zoomed in, where it tilts into 3D,
  and flat from above when zoomed out (the `3D buildings on the map` setting, and the city
  map's 2D/3D button). The route shows faintly through a house that hides it.
- The navigator is a rounded map with floating cards: the next turn and its street top left,
  the time, day and temperatures top right, the speed with the limit sign bottom left (red
  when too fast), the next stop with its line, distance, time and delay at the bottom.
- The navigator is a widget: drag it anywhere and size it at its bottom right corner (the
  `Navigator as a widget` setting; the size is kept, and in the launcher's settings).
- Every bus, tram and train of the timetable is on the maps where the timetable has it now,
  anywhere on the map, gliding along its route; from far the city map shows them as their
  lines' badges. The other players of a LAN session or server show as markers with names.
- Street names on maps without the stock street name signs: an add-on's own name plates and
  the stops named after their street name the street.
- The launcher's route preview draws the same picture: ground, water, rail, paved surfaces
  and houses, the route in blue, the stops on white cards.
- Road markings on the map when zoomed in: lane lines and their dashes, edge lines, stop
  lines, crossings and arrows, white and yellow, read off the road textures themselves (a
  spline's texture laid by its own scale, an object's flat faces by their texture
  coordinates), so a map maker's own textures work without a list. Light all-over textures
  (concrete, cobbles), worn blotches and flecks are not taken for paint.
- The city map closes with its cross or Escape only: a click beside it no longer closes it.
- The dark rim of a carriageway on the map is a kerb's breadth (it was a pavement's, and ate
  the outer lanes of a road without pavements, so the lane lines seemed to drift towards its
  edge); pavements stay dark all over. Paint that reaches past the paving is left out.
- Bends of wide roads on the map are round: a road of many profiles was cut in chords of
  ten metres or more on a turn (the game's own limit on a spline type's cross-sections), now
  at least every degree. Painted patches of ground (car parks, fields, sand) are read
  at about a metre and a half instead of five, so their outlines are finer.
- Houses on the map are the models themselves: zoomed in (within 1500 m of the view, the
  nearest 200 000 triangles; the in-game panel 30 000), a house is drawn in 3D from its model's
  own faces without textures (gabled and ridged roofs, wings, stepped towers, canopies on
  posts - a canopy or a bridge over a street is a roof in the air, no longer a solid block
  that shuts the street), each model kept once however often it stands; farther out, and
  past the budget, a house is its footprint with walls. Their flat outline is traced four
  times finer. Hairline gaps between pieces of paving no
  longer grow dark beads along a street's rim.
- More houses on the map: stations, railway halls and other buildings in folders with `road`
  in their name were never taken for houses; blocks of many buildings in one object (the
  tower blocks, a swimming pool complex) were dropped for their yards and are now drawn, each
  house with its own height; garages and greenhouses count too.

- **Troubleshooting** in the launcher's Help, below Tutorials: what is wrong with the
  installed content, by the bus or pack it concerns - packs waiting for their bus, buses
  taking parts from packs that are not installed, map HOF files that could not be added,
  failed installs. "Scan again" looks afresh after installing what was missing and says how
  many problems are gone; the menu entry shows how many are left.
- **Model test** (Troubleshooting's second tab, the launcher's former Model test page): any
  content is loaded the way the game loads it and every warning of its files is listed - a
  bus with the parts coupled behind it, a whole map (every tile, object and spline it places,
  what of it is not installed, its ground textures and AI lists; optionally every model,
  mesh and texture of it), an installed mod (picked from a searchable list), or any file or
  folder: a scenery object, a spline, a human, a weather, a mesh, a mod not installed yet
  (folder or zip). A folder's counts filter what it holds by kind. From the command line:
  `openomsi --check <file or folder> [--deep]`.
- Save slots: the pause menu's "Save to a slot..." saves to a new slot or over one of the
  map's slots, and the launcher's Continue card deletes saves [#2556](https://github.com/openOMSI-org/openOMSI/issues/2556).
- `[visible]` conditions combine: several `[visible]` of a mesh must all hold,
  `[visible_range] var min max`, a `!` after the variable turns a condition round, and a
  `||` line between blocks gives either-or (docs/MODDING.md) [#729](https://github.com/openOMSI-org/openOMSI/issues/729).
- Touch pedals: a rest zone at the foot of the pedals (a finger laid there presses nothing),
  a softer start of their travel, and their length in the settings [#1458](https://github.com/openOMSI-org/openOMSI/issues/1458).
- Rain, made anew: a quarter of a million drops worked out on the GPU each frame instead of
  a few thousand sprites - a downpour is a veil of many short, fine, faint lines, not a few
  long thick ones. Each streak is the drop's way against the eye in a seventieth of a
  second, as wide as the drop (fainter where that is less than a pixel) and as faint as it
  is long; the drops are as big and fall as fast as the rain is heavy (Marshall-Palmer drop
  sizes, their measured fall speeds), upright standing, slanting at the windscreen while
  driving; where they land, small splashes on the road, the kerb, a roof. The rain stays
  put in the world: turning or moving the camera round the bus no longer swings it [#1412](https://github.com/openOMSI-org/openOMSI/issues/1412).
- Rain and snow stop at roofs: a map of what stands over the ground round the camera, seen
  from above, keeps them out of every bus and car, under bridges, shelters and eaves - from
  the outside view as well, where it rained and snowed in the saloon.
- Snow builds up while it falls - a white cover in a quarter of an hour of a heavy snowfall,
  an hour or two of a light one - on the roofs of the buses, the cars and the houses, the
  verges and the ground, and thaws above zero and in the rain; the roads stay clear unless
  the weather lays snow on them.
- Enhanced: a wet bus mirrors the street - the water on its paint is as smooth as glass.
- Driving off the road soils the bus: the wheels on earth, grass or gravel throw it up onto
  the body (the dirt overlay), mud in the wet in a few minutes, dust in the dry more slowly.

**Multiplayer: public servers, the player list, machine bans**

- A server can list itself in a **public server list**: `public_server = 1` in `server.cfg`.
  It registers with a list service (`services/serverlist`, a Cloudflare Worker, which reads the
  server's own status page for a challenge before listing it, forgets it three minutes after
  its last word and limits abuse) and says it is alive every minute; Multiplayer -> Servers
  in the launcher shows the **Public servers** with name, map, players, version and message,
  and joins or adds one. The name, message and icon are checked by the same moderation on the
  server before it registers (a name or message that is flagged is refused, with the reason in
  the log) and in every launcher before it shows them (a flagged text is covered). A custom
  icon is shown only when the viewer's own image check cleared it; none is bundled yet (the
  library planned for it cannot be shipped, see docs/SERVER.md), so the public list shows the
  default icon for now.
- The game menu has a **Player list** in every multiplayer session: everybody's name, number,
  what they do and drive, and who is the host, an admin or a moderator. For the host of a game
  and for the admins and moderators of a server it is also where to send a player away with a
  reason, ban their **machine** - for this session or (admins) for all their sessions, for a
  day, a week or for good - and lift bans.
- **Machine bans**: the game sends a hash of the machine's own id (never the id itself, no
  serial numbers) when it joins, and a host turns away a banned machine whatever name, address
  or game it comes back with. The session's list is `bans.json` beside `server.cfg`; "all my
  sessions" is `~/.openomsi/bans-owner.json`, read by every session that account hosts
  (`owner_bans` in `server.cfg` to share one list between servers). The web panel has the
  bans with the button to lift them. A player can switch the identifier off
  (`OMSI_NO_HARDWARE_ID=1`), a server can turn such players away (`require_hardware_id = 1`).
  This stops the player who comes back, not one who works at it (docs/SERVER.md says how).

**Head tracking**

- Tobii eye trackers (the Eye Tracker 5, the 4C, the ones built into laptops) track the head
  natively on Windows, without opentrack: the game reads the Tobii Experience's own Stream
  Engine (`Head tracking with`: Automatic takes opentrack while it sends, else the Tobii).
  F1 takes where the head is as the middle. The tracker's trembling is smoothed away while
  big turns still follow at once (the `Smoothing` and `Dead zone` settings), and a look at
  the screen's left or right edge can turn the view further (`Look further at the screen's
  edges`). Every tracker's poses are drawn frame by frame, and a lost tracker glides the
  view home instead of snapping it there.

**Launcher**

- The Drive page puts the map, the line and tour, the roadbook, the bus and the start on one
  screen, in the layout the community chose: **Big Map** (the default) lays the route map over
  the whole page with the rest floating on it, framed in the part the panels leave free;
  **Control Room**, **Classic+** and **Classic** (in steps) are in **Settings > Launcher
  layout**. The page with cards, where the map came far down, is gone; its tour buses, next
  departures, map note, date calendar and save deletion are on every layout.
- The route map has a light and a dark look after Apple Maps, as the launcher's theme.
- A big map's route preview shows in about a second where it took over a minute (Berlin-Spandau
  with its paved surfaces: 88 s before), and only the part in view is drawn when zoomed in.
- Each tour says how long its trips take.
- The menu on the left is in parts - Play (Drive, Multiplayer, Running games), Content (Mods,
  Timetable), Options (Settings, Controls, Setup) and Help (Tutorials); the driver at its
  bottom opens the profile. It folds to its icons with the arrow at its bottom, and stays so;
  the numbers on Mods and Running games stay on their icons.
- An **Add-ons** page (Content > Add-ons; on a phone under More) lists projects others make
  for openOMSI, each with its author, a short description and a button that opens its website
  in the browser after saying which site it goes to. It is a list of links only: nothing is
  downloaded or installed. A developer adds an add-on with a pull request of one entry, see
  [docs/ADDONS.md](docs/ADDONS.md).

- The launcher's route map opens fast and follows the mouse smoothly on a big map: the
  roads and the route show at once and the surfaces (paved roads, land, houses) a moment
  later, read once per map and date, not again with each trip, and only the part in view is
  prepared; they come from the game as they are instead of being read and written again on the
  way; dragging and zooming move a picture of the map and draw it anew when the mouse lets
  go; the roads are drawn in a few strokes instead of one per road. Around the map's tiles
  the land has the map's own green, no more grey that looked as if it were still loading.

**Launcher**

- **The timetable editor works on the map**, as Drive does: every stop of the timetable stands on
  the map (the depots as amber squares), a stop clicked says which trips call there and puts one
  into the tour, and a stop found by name flies the map to it. A new trip is made by clicking its
  stops: only the stops the map has a station link to are ringed, the route it would drive is drawn
  as they are chosen, and it is saved as a `.ttp` beside the line (the changed trip of a stop list
  is saved as a new one, so the map's own stay as they are). The tours show their trips with a
  departure field and a minute up and down; copies, repeats and the reset moved into small menus.
- The route preview no longer runs doubled: the timetable lists the lanes of a road side by side
  (three lanes of the same stretch, one after another), which were all drawn, and a change of
  lane showed as a step sideways. The route is one line now, with a smooth curve over where it
  changes lane, as the game's own map draws it.
- No text of the launcher is set in capitals any more (the section titles, the menu's groups, the
  destination sign); the stops under a trip in the roadbook have a rail and dots of their own
  that no longer run over their times, a trip's depot-run label no longer sits on its stops, the
  panels of the Drive page keep their width on a small window, and the menu folds to its icons
  on a narrow one.

**Trams, metros and trains** ([docs/RAIL.md](docs/RAIL.md))

- Rail vehicles have a class (tram, metro, train), from `[rail_class]` in their file or from
  what it declares (a `[contact_shoe]` is a metro, a car over 45 t a train), and rail lanes
  a track kind (street or railway, third rail, tunnel, speed), from a map's new
  `openomsi-rail.cfg`, from the names of its spline profiles (`tram_*`, `metro_*`, `rail_*`)
  or from where the track lies. A road vehicle is never taken for a rail one.
- A rail vehicle moves by the forces of railway engineering instead of the road tyres of a bus
  and a made-up pull: running, air and curve resistance, the grade under it, wheel-rail
  adhesion (less on wet rails, leaves, snow and ice), drive and brakes limited by it - the
  driven wheels spin, the braked ones slide - and a tram's magnetic track brake. A vehicle
  scripted for the AI only is driven by the pedals with the typical performance of its class.
- **Pantographs.** A rail vehicle with a pantograph moves only with it raised. The game finds
  it from `[pantograph_var]` / `[pantograph_trigger]` in the vehicle's file or, for a mod
  without them, from its pantograph meshes and the variable they turn on (the Tatra T6B5's is
  `door_0`, which is then no longer a door: no door key, no boarding, no "door open while
  driving" penalty). Shift+P raises and lowers it.
- **Bogie frames of mods** (the Tatra T6B5's `_trail.bus`: no `[boogies]`, one axle, one coupling
  named) hang by their origin on the coupling of the car they belong to, as the mod is made for
  OMSI 2, and stand on the track by their own wheelbase: they stood two to three metres out
  of place, both under the middle of the car.
- Cars of a train no longer jump back and forth by a few centimetres in bends: a part just
  behind the vehicle was placed from the last 10 cm sample of the track behind it, a part just
  ahead from the track itself.
- The speed limit of a rail lane is brought down to what its curve allows (a tram loop, the
  diverging branch of a turnout).

**Road vehicles' physics** (new setting **Realistic road physics**, off by default; Esc menu, Settings)

- With it on the weather's wind leans and pushes a bus and a denser or thinner air changes the
  drag (the vehicle's scripts already have their still-air drag on the brakes, so only the
  difference is added); grip and rolling resistance depend on what each tyre rolls on, wet or
  dry (cobbles, dirt, grass, gravel, snow), rolling resistance also rises with speed and load;
  and the road is as uneven as ISO 8608 says its surface is, which shakes a bus on cobbles,
  gravel and the joints of a concrete-slab road (#3069).
- **Tyres hold across by slip angle**, as real tyres do, instead of OMSI's rule that the body
  follows its wheels exactly until the grip is passed: the contact patch lags behind the wheel
  and pulls back by the cornering stiffness, up to the grip shared with the longitudinal force
  by the friction ellipse. A bus follows its wheels at walking pace, turns in over 0.15-0.2 s at
  50 km/h, understeers a little at 80 km/h (yaw rate 0.92-0.98 of the geometry's), loses its hold
  across on a locked or spinning wheel. `cargo run -p omsi-sim --example handling -- <file.bus>...
  --kmh 50 [--realistic]` compares the two models; `OMSI_REALISTIC_ROAD=1` turns it on for one run.

### Fixes

- **The launcher's bus preview is no longer misted over**: the bus is made for it (the
  showroom and `--export-glb`) on a mild day (22 °C) instead of at 0 °C, the temperature a
  vehicle starts at without a weather. The
  Kajosoft Citaro, Citaro Facelift and 628 fog every window up in their {init} below 10 °C,
  so the preview always showed them misted; in the game the map's weather still decides.
- **Köln's houses on Engeldorfer Straße are back**: a binary `.x` string ended by a WORD
  token, as PAD-Labs' exporter writes it, no longer swallows the `}` after it. Read as the
  documented DWORD, the house's material never closed, the model failed to load and its 25
  binary houses were missing since 0.3.38.
- **Bus shelters show through a bus's windows** instead of a white shape in Enhanced and
  Enhanced+, or nothing at all in Vanilla. Grundorf's shelter is drawn "after vehicles"
  (`[rendertype] 4`) and the bus's panes write depth, so it failed the depth test behind them,
  and the houses behind it were already gone. The blended draws of `[rendertype]` 3 and 4 are
  now sorted far to near with the normal objects', after the opaque draws of all of them
  ([#2852](https://github.com/openOMSI-org/openOMSI/issues/2852)).
- **Destination displays light up at night in Enhanced and Enhanced+**: a script's screen is
  never taken for a pane. The EMT Citaro's Krueger matrix (Mallorca) is blended, writes no
  depth and draws its letters through a `\S:n` transparency map - the layout of a window - so
  the Enhanced pictures shaded it as glass, which gives no light of its own: its light map was
  left out and its letters stayed grey at night, where Vanilla+ lights them
  ([#2927](https://github.com/openOMSI-org/openOMSI/issues/2927)).
- **London's road markings and red bus lanes stay put** when the camera moves away or around
  the bus ([#2853](https://github.com/openOMSI-org/openOMSI/issues/2853)). A cut-out strip lying
  just over the driving surface its spline declares (`[heightprofile]`) is paint, as one lying
  just over the spline's line already was: Road-hog123's British markings lie 5 mm over a
  10 cm surface. A spline laid on an earlier spline of its tile at the same height is drawn
  over it, as Omsi.exe draws a tile's splines in order (the red lanes over the road fought it
  for every pixel). And a spline laid on a `[surface]` object - Crystal Palace's carriageway is
  one, its own tile's or the next one's - is pulled towards the eye as the object is, which
  covered it from a few metres away or from above. `OMSI_NO_DRAW_ORDER_DEPTH=1` turns the
  last two off.
- **A paint scheme's `[setvar]` is what the bus shows**, also when a script writes the same
  variable every frame: the scheme's values are set again after the scripts, as Omsi.exe
  draws them. The Kajosoft Jelcz Vectus with the JGSP Novi Sad repaint chooses the glazed
  doors (`v_drzwi 1`) and had the standard doors' white frames
  ([#3183](https://github.com/openOMSI-org/openOMSI/issues/3183)).
- **The MAN DN95 is no longer a mirror**: a texture a model asks for as `.dds` that is not
  there is missing, as in OMSI, instead of being stood in for by a `.bmp` or another file of the
  same name. The DN95's body asks for an `envmap.dds` it does not ship; its `envmap.bmp` (the
  windows' sphere map) at the body's factor of 10 drew the paint over with the reflection, where
  OMSI shows the paint without one.
- With the new setting **Realistic road physics** on, road damage shakes a vehicle again: cobbles, gravel, dirt, grass, snow and the joints of a concrete-slab road lift each tyre by what ISO 8608 gives such a road, where only the map's geometry did before ([#3069](https://github.com/openOMSI-org/openOMSI/issues/3069)).
- **A much lighter Enhanced frame by day**: since 0.2.42 the AI cars were lent headlamp beams
  whenever their lights were on, and in a grey winter noon (cars' lights on, no street lamp lit)
  those twelve cars' beams cost 13 ms of a 25 ms frame on an M4 - any light at all in the
  scene makes every pixel look through the lamp grid and has the weather's fog work the lamps'
  air light out. They are lent only while the scene is dark enough for them to show (the sun's
  height and the weather's gloom, the larger, is 0.12 or more): in a grey or rainy day the three
  nearest cars within 60 m, in the dark the twelve within 200 m as before. In a clear day, 27.4
  ms -> 13.0 ms of GPU a frame; in a heavy grey snow day 27.6 ms -> 19 ms; the night picture is
  as before.
- **AI cars and buses are drawn with their model's lower levels of detail** (`[LOD]`, as
  Omsi.exe picks them by the vehicle's size on the screen, and as the scenery's levels are):
  only the first level was loaded, so each AI vehicle was all its meshes at any distance. A
  model with interior lamps, displays, switched textures or a lit or animated lower level (a bus,
  a tram) keeps its full meshes.
  `OMSI_FORCE_LOD=1` draws the lower levels at any size, for a look at them.
- **Timetable buses no longer stall the frame when they appear**: a departure whose vehicle was
  not on the graphics card yet (the clock set far ahead, a type nothing read ahead) had its
  meshes and textures built on the frame it appeared on - 120 to 300 ms; it is tried again
  once the vehicle is read ahead. A parallel loop's caller sleeps while it waits for the items
  a helper has started instead of spinning.
- Enhanced: no more snow, or any ground, glowing orange from a small light far away. The ground took
  the map's baked lamp pools on top of the lamps themselves, three times over, through walls and
  roofs and with a tail sixteen radii long: the snow of a courtyard behind a building lit by the
  depot's floodlights glowed yellow at night, and so did the ground round a lamp whose pool ended
  long before. The ground is lit by the lamps alone now, with their shadows (Vanilla and
  Vanilla+ keep OMSI's baked light maps). The floodlights and the canopies of petrol stations
  (declared 10 to 20 m, held to 15 m and dimmed to a fifth as if they were a building's window
  glow) are lights as declared in the Enhanced picture, with a light of their own beside the held
  one the plain pictures keep.
- Review of the round: `hwban 3 session é` (a multi-byte last character of a duration) no longer
  stops a dedicated server; the hardware bans are given to every session a game hosts, not only
  the first of a run; the host passes voice frames only to players that have voice chat on
  (they ask `voice?`), the TACKs of the traffic table are rate limited and a player's open
  microphone sends nothing while an administrator has silenced them; a `server.cfg` without
  `voice` has voice chat off (the new file says `voice = 1`); the table of the AI traffic is
  written only while the game hosts, and a bus's sign in it once a second; the seated people's
  seat-back shift is worked out when the pose changes; the wire of a trolleybus map is read
  once per map, drawn once per scene, and a bus's poles and frogs leave the scene with it; a
  microphone that is gone is not opened again every few milliseconds; the beam shadows look up
  their map only where a beam shines; a plain bus is a trolleybus only if its "pole" meshes are
  long and thin; the server list does not let a registration take over another server's
  address, counts an IPv6 sender by its /64, and compares a status page's name as the listing
  has it; the glass of a dented body keeps the snow's coordinates; new texts in the options and
  the player list have translations.
- **Trolleybus poles stay on the wire over bumps and joints**: a shoe that a bump lifts for
  a moment no longer comes off (it has to stay out of its groove for 0.15 s, or leave it by
  more than three groove depths), the groove is deeper and bends are taken faster.
- **Trolleybus poles no longer come off at the map's frogs whatever the bus does**: a map's
  junctions are drawn with the three wires up to a metre apart or a branch started from the
  middle of another wire, and only one frog in seventeen was found (on Bronitsyn), so the
  shoe ran off a wire that simply ended. The ends of a junction within 1.2 m are now one
  point, a branch from the middle of a wire cuts it into a frog, and the contact plate before
  a frog is found over short spans and joints too (a toe cut shorter than 9 m never set its
  frog). Power on takes the branch, coasting goes straight.

- The windows of Russian and other mod buses whose texture names are not plain ASCII (the
  ZiU-682G, the Trolza Optima) are see-through again: the `.o3d` keeps a texture's name in
  the file's own bytes and the `.cfg` as text, so a mod's `[matl_alpha]` for its glass never
  found the slot and the windows were opaque. Both names are now compared as the bytes
  Omsi.exe sees.
- The snow on the windscreen and the cab's windows no longer comes out in swirls and bands,
  with huge flakes beside tiny ones: the film's coordinates in metres were laid out by a
  normal worked out at every corner, which turned the plane a little at each vertex of a
  curved pane. A pane (the film triangles that meet) is unrolled on one plane now.
- A trolleybus's poles are taken with the mouse: a click on the cab's pole switch, or on a
  pole from outside the cab, takes it in hand, it follows the cursor (the tip goes to the
  wire the cursor points at, or where it points when there is none), and the next click lets
  it go and it springs on to the wire. A pole on the wire can be taken off again the same way.
  A plain trolleybus shows its own pole mesh turning with the pole (the raised mesh is turned
  about its hinge by the pole's angles) instead of a drawn boom, and from the driver's seat one
  click on the switch is enough: the motor takes the pole to the wire nearest its tip.

- Road markings no longer disappear and come back as the bus drives towards them
  ([#3085](https://github.com/openOMSI-org/openOMSI/issues/3085)). The depth a road is pulled
  towards the eye (so that it wins over the ground beside it) stopped growing 12 m out, and
  across the long triangles of a straight road that turned its plane into a bowl: from a low
  eye the road came in front of the lines on it from 25 m on. A flat surface keeps the same
  share of the way to the eye all the way now; upright faces (a pole a mod tagged as a road)
  keep the cap. A new graphics test lays a marking on a road of 200 m triangles and looks
  at it from 4 to 100 m.
- The mirrors show the sun's shadows and the smoke and spray again
  ([#3051](https://github.com/openOMSI-org/openOMSI/issues/3051)): they look up the shadow
  maps the window last drew instead of drawing their own, and draw the particles the window
  has. Settings `mirror_shadows` and `mirror_particles` (both off until switched on: they cost a
  little in every frame, most in snow) turn them on for a slower
  machine, as OMSI 2's "No PS in reflections" did.
- Rain on the glass follows the rain: a drizzle leaves small beads, a downpour full drops,
  and the runners of a downpour are heavier, wider and quicker, with wider cleared tracks;
  the drops on an upright side window are smaller than on the sloping windscreen
  ([#3076](https://github.com/openOMSI-org/openOMSI/issues/3076)).
- A bus that declares air pressure variables it never sets (a shared varlist) no longer
  says "Air pressure is low" and "keep the engine running"
  ([#3030](https://github.com/openOMSI-org/openOMSI/issues/3030)): only a script that
  assigns the spring brake or tank pressures counts as a bus with air brakes.
- A passenger on foot in a bus (another player's too) looks the way the bus does: the view
  turns with the bus through a bend and keeps its own look as an offset
  ([#3093](https://github.com/openOMSI-org/openOMSI/issues/3093)).
- Winsenburg's Hauptbahnhof 4 crossing (win-4.sco): a bus asking at the second bus light
  alone gets its green. A point the program had just jumped to counted as a second visit
  and sent the clock round for good
  ([#3092](https://github.com/openOMSI-org/openOMSI/issues/3092)).
- Objects attached to a spline stand where the editor put them on maps with
  `[worldcoordinates]` (Shanghai and others): the chain distance of a spline was not scaled
  with the tile like the lengths and the rows' start distances were, so the first object of
  a row - and the whole row after it - stood off by that difference on tiles away from the
  map's middle row ([#3080](https://github.com/openOMSI-org/openOMSI/issues/3080)).
- `OMSI_PRESENT_MODE=fifo|fifo_relaxed|mailbox|immediate|auto` picks the swapchain's present mode
  for a test, when the surface has it (vsync stays `AutoVsync`, so a game just under the
  refresh rate does not drop to half of it): Vulkan reports of vsync that tears
  ([#2838](https://github.com/openOMSI-org/openOMSI/issues/2838)) can try `fifo`.

- Passengers drawn with the high quality avatars no longer sit with their backs through the
  seat back (robes and long hair on women's avatars): a seated avatar is moved forward on
  the seat as far as its back needs. Waiting passengers stand with a phone in hand and arms
  crossed less often.
- On a phone held upright an open page of the game menu (Options and the rest) takes the whole
  width: beside the menu it was a strip of switches with no names. Its tabs go on a second row
  instead of being cut off, and the cab panel's buttons fit a narrow screen (they ran off both
  sides). The top row of buttons shrinks to fit instead of running into itself in the free
  camera and on foot, and the cab panel's backdrop covers all its buttons.
- Photo mode no longer lies under the driving controls (the wheel and pedals were drawn over
  its panel) and its pages are no longer a 40-pixel strip on a phone held on its side.

- Misted windows follow the bus's own cabin: a bus whose scripts keep their cabin air (`heizung.osc` of
  the stock buses and their descendants) fogs its glass from that air's temperature and humidity and
  clears it by the heating, the fans and the vent windows, as its gauges say - they were misting from a
  second, guessed cabin (always 20 degrees, three air changes an hour) that no control of the bus touched, so
  at 17 inside and 7 outside the panes of a bus with a few people aboard stayed foggy for good. The
  windscreen's defroster blows the heater's own air when its fan runs. Buses without such scripts keep
  the built-in cabin, now with the fresh air a city bus brings in for each person (15 m3/h).
- Snow on windows is laid out in metres of glass, not in the layer's own texture coordinates: it came out
  as long streaks on the rear windows of double-deckers and articulated buses (the rear window
  shared a mask with the side windows and lay edge-on in it) and sparse on the small tilt windows and the
  doors (their coordinates squeezed the snow picture into a few centimetres). Every window of a bus now
  holds the same snow, in Vanilla, Vanilla+ and Enhanced.
- Headlights and lamps in fog and rain: a bright beam no longer carries a pattern of stripes (the soft
  light the fog scatters is upsampled with a weight that did not tell a road's rows from a step in depth),
  and the picture of a beam's cone and of a lamp's glare is smoothed between its texels, so a large cone
  has no stair of small squares along its edge. The classic picture's lamp glare ends softly at its sprite's
  edge, whatever the bitmap does there.
- Tyre spray comes only from puddles: the stock cars' and buses' wheel spray no longer sprays
  on every wet road from the first drops, and it stops as the puddles dry out after the rain
  instead of lingering until the road is quite dry. In snow the scripts' own slush is kept.

**Navigator**

- Traffic on the navigator follows how fast the traffic flows: busy streets that move are no
  longer amber, and two cars waiting at a red light or a bus at its stop are no jam; slow,
  heavy and standing traffic show amber, red and dark red. Junction paths take the colour of
  their traffic too (they were always blue).
- A way back to the route is the quickest to the end of the route, counted in time with turns
  and turning round, and may change lanes on the way: it no longer leads back to a part of
  the route already driven, nor round a block to reach a lane beside the bus. The way to a
  stop before the route is there comes to the stop on its own side of the road.

**Graphics**

- Grass and earth no longer shine like wet asphalt in the rain: a ground texture with
  `[moisture]` on grass, earth or gravel soaks the rain up instead of carrying a film of
  water, and a `[moisture]` road without `[puddles]` gets a lighter film with fewer pools
  (the stock grass strips glowed in every lamp at night).
- Tinted bus windows on an alpha-tested layer that shares its texture with the body are
  blended above OMSI's cut instead of drawn black (Ychebniy LiAZ 5292.65, NefAZ 5299 2024)
  [#2609](https://github.com/openOMSI-org/openOMSI/issues/2609).
- A road's painted lines (also cut-out dashes) are drawn over the crossings and junction
  plates laid over the road, as in OMSI 2, instead of vanishing under them with the camera's
  height and distance [#1694](https://github.com/openOMSI-org/openOMSI/issues/1694).
- Night in the city costs less: the classic picture lights a place with its eight strongest
  lamps, as Direct3D lit OMSI 2's, and every light grid cell keeps its strongest lamps
  rather than the first ones listed [#1467](https://github.com/openOMSI-org/openOMSI/issues/1467).
- It rains again on computers that draw with the reduced set of pipelines after a graphics
  failure (`~/.openomsi/gpu-fallback.cfg`): since 0.3.34 the new rain was left out there, and
  an Apple M1 that fell back once had no streaks for the rest of the version. Buses and
  shelters keep the rain out there as well.

**Controls**

- The steering wheel comes back after a turn the way a bus's does: gently at town speed and
  damped on its way back, without swinging past the centre [#2103](https://github.com/openOMSI-org/openOMSI/issues/2103).

**Launcher**

- The bus picker's preview and details fill their side of the window for every bus: a bus
  with a short description no longer leaves an empty strip left of its preview.

**Traffic and timetable**

- A test covers buses behind a bus standing long at a shared stop: each serves the stop from
  behind and goes round it [#1954](https://github.com/openOMSI-org/openOMSI/issues/1954).

**Traffic and timetable**

- Timetable buses appear when their tour runs, as in OMSI 2: the first trip of a tour comes
  at its departure instead of a quarter of an hour early at its first stop, and a tour under
  way has its bus waiting at the first stop between trips, at stops other lines use too
  ([#2595](https://github.com/openOMSI-org/openOMSI/issues/2595), [#2593](https://github.com/openOMSI-org/openOMSI/issues/2593)).
- An AI bus that served a stop from behind the bus in front pulls up and boards again once
  the stop is free, instead of standing back until its departure ([#2591](https://github.com/openOMSI-org/openOMSI/issues/2591)).
- A traffic light's approach request follows the indicator: a path marked left or right asks
  only with that indicator, so a junction no longer requests every stage at once ([#2532](https://github.com/openOMSI-org/openOMSI/issues/2532)).
- Chrono scenarios switch the money system, the AI lists and the parking lists as in OMSI 2:
  a scenario's full AI list replaces the map's, an update replaces only the groups it names,
  and parking lists come from the newest scenario that has them ([#2315](https://github.com/openOMSI-org/openOMSI/issues/2315)).

- Departure displays at the stops show their buses again (HafenCity Hamburg's HVV boards
  stayed empty under the stop name and headings since 0.2.39), and other scripted scenery
  objects near the camera no longer miss their texts, sounds and animations while an object
  farther away is out of range ([#2619](https://github.com/openOMSI-org/openOMSI/issues/2619)).

**Vehicles and cameras**

- The rear axle of articulated buses with a steered rear section (Van Hool AG300, Ikarus
  280) steers, and its scripts get its steering angle ([#322](https://github.com/openOMSI-org/openOMSI/issues/322)).
- The rear section's driver camera comes after the front's first and last cab views, as in
  OMSI 2, instead of one key press away from the standard view ([#2503](https://github.com/openOMSI-org/openOMSI/issues/2503)).
- The driver's eye moves with the seat's suspension, one movement as in OMSI 2; the camera
  had a spring of its own that dipped the other way under braking ([#2550](https://github.com/openOMSI-org/openOMSI/issues/2550)).
- GetTicketName gives the ticket's display string, what ticket printers print ([#2483](https://github.com/openOMSI-org/openOMSI/issues/2483)).

**Controls**

- Another add-on's key in keyboard.cfg no longer takes Ctrl+Shift+M (mirror editor) and
  similar game shortcuts from buses that have no use for that key ([#2577](https://github.com/openOMSI-org/openOMSI/issues/2577)).
- Windows: Num4 and the Left arrow held together (Num Lock off) no longer leave the steering
  stuck ([#1830](https://github.com/openOMSI-org/openOMSI/issues/1830)).
- DirectInput axes are scaled from the range the driver really gives them, and an axis with
  its own range is logged (Fanatec pedals reading 50-100 %, [#521](https://github.com/openOMSI-org/openOMSI/issues/521)).
- Linux: only real controller axes are used, and a free X axis steers counted from where it
  rests, so a macro keypad's knob no longer steers the bus ([#868](https://github.com/openOMSI-org/openOMSI/issues/868)).

**Graphics and sound**

- **The video memory setting is kept:** "Texture memory" is now "Video memory", and it bounds
  everything the game holds on the graphics card - textures, meshes, render targets and shadow
  maps, the driver's own - not the textures alone. The game took gigabytes more than it was
  set to, and an 8 GB card could page to the system's memory and drop to a fraction of its
  frame rate. Where the driver says how much it really hands out (DirectX 12, Vulkan, Metal),
  the game stays under that as well, whatever the setting; when it is over, the farthest
  textures give up their finest levels first, and come back when there is room again.
  "Automatic" is nine tenths of the card's own memory. Choices of 8 and 12 GB were added,
  500 MB was dropped. A texture memory set before (and OMSI's `texmemlimit`) was a texture
  budget and is not carried over: the video memory starts on Automatic. After a lost graphics
  device the safer settings take two thirds of what the card holds, not a fixed 800 MB.
- **The frame rate no longer collapses after half an hour on DirectX 12 and Vulkan:** the game
  kept its own total under the video memory setting while the card filled up anyway - the room
  wgpu's allocator keeps free in its 128-256 MB blocks was left out of the bound, and a block
  goes back to the driver only once it is empty. Far textures made smaller left holes, the
  textures coming back needed bigger ones, so new blocks opened: on a 12 GB card set to 6 GB the
  driver held 11.5 GB after 18 minutes (5.7 GB of it holes), the card paged, 10 fps. The holes
  now count against the bound, so textures come back only into room the card really has, and
  every card gets the allocator's small blocks (8-64 MB), which empty and go back to the driver
  ([#2295](https://github.com/openOMSI-org/openOMSI/issues/2295)).
- **Rain on the windows costs less:** the drops on the cab's panes are painted beside the
  frame instead of while it waits, and only the rows that changed go to the graphics card (12
  panes sent a megabyte each, 30 times a second). In heavy rain in the cab, on a Radeon 680M,
  the panes' share of the frame went from 10.3 to 3.3 ms with Enhanced+ and from 2.3 to 0.7 ms
  with Vanilla+ (152 to 179 fps). The drops show one frame later ([#2490](https://github.com/openOMSI-org/openOMSI/issues/2490)).
- **Enhanced+ draws 8x MSAA at 4x:** 8x doubled the ray traced passes' cost (2.4 ms a frame on
  an RTX 4050) and their multisampled targets' memory for little to see; `OMSI_FULL_GPU=1`
  keeps 8x ([#2228](https://github.com/openOMSI-org/openOMSI/issues/2228)).
- A video memory of 16 GB can be chosen, for cards with more than 16 GB of their own.
- Textures stay sharp on a graphics card of its own: their share grows to half of the video
  memory (about 3.7 GB on an 8 GB card on Automatic), where it stayed at an eighth of the
  computer's memory (2 GB with 16 GB) whatever the setting, and the far textures lost their
  finest levels even at the highest settings. The video memory still bounds everything on
  the card; integrated graphics and the safer settings after a lost device keep the old share.
- A bus's invisible cover hides its sun blind again instead of showing a light blue patch over
  the windscreen and the street behind it, in Enhanced, and in Vanilla+ with ambient
  occlusion and MSAA off: since 0.3.0 the parts listed after the cover still went into the
  depth the picture is drawn against, also when the cover is a mesh of its own. The ambient
  occlusion still sees them ([#2678](https://github.com/openOMSI-org/openOMSI/issues/2678), [#2178](https://github.com/openOMSI-org/openOMSI/issues/2178)).
- Render targets of a size not drawn for 5 s are let go: those of a photo taken at a large size
  stayed on the graphics card until the window changed size.
- A remembered MSAA fallback is tried at the chosen MSAA again once with each new version
  (written down before the try, so a failing or crashing retry is not repeated at every
  start), instead of keeping a computer at 1x for good ([#2597](https://github.com/openOMSI-org/openOMSI/issues/2597)).
- The paper timetable keeps Courier New's spacing whatever font draws it, and Linux finds
  Courier-compatible fonts (Liberation Mono, Cousine, Nimbus Mono) ([#1878](https://github.com/openOMSI-org/openOMSI/issues/1878)).
- Scenery sounds carry as far as their range lets them, up to the 800 m within which scenery
  objects run, and fade out over the last part instead of stopping dead at 300 m ([#2534](https://github.com/openOMSI-org/openOMSI/issues/2534)).
- Shader tests cover OpenGL ES 3.1/3.2 per entry point and catch the nested constant-array
  lookups the Adreno 710 refused ([#2009](https://github.com/openOMSI-org/openOMSI/issues/2009)).
### Multiplayer

- **Voice chat, built in** - off until switched on (Settings → Sound → *Voice chat*, or the game
  menu's Sound page). Hold a talk key to speak: *nearby* (V) is heard by the players within
  the session's range (75 m by default) from where you stand, quieter with the distance and
  muffled when one of you is in a bus and the other is not; the *radio* (B) is heard by the
  whole session, in a small speaker's voice with a click when the key goes down and up.
  Both keys are in Controls (and bindable on a controller button; on a touch screen the cab
  panel has two talk buttons). The microphone is opened only while a key is held (or with the
  open microphone setting), the system asks for it the first time voice chat is switched on,
  and nothing is recorded or stored. Opus at 24 kbit/s with a jitter buffer and loss
  concealment; the host only relays the frames (LAN datagram `0xBC`, session-bound, rate
  limited) and needs no audio device, so dedicated servers carry voice too. The screen shows
  when you transmit and who talks; every other player can be muted in the game menu's
  *Player list* (kept by name), or all voices at once; an admin can silence a player for the
  whole session (`vmute` / `vunmute`, a button on the player's page). New `server.cfg` keys
  `voice`, `voice_radio` and `voice_range`, see [docs/VOICE.md](docs/VOICE.md) and
  [docs/SERVER.md](docs/SERVER.md). **Removed:** the GreenTeaSpeak plugin and its settings
  (`voice_chat`, and `voice_server_uid`, `voice_channel`, `voice_channel_password` in
  `server.cfg`, which an old file may keep: they are ignored and the log says so).
- The AI traffic goes over the LAN as a table of vehicles under ids of their own (LAN
  protocol 15, [docs/TRAFFIC_TABLE.md](docs/TRAFFIC_TABLE.md)): the host's traffic writes it after
  each step and every player is told the cars within 650 m of it as changes of what it
  acknowledged, the nearer ones more often, within 40 KB/s. A car that stands costs nothing, a
  moving one 6-7 bytes ten times a second (the old frames took 17 and told every car in
  range every time), lost datagrams are told again, and the clients draw the cars between and
  past the host's poses. A car's id is not reused for 30 s. A game of the older protocol is turned away
  with the usual message. The script context of the host's cars goes to each client often at
  first and then now and then, not every tenth of a second for every car in range.
- **What another machine sends is checked for what it is** — a host's mod file a joining
  player fetches must be what its name says (a picture of a size a texture can have, a sound,
  a mesh the reader takes, a text), or it is left out and reported; a vehicle script that
  would never end is cut short instead of stopping the game.
- **The dedicated server's administration** — an admin and a moderator role, a menu in the
  game and a web panel (`/admin/`) that use the same operations, an audit log, and a passenger
  density (`passenger_density`) besides `passengers` (see [docs/SERVER.md](docs/SERVER.md)).
  For a server set up before: an `admin_password` or `moderator_password` shorter than 12
  characters now lets nobody in (the log says so; the start scripts ask for a new one), and a
  `server.cfg` without `admin_web` has the web panel off until it says `admin_web = local`.
- **Every player sees another player's bus exactly as its driver has it** — the same files,
  the same paint — fetched from the server by its content when they lack it; a dedicated
  server keeps every version its players send (`accept_uploads`, `uploads_keep_days`) and may
  leave the map for the players to install (`share_maps`: a new `server.cfg` says 0; one
  without the key goes on passing the map on, as before).
- **Behaviour change:** a bus that cannot be had as its driver has it (the server takes no
  uploads, the upload failed, it is too large to fetch) is drawn with another bus — the
  viewer's copy of that path, else their own — and **labelled** under the player's name.
  Before, the viewer saw their own copy of that path without being told it was not the
  driver's. While a bus is still arriving, a neutral box of its size stands in.

## 0.3.0 - 2026-10-14

A stabilization release: the slowdowns and breakages that came with the updates since 0.2.20,
traced back to the changes that caused them and fixed.

### Performance

- **Big maps with a lot of AI traffic run smoothly again:** the AI vehicles' scripts run once
  a frame, as before 0.2.28, instead of in every 20 ms traffic step. At low frame rates they
  ran up to four times a frame, which made the frame slower still; articulated buses paid it
  for every section. Fewer "Simulation" stutters and no more slide to a few frames a second
  ([#2246](https://github.com/openOMSI-org/openOMSI/issues/2246), [#2388](https://github.com/openOMSI-org/openOMSI/issues/2388), [#2208](https://github.com/openOMSI-org/openOMSI/issues/2208), [#2464](https://github.com/openOMSI-org/openOMSI/issues/2464), [#2579](https://github.com/openOMSI-org/openOMSI/issues/2579)).
- Passengers, pedestrians, the timetable and the weather move on once a frame again, not in
  every tick of the fixed step (since 0.2.39).
- With a wheel or joystick plugged in (Windows), the game no longer looks for controllers
  every 300 ms: it waits longer and longer while nothing changes, and looks again as soon as
  something is plugged in or out. No more regular small hitches.
- openOMSI starts again on older processors without POPCNT ("this app can't run on your PC",
  0xC000001D) ([#2484](https://github.com/openOMSI-org/openOMSI/issues/2484)).

### Graphics

- **Vanilla and Vanilla+ sky:** no more clouds stretched into layers and streaks towards the
  horizon; past the edge of the cloud dome there are no clouds, as in OMSI 2.
- Ambient occlusion is as strong as before 0.2.46 again, and the sun's glare and the fog lamps
  no longer shine through roofs and bus ceilings; invisible covers still hide what is behind
  them ([#2558](https://github.com/openOMSI-org/openOMSI/issues/2558), [#2605](https://github.com/openOMSI-org/openOMSI/issues/2605), [#2059](https://github.com/openOMSI-org/openOMSI/issues/2059)).
- Enhanced: glossy lacquer is sharp and shiny again, while paint that reflects little stays
  satin ([#2563](https://github.com/openOMSI-org/openOMSI/issues/2563), [#2301](https://github.com/openOMSI-org/openOMSI/issues/2301)).

### Traffic, trains and maps

- **AI traffic no longer stands still for good at big junctions and roundabouts**
  ([#2373](https://github.com/openOMSI-org/openOMSI/issues/2373), [#2562](https://github.com/openOMSI-org/openOMSI/issues/2562), [#2592](https://github.com/openOMSI-org/openOMSI/issues/2592), [#2257](https://github.com/openOMSI-org/openOMSI/issues/2257)):
  - cars drive into a junction unless they would stop inside the path of another car, instead of
    waiting at the edge of a big crossing whenever anyone stood inside it;
  - cars that wait for each other in a circle sort it out after a few seconds: the one that
    has waited longest goes (never through another vehicle, a red light or a person);
  - a car waiting at its give-way line no longer takes the right of way from the cars going
    round a roundabout or along a priority road;
  - a car waiting at a red light inside a junction no longer holds up the cross traffic
    that has green.
- **AI trains and trams stay on their tracks:** every car is laid along the track on its two
  bogies at the couplings its files declare, instead of being pushed behind the car ahead as
  a rigid block (since 0.2.24 and 0.2.28).
- Articulated AI buses' rear sections follow smoothly between frames.
- The ground under splines and spline attachments laid with `[spline_terrain_align]` is cut
  to the right one again: no more broken terrain beside or under roads ([#2004](https://github.com/openOMSI-org/openOMSI/issues/2004)).

### Vehicles and passengers

- Start sounds and other triggered sounds are heard again: a sound started at zero volume
  takes the volume its curves give later ([#2552](https://github.com/openOMSI-org/openOMSI/issues/2552)).
- The in-game vehicle lists offer **Original paint** again, beside Random livery ([#2598](https://github.com/openOMSI-org/openOMSI/issues/2598)).
- The timetable sheet in the bus shows the timetable again ([#2575](https://github.com/openOMSI-org/openOMSI/issues/2575)).
- Passengers read the destination off what the bus's display shows again.
- Street lamps' shadows no longer switch between lamps at night after the mirrors are drawn
  (Enhanced) ([#2498](https://github.com/openOMSI-org/openOMSI/pull/2498)).
- Passengers: the original OMSI animations are the default again; the natural poses no
  longer raise a hand or put arms through the body, and people out of sight take fewer
  poses ([#2588](https://github.com/openOMSI-org/openOMSI/pull/2588)).

### Launcher

- Phones: the Mods page has its own storage browser to install a folder or a .zip, .7z or
  .rar archive; "Add folder" and ZIP did nothing ([#2513](https://github.com/openOMSI-org/openOMSI/issues/2513)).
- The bus preview shows the chosen paint in the classic Drive page too ([#2585](https://github.com/openOMSI-org/openOMSI/pull/2585)).
- Switching the update channel while the launcher is still looking for an update no longer
  leaves (or installs) the previous channel's result ([#2623](https://github.com/openOMSI-org/openOMSI/pull/2623)).
- iPhone and iPad: a first start welcomes the player, shows how to copy OMSI 2 onto the
  device, finds the copy by itself and says to turn the phone and drive; the welcome page
  shows the openOMSI logo ([#2500](https://github.com/openOMSI-org/openOMSI/pull/2500)).
- If the launcher crashes, its log now says where, so a report can be traced ([#2574](https://github.com/openOMSI-org/openOMSI/issues/2574)).
- When the graphics driver resets the card and the game restarts with lower graphics, the
  game now says so instead of quietly looking worse.

### Project

- **Releases are started by hand:** a push to `main` no longer publishes a release by itself.
  Maintainers run the release from Actions, and promote a tested beta to stable as before
  (see [docs/VERSIONING.md](docs/VERSIONING.md)).
- Promotion runs from `main` only and reads the release policy from `main` ([#2620](https://github.com/openOMSI-org/openOMSI/pull/2620)).
- Governance and pull request rules, CODEOWNERS, a secret scan on every pull request and
  Dependabot for the launcher's own crates ([#2622](https://github.com/openOMSI-org/openOMSI/pull/2622), [#2633](https://github.com/openOMSI-org/openOMSI/pull/2633)).

## 0.2.60 - 2026-10-14

### New

- **Beta and Stable releases:** pushes to `main` publish Beta releases for testers.
  Maintainers can promote a tested Beta to Stable without rebuilding or replacing
  its files. Releases include `SHA256SUMS`, and an already published version is
  never replaced ([#2565](https://github.com/openOMSI-org/openOMSI/pull/2565)).
- **Update channel in the launcher:** Settings → General → Updates offers Stable
  (the default, including for existing settings) and Beta. The Beta channel offers
  the newest compatible release, including prereleases, while skipping releases
  listed as broken. The version picker marks Beta releases, and Stable releases
  remain visible behind pages of newer Betas
  ([#2571](https://github.com/openOMSI-org/openOMSI/pull/2571)).

### Project and security checks

- The pull-request template asks authors to describe risks and check translations,
  compatibility and the absence of original OMSI 2 content
  ([#2566](https://github.com/openOMSI-org/openOMSI/pull/2566)).
- CodeQL analyses the project's Rust, JavaScript/TypeScript and GitHub Actions
  workflows; the custom scan excludes vendored launcher patches
  ([#2568](https://github.com/openOMSI-org/openOMSI/pull/2568)).
- The source distribution uses GNU AGPL v3 only and includes `TRADEMARKS.md`,
  retaining historical MIT and third-party notices
  ([#2599](https://github.com/openOMSI-org/openOMSI/pull/2599)).

### Known follow-ups

- Switching update channels while a lookup is already running can leave a result
  from the previous channel. The approval of #2571 accepts this as a follow-up;
  this release does not claim to fix that race.
- Dispatch Beta promotion from current `main`: running the promotion workflow
  from an older ref can read an older broken-release policy. This remains a
  follow-up from the review of #2565.

### Credits

The release workflow, launcher channels, review template and CodeQL changes were
authored by **Thibaut (@thibautgnt)**. The original openOMSI codebase was created
by **usonskyyyy**. The official organization is
[openOMSI.org (`openOMSI-org`)](https://github.com/openOMSI-org).

## 0.2.48 - 2026-10-13

Additional security hardening and verification for mods, maps and online sessions (thanks to @thibautgnt); the details are in the list below. **Online play needs 0.2.48 on every computer** (the session protocol moves from 6 to 7; 0.2.47 was not released; an older game is refused cleanly when it joins), and a dedicated server's `admin_password` needs at least 8 characters.

### New
- **Model test** in the launcher, for modders: a bus is loaded exactly as for a drive, without a window, and every warning and error of its `.bus`, model, meshes, scripts, constfiles and paint schemes is listed, with the textures that are not there; the running game's warnings show live beside it (`openomsi --check-vehicle`) ([#2524](https://github.com/openOMSI-org/openOMSI/pull/2524), by @icacar98-dot).
- **The navigator and the city map show the land under the roads** on a map that brings a package (`Maps/<Map>/openomsi-realmap`): water, parks and woods, rivers, rail and borders, in a dark or a light look; two new settings (Navigator surface, Light navigator). See docs/MAP_PACKAGE.md ([#2529](https://github.com/openOMSI-org/openOMSI/pull/2529), by @lmonteirotech).
- **The launcher grows with its window** on a big or wide screen, and has a **Launcher size** setting; **Launcher layout: Classic** brings back the Drive page in steps (Bus, Day & weather, Map & duty); the bus preview can be looked at from the **passenger's and the driver's seat**; the launcher asks less of a weaker computer and rests while you drive ([#2493](https://github.com/openOMSI-org/openOMSI/pull/2493), by @thibautgnt; it supersedes #1631).

### Fixes
- **Hardening** (by @thibautgnt): mod archives can no longer write outside the mod being installed; texture and other content names cannot point at another computer (`\\server\share`) or climb out of the content folders, nor can another player's roller blind and script strings; online sessions: the host is signed (a per-session key), so nobody can pose as it on the relay or for its mods, a player's place cannot be taken with an overheard hello, the session is not told to strangers from outside the network, and a dedicated server's admin login uses PBKDF2 instead of one hash over plain UDP; bans hold the player's address; the mods download, the WebSocket gateway and files that claim billions of entries can no longer exhaust memory or hang the game; crafted glTF, `.x`, TGA and zip files no longer crash it; Android: other apps can no longer fake an update's answer; the website shows issue text through an allowlist with a Content-Security-Policy.
- Hardening: the launcher's glib (GTK3, Linux) has the fix of RUSTSEC-2024-0429: its `VariantStrIter` wrote through a pointer it did not own mutably.
- Hardening: ruzstd 0.9.1, the zstd decoder of plugin packages (`.oop`): a tiny crafted file could make it ask for all the memory (RUSTSEC-2026-0344).
- **The loading screen has its plain dark background again**, without the map's picture (0.2.46 put it there, #1383).
- **Passengers read the destination from the IBIS**: the depot row the IBIS was set to, not the display script's own row - nobody boarded a bus showing a code typed past the list ([#2508](https://github.com/openOMSI-org/openOMSI/pull/2508), by @icacar98-dot).
- **With one door open, nobody waits at a shut door with a button**: they walk to the open one ([#2509](https://github.com/openOMSI-org/openOMSI/pull/2509), by @icacar98-dot).
- **People who get off where there is no pavement spread out beside the bus** instead of crowding the doorway, before they go ([#2510](https://github.com/openOMSI-org/openOMSI/pull/2510), by @icacar98-dot).
- **The map's drivers stay out of the passengers** also when its humans.txt names nobody installed ([#2511](https://github.com/openOMSI-org/openOMSI/pull/2511), by @icacar98-dot).
- **A repaint's folder no longer paints over the details of the other repaints in it**: only what its `.cti` names comes from there ([#2515](https://github.com/openOMSI-org/openOMSI/pull/2515), by @icacar98-dot).
- **A paint scheme's `[setvar]`s are written before every frame of the scripts**, as Omsi.exe does - the IK-201's display went back to Latin letters only so ([#2519](https://github.com/openOMSI-org/openOMSI/pull/2519), by @icacar98-dot).
- **Serbian and Croatian text in Windows-1250** (č, ć, đ with š or ž) is read as such, no longer as Western letters ([#2521](https://github.com/openOMSI-org/openOMSI/pull/2521), by @icacar98-dot).
- Mac: the app is no longer overwritten by the game program beside it ([#2522](https://github.com/openOMSI-org/openOMSI/pull/2522), by @thibautgnt).
- iPhone: the touch buttons answer where they are drawn from the first drive ([#2491](https://github.com/openOMSI-org/openOMSI/pull/2491), by @thibautgnt).
- Dependencies: the crates' minor updates, pollster 1.0 and sha2 0.11 ([#2544](https://github.com/openOMSI-org/openOMSI/pull/2544), [#2545](https://github.com/openOMSI-org/openOMSI/pull/2545), [#2546](https://github.com/openOMSI-org/openOMSI/pull/2546)); Dependabot now comes once a month with the major versions grouped.

## 0.2.46 - 2026-10-13

### New
- Setting "The mouse works the pedals" - off, mouse steering only steers and the pedals stay on the keys ([#1704](https://github.com/openOMSI-org/openOMSI/issues/1704)).
- Setting "Pedal keys speed" for a finer throttle and brake on the keyboard ([#1371](https://github.com/openOMSI-org/openOMSI/issues/1371)).
- Keys for the radio - next / previous station, louder / quieter - for the keyboard, a wheel or a pad ([#1293](https://github.com/openOMSI-org/openOMSI/issues/1293), [#1462](https://github.com/openOMSI-org/openOMSI/issues/1462)).
- The mouse's side buttons (Mouse 4, Mouse 5) can be bound like keys ([#2281](https://github.com/openOMSI-org/openOMSI/issues/2281)).
- Key "Free camera keys on/off": the free view's W, A, S, D... can work the bus instead ([#1779](https://github.com/openOMSI-org/openOMSI/issues/1779)).
- What passengers say is shown as OMSI's hints (too cold, too little change, 'Pardon?') ([#1806](https://github.com/openOMSI-org/openOMSI/issues/1806)).
- Setting "People at once" (Settings, Gameplay; OMSI's AI humans limit, 50-1000) in the launcher and the game menu ([#2087](https://github.com/openOMSI-org/openOMSI/issues/2087)).
- Launcher: a calendar for the start date, and the mouse wheel turns the start time and date ([#2351](https://github.com/openOMSI-org/openOMSI/issues/2351), [#1625](https://github.com/openOMSI-org/openOMSI/issues/1625)).
- Launcher: it says that it rests while a game runs, and draws nothing meanwhile ([#2362](https://github.com/openOMSI-org/openOMSI/issues/2362)).
- Information bar: the road's speed limit and a stop request ([#2476](https://github.com/openOMSI-org/openOMSI/issues/2476), [#1767](https://github.com/openOMSI-org/openOMSI/issues/1767)).
- Launcher: a tour that does not run on the chosen date can be driven on its next day with one click ([#1626](https://github.com/openOMSI-org/openOMSI/issues/1626)).
- Quick menu: the destination form offers the depot file's destinations ([#2219](https://github.com/openOMSI-org/openOMSI/issues/2219)).
- Touch controls: an announcement button (Q) ([#2107](https://github.com/openOMSI-org/openOMSI/issues/2107)).
- Screenshots leave the interface out, as in OMSI ([#1265](https://github.com/openOMSI-org/openOMSI/issues/1265)).
- The loading screen shows the map's picture ([#1383](https://github.com/openOMSI-org/openOMSI/issues/1383)).
- Updates and downloads use the system's proxy ([#1000](https://github.com/openOMSI-org/openOMSI/issues/1000)).
- Game menu: choose which placed vehicle to drive ([#2026](https://github.com/openOMSI-org/openOMSI/issues/2026)).
- Launcher: the map's next departures over all lines ([#1885](https://github.com/openOMSI-org/openOMSI/issues/1885)).
- Launcher: a tour offers the buses its timetable gives it ([#1886](https://github.com/openOMSI-org/openOMSI/issues/1886)).
- Launcher: notes of your own on maps and buses ([#1791](https://github.com/openOMSI-org/openOMSI/issues/1791)).
- Launcher: the R and N keys say which gearbox they are for ([#1691](https://github.com/openOMSI-org/openOMSI/issues/1691)).

### Fixes
- **AI traffic no longer stands at green lights** near a vehicle whose script claims priority (the player's bus on some maps, ambulances and fire engines with the siren on): a junction is kept for an emergency vehicle only while it comes, and only against the cars whose way meets its; a left turn gives way to the oncoming traffic even when it got there first ([#2431](https://github.com/openOMSI-org/openOMSI/issues/2431), [#2307](https://github.com/openOMSI-org/openOMSI/issues/2307), [#2373](https://github.com/openOMSI-org/openOMSI/issues/2373)).
- **Cars no longer vanish in the middle of the road** where a junction's paths have no traffic density of their own ([#2466](https://github.com/openOMSI-org/openOMSI/issues/2466)).
- **AI traffic gives way as in OMSI 2**: where a map sets no priorities, whoever gets to the junction first goes first - no more main-road cars stopping for every car out of a side road on their right ([#2058](https://github.com/openOMSI-org/openOMSI/issues/2058), [#1452](https://github.com/openOMSI-org/openOMSI/issues/1452)).
- **AI cars no longer pass over a solid centre line** (`[rule] overtaking_prohib`, as OMSI reads it): the queue behind a bus at its stop waits ([#2048](https://github.com/openOMSI-org/openOMSI/issues/2048), [#1931](https://github.com/openOMSI-org/openOMSI/issues/1931)).
- **No road spray from aircraft and trains** of the AI traffic over a wet road ([#1330](https://github.com/openOMSI-org/openOMSI/issues/1330)).
- Free drive: a line and tour can be picked again from the pause menu with the timetable buses switched off ([#2016](https://github.com/openOMSI-org/openOMSI/issues/2016)).
- Night tours run on past midnight: their timetable buses come after 24:00, a duty picked after midnight is the night's tour under way, and the launcher keeps the trip picked after midnight ([#2166](https://github.com/openOMSI-org/openOMSI/issues/2166)).
- Automatic IBIS: a trip without a line number that the depot file runs as line 000 is typed itself, not the tour's next numbered trip, and its passengers board ([#2425](https://github.com/openOMSI-org/openOMSI/issues/2425)).
- On foot during a duty, the navigator stays with the bus: walking round it no longer recalculates the route or moves the stops on ([#2426](https://github.com/openOMSI-org/openOMSI/issues/2426)).
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
- Another player's articulated bus: its rear section keeps the lean their game gives it, and its wheels stand still when the bus does ([#1177](https://github.com/openOMSI-org/openOMSI/issues/1177)).
- The game opens on the screen the launcher stands on again ([#1959](https://github.com/openOMSI-org/openOMSI/issues/1959), [#2025](https://github.com/openOMSI-org/openOMSI/issues/2025)).
- Place a vehicle: "Random livery" really picks a random livery ([#2410](https://github.com/openOMSI-org/openOMSI/issues/2410)).
- Launcher: the bus picker shows the whole name of a bus ([#2355](https://github.com/openOMSI-org/openOMSI/issues/2355)).
- Android: Change the keys and Set up a wheel open the Controls page ([#2308](https://github.com/openOMSI-org/openOMSI/issues/2308)).
- Linux: when the launcher cannot start (no WebKitGTK 4.1, e.g. SteamOS) the game says why in a dialog, with the steps for SteamOS ([#2458](https://github.com/openOMSI-org/openOMSI/issues/2458)).
- Game menu: moving the seat, head pitch or field of view shows the view behind the menu ([#2359](https://github.com/openOMSI-org/openOMSI/issues/2359)).
- Textures: "Compress textures on loading" no longer changes the colour of small colour swatches in texture atlases (handrails, buttons) ([#2292](https://github.com/openOMSI-org/openOMSI/issues/2292)).
- Transparency: a fully transparent [matl_alpha] 2 mesh hides what its model draws behind it again, as in OMSI (masks such as scroll boxes), in Enhanced as well ([#2059](https://github.com/openOMSI-org/openOMSI/issues/2059)).
- Enhanced: bus and car paint that mirrors only a little is satin again, no longer shining like glass against the light ([#2474](https://github.com/openOMSI-org/openOMSI/issues/2474)).
- Enhanced: the headlamps' beam tilts with the bus on ramps and hills instead of staying level ([#1897](https://github.com/openOMSI-org/openOMSI/issues/1897)).
- Enhanced: displays and panels with a light map are as bright by day as in Vanilla+ ([#2051](https://github.com/openOMSI-org/openOMSI/issues/2051)).
- Vanilla+: nights, dusk and shade are as dark as in OMSI 2; the light is laid on the texture the way OMSI does ([#300](https://github.com/openOMSI-org/openOMSI/issues/300), [#1612](https://github.com/openOMSI-org/openOMSI/issues/1612)).
- Ctrl+Shift+U (and other chords with Ctrl or Alt) no longer also runs the game's own Shift+U, Shift+R, Shift+M, Shift+N or Shift+number; those and Ctrl+Shift+D/E/G, Ctrl+M step aside where keyboard.cfg binds the chord ([#2478](https://github.com/openOMSI-org/openOMSI/issues/2478)).
- Num+ and Num- no longer zoom the outside camera when they are bound (OMSI's file has the kickdown and a door on them); new keys "Zoom in" / "Zoom out" (view_zoom_in, view_zoom_out) ([#2469](https://github.com/openOMSI-org/openOMSI/issues/2469), [#2322](https://github.com/openOMSI-org/openOMSI/issues/2322)).
- Shift, Ctrl and Alt can be bound on their own, also OMSI's "Shift/Ctrl/Alt (both)" entries ([#2003](https://github.com/openOMSI-org/openOMSI/issues/2003)).
- A gamepad lying idle no longer pulls the wheel back while the keys steer (the wheel went to and fro) ([#1895](https://github.com/openOMSI-org/openOMSI/issues/1895)).
- Gamepad steering turns the wheel slower at speed, not only less far ([#2014](https://github.com/openOMSI-org/openOMSI/issues/2014)).
- Switching cab cameras, the zoom glides to the new camera's along with the view instead of jumping ([#2306](https://github.com/openOMSI-org/openOMSI/issues/2306)).
- With a wheel the arrow keys switch the cab cameras by default, as in OMSI (the glance stays as a setting) ([#2164](https://github.com/openOMSI-org/openOMSI/issues/2164)).
- Head tracking turns the view smoothly at every frame, also with a 30 Hz webcam tracker ([#2216](https://github.com/openOMSI-org/openOMSI/issues/2216), [#1983](https://github.com/openOMSI-org/openOMSI/issues/1983)).
- A turn of the mouse wheel zooming the cab view no longer stops when a switch comes under the cursor ([#1224](https://github.com/openOMSI-org/openOMSI/issues/1224)).
- AI trains: every car of a train is read and uploaded ahead, so trains (X10 S-Bahn and others) no longer cause half-second stutters when they appear or turn at a terminus ([#2365](https://github.com/openOMSI-org/openOMSI/issues/2365), [#2464](https://github.com/openOMSI-org/openOMSI/issues/2464)).
- Graphics: when the graphics card runs out of memory, a texture that cannot be made is drawn plain and the game keeps fewer textures, instead of a black picture for the rest of the session ([#2472](https://github.com/openOMSI-org/openOMSI/issues/2472)).
- Passenger avatars: their pictures are read in the background before anybody is drawn with them, so walking or driving past new people no longer stutters ([#2463](https://github.com/openOMSI-org/openOMSI/issues/2463)).
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

### New
- **An `[htmltexture]` page that says `lit` is a surface on a bus too**, not only on a scenery object: it takes the sun, the street lamps and the fog instead of glowing at its own brightness. With a dot mask and a white light map such a page is an LED matrix whose dots burn and glow in Enhanced like the Krueger and K++ panels. Pages without the word look exactly as before ([#2460](https://github.com/openOMSI-org/openOMSI/pull/2460), by @NACHN).
- **Traditional Chinese** for the rest of the launcher and the pause menu: Drive, Settings, Controls, Profile, the head tracking and ambience settings, the controller descriptions and the status messages ([#2459](https://github.com/openOMSI-org/openOMSI/pull/2459), by @shamough1792). The launcher's "Light" theme is now named as a theme in every language, no longer as the light of the photo mode.

### Fixes
- **The driver no longer flickers in front of the cab view while the bus drives fast** (from about third gear, sooner at low frame rates such as on phones). The figure is still seen in the mirrors, and in the photo mode once its camera leaves the driver's eyes ([#2449](https://github.com/openOMSI-org/openOMSI/issues/2449), [#2461](https://github.com/openOMSI-org/openOMSI/pull/2461), by @thibautgnt).
- **AI buses sound like they pull away and brake.** Their scripts now get the throttle and brake pedals, so engines rev up under load and brake sounds play, instead of idling all the way (Hamburg's HHA buses and others) ([#2456](https://github.com/openOMSI-org/openOMSI/pull/2456), by @SpicySpaceman).
- **On foot, getting up under a building** (an indoor terminus, a canopy, a covered platform) leaves you on the floor under the roof instead of on top of the building ([#2201](https://github.com/openOMSI-org/openOMSI/pull/2201), by @KYChung1287).

## 0.2.44 - 2026-10-13

### New
- **Character models: High quality (improved)** (Settings, Gameplay, Passengers) draws the people with the 91 everyday [Microsoft Rocketbox avatars](https://github.com/microsoft/Microsoft-Rocketbox) (MIT) in place of the content's own, on your computer only: each person an avatar of the same kind, posed by OMSI's animation or the procedural one, with a living face from the avatars' ARKit blend shapes - blinks, glances, a mood of their own, passing smiles, raised brows, frowns and yawns, a smile at the cash desk, and lips that move with what they say. Servers and other players keep their own people. The avatars are downloaded once, like the ambience (about 310 MB). **Original** stays the default.
- **glTF / GLB, FBX and OBJ models** wherever OMSI reads a mesh (sceneries, splines' objects, vehicles, humans), with their materials, textures beside the model or packed into it, skins and blend shapes; the format is told by content.
- **Every picture format**: DDS with BC4-BC7, the DX10 header's 10-bit, float and packed formats, ATI1/ATI2 and DX10 cube maps; KTX 1 / KTX 2 (BC, ETC, ASTC); WebP, GIF, TIFF, QOI, OpenEXR, Radiance HDR, ICO, PNM and farbfeld; 16-bit PNGs. A texture is read by its content, whatever its name says, and found under all these extensions.
- **Every sound format**: WAV with extensible floats, 64-bit float, A-law / mu-law, 20- and 12-bit samples, RF64, ADPCM and the truncated headers old tools wrote; AIFF, CAF, M4A (AAC, ALAC), Matroska audio and MP2 besides Ogg, FLAC and MP3, all told by content. Surround is mixed down to stereo, and a sound missing under its name (`horn.wav`) is taken in another format of the same name (`horn.ogg`).

### Fixes
- **Higher frame rates below 120 frames a second, mostly on big maps with heavy AI lists.** Since 0.2.39 the whole simulation ran 120 times a second however fast the picture was. Each frame below 120 fps ran it two to four times (at 30 fps four times), up to 13 times in a slow frame, and every run brought the AI cars' meshes, lamps, drivers and sounds and every person's pose up to date again. A slower frame held more ticks and got slower still. Now:
  - the simulation steps 60 times a second;
  - a frame runs at most four steps, so the game keeps real time down to 15 fps;
  - what only the picture and the sound need of the traffic and the people is brought up to date once a frame, in its last step.

  At 30 fps the simulation's share of the frame is about half what it was ([#2246](https://github.com/openOMSI-org/openOMSI/issues/2246), [#2388](https://github.com/openOMSI-org/openOMSI/issues/2388), [#2295](https://github.com/openOMSI-org/openOMSI/issues/2295), [#2365](https://github.com/openOMSI-org/openOMSI/issues/2365)).
- **The frame rate no longer collapses after half an hour on 8 GB graphics cards.** The texture budget counted only the scenery's and the vehicles' textures. All the others (the people's, the scripts' and texts', the sky's and the terrain's) grew past it until the card ran out of its own memory: from 140 to 25 fps on a long drive. Textures beyond half of the card's memory are now taken from the scenery and the vehicles ([#2295](https://github.com/openOMSI-org/openOMSI/issues/2295), [#2121](https://github.com/openOMSI-org/openOMSI/issues/2121), [#1314](https://github.com/openOMSI-org/openOMSI/issues/1314)).
- **The automatic render scale no longer swings between 85 and 100 % every ten seconds** on a card at its limit. Each swing made new targets and was a stutter of its own. A step up that does not hold now waits twice as long before the next try, up to two minutes ([#2295](https://github.com/openOMSI-org/openOMSI/issues/2295), [#2121](https://github.com/openOMSI-org/openOMSI/issues/2121)).
- **Night in Enhanced is lighter on the graphics card.** Only the 12 AI cars nearest the camera, within 200 m, light the road with their headlamps; every car on the street did, a hundred lights and more on a busy road ([#1774](https://github.com/openOMSI-org/openOMSI/issues/1774), [#1467](https://github.com/openOMSI-org/openOMSI/issues/1467)).
- **Macs with Apple M4 draw the full picture again.** 0.2.43's far-map shadow for Enhanced+ made the scene shader too big for Apple's shader compiler. It crashed on every scene pipeline, and the game fell back to the basic picture or closed. The same shadow now takes one call instead of two.

## 0.2.43 - 2026-10-13

### New
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

## 0.2.42 - 2026-10-12

### New
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

### New
- **The simulation runs at a fixed 120 steps a second** on every computer and at every frame rate: the bus, the traffic, the passengers and the map's scripts move by the same steps at 30 and at 144 frames a second, and the bus and the camera are drawn between two steps, so the drive is the same however fast the picture is. The old step of one frame's length is gone (and with it `OMSI_FIXED_STEP`). A dedicated server steps 120 times a second too.
- **The desktop launcher is the only launcher**: the game's own launcher window is gone. A plain start opens the desktop launcher; where it is missing or cannot open its window (no WebView2 on Windows, which is offered to be installed, no WebKitGTK on Linux) the game says why and where it should be. On iPhone and iPad the "Cross-Platform Launcher" option is gone with it.
- **The launcher shows its Discord status again** ("In launcher"), while the setting is on, and leaves it to the game while a game starts or runs.
- **The pause menu shows the game behind it**, dimmed, on its first page; Settings and the other pages keep a solid background. The row over the game (Paused, the sign, the buttons, the bus and the driver) is gone; the sidebar stays.
- **Reflections on Enhanced+ have textures**: the windscreen, the rear window and other glass reflect the buildings, the trees and the buses with their real textures, not in plain colours.
- **Choosing the line and the tour is a pair of lists with a search again**, a tour found by any stop of its trips ([#2414](https://github.com/openOMSI-org/openOMSI/pull/2414), by @thibautgnt).
- **The Mods page gets back** cancelling an install, the archive's size check, filters, the content folder and dropping a mod anywhere ([#2412](https://github.com/openOMSI-org/openOMSI/pull/2412), by @thibautgnt).
- **Swedish (Svenska)** is one of the interface's languages ([#2411](https://github.com/openOMSI-org/openOMSI/pull/2411), by @Equinity).
- **A joined server sets the map, the clock, the weather and the buses** on the Drive page again ([#2408](https://github.com/openOMSI-org/openOMSI/pull/2408), by @thibautgnt).
- **Running games shows the session code, the players and the chat** of a multiplayer game again ([#2407](https://github.com/openOMSI-org/openOMSI/pull/2407), by @thibautgnt).
- **Settings, Updates, Choose a version** lists the releases and installs the one picked; an older version stays until the latest is chosen again ([#2393](https://github.com/openOMSI-org/openOMSI/pull/2393), by @thibautgnt).
- **A door release action for any bus** (`doors_release`), beside the buses' own ([#2390](https://github.com/openOMSI-org/openOMSI/pull/2390), by @ThiBot77).
- **Add binding lists every action again** (the installed buses' triggers, OMSI's names) and a controller's buttons can be given any of them ([#2387](https://github.com/openOMSI-org/openOMSI/pull/2387), by @thibautgnt).
- **HTML textures can be lit surfaces**: `lit` after a page's path ([#2423](https://github.com/openOMSI-org/openOMSI/pull/2423), by @NACHN).

### Performance
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

## 0.2.38 - 2026-10-12

### New
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

### Performance
- **The roundabout search is 2-5 times faster** at loading a map, finding the same roundabouts ([#2331](https://github.com/openOMSI-org/openOMSI/pull/2331), by @ThiBot77).

### Fixes
- **A line shows its two ends**: the launcher names a line by the first and last stop of its longest trip ("137 - Stadtgrenze - Falkensee Bhf"); it listed the termini of every short working and depot run one after another, which read as all its stops.
- **Mod buses have their 3D preview again** (the EN92, the GN92 and other big buses): a bus whose preview was still loading when the launcher closed or a game started was taken for one that hung the launcher and left out for good. The launcher now lets the preview helper end in order and clears the mark itself; previews left out by an older version are tried again after every update ([#2352](https://github.com/openOMSI-org/openOMSI/pull/2352), by @SpicySpaceman); a bus whose preview fails no longer shows the bus chosen before ([#2349](https://github.com/openOMSI-org/openOMSI/pull/2349), by @SpicySpaceman).
- **The bus preview no longer takes graphics memory from a game** started from the launcher on small graphics cards ([#2343](https://github.com/openOMSI-org/openOMSI/pull/2343), by @thibautgnt).
- **The version is right on macOS and Linux too**: the launcher's About window and bundle, multiplayer games, the dedicated server's listing, plugins and the radio's requests said 0.1.0 or 0.2.33.
- **The launcher's sliders**: the knob sits on the end of the coloured track; it stood beside it, by up to half its width.
- **The Drive page follows the computer's clock** when the real time or date is on ([#2312](https://github.com/openOMSI-org/openOMSI/pull/2312), by @ThiBot77).
- **The Time and Weather windows opened from the Alt quick menu** take the keys and the mouse again ([#2354](https://github.com/openOMSI-org/openOMSI/pull/2354), by @thibautgnt).
- **A livery's tinted windows on an alpha-tested layer are blended**, not drawn black (GPM C2) ([#2339](https://github.com/openOMSI-org/openOMSI/pull/2339), by @ThiBot77).

## 0.2.37 - 2026-10-12

### New
- **WebView2 installed from the launcher**: on a Windows without Microsoft WebView2 the new launcher cannot open its window; the game's own launcher then offers to install it with Microsoft's own installer, and opens the new launcher once it is there.
- **The clock moves by a tap**: World options has -1 h, -10 min, +10 min and +1 h buttons beside the hour and minute, so the time can be moved on a phone too (typing the time needs Enter, holding it needs Ctrl+Shift+Page Up).

### Fixes
- **The new launcher shows the right version** on every system; it said 0.1.0.
- **A tour can be picked and started on a phone**: in the pause menu's Line and tour the timetable with its Start button is under the list on a narrow screen; it was left out there, so a tour could only be marked.

## 0.2.36 - 2026-10-11

(0.2.35 was not released: its changes come with this version.)

### New
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

### Performance
- **Steadier frames**: the scenery scripts, traffic step and people run on the frame's thread with workers joining in instead of waiting for them (4.5 ms to 0.24 ms a frame for the scripts standing still on Spandau, and no more 20-70 ms frames from them); the population, timetable and people fill the map in different frames; far traffic light lamps update in turns.
- **Less work per frame**: street lamps' shadow tiles kept while nothing in them changes, mirrors keep their draw bundles, smoke written only when it changed, sky and shadow passes spread evenly, Enhanced passes that cannot show are left out, tiles streamed in batches and a season change read a few tiles a frame.
- **Frame limiter**: no sleep on top of V-sync and a real limit; mimalloc as the desktop allocator; the autosave written on a thread of its own.

### Fixes
- **Pedestrians no longer walk round and round a roundabout's pavement** or pile up there: they can turn into a path that meets theirs midway ([#2294](https://github.com/openOMSI-org/openOMSI/pull/2294), by @thibautgnt).
- **Cars behind the bus at a red light wait with it**; after 10 s they went round it on the oncoming lane ([#2293](https://github.com/openOMSI-org/openOMSI/pull/2293), by @thibautgnt).
- **Vanilla lighting no longer washes out** where lights overlap, **large textures no longer whiten** from their transparent pixels when they are scaled or compressed, and **invisible transparent faces no longer hide** what is behind them ([#2272](https://github.com/openOMSI-org/openOMSI/pull/2272), by @isaacsa2).
- **Enhanced clouds**: a deck's underside shows its heaps instead of one smooth grey, and the clouds no longer jump every few minutes.
- **Bus physics** take as many substeps as the frame's time needs instead of at least four a frame.

## 0.2.34 - 2026-10-11

### New
- **The trip summary rates the trip**: five stars, a line on how the passengers felt, the trip at a glance (stops served, stops on time, distance, time driven, passengers) and gauges for driving, comfort and ticket selling above the table of stops, all in the game's language. It opens 5 seconds after the bus stops, so there is time to open the doors first ([#2263](https://github.com/openOMSI-org/openOMSI/pull/2263), by @thibautgnt).
- **The launcher's start button says which multiplayer game it joins**: "Join Spandau 1" or "Join Thibaut's game" after joining a server or pasting a session code, instead of "Drive" ([#2262](https://github.com/openOMSI-org/openOMSI/pull/2262), by @thibautgnt).

### Fixes
- **Bus signals no longer freeze at the start of their cycle**: a traffic-light jump that lands where it started (Grand Paris-Moulon's TCSP17) is done once, not again every frame, so the bus signal reaches its next green ([#2265](https://github.com/openOMSI-org/openOMSI/pull/2265), by @YanisK33; [#2224](https://github.com/openOMSI-org/openOMSI/issues/2224), [#2266](https://github.com/openOMSI-org/openOMSI/issues/2266)).
- **A scripted bus signal never shows red and green together**: when its script lights both, it stays red ([#2264](https://github.com/openOMSI-org/openOMSI/pull/2264), by @YanisK33).
- **"Save the situation" can be continued**: the quicksave (Ctrl+S) is offered in the launcher's Continue on the map it was made on, beside the last situation and the slots; before, only a slot seemed to save ([#1746](https://github.com/openOMSI-org/openOMSI/issues/1746)).
- **Winter textures without snow as in OMSI**: a map's winter of kind 4 ("winter with snow") shows its winter pictures like kind 3; the snowy ones come only with snow in the weather - they showed under a clear sky ([#2131](https://github.com/openOMSI-org/openOMSI/issues/2131)).
- **Text textures with '@' line breaks are laid out as in OMSI**: the lines a font height apart and centred as a block, cut at both edges when it is taller than the texture; a text beginning with '@' (the New Lion's City's settings list) stood a line too low ([#1387](https://github.com/openOMSI-org/openOMSI/issues/1387)).
- **The map's depot file stays assigned** to a bus that lacks it when it carries the line chosen; a depot of the bus's own folder with a line of the same number was taken instead ([#1837](https://github.com/openOMSI-org/openOMSI/issues/1837)).
- **On foot without a bus, the navigator and the city map (Shift+M) work**, following the walker ([#2118](https://github.com/openOMSI-org/openOMSI/issues/2118)).

## 0.2.33 - 2026-10-11

### Changes
- **Timetable buses vary from session to session**: a tour's bus is drawn from its depot with the session's seed, so a tour gets another of the depot's buses each time, as in OMSI (alike on every machine of a LAN game); the same bus ran each tour every time ([#2250](https://github.com/openOMSI-org/openOMSI/issues/2250)).
- **Each cab camera keeps its own zoom**, as in OMSI, instead of one zoom for every camera of the view ([#2183](https://github.com/openOMSI-org/openOMSI/issues/2183)).
- **Sun blinds work as in OMSI**: one pushed up by hand drops back to where it was pulled down to once it is let go; only its cord rolls it up. Before, every drag moved the blind's catch with it and it stayed wherever it was dragged ([#2146](https://github.com/openOMSI-org/openOMSI/issues/2146)).

### Fixes
- **Single railcars no longer crash the game**: a railcar with nothing coupled behind it (Thüringer Wald's AI trains) is put on its track without a panic ([#2259](https://github.com/openOMSI-org/openOMSI/issues/2259), [#2229](https://github.com/openOMSI-org/openOMSI/issues/2229)).
- **The clutch, the throttle and the brake on a controller button work**: a button or trigger set to a pedal works it as the pedal's key does; the clutch on a wheel button did nothing ([#2029](https://github.com/openOMSI-org/openOMSI/issues/2029)).
- **No more hitch every 5 seconds without a controller (Windows)**: with none plugged in, DirectInput is looked for again only in the first 30 seconds and then when Windows reports a new device, instead of every 5 seconds for the whole session ([#2213](https://github.com/openOMSI-org/openOMSI/issues/2213), [#2068](https://github.com/openOMSI-org/openOMSI/issues/2068), [#2159](https://github.com/openOMSI-org/openOMSI/issues/2159)).
- **Parked buses stay where they were left**: a placed bus keeps its place while its tile is not loaded, and one saved under the ground is put back on it; they fell through the world when the player drove off and were saved kilometres down ([#2132](https://github.com/openOMSI-org/openOMSI/issues/2132)).
- **Photo mode gives the time back**: a clock moved for a photo returns when photo mode is left, so the duty goes on at the time it stood at ([#2116](https://github.com/openOMSI-org/openOMSI/issues/2116)).
- **The free camera keeps the horizon level** when taken over from the driver's view of a bus leaning on a slope; it went on with the bus's roll ([#2220](https://github.com/openOMSI-org/openOMSI/issues/2220)).
- **The automated manual shifts past first gear** on buses whose script keeps the gear in a variable of its own (the gear up/down keys too): such a bus was seen in neutral after every shift and put into first again and again. The gate put in last is now taken as the gear when the script's variable does not follow it ([#1832](https://github.com/openOMSI-org/openOMSI/issues/1832); reports on the O305 Schaltwagen and the Citybus i280 are welcome).
- **Articulated buses' rear sections sit higher**: a rear section on one axle hangs the share of its weight ahead of that axle on the joint, which presses the front section down there, as OMSI's joined bodies do; its axle carried all of it and sat that much deeper on its springs (0.6 of the GN92 rear section's 7.2 t). Right after the start the GN92's rear section still stands lower until its level control has filled its bellows, as the stock script has it ([#1835](https://github.com/openOMSI-org/openOMSI/issues/1835)).
- **The NL/NG wheelchair lift and ramp work at petrol stations**: the scripts' ground sensor (`GetHeightAbovePoint`) measures down to the ground under the point, not to the highest face there - under a canopy that was its roof, so the lift would not go down and the ramp turned on its side and vanished ([#2205](https://github.com/openOMSI-org/openOMSI/issues/2205)).

## 0.2.32 - 2026-10-11

### Fixes
- 0.2.31 was never published: its Windows x64 build stopped at a check of the shaders that read Windows' line endings (not the game itself). 0.2.32 is 0.2.31 for every platform - see below for what it brings.

## 0.2.31 - 2026-10-11

### New
- **openOMSI heals a broken start by itself.** A release that crashes or hangs before the launcher's window used to strand everyone - the updater lives in the launcher, so a launcher that never opened never updated (0.2.28 on Windows had to be downloaded again by hand). Now the program's very first act is a small watcher without a window; if the launcher crashes, ends on an error or has not drawn its first frames after two minutes, it is healed on its own: a newer release is installed (its program must start before it replaces anything), else the version before the last update - kept until the new one has started well - is put back, else it starts once more in safe mode (basic graphics, no plugins), else a message says what to do. Each step is said in a message box in the game's language. A version that failed is never installed again on that computer, and the project can list a broken release (`release/health.json`) so that it is replaced at the next start even when it starts. A crash in the middle of a drive changes nothing. `OMSI_NO_GUARD=1` switches it off (see the user guide, "When a version does not start").
- **Mirror panels every frame**: a switch in Settings → Graphics (off by default) draws the mirror panels in the cab (Ctrl+M) every frame, as ETS2 draws its screen mirrors, so the street no longer jerks past in them ([#2209](https://github.com/openOMSI-org/openOMSI/pull/2209), by @m3rlinek01).

### Changes
- **AI traffic drives more smoothly**: brake lights with hysteresis and a minimum time lit (no flicker), gentle braking eased in, bends taken at 1.2 m/s² and slowed for from further back, AI buses steer into turns with a shorter look-ahead and a quicker wheel, and the AI horn is heard (OMSI's own `Sounds/AI_Cars` horn where a car's sounds have none on `ev_AI_Horn`).
- **Emergency vehicles on a drive are faster**: 1.5 times the limit, at most 30 km/h over it, pulling away harder and taking bends harder.
- **The trip summary waits until the bus stops**, so it no longer takes the mouse and the pedals while the bus pulls into the last stop ([#2240](https://github.com/openOMSI-org/openOMSI/pull/2240), by @Dasteee; #2227, #2236), and **fits a phone's screen** instead of spilling past its edges.
- **Spline rows as Omsi.exe draws them**: the objects of a row that reach past their tile's edge are drawn again (the far side of the parked cars along a Berlin-Spandau street crossing a tile border; [#2249](https://github.com/openOMSI-org/openOMSI/pull/2249)), and a row whose first object lies before its own spline is not drawn, whatever its lateral offset ([#2238](https://github.com/openOMSI-org/openOMSI/pull/2238); both by @NACHN, checked against Omsi.exe).
- **Under the hood**: the program is split into 33 parts with clear layers - the launcher, input devices, VR, LAN play, platform services, weather, career, soundscape, the world, the scene, photo mode and the HTML displays each in their own crate, the renderer's 10,000-line core cut up by concern, the vehicle and the script host by subsystem, the frame as one declared list of stages for the window and the offscreen run. What is drawn, heard and simulated is unchanged (checked picture by picture, sound by sound and with every test); a check on every pull request keeps the layers apart.

### Fixes
- **Destination displays that only their model names a variable for show again** (the Enviro500's `DPIP_8words`): a variable a model's `[visible]` uses is registered even when no varlist declares it ([#2217](https://github.com/openOMSI-org/openOMSI/pull/2217), by @shamough1792).
- **A second drive in the same program** (Android starts one from the launcher, a lost graphics device restarts the renderer) no longer inherits the last one's state: corona and spotlight pictures were missing on the second renderer, a duty after one begun on the real time could not move the clock to its first trip, a map opened for a duty start could be taken over with its old state, the map lights and fading lamps of the previous map, the photo mode's settings, passengers' lines and a LAN `/admin` password carried over, and a LAN rejoin said hello twice.
- **With mirrors on, the window's draw lists are reused again** from frame to frame (a mirror pass threw them away every frame).
- **Offscreen recordings (`OMSI_RECORD`)** fade sounds in by the film's time, not the wall clock, so a recording comes out the same every time.

## 0.2.30 - 2026-10-11

### Changes
- **Clouds: an overcast sky is a dark deck with its own heaps in it** instead of a flat pale sheet. A few fair-weather clouds keep the light bases they had; as the cover rises past about 55 % their undersides go darker, towards the grey of a storm, and a shut sky or rain is drawn by the deck itself (Enhanced; [#2198](https://github.com/openOMSI-org/openOMSI/pull/2198), by @NACHN).
- **All platforms again**: 0.2.29 came out for Windows only; 0.2.30 is built for Windows, macOS and Linux (x64 and ARM64), Android (arm64) and the dedicated servers.

### Fixes
- **Spline rows no longer draw objects Omsi.exe drops**: a row's objects that fall outside their own tile, and a row written along the carriageway that starts before its chain - on Grundorf Island 4's interchange, bridge piers no longer stand in the road. Rows of `[absheight]` objects stay wherever they fall, as single objects do ([#2194](https://github.com/openOMSI-org/openOMSI/pull/2194), by @KYChung1287; #787).
- **Objects a map tags as a surface stay behind what stands beside them at a distance**: the pull towards the eye that keeps road markings over the road stops growing at 5 cm, so a traffic light's pole no longer draws across its signal head from tens of metres away; markings, crossings and kerbs keep their depth near the camera ([#2197](https://github.com/openOMSI-org/openOMSI/pull/2197), by @NACHN; #1694, #1196).

## 0.2.29 - 2026-10-10

### Fixes
- **Windows: the launcher opens again.** In 0.2.28 its window stayed hidden until its first picture, and Windows never asks a hidden window to draw, so the launcher never appeared. If you installed 0.2.28 on Windows, download 0.2.29 by hand: that launcher cannot update itself.

## 0.2.28 - 2026-10-10

### New
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

### Changes
- **Faster starts.** The game's window shows its loading screen at once and stays responsive while the shaders are built; the launcher draws its page from the first frame and loads the 3D preview after it.
- **Less work per frame**: the script engine, the AI cars' sounds and materials, the timetable paper, departure boards, scenery scripts, the light grid, ray tracing (Enhanced+, about half the CPU time), render bundles and texture lookups; the settings file is read once.
- **Smooth view changes as in OMSI**, and the steering follow only in the standard driver's view.
- **`Velocity` is the driven wheels' speed** (wheelspin and locked wheels show in it), `Velocity_Ground` the body's, as in OMSI.
- **Choosing a duty no longer types into the IBIS or the roller blind**, as in OMSI 2 (Shift+U and `--autostart` still do). `GetTTDelay` is whole seconds.
- **Clouds**: a deck's underside keeps the shape of the heaps it is made of ([#2171](https://github.com/openOMSI-org/openOMSI/pull/2171), by @NACHN).
- **Low sun**: rough roads no longer glare; Enhanced paint reads its reflection map as OMSI does, and windows with one show the lamp-lit street at night.
- **Integrated GPUs get the texture memory their driver reports** ([#2115](https://github.com/openOMSI-org/openOMSI/pull/2115), by @anonchan958).
- **Outside sounds are muffled in the closed cab**, and the cab and street sounds crossfade as you walk in or out.
- **Rail vehicles follow tracks and grades more precisely** ([#2122](https://github.com/openOMSI-org/openOMSI/pull/2122), by @Maarceeli).
- **Text fields** in the launcher: selection, word-wise moves, copy, cut and paste.

### Fixes
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
- **Tunnel entrances** are no longer walled off by stretched terrain ([#2130](https://github.com/openOMSI-org/openOMSI/pull/2130), by @juanjopuntcat; #1472).
- **A mod's lowercase `texture` folder no longer hides the original's textures** on Linux ([#2119](https://github.com/openOMSI-org/openOMSI/pull/2119), by @artemix1508).
- **Linux force feedback for wheels without a gilrs mapping** ([#2168](https://github.com/openOMSI-org/openOMSI/pull/2168), by @YanisK33).
- **Android: the pause menu scrolls by touch again** ([#2139](https://github.com/openOMSI-org/openOMSI/pull/2139), by @isaacsa2; #2108).
- **`[illumination_interior]` lists** ignore -1, out-of-range and repeated lamps.
- **Wheel spray** no longer shows inside the cab.
- **The "playing now" service and the PR build comments are hardened against bad input** ([#2151](https://github.com/openOMSI-org/openOMSI/pull/2151), by @isaacsa2).

## 0.2.27 - 2026-10-10

### New
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

### New
- **Photo mode** (pause menu → *Photo mode*, Ctrl+F12 or the `photo_mode` key). It replaces the menu's Screenshot.
  - The game stops and a camera of its own flies freely or orbits the bus (WASD, Q/E, mouse or right stick to look, wheel to zoom, Z/X to roll).
  - A real lens: focal length from 10 to 400 mm, depth of field from the aperture and the focus (on the bus or set by hand), round or bladed bokeh.
  - A rolling shot: the shutter blurs the street and turns the wheels while the bus stays sharp.
  - A darkroom (exposure, contrast, highlights, shadows, saturation, temperature, tint), film looks, grain, vignette, frames of any aspect ratio with composition guides, and the clock moved for another light.
  - Photos are saved as PNG into `Screenshots`, at the window's size or double. The note that says where it went opens the folder with the photo selected when clicked (Finder, Explorer, the file manager). On Android the photo goes straight into the gallery.
- **The pause menu is drawn like the launcher**: its rail with Resume first, and the settings and lists as launcher pages with the same fields, switches and sliders. The keyboard works it as before.
- **Procedural passenger animation as an option** ([#2101](https://github.com/openOMSI-org/openOMSI/pull/2101), by @isaacsa2). Settings → Gameplay → Passengers → *Passenger animations*: *Procedural (experimental)* plants the feet on the floor and the steps, seats people on their seats and lets standing riders hold the rails. *Original OMSI* stays the default.
- **Clouds cover the sky by a continuous amount** ([#1360](https://github.com/openOMSI-org/openOMSI/pull/1360), by @NACHN). A cloud cover slider in the launcher, the phone layout and the game's weather page, carried smoothly across weather changes.

### Changes
- **Looking round with the mouse is smooth.** While the right (or middle) button is held, the cursor stays where it is and shows OMSI's four arrows, and the view turns by the mouse's own movement. It no longer stops at the edge of the screen, jumps when the button comes up, or loses the steering cross with mouse steering on. The photo mode's camera turns the same way.
- **Plain text in the interface**: separators are `|`, dashes `-` and ellipses `...`. The photo mode's line of keys and the pause menu's key hint are gone.
- **Passengers take a free seat first** and stand only by chance (Settings → Gameplay → *Standing passengers*, 5 % by default) or once the seats are full ([#2063](https://github.com/openOMSI-org/openOMSI/pull/2063), by @SuSeu1414).
- **Logitech G923 force feedback** pushes the right way, and a gentle centring spring holds the wheel while parked ([#2065](https://github.com/openOMSI-org/openOMSI/pull/2065), by @atzock).
- **The destination menu shows the HOF's names**, as OMSI's own list does ([#2081](https://github.com/openOMSI-org/openOMSI/pull/2081), by @SpicySpaceman).
- **Less GPU work on slow machines**: the launcher no longer redraws an unchanged picture while idle ([#2097](https://github.com/openOMSI-org/openOMSI/pull/2097)), and the navigator is drawn every other frame at most where frames are slow ([#2096](https://github.com/openOMSI-org/openOMSI/pull/2096), both by @isaacsa2).

### Fixes
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

### New
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

### Changes
- **The ambience is removed.** The synthesised wind, leaves, raindrops, thunder, birds, crickets, town hum and tyre sounds added in 0.2.21 are gone, together with their settings. OMSI 2's own sounds play as they always did, and the bus scripts still learn what each wheel rolls on (`Axle_SurfaceID`).

### Fixes
- **Objects placed along a spline** stand where the map puts them, without gaps or overlaps where a row crosses into the next tile ([#2032](https://github.com/openOMSI-org/openOMSI/pull/2032), by @KYChung1287). This covers bridge piers, street lamps, catenary masts, fences and road markings.
- **Scenery shows its own textures** ([#2006](https://github.com/openOMSI-org/openOMSI/pull/2006), by @KYChung1287). A texture missing from an object's folders is no longer borrowed from another folder (a gantry wearing the pack's stock sign, a plate wearing asphalt). Fully opaque sign pictures no longer fade into the background.
- **Steam overlay** opens with Shift+Tab, because Steam now starts before the graphics and the window ([#2001](https://github.com/openOMSI-org/openOMSI/pull/2001), by @shamough1792).
- **The French menu title** is plain "openOMSI" ([#2024](https://github.com/openOMSI-org/openOMSI/pull/2024), by @Showtc).

## 0.2.23 - 2026-10-09

### Changes
- **The previous launcher is back.** The launcher rebuilt in 0.2.22 is withdrawn; openOMSI opens the launcher of 0.2.21 again, with every page, the phone layout and the settings as they were (the ambience's switch and volume stay under Settings → Sound).
- **The Mods page of 0.2.22 lives on in it.** It still lists every installed mod and every item found in the content folder, with its kind, size and install date. You can search, filter by buses, maps, archives, other and switched-off mods, switch each mod off and on, and delete it after a confirmation. The list follows installs as they finish.

### Fixes
- **The ambience no longer crackles.** Raindrops on the ground, drips from the trees after rain and drops into puddles were single sharp clicks that hit the layer's limiter, so the ambience sounded like eating crisps. Each drop is now a soft tick, and the limiter recovers in 80 ms instead of 0.4 s, so it no longer ducks the whole layer after each one.
- **Leaves rustle instead of crunching.** A tree's leaves knock against each other thousands of times a second, but there were far too few, too loud clicks. Autumn's dry leaves (from late September) made a crisp-packet crackle. The rustle is now dense and soft, and still grows with the wind and the trees around.
- **Ending the launcher while a bus's preview is being placed** no longer leaves that bus out of the previews from then on.

## 0.2.22 - 2026-10-09

### New
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
  - In open country, where nothing hides a car, the traffic no longer dies out.

  On Spandau with 120 cars, the cars standing for over a minute fell from 41-77 to 10-21.
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

### New
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

### New
- **Gamepads**: ready-made Xbox, PS4 and PS5 profiles with default buttons, a Gamepad settings page (stick dead zone, steering speed), and the Menu/Options button opens the main menu [#1939](https://github.com/openOMSI-Project/openOMSI/pull/1939); a controller action to open and close the main menu [#1594](https://github.com/openOMSI-Project/openOMSI/pull/1594).
- **Key bindings**: "Add binding" on the Controls page lists every action - the installed buses' own script triggers too, with how many buses use each - searchable [#1437](https://github.com/openOMSI-Project/openOMSI/pull/1437).
- **Platform screen doors**: light paths marked as a turn work as detectors for that indicator, for every vehicle, so BRT platform doors open for the bus that stops there [#1889](https://github.com/openOMSI-Project/openOMSI/pull/1889).
- **HTML LED destination signs** glow at night like the LED panels, with the same LED glow setting [#1905](https://github.com/openOMSI-Project/openOMSI/pull/1905).
- Lua plugins: a `service` event (refuel, wash, repair, reset, teleport) [#1882](https://github.com/openOMSI-Project/openOMSI/pull/1882), and `trip_done` with the trip's ratings, `jolt` and `ticket_sold` events [#1937](https://github.com/openOMSI-Project/openOMSI/pull/1937).
- `telemetry.json` for outside tools (the player's bus, the duty and its stops, the AI buses): written only while the file exists, see `docs/PLUGINS.md` [#1909](https://github.com/openOMSI-Project/openOMSI/pull/1909).
- Translations: Dutch completed [#1944](https://github.com/openOMSI-Project/openOMSI/pull/1944), German reviewed [#1936](https://github.com/openOMSI-Project/openOMSI/pull/1936), Chinese [#1587](https://github.com/openOMSI-Project/openOMSI/pull/1587), French [#1713](https://github.com/openOMSI-Project/openOMSI/pull/1713), the game menu in every language [#1540](https://github.com/openOMSI-Project/openOMSI/pull/1540), the stock weather names [#1598](https://github.com/openOMSI-Project/openOMSI/pull/1598); messages with a number or a name in them are translated too [#1724](https://github.com/openOMSI-Project/openOMSI/pull/1724).

### Fixes
- Dedicated server and LAN host: AI cars no longer appear or vanish in plain view of the other players [#1893](https://github.com/openOMSI-Project/openOMSI/pull/1893).
- Enhanced at night: no more black disc under street lamps whose light sits inside the lamp (the Ufo lamps of Spandau), and the lamp shadows got faster [#1915](https://github.com/openOMSI-Project/openOMSI/pull/1915).
- Faster in scenes with excavations (U-Bahn entrances): the MSAA depth prepass is kept, about 1.5 ms less per frame there [#1614](https://github.com/openOMSI-Project/openOMSI/pull/1614).
- A Shift, Ctrl or Alt whose release was missed (after a screenshot shortcut) no longer sticks [#1901](https://github.com/openOMSI-Project/openOMSI/pull/1901).
- Vehicle descriptions are shown in full, and the phone launcher's vehicle sheet scrolls [#1932](https://github.com/openOMSI-Project/openOMSI/pull/1932).
- The German tutorial pages show their special characters [#1552](https://github.com/openOMSI-Project/openOMSI/pull/1552).
- Documentation fixes [#1917](https://github.com/openOMSI-Project/openOMSI/pull/1917), [#1918](https://github.com/openOMSI-Project/openOMSI/pull/1918), [#1919](https://github.com/openOMSI-Project/openOMSI/pull/1919); a deprecated call replaced [#1947](https://github.com/openOMSI-Project/openOMSI/pull/1947).

## 0.2.19 - 2026-10-08

### Fixes
- Mouse steering shows its "+" again: since 0.2.17 the cursor is held while the mouse steers (so the wheel reaches its full lock past the screen's edge), and the crosshair went with it. The cross is now drawn at the point that steers and takes your clicks.

## 0.2.18 - 2026-10-08

### New
- **Quality presets for each graphics mode**, the same in the launcher and the in-game options; the mode is chosen first and a preset never switches it. Your current settings stay as they are [#1873](https://github.com/openOMSI-Project/openOMSI/pull/1873).
- **Cloud quality** (Settings → Graphics): "Low" draws the same physical clouds with fewer steps, about 0.8 ms faster on an M4; the cloud noise is kept on disk so later starts make it no more [#1872](https://github.com/openOMSI-Project/openOMSI/pull/1872).
- **"DXT/BC textures stay compressed on the GPU"** can be switched off for drivers that draw compressed textures wrongly (the textures then take about twice the video memory) [#1873](https://github.com/openOMSI-Project/openOMSI/pull/1873).
- **Searchable vehicle lists**, and placing, swapping or reloading a vehicle loads it in the background instead of freezing the game (1.4 s less on an articulated bus) [#1867](https://github.com/openOMSI-Project/openOMSI/pull/1867).
- **Support package** (Setup → Export diagnostics): a ZIP for a bug report with the versions, the graphics card and driver, the graphics settings, the controllers and the last map and bus - no folders, names, chat, LAN codes or addresses. It stays on your computer and its folder opens, so you can look inside before attaching it [#1870](https://github.com/openOMSI-Project/openOMSI/pull/1870).
- Mods: `[matl_glow] <texture> <value>` lets a script make a material glare (an openOMSI extension; stock content is unchanged) [#1853](https://github.com/openOMSI-Project/openOMSI/pull/1853).
- Performance captures: an `OMSI_PROFILE` run now logs the frame-time percentiles, `OMSI_PROFILE_JSON=<file>` saves them, and `scripts/compare-performance.py` compares two runs [#1869](https://github.com/openOMSI-Project/openOMSI/pull/1869).

### Fixes
- A duty started from the game menu starts at the current time, not at the first trip of the day [#1864](https://github.com/openOMSI-Project/openOMSI/pull/1864).
- Articulated buses: every exit follows its own door, passengers on foot find the nearest section's doors [#1863](https://github.com/openOMSI-Project/openOMSI/pull/1863).
- LAN: a weather change by the host reaches the other players at once [#1865](https://github.com/openOMSI-Project/openOMSI/pull/1865).
- Launcher: a key binding keeps the Shift, Ctrl or Alt it was pressed with [#1866](https://github.com/openOMSI-Project/openOMSI/pull/1866).
- HafenCity: the harbour backdrop no longer stands across the road at Landungsbrücken [#1861](https://github.com/openOMSI-Project/openOMSI/pull/1861).
- Enhanced at night: lamp shadows are no longer looked up for surfaces facing away from the lamp (same picture, a little faster) [#1875](https://github.com/openOMSI-Project/openOMSI/pull/1875).
- The launcher opens faster: its bus preview no longer prepares resources it doesn't use [#1871](https://github.com/openOMSI-Project/openOMSI/pull/1871).
- Build: no compiler warnings left, and two examples no longer share an output name [#1862](https://github.com/openOMSI-Project/openOMSI/pull/1862); more content tests [#1868](https://github.com/openOMSI-Project/openOMSI/pull/1868).

## 0.2.17 - 2026-10-08

A big one: early, mid and late seasons with trees that turn one by one, mouse steering to full
lock, the wipers really clearing the glass, collisions with real momentum and dents, encrypted
LAN mods, a DirectX 11 option for old graphics cards, and the game's code reorganised from the
ground up so that it stays easy to change.

### New
- **Early, mid and late seasons.** Pick a season and under it Early, Mid or Late. The phase
  sets a real date in that part of the season (the map's hemisphere decides which months), so
  the sun's path, the length of the day, the natural weather's temperatures and snow, the
  street lamps, the map's dated changes and the timetable all follow it by themselves. "By
  date" works as before. Also on the command line: `--season autumn-late`.
- **Trees that turn one by one.** In between two seasons the trees no longer change all
  together: each tree takes one of the two looks, the same every time and for every LAN
  player - early autumn is green with the first yellow trees, late autumn mostly bare with a
  few still in leaf, early spring bare with the first green, late summer partly dry.
- **Wipers clear the rain and snow only where the blade has passed**, along its sweep, pushing
  the water ahead of it; drops merge and run off, and in snow the flakes settle on the glass
  and build up at the edge of the wiped area [#1828](https://github.com/openOMSI-Project/openOMSI/pull/1828).
- **Collisions with AI vehicles**: a car that is hit recoils by momentum and finds its way back
  onto its lane, crash energy follows the masses and speeds of both vehicles, both the bus and
  the AI cars get dents where they were hit, and glass shows cracks, lit like the pane it is in
  [#1819](https://github.com/openOMSI-Project/openOMSI/pull/1819). Mouse pedal strength is a new
  setting [#1819](https://github.com/openOMSI-Project/openOMSI/pull/1819).
- **"ANGLE (DirectX 11)" graphics API on Windows**, for graphics chips whose DirectX 12, Vulkan
  and OpenGL drivers don't run the game (Intel HD Graphics 2000-4000, Radeon HD 5000/6000). It
  is tried by itself when the others fail, or chosen in Settings. The DLLs are built from
  Google's ANGLE source in [angle-openomsi](https://github.com/openOMSI-Project/angle-openomsi)
  and checked by their SHA-256.
- **LAN mods are encrypted** on the network and on disk: the files a host sends can't be read
  off the network, and copying them out of the game's folders gives you scrambled data, while
  the game reads them normally. (This stops casual copying, not a determined person: the game
  is open source and decrypts on the player's computer.) Hosts and players need 0.2.17 both.
- The information bar shows how many passengers fit next to how many are aboard
  [#1826](https://github.com/openOMSI-Project/openOMSI/pull/1826).
- Plugins: `omsi.info()` names the stops on either side of the bus with their IDs, the
  distance to each, and whether the bus stands at a stop
  [#1588](https://github.com/openOMSI-Project/openOMSI/pull/1588).
- New option (Settings → Gameplay → Traffic, off by default): timetable buses running ahead
  of time wait for their departure only at the stops the timetable times itself, instead of
  at every stop as in OMSI [#1773](https://github.com/openOMSI-Project/openOMSI/pull/1773).

### Fixes
- Mouse steering reaches full lock both ways: the wheel used to stop as soon as the cursor
  hit the edge of the screen (on macOS often at a quarter of the lock). The cursor is now
  held while you steer and comes back when you look around, open a menu or switch windows;
  the mouse pedals reach full throttle and full brake too.
- Dedicated server: people walk the pavements and wait at the stops around every player
  again, and board the timetable buses there
  [#1817](https://github.com/openOMSI-Project/openOMSI/pull/1817).
- The offscreen pictures (and so the game's own picture checks) are taken exactly as the
  window draws the game: the clock goes on, the AI's lights follow the daylight, the
  collision settings count, the wet roads and the cabin air develop, the lighting is built
  the same way.
- No more crashes when the graphics driver fails in some ways it used to take the game down
  with: running out of video memory while recording, a driver that can't compile a shader,
  adapter checks that fail, closing an OpenGL context. Shader compile failures now say what
  the driver said.
- OpenGL: no more "Could not lock adapter context" crashes while another part of the game
  waits for the GPU (#843, #898, #1110).
- The LAN tests no longer fail now and then.

### Under the hood
- The game's code was reorganised without changing what it does (checked frame by frame
  against the previous version): the frame is a sequence of named steps shared by the window
  and the offscreen run; the AI traffic, the people and the timetable simulate in `omsi-sim`
  with no graphics in them, and one place hands their state to the renderer; the renderer's
  passes and pipelines each have a module of their own; the game's state is grouped by
  subsystem; every `OMSI_*` switch is listed in `docs/DEBUG_FLAGS.md` and read through one
  registry. No function is longer than 500 lines any more (there were ten), and a check on
  every pull request keeps files and functions from growing past their size again.
- openOMSI now builds on [wgpu-openomsi](https://github.com/openOMSI-Project/wgpu-openomsi),
  the project's fork of wgpu 29.0.4 with the graphics fixes above (and a video-memory query,
  which the log now shows).
- `docs/ARCHITECTURE.md` describes the architecture; its history moved to `docs/HISTORY.md`.

## 0.2.16 - 2026-10-07

### New
- Lua plugins can show panels (texts, icons, progress bars, buttons) and sliding notifications of their own in the game's look (`omsi.ui`); F10 gives the mouse to the panels, Esc back to the bus. See `docs/PLUGINS.md` and the `trip_panel.lua` example [#1788](https://github.com/openOMSI-Project/openOMSI/pull/1788).
- Native TrackIR head tracking on Windows (NaturalPoint's NPClient), with sensitivity and inversion per axis in the launcher; opentrack keeps working as before [#1763](https://github.com/openOMSI-Project/openOMSI/pull/1763).
- Night brightness (Settings → Graphics): brightens the Enhanced picture after sunset only, Off by default [#1799](https://github.com/openOMSI-Project/openOMSI/pull/1799).
- Plugins: `trip_done` in `omsi.info()` [#1810](https://github.com/openOMSI-Project/openOMSI/pull/1810).

### Fixes
- Refuelling and the bus wash take their time, litre by litre, and stop when the bus drives off [#1785](https://github.com/openOMSI-Project/openOMSI/issues/1785).
- Pedestrians run over are counted once each, within the bus's real box [#1805](https://github.com/openOMSI-Project/openOMSI/issues/1805).
- Enhanced+: the buildings of a depot or other surface object cast their shadow up close too [#1783](https://github.com/openOMSI-Project/openOMSI/issues/1783).
- Smoke, exhaust and other particles drift with the wind [#1798](https://github.com/openOMSI-Project/openOMSI/issues/1798).
- The automated manual gearbox can be switched on in the launcher's driving settings [#1780](https://github.com/openOMSI-Project/openOMSI/issues/1780).
- A traffic light no longer reacts to a bus on the neighbouring road or bay: the depot-gate request applies only off the lanes [#1790](https://github.com/openOMSI-Project/openOMSI/issues/1790).
- A duty started at the real time keeps the clock instead of jumping to ten minutes before its first trip [#1792](https://github.com/openOMSI-Project/openOMSI/pull/1792).
- Puddles are smaller, with sharp irregular edges [#1804](https://github.com/openOMSI-Project/openOMSI/pull/1804).
- Dedicated server: AI traffic and pedestrians move smoothly on the players' screens and at their real speed; the server's clock keeps to real time [#1807](https://github.com/openOMSI-Project/openOMSI/pull/1807).
- Hong Kong maps: AI buses of a plain `[aigroup_2]` group show their destination again [#1757](https://github.com/openOMSI-Project/openOMSI/pull/1757), and bus stop signs show their route numbers and pictures [#1765](https://github.com/openOMSI-Project/openOMSI/pull/1765).
- AI vehicles wait before a junction when a queue on the short road pieces beyond it leaves no room, instead of blocking it [#1761](https://github.com/openOMSI-Project/openOMSI/pull/1761).

## 0.2.15 - 2026-10-07

### Bug Fixes & Improvements

## 0.2.14 - 2026-10-07

### Bug Fixes & Improvements

## 0.2.13 - 2026-10-07

### Bug Fixes & Improvements

## 0.2.12 - 2026-10-07

### Bug Fixes & Improvements

## 0.2.11 - 2026-10-07

### Bug Fixes & Improvements

## 0.2.10 - 2026-10-07

### Bug Fixes & Improvements

## 0.2.9 - 2026-10-06

### Bug Fixes & Improvements

## 0.2.8 - 2026-10-06

### Bug Fixes & Improvements

## 0.2.7 - 2026-10-06

### Bug Fixes & Improvements

## 0.2.6 - 2026-10-06

### Bug Fixes & Improvements

## 0.2.5 - 2026-10-06

### Bug Fixes & Improvements

## 0.2.4 - 2026-10-06

### Bug Fixes & Improvements

## 0.2.3 - 2026-10-06

### Bug Fixes & Improvements

## 0.2.2 - 2026-10-06

### Bug Fixes & Improvements

## 0.2.1 - 2026-10-06

### Bug Fixes & Improvements
- Android: a black screen instead of the game since 0.2.0 (#1633, #1597) on phones whose graphics driver cannot compile one of 0.2's new shaders (the snowfall, the lamps in the fog, the street lamps' shadow maps): the driver's error lost the whole graphics device. The game now notices this while it starts, opens the device again without those three and draws as before (snow then falls without flakes). The log names the shaders it was compiling when the device was lost.

## 0.2.0 - 2026-10-06

A release about light and weather: the Enhanced and Enhanced+ pictures worked out from the physics of the eye, the air and the lamps, and a new snowfall for every graphics mode.

### Night (Enhanced, Enhanced+)
- The night is dark again: the eye's adaptation follows a lightness-perception model (Krawczyk et al. 2005) instead of nearly full adaptation, the automatic metering no longer brightens a night, and the eye adapts to the lamps and headlights actually in view (their log-average) rather than to a fixed city level.
- Street lamps cast real shadows: shadow maps for the four lamps lighting the camera's surroundings most (the bus, poles, signs and trees throw their shadows on the street).
- A street lamp's light goes down and out, not into the sky: tree crowns and upper floors above the lamps stay dark.
- The lit ground throws light back up: a bus's flank or a facade beside a lit street is no longer black; much more so over snow.
- Moonlit nights: the moon is a directional light with its own shadows; on a dark country road under a full moon the eye takes to the moonlight.
- The night sky's glow comes from the lamps round the camera (Walker's law): a village keeps its stars, a city glows orange-grey.

### Weather (Enhanced, Enhanced+)
- Fog and mist light up round every lamp and in front of every headlight (single scattering by the droplets, a forward peak and back-scatter): halos, cones under the lamps, the glow of a bus's own beams in the fog. Rain does not do this (its drops scatter too narrowly), so headlights keep a single glow in a drizzle.
- Shafts of sunlight between the shadows of houses and trees in haze, mist and fog.
- Falling snow takes the view as a real snowfall does (heavy snow: some 400 m).
- Wet porous surfaces (soil, paving, plaster, bark) turn darker and deeper in colour in the rain, not only the asphalt.
- An overcast deck lets through light by its thickness: a raining nimbostratus is darker, a grey day over snow brighter.
- Raindrops and snowflakes are lit by what is round them - the sky, the sun, the lamps - instead of glowing at one level in the dark.

### Light and the eye (Enhanced, Enhanced+)
- Looking at the sun blinds: the eye's scattered light round it (CIE 146 glare function) with a faint ciliary corona and lenticular halo.
- Eye adaptation: the picture darkens when the sun comes into view and brightens in a dark cab or an underpass, quickly towards the light and slowly towards the dark.
- Lit windows and lights have a light glow; only what is brighter than the screen's white glows.

### Snowfall (all graphics modes)
- A new snowfall: up to 150 000 flakes worked out on the graphics card (nothing per flake on the processor), fixed in the world so the bus drives through the snow, falling gently at about a metre a second, swaying and drifting with the wind; a light fall is fewer, smaller crystals. No snow falls inside the player's bus.

## 0.1.1740 - 2026-10-05

### Merged pull requests
- Headlamps: the grass and hedges beside the road are no longer washed out at night, the beam keeps to the lane, and a full beam reaches far down the road [#1563](https://github.com/openOMSI-Project/openOMSI/pull/1563); a spot declared ahead of the bus's lamps shines from both headlamps, not from the middle [#1565](https://github.com/openOMSI-Project/openOMSI/pull/1565).
- Enhanced lighting uses a material's own ambient colour: depot interiors with black diffuse and white ambient (Thüringer Wald, OVR Lichtentanne) are no longer black [#1556](https://github.com/openOMSI-Project/openOMSI/pull/1556).
- Raised floors, markings and rails stay visible above the road surfaces at a distance, without a depth bias that moved with the camera angle [#1557](https://github.com/openOMSI-Project/openOMSI/pull/1557).
- Linux: wheels such as the Logitech G29 are no longer taken for gamepads: linear steering and native force feedback again [#1566](https://github.com/openOMSI-Project/openOMSI/pull/1566).
- Performance: no more regular 30-45 ms hitches near heavy collision meshes (Grand Paris-Moulon's street lamps) [#1561](https://github.com/openOMSI-Project/openOMSI/pull/1561).

## 0.1.1727 - 2026-10-05

### Fixes
- The Linux build of the release no longer runs out of disk space after the workspace tests (the release of 0.1.1726 did not build).

## 0.1.1726 - 2026-10-05

### Merged pull requests
- Controllers: a gilrs panic on a controller's first event no longer ends the game on Windows [#1537](https://github.com/openOMSI-Project/openOMSI/pull/1537); an idle joystick nobody set up no longer takes the arrow keys for looking [#1548](https://github.com/openOMSI-Project/openOMSI/pull/1548).
- Czech and Slovak content is read as Windows-1250, so stop names keep their ř, ě and ů [#1542](https://github.com/openOMSI-Project/openOMSI/pull/1542).
- A `.owt` weather's second `[temp]` value is read as the dew point: winter weathers give the scripts a real humidity (exhaust steam in the frost, the heating's misted panes) [#1533](https://github.com/openOMSI-Project/openOMSI/pull/1533).
- Destinations of legacy depot files show their names in the menu and on the displays, and a destination picked by hand runs the scripts' own trigger, as in OMSI 2 [#1535](https://github.com/openOMSI-Project/openOMSI/pull/1535).
- Vulkan: running out of graphics memory on a demanding map frees the old render targets before falling back, and far textures are reduced right after streaming [#1551](https://github.com/openOMSI-Project/openOMSI/pull/1551).

## 0.1.1711 - 2026-10-05

### Fixes
- The launcher's download of a server's map and buses before joining (#1486, with #1511) is taken out again for now.
- The switches for sharing and downloading mods in multiplayer (#1432) are taken out again for now.

## 0.1.1707 - 2026-10-05

### Merged pull requests
- Headlamps in Enhanced and Enhanced+ light the road as real ones do: evenly from the bumper on, wide, with a low beam's cut-off, instead of one bright pool where the lamp's axis meets the road; lamps pointing down (over a door) keep their cone [#1522](https://github.com/openOMSI-Project/openOMSI/pull/1522).
- Performance: meshes share pages of vertex and index buffers, about a third less drawing work on the CPU; Enhanced+ keeps one ray tracing structure per mesh in its page [#1340](https://github.com/openOMSI-Project/openOMSI/pull/1340).
- Catalan as an interface language [#1521](https://github.com/openOMSI-Project/openOMSI/pull/1521); Portuguese (Brazil and Portugal) complete again [#1501](https://github.com/openOMSI-Project/openOMSI/pull/1501).
- Launcher: buttons that set the start to the current time and date [#1517](https://github.com/openOMSI-Project/openOMSI/pull/1517); no automatic update from a server while downloading missing mods is off [#1511](https://github.com/openOMSI-Project/openOMSI/pull/1511).
- The personnel file stores late arrivals before early departures, as OMSI 2 does: the counts no longer swap between the two games [#1499](https://github.com/openOMSI-Project/openOMSI/pull/1499).
- Dependency updates [#1510](https://github.com/openOMSI-Project/openOMSI/pull/1510) [#1509](https://github.com/openOMSI-Project/openOMSI/pull/1509) [#1508](https://github.com/openOMSI-Project/openOMSI/pull/1508) [#1507](https://github.com/openOMSI-Project/openOMSI/pull/1507) [#1506](https://github.com/openOMSI-Project/openOMSI/pull/1506); a plugin DLL that does not load says the system's reason again.

## 0.1.1674 - 2026-10-05

### Merged pull requests
- AI trams no longer leave their rails to overtake over the oncoming lane: they wait behind what blocks them [#1512](https://github.com/openOMSI-Project/openOMSI/pull/1512).

## 0.1.1671 - 2026-10-05

### Merged pull requests
- Junction plates are raised by their height field only where the field covers them: the road connection at U Ruhleben (Berlin-Spandau) is level again, without the gap and the bump, and AI lanes follow the field there too [#1493](https://github.com/openOMSI-Project/openOMSI/pull/1493).

## 0.1.1668 - 2026-10-05

### Merged pull requests
- Controls: keyboard, wheels and gamepads can be set up while playing [#1382](https://github.com/openOMSI-Project/openOMSI/pull/1382); DirectInput wheels recover after a failed read or a replug, retrying a device that will not open at growing intervals [#1489](https://github.com/openOMSI-Project/openOMSI/pull/1489); H-pattern shifters can return to neutral when a gear is let go [#1490](https://github.com/openOMSI-Project/openOMSI/pull/1490); stale gamepad events after a focus change are ignored [#1388](https://github.com/openOMSI-Project/openOMSI/pull/1388); the menu shows the default keys of the timetable (Insert) and the information bar (Shift+Y) [#1425](https://github.com/openOMSI-Project/openOMSI/pull/1425).
- VR: the head's pitch is applied [#1373](https://github.com/openOMSI-Project/openOMSI/pull/1373).
- Launcher: the roadbook keeps a picked trip when the start follows the real time [#1460](https://github.com/openOMSI-Project/openOMSI/pull/1460); the tour list filters by route or stop, starts at the trip found and hides ended tours [#1461](https://github.com/openOMSI-Project/openOMSI/pull/1461); a shared `HOFs` folder serves every vehicle, listed in the depot too [#1491](https://github.com/openOMSI-Project/openOMSI/pull/1491); the map zooms smoothly on big maps [#1139](https://github.com/openOMSI-Project/openOMSI/pull/1139).
- Multiplayer: joining players get the map's ground textures [#1485](https://github.com/openOMSI-Project/openOMSI/pull/1485); the launcher downloads a server's map and buses before joining, and only changed files update by themselves [#1486](https://github.com/openOMSI-Project/openOMSI/pull/1486); controls for mod transfers [#1432](https://github.com/openOMSI-Project/openOMSI/pull/1432); a game the server sends or turns away ends and says why, while a refused reconnect plays on [#1429](https://github.com/openOMSI-Project/openOMSI/pull/1429); traffic light cycles no longer stall on simultaneous inactive events [#1398](https://github.com/openOMSI-Project/openOMSI/pull/1398) and stay under the host's clock [#1399](https://github.com/openOMSI-Project/openOMSI/pull/1399); a host's natural weather or weather cycle reaches the clients.
- Lua plugins: crash, pedestrian and skipped-stops events, trip, stop and vehicle details in `omsi.info()` [#1447](https://github.com/openOMSI-Project/openOMSI/pull/1447); `omsi.send` sends UDP messages (up to 8 KB) to other programs on this computer [#1448](https://github.com/openOMSI-Project/openOMSI/pull/1448); `.opl` plugins read the game's values through `openomsi_*` variables [#1361](https://github.com/openOMSI-Project/openOMSI/pull/1361).
- Graphics: trees' pictures are no longer repeated and have no line along their top [#1480](https://github.com/openOMSI-Project/openOMSI/pull/1480); no invisible road walls from thin triangles [#1454](https://github.com/openOMSI-Project/openOMSI/pull/1454); the high cloud layer's noise no longer jitters [#1357](https://github.com/openOMSI-Project/openOMSI/pull/1357); transparent terrain costs less [#1426](https://github.com/openOMSI-Project/openOMSI/pull/1426); MSAA depth prepass for mixed-material views on Apple GPUs [#1465](https://github.com/openOMSI-Project/openOMSI/pull/1465); lamps light the walls next to them softer and a doubled maplight counts once [#1372](https://github.com/openOMSI-Project/openOMSI/pull/1372); OpenGL no longer deadlocks prefetching vehicles [#1416](https://github.com/openOMSI-Project/openOMSI/pull/1416).
- Performance: a new pose rescans only the instances drawing reshaped meshes [#1466](https://github.com/openOMSI-Project/openOMSI/pull/1466); draws of materials that look alike are batched together [#1456](https://github.com/openOMSI-Project/openOMSI/pull/1456).
- Translations: Traditional Chinese expanded [#1445](https://github.com/openOMSI-Project/openOMSI/pull/1445).
- Build: the whole workspace's tests run on Windows and Linux before a release [#1431](https://github.com/openOMSI-Project/openOMSI/pull/1431); shared crates declared once, an unused patch fails the audit, Dependabot waits a week [#1403](https://github.com/openOMSI-Project/openOMSI/pull/1403) and updates wgpu with naga [#1370](https://github.com/openOMSI-Project/openOMSI/pull/1370); dependency updates [#1349](https://github.com/openOMSI-Project/openOMSI/pull/1349) [#1352](https://github.com/openOMSI-Project/openOMSI/pull/1352) [#1347](https://github.com/openOMSI-Project/openOMSI/pull/1347) [#1353](https://github.com/openOMSI-Project/openOMSI/pull/1353).

## 0.1.1554 - 2026-10-05

### Fixes
- Enhanced+ ray tracing works on Windows and Linux again (RTX and RDNA 2 cards and newer, Direct3D 12 and Vulkan), not only on Apple silicon: on Direct3D 12 its shaders were refused by the shader compiler, so every frame was thrown away and the picture stood still on the loading screen. Cut-out leaves and fences also cast their full traced shadows on Vulkan and Direct3D 12 now.
- Should a graphics driver refuse the ray tracing all the same, Enhanced+ falls back to Enhanced instead of freezing.

## 0.1.1553 - 2026-10-05

### Fixes
- Enhanced+ no longer stops drawing at the end of the loading screen on Windows and Linux: its ray tracing is used on Apple silicon (Metal) only for now, and elsewhere Enhanced+ draws as Enhanced. The graphics library's ray tracing on Vulkan and Direct3D 12 is still experimental; `OMSI_RT=1` tries it.

## 0.1.1552 - 2026-10-05

### Fixes
- "Playing now" stays up all day: the game reports every ten minutes instead of every three and waits half an hour when the counter is busy, and the website asks every five minutes - the counter had run out of its daily requests and answered nobody until midnight UTC.

## 0.1.1551 - 2026-10-05

### New
- Enhanced+ graphics (Settings → Graphics, `--enhanced-plus`): hardware ray tracing on Apple M3/M4 and newer, RTX and RDNA 2 cards - ray-traced sun shadows (soft away from their caster, crisp at the contact), ambient occlusion and reflections: wet roads, puddles, water, glass and chrome mirror what really stands around them, off the screen too. Elsewhere it draws as Enhanced.
- Natural light in Enhanced and Enhanced+: the sky is computed from the physics of the atmosphere (multiple scattering, ozone, the day's haze and its particle size, a stratospheric layer), so the blue hour, the twilight's purple and every sunset look their own. Clouds glow after the sun has set for the street, a veil of high cloud gives a milky sun with soft pale shadows, passing cumulus take the sun away and bring it back, the moon stands at its real place with its real phase and lights the night, stars show in a dark sky and a city lights its own clouds. A camera's tone curve and exposure, street lamps as bright points with a little glare (wide halos in mist and rain).
- Natural weather (the default when no weather is chosen, `--weather natural`): a physical weather model instead of one fixed state. Highs and lows pass through, cumulus grows in the afternoon and dissolves in the evening, calm clear nights leave morning fog, fronts bring a grey deck and rain, snow lies in winter and thaws in a mild spell, rain leaves clear air behind - grey days, blue evenings and sunny mornings follow from the days before, as the season and the map's latitude allow.

### Fixes
- Vehicle shadow blobs no longer flicker against the road, and in Enhanced and Enhanced+ they darken the ground under the bus as in OMSI 2.

## 0.1.1541 - 2026-10-04

### Merged pull requests
- Performance: vehicle scripts run about a quarter faster [#1328](https://github.com/openOMSI-Project/openOMSI/pull/1328), and finding the ground under the wheels costs about a quarter less CPU [#1334](https://github.com/openOMSI-Project/openOMSI/pull/1334), with the same results.
- Texture memory: on Linux with NVIDIA's driver the card's memory is read from Vulkan, and cards over 6 GB get more automatic texture memory [#1320](https://github.com/openOMSI-Project/openOMSI/pull/1320).
- Automatic start-up no longer releases the starter too early (Volvo 7900H, HH109, WSW C2), the stock buses unchanged [#1147](https://github.com/openOMSI-Project/openOMSI/pull/1147).
- Trains: cars whose bogies are declared reversed face the right way, and cars are spaced by their declared couplings as in OMSI 2 [#1186](https://github.com/openOMSI-Project/openOMSI/pull/1186).
- Camera: an optional precision curve for zooming with both mouse buttons, and a zoom cursor while the right button zooms [#903](https://github.com/openOMSI-Project/openOMSI/pull/903); the driver's eye is back on the authored seat point [#1337](https://github.com/openOMSI-Project/openOMSI/pull/1337).
- Repository: new issues are sorted by topic and milestone, repeated crash reports point to the existing issue, and pull requests get translation and file size checks; dependency updates and a weekly security audit [#1329](https://github.com/openOMSI-Project/openOMSI/pull/1329).
- Code clean-ups after review [#1322](https://github.com/openOMSI-Project/openOMSI/pull/1322).

## 0.1.1518 - 2026-10-04

### New
- The chat has a size of its own: Ctrl + the mouse wheel over it, or Settings → General → Chat size (50-300 %), for large and 4K screens.

## 0.1.1517 - 2026-10-04

### Fixes
- The README's "playing now" badge keeps its label while the counter cannot be reached.

## 0.1.1515 - 2026-10-04

### New
- Multiplayer: every variable of the other players' buses (floats and strings: gearbox, displays, IBIS, ticket printer, plugin values) is synced, so their buses look and behave as they do for their drivers.
- Updates during a session: the game looks for a new version, downloads it in the background and says so over the navigator; it is installed when the session ends. The cards can be switched off (Settings → General → "Tell me about a new version during a session").
- The launcher looks for updates again every 30 minutes while it is open and after a game ends, not only when it starts.
- "Playing now": the website and the README show how many people play openOMSI right now. The game sends only a random per-session id, its version and the kind of system; switch it off under Settings → General.

### Fixes
- Updates no longer fail with "Timeout" on a slow or unsteady connection: requests are tried again, a broken download goes on where it stopped, and github.com is asked when the GitHub API does not answer.

## 0.1.1512 - 2026-10-04

### Merged pull requests
- Mirror panels: only the player's bus notes its mirrors' glass, so the panels no longer take another bus's mirrors [#1171](https://github.com/openOMSI-Project/openOMSI/pull/1171).
- Translations: Simplified Chinese numbers in their single-character forms [#1282](https://github.com/openOMSI-Project/openOMSI/pull/1282), Traditional Chinese labels refined [#1264](https://github.com/openOMSI-Project/openOMSI/pull/1264).

## 0.1.1502 - 2026-10-04

### Merged pull requests
- Force feedback: the wheel vibrates with the road surface and the engine, and jolts fade out instead of stopping dead, with three new sliders in the driving settings [#867](https://github.com/openOMSI-Project/openOMSI/pull/867).
- The navigator shows buses, trolleybuses and trams as pictograms pointing the way they go, not as dots [#1315](https://github.com/openOMSI-Project/openOMSI/pull/1315).
- Camera: the driver's head pitch can be set (-45 to +45 degrees) under Camera → Seat position [#1179](https://github.com/openOMSI-Project/openOMSI/pull/1179); the view can glide to where the mouse turned it (Look smoothing, off by default) [#998](https://github.com/openOMSI-Project/openOMSI/pull/998); the head can sway a little while the bus waits (Head sway at a standstill, off by default) [#1204](https://github.com/openOMSI-Project/openOMSI/pull/1204); a snappier F1 camera switch and an eased Space return [#870](https://github.com/openOMSI-Project/openOMSI/pull/870).
- Controllers: the gamepad's right stick turning the view can be switched off [#1170](https://github.com/openOMSI-Project/openOMSI/pull/1170), and an idle device nobody set up no longer stops the gamepad stick from steering [#1165](https://github.com/openOMSI-Project/openOMSI/pull/1165).
- Mouse steering switched on with O starts from the centre of the window [#1301](https://github.com/openOMSI-Project/openOMSI/pull/1301).
- Mirror panels (Ctrl+M) show each mirror as its glass does, and the panel editor lists its keys and resizes panels [#1171](https://github.com/openOMSI-Project/openOMSI/pull/1171).
- Passengers press the stop button at a random distance before their stop instead of all at the same point [#1191](https://github.com/openOMSI-Project/openOMSI/pull/1191).
- Timetable: after serving the terminus, driving off starts the next trip early when it is due within five minutes [#1199](https://github.com/openOMSI-Project/openOMSI/pull/1199).
- Servers: the status page counts the shared world (AI cars, buses, people) and an admin can set the AI traffic [#1206](https://github.com/openOMSI-Project/openOMSI/pull/1206); the dispatch can take a player's duty back [#1207](https://github.com/openOMSI-Project/openOMSI/pull/1207); the world options have a button to clear the AI traffic [#1234](https://github.com/openOMSI-Project/openOMSI/pull/1234).
- The launcher's Join can be told to connect over UDP or WebSocket [#958](https://github.com/openOMSI-Project/openOMSI/pull/958).
- Lua plugins can see and drive the vehicles round the bus (omsi.others) [#894](https://github.com/openOMSI-Project/openOMSI/pull/894).
- Player screenshots leave the touch controls out [#1188](https://github.com/openOMSI-Project/openOMSI/pull/1188).
- Texture memory is no longer capped at 2 GB on cards with more, and the VRAM of AMD cards on Linux is read [#1137](https://github.com/openOMSI-Project/openOMSI/pull/1137).
- Android: a black screen on Adreno 6xx, 7xx and 8xx chips is avoided by rebuilding their shader cache [#1310](https://github.com/openOMSI-Project/openOMSI/pull/1310).
- Translations: Simplified Chinese [#1282](https://github.com/openOMSI-Project/openOMSI/pull/1282), Traditional Chinese [#1264](https://github.com/openOMSI-Project/openOMSI/pull/1264), Polish [#1262](https://github.com/openOMSI-Project/openOMSI/pull/1262) and French [#1251](https://github.com/openOMSI-Project/openOMSI/pull/1251).

## 0.1.1423 - 2026-10-04

- Build: the small-BAR fix for NVIDIA cards ([#905](https://github.com/openOMSI-Project/openOMSI/pull/905)) now comes from the project's own copy of gpu-allocator, not a personal fork.

## 0.1.1422 - 2026-10-04

### Merged pull requests
- Performance: script names are looked up without allocating and debug switches read once [#1292](https://github.com/openOMSI-Project/openOMSI/pull/1292), a wheel's ground probe walks its cell's faces once [#1287](https://github.com/openOMSI-Project/openOMSI/pull/1287), a render origin moved sideways rewrites only the models' translations [#1283](https://github.com/openOMSI-Project/openOMSI/pull/1283), an instance given another mesh is updated alone [#1280](https://github.com/openOMSI-Project/openOMSI/pull/1280), the AI vehicles' heaviest scripts each get a job of their own [#1260](https://github.com/openOMSI-Project/openOMSI/pull/1260), traffic light lamps are set again only when what they show changed [#1259](https://github.com/openOMSI-Project/openOMSI/pull/1259), and the instances are culled by blocks of 128 before one by one [#1256](https://github.com/openOMSI-Project/openOMSI/pull/1256).
- AI cars no longer drive through red lights, and a car put on the road in front of a red light comes in slowly enough to stop [#1253](https://github.com/openOMSI-Project/openOMSI/pull/1253).
- The outside camera is no longer trapped under buses whose orbit centre lies below the ground clearance [#1246](https://github.com/openOMSI-Project/openOMSI/pull/1246).
- Crash reports carry the computer, its graphics card and the map, and their title is the error alone [#1225](https://github.com/openOMSI-Project/openOMSI/pull/1225).
- Dedicated servers: a player who drives a bus the `vehicles` list of `server.cfg` does not allow is sent away and told which buses the server has [#1222](https://github.com/openOMSI-Project/openOMSI/pull/1222) (the vehicle menu offers only those since 0.1.1382).
- Bus stops put on splines are placed right in the map index before their tiles load, so a duty's stops and the announcements are right from the start [#1208](https://github.com/openOMSI-Project/openOMSI/pull/1208).
- Terrain holes: the ground's exposed edges along spline and object cuts are closed by walls textured like the ground around them [#1069](https://github.com/openOMSI-Project/openOMSI/pull/1069).
- Linux: a gear shifter or a button box (a device with buttons only) can be set up [#1059](https://github.com/openOMSI-Project/openOMSI/pull/1059).
- NVIDIA cards without Resizable BAR (Vulkan): uploads no longer go to the small BAR heap, which made loading take minutes and the game run at a few fps [#905](https://github.com/openOMSI-Project/openOMSI/pull/905).
- [#1223](https://github.com/openOMSI-Project/openOMSI/pull/1223): the same OpenGL fix (Intel HD 2500, Mali) was already in 0.1.1382.

## 0.1.1382 - 2026-10-04

Bug Fixes & Improvements

## 0.1.1313 - 2026-10-03

### Triple screens
- Native triple-screen support with the HUD on the centre screen
  ([#1127](https://github.com/openOMSI-Project/openOMSI/pull/1127)), merged with fixes: the three
  panels are drawn at one size (the depth, AO and rain targets were made anew twice a frame),
  SSAO on the angled side panels, traffic and pedestrians no longer appear or vanish on them,
  fullscreen and Alt+Enter leave a spanned window alone, nothing is drawn after a lost device,
  and the Display page shows the rig's settings only while it is on. A single screen draws
  exactly as before.

## 0.1.1302 - 2026-10-03

### Fixes
- The Drive page's map no longer fills with the interface's words after a game or a lost
  graphics device: the launcher kept the map's texture number from its old device, and a
  number past the new device's textures was drawn with the font atlas.
- The game starts again on every system: Steam's rich presence ([#996](https://github.com/openOMSI-Project/openOMSI/pull/996)) is built only where
  its library exists (Windows x64, Linux x64, macOS), the library is put beside the program
  (also for the servers and a Linux build, which could not find it), and Android, Windows ARM64
  and macOS build again (0.1.1246 was not released because of it).
- The window losing the focus or being minimised lets go of every key, button and mouse
  steering held, and takes no input until it is back; a minimised game runs at 30 fps.

### Merged pull requests
- Voice chat in sessions through an outside voice program (replaced since by the game's own, see Unreleased), players heard where they are [#1102](https://github.com/openOMSI-Project/openOMSI/pull/1102) - with a
  key handshake between the game and the plugin (a web page can no longer drive it), the voice
  server's id required, and the plugin putting the user's name and channel back.
- Resumed sessions keep the bus displays and the timetable's place, and the IBIS moves on to
  the next trip [#1087](https://github.com/openOMSI-Project/openOMSI/pull/1087).
- The server's notifications over the navigator [#974](https://github.com/openOMSI-Project/openOMSI/pull/974), a duty given by the server [#985](https://github.com/openOMSI-Project/openOMSI/pull/985),
  a fresh hello after the WebSocket is made again [#980](https://github.com/openOMSI-Project/openOMSI/pull/980), the paint the bus wears now seen by
  the other players [#1022](https://github.com/openOMSI-Project/openOMSI/pull/1022).
- Controllers: latching switches [#1060](https://github.com/openOMSI-Project/openOMSI/pull/1060), Linux wheels no longer buzz [#1042](https://github.com/openOMSI-Project/openOMSI/pull/1042); one key between
  cabin and outside [#1007](https://github.com/openOMSI-Project/openOMSI/pull/1007).
- Rendering: the render origin no longer flips while the camera is near it [#1126](https://github.com/openOMSI-Project/openOMSI/pull/1126), the
  ICU400 sign controller shows its text [#993](https://github.com/openOMSI-Project/openOMSI/pull/993), fewer meshes turned inside out [#1105](https://github.com/openOMSI-Project/openOMSI/pull/1105);
  Chinese AI car plates [#1002](https://github.com/openOMSI-Project/openOMSI/pull/1002); Volvo Wright rear doors [#1125](https://github.com/openOMSI-Project/openOMSI/pull/1125).
- Tile loading diagnostics [#1111](https://github.com/openOMSI-Project/openOMSI/pull/1111), cloudflared without console windows [#1001](https://github.com/openOMSI-Project/openOMSI/pull/1001), Simplified
  Chinese [#1028](https://github.com/openOMSI-Project/openOMSI/pull/1028), the radio documented [#1124](https://github.com/openOMSI-Project/openOMSI/pull/1124), the release history filled in [#1130](https://github.com/openOMSI-Project/openOMSI/pull/1130).

## 0.1.1246 - 2026-10-03

### AI traffic
- A car stopped behind a parked car just past a joint between road pieces pulls out round it:
  a lane change now carries on across the joint, so cars no longer queue for good behind a
  parked car on short road pieces ([#1055](https://github.com/openOMSI-Project/openOMSI/issues/1055)).
- A car held for a long time by nothing anybody can see is taken away even in view, and a
  car creeping a few centimetres at a time counts as standing; people and cars on a bridge
  above or in a subway below no longer stop a car.
- AI cars give way to the player's bus by the same rules as to each other - at junctions,
  where lanes merge, and when the bus is in the junction - and let a bus out of a stop when it
  indicates towards the road (for 20 s at most).

### Maps
- Route arrows show street and stop names in Cyrillic and other scripts their font lacks,
  drawn with the interface font in the arrow's colour.

## 0.1.1234 - 2026-10-03

### Launcher
- The Drive page is laid out again, in three steps - the bus, the day and the weather, the map
  and the duty. The column on the left is a solid panel; the bus or the map has a stage of its
  own on the right, the roadbook beside the map, and a foot under it sums the choice up with
  the buttons in a row. Nothing lies over the map or shows through a panel any more, the line
  and tour rows have room for their text, stop names keep off each other and inside the map,
  the lists stretch with the window, and the roadbook opens on your first trip.

### Graphics and devices
- OpenGL chips without storage buffers in the vertex shader (Intel HD 2500, Mali on GLES) draw
  the scene from textures instead of failing to start (`OMSI_GPU_ARRAYS=textures` forces it). [#770](https://github.com/openOMSI-Project/openOMSI/issues/770) [#316](https://github.com/openOMSI-Project/openOMSI/issues/316)
- The Vulkan and OpenGL interfaces are only started when DirectX 12 cannot draw, so their
  loader no longer breaks a DirectX 12 start on dual-GPU laptops. [#1058](https://github.com/openOMSI-Project/openOMSI/issues/1058) [#1044](https://github.com/openOMSI-Project/openOMSI/issues/1044)
- A chip without a DirectX 12 driver draws on its own Vulkan or OpenGL instead of Microsoft's
  software renderer. [#770](https://github.com/openOMSI-Project/openOMSI/issues/770)
- Script, text and HTML textures larger than the chip takes are halved until they fit.
- A window the system cannot open ends with a message instead of a panic report.
- A script value that is not a number no longer turns the bus to NaN and stops the game. [#1045](https://github.com/openOMSI-Project/openOMSI/issues/1045)

## 0.1.1223 - 2026-10-03

### Driving and controls
- The cab's steering wheel turns as far left as right: `Axle_Steering_N_L` and `_R` carry the
  axle's one angle, as Omsi.exe hands it to the scripts, not each tyre's Ackermann angle. [#953](https://github.com/openOMSI-Project/openOMSI/issues/953) [#1073](https://github.com/openOMSI-Project/openOMSI/issues/1073)
- A steering or pedal key let go while Shift is held, or while the game menu is open, lets go
  instead of turning the wheel on to full lock. [#1040](https://github.com/openOMSI-Project/openOMSI/issues/1040) [#1050](https://github.com/openOMSI-Project/openOMSI/issues/1050) [#1053](https://github.com/openOMSI-Project/openOMSI/issues/1053)
- Mouse steering switched off in the game menu keeps the brake on, as the O key does. [#517](https://github.com/openOMSI-Project/openOMSI/issues/517) [#760](https://github.com/openOMSI-Project/openOMSI/issues/760)
- Ctrl+click on the ground in the free view (F4) moves the bus to the nearest street there. [#1039](https://github.com/openOMSI-Project/openOMSI/issues/1039)
- The engine-off hint names the player's own key for the electrics. [#461](https://github.com/openOMSI-Project/openOMSI/issues/461)

### Timetable and passengers
- An early timetable bus waits at its stop until 20 s before its departure (a train 2 min), as
  in Omsi.exe, instead of leaving after 40 s. [#1012](https://github.com/openOMSI-Project/openOMSI/issues/1012)
- A tour that repeats the same trip no longer leaves its bus at the trip's last stop for good. [#976](https://github.com/openOMSI-Project/openOMSI/issues/976)
- Stops hung on another object (`[attachObj]`) are on the route map, and a duty's stop whose
  tile was not loaded at the start is reached once it loads. [#1014](https://github.com/openOMSI-Project/openOMSI/issues/1014) [#975](https://github.com/openOMSI-Project/openOMSI/issues/975)
- People leaving a bus walk onto the pavement at their own pace instead of sliding sideways. [#1033](https://github.com/openOMSI-Project/openOMSI/issues/1033) [#1079](https://github.com/openOMSI-Project/openOMSI/issues/1079)

### AI traffic and maps
- A map without `unsched_vehgroups.txt` takes its random traffic from the default AI group alone,
  as Omsi.exe does. [#1025](https://github.com/openOMSI-Project/openOMSI/issues/1025)
- AI cars whose road is drawn more than 0.3 m under their lane come down onto it instead of
  driving in the air. [#876](https://github.com/openOMSI-Project/openOMSI/issues/876)
- A signal that names no crossing is still a lamp, and reads phase 0 as in Omsi.exe. [#988](https://github.com/openOMSI-Project/openOMSI/issues/988)
- A terrain object lying outside its own tile is not drawn, as in OMSI (leftovers of copied
  tiles no longer stand in the road). [#787](https://github.com/openOMSI-Project/openOMSI/issues/787)

### Graphics
- Texture names stored in a Korean, Chinese or Japanese code page are found. [#990](https://github.com/openOMSI-Project/openOMSI/issues/990)
- A `[rendertype] surface` object with `[matl_alpha] 2` on a texture without alpha is opaque, so
  roads behind it no longer show through. [#1008](https://github.com/openOMSI-Project/openOMSI/issues/1008)
- Enhanced: the custom weather's brightness no longer darkens the mirrors; a white light map no
  longer bleaches displays and gauges at night. [#1018](https://github.com/openOMSI-Project/openOMSI/issues/1018) [#827](https://github.com/openOMSI-Project/openOMSI/issues/827)
- Vanilla+: the sphere map is blended on encoded colours as in Vanilla and Omsi.exe, so
  reflections are no longer twice too strong over dark surfaces. [#780](https://github.com/openOMSI-Project/openOMSI/issues/780)
- A mesh borrowed from another vehicle pack keeps the winding of its own pack. [#977](https://github.com/openOMSI-Project/openOMSI/issues/977) [#1054](https://github.com/openOMSI-Project/openOMSI/issues/1054)

### Menus and platforms
- Scroll bars of the game menu's drop-downs and of a line's stop list can be dragged. [#794](https://github.com/openOMSI-Project/openOMSI/issues/794)
- Names macOS stores decomposed (the weather "Eiseskälte") show their accented letters.
- A bus whose `[boundingbox]` has a negative size no longer stops the game. [#986](https://github.com/openOMSI-Project/openOMSI/issues/986)
- macOS: a fresh openOMSI.app in Applications keeps its content in `~/.openomsi/content` instead
  of creating OMSI's folders among the applications; content already installed there stays. [#1043](https://github.com/openOMSI-Project/openOMSI/issues/1043)

## 0.1.1196 - 2026-10-03

### Roads and driving
- Roads are drawn at their own height again and win over the ground by the surfaces' depth bias,
  as in Omsi.exe; the 8 cm lift is gone, so footways no longer stand over the ground aligned to
  them and markings no longer sink under the road. [#1077](https://github.com/openOMSI-Project/openOMSI/pull/1077)
- The bumps of a road texture's `.surf` map (cobbles, slabs, broken asphalt) are felt under the
  wheels, on splines and on junction objects alike (`OMSI_NO_SURF=1` for an A/B). [#1056](https://github.com/openOMSI-Project/openOMSI/pull/1056)
- Train cars stand end to end by their model bodies, not by their declared coupling points
  (a CR200J's second car no longer sits 2.6 m inside the first). [#1016](https://github.com/openOMSI-Project/openOMSI/pull/1016)

### Graphics
- Wet-road reflections work in Vanilla and Vanilla+ as well as Enhanced, also seen through the
  bus's own windows; rain films no longer feed the previous frame back into themselves. [#970](https://github.com/openOMSI-Project/openOMSI/pull/970)
- The texture fallback index looks into archives and content installed while the game runs. [#1029](https://github.com/openOMSI-Project/openOMSI/pull/1029)

### Maps and launcher
- On the navigator and city map, trolleybuses, buses and trams have their own colours and show
  their line. [#992](https://github.com/openOMSI-Project/openOMSI/pull/992)
- The Drive page shows the map itself: its roads, entry points and the chosen line's route. [#943](https://github.com/openOMSI-Project/openOMSI/pull/943)
- A server whose map is not installed here can be joined: the map comes with the server's mods. [#1076](https://github.com/openOMSI-Project/openOMSI/pull/1076)

## 0.1.1169 - 2026-10-03

### Graphics
- A declared `[matl_transmap]` whose image is missing still keeps its opaque texture slot, as
  Omsi.exe does, so traffic bodies that use diffuse alpha as a reflection mask are no longer
  half transparent.

## 0.1.1166 - 2026-10-02

### Graphics
- A vehicle's blended layers write depth unless `[matl_noZwrite]`, exactly as Omsi.exe sets its
  states (0x7fd6c4), and are drawn in model order with no reordering: stacked panes (door glass
  with dirt, decals on glass) no longer see through each other. The vehicle the camera is in is
  drawn after the whole scene and its own lamp flares after it, as in Omsi.exe. [#211](https://github.com/openOMSI-Project/openOMSI/issues/211) [#596](https://github.com/openOMSI-Project/openOMSI/issues/596)
- A 16-bit TGA keeps its alpha bit (A1R5G5B5, as D3DX reads it).

### Sound
- Sounds are mixed as in Omsi.exe's sound update (0x750340): the volume is checked against
  0 dB after the distance factor (DirectSound keeps the last volume when asked for more), a loop's
  pitch outside 100-200,000 Hz is refused and the last rate kept (the LiAZ and trolleybus
  "whine" played 30x too fast), AI vehicles use viewpoint 4, sounds of other vehicles heard from
  the cab are x(0.2 + Snd_OutsideVol) with no invented filter, only `[3d]` sounds are panned
  (5 dB at most), Doppler only on loop sounds, conditions compared exactly.

### Passengers and cash desk
- Coins lie flat on the change tray at random spots and turns, as Omsi.exe places them, instead
  of a tower that grew 3 mm a coin.
- People come from one pool, as in Omsi.exe: `ai_max_humans` (OMSI's `[AIMaxCountRandom]`,
  200 by default), walkers at most half of it. A waiting place another stop's person stands on
  is not free, so no two people stand inside each other.
- A duty moves on to the next trip when the bus stands at its first stop a minute before
  departure, so the IBIS is no longer left on the old terminus with riders refusing to board.

### Physics
- AI wheels each stand on the highest drawn face up to 3 m above them, as Omsi.exe's ground
  query (0x7a0814): no more cars sunk into cambered roads or hidden inside roads above the lane.
- A wheel no longer falls through the hair-wide seam between two spline segments (an 18 cm drop).

### Menu and website
- The pause menu uses the launcher's greys, switches, sliders and scroll bars, follows the
  Interface size, and lists destination codes in a column.
- The website has a light theme (and a theme button), readable colours, and no sideways
  scrolling on phones.

## 0.1.1147 - 2026-10-02

### Mirrors
- In the cab, Ctrl+M can put copies of the bus's mirrors over the picture; Ctrl+Shift+M opens
  an editor to move, resize, add, aim and zoom them. Layouts are saved per bus. [#920](https://github.com/openOMSI-Project/openOMSI/pull/920)

### LAN and chat
- After a LAN connection times out, the client keeps trying the host and can rejoin without
  restarting the game; `/reconnect` retries immediately. [#926](https://github.com/openOMSI-Project/openOMSI/pull/926)
- Long chat messages wrap onto following rows instead of being cut with an ellipsis. [#969](https://github.com/openOMSI-Project/openOMSI/pull/969)
- A dedicated server behind a local web gateway logs the player's forwarded address, so the
  operator can distinguish clients that would otherwise all appear as loopback. [#962](https://github.com/openOMSI-Project/openOMSI/pull/962)

### Radio
- A bus radio display can show the station and song that are actually playing. A map can supply
  its own stations in `radio.cfg` and give them frequencies by position on the map. [#945](https://github.com/openOMSI-Project/openOMSI/pull/945)

### Graphics and textures
- In Enhanced, a soaked road has puddles in patches instead of becoming one mirror from kerb
  to kerb. [#950](https://github.com/openOMSI-Project/openOMSI/pull/950)
- A missing scenery or spline texture can be found from another folder of the same content
  type when the installation contains a matching image. [#912](https://github.com/openOMSI-Project/openOMSI/pull/912)
- In Vanilla+, terrain under street lamps keeps its ground colour instead of getting a white
  or beige veil at night. [#935](https://github.com/openOMSI-Project/openOMSI/pull/935)

### Routes and input
- Route numbers with symbols, such as `-10`, are kept as written from a HOF or manual input
  and work on destination displays; the free route-number field accepts keyboard symbols. [#967](https://github.com/openOMSI-Project/openOMSI/pull/967)

## 0.1.1122 - 2026-10-02

### Graphics
- In Vanilla+, terrain no longer gets a white veil at night.
  [#935](https://github.com/openOMSI-Project/openOMSI/pull/935)

## 0.1.1120 - 2026-10-02

### Passengers and doors
- A timetable bus waits only for the people walking up to its doors from the stop it serves and
  those getting off, as Omsi.exe does, so a full bus no longer stands with open doors for ever. [#767](https://github.com/openOMSI-Project/openOMSI/issues/767)
- People off a bus walk on along the pavement instead of milling round each other at the stop. [#913](https://github.com/openOMSI-Project/openOMSI/issues/913)
- A controller button or a keyboard.cfg key can work door 1-9 front to back, or all doors
  (`door_1` ... `door_9`, `doors_all`). [#916](https://github.com/openOMSI-Project/openOMSI/issues/916)
- A second Shift+1 shuts both front leaves of the SD202 again.

### Graphics
- A transmapped car body is see-through only where its transmap is: AI cars are no longer half
  transparent, with wheel arches showing through. [#928](https://github.com/openOMSI-Project/openOMSI/issues/928) [#932](https://github.com/openOMSI-Project/openOMSI/issues/932)
- A night or light map named in a paint scheme's `[CTCTexture]` is the scheme's picture. [#895](https://github.com/openOMSI-Project/openOMSI/issues/895)
- An active chrono event that reshapes a tile brings its own terrain and water. [#923](https://github.com/openOMSI-Project/openOMSI/issues/923) [#925](https://github.com/openOMSI-Project/openOMSI/issues/925)
- A full beam reaches as much further than the low beam as its `[spotlight]` range says. [#941](https://github.com/openOMSI-Project/openOMSI/issues/941)
- A see-through layer drawn in model order (a sticker on a window) is no longer painted over by
  the opaque parts listed after it. [#918](https://github.com/openOMSI-Project/openOMSI/issues/918)

### Traffic
- A scripted child of a crossing that names one of its lights reads that light's phase, so such
  traffic lights no longer flash yellow. [#922](https://github.com/openOMSI-Project/openOMSI/issues/922)

### Vehicles
- The rear section's wheels of an articulated bus spring on the road under each of them. [#901](https://github.com/openOMSI-Project/openOMSI/issues/901)
- Textures waiting to be compressed go up at half size meanwhile, so a big articulated bus no
  longer runs a 3 GB graphics card out of memory while loading. [#921](https://github.com/openOMSI-Project/openOMSI/issues/921)
- A bus takes its own depot file of the map's place before another bus's. [#896](https://github.com/openOMSI-Project/openOMSI/issues/896)
- A four-digit line such as 7110 keeps its number on the IBIS. [#459](https://github.com/openOMSI-Project/openOMSI/issues/459)

### Launcher, menu and input
- The launcher keeps "Hold manual gear buttons", and the pause menu's switch applies to the bus.
- The passenger view (F2) turns all the way round. [#909](https://github.com/openOMSI-Project/openOMSI/issues/909)
- "Camera..." in the pause menu opens the driver's view settings. [#908](https://github.com/openOMSI-Project/openOMSI/issues/908)
- On OpenGL no thread polls the GPU beside the one drawing (a crash at start). [#898](https://github.com/openOMSI-Project/openOMSI/issues/898)
- The minimap can be dragged anywhere on the screen. [#940](https://github.com/openOMSI-Project/openOMSI/issues/940)
- Ctrl+Up / Ctrl+Down (gear up and down) are keys of the list that can be moved or cleared. [#907](https://github.com/openOMSI-Project/openOMSI/issues/907) [#930](https://github.com/openOMSI-Project/openOMSI/issues/930)
- A window size can be chosen (Graphics > Window size); under gamescope the game opens full screen. [#904](https://github.com/openOMSI-Project/openOMSI/issues/904)

## 0.1.1098 - 2026-10-02

### Passengers
- Riders complain about hard braking, fast bends and a jerky foot on the pedals (TooBad_A/B/C
  of the ticket packs), as in OMSI; the third time they get off at the next stop. [#862](https://github.com/openOMSI-Project/openOMSI/issues/862) [#873](https://github.com/openOMSI-Project/openOMSI/issues/873)
- After the bus was removed, the mouse wheel zooms on foot and the field of view holds. [#837](https://github.com/openOMSI-Project/openOMSI/issues/837)

### LAN
- A stop whose waiting people another player's bus took fills up again only once that bus has
  left, instead of a passenger a frame: no more endless streams of riders. [#842](https://github.com/openOMSI-Project/openOMSI/issues/842) [#840](https://github.com/openOMSI-Project/openOMSI/issues/840) [#830](https://github.com/openOMSI-Project/openOMSI/issues/830)
- The chat line opens on the key left of 1 where '/' is the manual gearbox's gear down.

### Graphics
- A compressed smooth dark texture keeps its colour, no longer grainy with green and blue. [#845](https://github.com/openOMSI-Project/openOMSI/issues/845)
- A mesh with no matrix or the identity is drawn as wound, as Omsi.exe draws it, so houses
  are no longer shown inside out. [#874](https://github.com/openOMSI-Project/openOMSI/issues/874)
- The see-through part of a blended layer shows no reflection (stickers). [#861](https://github.com/openOMSI-Project/openOMSI/issues/861)
- A map's WinterSnow textures show their own snow, with no white laid over them. [#879](https://github.com/openOMSI-Project/openOMSI/issues/879)
- Falling snow covers the windscreen as rain does, and the wipers clear it. [#883](https://github.com/openOMSI-Project/openOMSI/issues/883)
- In Enhanced the map's water is drawn as water: small waves, mirroring the sky. [#841](https://github.com/openOMSI-Project/openOMSI/issues/841)
- Street lamps' light map lights the roads as it lights the ground beside them. [#847](https://github.com/openOMSI-Project/openOMSI/issues/847)
- Road markings and zebra crossings lie on the road again instead of under it. [#871](https://github.com/openOMSI-Project/openOMSI/issues/871)

### Maps
- Objects beside a crossing stand on the ground the map gives them: the terrain is no longer
  pressed into a crossing's height mesh, which Omsi.exe uses for its paths only
  (`OMSI_CROSSING_DEFORM=1` brings the old way back). [#860](https://github.com/openOMSI-Project/openOMSI/issues/860)

### Vehicles
- `gear_up` shifts a gear lever past first gear (VW T3, Peugeot 106, Manta), and a force
  feedback wheel that is not set up steers without a dead zone. [#866](https://github.com/openOMSI-Project/openOMSI/issues/866)
- An automated manual gearbox (in-game driving settings, off by default). [#713](https://github.com/openOMSI-Project/openOMSI/issues/713)
- `A_Trans_*` is taken over OMSI's thirtieth-of-a-second frames, so rattle scripts rattle on
  rough roads at any frame rate. [#772](https://github.com/openOMSI-Project/openOMSI/issues/772) [#886](https://github.com/openOMSI-Project/openOMSI/issues/886)
- The game menu swaps the driven vehicle for another in its place, or reloads it. [#728](https://github.com/openOMSI-Project/openOMSI/issues/728)

### Launcher, menu and input
- "A right click ends the mouse steering" is back in the menu and the launcher. [#878](https://github.com/openOMSI-Project/openOMSI/issues/878)
- "The launcher rests while a game runs" is a setting. [#834](https://github.com/openOMSI-Project/openOMSI/issues/834)
- Mouse look sensitivity is a setting (100% = OMSI). [#859](https://github.com/openOMSI-Project/openOMSI/issues/859)
- Parked cars can be left out altogether (Parked cars: None). [#864](https://github.com/openOMSI-Project/openOMSI/issues/864)
- "Indicators cancel themselves" can be switched off. [#451](https://github.com/openOMSI-Project/openOMSI/issues/451)
- The touch wheel turns as far as Wheel rotation and Full lock say. [#856](https://github.com/openOMSI-Project/openOMSI/issues/856)
- An action can be given another key, and a mod's own trigger can be added to the list. [#854](https://github.com/openOMSI-Project/openOMSI/issues/854)
- Any route number can be typed (Route number > Type a route number...). [#836](https://github.com/openOMSI-Project/openOMSI/issues/836)
- A gamepad's right stick turns the driver's head. [#454](https://github.com/openOMSI-Project/openOMSI/issues/454)
- Buses can be starred in the bus list ("Favourites only"). [#524](https://github.com/openOMSI-Project/openOMSI/issues/524)
- On OpenGL a wait for the GPU no longer holds the GL context for seconds (a crash). [#843](https://github.com/openOMSI-Project/openOMSI/issues/843)
- On a phone, a crash long after the shaders were compiled is no longer blamed on Vulkan. [#848](https://github.com/openOMSI-Project/openOMSI/issues/848)

### Merged pull requests
- [#892](https://github.com/openOMSI-Project/openOMSI/pull/892) fractional `achse_antrieb` drives the axle, [#888](https://github.com/openOMSI-Project/openOMSI/pull/888) free camera fly keys,
  [#885](https://github.com/openOMSI-Project/openOMSI/pull/885) manual gearbox detection for touch controls, [#891](https://github.com/openOMSI-Project/openOMSI/pull/891) staged ignition start-up,
  [#881](https://github.com/openOMSI-Project/openOMSI/pull/881) Portuguese, [#839](https://github.com/openOMSI-Project/openOMSI/pull/839) mobile launcher options, [#789](https://github.com/openOMSI-Project/openOMSI/pull/789) custom weather editor,
  [#792](https://github.com/openOMSI-Project/openOMSI/pull/792) real-time reflections setting for the mirrors, [#910](https://github.com/openOMSI-Project/openOMSI/pull/910) mirror budget counted once.

## 0.1.1068 - 2026-10-02

### Mirrors
- Mirrors are no longer redrawn twice as often as their refresh budget allows.
  [#910](https://github.com/openOMSI-Project/openOMSI/pull/910)

## 0.1.1066 - 2026-10-02

### Mirrors
- Mirror reflections have a real-time setting with None, Economical and Full modes.
  [#792](https://github.com/openOMSI-Project/openOMSI/pull/792)

## 0.1.1061 - 2026-10-02

### Weather
- Custom weather can be edited from the launcher and in-game, including visibility, wind,
  temperature, humidity, clouds, precipitation, road wetness and snow. METAR ICAO stations
  can be entered directly, the controls are available on mobile too, and LAN clients follow
  a host's custom weather.
  [#789](https://github.com/openOMSI-Project/openOMSI/pull/789)

## 0.1.1050 - 2026-10-02

### Vehicles
- Very small resting speeds count as stopped, so systems that require a stationary vehicle,
  including passenger boarding, do not get held up by tiny residual motion.
  [#897](https://github.com/openOMSI-Project/openOMSI/pull/897)

## 0.1.1043 - 2026-10-02

### Tests
- Gearbox tests declare their script variables the same way a bus varlist does, including the
  shared-cockpit manual gearbox case.

## 0.1.1042 - 2026-10-02

### Vehicles
- An axle with a fractional `achse_antrieb` share is treated as a driven axle.
  [#892](https://github.com/openOMSI-Project/openOMSI/pull/892)

## 0.1.1038 - 2026-10-02

### Camera and controls
- Free-camera fly keys no longer fire OMSI `[game]` actions at the same time.
  [#888](https://github.com/openOMSI-Project/openOMSI/pull/888)

## 0.1.1036 - 2026-10-02

### Force feedback
- Road seams no longer kick the steering wheel.
  [#821](https://github.com/openOMSI-Project/openOMSI/pull/821)

## 0.1.1028 - 2026-10-02

### Passengers
- People getting off who wait at a shut exit keep to its point; they are no longer lifted up
  inside the bus and stacked there, blocking everyone behind them. [#709](https://github.com/openOMSI-Project/openOMSI/issues/709)
- Getting off, passengers head for the nearest open exit, not a shut door, as in OMSI. [#493](https://github.com/openOMSI-Project/openOMSI/issues/493)
- `GetHumanCountOnSeat` numbers the seats with the driver's place, as OMSI does (seats were one
  off for scripts).
- The driver no longer stands in a T-pose in the aisle before he is first seated.

### LAN
- Passengers handed over to a client's bus ride to a stop instead of getting off at once. [#813](https://github.com/openOMSI-Project/openOMSI/issues/813)
- The chat keys are in the key list (Controls) and can be moved off a key the bus needs. [#130](https://github.com/openOMSI-Project/openOMSI/issues/130)

### Traffic and maps
- A junction runs its traffic lights for a lamp without `[trafficlight]`, as Omsi.exe does, so
  mod traffic lights work again and AI cars stop at them. [#822](https://github.com/openOMSI-Project/openOMSI/issues/822) [#818](https://github.com/openOMSI-Project/openOMSI/issues/818)
- Timetable AI buses pass a stop where nobody gets off or waits, as in OMSI. [#703](https://github.com/openOMSI-Project/openOMSI/issues/703)
- An object with traffic paths keeps the height the map gives it (mod road pieces). [#828](https://github.com/openOMSI-Project/openOMSI/issues/828)
- A car standing behind a vehicle that waits to move over lets it in (a deadlock). [#414](https://github.com/openOMSI-Project/openOMSI/issues/414)
- A `[terrainhole]` cuts the ground along the cutter's rim, so no grass stands over the edges of
  a junction's carriageway. [#823](https://github.com/openOMSI-Project/openOMSI/issues/823) [#672](https://github.com/openOMSI-Project/openOMSI/issues/672)

### Graphics
- A bus's outer skin below the roof is lit as outside, not as cab: no seam round the body and
  its reflections are back in Enhanced. [#805](https://github.com/openOMSI-Project/openOMSI/issues/805)
- `Envir_Brightness` is the night light plus the street lamps' light on the vehicle, so bus glass
  no longer turns clear at night. [#624](https://github.com/openOMSI-Project/openOMSI/issues/624)
- Signal lenses follow their `[alphascale]` and `[matl_lightmap]` variables, and scenery objects
  get their light maps. [#826](https://github.com/openOMSI-Project/openOMSI/issues/826)

### Vehicles
- The rear section of an articulated bus finds a viaduct's deck again after a gap. [#135](https://github.com/openOMSI-Project/openOMSI/issues/135)
- A rear section turns about its own `[rot_pnt_long]` line, so steered rear axles work. [#322](https://github.com/openOMSI-Project/openOMSI/issues/322)
- `A_Trans_X/Y/Z` are the body's acceleration without gravity, as in Omsi.exe (`A_Trans_Z` read
  9.81 standing, so air suspension scripts never re-levelled).

### Launcher, menu and input
- The launcher brought forward while a game runs is drawn and answers again. [#825](https://github.com/openOMSI-Project/openOMSI/issues/825)
- A long tour no longer makes the Timetable page lose its sidebar. [#666](https://github.com/openOMSI-Project/openOMSI/issues/666)
- Save slots: the game menu saves to a new slot, and the launcher continues from any. [#341](https://github.com/openOMSI-Project/openOMSI/issues/341)
- A bus deleted from `Mods/installed` is uninstalled (its folder kept in `Mods/uninstalled`). [#819](https://github.com/openOMSI-Project/openOMSI/issues/819)
- The exit key of keyboard.cfg (Ctrl+Q) ends the game. [#817](https://github.com/openOMSI-Project/openOMSI/issues/817)
- Mouse steering goes on in the map camera (F4), as in OMSI. [#516](https://github.com/openOMSI-Project/openOMSI/issues/516)
- The paper timetable's rows are as high as GDI makes them and stay on the paper's lines. [#629](https://github.com/openOMSI-Project/openOMSI/issues/629)
- On Linux the new launcher is started after an update, not the old one. [#811](https://github.com/openOMSI-Project/openOMSI/issues/811)

## 0.1.1000 - 2026-10-02

### Menu and UI
- Interaction text is hidden when the object cannot actually be reached.
- Loading a graphics profile works from the in-game menu, and 8x MSAA is available there.
- The in-game server code can be copied, cursor flicker is fixed, and menu selection/rendering
  is less distracting.
  [#846](https://github.com/openOMSI-Project/openOMSI/pull/846)

## 0.1.983 - 2026-10-02

### Launcher and controls
- A tour can be started from a chosen trip instead of only from the duty's first one.
- Real-time and real-date options keep the launcher's choice on the system clock.
- Controller actions use readable OMSI names, and Alt+Enter switches between a window and
  borderless full screen.
  [#758](https://github.com/openOMSI-Project/openOMSI/pull/758)

## 0.1.973 - 2026-10-02

### Driver
- The driver's pose and arms are more natural: the torso sits straighter, shoulders stay
  visible through turns, and both hands remain on the wheel except while shifting.
  [#734](https://github.com/openOMSI-Project/openOMSI/pull/734)

## 0.1.968 - 2026-10-02

### Discord
- Discord Rich Presence can show the launcher or the current map, bus, line and tour, and can
  be disabled in Settings.
  [#779](https://github.com/openOMSI-Project/openOMSI/pull/779)

## 0.1.965 - 2026-10-02

### Documentation
- The changelog was expanded with 29 issue fixes from the preceding development work.

## 0.1.960 - 2026-10-02

### Passengers
- A stop is no longer a destination of itself or of a stop of the same name, so riders on
  circular lines no longer board and get straight off again. [#795](https://github.com/openOMSI-Project/openOMSI/issues/795)
- Riders know their stop by its timetable name as well as by its map label, so they get off
  at intermediate stops where the two names differ, not only at the terminus. [#773](https://github.com/openOMSI-Project/openOMSI/issues/773) [#748](https://github.com/openOMSI-Project/openOMSI/issues/748) [#736](https://github.com/openOMSI-Project/openOMSI/issues/736)

### Traffic and maps
- Cars drive into the end of a road at speed and are taken off there, as in OMSI, instead of
  stopping before it one by one; released AI buses no longer queue at the map's end. [#765](https://github.com/openOMSI-Project/openOMSI/issues/765) [#598](https://github.com/openOMSI-Project/openOMSI/issues/598)
- The player's bus holds up only traffic on its own level, not cars on a bridge above it. [#753](https://github.com/openOMSI-Project/openOMSI/issues/753)
- A depot file's stop list belongs to the trip written before it, as Omsi.exe reads it; a trip
  without a list no longer gives every later route another trip's stops in the IBIS. [#667](https://github.com/openOMSI-Project/openOMSI/issues/667)
- On a Chinese, Japanese or Korean Windows, Russian HOF and map text keeps its stop names. [#785](https://github.com/openOMSI-Project/openOMSI/issues/785)
- A continued situation resumes at the trip of the tour it was saved on, with the rest of the
  tour, and is saved again with that trip. [#653](https://github.com/openOMSI-Project/openOMSI/issues/653)

### Vehicles and input
- Door hit sounds with a `doorSpeed` volume curve are heard: a triggered sound reads its
  volume curves as they stand when the trigger fires (player's and AI buses). [#676](https://github.com/openOMSI-Project/openOMSI/issues/676)
- A switch let go with the right button held stays where it is. [#769](https://github.com/openOMSI-Project/openOMSI/issues/769)
- On foot, the switches of an articulated bus's rear section are in reach by that section. [#715](https://github.com/openOMSI-Project/openOMSI/issues/715)
- A key bound in both [game] and [vehicles] works the vehicle too. [#745](https://github.com/openOMSI-Project/openOMSI/issues/745)
- The brake stays on when the mouse steering is switched off. [#517](https://github.com/openOMSI-Project/openOMSI/issues/517) [#760](https://github.com/openOMSI-Project/openOMSI/issues/760)
- The built-in view keys step aside for keys the player bound themselves. [#701](https://github.com/openOMSI-Project/openOMSI/issues/701)
- The city map (Shift+M) and the navigator (Shift+N) open on foot. [#705](https://github.com/openOMSI-Project/openOMSI/issues/705)
- The ticket desk camera keeps where it was turned (Home recentres only when unbound). [#733](https://github.com/openOMSI-Project/openOMSI/issues/733)
- Head tracking stays on when its UDP port is taken by opentrack. [#775](https://github.com/openOMSI-Project/openOMSI/issues/775)

### Launcher
- Dropdown lists scroll by dragging their bar, and sliding an open list no longer scrolls the
  page behind it. [#794](https://github.com/openOMSI-Project/openOMSI/issues/794) [#774](https://github.com/openOMSI-Project/openOMSI/issues/774)
- Typing into an open dropdown filters it (entry points, buses, fleet numbers). [#747](https://github.com/openOMSI-Project/openOMSI/issues/747)
- The game's and the launcher's windows fit the screen and open in its middle. [#771](https://github.com/openOMSI-Project/openOMSI/issues/771)

### Graphics
- No rain inside the rear section of articulated buses. [#777](https://github.com/openOMSI-Project/openOMSI/issues/777)
- Clouds no longer vanish when a sky texture cannot be read; 24-bit bitfield BMPs load. [#749](https://github.com/openOMSI-Project/openOMSI/issues/749)
- The VR headset shows the Enhanced graphics. [#784](https://github.com/openOMSI-Project/openOMSI/issues/784)
- Fleet numbers and plates no longer glow in the dark; only text with a light or night map
  does. [#698](https://github.com/openOMSI-Project/openOMSI/issues/698)
- The loading screen remakes a Vulkan swapchain that no longer fits instead of freezing. [#776](https://github.com/openOMSI-Project/openOMSI/issues/776)

## 0.1.936 - 2026-10-02

### Graphics
- Text textures no longer break up when viewed from a distance.
  [#806](https://github.com/openOMSI-Project/openOMSI/pull/806)

## 0.1.929 - 2026-10-02

### VR
- VR navigator settings are restored in the new pause menu
  [#808](https://github.com/openOMSI-Project/openOMSI/pull/808)

## 0.1.927 - 2026-10-02

### Menu
- A new in-game menu is implemented, together with real-time and METAR synchronisation.
  [#679](https://github.com/openOMSI-Project/openOMSI/pull/679)

### Maps
- Far stand-in models are drawn only from the tiles OMSI loads together with them, so
  stand-ins no longer appear for tiles that are not part of the loaded set.
  [#743](https://github.com/openOMSI-Project/openOMSI/pull/743)
- Scenery sign text is aligned correctly again.
  [#764](https://github.com/openOMSI-Project/openOMSI/pull/764)

### Input
- Steering wheel hats are read on Linux, and the list scrolls to the pressed button.
  [#788](https://github.com/openOMSI-Project/openOMSI/pull/788)

### Physics
- Wheels no longer jolt when driving over stacked road surfaces.
  [#757](https://github.com/openOMSI-Project/openOMSI/pull/757)

## 0.1.864 - 2026-10-02

### AI traffic
- Cars simulated out of range take the same routes that an active car on the road would take.
  [#782](https://github.com/openOMSI-Project/openOMSI/pull/782)

## 0.1.862 - 2026-10-02

### AI buses
- AI buses can pull into a stop bay immediately after a junction instead of missing it.
  [#737](https://github.com/openOMSI-Project/openOMSI/pull/737)

## 0.1.857 - 2026-10-02

### Displays
- Masterbus destination displays work again.
  [#768](https://github.com/openOMSI-Project/openOMSI/pull/768)

## 0.1.853 - 2026-10-02

### Graphics
- Vehicle glow bitmaps are no longer upside down and use the same sizing as OMSI 2.
  [#741](https://github.com/openOMSI-Project/openOMSI/pull/741)

## 0.1.851 - 2026-10-02

### Maps
- Attached objects (`[attachObj]`) and object labels are read as Omsi.exe reads them. The
  records of a tile depend on its `[version]`: before version 9 there is no detail level
  line, before 6 no IDCodes, before 10 an attached object names its parent by its place in
  the tile instead of its IDCode, before 8 it has no heading, and before 11 a spline has one
  line linking it to the spline before it instead of both neighbours. Older tiles were read
  as version 14 ones, so their objects hung on the wrong parents and pieces of road stood
  in the air. An object's labels are now exactly as many lines as it says (an empty label
  or one in brackets no longer cuts the rest off), an attachment whose parent is written
  after it or that hangs on a later object of a spline attachment row is not loaded, as in
  OMSI.

## 0.1.850 - 2026-10-02

### Launcher
- Big installations no longer show an empty launcher for minutes: the maps appear at once and
  the buses as their folders are read, several folders at a time, with the progress in the
  status line. A poll no longer starts the whole reading over while it runs (the growing cache
  changed the content stamp, so a large OMSI folder never finished loading). Depot files are
  read for their name only, and the search of every vehicle folder for a map's depot runs
  once per session.

### Driving
- Mouse steering stays on: the right button looks round without ending it (also in the
  pause), the cursor goes back to where it steered when the button is let go, and the game
  starts with mouse steering as it was left, the cursor in the middle of the window. OMSI's
  right click that ends mouse steering is a setting (Esc > Options > "A right click ends the
  mouse steering").
- Door keys of UK buses: on Road-hog123's door script (London Citybus 400, Enviro400s and
  many more) `bus_doorfront0` opens the door and `bus_doorfront1` closes it, and Shift+1 fired
  both, so the door never opened. Door key triggers are now tried on the scripts first, and
  when they undo each other only the one that moves the door is fired.

### People
- Passengers get off double-deckers again: the once-a-second check for another open door ran
  every frame, pulling everyone back to the nearest path point, so people coming down from
  the upper deck stayed on the stairs. People held up face to face in the aisle or on the
  stairs squeeze past after two seconds.
- Passengers paying at the cash desk hold the money out to the tray instead of raising the
  arm up and forward: the arm's reach had its lift and turn the wrong way round.
- Standing passengers keep their feet on the ground at every tick, as Omsi.exe does, not at
  the height of their waiting place's object.
- A long bus station stop gets the waiting places of objects along its whole length.

### Performance
- People standing still are not skinned and uploaded again every frame (2.6 ms to 0.15 ms a
  frame for thirty waiting passengers).
- `OMSI_PROFILE` lists triangles per asset and names the stages of slow people ticks;
  `OMSI_SKIP_PIPE` and `OMSI_CHECK_GROUND` help measuring.

### Merged pull requests
- #661 shadow blobs lie on the road under a bridge, and can be switched off; #696 glass found
  by its texture's alpha (lamps show through every bus's windows); #699 "The game is running"
  in the launcher; #704, #687, #689, #675, #688 dedicated server administration (`tell`,
  weather by name, `/status` weather and clock, `POST /admin` from the same machine - refused
  through a tunnel or proxy); #706, #710 outside camera; #707 16x anisotropic filtering;
  #711 8x MSAA; #708 a Windows test; #714 changelog.

## 0.1.810 - 2026-10-02

### Launcher
- Buttons that contain only an icon now centre it correctly instead of leaving the spacing
  reserved for a missing label; this fixes the livery arrows on Drive > Bus and the mobile
  file browser's folder-up button (#678).

## 0.1.808 - 2026-10-02

### Traffic
- Traffic-light programs that use conditional backwards jumps to extend a phase no longer
  repeat the same jump indefinitely and get stuck on one signal combination. The extension
  is replayed once before the normal cycle continues (#692).

### VR
- VR has an adjustable cockpit navigator attached to the bus. It can be moved and rotated,
  its distance and size can be adjusted, and position, rotation, size, opacity and visibility
  are saved separately for each bus (#693).
- `Ctrl+Shift+M` enters navigator placement mode and `Ctrl+Shift+N` shows or hides it; both
  actions can be rebound. The same placement settings are available in the VR options menu
  (#693).
- The VR pause menu is smaller for a more comfortable fit (#693).

## 0.1.800 - 2026-10-02

### Server
- A dedicated server can optionally expose `GET /players` for live web maps. The JSON list
  contains each player's name, bus, line, destination, tour, position, heading and speed,
  and latitude/longitude on maps with `[worldcoordinates]` (#674).
- Player position sharing is off by default and must be enabled with `share_positions = 1`
  in `server.cfg`; otherwise `/players` returns 404 (#674).
- A player on foot is reported at the walker's position, and the walker is preferred over
  a parked bus when both exist (#674).

## 0.1.796 - 2026-10-01

### Graphics
- Enhanced graphics can reflect buses, buildings and scenery in wet-road puddles. Reflections
  follow the local road height, slope and camber, and nearby articulated bus sections are
  included (#686).
- Puddle reflections use bounded half-resolution screen-space rendering and are skipped on
  dry roads, full snow, mirror views, OpenGL and when reflections are disabled, limiting
  their cost when they are not needed (#686).

## 0.1.791 - 2026-10-01

### Controls
- DirectInput now finds generic controllers and button boxes with no axes, including custom
  Arduino, Pro Micro and STM32 devices, instead of silently leaving them disconnected.
  Keyboards, mice and screen pointers remain filtered out (#71, #662).
- Holding both opposite steering keys keeps the wheel at its current position, as in OMSI,
  instead of always giving the left key priority. Releasing either key immediately continues
  steering in the remaining direction (#663).
- `Toggle game controllers` follows its configured key binding instead of also having a
  hard-coded `K`. Leaving the action unbound now frees `K`, and rebinding it to another key
  works as expected (#649, #670).

## 0.1.785 - 2026-10-01

### Multiplayer
- In LAN multiplayer, a client's bus can take the host's waiting passengers again. They are
  claimed from the stop where the client's bus is listed and handed over as that client's
  waiting passengers.

### Project
- Repository, updater, website, documentation and release links now use the project's new
  `openOMSI-Project/openOMSI` home after the repository moved to the openOMSI-Project
  organization.

## 0.1.782 - 2026-10-01

### People
- Passengers are rewritten after Omsi.exe: the same tasks (waiting, the bus coming,
  walking to the bus, to a place, to the exit, sitting), waiting places along the stop's
  platform by its length and side, a bus taken from 60 m out and only when its terminus
  is one of theirs, the nearest open door (or one with a button), the cabin's path network
  walked by its own routing tables, a free place reserved at random (none free: they stay
  behind), the ticket stamped or bought at the desk with the game's dialogue, and everybody
  out at the terminus. The made-up queues, door waits and aisle shuffling are gone: people
  no longer stand at a door for long or crowd into the bus.
- Passengers and people on foot are posed and animated as in OMSI (its joint angles, gait
  curves, stride and stoop) instead of with leg IK. Pedestrians on the pavements stay as
  they were.
- Timetable buses no longer set off with a made-up number of riders by the hour.

## 0.1.780 - 2026-10-01

### Controls
- The left and right turn signals can be toggled by keys or wheel buttons (automatic
  cancelling still applies), and the driver's view can follow the steering with an
  adjustable angle and response (#657, #646).

## 0.1.774 - 2026-10-01

### Vehicles
- `GetTTTerminusIndex` answers the depot terminus named like the trip's terminus (or -1),
  as OMSI does: the IBIS's automatic destination showed a wrong or random terminus (#623,
  #545). Switching to free drive clears the bus's timetable values (#623).
- The cab's paper timetable is OMSI's: one Courier New text, the stop names cut or dotted to
  25 characters, 24 rows a column, one time a stop and the arrival/departure words of the
  game's language instead of German everywhere (#629).

### Controls
- The combined throttle/brake axis puts the throttle on the right half, as OMSI does (#577).
- A vJoy device no longer crashes the game when vibration starts (#655).

### People
- People on foot step only onto surfaces up to 0.5 m higher, as in OMSI, and no longer onto
  low roofs or benches (#600).
- Busy stops no longer drop the frame rate (#656).

### Traffic
- A map's own AI cars load when their `.ovh` leaves the registration affixes out (#644).

## 0.1.759 - 2026-10-01

### Traffic
- AI vehicles switch their lights on below OMSI's light value of 0.75, before the street
  lamps, and keep them on after the lamps go out (#620).
- The street lamps come on below a light value of 0.6 as in OMSI, and each scenery object's
  `NightlightA` at its own threshold (a random 0.3-0.75 with `[NightMapMode]`) (#620).
- A timetable bus at the end of its tour's last trip leaves the road at once instead of
  driving on and queueing at the map's end (#598, #536).
- The amount of street traffic follows the paths' `trafficdensity`; density-0 and no-car
  paths stay empty (#625, #161).
- Fewer stutters when AI vehicles appear (#641).

### Controls
- Looking round in a view of the bus follows the cursor and scales with the field of view, as
  in OMSI (#622, #398); the cursor shows up-down arrows while zooming (#621).
- Force feedback direction is detected and saved per wheel (#637).

### Graphics
- The scene shader compiles on OpenGL and GLES again (black screen and crashes on some
  Android and Linux machines, #617, #610, #556, #456).
- A model takes the ambient light with OMSI's white material ambient (or its
  `[matl_allcolor]`), not its diffuse colour (#530).
- Coloured lamp sprites are blended as OMSI blends them and keep their colour (#601).

### Interface
- IBIS picks the route by the trip's stops (#631); the information bar (Ctrl+Y) shows the
  passengers aboard (#628).

### Multiplayer
- A server seeds bus-stop passengers near every player (#626).

## 0.1.737 - 2026-10-01

### Sound
- A triggered sound keeps its loudest volume while it plays, as in OMSI: door sounds are no
  longer cut off when the door stops moving (#473, #611, #542).

### Passengers
- A passenger's stop request fires `int_haltewunsch` as in OMSI, not the cab's stop button
  triggers, which played the driver's switch and brake sounds on some buses (#569).

### Traffic
- The map's path rules `bus` and `trucks` open a path to AI buses and trucks by their
  `ai_veh_type`, as in OMSI: trucks no longer avoid exactly the roads marked for them, and
  bus roads are no longer closed to cars (#612).

### Graphics
- Every camera clips at 0.1 m as in OMSI: a wide field of view no longer cuts into the cab
  (#587).
- A light's sprite keeps its own colours, tinted by the light's colour (#601).
- `STLoadTex`/`STNewTex` size a script texture as D3DX does (the bitmap's power of two,
  stretched), and a resized one no longer keeps the old picture's memory (#607).
- A weather chosen from the menu comes at once, as in OMSI, and the clouds drift on smoothly
  while a weather blends in (#609).

### Controls
- Gamepad triggers on Windows are no longer read as buttons, and the D-pad works (#602).

### On foot
- The ground is the floor at the walker's level, not a stop's roof over it (#600).

### Launcher
- No "missing pack" for a part the game finds from the vehicle's own folders (#582).
- The pause notice appears once and is translated (#616).

## 0.1.721 - 2026-10-01

Pull requests from the community: #455, #560, #563, #566, #570, #580, #583, #585, #588,
#591, #592, #594, #597, #599, #604, #605, #608.

### Traffic
- AI cars no longer wait at junctions without traffic lights for no reason (#591).
- AI buses at a stop on the left open their left doors (#592).

### Controls
- Force feedback fixes and improvements (#585); manual gear buttons can be held, letting go
  returns to neutral (#570).

### Launcher
- Mods can be imported from .7z and RAR archives as well (#570).
- Better vehicle selection with a livery preview (#583); the fleet number is set before the
  bus starts up (#560).
- The launcher gives its graphics device up while a game runs (#599).

### Interface
- Interior and exterior temperatures in the HUD and the navigator (#605), stop requests on the
  minimap (#604), better roads and stop markers in the navigator (#563).
- Route numbers that start with a letter (X10, M48, N9) on the destination display (#588).

### Graphics
- Scripted LED traffic lamps get their materials, and a same-name DDS texture is preferred
  (#566); `[matl_freetex]` paths relative to the OMSI folder work (#580).
- VR: the mirrors can refresh faster, up to every frame (#608).

### Passengers
- A map's `humans.txt` may name people in nested pack folders (#455).

### Fixes
- On foot: walking stays on the bus's right level (#594); a dedicated server updates the
  passenger density and the time of day every tick (#597).

## 0.1.623 - 2026-10-01

### Passengers
- Passengers see the doors open on buses whose script sets `PAX_Entry`/`PAX_Exit` without
  listing them in its varlist (stock MAN NL, MAN NLC, many Citaros), and on 3-door buses the
  middle and rear doors let people out again; `door<i>` without the underscore works too
  (#532, #535, #512, #537, #179, from #551).
- They walk to the nearest open door or one with a request button, as OMSI does, and switch
  when the driver opens a nearer one, instead of all crowding the front door (#535).
- Waiting at a door that is still shut, they press the request and stay, and the bus
  creeping a metre or two to line up no longer sends them back to the shelter (#532, #512).

### Controls
- With the default keys plain Left/Right switch the interior camera as in OMSI (A/D steer;
  the "Arrow keys only" preset still steers with them), and the camera keys go on into the
  rear section's driver cameras of an articulated bus (#519, #525, #464).
- The viewpoint key steps through the driver, passenger, outside and map views as in OMSI
  (#519).
- Wheels: the scripts' shaking (`FF_Vib_Amp`, `FF_Vib_Period`) plays as a real periodic
  force-feedback effect (#501).
- Head tracking on Windows also reads opentrack's freetrack output (TrackIR protocol) (#522).

### Launcher
- The fleet number can be picked from the bus's number list before driving (also in a duty
  and with `--number`), as in OMSI's vehicle dialog (#538, #133).

### Graphics
- A `[matl_lightmap]` on a variable the bus does not have is always lit, as in OMSI, not
  always dark (dashboard lamps, #475, #352, #231).
- Painted ground no longer darkens through stacked layers at shallow angles, and painted
  edges no longer shift with the texture's mip level (#552).

### Maps
- `[surface]` objects stand on the terrain like every other object without `[absheight]`,
  as in OMSI (#531, #453, #460, #417).
- Stairs and excavations of presurface objects are no longer filled by the terrain (#526),
  and declared `[terrainhole]`s cut railway cuttings and underpasses however deep (#550).
- Terrain-mapped textures of a map's own folder win over global seasonal copies (#548).
- On a Windows with a Chinese, Japanese or Korean code page, content text is read in that
  code page, as OMSI does.

### Traffic
- Random AI cars at a dead end or the map edge leave at once, as in OMSI, instead of standing
  there and blocking the cars behind (#536, #327).

## 0.1.592 - 2026-10-01

### Passengers
- People getting off wait at the exit they chose when they pressed the stop button, as in
  OMSI, instead of running to whichever door opens first, usually the front one (#493, #336).
- As many people wait at a stop as the map's passenger counts for it and the passengers
  setting give, up to its waiting places, as in OMSI. Before, never more than seven: 200%
  changed nothing (#458, from #513).

### Controls
- Game controller axes use their characteristic from gamectrler.cfg (progressive,
  degressive, bi-..., range extension), as in OMSI; the launcher's controller page sets it.
  Before, a G25 set to bi-progressive steered linearly (#479).
- Plain Left/Right switch the interior camera again unless a wheel steers (#464, #519).
- The hints for a bus that does not move name the player's own keys (#461).
- P pauses and resumes without opening the menu; a LAN session cannot be paused (#527).

### Graphics
- Sun shadows stay until the sun is about a degree below the horizon, as in OMSI (they went
  at 4.6°), and overcast weather or fog under 350 m casts none (#518, #506).
- Route arrows cast no shadow (#508).
- Scenery text textures (street signs) are lit like their object and no longer glow at night
  (#470).

### Fixes
- A crash in the interface's texture cleanup (#534).

## 0.1.576 - 2026-10-01

### Passengers
- People no longer freeze at an open door before boarding (#498).
- Buses whose scripts set `PAX_Entry0_Open` for the front door only (the SD200, SD202) let
  people out of the rear doors again: an exit without a `PAX_Exit` variable follows its
  `door_<i>` (#486).

### Controls
- Steering wheels (G27, G29, Driving Force GT...): the pedals no longer act as buttons and
  buttons no longer fire twice (#480).
- The weather's METAR airport can be typed as an ICAO code (#492).

### Interface
- In-game interface size and opacity, and the notes in the corner, are settings; sharper
  text; the pause menu's Options list one line a setting (#446).
- The in-cab schedule sheet shows the player's timetable (#491).
- AI-only lines are no longer offered for driving (#483).

### Graphics
- LED panels: a "LED mip strength" slider (0-4) instead of the on/off switch (#490).
- Dashboard lamps built as `[visible]` meshes with a plain `[matl_nightmap]` light up by
  day as well, as in OMSI (#507).
- `[terrainmapping]` takes the map's first ground texture only, as Omsi.exe does; ground
  lighting and tile seams fixed (#436).
- VR: the bus mirrors are no longer black when looking round (#487).
- The driver figure's arms and the hand on a manual gear lever (#465).

### Maps
- Surface objects modelled away from their origin (bridges) stand at the road's height
  (#496); scenery `[matl_freetex]` works without a script (#474).

### Traffic
- AI cyclists ride at 15-21 km/h and no longer show green boxes on the rider (#502).

## 0.1.538 - 2026-10-01

### Graphics
- Enhanced: flipdot destination displays (the stock MAN NL/NG Krueger matrix, the flipdot
  variants of the NEOMAN and Sprinter mods), whose light map is a picture of their dots, no
  longer glow and bloom like LED panels. LED panels (a plain white light map) still glow,
  and go dark when the bus's power or lights are switched off (#413).
- Enhanced: the mirrors are no longer far darker than the view through the windscreen at
  dusk and in daytime rain; they dim only once the sun is below the horizon (#432).
- The driver figure is lit by the four interior lamps of its seat, as OMSI lights a seated
  person - coloured, by distance and from the lamp's side - instead of a flat glow of every
  lamp in the bus that left it overexposed (#206).
- `[matl_texadress_mirror]` and `[matl_texadress_mirroronce]` mirror the textures beyond
  their edges as in OMSI, instead of smearing the edge (#145).
- A TGA picture under a `.png`, `.bmp` or `.jpg` name is read by its content, as OMSI does;
  it used to leave the slot without a texture (#414).

### Maps
- A tile has water only when its `.map` says `[water]`: a leftover `.water` file no longer
  floods a tile whose water was removed (#414).

### Controls
- New setting "Dynamic steering" (Driving tab), OMSI's `redSteerSpd`: the steering keys and
  the wheel's return to centre slow down with speed (#347).
- Camera tab: "Right mouse button turns the view" switched off gives OMSI's own mouse: the
  right button zooms (up widens the cab view, backs the outside camera away) and the wheel
  button turns the view. On (the default) right-drag looks round as before and Shift+right
  zooms (#398, #104).

### Sound
- Passenger footsteps in a bus use the step sounds its `paths.cfg` gives each walkway: on the
  SD200 the stairs sound like stairs and the upper deck has its own floor. Before, every
  step picked from all of OMSI's floor samples (#311).
- The bus's outside sounds (the SD200's exterior engine) are heard from the cab through an
  open door or window, at the level the bus script gives (`Snd_OutsideVol`), as in OMSI.
- The cab no longer plays a made-up low engine rumble under the bus's own sounds: OMSI makes
  no vehicle sound of its own.

### People
- `[walk_param]` is read as OMSI reads it (stride, then arm angle), and people walk at OMSI's
  1.1 m/s ± 0.2: the stride was taken for a speed (#393).

### Vehicles
- Script callbacks round their arguments to the nearest whole number as OMSI does, instead of
  cutting them off: a terminus code of 1100.9999 out of a script's arithmetic now finds
  terminus 1101, not 1100 (#312, #197).

## 0.1.523 - 2026-10-01

### Controls
- In mouse steering the cursor sets the pedals every frame, as in OMSI: a brake held on the
  keyboard (with "The keyboard brake stays on until the throttle") no longer stays on under
  the mouse (#395).
- Letting go of a key fires only that key's own `_off` trigger, as in Omsi.exe. On some mods
  it fired an alias's release (e.g. `parking_brake_mouse_off`, `kw_blinker_*_off`), which
  undid the parking brake or the turn signal just set with the keyboard (#420).
- The hand cursor appears over every clickable part of the cockpit, including large ones
  such as door leaves, the cockpit door and the steering column (#411).

### Vehicles
- Articulated buses: the rear section stops at the joint's maximum angle from
  `[coupling_front_character]` (52.5° on the GN92) instead of folding through the front
  section when turning tightly or reversing (#410).
- The odometer starts at a used bus's reading, as in OMSI: from the year and the km a year
  in `[kmcounter_init]` (1980 and 60000 when a bus has none), ±20% from bus to bus. Before,
  such buses showed 000000; reversing now also takes the counter back (#305).
- Number plates: a repaint's own `[registration_list]` plate is used for AI buses whatever
  the template says afterwards; the player's bus takes it as the vehicle dialog does. Before,
  they showed the automatic prefix and the fleet number (#133).
- The passenger cameras of an articulated bus's rear section hang on its body like the
  front section's: `[add_camera_pax]` distance, pitch and roll (#174, #126).
- `[matl_change]`: the first `[matl_item]` is made of its own block only, no longer of the
  later items' maps (an LED matrix showed its script texture at value 1) (#210).
- A `[matl_freetex]` declared inside a `[matl_item]` changes the powered material only, and
  named material references in `.x` models are resolved (from #436).

### Maps
- Crossings draped over the ground (`[crossing_heightdeformation]`) get normals rebuilt from
  their faces, as Omsi.exe does: some junctions looked as if their faces were turned
  inside out (#428).

### Sound
- `[important]` sounds keep their place when more sounds play than OMSI's
  `[sound_maxcount]` of 200 (#434).

### Graphics
- Mirrors can be switched off (Settings, Graphics, Mirrors: Off) on slower machines (#433).

### Multiplayer
- Numpad ÷ opens the front door again instead of the chat line (#130).

### Modding
- HTML pages: `omsi.getDepartures(stop)` lists the next departures of a stop, and
  `omsi.setNextStop(index)` can also go back to an earlier stop (#437).

### Android
- The content folders and the OMSI 2 installation get a `.nomedia` file, so the gallery no
  longer lists thousands of textures as photos - the media scan kept phones busy and people
  deleted the "pictures", leaving buses white (#443).

## 0.1.486 - 2026-10-01

### Launcher
- The Settings page is split into six tabs - Graphics, Driving, Camera, Sound, Gameplay,
  General - each fitting the window, instead of about ninety controls in three long
  columns; on a phone too (#430).

## 0.1.483 - 2026-10-01

### Graphics
- PBR maps (`_nn`, `_rr`, `_ao`, ...) work on map textures: roads, splines and scenery
  objects were drawn flat - only the textures loaded on the spot got their maps, not the
  ones a tile's preparation brought, which are nearly all of a map's. Maps put into the
  same folder in the openOMSI content folder, beside a texture of the OMSI install, are
  found too.

## 0.1.481 - 2026-10-01

### Controls
- Holding both mouse buttons and moving the mouse zooms as in OMSI (its "M_Zoom"): up
  zooms in on the dashboard, down back out to the seat's view; outside, the camera moves
  further away or closer. It did nothing - the right button only looked round.
- Pressing the right button while a switch or lever is held with the left one no longer
  turns the view and stops the drag.

## 0.1.479 - 2026-10-01

### Maps
- Objects placed along a spline that follow its slope and cant (railings, posts, signs,
  lights) turn within the spline's inclined surface: one turned sideways to the road no
  longer leans across it and off the ground on a sloped or canted street, and one on a
  chain running backwards no longer tips downhill (from #409).

### Android
- No "fs_blur" shader error on OpenGL devices: the ambient occlusion pipelines, which
  OpenGL ES cannot compile, are left out there - AO stays available on Vulkan (#422).

## 0.1.476 - 2026-10-01

### Controls
- Mouse steering sensitivity can be set in the pause menu's Options (Mouse steering
  sensitivity + / -, 10% to 300%; 100% is OMSI's), and the launcher's slider goes as far.
- The log says how each game controller came in (its layout, DirectInput or the system's)
  and, the first time a stick or axis moves, whether it steers - for reports of sticks
  that do nothing.

### Windows
- The game and the dedicated server start on a PC without the Visual C++ Redistributable:
  its runtime DLLs ship beside `openomsi.exe` ("The code execution cannot proceed because
  VCRUNTIME140_1.dll was not found").

## 0.1.471 - 2026-10-01

### Physics
- Buses no longer fall through the road at junctions over a buried embankment slope
  (Cotterell, the junction by the park): a road face lying more than a metre under the
  drawn ground beneath the wheel is no road there, and the bus stands on the ground, as
  with Omsi.exe's highest-face query (#424, #423).
- The wheels roll on the road where it is drawn: splines and `[surface]` objects are drawn
  8 cm over their authored height and the bus now stands on them there, not 8 cm into
  the asphalt (#421).

### Maps
- An object stored in a neighbouring tile's file, past its own tile's edge, stands on the
  ground under it instead of the height at its tile's border (#421).

## 0.1.463 - 2026-10-01

### Passengers
- With the door release on, the SD200's automatic rear door no longer opens on its own
  while riders board at the front: riders no longer press the outside door opener, which
  Omsi.exe never does - they only ask for a door through `PAX_Entry<n>_Req` (#416, #415).

### Project
- Pull requests get a template, and ones opened from a `main` branch or an organisation
  account are closed with a note (#418).

## 0.1.455 - 2026-10-01

### Traffic
- Emergency vehicles have right of way at crossings: a vehicle whose script sets
  `TrafficPriority` goes before the others and they give way to it, as Omsi.exe does for
  any vehicle, not only the player's bus (#356).
- Timetable buses stop at the stop on their own side of the road: on a route back along
  the same street, they stopped at the stop across the road on the way out. Stops are
  matched in the trip's order, on the lane they stand beside.

### Graphics
- Enhanced: the far road and ground no longer go dark at grazing angles - what reflects
  nothing keeps its light, and a wet road reflects the sky (#374).

### Graphics cards
- Cards of up to 4 GB use the allocator's small memory blocks, and a texture budget larger
  than the card holds is taken down to its size: 2 GB cards lost their device to "out of
  memory" in the first frames (#332, #295, #323).

## 0.1.451 - 2026-10-01

### Vehicles
- Wheels stay under their hub caps: the drawn tyres are seated on the physical hub at the
  point they turn about (the rotation's `origin_trans`), not at the .o3d's own pivot. A tyre
  without a pivot (the NEOMAN's right front) was measured at a point circling the hub and
  moved up and down by centimetres as it turned, so its cap seemed to roll off it.

### View
- Smooth camera transitions when changing and entering views, a setting in the pause menu
  as well (#408, by shloooo).

### Graphics (Vanilla, at night)
- The map's lamps leave `[tree]`s dark, as in OMSI 2 (#407, by Sulamufor).
- The terrain's light map lights the ground instead of glowing over it (#406, by Sulamufor).

## 0.1.438 - 2026-10-01

### View
- The driver's hands in the cab view are a setting now (Settings and the pause Options,
  "Driver's hands in the cab view"), off by default.

### Graphics
- Vanilla: reflections blend in gamma like the rest of the classic picture; at night the
  MAN NL/NG instrument glass no longer lies milky white over the unlit gauges (#401, by
  Sulamufor).

## 0.1.434 - 2026-10-01

### Driving
- An automatic gearbox is no longer taken for a manual one. A bus counted as manual when
  its scripts answered to the gate keys (`kw_s_1`, `kw_s_2`) and read a `Clutch` anywhere -
  many automatics do both (gear hold keys, a torque converter's own clutch) - and the
  automatic clutch of the settings then worked their clutch at every stop and pull-away,
  and the phone showed a manual's gate. Now a gearbox is manual when it has the gates and
  no automatic's `automatic_D`, or when its first gate itself asks for the clutch, or when
  it works a clutch of its own through `AutoClutch` (checked on the LiAZ MKPP/GMP, the
  Sprinter G32/G-tronic, the SD202 and the NEOMAN A23).

## 0.1.433 - 2026-10-01

### Graphics
- Enhanced graphics are there again on every device and graphics API (0.1.402 left them
  out on phones and OpenGL). They are built whenever Enhanced is chosen; only a phone or
  OpenGL device not set to Enhanced skips compiling them, as it never draws them - that
  compile is what killed Mali and Adreno drivers at the start.

### Driving and view
- Scripts: a trigger starts with 1 on its stack, as in OMSI - a trigger guarded by a bare
  `{if}` did nothing (the S315 UL-GT's ticket printer switch) (#388, by hannsadrian).
- The interior camera glides between viewpoints (OMSI's `driverview_smooth`, a setting)
  (#388).
- The driver's hands are seen in the cab view, the rest of the figure folded away (#376, by
  Neblina666).

### Displays and translations
- The Atron ticket machine shows its stop text and keeps its sales screen (#390, by
  TruckiHD).
- Brazilian Portuguese improved, European Portuguese added (#396, by isaacsa2).

## 0.1.402 - 2026-09-30

### Graphics
- Rain on the windows: drops no longer run down in lanes of wavy lines all at once. Now and
  then a single drop breaks loose, slides a few centimetres to a hand's width in jerks,
  nearly straight with a little drift, and stops again - each at its own moment.

### Vehicles (compared with Omsi.exe)
- A `[matl_change]` with several `[matl_item]`s shows item n at value n, as Omsi.exe does:
  only the first was kept and shown at 1, so e.g. the MAN New Lion's City's door buttons
  (2 = lit while the door is open) stayed dark (#352).
- `[animparent]` hangs a mesh on the last mesh before it that carries the name, as Omsi.exe
  resolves it while reading: door variants reusing their arms' names moved the later
  variant's leaves with the first one's arm (Solaris Urbino III "Bode new", #348).
- Station displays and other scenery text show a string that arrives after their first
  frame (they were only redrawn on `Refresh_Strings`, #367).

### Driving
- A throttle pedal takes off a brake the keyboard holds, as the throttle key does: the
  bus was driven against its brakes (#377).

### Traffic
- Cars change only onto lanes open to their own traffic group: trucks no longer take the
  cycle paths beside a road (#327); trucks keep to the speed limit (up to 80-90 km/h, it was
  38-47 on every road), bicycles ride at 20-28 km/h.
- In multiplayer the host has traffic round itself again (#342).

### Phones and crash reports
- Phones and OpenGL leave out the Enhanced graphics' pipelines: they are never drawn there,
  and compiling the ray-marched clouds' sky killed Mali and Adreno drivers before the first
  frame (#364, #333, #316, #371).
- A crash report takes its title from the run itself: an error before the game started, or
  one the game got over, titled reports of games that died much later (#381, #331).

## 0.1.400 - 2026-09-30

### Performance
- Macs and phones: a fixed render scale keeps to the same pixel budget as the automatic
  one on high-resolution screens, the navigator draws without multisampling, the picture's
  depth is not written back where nothing reads it, and two frames are in flight so the
  graphics chip works while the next frame is prepared (M1 Pro, Thüringer Wald: 20 to 57
  fps; one frame more of input delay with V-sync) (#385, by hannsadrian).
- Linux builds link with lld (#384, by no-felix).

## 0.1.394 - 2026-09-30

### Graphics
- Rain drops on the windows are smaller again (the big ones of 0.1.381 were far too big and
  lumpy), and a drop running down leaves only a cleared track and a few beads - no more
  thin tail drawn behind it.

### Driving
- A clutch pedal pushed to the floor is fully in: a wheel's pedal reads 0.93-0.99 there, and
  the LiAZ/PAZ gearboxes part the engine from the wheels only above 0.95 and take a gear only
  at 1 - holding the clutch at a stop still stalled the engine. All pedals' last few per
  cent count as their ends; the phone's clutch is in from three quarters down.

### Passengers
- Riders who stood up as the bus pulled in get off at their stop: they lost "this is my
  stop" as the bus came to a stand and stayed at the door (#336).
- Riders of timetable buses no longer lose their stop whenever the player's bus stops
  somewhere (#317); somebody held off the exit by a pole gets off from there.

### Controllers and launcher
- Controller buttons can pause, take a screenshot, quicksave and switch mouse steering or the
  controllers (#380, by isaacsa2); DirectInput devices with unusual layouts (button boxes
  without axes) are taken (#379, by isaacsa2).
- Text fields in the launcher can be clicked into, selected and overwritten (#373, by XiZyno).
- The dedicated server starts on machines without a graphics card again (#375, by no-felix;
  #368).
- Translations completed and corrected, Traditional Chinese in full (#383, by EFour4).

## 0.1.381 - 2026-09-30

### Graphics
- Rain on the bus windows looks like real drops, in all three graphics modes (Vanilla had
  OMSI's sliding texture until now):
  - every drop is a lens: it shows the street behind the glass through itself, small and
    upside down, with the sky at its bottom (from the last frame's picture);
  - drops come in four sizes up to over a centimetre, with uneven rims, the heavy ones
    drawn out downwards, and clear glass between them;
  - a runner leaves a thin stream of water in its track;
  - while the bus drives, the airstream takes the runners up and out across the windscreen
    and back along the side windows, harder the faster it goes.
  It costs about half a millisecond at 1080p.

## 0.1.380 - 2026-09-30

### Game menu
- The pause menu's Options are kept: most switches (collisions, camera collisions, view
  turning with the steering, force feedback, keyboard brake hold, automatic clutch, head
  tracking) were written in a form the settings file read as "not set" and came back at
  their defaults, and LED glow and LED mipmaps were not saved at all.
- The launcher takes over the settings a game changed instead of writing its older copy back
  over them when it saved something of its own.

### Passengers
- Somebody at the front of a queue whom a railing, pole or shelter wall holds off the door
  boards from where they stand; they stood a metre from the open door until the bus left.

## 0.1.378 - 2026-09-30

### Performance
- Busy maps run much faster: the frame's render preparation (culling, shadow casters, draw
  lists, bundle recording) is spread over the render threads instead of one core, mirrors
  are drawn only when in view and at most 30 times a second, the navigator's map is redrawn
  at most 30 times a second, and the cab's hover pick tests only the triangles near the
  cursor. On St-Servan with traffic and passengers: 37 to 60 fps (#369, by ThiBot77).

### Website
- Download: the "Your system" badge no longer breaks across the card title, and the
  Download buttons line up.

## 0.1.374 - 2026-09-30

### Graphics
- Vanilla lights textures through the sRGB curve instead of a plain power of 2.2, so dark
  colours are no longer crushed (#343, by Sulamufor).
- Small dashboard indicator lights are drawn again instead of being dropped as too small
  (#346, by no-felix).

### Driving
- Turning the wheel can turn the driver's view into the bend, as in OMSI's "look with the
  steering wheel" (#363, by shloooo).
- AI emergency vehicles sound their siren when something holds them up (#357, by Sulamufor).
- System gamepads on Windows can have their buttons bound again (#358, by EpixIXIx).

### Game and launcher
- The game menu's scrollbar can be dragged (#354, by XiZyno).
- When the game was closed by the system (out of memory), the launcher says so and points
  to the settings that help (#355, by no-felix).
- A saved situation keeps each vehicle's livery; read-only content folders fall back to a
  writable place (#365, by no-felix).

### Multiplayer
- LAN protocol 6: a vehicle's state carries up to 63 values, the rear section's sounds and
  what is seen first come first, and INFO messages are sent at most four times a second
  (#353, with the updated #338 and #334, by Jaja80330). Players and servers need this
  version together.

### Displays
- A character a font lacks is left out, as Omsi.exe does (its glyph lookup gives none),
  instead of being drawn as the font's first glyph; spaces keep their width. The MAN Lion's
  City's odometer and trip meter lose the `|` in front of them (#370, by no-felix; #360).

### Website
- Download: the Windows button downloaded the dedicated server (its zip ends the same way).
  Every build now has its own button - Windows and Windows on ARM, macOS for Apple silicon
  and Intel, Linux and Linux on ARM, Android - the one for your system first, and the four
  dedicated servers apart below. Installing on macOS is explained too.

## 0.1.371 - 2026-09-30

### Website
- Every build has its own download button instead of matching files only by the end of their
  names. Windows ARM64, macOS Intel, Linux ARM64 and the dedicated-server builds are listed,
  the visitor's platform is shown first, and macOS installation is explained.

## 0.1.344 - 2026-09-30

### Driving
- Braking and pulling away pitch the bus as much as in OMSI (the tyres' forces act about
  the hubs), and it no longer pitches at a standstill.
- Turning the wheel no longer kicks the body into a roll: while the tyres hold, the bus
  leans only by the bend's pull, as Omsi.exe does.
- A wheel in the air stays where it hangs at rest (`Axle_Suspension`), as in OMSI.

## 0.1.342 - 2026-09-30

### Performance
- Busy spline scenes cost far less CPU: short static kerb, grass and pavement splines are
  drawn together per 48 m cell, and `[terrainmapping]` spline faces share the tile's ground
  materials (#340, by TruckiHD; for #284).

### Multiplayer
- Another player's articulated bus has its rear section lit, with its displays and its
  sounds (#338); its roller blind shows the line number (#334); a passenger in another
  player's bus hears it from inside (#330) - all by Jaja80330. The network format changed
  with #334: players and servers need this version together.

## 0.1.330 - 2026-09-30

### Driving
- The driver's and the passengers' views ride with the bus: their cameras hang on the body
  as Omsi.exe's do, pitching under braking and leaning in bends with the cab, the mouse
  look turned in the bus's frame. The level view with the cab rocking about it was most of
  the "boat" - the body's own heave, pitch and roll already settle as Omsi.exe's do.
- Mods with physics of their own: `Brakeforce` and every `Axle_Brakeforce_*` go back to 0
  after the physics read them, as Omsi.exe clears them each frame before the scripts run -
  a script that brakes only now and then (a retarder, a stop brake, custom physics) no
  longer leaves the brakes on for good.

### Traffic
- Timetable buses pull into the bay: they move over to the `[busstop]` box as Omsi.exe
  moves them - the kerb-side flank 0.3 m past the box's centre, from the stop's docking
  distance (30 m) out - whether or not a path leads into the bay (#241).

## 0.1.328 - 2026-09-30

### Vehicles
- The wheels stand on the road: the suspension's spring point is where Omsi.exe puts it,
  on the model's origin plane under each wheel, measured straight up. Measured from the hub
  less the `.bus` file's tyre radius, a mod whose tyre mesh is larger than that stood with
  its wheels sunk a few centimetres into the spline.

### Pictures
- Enhanced: chrome and metal parts of a vehicle - opaque, with a sphere map, not the body -
  are metal by their `[matl_envmap]` factor, as the vanilla picture shows them; their bump
  maps bend the reflection. The body stays paint unless it has a mask of its own.
- LED destination panels glow in the enhanced picture; "LED glow" and "LED masks keep
  their mipmaps" are settings (#324, by NACHN).

### Sound
- People walking in the street no longer sound as if they walked on a bus floor (the
  passengers' step samples, `Sounds\Passengers`, are for passengers aboard), and the stair
  samples are left out of the steps (#236).

### Game
- Teleporting to a street picked on the city map works across a big map: the navigator's
  lanes of the whole map are taken when the loaded tiles have no street there, and the bus
  waits at the street's height for its tiles instead of dropping through (#235).

## 0.1.323 - 2026-09-30

Crash reports from phones, a lost graphics device on DirectX 12, the vanilla night, and five
pull requests.

### Crashes and reports
- Phones: an app the system ended in the background (or that was swiped away) is no longer
  reported as a crash at the next start - most of the "closed without a word" reports were
  that. A report sent to GitHub carries the end of the log, and the whole report is on the
  clipboard as well; the renderer names each stage it compiles, so a report says where a
  driver gave up.
- A phone whose Vulkan driver went down while the shaders were being compiled (the reports
  that end at "cloud noise made") draws with OpenGL from then on (Settings → Graphics API
  takes it back).
- A graphics device lost on DirectX 12 starts the game again on Vulkan, as one lost on
  Vulkan starts it on DirectX 12. The launcher, too, makes its device again on the other
  interface instead of drawing on a dead one with thousands of errors (#274, an AMD Radeon).
- The automatic texture budget stays at 2.5 GB: since 0.1.237 a PC with 32-64 GB let the
  textures take 4-8 GB (#277).

### Pictures
- Vanilla: the texture times the light as Omsi.exe multiplies them, in gamma space - nights
  were several times too bright, a late dusk instead of the dark (#300).
- Night maps switch on with the street lamps, fully, as in Omsi.exe, instead of fading in
  with the dusk (a clear evening showed lit windows at a fraction, #276).
- Enhanced: chrome and other opaque sphere-mapped parts reflect again (#266, #264).

### Vehicles
- `[kmcounter_init]` starts the odometer at the bus's years in service times its
  kilometres a year (#305).
- Phones: a manual gearbox whose dashboard answers to the automatic's keys shows the manual
  gate (#279).
- The automatic clutch's help is for gearboxes that read the clutch pedal only: an
  automatic with number-key gears had its clutch pressed at stops (#234); a script without
  `engine_n` no longer keeps the clutch down for good (#260).

### Pull requests
- Merged: #298 (backwards meshes of exporters with a positive determinant: the Citelis'
  dashboard lamps, by ThiBot77), #307 (force feedback on Logitech and Moza wheels, by
  tistron), #310 (a warning when the driver uploads far too slowly, by ThiBot77), #313 (all
  buttons of a Linux wheel in the launcher, by ThiBot77), and #240's scenery-object support
  for HTML textures (by shloooo).

## 0.1.307 - 2026-09-30

Passengers, bus physics and light maps checked against Omsi.exe once more, and ten pull
requests.

### Passengers
- A bus that is not in service (no valid destination, or a "$allexit$" one such as
  Betriebsfahrt) or that stands at its own terminus empties there and takes nobody on, as
  Omsi.exe does (0x61f3e3). People boarded buses showing nothing; nobody got out at the
  last stop of a late trip (its stop index started again at 0 with the next trip - riders
  now go by the stop itself as well).
- Riders get up as the bus pulls in to their stop, not once it stands.
- Everyone on the way out holds the door request the whole way, as in OMSI: the automatic
  rear door no longer shuts on the next person walking up and opens again ("the door
  doesn't know whether people are getting off"). The requests are pulses, cleared after the
  vehicle's scripts each frame (0x7d6214): a timetable bus out of the passengers' reach no
  longer keeps its door open for good.
- The front of a queue stands aside while people get off: both used the same spot at the
  door and each waited for the other (#253).

### Physics
- `[momentofintertia]` on Omsi.exe's axes: roll is the third value, yaw the second (the
  SD202 rolled on 80 t m² instead of 300 - twice as fast, rocking over every uneven patch:
  the "boat").
- Speed bumps, cushions, manhole covers, lowered kerbs and slab edges are felt again: only
  faces under 2 cm over the road count as paint (4.5 cm took them away, and the bottom of
  every bump's ramp).
- When nothing is found under a wheel the ground is looked for up to 3 m above, as
  Omsi.exe's ground query does - a bus no longer falls through where it sank into a joint.
- An articulated bus's rear section rides on springs: a bump under its axle is a jolt.

### Light maps
- A light-mapped material is lit as D3D lights it: the material's own light and colour
  times every light - the saloon lamps included - clamped, then the light map laid on with
  ADDSMOOTH. The saloon lamps are no longer counted twice (flat white where both were on).
- Enhanced lays the light maps the same way: little by day, fully at night.
- Several maps on a slot chain as ADDSMOOTH and switch on at 0.5.

### Duties and traffic
- On a circular or turn-back route the duty no longer jumps to the stop over the road:
  stops are told apart by the direction the trip runs through them (#254).
- AI cars follow bends tighter than their model's lock instead of running wide through
  kerbs and corner houses (#249: 851 → 301 moments of a car over 1.5 m off its path in
  150 s of Spandau traffic).
- The view reset restores the zoom and the outside camera's distance (#244, with #281).

### Pull requests
- Merged: #237 (own number plate), #240 (HTML textures, by shloooo), #257 and #263 (AI
  bus displays and rear sections, by NACHN), #281 (H-pattern `kw_s_*_fest` gates, tour
  start and end in the chooser, by isaacsa2; #280 is the same), #282 (road markings and
  rails near the camera, by TruckiHD), #287 (tile light maps on their middle third, by
  Sulamufor), #288 (issue templates, by shloooo), #290 (mouse throttle reaches full, so
  automatic gearboxes kick down, by Sulamufor - taken without its build folder).

## 0.1.238 - 2026-09-30

Manual gearboxes, dashboard lamps, phones that crash or run slowly (#226, #231, #229, #225).

### Manual gearboxes (#226)
- With the automatic clutch on (the default), a gear chosen with a key, a phone's gear
  button or a controller comes with the clutch pressed and let up again, as OMSI's clutch
  key does, for gearbox scripts that only take a gear with the pedal right down and do not
  work the clutch themselves (the LiAZ and PAZ KPP: `(L.L.clutch) 1 =`). Before, a player
  without a clutch pedal - every phone - could not put such a bus in gear at all, forwards
  or backwards. Scripts that read `AutoClutch` (the Sprinters' G32) still do it themselves.
- The automatic clutch's pull-away help (the clutch bites as the throttle goes down, so the
  engine does not stall) works for these scripts' `antrieb_getr_gang` too, and is left to
  the scripts that work the clutch themselves.
- "Automatic clutch" can be switched in the launcher (Controls) and in the game menu; with
  it off, a phone shows its clutch pedal.
- The phone's gear buttons light the gear engaged for either kind of script.

### Dashboard lamps (#231)
- A `[matl_change]` variant shows as Omsi.exe shows it (0x5fd6xx): the variable rounded to
  the nearest whole number picks the `[matl_item]` (1 = the first), anything else the plain
  material - a lamp whose variable stands at 2 with one item is dark. A variable no script
  declares counts as 0, as the model loader registers it: the stock MANs' spare buttons
  (switched by `*Noch nicht belegt*`, "not assigned yet") and mods' door button lamps were
  lit all the time.

### Phones (#229, #225)
- The game's log is written on the phone too (`game.log` in the app's folder, the previous
  run's as `game-prev.log`), with the device's maker and model. A run that closed in the
  middle of a drive - a graphics driver taking the app down without a word - is shown by
  the launcher at the next start, with the end of its log for "Copy report".
- The first drive after such a closing starts with safer graphics, and on OpenGL when the
  one that closed drew with Vulkan.
- A phone's graphics chip always gets the light picture (no SSAO, no MSAA, small shadow
  maps), whatever type its driver reports.
- The automatic render scale has a fourth step, 55 %, for a chip that is still too slow at
  70 %.

### Checks
- `OMSI_DEBUG_VARS` with `OMSI_DEBUG_VARS_EVERY=<s>` logs the variables through an
  offscreen `--drive`; a manual gate given with `--triggers` comes with the automatic clutch
  as from the keys.

## 0.1.237 - 2026-09-30

The testers' second round: the crashes with "the graphics device was lost", weak cards and
phones, traffic that stood on free roads and roundabouts, passengers' necks, VR, gamepads,
and six pull requests.

### Crashes and graphics cards
- A lost graphics device (the driver reset the card: "the graphics device was lost",
  #219, #223) no longer ends the drive: the game saves the situation and starts again on
  it by itself with lighter graphics (no MSAA, no SSAO, smaller shadow maps, mirrors and
  texture budget; on Windows DirectX 12 when it was Vulkan that was lost), at most twice. The launcher does not
  report such a restart as a crash.
- Windows tries DirectX 12 before Vulkan.
- When the card runs out of memory, the textures are cut down (to 60 % of the budget each
  time, not below 300 MB) before the driver gives up.
- The interface's vertex buffers are made where a failure can be seen: after the card ran
  out of memory, one invalid buffer was written to every frame, flooding the log with
  thousands of GPU errors and taking the frame rate down to 12 fps (#217). A failed one
  is now made again at the next frame and nothing draws from it meanwhile.
- The automatic render scale moves in three steps (100, 85, 70 %), at most every five
  seconds. Every 5 % step every two seconds made all the picture's targets anew -
  hundreds of MB each time - a stutter and memory the driver ran out of. At the smallest
  scale and still too slow, SSAO and then the shadows go off.
- A small or shared graphics chip (integrated graphics outside a Mac, a phone, a card of up
  to 2.5 GB, OpenGL) is drawn without SSAO and MSAA; a card of up to 4 GB without SSAO and
  with at most 2x MSAA. `OMSI_FULL_GPU=1` keeps the settings as they are.
- The status log shows the GPU memory the textures and meshes take; on Windows the
  machine's memory sets the texture budget.
- Textures shrunk while far away come back whole at once when they are near again:
  buildings right in front of the bus stayed blurred on a map that filled the budget.

### Traffic
- A roundabout's entry no longer waits at its line for a gap at the far side of the ring:
  a nine-second gap that never came kept the queue standing for minutes (Westcountry: no
  car stuck any more, mean speed 13 -> 21 km/h).
- A hold of one frame winds a waiting driver's reaction back only a little: a junction
  "free, not free" by turns kept cars about to go for good, on open roads too.
- A car at the stop line when the light turns green goes; a green of a second let nobody
  through before.
- Nobody waits for a car of the ring that is itself creeping in a queue.
- `OMSI_DEBUG_STUCK` names the light programs and the hidden reasons a car holds.

### People
- Passengers who look at the bus turn their shoulders with it, and the head turns no more
  than 45 degrees on them. The people have no neck bone: a head turned 60 degrees on still
  shoulders twisted the neck.

### VR
- The bus's own head movement is off in the headset (the cab swayed before the eyes).
- The sphere-map reflections are laid out by the bus's heading, not by each eye's view:
  they no longer swim with every turn of the head.

### Controllers
- Gamepads (#200): the stick sets where the wheel turns to, on a gentler curve and less the
  faster the bus goes, and the wheel follows at a hand's pace; the bus no longer swerves
  with every touch of the stick.
- An Xbox pad on Windows named in OMSI's `gamectrler.cfg` keeps its sticks and triggers
  (#171).
- Force feedback (#224, #230, by tistron): DirectInput wheels have their own centring
  spring turned off before they are acquired, and again when they are acquired anew; the
  steering is lighter while turning, heavier when parking, centres itself under control
  and follows the bus's sideways acceleration; the front wheels' bumps and kerbs are felt
  as short vibrations (also in a gamepad's rumble). Steering force and vibration are set
  per controller under Controls -> Game controllers and kept in `Inputs/gamectrler.cfg`.

### Pictures
- Raindrops on the glass are lenses (#228, by Jaja80330): each drop shows the world behind
  it upside down and mirrors the sky; drops sit in three sizes on turned grids, a mist of
  droplets greys the pane, and runners slide down in fits and starts, wiping a track and
  leaving beads behind. Storms are denser and less regular (#222, by TruckiHD).
- `[rendertype] presurface` objects draw before the terrain, so excavations under the
  ground show through their invisible covers (#218 by TruckiHD, #215).

### Sound
- The player's bus's own sounds keep their pitch while the camera follows it: sound and
  listener were moved at different moments and the Doppler shift made them waver (#214,
  by TruckiHD).

### Launcher
- The timetable chooser shows the chosen trip's duration, in words as OMSI's BBS writes
  them (#233, by tistron).

### Checks
- `OMSI_AUTOPILOT=<km/h>` (offscreen): the player's bus follows the lanes and logs where
  it stands against the ground, for roundabouts and places buses fall through.

### Pull requests
- Merged: #214, #218, #222, #224 / #230 (the same commits), #228 (with #222's hash; its
  own patches replace #222's density field), #233.

## 0.1.221 - 2026-09-30

Everything since 0.1.178: the testers' reports from Fikcyjny Szczecin (MAN NL/NG Enhanced),
KS Węglin, Cotterell and The Adstow Project, multiplayer, the trains, ten GitHub issues and
six pull requests. Where OMSI 2 has the behaviour, it was taken from Omsi.exe itself (the
addresses are in the commits).

### Driving and physics
- The suspension is Omsi.exe's: the body hangs on a spring and damper at each wheel over the
  ground point under it (`achse_feder`, `achse_daempfer`, `Axle_Springfactor`, capped at
  `achse_maxforce`), with no tyre spring, wheel mass or bump stop of our own in between.
  Buses no longer float over the road "like a boat"; every bus drives on its own `.bus`
  values. (`OMSI_TYRE_SUSPENSION=1` brings the old model back for comparison.)
- The driver's head moves as in OMSI: thrown by the body at the eye, up and down always,
  sideways and fore and aft with `[driverview_moving]`, never more than 10 cm.
- Mouse steering switched off (right click, O, the menu) leaves the wheel where it is; the
  keys go on from there (#184).
- While the view is turned with the mouse, the cursor shows OMSI's four arrows (#185).

### Keyboard
- `Inputs/keyboard.cfg` is read as Omsi.exe reads it (#195): the third value's bit 1 means
  "the action follows the key's state" (throttle, brake, steering), 2 is Shift and 4 is Ctrl.
  The stock driving keys no longer need Shift; parking lights are Shift+L, the quicksave
  Ctrl+S, the screenshot Ctrl+Shift+P, the information display Shift+Y, the timetable Insert
  and the ticket desk camera Home. Rebinding a key in the launcher keeps the entry's own
  "held" bit; an Alt chord of our own is bit 8, which OMSI ignores.

### Pictures, lights and mirrors
- Material highlights are Direct3D's per-vertex specular term from the sun and the light
  above, as in OMSI: gear selectors, buttons and screens no longer catch sharp sun spots.
- `[matl_envmap]` on glass and paint blends as Omsi.exe blends it: windows are no longer
  mirrors of the street; the enhanced picture's glass reflects 4-12 %.
- `[matl_lightmap]` is on or off at its variable's 0.5 and is added to the light before the
  texture (ADDSMOOTH): lit saloons glow at night and hardly show by day.
- Mirrors and door monitors (the BMC Procity's `camera_TFT` among them) show what they
  reflect: each frame the ray from the eye to the mirror is reflected in the mirror's face,
  as Omsi.exe does; left mirrors, kerb-side blind-spot mirrors and middle-door monitors no
  longer look into the saloon or the sky (#192).
- A lamp's `[light_enh]` lights move with the mesh they belong to: a level crossing's
  barrier lamps stay on the barrier.

### Map objects
- Parked cars stand on the ground as Omsi.exe puts them, with the map's own pitch and bank,
  no longer tilted by the slope under them (Cotterell).
- Attached objects turn as Omsi.exe turns them (own rotation, then the parent's).
- The stop helper (`routearrows_busstop.sco`) stands on the stop object with its rotation;
  it no longer "cuts" into the bus beside it.
- A far AI bus keeps its destination sign instead of a flat colour beyond 50 m.
- Scenery whose free-texture filename is built in `{frame}` shows its texture (#198).
- `model.cfg` `[item]`/`[setvar]` are paint schemes of the model, as in Omsi.exe, and the
  chosen scheme's variables are there for the scripts' `{init}` (#190).
- Checks for road builders: `OMSI_CHECK_SPIKES`, `OMSI_HOLE_PHOTO`, `OMSI_ROAD_PHOTO_N`;
  `--cam` takes a field of view.

### Passengers and people
- Passengers get off where Omsi.exe sends them: each rider's stop is drawn among the stops
  ahead by the stops' "passengers alighting" numbers. They no longer all leave after one or
  two stops.
- Waiting passengers keep to their nearest door while it opens: a bus whose rear doors open a
  moment before the front one no longer sends the people at the front to the back.
- People on foot wait for a car or bus standing in their way on a crossing, then go round
  it, instead of pressing against its side.
- F2 reaches the passenger cameras of an articulated bus's rear section.

### Traffic
- Traffic keeps to the middle of its lane beside parked cars (narrow British streets, The
  Adstow Project).
- Random traffic keeps to its pool's path densities (`unsched_vehgroups.txt` pools with their
  own `[rule] trafficdensity`), and a positive density as low as 0.001 still lets cars on
  (#201, #199, from Aurora Studio). A car whose pool may go nowhere at a junction goes on
  where cars may instead of standing there.

### Trains
- `[trainreverse]` works as in Omsi.exe: a train whose next trip runs the other way is turned
  round where it stands - its last car leads - and goes on with the trip. The Berlin U-Bahn
  no longer drives off the end of its siding while another train appears for the trip back.
- Trains stop with their front at the station, as Omsi.exe measures it (half the train's
  length and the `[ai_brakeperformance]` holding offset): the S-Bahn and U-Bahn no longer
  stand half a car past the end of the platform. Train cars without `[boundingbox]` take
  their model's length (18 m, not 12 m).

### Multiplayer and servers
- No more micro-teleports: states are stamped with the moment of the frame they show, and the
  other players' buses and the host's traffic are drawn by a clock that runs smoothly
  instead of jumping with every datagram. Another player's bus: speed jitter per frame
  median 11 % -> 1.4 %; the host's cars at a client are drawn within 2 cm of where the host
  has them.
- A joining player sees the host's traffic and people whatever their own traffic settings
  ("passengers but no traffic" on a server).
- A server no longer stalls when someone drives a bus it cannot load: it tries again after
  half a minute, loads only the buses its `vehicles` list allows and shows the first of them
  for any other.
- Joining by code starts on the host's map.
- Session codes end in a full group of four characters (#152); old codes are still read.
- Each camera keeps where it was turned, as in OMSI 2.
- A door whose entry point lies on the aisle opens to the kerb: the left where traffic keeps
  left.

### VR (Windows)
- OpenXR VR support (#168, by EpixXx): stereo rendering with head tracking, a spatial Esc
  menu and cockpit pointer, right-click zoom, the headset picture on the monitor, its own
  settings and keys (Ctrl+Shift+R recentre, F7 monitor picture, F8 VR / desktop). See
  [docs/VR.md](docs/VR.md).

### Phones and on-screen controls
- With `OMSI_TOUCH=1` on a computer, the mouse works the on-screen controls as a finger
  (from #202).

### Translations
- Hungarian refined (from #143, by agost4002).

### GitHub issues closed
- #127 (an overlay layer drawn opaque), #151 (default specular), #152 (session code), #176
  (shiny windows), #184 (mouse steering), #185 (look cursor), #187 (envmap brightness), #190
  (`[setvar]`), #192 (mirrors and door monitors), #195 (keyboard.cfg bits).

### Pull requests
- Merged: #168 (VR), #198, #199, #201. Taken in part: #202 (the mouse as a finger; its fixed
  gear panel and the committed rustup installer were left out), #143 (the Hungarian lines;
  its edits to the English texts would have dropped those lines in every language).

## 0.1.182 - 2026-09-30

### Website
- Ko-fi was added alongside Buy Me a Coffee, and the site's corner Donate button opens a menu
  with both options.

## 0.1.180 - 2026-09-30

### Multiplayer
- Joining a server by code starts on the host's map even when the server welcome arrives after
  loading has begun; maps not installed locally can still come with the host's mods.

## 0.1.178 - 2026-09-30

Everything since 0.1.146. Where OMSI 2 has the behaviour, it was taken from Omsi.exe itself.

### Roads, splines and the ground
- Roads no longer disappear under the grass. Splines the map marks `[spline_terrain_align]`
  cut their outline out of the ground, as Omsi.exe does: whole stretches of Spandau's roads,
  the six-lane Falkenseer Chaussee among them, were buried. The cut is exact to a few
  centimetres: no sky along the kerbs, and narrow medians stay green.
- The ground is no longer taken away under every road in rough 1.5-3 m steps (the "holes in
  the world" beside kerbs and car parks); only where the map says.
- Road cant takes its width from the spline's height profiles, as in Omsi.exe.

### Vehicles
- Bellows of articulated buses bend with the rear section on slopes instead of away from it.
- Skinned meshes (bellows, levers of mod buses such as the AA-FR Agora) deform as in OMSI 2.
- Headlights in the classic picture shine forward from the lamps, one beam per headlamp,
  as bright as in OMSI 2, and no longer light up the bus's own saloon and dashboard.
- Roller-blind destination displays (`[texcoordtransY]`, `[matl_freetex]`, borders) work.
- Thüringer Wald buses keep their roof at night.
- Mirrors see closer and further (0.1 m to the objects' range, as Omsi.exe).

### Trains
- Trains are put together as in OMSI 2: every unit with its cars, the last car turned round
  (Berlin U-Bahn A3, S-Bahn BR 275).

### AI traffic and passengers
- AI cars no longer wait for each other for ever: a long wait at a side road now gets its
  turn, and two cars that each waited for the other drive on.
- Passengers at a stop no longer all stare at the driver: each watches a coming bus on their
  own, and only the people it takes keep looking once it stands.
- Timetable buses' door handshake follows Omsi.exe (a trace: `OMSI_DEBUG_DOORS=1`).

### Weather and administration
- Weather cycle (launcher, phone launcher, `weather = cycle` in server.cfg): a new weather
  every 25-60 game minutes, fitting the month; every weather change blends in over 4 minutes.
- Server admins: set any installed weather, switch the cycle on and off, clear jammed traffic.

### Multiplayer
- The official server: type `openomsi` to join "openOMSI | Official Server"; it is first in
  the server list.
- Any server address works: an IP, a host name, host:port or a link.
- Parked cars are the same for everybody: a car that drove off at the host is gone for the
  other players too (their buses drove through cars only one side had).
- Joining keeps the duty on the host's map; the launcher never hangs on a job that died.

### Phones
- A launcher made for phones: tabs at the bottom, a Play screen, full-screen choice sheets.
- Manual gearboxes on the touch controls, with a clutch pedal.
- Installing mods works again (it stood at "reading the archive's table of contents").
- On foot, the own bus answers clicks.

### Performance
- Less stutter when the camera moves (culling buffers are kept between frames).

## 0.1.175 - 2026-09-30

### Passengers
- Waiting passengers decide individually whether to watch an approaching bus, and once it
  stops only the people who intend to board it keep looking at it.

## 0.1.174 - 2026-09-30

### Trains and multiplayer
- Train and coupled-vehicle consists are assembled in OMSI's order, including reversed cars
  and couplings from every unit. The same orientation rules apply to the player, AI and LAN
  vehicles.

## 0.1.173 - 2026-09-30

### AI traffic
- A car that has waited a long time at a junction keeps its turn instead of being starved by
  every new arrival.
- Two cars that end up treating each other as their lead no longer wait for each other
  forever.
- A car committed to changing lanes is no longer held back by parked cars in the lane it is
  leaving.

## 0.1.171 - 2026-09-30

### Performance
- Main-view culling buffers are reused between frames, reducing allocation and camera-motion
  overhead without changing visibility or LOD decisions.
  [#175](https://github.com/openOMSI-Project/openOMSI/pull/175)

## 0.1.160 - 2026-09-30

### Roads and splines
- A spline type's half cant width is derived from its height profiles, as Omsi.exe does;
  splines without height profiles have no cant.

## 0.1.146 - 2026-09-29

### Scenery
- Scripted advertisements can be rendered on scenery objects.
  [#147](https://github.com/openOMSI-Project/openOMSI/pull/147)

## 0.1.144 - 2026-09-29

### Controllers
- Linux wheels get force feedback and stable button numbering.
  [#146](https://github.com/openOMSI-Project/openOMSI/pull/146)

## 0.1.139 - 2026-09-29

### Mouse steering
- A right click can end mouse steering again.
  [#162](https://github.com/openOMSI-Project/openOMSI/issues/162)

## 0.1.138 - 2026-09-29

### Diagnostics
- `OMSI_DEBUG_TRAILERS` logs coupled parts whose level does not match the vehicle pulling
  them.

## 0.1.136 - 2026-09-29

### Graphics
- Model-order vehicle drawing is off by default again after it exposed saloons through body
  panels on several buses; `OMSI_MODEL_ORDER=1` keeps the ordered path available for tests.

## 0.1.135 - 2026-09-29

### Scripting
- `$cutBegin`, `$cutEnd` and `$SetLength` round their counts like Omsi.exe, and `$*`
  repeats a pattern to fill the requested length.

## 0.1.133 - 2026-09-29

### Passengers
- Waiting passengers stop at the pavement instead of stepping into traffic, and people under
  shelters stand on the floor at their own height rather than on the roof above them.

## 0.1.132 - 2026-09-29

### Passengers
- Passengers choose buses from the stops and termini served by the timetable, as Omsi.exe
  does, instead of using the old random "take the next bus" rule.

## 0.1.131 - 2026-09-29

### Text and displays
- Text textures follow Omsi.exe's font rules for missing glyphs, clipping and glyph spacing,
  fixing destination words that could run together or be squeezed into a texture.

## 0.1.129 - 2026-09-29

### Graphics
- Vehicle materials follow Omsi.exe's fixed-function texture stages more closely, including
  environment maps, transmaps and model-order blend/depth behaviour.

## 0.1.126 - 2026-09-29

### AI traffic
- Cars no longer brake for the player's bus merely following behind them.
  [#139](https://github.com/openOMSI-Project/openOMSI/issues/139)

## 0.1.121 - 2026-09-29

### Diagnostics
- `OMSI_JOINT_ANGLE` can hold an articulated bus's rear section at a chosen angle in
  offscreen tests, making joints and bellows easier to inspect.

## 0.1.119 - 2026-09-29

### Passengers
- Passengers meeting an arriving bus step only toward the kerb and stay on the pavement
  instead of standing out in the road.
  [#123](https://github.com/openOMSI-Project/openOMSI/issues/123)

## 0.1.117 - 2026-09-29

### Graphics
- Bus `[matl_alpha]` modes are respected again, so alpha-tested cut-outs and overlay layers
  are not forced opaque by the body-depth repair.
  [#127](https://github.com/openOMSI-Project/openOMSI/issues/127)

## 0.1.116 - 2026-09-29

### Camera
- The outside-view zoom changes in steps again instead of compounding every frame and jumping
  straight to its minimum or maximum.
  [#126](https://github.com/openOMSI-Project/openOMSI/issues/126)

## 0.1.110 - 2026-09-29

### Maps
- Objects attached to splines are placed at their stored chain offsets.
  [#124](https://github.com/openOMSI-Project/openOMSI/pull/124)

## 0.1.101 - 2026-09-29

### Graphics and startup
- The game and launcher try the requested graphics backend and then other available Vulkan,
  DirectX 12 and OpenGL adapters instead of giving up on the first device that cannot open.
- Windows asks switchable-graphics drivers for the high-performance GPU, and a complete
  failure now reports an error cleanly instead of panicking.
  [#97](https://github.com/openOMSI-Project/openOMSI/issues/97)

## 0.1.99 - 2026-09-29

### Localisation
- English interface strings remain the lookup keys, so renamed text does not silently lose
  every translation; the Hungarian improvements are kept.

## 0.1.96 - 2026-09-29

### Performance
- Main-thread frame time is reduced on busy maps.
  [#119](https://github.com/openOMSI-Project/openOMSI/issues/119)

## 0.1.95 - 2026-09-29

### Graphics
- Graphics cards with about 2 GB of VRAM give textures a smaller share of memory, avoiding
  device loss on maps such as Grundorf with MSAA, SSAO and shadows enabled.
  [#114](https://github.com/openOMSI-Project/openOMSI/issues/114)

## 0.1.92 - 2026-09-29

### Maps
- Map pitch and bank are converted into the world's rotation convention correctly, fixing
  scenery such as Thüringer Wald rock faces.
  [#117](https://github.com/openOMSI-Project/openOMSI/issues/117)

## 0.1.91 - 2026-09-29

### Materials
- When a material slot has several `[matl_change]` records, the item is shown while any of
  their variables is active, matching Omsi.exe.
  [#112](https://github.com/openOMSI-Project/openOMSI/issues/112)

## 0.1.87 - 2026-09-29

### Controllers
- DirectInput now logs why a force-feedback wheel could not get exclusive access or create its
  constant-force effect instead of silently running without forces.
  [#99](https://github.com/openOMSI-Project/openOMSI/issues/99)

## 0.1.84 - 2026-09-29

### Maps and graphics
- Only splines whose profiles are entirely overhead are excluded from the ground surface;
  walls, embankments and watersides still shape the ground.
- Mesh files are looked up beside their model first, and the vehicle body-depth repair keeps
  applying where it is needed.

## 0.1.79 - 2026-09-29

### Materials
- When several plain `[matl]` blocks address the same slot, a later `[matl_alpha]` can set
  the slot's alpha mode, matching OMSI's material handling.
  [#95](https://github.com/openOMSI-Project/openOMSI/issues/95)

## 0.1.78 - 2026-09-29

### Build
- `Cargo.lock` now records the Windows dependency used by `omsi-render` for DXGI.

## 0.1.68 - 2026-09-29

### Controllers
- Controllers plugged in while the game is running are detected again without periodic
  polling stutter, and DirectInput device names with non-Latin letters match their
  `gamectrler.cfg` entries.
  [#71](https://github.com/openOMSI-Project/openOMSI/issues/71)

## 0.1.60 - 2026-09-29

### Documentation
- The README links the modding guide, and the build guide documents the Android toolchain and
  build command.
  [#66](https://github.com/openOMSI-Project/openOMSI/issues/66)

## 0.1.59 - 2026-09-29

### Documentation
- The format documentation now covers model LOD selection, transmap alpha, entry-point records
  and map tile numbering.

## 0.1.57 - 2026-09-29

### AI traffic
- A car stuck at a dead end leaves sooner when a queue is building behind it.

## 0.1.56 - 2026-09-29

### AI traffic and graphics
- UK AI car packs are visible again, their paint/transmap layers render correctly, and object
  LOD selection follows OMSI's model order.

## 0.1.55 - 2026-09-29

### Physics
- Stacked road markings are ignored as extra steps by the wheel-ground query.

## 0.1.54 - 2026-09-29

### Maps
- Tile numbers include repeated `[map]` entries, as in OMSI, and entry points are resolved on
  their own tile instead of a different tile with the same object id.

## 0.1.53 - 2026-09-29

### Audio
- Sound follows the system output device when headphones or a Bluetooth headset are connected
  or disconnected while the game is running.

## 0.1.52 - 2026-09-29

### Vehicles and maps
- A bus is put down on the surface its wheels should stand on near the entry point's height,
  rather than on the highest nearby surface such as a wall top or bridge deck.

## 0.1.51 - 2026-09-29

### Passengers
- Seated passengers on high seats let their feet hang naturally instead of stretching their
  legs straight down to the floor.

## 0.1.48 - 2026-09-29

### Passengers
- People keep to the floor at their own height instead of being placed on a roof or deck above
  them.

## 0.1.45 - 2026-09-29

### Localisation
- Fourteen interface languages were added, bringing the built-in total to eighteen, with the
  launcher and game-menu strings filled in and system fonts used for Chinese, Japanese and
  Hindi text.

## 0.1.44 - 2026-09-29

### Launcher
- The bus list is rebuilt only when its search, content or host list changes instead of every
  frame.

## 0.1.42 - 2026-09-29

### Graphics
- Puddle splashes are rendered as a widening, scene-lit mist instead of glowing sprite rings.

## 0.1.40 - 2026-09-29

### Controllers
- Pressing a controller button highlights it in the controller list, making bindings easier to
  identify.

## 0.1.37 - 2026-09-29

### Controls
- OMSI's held keyboard-pedal behaviour is available as an option: a tapped throttle or brake
  can keep its position until the opposite pedal is used.

## 0.1.34 - 2026-09-29

### Doors and spawning
- Berlin buses' rear doors close on Shift+2 again.
- An entry point whose marker lies below empty space places the bus on the ground above it,
  while real lower levels are preserved.

## 0.1.32 - 2026-09-29

### Website
- Buy Me a Coffee links were added to the README and project website.

## 0.1.31 - 2026-09-29

### Mirrors
- `reflexionN.bmp` camera pictures work wherever a vehicle material refers to them, including
  switched material items and light/night maps.

## 0.1.29 - 2026-09-29

### Traffic
- Barriers can open for the player's bus when it approaches off the traffic lanes, such as in
  depot yards.

## 0.1.27 - 2026-09-29

### Diagnostics
- Missing objects, splines and textures are reported by add-on in
  `~/.openomsi/missing_content.txt`.
- The game log records the system, launch settings, controls, views, pauses and regular runtime
  status so broken maps and sessions are easier to diagnose.

## 0.1.26 - 2026-09-29

### World
- A bus on a genuine lower level is no longer "rescued" onto a roof, and teleports that specify
  a height keep the intended level.

## 0.1.24 - 2026-09-29

### Controls and releases
- Door keys are assigned from the model's actual doorways and the triggers that move their
  leaves, fixing unusual mod door layouts.
- Release notes are generated from changelog lines added since the preceding release instead
  of falling back to a generic message.

## 0.1.22 - 2026-09-29

### Camera and controls
- Mouse steering can keep turning beyond the window edge, free-camera keys no longer drive the
  bus at the same time, and the mouse wheel zooms in free camera and on foot.
- Snow is matte instead of taking the wet-road gloss.

## 0.1.21 - 2026-09-29

### Physics
- A spline height profile far above the geometry it describes is taken at the drawn height
  instead of becoming an invisible wall; `OMSI_CHECK_WHEELS=1` can report what wheels meet.

## 0.1.20 - 2026-09-29

### Doors and controls
- Automatic rear doors close again instead of having their timer constantly restarted by
  waiting passengers.
- Wall tops stay walls, route numbers can be chosen correctly, and a force-feedback wheel
  recovers properly after a pause.

## 0.1.19 - 2026-09-29

### Graphics and diagnostics
- Automatic render scale stays at full size up to 4K on Windows and Linux.
- `OMSI_CHECK_SPLINES=1` can list spline chains whose ends do not meet.

## 0.1.18 - 2026-09-29

### Game and Discord
- The clock can be moved forward or back from the game menu, including an option to put a duty
  back on time with its timetable.
- Discord Rich Presence shows the current bus, map and line.

## 0.1.17 - 2026-09-29

### AI traffic
- A junction claim expires when its car has been crawling outside the junction for too long,
  preventing chains of traffic from waiting behind a stalled claimant.

## 0.1.15 - 2026-09-29

### Driving and vehicles
- Wheels prefer the road surface over terrain cutting through it, removing invisible bumps and
  walls where roads pass through the ground.
- Bus auto-start waits longer for delayed starters and can select D with the brake held.
- Manual gearboxes can be shifted with Ctrl+Up/Ctrl+Down, with an optional automatic clutch.
- Continued situations keep the livery, mouse steering survives a look-around, glance keys
  return to the road, and the bus can be moved from the map.
- The pause menu no longer flickers its top selection, and the phone wheel follows the bus wheel
  one to one.

## 0.1.14 - 2026-09-28

### More fixes
- Esc → More → *Set the clock...*: the clock one, five, fifteen or sixty minutes on or back,
  and on a duty *On time with the timetable* (early or late by six minutes: the clock is
  put where the bus is on time).
- Discord shows "Playing openOMSI" with the bus, the map and the line (Rich Presence, over
  Discord's local connection; `discord_app_id` in the settings, `discord_status=0` turns it
  off).
- AI traffic no longer stands for minutes on a free road: a car crawling in a jam of its own
  kept its claim on the junction ahead, and the cars that give way to it waited behind it
  in a chain (one Golf stood 81 s on Spandau; none now in four minutes of 80 cars).
- On a road the wheels stand on the road, as in OMSI: the terrain over or through the
  carriageway (an embankment the road runs under, ground poking through the asphalt) was an
  invisible wall under bridges and a bump that threw the bus.
- Mod buses with a lamp test after the key (the GX7767 E500 MMC waits four seconds) start
  with Shift+U: the starter is tried for longer; an automatic gearbox that takes D only
  with the brake held (ZF, `(L.L.Brake) 0 >`) is put into D by the auto-start.
- Manual gearboxes: Ctrl+Up / Ctrl+Down shift up and down (gear levers with a trigger per
  gate, `kw_s_1`...`kw_s_10`, `kw_s_N`, `kw_s_R`, as the LiAZ MKPP), the clutch let up as
  OMSI's clutch key lets it; with *automatic clutch* on, the clutch bites by itself when
  pulling away, as far as the engine keeps its revs. *gear_up* / *gear_down* for buttons.
- A situation loaded with *Continue* keeps the bus's livery.
- Mouse steering keeps the wheel and pedals while the right button looks round, as in OMSI.
- The arrow keys' look with a wheel is a glance: held, the head turns (at most 140 degrees);
  let go, it comes back to the road.
- Esc → More → *Move the bus on the map...*: click a street on the city map and the bus is
  put there (Ctrl+click on the map does it too, now also on a duty).
- The pause menu no longer flickers its top line while the mouse moves over it.
- Phones: the on-screen wheel turns the bus's wheel one to one.
- Windows and Linux: the automatic render scale draws at full size up to 4K (it drew a 4K
  screen at 58 %, and enhanced graphics looked like low-quality textures).
- Automatic rear doors (SD202, SD200 and the like) close again: passengers walking to the
  door or standing at the back of the queue kept asking for it, and the script starts its
  closing time again on every request. A request now opens a shut door; an open one is held
  only by somebody in the doorway, as by the light barrier.
- Walls with a height profile on their top (the stone and brick walls of UK maps) are walls
  to the wheels, not a road: where a wall's top met the road the bus drove up onto it and
  along it as the road fell away.
- A spline's height profile lying well over everything the spline draws (Westcountry's
  yellow surface marking: paint 10 cm up, height profile 50 cm) is taken at the drawn
  height: it was an invisible wall across the road. `OMSI_CHECK_WHEELS=1` with `--offscreen`
  lists what the wheels meet along the driving lanes (for map makers).
- Mouse steering turns on to the full lock past the window's edge: with the cursor at the
  edge, moving the mouse on outwards keeps turning the wheel (the width of the window is a
  smaller part of the lock the faster the bus goes, as in OMSI, and at 30 km/h the edge was
  a third of it); moving back gives that turn back first.
- The free camera (F4): its keys (W A S D Q E, Space, Shift, the arrows) no longer work the
  bus as well (W switched the wipers on), and the mouse wheel zooms there and on foot
  (Ctrl+wheel moves the camera on).
- Snow is matte: it no longer takes the rain's gloss and shines like plastic in the lights.
- Modding: a mesh can be lit by up to 63 interior lamps (the extra numbers on the lines
  after the four of `[illumination_interior]`; OMSI 2 reads the first four), PBR maps up to
  4096 px, and 32 lights per 25 m of the world instead of 16. What openOMSI allows beyond
  OMSI 2 is written down in docs/MODDING.md and on the site.
- Door keys: Shift+1, Shift+2 ... are the bus's doors front to back, found from the model
  (where each door leaf sits along the bus and which leaves each door trigger moves). The
  LiAZ's Shift+1 opened its middle and rear doors together and its front door had no key;
  a mod door script that mentions a closing variable while opening had its two leaves on
  two keys (one leaf moved, the other needed its own press). The game's log lists the keys
  of each bus ("door keys: ...").
- The release notes list what changed since the release before (they said "Small changes
  and fixes" for every release without a section of its own here).
- Phones: 60 frames a second by default (the settings took the PC OMSI's limit of 30 from
  its options.cfg), and dragging the view turns it the way the finger moves (it was the
  other way round, left for right and up for down).
- A bus on a lower level (a car park under a building, a road under a bridge) is no longer
  taken for one fallen through the world and put up on the roof: it has fallen only with
  nothing under it at all. A teleport to a place with a height lands on that level.
- A map that uses objects or splines that are not installed says so when it loads (how
  many, and the add-on folders they come from), and every missing object, spline and
  texture is listed by add-on in `~/.openomsi/missing_content.txt` (written again when the
  game ends, with the tiles loaded on the way). Holes, bare roads and white objects of such a
  map are a missing download, not a fault of the game - now one can tell.
- The game's log records the whole session: the system (OS, processor, memory), the command
  line and every setting at the start; then everything said on the screen, each view, pause
  and resume, every key action and door key, and a status line every minute (frame rate and
  the worst frame, where the bus is, its speed, the view, the time, the traffic).
- The bus is no longer put down inside scenery: an object no taller than a vehicle that
  stands for a third or more where the bus is put (a mod map's static buses in its depot,
  a sign) is taken away for the session, as the object editor takes one away (the map's
  files are not changed).
- Barriers (depot and car park gates on a light program) open for the player's bus off the
  lanes too: a gate whose lane starts up to 25 m ahead, the way the bus faces, is asked for
  (in a depot yard the bus stood beside every lane and the barrier stayed down).
- Passengers: the queue at a front door no longer goes on round the bus's nose (it stops
  short of the front and turns out along the kerb - people stood across the road in front
  of the windscreen, facing the bus), and a door shut for a moment no longer sends the
  waiting people away: they wait on 25 s after a door of the standing bus was last open
  (they turned away at once and came back when it opened again).
- Camera monitors: `reflexionN.bmp` is camera N's picture wherever a vehicle's material
  names it (its light map, night map, a `[matl_item]` switched on by the script), not only
  as the plain texture - monitors that show the camera once switched on were white.
- "Doors are open" follows what the passengers are told is open (`PAX_Entry/Exit<n>_Open`)
  - mods use `door_<n>` for other things, and a bus with its doors shut said they were open;
  "Air pressure is low" is no longer said with the tanks full (the spring brake is then held
  by the bus's own parking brake).
- The rear doors of the Berlin buses (SD, NL, EN/GN) close on Shift+2: switching their
  release off with the doors open shuts them at once, rather than when the passengers'
  last request has lapsed.
- An entry point whose marker lies under the ground (nothing under its height at all) puts
  the bus on the ground above it, not in the void under the map; a real lower level (a car
  park's floor) is kept.
- Settings → Controllers → *Force feedback and vibration* (and Esc → More → Options):
  switches the wheel's forces and a pad's rumble off altogether (a pad left plugged in
  shook all the time).
- Spaces on displays and signs: a font without a space character (many display fonts have
  none) leaves the width of a narrow letter between the words; the words of a destination
  ran into one another.
- OMSI's held keyboard pedals: Settings → Controllers → *Keyboard pedals stay where they
  are* (and Esc → More → Options). Tap the brake and it keeps that pressure until the
  throttle is tapped, and the other way round.
- Settings → *Reset all settings...*: every setting back to how it came (the language, the
  drivers, the key bindings and the game folder stay), after a dialog that asks first. The
  quality presets are under Performance.
- The bus radio also plays the stations of OMSI's radio plugins (SuperRadio's `.opl` and
  its lists under `plugins`): every stream address found there is a station, after the
  ones of `~/.openomsi/radio.cfg`.
- Controls → Game controllers: a button pressed on the wheel lights its line in the list for
  a few seconds, and the status line says which button it is and what it does - press it and
  give it an action right there.
- Railway signals clear for the player's own train as well (driven on the rails): its
  signals stayed at stop, as only an AI train ever asked for them.
- Puddle splashes are a mist of water - soft, lit by the scene, widening and thinning out as
  it sinks - instead of rings of glowing light flying off the wheels.
- Enhanced graphics: the sky is drawn again after a third of the way it waited before, so
  the clouds no longer drift and jump back into place when the camera flies fast.
- Launcher: the bus list is built again only when the search, the buses or the host's list
  change (it was rebuilt every frame, every name copied - scrolling it stuttered on phones).
- 14 more interface languages: Українська, Беларуская, Қазақша, Polski, Čeština, Magyar,
  Español, Português (Brasil), Italiano, Nederlands, Türkçe, 日本語, 中文 (简体), हिन्दी -
  with English, German, French and Russian 18 in all (Settings → Language), and every text
  of the launcher and the game menus the tables lacked now translated in all of them. The
  tables are in the program, so they work on every system (the machine translation, which
  runs on Macs with Apple silicon only, is not needed for them). Chinese, Japanese and Hindi
  are drawn with the system's own fonts. OMSI's own texts (key names, descriptions) show in
  English where OMSI has no such language.
- Shift+U after a crash starts the bus again: a bus under power whose engine had died was
  taken for a running one and "switched off" round and round ("Shutting down..." for good).
  An auto-start that has gone on for 20 s is begun again by the next Shift+U.
- The weather turning to snow (Next weather, or the weather file) no longer drops the bus
  through the world: every tile is read again with the winter textures, and while the one
  under the bus is away the bus is held where it stands (it fell, was put back in the sky
  and fell again).
- Passengers in an indoor station stand on its floor, not on its roof: walking, they took
  any surface over them for a kerb to step up on. They now keep to the floor within a step
  of where they are (a station's floor under its roof, a car park's level under the deck).
- Keys the player set in `Inputs/keyboard.cfg` are theirs, also when they edited the
  installation's own file: Z / X / C (the indicators), Shift+number (the doors), W A S D and
  the arrows no longer take over a key bound to something else. What counts as changed is
  told from OMSI 2's own assignment, built into the game, with Shift held as well.
- The hazard lights go off again (X, and the phone's hazard button): pressed with them on,
  the key let go of the indicator lever instead of their own switch.
- Seated passengers on a high seat (on a podium, over a wheel arch) let their feet hang as a
  sitting body does, instead of stretching the legs straight down through the seat's front
  to the floor far below.
- The bus no longer spawns floating on a wall's top: the place it is put down at is the face
  its wheels stand on near the entry point's height (a road, a deck, an underground floor),
  not the highest surface of the map's height raster there, which is a wall's top beside a
  pavement (London) or a deck over the road.
- The sound follows the system's output device: a Bluetooth headset or headphones connected
  while the game runs take the sound over, and disconnected, the sound comes back on the
  speakers (it had stayed on the speakers, or stopped for good).
- Maps whose `[map]` list names a tile twice (Westcountry 3 names 33 tiles twice): the tile
  numbers the map's files use count those entries, as in OMSI. Counted without them, every
  number after the first repeat named the wrong tile: rows of objects repeated along a road
  (fences, bollards, lamps) hung from another tile's row and stood across the road or were
  missing (3 of 1536 rows found their start on Westcountry 3, now 165, 159 of them where the
  map says), timetable tracks ran over the wrong tiles, and entry points were looked for on
  the wrong tile.
- An entry point is found on its own tile: a map joined from two (two towns you cannot drive
  between) repeats object ids, and choosing a stop in one town put the bus on the grass of
  the other, where the other object of that id stands.
- Road markings laid over road markings (where lines cross, a box junction over a lane's
  arrows) are no step for the wheels: every layer of paint is looked through, not only the
  first.
- Traffic of the UK car packs (WH UK AI: Westcountry, London and others) is no longer
  invisible, only shadows and lamps driving about: a mesh written before a model's first
  `[LOD]` belongs to that level, as in OMSI. These cars put their shadow there; as a level
  of its own it was all a moving car had.
- Their paint: a `[matl_transmap]` picture without an alpha channel is opaque, as Direct3D
  reads it (the cars' paint layer has a black 24-bit `transmap_null.tga` and was invisible),
  and a layer drawn over another mesh of the same shape keeps its blending (the baked
  shading over the paint had been made opaque: black cars, black roofs).
- An object's `[LOD]` level is chosen as OMSI does: the first level in the model's order
  whose size the object reaches, else the last. The stock Sv signals list their detailed
  level before their low one; sorted by size, the low one stood in close up and the signal
  vanished in the distance. A model with a single `[LOD]` is drawn at any size.
- A car that has reached a dead end goes after 25 seconds, even in view, when others are
  waiting behind it: a fire engine at the end of a dead-end street held a queue of fourteen
  cars for two and a half minutes, and the junctions before it jammed full (Westcountry 3,
  38 cars stuck for over a minute in five minutes of traffic, now none).
- No more sky showing through the road in stars and stripes at junctions: a spline made
  only of blended layers (Westcountry's lane darkeners laid over the junctions' painted
  ground) no longer cuts the ground away under itself; the ground is what it darkens.
- `OMSI_DEBUG_LAMPS=1` lists every traffic light object, the crossing it belongs to and
  those that name none (and so stay dark).
- Esc → Destination display → *Route number*: the route (line) number on the displays, from
  the depot file's routes and the map's timetable; the destination stays.
- Windows: a force feedback wheel (G29) no longer pulls itself to the middle after the pause
  (taken back by the game, its own centring spring came back on).
- `OMSI_CHECK_SPLINES=1` with `--offscreen` lists the map's spline chains whose ends do not
  meet (for map makers).

### Controllers
- No hidden dead zone on wheels any more: gilrs's default filters took 10 % of every axis (90
  degrees either side on a wheel of 1800) and held back small movements; Windows: the
  driver's own DirectInput dead zone and saturation are cleared, as OMSI does (PXN V99).
- macOS: a device with sliders or the simulation page's axes is read as a wheel from its HID
  elements even when SDL's list calls it a gamepad (HORI Truck Control System: accelerator
  and brake stayed merged and the steering had a gamepad's dead zone).
- The Controllers page offers the view actions for buttons: *view_look_left/right/up/down*
  (look round while held), the interior cameras, the views. OMSI's view actions on buttons
  work in the game.
- Force feedback in every view of the bus, not only the driver's.
- The mouse no longer freezes the picture: a gaming mouse's thousands of moves a second each
  looked for the switch under the cursor, and no frame was drawn while the mouse moved.
- Mouse steering shows a cross as the cursor, as in OMSI.
- Phones: the on-screen wheel turns one and a half turns to the lock, as a bus's does, and
  comes back by itself when let go.

### Game
- P pauses into the pause menu in a LAN session too (the session goes on for the others).
- The launcher starts the game that came with it, not a path remembered from an older
  installation (on macOS it kept starting a build of the days before the rename).
- Esc → More → *Depot file (HOF)...*: choose the bus's depot file by hand. Placing a vehicle
  asks for its livery and depot file.
- A bus that could not be loaded is tried once more, and the reason is shown on the screen
  (it started on foot without a word).

### World
- Parked cars have their paint (the paint scheme's pictures were looked for in the wrong
  folder and the cars stood white) and lean with an inclined street.
- A street running through an object's `[boundingbox]` (a bridge, a gantry, a hall) makes
  that box no wall: mod maps' invisible walls across the road.
- The automatic rear door closes: the passengers' request button was never let go, so the
  stop request stayed on. A passenger who cannot get in stops asking after a while.
- Passengers turn their heads about the middle of the neck (aXYZ man02's neck point lies at
  the back of his neck: the head swung off the collar).

### Crashes
- "RenderBundleEncoder::finish: Validation Error" after the card ran out of memory no longer
  ends the game: the part of the picture is left out.

## 0.1.13 - 2026-09-28

### Menu
- P opens the pause menu and resumes the game from it.

## 0.1.12 - 2026-09-28

### Driving and physics
- A bus spawned inside an obstacle can move out of it instead of being permanently stuck.
- Road paint close above another road face no longer lifts the wheels.
- With a steering wheel, the arrow keys look around as in OMSI.
- Ctrl+Alt+arrow turns the mirror being viewed, with the adjustment saved per bus in
  `mirrors.cfg`.

## 0.1.10 - 2026-09-28

### Performance and crashes
- Big mod maps (Grande Porto, Novi Sad) no longer freeze for up to two seconds while
  driving: the night copies of object textures (`night\` folder) were decoded on the
  thread that draws. They are now read with the rest of a tile in the background; the
  slowest object upload on Novi Sad went from 1760 ms to 24 ms.
- Fixed the crash "Error in Buffer::get_mapped_range: Validation Error" (Windows): the
  vertex updates of a frame no longer go through one staging buffer that could outgrow the
  graphics card's buffer limit.

### Driving and physics
- Steering no longer eats the engine's power: both front wheels turned by the same angle
  and the tyres fought each other, so at 60 % steering a bus barely moved and at full lock
  not at all. The inner wheel now turns further than the outer one (Ackermann) - buses
  and cars take tight corners at the speed you give them.
- Modded maps: the bus no longer hops over invisible things. Only solid objects (`[fixed]`,
  not `[nocollision]`) give the wheels a step to climb; the low collision meshes of helper
  and sensor objects did too.
- Trains stay on their track: a track under a bridge counted as a "neighbouring lane", and
  a train changed lanes down through the viaduct.
- The automatic rear door (MAN SD200, NL202/EN92) closes after the passengers are out:
  one passenger held up on the way out kept the stop request on for good.

### Vehicles and mods
- Add-ons that name their meshes from another folder than their model (Studio Polygon's
  `Configuration Files`, packs that borrow from the vehicle folder or the game folder) find
  them.
- The side mirrors show the bus's own flanks, as in OMSI (the outside-only meshes are drawn
  in the mirrors from the cab).
- Esc → *Destination display...*: choose any destination of the bus's depot file by hand
  (roller blinds, matrix displays, custom blinds).

### Controllers (macOS)
- Wheels and pedals are read from their HID elements: two axes of the same kind stay two
  (HORI Truck Control System: the brake pedal moved the accelerator), a 16-bit wheel uses
  its whole range (no dead zone of a quarter turn), and the simulation page's steering,
  clutch, accelerator and brake are read on wheels that use it.
- Windows: the hat switches (D-pads of wheel rims, Moza among them) can be given keys like
  buttons (*Hat 1 up* ... on the Controllers page).

### Settings
- *Throttle pedal strength* / *Brake pedal strength*: a softer or stronger response of the
  analog pedals (launcher, and Esc → Options).
- *Seat position*: the driver's eye forward/back, up/down, left/right, with *Reset the seat
  position* (launcher, and Esc → Options).
- *Camera collisions*: the outside camera no longer jumps in when something passes behind
  it (it is pulled in over a tenth of a second); switched off, it goes through everything,
  as in OMSI.
- The field of view applies to the free camera and the view on foot too.

### More from the players
- A bus put down inside an obstacle (a shelter's or a depot's collision box - GPM) is no
  longer held there: what it spawned in is left alone until it has driven out of it.
- Road markings are paint, not steps: a road face within 4.5 cm over another one (markings
  made as `[surface]` objects or as splines with a height profile - Horizon) no longer lifts
  the wheels, so the bus stops hopping over lines at stops and roundabouts. Kerbs stay kerbs.
- With a steering wheel the arrow keys look around again, as in OMSI (a G29's buttons set to
  the arrow keys turned the view there; here they steered).
- Mirrors can be turned: Ctrl+Alt+arrows in the cab turn the mirror you look at, kept per bus
  in `~/.openomsi/mirrors.cfg`.

### Passengers
- The aXYZ man in the grey jacket no longer looks as if his neck were broken: a head turns
  about a point under its middle, not about the `[links]` neck point at the back of the
  neck, which swung the head off the collar whenever he looked to the side.

### Head tracking and time
- Head tracking: Settings → *Head tracking* takes the head's pose from opentrack's
  "UDP over network" output (port 4242) - TrackIR, Tobii, webcams and phones through
  opentrack. `head_tracking_invert=yaw,pitch,roll` in `settings.cfg` turns an axis round.
- The clock can be changed gradually: hold Ctrl+Shift+Page Up / Page Down (faster the longer
  it is held), or Esc → More → *Clock +10 minutes* / *-10 minutes*.

### Interface
- P pauses into the pause menu (P or *Resume* go on); it was only a line of text before.
- The pause menu shows the everyday lines first (*Resume, Options, Line and tour,
  Destination display, City map, Timetable, Save, ...*); the rest is under *More...*.
  The mouse wheel scrolls the menu instead of moving the highlight, and only the line under
  the mouse is lit.
- The timetable shows departure times (a stop with a wait shows both), the trip number of
  the tour and the next trip.
- Android: dropdowns no longer close (or pick something) the moment they open.

### Lua plugins
- `omsi.info()` (map, clock, view, speed, delay, line, tour, trip, next stop, ...),
  `omsi.command(name)` (refuel, wash, repair, screenshot, save, weather, clock ...),
  `omsi.vars()`, `omsi.clock()`, `omsi.speed()`, `omsi.distance(x, y)`, and the events
  `key`, `next_stop`, `view` and `duty`. See [Plugins](docs/PLUGINS.md).

### Website
- A link to the Discord server on the website and in the README.
- The website shows the releases (with their notes and downloads) and the issues (open and
  closed, searchable, with their discussion) on pages of its own.

### AI traffic
- A car already in a crossing on its green no longer stops again at a light of a path it
  joins inside that crossing (the cross traffic's red, an invisible stop line mid-turn).
- Depot buses wear the repaint of their fleet number when the bus's `[number]` lists name
  one (a `.org` file per repaint); they were painted at random.
- The AI vehicles are heard round the camera: a free camera following an AI bus lost its
  sound 250 m from the player's bus.

## 0.1.9 - 2026-09-28

### Graphics cards and crashes
- A graphics validation error no longer ends the game: it is written to the log and the game
  goes on.
- "The graphics device was lost" (RTX 4060 reports, a few seconds into big maps) is the
  Vulkan driver giving up. On Windows the game can now draw with DirectX 12 instead:
  Settings → *Graphics API*, or *Use DirectX 12* in the launcher after such a crash.
- Graphics cards without Vulkan (GeForce GT 530 and other older ones) run the game: it asks
  Vulkan, then DirectX 12 (Windows), then OpenGL, and takes the first that draws. Settings →
  *Graphics API* chooses one; the Windows download brings DirectX 12's shader compiler.
- Phones where every mesh came out flat and far away (the bus in the launcher too): the
  shaders no longer read the objects' matrices in the way some phone GPUs get wrong.
- Cards below the usual limits get a smaller shadow map, and textures larger than the card
  takes are scaled down instead of stopping the game.
- When a game ends on an error, the launcher says so, with *Copy report*, *Report on
  GitHub* and, after a lost graphics device on Windows, *Use DirectX 12*.

### Collisions
- Only the objects OMSI makes solid stop the bus (`[fixed]` ones and poles). Signs on stop
  poles, gantries and the bridges of mod maps not marked so were invisible walls - under the
  bridges of Saint Servant, for example.
- Collisions with objects can be switched off, as in OMSI: Settings → *Collisions with
  objects*, or Esc → Options in the game (OMSI's own `no_collision` setting is taken over).

### Steering, pedals and controllers
- Mouse steering works in the outside and passenger views too, and the wheel follows the
  cursor smoothly: it crept on by itself and came back in steps.
- New settings, off by default: *Steering linearity* (the steering keys turn the wheel at
  OMSI's steady pace) and *Old Steering* (the wheel stays where you leave it, as in OMSI 2 -
  turn it back yourself).
- The clutch key works as in OMSI: the pedal goes down at once and comes up slowly.
- Settings → *Wheel rotation* and *Full lock at*: a wheel of 900° can steer like a real bus
  (the full lock at, say, 540° of the wheel), *Reset wheel settings* goes back to OMSI's
  (the whole wheel is the full lock). *Invert force feedback* for wheels that push the wrong
  way (G29).
- Settings → *Field of view* for the views from the bus (Default: the bus's own cameras);
  the mouse wheel, = and - still zoom as in OMSI.
- A steering wheel listed twice (Logitech G29: once as a wheel, once as a gamepad) is listed
  once, and *Use this device* on the Controllers page switches any device off.

### Multiplayer
- A joining game plays on the host's map when it is installed, whichever map was chosen
  before joining. On big add-on maps it often stayed on its own map - where nobody met it:
  - the host lists its mods for the joining players after it starts, which takes a while on
    a big map, and the joining game gave up waiting for that list after 25 s;
  - a host busy loading a heavy area answered later than the 3 s the joining game waited;
  - a dedicated server answered nobody until it had loaded its whole map.
- The host answers joining players while it loads its world, and a joining game stays in
  the session while its own map loads.
- A joining game with everything the host uses installed no longer needs 3 GB of free disk.
- Mods installed into openOMSI's content folder inside the OMSI 2 folder are passed on to
  joining players (they were taken for OMSI's own files).
- A player's info (bus, destination, display texts) always fits one datagram: long paths and
  texts made it too big to arrive, and the others never saw which bus the player drove.

### Maps and vehicles
- Parked cars, people and objects whose `Texture` or `model` folder is spelt with another
  case are no longer white or missing on Linux (TH_Zafira and others).
- The warning lamps' glass of the Thüringer Wald buses (S 315 UL, S 317 UL, O 550) and the
  MB O 407 is see-through again instead of a row of white tiles ("glas" is glass too).
- Free roam (no duty): people only board a bus that shows a destination, not one showing
  nothing or "not in service".
- Drive → *Depot file*: the depot file (HOF) can be chosen by hand; *Automatic* follows the
  map and the date.

### Sound
- A limiter on the whole mix: many loud sounds at once no longer clip (heard as squeaks and
  crackles).

### Launcher
- Icons missing on some phones: the launcher's picture atlas grows when a screen needs more
  room than it had (high-resolution phones).
- The README has an installation guide and what to do when something goes wrong.

## 0.1.8 - 2026-09-28

### Controls
- Changing a key on the Controls page now takes effect: the page says which driving keys are
  in use, and changing any key switches *Driving keys* to *Custom controls* by itself (before,
  with W A S D the edited keys were silently ignored).
- Keys you bind yourself now beat the ready-made layouts: with W A S D chosen, a D you gave
  to the gearbox is the gearbox, not "steer right". The layouts' extra keys (Z/X/C for the
  indicators, I for the saloon lights) no longer apply with Custom controls (C is OMSI's
  "look ahead" there again).
- **Space** looks ahead again (OMSI's `view_reset_all_directions`); it was swallowed by the
  W A S D layout. Every view keeps its own direction: turning the outside camera (F3) no
  longer turns the driver's head (F1).
- Zoom inside the bus: the mouse wheel, **=** / **-** and a pinch narrow the view in the
  driver's and passenger views, as in OMSI.
- Mouse steering follows Omsi.exe exactly, now including its pedals (throttle from the middle
  of the window to the top edge, brake to the bottom edge, no dead zone). Settings → *Mouse
  steering* makes it more or less sensitive (100 % = OMSI).

### Wheels, pedals, joysticks
- On Windows the game controllers are read through DirectInput, as OMSI reads them: every
  device Windows lists as a game controller (wheels with their makers' drivers included), up
  to 128 buttons, and force feedback on wheels - the centring that grows with the speed, the
  heavy steering of a bus standing still, and the scripts' shaking.
- Buttons are numbered as DirectInput numbers them (they were counted in the order they were
  first pressed), and every button of a device can be given a key: the list stopped at the
  ten that fitted on the page (T16000M).
- *Set up step by step*: turn the wheel to the left, press each pedal - the axes, their
  direction and combined pedals are found by themselves. A connected device nobody has set up
  yet steers with its X axis and says where to set it up.

### Multiplayer
- Hosting no longer stops working after a while: the router's port forwarding was asked for
  two hours and never renewed, and the rendezvous relay was asked every second and refused the
  host after an hour or two. The forwarding is renewed every 20 minutes, the relay is asked
  every few seconds with a growing pause after a refusal, and a Cloudflare tunnel that ends is
  started again.

### Launcher
- Phones: the launcher is drawn at least at the system's text size, the settings stand in one
  column, a finger on a list at its end scrolls the page on, page titles no longer run under
  the tabs.
- The OMSI 2 folder is found when openOMSI was unpacked into it (openOMSI keeps its own content
  in an `openOMSI` folder there), when the path is pasted with quotes, or when `Omsi.exe` or a
  folder inside the game is chosen; a folder that is not a complete OMSI 2 is reported with
  what it lacks.
- Timetable: changes stay while you move between lines and are saved together (*Save all*);
  *New line*; *Repeat* makes a whole day of tours (every *n* minutes up to a last departure).
- Settings → Graphics → *Reflection maps* switches the materials' reflections
  (`[matl_envmap]`) off.

### Sound
- Distance as in OMSI: full volume up to the `[3d]` reference distance, then falling as 1/d
  (DirectSound's law). It fell much faster, so most sounds were far too quiet.
- The bus's own sounds are no longer muffled in the cab unless they are other vehicles':
  interior sounds such as the indicator relay were cut to a quarter and dulled, and only came
  through with a door or window open.
- Footsteps outside are heard through the bodywork from the cab (and the saloon's from the
  street); people in the street sounded as if they walked inside the bus.

### Maps and vehicles
- Matrix displays drawn by scripts (script textures as the LED mask, `\S:n`, e.g. churaPixel/
  Krüger++ matrices) show their dots instead of a fully lit panel: a `[matl_change]` ahead of
  the slot's `[matl]` made it opaque.
- Objects put on a road spline (`[splineAttachement]`), such as an entry point or a stop, are
  found by their id: a Novi Sad start point was "not in the map".
- An entry point whose object comes out on another level than the map recorded (under a
  bridge) starts the bus at the recorded height.

### Game
- The depot file (HOF) follows the date as the map's chrono says: Berlin in 1994 has line 137
  where 1986 had 92 - on every map with chrono depot changes.
- Phone: the pause menu scrolls with the finger; a finger put down to scroll no longer picks
  the line under it.
- With V-sync one frame waits for the screen instead of two: less input delay.

### Builds
- New downloads: Windows ARM64, macOS Intel, Linux ARM64, and the dedicated server for Windows
  (x64, ARM64) and Linux ARM64. The launcher updates itself on all of them.
- The release notes on GitHub list what changed (this changelog) instead of a link to the
  code changes.

## 0.1.7 - 2026-09-28

### Driving physics as the .bus file makes it
- The bus now follows its steering the way OMSI's own physics does: it turns exactly as far
  as its wheels point and only slides when a bend asks more grip than the road has. Before,
  every bus turned at 70 % of what its steering asked, 0.8 s late and drifting sideways -
  the "boat" feeling, and the same for every bus.
- Springs, dampers and their limits act where OMSI takes them (`achse_feder`,
  `achse_daempfer`, `achse_maxforce`, `achse_minwidth`/`achse_maxwidth`), every axle steers
  towards `[rot_pnt_long]`, and body pitch and roll are damped as in OMSI. Each bus feels as
  its author made it: stiffer springs, stronger dampers, a higher centre of gravity all show.
- `cargo run --release -p omsi-sim --example handling -- <file.bus>` prints how a bus
  handles (yaw response, side slip, body roll and how fast it settles).

### Steering
- Mouse steering (O) as in OMSI: the whole window width is the full lock, and above 10 km/h
  the same hand movement turns the wheel less and less (at 50 km/h a fifth as far). No more
  jumps when the cursor passes the middle.
- Phone: the on-screen wheel turns with the finger round it (a third of a turn is the full
  lock). It used to stop at about half a turn and spring back.

### Graphics and world
- Mirrors follow the bus's pitch and roll, use the camera distance from the `.bus` file, and
  only the mirrors in view are redrawn (the radius of `[add_camera_reflexion_2]`).
- The bus's own screens (IBIS, matrix displays, dashboard LCDs) are sharp again in Enhanced
  graphics: FXAA and the glow no longer blur them.
- Trees have the width the map gives them: slim trees such as firs were drawn up to six
  times too wide.
- Map tiles of older editor versions are read correctly (object tilt and strings).

### Updates
- The launcher updates itself from the GitHub releases: when a newer version is out it asks
  at the start, downloads it (checked against GitHub's SHA-256), replaces the program and
  starts again. Mods, content and settings stay as they are.
- On Android the system's installer is used: Update replaces the app and starts it again,
  Cancel leaves it as it was.
- Settings → Updates: look for updates at the start, install without asking, Check now.

### Fixes
- Android: vibration of the on-screen buttons works (the calls never reached the app).
- Android: `openOMSI/env.txt` takes the `OMSI_*` switches a computer takes from its
  environment (for looking into problems).

## 0.1.6 - 2026-09-27
- Phone: the on-screen wheel is drawn cleanly; calmer steering, tilt steering reaches the
  full lock at 45°.
- A version built again updates its release instead of failing.

## 0.1.5 - 2026-09-27
- Lua plugins (`plugins/<name>.lua` or `plugins/<name>/main.lua`) on every platform, next to
  the original DLL plugins: bus variables and triggers, events, timers, saved data, hot
  reload, sandboxed. See [docs/PLUGINS.md](docs/PLUGINS.md).

## 0.1.4 - 2026-09-27
- Android: the launcher and the game on phones and tablets, with on-screen driving controls
  (wheel or tilt, pedals, gearbox, doors, indicators, cab panel, cameras). See
  [docs/ANDROID.md](docs/ANDROID.md).

## 0.1.3 - 2026-09-27
- Small changes.

## 0.1.2 - 2026-09-27
- Website and repository improvements and fixes.

## 0.1.1 - 2026-09-27
- Website and repository improvements and fixes.

## 0.1.0 - 2026-09-27
- First public release as openOMSI: a from-scratch recreation of OMSI 2 in Rust that runs
  every map and mod (an original OMSI 2 is needed). Builds for Windows, macOS and Linux and
  a dedicated server, released automatically on every push.
