# Plugin API reference

Every function and event a plugin can use, grouped by namespace. It is for plugin authors; start with the [plugin guide](PLUGINS.md) if you have not written a plugin yet.

The same list is in [`plugin-api.json`](plugin-api.json), for editors, SDKs and other tools. Both come from the game's API registry, so a function exists here exactly when the game has it.

## Reading this reference

Each function shows its call, what it returns, what it does, its parameters, the openOMSI version that added it, and the permission it needs (if any). `[name]` is an optional argument.

```lua
local x, y, z, heading = omsi.position()   -- several results are several values
local ok, why = omsi.weather.set({ temperature = -5 })  -- a change that can fail
```

These rules hold for every function:

- **No data means `nil`.** With no bus, on foot, without a duty, or when a bus lacks a variable, readings are `nil` (or nothing), lists are empty, and changes return `false`. A plugin works on every map and with every bus.
- **Several results** come as several values. When there is nothing, none come back: `if omsi.position() then ... end`.
- **Changes that can fail** return `true`, or `false` and the reason.
- **A wrong argument is an error** that names the function and the argument: `omsi.var: argument 1 (name) must be a string, not table`. Catch it with `pcall` where it can happen.
- **Units:** metres; map coordinates x east, y north, z up; headings in degrees clockwise from north; speeds in km/h; times in seconds (clock times: seconds since midnight).
- **Lists start at 1**, except the arguments and `index` fields of the game data getters and the HOF, which keep OMSI's zero-based numbering.
- **Newer functions** come with newer openOMSI versions. `omsi.game.has("weather.set")` tells whether the running game has one.

### Permissions

A plain `.lua` plugin has every permission. Starting a native helper (`process`) also always needs the player's approval. A compiled `.oop` plugin has only the permissions it declares; calling a function that needs another one raises an error the plugin can catch. Reading the game needs no permission. See [Permissions and sandbox](PLUGINS.md#permissions-and-sandbox).

| Permission | Allows |
| --- | --- |
| `ui` | panels, messages, notifications, cables, screenshots |
| `storage` | `omsi.storage` and `omsi.files` |
| `vehicle_write` | driving the player's bus: variables, triggers, doors, IBIS |
| `traffic_write` | changing AI traffic, people and other vehicles |
| `world_write` | time, weather, duty, teleport, game commands |
| `camera` | moving the camera |
| `audio` | playing sounds |
| `network_local` | `omsi.send` (UDP to this computer) |
| `network_remote` | `omsi.ws` (read-only feeds to hosts the player allows) |
| `lan` | sending in a LAN session |
| `process` | starting native helper programs |

### Lua-only parts

These are part of the Lua binding, not of the registry:

| Name | What it is |
| --- | --- |
| `omsi.data` | a table saved between sessions (see [Saved data](PLUGINS.md#saved-data)) |
| `omsi.save()` | writes `omsi.data` now |
| `omsi.version` | the game's version string |
| `omsi.name` | the plugin's name |
| `omsi.ui.version` | the panel feature level, currently `8` |
| `print(...)` | the same as `omsi.log(...)` |
| `function on_<event>(...)` | a global function is a handler of that event |

### Other languages

A WebAssembly plugin calls the same functions by the same names, without `omsi.`: `openomsi.call("ui.set", ...)` with the arguments and the result as JSON. The Rust SDK of the [openOMSI Development Tools](https://github.com/openOMSI-org/openOMSI-Development-Tools) wraps every function in a typed call. Events, timers and watches work the same way.

## Player's bus

`omsi.bus.*` and the core functions `omsi.var`, `omsi.position`, `omsi.trigger` and friends read and drive the vehicle the player is in. Every reading is `nil` (or nothing) while the player is on foot. Script variable names are the ones the bus's `.osc` files use; `omsi.vars()` lists them.

### omsi.body_frame

`omsi.body_frame([section])` → x, y, z, rx, ry, rz, fx, fy, fz, ux, uy, uz

The bus's body now, as the last simulation step left it: its origin (map metres) and its model's axes in the world - right, forward and up, each a unit vector - with the heading, the climb and the lean on the springs in them. A point of the model at (mx, my, mz) is at origin + mx·right + my·forward + mz·up. `section` 1, 2, ... is a section coupled behind (an articulated bus's rear one). Nothing on foot.

Parameters: `section` integer, optional.

*Since 0.2.22.*

### omsi.bus.acceleration

`omsi.bus.acceleration()` → across, along, up

Its acceleration in its own frame, m/s² without gravity: to the right, forwards, up.

*Since 0.2.22.*

### omsi.bus.action

`omsi.bus.action(name, [down])` → boolean

A key action of the vehicles (`[vehicles]` of keyboard.cfg: `horn`, `parking_brake_toggle`, `kw_scheinwerfer_toggle`, `blinker_left_set`, `ticket_give`, ...): pressed and let go, or held (`down` true) and let go (`false`). `true` when the bus knows it.

Parameters: `name` string; `down` bool, optional.

*Since 0.2.22. Permission: `vehicle_write`.*

```lua
omsi.bus.action("parking_brake_toggle")
```

### omsi.bus.controls

`omsi.bus.controls()` → throttle, brake, clutch, steering

What the bus drives with now: the pedals (0 to 1) and the steering (-1 left to 1 right), from the keys, the mouse or a controller.

*Since 0.2.22.*

### omsi.bus.cruise

`omsi.bus.cruise()` → number or nil

The speed the cruise control holds, km/h (`nil` while it is off). The player switches it with Num0.

*Since 0.3.55.*

### omsi.bus.damage

`omsi.bus.damage()` → table or nil

`{crashes, last_impact_kj, repair_minutes}`: the bus's crashes, the energy of the last one and how long a repair would take (`nil`: nothing to repair).

*Since 0.2.22.*

### omsi.bus.destination

`omsi.bus.destination()` → name, index

The destination the bus shows and its index in `destinations` (nothing when none).

*Since 0.2.22.*

### omsi.bus.destinations

`omsi.bus.destinations()` → list of tables

The destinations of the bus's depot file: `{index, code, name, all_exit}` (`index` from 1, for `set_destination`).

*Since 0.2.22.*

```lua
for _, d in ipairs(omsi.bus.destinations()) do
  if d.name == "Hauptbahnhof" then omsi.bus.set_destination(d.index) end
end
```

### omsi.bus.dirt

`omsi.bus.dirt()` → number or nil

How dirty the bus is, 0 (clean) to 1.

*Since 0.2.22.*

### omsi.bus.door

`omsi.bus.door(n)` → boolean or nil

Whether door leaf `n` (from 1) is open.

Parameters: `n` integer.

*Since 0.2.22.*

### omsi.bus.door_count

`omsi.bus.door_count()` → integer

The doorways the door keys work, front to back.

*Since 0.2.22.*

### omsi.bus.doors

`omsi.bus.doors()` → list of numbers

Each door leaf's position, front to back: 0 shut, 1 open (the scripts' `door_0`, `door_1`, ...).

*Since 0.2.22.*

### omsi.bus.doors_open

`omsi.bus.doors_open()` → boolean or nil

Whether any door is open (by the passengers' door flags where the bus has them).

*Since 0.2.22.*

### omsi.bus.electrics

`omsi.bus.electrics()` → boolean or nil

Whether the bus's electrics are on (the main switch).

*Since 0.2.22.*

### omsi.bus.engine

`omsi.bus.engine()` → running, rpm, electrics

The engine: whether it runs, its rpm (where the bus shows one) and whether the electrics are on.

*Since 0.2.22.*

### omsi.bus.engine_running

`omsi.bus.engine_running()` → boolean or nil

Whether the engine runs.

*Since 0.2.22.*

### omsi.bus.file

`omsi.bus.file()` → string or nil

The bus's `.bus` file, relative to the game folder.

*Since 0.2.22.*

### omsi.bus.fuel

`omsi.bus.fuel()` → number or nil

The fuel in the tank, litres (the scripts' `engine_tank_content`).

*Since 0.2.22.*

### omsi.bus.gear

`omsi.bus.gear()` → integer or nil

The gear engaged (-1 reverse, 0 neutral), where the bus shows one.

*Since 0.2.22.*

### omsi.bus.get_strings

`omsi.bus.get_strings([names])` → table

The same for string variables.

Parameters: `names` table, optional.

*Since 0.2.22.*

### omsi.bus.get_vars

`omsi.bus.get_vars([names])` → table

Many script variables at once: a table name -> value of the names given, or of every variable of the bus without a list.

Parameters: `names` table, optional.

*Since 0.2.22.*

```lua
local v = omsi.bus.get_vars({ "Velocity", "throttle" })
print(v.Velocity, v.throttle)
```

### omsi.bus.handbrake

`omsi.bus.handbrake()` → boolean or nil

Whether the parking brake is on.

*Since 0.2.22.*

### omsi.bus.headlights

`omsi.bus.headlights()` → integer or nil

The headlights: 0 off, 1 side lights, 2 dipped, 3 high beam.

*Since 0.2.22.*

### omsi.bus.horn

`omsi.bus.horn()` → boolean or nil

Whether the horn sounds.

*Since 0.2.22.*

### omsi.bus.indicator

`omsi.bus.indicator()` → string or nil

The indicators: `"off"`, `"left"`, `"right"` or `"hazard"`.

*Since 0.2.22.*

### omsi.bus.interior_light

`omsi.bus.interior_light()` → number or nil

The passenger room's light, 0 (off) to 1.

*Since 0.2.22.*

### omsi.bus.km_today

`omsi.bus.km_today()` → number or nil

Kilometres driven this session (as the personnel file counts them).

*Since 0.2.22.*

### omsi.bus.kneeling

`omsi.bus.kneeling()` → boolean or nil

Whether the bus kneels.

*Since 0.2.22.*

### omsi.bus.mass

`omsi.bus.mass()` → number or nil

The bus's mass in kg. With **Passengers add weight to the bus** (`passenger_mass`) on, the weight
of the riders in its front section is included (see [Passenger weight](DRIVING.md#passenger-weight)).

*Since 0.2.22.*

### omsi.bus.number

`omsi.bus.number()` → string or nil

The bus's fleet number.

*Since 0.2.22.*

### omsi.bus.odometer

`omsi.bus.odometer()` → number or nil

The bus's odometer in km.

*Since 0.2.22.*

### omsi.bus.orientation

`omsi.bus.orientation()` → heading, pitch, bank

Heading (degrees clockwise from north), pitch (nose up positive) and bank (right side down positive).

*Since 0.2.22.*

### omsi.bus.passengers

`omsi.bus.passengers()` → total, seated, standing

The people aboard the bus: all, sitting, standing.

*Since 0.2.22.*

```lua
local all, seated, standing = omsi.bus.passengers()
```

### omsi.bus.play_sound

`omsi.bus.play_sound(event)` → boolean

Plays the bus's own sound of this event (a trigger name of its sound files, `ev_...`).

Parameters: `event` string.

*Since 0.2.22. Permission: `vehicle_write`.*

### omsi.bus.retarder

`omsi.bus.retarder()` → number or nil

The retarder's step, where the bus has one.

*Since 0.2.22.*

### omsi.bus.rpm

`omsi.bus.rpm()` → number or nil

The engine's revolutions per minute.

*Since 0.2.22.*

### omsi.bus.sales

`omsi.bus.sales()` → tickets, money

Tickets sold this session and the money taken (the game knows no currency).

*Since 0.2.22.*

### omsi.bus.set_cruise

`omsi.bus.set_cruise([kmh])` → boolean

Switches the cruise control on to hold `kmh` (5 km/h or more), or off (`nil`). Like Num0, it is refused (false) while the bus goes slower than 5 km/h. It works the throttle and the brake the bus's scripts read, which a plugin's `set_var("Throttle")` cannot: the player's pedals are written over it every step. The brake switches it off; the throttle pressed further wins while held.

*Since 0.3.55. Permission: `vehicle_write`.*

### omsi.bus.set_destination

`omsi.bus.set_destination(index)` → boolean

Shows destination `index` (from 1, as `destinations` lists them).

Parameters: `index` integer.

*Since 0.2.22. Permission: `vehicle_write`.*

### omsi.bus.set_indicator

`omsi.bus.set_indicator(state)` → boolean

Sets the indicators: `"off"`, `"left"`, `"right"` or `"hazard"`.

Parameters: `state` string.

*Since 0.2.22. Permission: `vehicle_write`.*

```lua
omsi.bus.set_indicator("left")
```

### omsi.bus.set_interior_light

`omsi.bus.set_interior_light(on)` → boolean

Switches the passenger room's light on or off.

Parameters: `on` bool.

*Since 0.2.22. Permission: `vehicle_write`.*

### omsi.bus.set_line

`omsi.bus.set_line(line)` → boolean

Types a line (route) into the bus's IBIS, as the player would.

Parameters: `line` string.

*Since 0.2.22. Permission: `vehicle_write`.*

### omsi.bus.set_vars

`omsi.bus.set_vars(values)` → integer

Sets many script variables at once (a table name -> number); how many the bus has.

Parameters: `values` table.

*Since 0.2.22. Permission: `vehicle_write`.*

### omsi.bus.shift

`omsi.bus.shift(gear)` → boolean

Puts a manual gearbox's lever in a gear (-1 reverse, 0 neutral).

Parameters: `gear` integer.

*Since 0.2.22. Permission: `vehicle_write`.*

### omsi.bus.sound_horn

`omsi.bus.sound_horn(down)` → boolean

Holds the horn (`true`) or lets it go.

Parameters: `down` bool.

*Since 0.2.22. Permission: `vehicle_write`.*

### omsi.bus.start_up

`omsi.bus.start_up()` → string or nil

Starts the bus up the way Shift+U does (the battery, the electrics, the engine) - or shuts a running bus down; what the game says it does.

*Since 0.2.22. Permission: `vehicle_write`.*

### omsi.bus.state

`omsi.bus.state()` → table or nil

Everything a dashboard shows, in one table: `speed` (km/h, signed), `gear`, `engine` (running), `rpm`, `electrics`, `doors_open`, `indicator`, `headlights`, `dirt`, `passengers`, `odometer`, `km_today`, `throttle`, `brake`, `clutch`, `steering`, `fuel`, `handbrake`, `horn`, `stop_request` (a key is `nil` where the bus does not have it).

*Since 0.2.22.*

```lua
local s = omsi.bus.state()
if s then print(s.speed, s.gear, s.doors_open) end
```

### omsi.bus.steering_angle

`omsi.bus.steering_angle()` → angle, max

The front wheels' angle and the most they turn, degrees (right positive).

*Since 0.2.22.*

### omsi.bus.stop_brake

`omsi.bus.stop_brake()` → boolean or nil

Whether the stop brake (the door brake) holds the bus.

*Since 0.2.22.*

### omsi.bus.stop_requested

`omsi.bus.stop_requested()` → boolean or nil

Whether a passenger has asked to stop.

*Since 0.2.22.*

### omsi.bus.ticket_request

`omsi.bus.ticket_request()` → name, price

The ticket the passenger at the cash desk asks for (nothing when nobody asks).

*Since 0.2.22.*

### omsi.bus.tickets

`omsi.bus.tickets()` → list of tables

The tickets the map sells: `{name, price, day_ticket}`.

*Since 0.2.22.*

### omsi.bus.toggle_door

`omsi.bus.toggle_door([n])` → boolean

Presses the key of doorway `n` (from 1; 0 or none: all doors), as the player would.

Parameters: `n` integer, optional.

*Since 0.2.22. Permission: `vehicle_write`.*

```lua
omsi.bus.toggle_door(1)  -- the front door
```

### omsi.bus.toggle_handbrake

`omsi.bus.toggle_handbrake()` → boolean

Puts the parking brake on or off, as its key does.

*Since 0.2.22. Permission: `vehicle_write`.*

### omsi.bus.track

`omsi.bus.track()` → table or nil

Where the player's rail vehicle (tram, metro, train) is on its track, as distances along it rather than x and y: `{lane, s, lane_length, forward, travelled}` - the track (an index into the map's lanes), the metres from its start and its length, whether the vehicle faces the track's direction, and the net metres it has moved along the tracks since it was put on them (they go down, below zero too, while it backs up). `nil` for a road vehicle. A plugin can work out block sections, distances to a signal or a stop, or a train protection system from it.

*Since 0.3.55.*

```lua
local t = omsi.bus.track()
if t then print("lane", t.lane, "at", t.s, "of", t.lane_length) end
```

### omsi.bus.trailers

`omsi.bus.trailers()` → integer or nil

Parts coupled behind the bus (an articulated bus's rear counts).

*Since 0.2.22.*

### omsi.bus.triggers

`omsi.bus.triggers()` → list of strings

The names of the bus's script triggers (for `trigger`, `press`).

*Since 0.2.22.*

### omsi.bus.velocity

`omsi.bus.velocity()` → number or nil

The bus's speed in km/h, negative backwards.

*Since 0.2.22.*

### omsi.bus.velocity_vector

`omsi.bus.velocity_vector()` → x, y, z

Its velocity in the world, m/s (x east, y north, z up).

*Since 0.2.22.*

### omsi.bus.wheels

`omsi.bus.wheels()` → list of tables

Each wheel: `{axle, side, rpm, radius, suspension, driven}` (`side` 0 left, 1 right; `suspension` the spring's travel).

*Since 0.2.22.*

### omsi.distance

`omsi.distance(x, y)` → number or nil

Metres from the bus to a map point, or `nil` on foot.

Parameters: `x` number; `y` number.

*Since 0.1.10.*

### omsi.has_vehicle

`omsi.has_vehicle()` → boolean

`true` while the player drives a vehicle.

*Since 0.1.5.*

### omsi.mesh_point

`omsi.mesh_point(mesh, x, y, z, [section])` → x, y, z

Where a point of one of the bus's meshes is now, in map metres: `mesh` names the mesh's file (`"sht_bashmak.o3d"`, or a part of its path), `x, y, z` is the point in the model's frame (x right, y forward, z up) as the mesh stands at rest; its animations and the body's lean move it. `section` (0, the default) looks in a section coupled behind instead (1, 2, ...: an articulated bus's rear part), its point in that section's model frame. For a plugin that lines a part of the bus up with something outside it. Nothing when the bus or the section has no such mesh, or `mesh` is empty.

Parameters: `mesh` string; `x` number; `y` number; `z` number; `section` integer, optional.

*Since 0.2.22.*

```lua
local x, y, z = omsi.mesh_point("mirror.o3d", 0.2, 1.0, 2.1)
```

### omsi.position

`omsi.position()` → x, y, z, heading

Where the bus is: map metres (x east, y north, z up) and its heading in degrees clockwise from north; nothing on foot.

*Since 0.1.10.*

```lua
local x, y, z, heading = omsi.position()
if x then omsi.log(string.format("%.0f %.0f facing %.0f", x, y, heading)) end
```

### omsi.press

`omsi.press(name)` → nil

Holds a key of the bus down: fires the trigger `name` (let it go with `release`).

Parameters: `name` string.

*Since 0.1.5. Permission: `vehicle_write`.*

```lua
omsi.press("bus_horn")
omsi.after(0.5, function() omsi.release("bus_horn") end)
```

### omsi.release

`omsi.release(name)` → nil

Lets a key go: fires `<name>_off`.

Parameters: `name` string.

*Since 0.1.5. Permission: `vehicle_write`.*

### omsi.set_str

`omsi.set_str(name, text)` → boolean

Sets a string variable; `true` when the bus has it.

Parameters: `name` string; `text` string.

*Since 0.1.5. Permission: `vehicle_write`.*

### omsi.set_var

`omsi.set_var(name, value)` → boolean

Sets a script variable; `true` when the bus has that variable.

Parameters: `name` string; `value` number.

*Since 0.1.5. Permission: `vehicle_write`.*

```lua
omsi.set_var("elec_busbar_main", 1)
```

### omsi.speed

`omsi.speed()` → number

The bus's speed in km/h, forwards or backwards (0 on foot).

*Since 0.1.10.*

### omsi.str

`omsi.str(name)` → string or nil

A string variable of the bus's scripts (`IBIS_terminus_name`).

Parameters: `name` string.

*Since 0.1.5.*

### omsi.sys

`omsi.sys(name)` → number or nil

A system variable of the scripts: `Time`, `Day`, `Weather_Temperature`, `SunAlt`, ... (read only).

Parameters: `name` string.

*Since 0.1.5.*

The system variables (the scripts' `S.L.` list), read only:

`Timegap`, `GetTime`, `NoSound`, `Pause`, `Time`, `Day`, `Month`, `Year`, `DayOfYear`, `mouse_x`, `mouse_y`,
`PrecipType`, `PrecipRate`, `coll_pos_x`, `coll_pos_y`, `coll_pos_z`, `coll_energy`, `Weather_Temperature`,
`Weather_AbsHum`, `wearlifespan`, `AutoClutch`, `SunAlt`. Names are matched without regard to case.

```lua
local temp = omsi.sys("Weather_Temperature")
```

### omsi.trigger

`omsi.trigger(name)` → nil

A key press of the bus: fires the trigger, then `<name>_off`.

Parameters: `name` string.

*Since 0.1.5. Permission: `vehicle_write`.*

```lua
omsi.trigger("bus_horn")
```

### omsi.var

`omsi.var(name)` → number or nil

A script variable of the bus (`Velocity`, `elec_busbar_main`, the names the `.osc` files and `.opl` lists use); `nil` without a bus or for a name it does not have.

Parameters: `name` string.

*Since 0.1.5.*

```lua
local kmh = omsi.var("Velocity")
if kmh and kmh > 50 then omsi.message("Too fast!", 2) end
```

### omsi.vars

`omsi.vars([kind])` → list of strings

The names of every variable of the bus's scripts, or of every string variable with `"str"`.

Parameters: `kind` string, optional.

*Since 0.1.10.*

```lua
for _, name in ipairs(omsi.vars()) do print(name) end
```

### omsi.vehicle

`omsi.vehicle()` → string or nil

The vehicle's name (manufacturer and type), or `nil` on foot.

*Since 0.1.5.*

### omsi.vehicle_manufacturer

`omsi.vehicle_manufacturer()` → string or nil

The manufacturer part of the vehicle's name, as its `[friendlyname]` has it (`"Solaris III Gen"`).

*Since 0.2.21.*

### omsi.vehicle_model

`omsi.vehicle_model()` → string or nil

The model part of the vehicle's name (`"Urbino 10 / 2D"`).

*Since 0.2.21.*

## Game data getters

`omsi.game.query` reads the same game-owned data as the vehicle scripts' getter functions, without running a script. Getter arguments keep OMSI's zero-based indices; every other list in the API starts at 1.

### omsi.game.queries

`omsi.game.queries()` → list of tables

Read-only game data getters: `{name, args = {{name, type}, ...}, result}`. Names and zero-based indices follow the M.V. getter names, but plugins read the game data directly without executing scripts. `rgba` means four results in the range 0–255.

*Since 0.3.40.*

```lua
for _, q in ipairs(omsi.game.queries()) do print(q.name, q.result) end
```

### omsi.game.query

`omsi.game.query(name, [args], [object])` → value(s), or nil and reason

Read a getter listed by `game.queries` from the player's vehicle's HOF, timetable, passengers, tickets, radio, fonts or textures. Arguments are an ordered list (empty by default). No script callback is invoked. `STReadPixel` returns RGBA without changing the bus's drawing colour. Supply a loaded scripted map object's id as `object` to read its data, including `GetArrBus*` on departure boards, without needing a player vehicle. Object ids can be found through `map.objects_near`. Without `object`, reads use the player's vehicle. Unknown names, mutating operations and invalid arguments raise an error; no vehicle returns nil and a reason.

Parameters: `name` string; `args` table, optional; `object` integer, optional.

*Since 0.3.40.*

The getters `game.query` takes (names are matched without regard to case; arguments in this order):

| Getter | Arguments | Result |
| --- | --- | --- |
| `TextLength` | `text`, `font` | number |
| `STReadPixel` | `texture`, `x`, `y` | r, g, b, a (0-255) |
| `STGetR` | `texture` | number |
| `STGetG` | `texture` | number |
| `STGetB` | `texture` | number |
| `STGetA` | `texture` | number |
| `GetRouteIndex` | `code` | number |
| `GetRouteTerminusIndex` | `route` | number |
| `GetTerminusCode` | `terminus` | number |
| `GetTerminusIndex` | `code` | number |
| `GetTerminusString` | `terminus`, `field` | string |
| `GetBusstopCount` | `route` | number |
| `GetRouteBusstopIdent` | `route`, `stop` | string |
| `GetBusstopIndex` | `ident` | number |
| `GetBusstopString` | `stop`, `field` | string |
| `GetDepotStringGlobal` | `field` | string |
| `GetTTLineString` | - | string |
| `GetTTDelay` | - | number |
| `GetTTBusstopCount` | - | number |
| `GetTTBusstopIndex` | - | number |
| `GetTTerminusIndex` | - | number |
| `GetTTTerminusIndex` | - | number |
| `GetHeightAbovePoint` | `x`, `y`, `z` | number |
| `GetHumanCountOnPathLink` | `link` | number |
| `GetTTBusstopName` | `stop` | string |
| `GetTTBusstopDep` | `stop` | number |
| `GetTTBusstopArr` | `stop` | number |
| `NrSpecRandom` | `seed` | number |
| `GetTicketName` | `ticket` | string |
| `GetTTicketName` | `ticket` | string |
| `GetTicketValue` | `ticket` | number |
| `GetHumanCountOnSeat` | `seat` | number |
| `GetArrBusLine` | `arrival` | string |
| `GetArrBusTerminus` | `arrival` | string |
| `GetArrBusTimeDiff` | `arrival` | number |
| `GetRadioName` | - | string |
| `GetRadioSong` | - | string |

`GetFontIndex` is not a getter here: it can load fonts. Commands that change or draw are not getters either.

```lua
local text = omsi.game.query("GetTerminusString", { 0, 0 })
local arrival = omsi.game.query("GetTTBusstopArr", { 0 })
```

## HOF depot file

`omsi.hof.*` reads the player's loaded HOF depot file as a snapshot. Records keep their zero-based `index` for the getters; Lua lists start at 1. Snapshots are copies: changing them changes nothing in the game. Keep one and read a new one when the vehicle or its HOF changes.

### omsi.hof.busstop

`omsi.hof.busstop(identifier)` → table or nil

The first bus stop whose `identifier` exactly matches the supplied HOF stop identifier. Returns `{index, identifier, strings}`; `index` is zero-based for `GetBusstopString` and `strings[1]` is getter field 0. Returns nil without a vehicle, HOF or match.

Parameters: `identifier` string.

*Since 0.3.40.*

### omsi.hof.get

`omsi.hof.get()` → table or nil

A snapshot of every parsed field of the player's loaded HOF: `path`, `name`, `service_trip`, `global_strings`, `string_count_terminus`, `string_count_busstop`, `termini`, `bus_stops`, `info_trips`, `info_busstop_lists`, `info_busstops`. Records include their zero-based `index`; Lua arrays (including strings) start at 1. Returns nil without a vehicle or HOF. No filesystem access or scripts are involved; retain the snapshot until the vehicle or HOF changes.

*Since 0.3.40.*

```lua
local hof = omsi.hof.get()
if hof then print(hof.name, #hof.termini, #hof.bus_stops) end
```

### omsi.hof.terminus

`omsi.hof.terminus(identifier)` → table or nil

The first terminus whose internal `identifier` exactly matches the supplied timetable terminus, using the same case-sensitive comparison as `GetTTTerminusIndex`. Returns `{index, code, identifier, terminus_stop, all_exit, strings}`; `index` is zero-based for `GetTerminusString`, `strings[1]` is getter field 0, and `terminus_stop` is nil when absent. Returns nil without a vehicle, HOF or match; does not change the duty or destination.

Parameters: `identifier` string.

*Since 0.3.40.*

```lua
local tour = omsi.duty.tour()
if tour then
  for _, trip in ipairs(tour.trips) do
    local t = omsi.hof.terminus(trip.terminus)
    -- getter fields 8 and 9 are Lua elements 9 and 10
    if t then print(trip.name, t.index, t.code, t.strings[9], t.strings[10]) end
  end
end
```

## Duty and timetable

`omsi.duty.*` is the player's current duty (line, tour, trip, stops). `omsi.timetable.*` reads the whole map's timetable without taking a duty. Times are seconds since midnight of the service day and can go past 86400.

### omsi.duty.active

`omsi.duty.active()` → boolean

Whether the player drives a duty (a line and tour of the timetable).

*Since 0.2.22.*

### omsi.duty.at_stop

`omsi.duty.at_stop()` → boolean

Whether the bus stands at the next stop (within 25 m of it).

*Since 0.2.22.*

### omsi.duty.delay

`omsi.duty.delay()` → number or nil

Seconds the bus is late (early negative), worked out between the stops as the game's timetable does.

*Since 0.2.22.*

### omsi.duty.finish

`omsi.duty.finish()` → boolean

Gives the duty up (the bus drives on without a timetable); `true` when there was one.

*Since 0.2.22. Permission: `world_write`.*

### omsi.duty.get

`omsi.duty.get()` → table or nil

The duty now: `line`, `tour`, `trip` (its number in the duty, from 1), `trips`, `trip_name`, `terminus`, `departure`, `arrival`, `stops`, `next_stop` and `previous_stop` (each a stop: `{number, name, id, arrival, departure, stops, x, y, z}`), `at_stop`, `trip_done`, `delay` (seconds, late positive).

*Since 0.2.22.*

```lua
local d = omsi.duty.get()
if d and d.next_stop then
  omsi.ui.toast(d.next_stop.name .. " " .. omsi.fmt.delay(d.delay))
end
```

### omsi.duty.next_stop

`omsi.duty.next_stop()` → table or nil

The next stop of the trip (as `duty.stops` gives them).

*Since 0.2.22.*

### omsi.duty.set_auto_ibis

`omsi.duty.set_auto_ibis(on)` → boolean

Whether the game types the duty into the bus's IBIS by itself - line, route, destination and the stop, at Shift+U and `--autostart` and again as each trip begins (on as a bus is taken). Off, the driver types them, or the plugin with `duty.type_ibis`. For the bus the player drives now, and while the plugin runs: when it stops or is switched off the game types the IBIS again, unless another plugin still leaves it to the driver. Off lasts for that bus only: after a change of vehicle (the `vehicle` event) the game types the new one's IBIS again, so a plugin that leaves it to the driver calls `duty.set_auto_ibis(false)` again there. `true` when there is a bus.

Parameters: `on` bool.

*Since 0.3.41. Permission: `vehicle_write`.*

```lua
omsi.on("vehicle", function()
  omsi.duty.set_auto_ibis(false)  -- the driver types the IBIS
end)
```

### omsi.duty.set_closed_stops

`omsi.duty.set_closed_stops(stops, [via])` → boolean

Closes stops of the duty's route for today - a diversion, roadworks: a list of their names (on every trip of the duty), and the way round them (`via`). The navigator strikes them through and shows `via` for the next one; the bus does not call there, and the duty goes on past them without counting them missed (no `stops_skipped`). Each call replaces the list (64 names at most), an empty one opens them all again; a duty taken anew (`duty.start`) starts with none. `duty.stops` marks them `closed`. `true` when there is a duty.

Parameters: `stops` table; `via` string, optional.

*Since 0.3.36. Permission: `world_write`.*

```lua
omsi.duty.set_closed_stops({ "Markt", "Rathaus" }, "via Bahnhofstraße")
```

### omsi.duty.skip_stop

`omsi.duty.skip_stop()` → string or nil

Skips the next stop (the duty goes on to the one after); the name of the stop skipped.

*Since 0.2.22. Permission: `world_write`.*

### omsi.duty.skip_to

`omsi.duty.skip_to(stop)` → boolean

Makes stop `stop` (from 1) of the trip the next one, forwards or back.

Parameters: `stop` integer.

*Since 0.2.22. Permission: `world_write`.*

### omsi.duty.start

`omsi.duty.start(line, tour, [trip], [stop])` → true, or false and the reason

Takes a duty, as the game menu's "Line and tour" does: a line and tour of `timetable.lines`, its trip (from 1 in the order they leave; default the first) and the stop to start at (from 1 among those the trip calls at; default the first). The bus stays where it is.

Parameters: `line` string; `tour` string; `trip` integer, optional; `stop` integer, optional.

*Since 0.2.22. Permission: `world_write`.*

```lua
local ok, why = omsi.duty.start("136", "3")
if not ok then omsi.warn(why) end
```

### omsi.duty.stops

`omsi.duty.stops()` → list of tables

The stops of the trip now: `{number, name, id, arrival, departure, stops, closed, passed, x, y, z}` (`stops` false: the bus passes it; `closed`: closed today, `duty.set_closed_stops`; `x, y, z` where the stop's place is known; `id` the map's object id).

*Since 0.2.22.*

```lua
for _, stop in ipairs(omsi.duty.stops()) do
  print(stop.name, omsi.fmt.clock(stop.departure))
end
```

### omsi.duty.tour

`omsi.duty.tour()` → table or nil

The complete source tour of the selected duty, even when only one trip was selected. Same structure as `timetable.tour`; times align with the active duty's service day, with `time_offset` seconds applied to all trips and stops.

*Since 0.3.40.*

```lua
local tour = omsi.duty.tour()
if tour then
  for _, trip in ipairs(tour.trips) do
    for _, stop in ipairs(trip.stops) do
      print(trip.line, trip.terminus, stop.name, stop.arrival, stop.departure)
    end
  end
end
```

### omsi.duty.trip_stops

`omsi.duty.trip_stops(trip)` → list of tables or nil

The stops of a trip of the duty (from 1), as `duty.stops` gives them.

Parameters: `trip` integer.

*Since 0.2.22.*

### omsi.duty.trips

`omsi.duty.trips()` → list of tables

The trips of the duty: `{number, name, line, terminus, departure, arrival, stops}`.

*Since 0.2.22.*

### omsi.duty.type_ibis

`omsi.duty.type_ibis()` → boolean

Types the trip under way into the bus's IBIS now - line, route, destination and the stop the bus is at - as Shift+U does (`bus.set_line` types the line only); with `duty.set_auto_ibis` on, the next trips follow by themselves. `true` when there is a duty and a bus.

*Since 0.3.41. Permission: `vehicle_write`.*

### omsi.timetable.buses

`omsi.timetable.buses()` → list of tables

The timetable buses of the AI on the road: `{id, line, tour, trip, terminus, departure, next_stop_id, at_stop, trip_done, delay, x, y, number}`.

*Since 0.2.22.*

### omsi.timetable.lines

`omsi.timetable.lines()` → list of tables

The map's lines: `{name, user_allowed, tours}`, each tour `{number, today, trips}` (`today`: it runs on the game's date).

*Since 0.2.22.*

```lua
for _, line in ipairs(omsi.timetable.lines()) do print(line.name, #line.tours) end
```

### omsi.timetable.stop_names

`omsi.timetable.stop_names()` → list of tables

The stops the timetable knows: `{id, name}` (`id` the map's object id).

*Since 0.2.22.*

### omsi.timetable.stops

`omsi.timetable.stops(line, tour, [trip])` → list of tables

The stops of a tour's trips (or of its trip `trip`, from 1 in the order they leave) that the bus calls at: `{trip, station, name, departure}`.

Parameters: `line` string; `tour` string; `trip` integer, optional.

*Since 0.2.22.*

### omsi.timetable.tour

`omsi.timetable.tour(line, tour)` → table, or nil and reason

Read a full tour without taking it: `{line, number, ai_group, extra, today, time_offset, trips}`. Each trip has `number`, `source_number` (both from 1), `name`, `line`, `terminus`, `track`, `reverse`, `profile_index` (from 0), `profile` (`name`, `factor`), `departure`, `arrival`, and all `stops` (`id`, `name`, `arrival`, `departure`, `stops`, optional `x,y,z`). Times are seconds from service-day midnight and can exceed 86400; passing stops are retained. Unknown trip references are skipped, retaining their source numbers. No AI vehicle is reserved.

Parameters: `line` string; `tour` string.

*Since 0.3.40.*

## Map

`omsi.map.*` reads the map: tiles, ground height, lanes, stops, objects, start points; and moves the player's bus. Coordinates are map metres: x east, y north, z up.

### omsi.map.entrypoints

`omsi.map.entrypoints()` → list of tables

The map's start points: `{index, name, x, y, z, heading}` (`index` from 1; the place where the game has read its tile).

*Since 0.2.22.*

### omsi.map.ground

`omsi.map.ground(x, y)` → number or nil

The height of the ground (the road where there is one, else the terrain) at a map point, where its tile is loaded.

Parameters: `x` number; `y` number.

*Since 0.2.22.*

```lua
local z = omsi.map.ground(x, y)
```

### omsi.map.info

`omsi.map.info()` → table or nil

`{name, friendly_name, path, left_hand_traffic, tile_size}`: the map's names, its global.cfg and the side it drives on.

*Since 0.2.22.*

### omsi.map.lane

`omsi.map.lane(x, y)` → table or nil

The traffic lane nearest to a map point: `{index, distance, speed_limit, name, traffic_light}`.

Parameters: `x` number; `y` number.

*Since 0.2.22.*

### omsi.map.name

`omsi.map.name()` → string or nil

The map's name (its folder's).

*Since 0.2.22.*

### omsi.map.object

`omsi.map.object(id)` → x, y, z, heading

Where a map object is, by its id (a stop's id of the timetable, say); nothing when the game has not read its tile.

Parameters: `id` integer.

*Since 0.2.22.*

### omsi.map.objects_near

`omsi.map.objects_near(x, y, [radius])` → list of tables

The map objects within `radius` m (default 50, at most 2000 of them, nearest first): `{id, x, y, z, heading, distance}`.

Parameters: `x` number; `y` number; `radius` number, optional.

*Since 0.2.22.*

```lua
local x, y = omsi.position()
for _, o in ipairs(omsi.map.objects_near(x, y, 30)) do print(o.id, o.distance) end
```

### omsi.map.place_on_road

`omsi.map.place_on_road(x, y)` → boolean

Puts the player's bus on the street nearest to a map point (within 300 m), along it.

Parameters: `x` number; `y` number.

*Since 0.2.22. Permission: `world_write`.*

### omsi.map.speed_limit

`omsi.map.speed_limit()` → number or nil

The speed limit (km/h) of the lane the player's bus drives on (`nil` off the lanes).

*Since 0.2.22.*

### omsi.map.stops

`omsi.map.stops()` → list of tables

The bus stops of the tiles loaded (around the camera): `{id, name, x, y, z, heading}`.

*Since 0.2.22.*

### omsi.map.teleport

`omsi.map.teleport(x, y, [z], [heading])` → boolean

Moves the player's bus to a map point (z none: onto the highest ground there) facing `heading` (default north), as the game menu's move does; the `service` event says `teleport`.

Parameters: `x` number; `y` number; `z` number, optional; `heading` number, optional.

*Since 0.2.22. Permission: `world_write`.*

```lua
omsi.map.teleport(x, y, nil, 90)  -- z nil: onto the ground, facing east
```

### omsi.map.teleport_to

`omsi.map.teleport_to(index)` → boolean

Moves the player's bus to start point `index` (from 1, as `map.entrypoints` lists them).

Parameters: `index` integer.

*Since 0.2.22. Permission: `world_write`.*

### omsi.map.terrain

`omsi.map.terrain(x, y)` → number or nil

The terrain's height alone at a map point.

Parameters: `x` number; `y` number.

*Since 0.2.22.*

### omsi.map.tile

`omsi.map.tile()` → tile_x, tile_y

The tile the player's bus is on (nothing on foot).

*Since 0.2.22.*

### omsi.map.tile_at

`omsi.map.tile_at(x, y)` → tile_x, tile_y, local_x, local_y

The tile of a map point and the metres in it (x east, y north).

Parameters: `x` number; `y` number.

*Since 0.2.22.*

```lua
local tx, ty, lx, ly = omsi.map.tile_at(x, y)
```

### omsi.map.tiles

`omsi.map.tiles()` → list of tables

The map's tiles: `{x, y, file, loaded}` (numbered as global.cfg's `[map]` list).

*Since 0.2.22.*

### omsi.map.to_world

`omsi.map.to_world(tile_x, tile_y, local_x, local_y)` → x, y

A place given by its tile and the metres in it, in map coordinates.

Parameters: `tile_x` integer; `tile_y` integer; `local_x` number; `local_y` number.

*Since 0.2.22.*

## Scenery objects

`omsi.object.*` is for scripts that a scenery object's `.sco` names with `[lua_script]` (see [Scripts of map objects](OBJECT_SCRIPTS.md)). They only work on objects that script drives; ids come from the `object_*` events or `object.list()`.

### omsi.object.info

`omsi.object.info(id)` → table

`{id, type, x, y, z, heading, active, distance}` of an object: its type (the `.sco` file's name), where it stands (map coordinates), the way it faces (compass degrees), whether it is in the script's range and how far the listener is from it (m).

Parameters: `id` integer.

*Since 0.2.22.*

### omsi.object.list

`omsi.object.list()` → list of numbers

The ids of the objects this script drives that are loaded now (those within its range are active).

*Since 0.2.22.*

### omsi.object.meshes

`omsi.object.meshes(id)` → list of strings

The mesh files of an object, in the order the scripts number them from 1.

Parameters: `id` integer.

*Since 0.2.22.*

### omsi.object.set_mesh

`omsi.object.set_mesh(id, mesh, [transform])` → boolean

Moves, turns and scales one mesh of an object over what its animations do. `mesh` is its number from 1 or its file name; `transform` is `{x, y, z, rx, ry, rz, sx, sy, sz, px, py, pz}`: a move in metres, a turn about the x, y and z axes in degrees (z up, positive counter-clockwise seen from above), a scale, all about the point (px, py, pz) of the mesh, every key optional. Without a table the mesh is as the model has it. Objects with a script are redrawn every frame; moving a mesh costs nothing more.

Parameters: `id` integer; `mesh` string; `transform` table, optional.

*Since 0.2.22.*

```lua
omsi.object.set_mesh(id, "barrier.o3d", { rz = 90, px = 0.1 })
```

### omsi.object.set_str

`omsi.object.set_str(id, name, value)` → boolean

Sets a string variable of an object (a display's text, a texture's name).

Parameters: `id` integer; `name` string; `value` string.

*Since 0.2.22.*

### omsi.object.set_var

`omsi.object.set_var(id, name, value)` → boolean

Sets a script variable of an object, which its `[newanim]` animations, `[visible]` conditions, materials, lamps and sounds read; false when it has none of that name.

Parameters: `id` integer; `name` string; `value` number.

*Since 0.2.22.*

```lua
omsi.on("object_init", function(id)
  omsi.object.set_var(id, "lamp_on", 1)
end)
```

### omsi.object.set_vars

`omsi.object.set_vars(id, values)` → integer

Sets many variables of an object at once (a table name -> number); how many it has.

Parameters: `id` integer; `values` table.

*Since 0.2.22.*

### omsi.object.set_visible

`omsi.object.set_visible(id, mesh, [visible])` → boolean

Shows or hides one mesh of an object whatever its `[visible]` conditions say; without `visible` the conditions decide again.

Parameters: `id` integer; `mesh` string; `visible` bool, optional.

*Since 0.2.22.*

### omsi.object.str

`omsi.object.str(id, name)` → string or nil

A string variable of an object.

Parameters: `id` integer; `name` string.

*Since 0.2.22.*

### omsi.object.trigger

`omsi.object.trigger(id, name)` → boolean

Runs a `{trigger:name}` of the object's own `.osc` script; false when it has none.

Parameters: `id` integer; `name` string.

*Since 0.2.22.*

### omsi.object.var

`omsi.object.var(id, name)` → number or nil

A script variable of an object (those its `[varnamelist]` declares, and the builtin ones: `Switch`, `InUse`, `NightlightA` ...); `nil` when it has none of that name.

Parameters: `id` integer; `name` string.

*Since 0.2.22.*

## AI traffic

`omsi.traffic.*` reads and changes the AI traffic. `omsi.others` and the `other_*` functions also cover other players' buses in a LAN game.

### omsi.other_body_frame

`omsi.other_body_frame(id, [section])` → x, y, z, rx, ry, rz, fx, fy, fz, ux, uy, uz

`body_frame` of one of `others`: its body now, origin and axes.

Parameters: `id` integer; `section` integer, optional.

*Since 0.2.22.*

### omsi.other_mesh_point

`omsi.other_mesh_point(id, mesh, x, y, z, [section])` → x, y, z

`mesh_point` of one of `others`: where a point of one of its meshes (or, with `section`, of one of its coupled sections') is now.

Parameters: `id` integer; `mesh` string; `x` number; `y` number; `z` number; `section` integer, optional.

*Since 0.2.22.*

### omsi.other_var

`omsi.other_var(id, name)` → number or nil

A script variable of one of `others`, or `nil`.

Parameters: `id` integer; `name` string.

*Since 0.2.21.*

### omsi.others

`omsi.others([radius])` → list of tables

The other vehicles within `radius` m of the bus (default 300): each `{id, kind, name, x, y, z, heading}`, `kind` being `"ai"` (the traffic) or `"player"` (another player's bus in a LAN game); empty on foot.

Parameters: `radius` number, optional.

*Since 0.2.21.*

```lua
for _, v in ipairs(omsi.others(100)) do print(v.kind, v.name) end
```

### omsi.set_other_var

`omsi.set_other_var(id, name, value)` → boolean

Sets a script variable of one of `others`; `true` when that vehicle has it. An AI vehicle keeps it until its scripts write it again; another player's bus takes its values from the network again.

Parameters: `id` integer; `name` string; `value` number.

*Since 0.2.21. Permission: `traffic_write`.*

### omsi.traffic.ahead

`omsi.traffic.ahead([reach])` → table or nil

The AI vehicle ahead of the player's bus within `reach` m (default 100) and 20° of its heading, with its `distance`: for a distance warning or a cruise control.

Parameters: `reach` number, optional.

*Since 0.2.22.*

```lua
local car = omsi.traffic.ahead(80)
if car and car.distance < 15 then omsi.message("Keep your distance", 2) end
```

### omsi.traffic.clear

`omsi.traffic.clear()` → integer or nil

Takes every car not running to a timetable off the road; how many went.

*Since 0.2.22. Permission: `traffic_write`.*

### omsi.traffic.counts

`omsi.traffic.counts()` → driving, buses, asleep, parked

The AI vehicles: driving, timetable buses among them, asleep out of range, parked.

*Since 0.2.22.*

### omsi.traffic.density

`omsi.traffic.density()` → cars, share

How many cars the traffic keeps around the camera, and the share of them that do not run to a timetable (0 to 1).

*Since 0.2.22.*

### omsi.traffic.get

`omsi.traffic.get(id)` → table or nil

One AI vehicle by its id, as `traffic.list` gives them.

Parameters: `id` integer.

*Since 0.2.22.*

### omsi.traffic.light_ahead

`omsi.traffic.light_ahead([reach])` → table or nil

The traffic light the player's bus comes to within `reach` m (default 80): `{aspect, change_in, distance}` - `aspect` `"red"`, `"red_yellow"`, `"green"`, `"green_yellow"`, `"yellow"` or `"dark"`, `change_in` the seconds to its next change.

Parameters: `reach` number, optional.

*Since 0.2.22.*

```lua
local l = omsi.traffic.light_ahead()
if l and l.aspect == "red" then print("red in", l.distance, "m") end
```

### omsi.traffic.list

`omsi.traffic.list([radius])` → list of tables

The AI vehicles (within `radius` m of the player's bus, when given): `{id, kind, name, x, y, z, heading, speed, max_speed, waiting_for, standing, braking, blinker, line, bounding_box, distance}`. `kind`: `"car"`, `"taxi"`, `"bus"`, `"truck"`, `"timetable_bus"`, `"tram"`, `"bicycle"`; `waiting_for` why it waits or slows (`"lead"`, `"light"`, `"yield"`, `"people"`, ...); `standing` the seconds it has stood; `bounding_box` the body's `[boundingbox]` as `{width, length, height, cx, cy, cz, heading, default}` (metres, the .bus/.ovh order; `cx`/`cy`/`cz` the box centre's offset from the vehicle's x, y, z along the body's right, forward and up, which follow `heading`; `default` when the file gives no box and the collision code's 2.5 x 12 x 3 m is used).

Parameters: `radius` number, optional.

*Since 0.2.22.*

### omsi.traffic.nearest

`omsi.traffic.nearest([kind])` → table or nil

The AI vehicle nearest to the player's bus (of that `kind`, when given), with its `distance`.

Parameters: `kind` string, optional.

*Since 0.2.22.*

### omsi.traffic.remove

`omsi.traffic.remove(id)` → boolean

Takes an AI vehicle off the road (a timetable bus too: its timetable forgets it); its passengers get out.

Parameters: `id` integer.

*Since 0.2.22. Permission: `traffic_write`.*

### omsi.traffic.set_density

`omsi.traffic.set_density(cars, [share])` → boolean

Changes the traffic's amount (as the game menu's slider: 0 to 500 cars) and, when given, the share not running to a timetable; it fills up or thins out over the next seconds.

Parameters: `cars` integer; `share` number, optional.

*Since 0.2.22. Permission: `traffic_write`.*

```lua
omsi.traffic.set_density(50)
```

## People

`omsi.people.*` reads the pedestrians and passengers, and lets a plugin set a boarding passenger's ticket. See [Passengers' tickets](PLUGINS.md#passengers-tickets) for the full flow.

### omsi.people.counts

`omsi.people.counts()` → walking, waiting, riding

The people of the map near the camera: walking, waiting at stops, riding a bus.

*Since 0.2.22.*

### omsi.people.density

`omsi.people.density()` → number or nil

The people setting: 0 to 3, 1 the map's own amount.

*Since 0.2.22.*

### omsi.people.get

`omsi.people.get(id)` → table or nil

One person by id, as `people.list` gives them, and more: `stop_name` (the timetable's name of the stop they wait or waited at), `ticket` (the name of the ticket they ask for), `ticket_kind` (`"none"`, `"stamp"`, `"buy"`), `ticket_index` (the ticket pack's, from 1; 0 none), `ticket_price`, `ticket_voice`, `ticket_by` (the plugin that changed it), `ticket_asked` (asked for at the cash desk already), `tag` (`people.set_ticket`'s), `age`, `model` (the `.hum` file), `voice` (its voice folder). `people.list` gives the same fields.

Parameters: `id` integer.

*Since 0.3.*

### omsi.people.list

`omsi.people.list([radius])` → list of tables

The people (within `radius` m of the player's bus, when given): `{id, x, y, z, state, aboard, ai_bus, stop, destination, ticket, complaint}`. `state`: `"strolling"`, `"idle"`, `"standing"`, `"waiting"`, `"to_bus"`, `"boarding"`, `"riding"`, `"seated"`, `"leaving"`, `"to_stop"`; `aboard` in the player's bus; `complaint` 0 to 3 (3: they leave); and the fields `people.get` describes.

Parameters: `radius` number, optional.

*Since 0.2.22.*

### omsi.people.set_density

`omsi.people.set_density(value)` → boolean

Changes the people setting (0 to 3).

Parameters: `value` number.

*Since 0.2.22. Permission: `traffic_write`.*

### omsi.people.set_ticket

`omsi.people.set_ticket(id, ticket)` → true, or false and the reason

Changes the ticket of a passenger boarding the player's bus (the `passenger_ticket` event names them): `{kind, ticket, name, price, payment, voice, tag}`, each optional. `kind` (`"none"`: a pass, `"stamp"`, `"buy"`) can only be changed until they are in the bus; `ticket` selects another ticket from the pack (starting at 1, with its name and price; a buyer must have a ticket: 0 is refused, and a buyer created without `ticket` gets the pack's ticket for their age). `name` and `price` set the ticket name and ticket price, respectively, until the passenger has asked at the cash desk. `payment` specifies the amount of money handed over for a ticket purchase; it does not change the ticket price and is raised to at least the ticket price. It must be set before the passenger reaches the cash desk. If the currency's denominations cannot represent the requested amount exactly, the amount handed over may be higher. `voice` selects a line from the passenger's voice folder to use instead of `Ticket_<n>_<k>` (`"Zone_4"` selects `Zone_4.wav`; if the file is missing, the usual line is used; `""` also falls back to the usual line), until the passenger has asked at the cash desk. `tag` is plugin-defined text kept with the person (`people.get`), and can be set at any time. Returns `false` and the reason when nothing changed.

Parameters: `id` integer; `ticket` table.

*Since 0.3. Permission: `traffic_write`.*

```lua
omsi.on("passenger_ticket", function(p)
  if p.kind == "buy" then
    omsi.people.set_ticket(p.id, { name = "Single", price = 3, tag = "my_tariff" })
  end
end)
```

### omsi.people.stops

`omsi.people.stops()` → list of tables

The stops near the camera where people wait: `{id, name, x, y, z, waiting}`.

*Since 0.2.22.*

### omsi.people.waiting

`omsi.people.waiting(stop)` → integer

How many people wait at a stop (its map object id).

Parameters: `stop` integer.

*Since 0.2.22.*

```lua
local waiting = omsi.people.waiting(stop_id)
```

## Time and date

`omsi.world.*` reads and sets the game's clock, date and pause; `omsi.clock()` is the time of day as text.

### omsi.clock

`omsi.clock()` → string

The game's time of day as `"HH:MM:SS"`.

*Since 0.1.10.*

### omsi.world.date

`omsi.world.date()` → table or nil

The game's date: `{year, month, day, weekday, day_of_year}` (`weekday` 1 Monday to 7 Sunday).

*Since 0.2.22.*

```lua
local d = omsi.world.date()
if d then print(d.year, d.month, d.day) end
```

### omsi.world.pause

`omsi.world.pause()` → boolean

Pauses the game, as P does (not in a LAN game). The plugins stand still with it: the player resumes it.

*Since 0.2.22. Permission: `world_write`.*

### omsi.world.paused

`omsi.world.paused()` → boolean

Whether the game stands still (a plugin hears `pause` and runs no more until `resume`).

*Since 0.2.22.*

### omsi.world.play_time

`omsi.world.play_time()` → number or nil

Seconds played this session (game time, never wraps).

*Since 0.2.22.*

### omsi.world.season

`omsi.world.season()` → folder, snow

The season's texture folder (`nil`: the base textures) and whether snow lies.

*Since 0.2.22.*

### omsi.world.set_date

`omsi.world.set_date(year, month, day)` → boolean

Sets the game's date (the season's textures follow).

Parameters: `year` integer; `month` integer; `day` integer.

*Since 0.2.22. Permission: `world_write`.*

### omsi.world.set_time

`omsi.world.set_time(time)` → boolean

Sets the time of day: seconds since midnight or `"HH:MM"` / `"HH:MM:SS"`, as the game menu's clock does (the timetable starts again after a jump of minutes). Not in a LAN game as a client, nor while the clock follows the computer's.

Parameters: `time` any.

*Since 0.2.22. Permission: `world_write`.*

```lua
omsi.world.set_time("07:30")
```

### omsi.world.set_time_speed

`omsi.world.set_time_speed(factor)` → boolean

Sets how much faster the clock runs (1 to 30; not in a LAN game).

Parameters: `factor` number.

*Since 0.2.22. Permission: `world_write`.*

### omsi.world.sun_altitude

`omsi.world.sun_altitude()` → number or nil

The sun's height over the horizon, degrees.

*Since 0.2.22.*

### omsi.world.time

`omsi.world.time()` → number or nil

The game's time of day, seconds since midnight.

*Since 0.2.22.*

### omsi.world.time_speed

`omsi.world.time_speed()` → number or nil

How much faster than real time the game's clock runs (1 to 30).

*Since 0.2.22.*

## Weather

`omsi.weather.*` reads the weather and changes it as the game menu does. Changes are refused in a LAN game as a client, and while the weather follows a real station.

### omsi.weather.get

`omsi.weather.get()` → table or nil

The weather now: `{name, visibility (m), wind_direction (°), wind_speed (m/s), temperature (°C), humidity (%), absolute_humidity (g/m³), pressure (hPa), clouds, cloud_base (m), precipitation ("none", "rain", "snow"), precipitation_rate (0..1), snow_cover, snow_on_road, wetness (the roads, 0..1), changing, locked}` (`locked`: it follows a real weather station and cannot be set).

*Since 0.2.22.*

### omsi.weather.precipitation

`omsi.weather.precipitation()` → kind, rate

`"none"`, `"rain"` or `"snow"`, and how hard (0 to 1).

*Since 0.2.22.*

### omsi.weather.preset

`omsi.weather.preset(file, [seconds])` → true, or false and the reason

Changes to a weather file (as `weather.presets` names it; `""`: the map's own, changing with the day) over `seconds` (default 1).

Parameters: `file` string; `seconds` number, optional.

*Since 0.2.22. Permission: `world_write`.*

```lua
omsi.weather.preset(omsi.weather.presets()[1].file, 60)
```

### omsi.weather.presets

`omsi.weather.presets()` → list of tables

The weather files installed: `{file, name}`.

*Since 0.2.22.*

### omsi.weather.set

`omsi.weather.set(values)` → true, or false and the reason

Changes the weather as the game menu's sliders do; any of `visibility`, `wind_direction`, `wind_speed`, `temperature`, `pressure`, `clouds` (`"-1"` none, `"Cumulus 1"`..`"3"`, `"Overcast 1"`), `cloud_base`, `precipitation` (`"none"`, `"rain"`, `"snow"`), `precipitation_rate` (0..1), `snow_cover`, `snow_on_road`, `wetness`; the rest stays. Held to the game's ranges; refused in a LAN game as a client or while the weather follows a station.

Parameters: `values` table.

*Since 0.2.22. Permission: `world_write`.*

```lua
local ok, why = omsi.weather.set({ precipitation = "snow", precipitation_rate = 0.5, temperature = -3 })
```

### omsi.weather.temperature

`omsi.weather.temperature()` → number or nil

The air temperature, °C.

*Since 0.2.22.*

### omsi.weather.visibility

`omsi.weather.visibility()` → number or nil

How far one sees, metres.

*Since 0.2.22.*

### omsi.weather.wetness

`omsi.weather.wetness()` → number or nil

How wet the roads are, 0 (dry) to 1.

*Since 0.2.22.*

### omsi.weather.wind

`omsi.weather.wind()` → direction, speed

The wind: where it comes from (degrees) and its speed (m/s).

*Since 0.2.22.*

## Camera

`omsi.camera.*` reads the camera and moves it. Changing it needs the `camera` permission.

### omsi.camera.fov

`omsi.camera.fov()` → number or nil

Its vertical field of view, degrees.

*Since 0.2.22.*

### omsi.camera.get

`omsi.camera.get()` → table or nil

The camera: `{view, x, y, z, yaw, pitch, roll, fov, in_cab, zoom, look_yaw, look_pitch, width, height}` - map metres, degrees (yaw clockwise from north, pitch up positive), the vertical field of view, the picture's pixels.

*Since 0.2.22.*

### omsi.camera.in_cab

`omsi.camera.in_cab()` → boolean

Whether the camera is in the player's own bus (driver or passenger view).

*Since 0.2.22.*

### omsi.camera.look

`omsi.camera.look(yaw, pitch)` → boolean

Turns the head (driver, passenger view) or swings the outside camera round the bus: degrees from straight ahead.

Parameters: `yaw` number; `pitch` number.

*Since 0.2.22. Permission: `camera`.*

### omsi.camera.orientation

`omsi.camera.orientation()` → yaw, pitch, roll

Where it looks, degrees.

*Since 0.2.22.*

### omsi.camera.position

`omsi.camera.position()` → x, y, z

Where the camera is (map metres).

*Since 0.2.22.*

### omsi.camera.project

`omsi.camera.project(x, y, z)` → screen_x, screen_y

Where a map point is seen on the screen, in the panels' pixels (as `ui.screen` measures them); nothing when it is behind the camera. For labels over buses, stops, people.

Parameters: `x` number; `y` number; `z` number.

*Since 0.2.22.*

```lua
local sx, sy = omsi.camera.project(x, y, z + 3)
if sx then -- the point is on screen
end
```

### omsi.camera.set_free

`omsi.camera.set_free(x, y, z, [yaw], [pitch])` → boolean

Puts the free camera at a map point looking along `yaw` and `pitch` (degrees); the view becomes `"free"` (the player moves it on from there).

Parameters: `x` number; `y` number; `z` number; `yaw` number, optional; `pitch` number, optional.

*Since 0.2.22. Permission: `camera`.*

### omsi.camera.set_view

`omsi.camera.set_view(view)` → boolean

Switches the view: `"driver"`, `"pax"`, `"outside"`, `"map"` (the free camera above the bus), `"ego"` (walking), or a `view_*` action of keyboard.cfg.

Parameters: `view` string.

*Since 0.2.22. Permission: `camera`.*

```lua
omsi.camera.set_view("outside")
```

### omsi.camera.set_zoom

`omsi.camera.set_zoom(zoom)` → boolean

The zoom of the view now (its field of view times this, 0.2 to 3).

Parameters: `zoom` number.

*Since 0.2.22. Permission: `camera`.*

### omsi.camera.view

`omsi.camera.view()` → string or nil

The view: `"driver"`, `"pax"`, `"outside"`, `"free"` or `"foot"`.

*Since 0.2.22.*

## Input

`omsi.input.*` reads keys, the mouse and game controllers, and registers hotkeys. Key names are winit's: `KeyH`, `F5`, `Numpad8`, `ShiftLeft`.

### omsi.input.bindings

`omsi.input.bindings([vehicles])` → list of tables

The key bindings: `{action, key}` of the game's keys, or the vehicles' with `true` (`key` as the game writes it: `"Ctrl+D"`).

Parameters: `vehicles` bool, optional.

*Since 0.2.22.*

### omsi.input.controllers

`omsi.input.controllers()` → list of tables

The steering wheels, pedals, joysticks and gamepads: `{name, gamepad, axes, buttons}` (`axes` each axis's value; `buttons` how many it has - the `controller_button` event tells presses).

*Since 0.2.22.*

### omsi.input.hotkey

`omsi.input.hotkey(keys, fn)` → integer id

Runs `fn(key)` when a key combination is pressed: `"F10"`, `"Ctrl+KeyH"`, `"Shift+Alt+Digit1"` (modifiers exact: `"KeyH"` is not `Ctrl+KeyH`). The keys still reach the bus. `cancel(id)` removes it.

Parameters: `keys` string; `fn` function.

*Since 0.2.22.*

```lua
omsi.input.hotkey("Ctrl+KeyH", function()
  omsi.ui.toggle("help")
end)
```

### omsi.input.key_down

`omsi.input.key_down(key)` → boolean

Whether a key is held now (winit's names, as the `key` event: `"KeyW"`, `"ShiftLeft"`, `"F5"`).

Parameters: `key` string.

*Since 0.2.22.*

### omsi.input.keys_down

`omsi.input.keys_down()` → list of strings

Every key held now.

*Since 0.2.22.*

### omsi.input.mouse

`omsi.input.mouse()` → x, y, left, right, middle

The mouse: where it is in the panels' pixels and its buttons held.

*Since 0.2.22.*

```lua
local x, y, left = omsi.input.mouse()
```

## Sound

`omsi.audio.*` plays WAV files from the plugin's folder: everywhere, at a map point, or moving with the bus.

### omsi.audio.play

`omsi.audio.play(file, [opts])` → integer id or nil, reason

Plays a WAV file of the plugin's folder. `opts`: `volume` (1), `pitch` (1), `loop`, `range` (metres heard at full volume, 5), and either `x, y, z` (a sound at a map point) or `on_bus = true` (it moves with the player's bus); none: heard alike everywhere. The game's volume setting applies. A plugin plays 32 at most.

Parameters: `file` string; `opts` table, optional.

*Since 0.2.22. Permission: `audio`.*

```lua
local id = omsi.audio.play("gong.wav", { on_bus = true, volume = 0.8 })
```

### omsi.audio.playing

`omsi.audio.playing(id)` → boolean

Whether a sound of the plugin's still plays.

Parameters: `id` integer.

*Since 0.2.22.*

### omsi.audio.set

`omsi.audio.set(id, opts)` → boolean

Changes a sound of the plugin's: `volume`, `pitch`, `range`, `x, y, z` / `on_bus`.

Parameters: `id` integer; `opts` table.

*Since 0.2.22. Permission: `audio`.*

### omsi.audio.stop

`omsi.audio.stop(id)` → boolean

Stops a sound of the plugin's.

Parameters: `id` integer.

*Since 0.2.22. Permission: `audio`.*

### omsi.audio.stop_all

`omsi.audio.stop_all()` → nil

Stops every sound of the plugin's.

*Since 0.2.22. Permission: `audio`.*

### omsi.audio.volume

`omsi.audio.volume()` → number or nil

The game's volume setting, 0 to 1.

*Since 0.2.22.*

## Screen and panels

`omsi.ui.*` puts panels, notifications and pause-menu lines of the plugin on the screen. The panel table and every element type are described in [Panels](PLUGINS.md#panels). `omsi.ui.version` (currently `8`) tells which panel features the game has.

### omsi.message

`omsi.message(text, [seconds])` → nil

A line of text on the screen (5 seconds when not given).

Parameters: `text` string; `seconds` number, optional.

*Since 0.1.5. Permission: `ui`.*

```lua
omsi.message("Good morning!", 6)
```

### omsi.ui.clear

`omsi.ui.clear()` → nil

Removes every panel of the plugin.

*Since 0.2.21. Permission: `ui`.*

### omsi.ui.dock

`omsi.ui.dock()` → width, room, height, mine, frame

The navigator's board, where a panel with `anchor = "navigator"` goes, in that panel's pixels: its width, the most height there is room for, the height the panel there came to (0 before it is laid out; more than `room`: it is cut off at the bottom), whether that panel is this plugin's (the first one asked for has the board, another waits), and whether it fills the navigator's whole frame (`frame = true`, ui version 6; false under the map, and nothing in an older game). Nothing while the navigator shows no board: it is off, or Shift+N put its lower part away.

*Since 0.3.36.*

```lua
local width, room, height, mine, frame = omsi.ui.dock()
```

### omsi.ui.focus

`omsi.ui.focus(on)` → boolean

`true`: the panels get the mouse (the cursor shows, a click goes to the panel under it and none to the bus); `false`, Esc or a menu of the game gives it back. Returns the new state.

Parameters: `on` bool.

*Since 0.2.21. Permission: `ui`.*

```lua
omsi.input.hotkey("F10", function() omsi.ui.focus(not omsi.ui.focused()) end)
```

### omsi.ui.focused

`omsi.ui.focused()` → boolean

Whether the panels have the mouse.

*Since 0.2.21.*

### omsi.ui.moved

`omsi.ui.moved(panel)` → dx, dy

How far the player dragged a panel from where its table puts it (pixels).

Parameters: `panel` string.

*Since 0.2.22.*

### omsi.ui.panels

`omsi.ui.panels()` → list of strings

The ids of the plugin's panels.

*Since 0.2.22.*

### omsi.ui.remove

`omsi.ui.remove(id)` → boolean

Removes a panel; `true` when there was one.

Parameters: `id` string.

*Since 0.2.21. Permission: `ui`.*

### omsi.ui.remove_menu_item

`omsi.ui.remove_menu_item(id)` → boolean

Takes the plugin's line `id` out of the pause menu; `true` when there was one.

Parameters: `id` string.

*Since 0.3.36. Permission: `ui`.*

### omsi.ui.screen

`omsi.ui.screen()` → width, height, scale

The screen in the panels' pixels, and how many of the screen's own pixels one of them is.

*Since 0.2.21.*

### omsi.ui.set

`omsi.ui.set(id, panel)` → true, or false and the reason

Creates the panel `id` or replaces it; a table that is not right gives `false` and where (`"children[2].size: a number is expected"`). The same table again changes nothing.

Parameters: `id` string; `panel` table.

*Since 0.2.21. Permission: `ui`.*

The panel table, its keys and all element types are in [Panels](PLUGINS.md#panels).

```lua
omsi.ui.set("speed", {
  anchor = "top_left", x = 16, y = 60, width = 220,
  children = {
    { type = "text", id = "kmh", text = "0 km/h", size = 24, weight = "bold" },
  },
})
```

### omsi.ui.set_menu_item

`omsi.ui.set_menu_item(id, label, [icon])` → true, or false and the reason

Puts a line in the game's pause menu, or changes the plugin's line `id`: its label (40 characters at most, shown as it is) and a Material Symbols icon of the game's (`"departure_board"`; the plugins' own icon without one). Choosing it closes the menu and sends `menu_item` with `id` - to open the plugin's panel, say. A plugin has 3 lines at most; they go when it stops.

Parameters: `id` string; `label` string; `icon` string, optional.

*Since 0.3.36. Permission: `ui`.*

```lua
omsi.ui.set_menu_item("board", "Duty board", "departure_board")
omsi.on("menu_item", function(id)
  if id == "board" then omsi.ui.toggle("board") end
end)
```

### omsi.ui.set_theme

`omsi.ui.set_theme(id)` → true, or false and the reason

Switches the game's interface to theme `id` (`""` or `"openomsi"`: openOMSI's own look), as if the player chose it in Options › Interface: it is kept for the next start. Ask the player first; they can switch back there.

Parameters: `id` string.

*Since 0.3.42. Permission: `ui`.*

```lua
if omsi.ui.theme() == "" then omsi.ui.set_theme("myplugin") end
```

### omsi.ui.show

`omsi.ui.show(panel, [on])` → boolean

Shows a panel (or hides it with `false`; it is kept); `false` when there is none.

Parameters: `panel` string; `on` bool, optional.

*Since 0.2.22. Permission: `ui`.*

### omsi.ui.theme

`omsi.ui.theme()` → string

The id of the interface theme in use (`""`: openOMSI's own look).

*Since 0.3.42.*

### omsi.ui.themes

`omsi.ui.themes()` → list of tables

The game's interface themes: each `{ id, name }`, openOMSI's own look first (its id `""`). A theme is a folder with a `theme.json` (`Themes/<id>/`, or a plugin's own folder: see docs/THEMES.md).

*Since 0.3.42.*

### omsi.ui.toast

`omsi.ui.toast(text, [opts])` → true, or false and the reason

A notification card at the top right, newest at the top; it goes after `opts.seconds` (1 to 60, default 5). `opts`: `title`, `icon`, `color`. A plugin shows 8 at most: a ninth makes its oldest go.

Parameters: `text` string; `opts` table, optional.

*Since 0.2.21. Permission: `ui`.*

```lua
omsi.ui.toast("Next stop: Markt", { title = "Line 5", icon = "directions_bus", seconds = 4 })
```

### omsi.ui.toggle

`omsi.ui.toggle(panel)` → boolean

Shows a hidden panel or hides a shown one; whether it shows now.

Parameters: `panel` string.

*Since 0.2.22. Permission: `ui`.*

### omsi.ui.typing

`omsi.ui.typing()` → boolean

Whether the player types into a text field of a plugin now (the keys then go to the field, not to the bus).

*Since 0.2.22.*

### omsi.ui.update

`omsi.ui.update(panel, element, values)` → true, or false and the reason

Changes one element of a panel in place, by its id: `text`, `color`, `value` (a bar, a slider), `segments` (a bar), `checked`, `selected`, `name` (an icon), `values` (a chart), `rows` and `aligns` (a table), `stops` (a stop list), `src` (an image) - cheaper than setting the whole panel again.

Parameters: `panel` string; `element` string; `values` table.

*Since 0.2.22. Permission: `ui`.*

```lua
omsi.ui.update("speed", "kmh", { text = omsi.fmt.speed(omsi.speed()) })
```

### omsi.ui.value

`omsi.ui.value(panel, element)` → any

The value of a checkbox, slider, text field or tabs element now (as `ui_change` gives it).

Parameters: `panel` string; `element` string.

*Since 0.2.22. Permission: `ui`.*

## Cables

`omsi.cable.*` draws thin tubes (cables, ropes, wires) in the 3D world, in map coordinates or attached to the bus. They need the `ui` permission.

### omsi.cable.clear

`omsi.cable.clear()` → integer

Takes every cable of the plugin away; how many went. They go by themselves when the plugin stops.

*Since 0.2.22. Permission: `ui`.*

### omsi.cable.remove

`omsi.cable.remove(id)` → boolean

Takes the cable `id` away; `true` when there was one.

Parameters: `id` string.

*Since 0.2.22. Permission: `ui`.*

### omsi.cable.set

`omsi.cable.set(id, points, [options])` → boolean

Lays the cable `id` or moves it: a thin round tube through `points` (a flat list `{x1, y1, z1, x2, ...}` or a list of `{x, y, z}`, metres), lit and shaded as the scene. `options`: `radius` (m, default 0.006), `color` (`{r, g, b}` or `{r, g, b, a}`, 0 to 1, default dark grey) and `frame`: `"world"` (the map's coordinates, z up; the default) or `"bus"` (the player's bus's model frame: x right, y forward, z up, so a rope tied to the bus moves with it as drawn). Set it again whenever it moves; `false` (and the cable stays as it was) when the plugin would have more than 4096 cables, or more than 65536 points in all its cables together.

Parameters: `id` string; `points` table; `options` table, optional.

*Since 0.2.22. Permission: `ui`.*

```lua
omsi.cable.set("tow", { 0, 6, 0.5,  0, 10, 0.4 }, { frame = "bus", radius = 0.01 })
```

## Events and timers

Register event handlers, timers and watches. The events themselves are listed under [Events](#events).

### omsi.after

`omsi.after(seconds, fn)` → integer id

Runs `fn` once, `seconds` of game time later.

Parameters: `seconds` number; `fn` function.

*Since 0.1.5.*

```lua
omsi.after(5, function() omsi.message("Five seconds later") end)
```

### omsi.cancel

`omsi.cancel(id)` → boolean

Stops a timer, a watch or a hotkey; `true` when there was one.

Parameters: `id` integer.

*Since 0.1.5.*

### omsi.emit

`omsi.emit(event, [...])` → nil

Sends an event to this plugin's own handlers at once (handy between the modules of a bigger plugin); an error of a handler is this call's.

Parameters: `event` string; `...` any, optional.

*Since 0.1.5.*

```lua
omsi.on("fare_changed", function(price) print(price) end)
omsi.emit("fare_changed", 2.8)
```

### omsi.every

`omsi.every(seconds, fn)` → integer id

Runs `fn` every `seconds` of game time.

Parameters: `seconds` number; `fn` function.

*Since 0.1.5.*

```lua
local id = omsi.every(1, function() print(omsi.clock()) end)
-- later: omsi.cancel(id)
```

### omsi.off

`omsi.off(event, fn)` → nil

Removes a handler added with `on`.

Parameters: `event` string; `fn` function.

*Since 0.1.5.*

### omsi.on

`omsi.on(event, fn)` → the function

Adds a handler of an event (see the events); several may hear one event, in the order they were added.

Parameters: `event` string; `fn` function.

*Since 0.1.5.*

```lua
omsi.on("stop_arrive", function(name, id, number, delay)
  omsi.ui.toast(name .. ", " .. omsi.fmt.delay(delay))
end)
```

### omsi.time

`omsi.time()` → number

Seconds of game time since the plugin started (stands still while paused).

*Since 0.1.5.*

### omsi.watch

`omsi.watch(kind, name, [fn])` → integer id

Runs `fn(new, old)` whenever a value changes: `watch(name, fn)` a variable of the bus, `watch(kind, name, fn)` of kind `"var"`, `"str"`, `"sys"` or `"info"` (a key of `info()`).

Parameters: `kind` string; `name` string; `fn` function, optional.

*Since 0.1.5.*

```lua
omsi.watch("Velocity", function(new, old) print(old, "->", new) end)
omsi.watch("info", "next_stop", function(new) print("next:", new) end)
```

## The plugin itself

`omsi.plugin.*` is about the plugin: its name, files, settings, permissions, and messages to other plugins.

### omsi.plugin.broadcast

`omsi.plugin.broadcast(topic, [data])` → nil

Sends a message to every other plugin loaded.

Parameters: `topic` string; `data` any, optional.

*Since 0.2.22.*

```lua
omsi.plugin.broadcast("delay", omsi.duty.delay())
```

### omsi.plugin.disable

`omsi.plugin.disable([reason])` → nil

Switches the plugin off until its file changes or the game starts again (its `stop` comes).

Parameters: `reason` string, optional.

*Since 0.2.22.*

### omsi.plugin.errors

`omsi.plugin.errors()` → integer

How many errors the plugin had (at 10 it is switched off).

*Since 0.2.22.*

### omsi.plugin.files

`omsi.plugin.files([dir])` → list of tables

The files of the plugin's own folder (or a folder in it): `{name, dir, size}`.

Parameters: `dir` string, optional.

*Since 0.2.22.*

### omsi.plugin.list

`omsi.plugin.list()` → list of strings

The names of the plugins loaded, this one too.

*Since 0.2.22.*

### omsi.plugin.name

`omsi.plugin.name()` → string

The plugin's name: its file's, or its folder's for a `main.lua`.

*Since 0.2.22.*

### omsi.plugin.permissions

`omsi.plugin.permissions()` → list of strings

The permissions the plugin has (native process access additionally requires player opt-in).

*Since 0.2.22.*

### omsi.plugin.read

`omsi.plugin.read(path)` → text, or nil and the reason

A file of the plugin's own folder (a table of stops, a translation): read only, relative to the folder.

Parameters: `path` string.

*Since 0.2.22.*

### omsi.plugin.send

`omsi.plugin.send(to, topic, [data])` → nil

Sends a message to another plugin (by its name): it hears `message(from, topic, data)` in its next frame. The data is numbers, texts, booleans or tables of them.

Parameters: `to` string; `topic` string; `data` any, optional.

*Since 0.2.22.*

```lua
omsi.plugin.send("dispatcher", "delay", omsi.duty.delay())
```

### omsi.plugin.set_setting

`omsi.plugin.set_setting(key, value)` → boolean

Changes a setting (held to its range; saved); `true` when it took the value.

Parameters: `key` string; `value` any.

*Since 0.2.22.*

### omsi.plugin.setting

`omsi.plugin.setting(key)` → any

A setting's value (`nil`: no such setting).

Parameters: `key` string.

*Since 0.2.22.*

### omsi.plugin.settings

`omsi.plugin.settings(settings, [title])` → table

Declares the plugin's settings: a list of `{key, type, label, default}` with `type` `"bool"`, `"number"` (`min`, `max`, `step`), `"text"` or `"choice"` (`choices`, a list of texts). The values the player chose before come back (as a table key -> value); the game makes a settings panel of them (`plugin.show_settings`), saves them in the data folder and sends `setting(key, value)` when one changes.

Parameters: `settings` table; `title` string, optional.

*Since 0.2.22.*

```lua
local s = omsi.plugin.settings({
  { key = "volume", type = "number", label = "Volume", default = 50, min = 0, max = 100, step = 5 },
  { key = "unit", type = "choice", label = "Speed in", choices = { "km/h", "mph" }, default = "km/h" },
}, "My plugin")
print(s.volume, s.unit)
```

### omsi.plugin.show_settings

`omsi.plugin.show_settings([on])` → boolean

Shows the plugin's settings panel (or hides it with `false`); it can be dragged and closed while the panels have the mouse (`ui.focus`). `false` when the plugin declared no settings.

Parameters: `on` bool, optional.

*Since 0.2.22. Permission: `ui`.*

## Storage and files

`omsi.storage.*` is a key/value store, `omsi.files.*` reads and writes files. Both live only in the plugin's data folder. Paths are relative to that folder; `..` and absolute paths are refused.

### omsi.files.append

`omsi.files.append(path, data)` → true, or false and the reason

Adds to the end of a file of the data folder (a log, a CSV of trips).

Parameters: `path` string; `data` string.

*Since 0.2.22. Permission: `storage`.*

```lua
omsi.files.append("trips.csv", "136,3,+30\n")
```

### omsi.files.delete

`omsi.files.delete(path)` → boolean

Removes a file or an empty folder of the data folder.

Parameters: `path` string.

*Since 0.2.22. Permission: `storage`.*

### omsi.files.dir

`omsi.files.dir()` → string

Where the data folder is on this computer (to tell the player; the plugin reaches it with relative paths only).

*Since 0.2.22. Permission: `storage`.*

### omsi.files.exists

`omsi.files.exists(path)` → boolean

Whether the data folder has this file or folder.

Parameters: `path` string.

*Since 0.2.22. Permission: `storage`.*

### omsi.files.list

`omsi.files.list([dir])` → list of tables, or nil and the reason

The entries of the data folder (or a folder in it): `{name, dir, size}`.

Parameters: `dir` string, optional.

*Since 0.2.22. Permission: `storage`.*

### omsi.files.mkdir

`omsi.files.mkdir(path)` → boolean

Makes a folder (and those above it) in the data folder.

Parameters: `path` string.

*Since 0.2.22. Permission: `storage`.*

### omsi.files.read

`omsi.files.read(path)` → text, or nil and the reason

A file of the plugin's data folder (`path` relative to it; `..` and absolute paths are refused).

Parameters: `path` string.

*Since 0.2.22. Permission: `storage`.*

```lua
local text, why = omsi.files.read("notes.txt")
```

### omsi.files.write

`omsi.files.write(path, data)` → true, or false and the reason

Writes a file of the data folder (its folders are made); at most 16 MB at once and 256 MB in all.

Parameters: `path` string; `data` string.

*Since 0.2.22. Permission: `storage`.*

### omsi.storage.all

`omsi.storage.all()` → table

Everything stored, as one table.

*Since 0.2.22. Permission: `storage`.*

### omsi.storage.clear

`omsi.storage.clear()` → nil

Removes every key.

*Since 0.2.22. Permission: `storage`.*

### omsi.storage.delete

`omsi.storage.delete(key)` → boolean

Removes a key; `true` when it was there.

Parameters: `key` string.

*Since 0.2.22. Permission: `storage`.*

### omsi.storage.get

`omsi.storage.get(key)` → any

A value the plugin stored, or `nil`. The storage is the plugin's own and survives the session (kept as `storage.json` in its data folder).

Parameters: `key` string.

*Since 0.2.22. Permission: `storage`.*

### omsi.storage.keys

`omsi.storage.keys()` → list of strings

The keys stored, in the order they were first set.

*Since 0.2.22. Permission: `storage`.*

### omsi.storage.save

`omsi.storage.save()` → nil

Writes the storage now (it is written by itself when the game ends).

*Since 0.2.22. Permission: `storage`.*

### omsi.storage.set

`omsi.storage.set(key, value)` → nil

Stores a value under a key: a number, text, boolean or a table of them (`nil` removes it). It is written when the game ends, the plugin is loaded again, or `storage.save()` is called.

Parameters: `key` string; `value` any.

*Since 0.2.22. Permission: `storage`.*

```lua
omsi.storage.set("best_trip", 812)
local best = omsi.storage.get("best_trip") or 0
```

## External programs

`omsi.process.*` starts native helper programs from the plugin's own folder. Every helper needs the player's approval first, see [Native helper programs](PLUGINS.md#native-helper-programs).

### omsi.process.exit_code

`omsi.process.exit_code(handle)` → code or nil, state or reason

Nonblocking exit code lookup (queued verification also counts as running): `nil, "running"` while running; `code, "exited"` after normal exit. A nonzero code is retained. Signal termination returns `nil, "exited"`; inspect `process.status` for details. Preparation failure returns `nil, "failed", reason`. An unknown handle returns `nil` and a reason.

Parameters: `handle` integer.

*Since unreleased. Permission: `process`.*

### omsi.process.forget

`omsi.process.forget(handle)` → true, or nil and reason

Release a completed or failed process handle and its retained result. A running process cannot be forgotten. On Windows, closing its job also terminates any remaining descendants.

Parameters: `handle` integer.

*Since unreleased. Permission: `process`.*

### omsi.process.kill

`omsi.process.kill(handle)` → true, or nil and reason

Cancel a queued start or request termination of a plugin-owned direct child without waiting. Poll `process.status` for completion; Windows also terminates the job descendants, Unix targets only the direct child.

Parameters: `handle` integer.

*Since unreleased. Permission: `process`.*

### omsi.process.start

`omsi.process.start(file, [args], [options])` → handle, or nil and reason

Queue a native helper without waiting; return a reserved handle immediately. The interactive game asks the player with Allow once, Always allow or Deny unless the exact content was already approved. Embedded hosts without a permission broker remain default-deny. Verification and OS process creation run on two shared workers with at most 32 globally queued starts. The 32-handle plugin limit (including queued and failed starts) is checked before admission. A full queue returns nil and a reason. Accepted starts can fail later: inspect status.error or exit_code/wait returning nil, failed, reason. `file` must be a relative path inside the plugin folder. Approval is bound to the entry, folder snapshot and helper SHA256. Allow once covers one launch; Always allow saves consent in the host user settings; Deny blocks all native helpers for this canonical plugin entry for the current session, including renamed helpers or changed content or arguments. Changed content needs new approval. While the dialog is open, preparation workers and child cleanup keep running. The entry, executable-content snapshot (including importable Lua modules and native libraries), and helper hashes are rechecked on the worker immediately before each start; code changes revoke consent, while ordinary save/data writes do not. `args` are separate literal strings without shell expansion. Options: `cwd` and `env` (string overrides, other variables inherited, except OMSI_PLUGIN_PROCESS_ALLOW is always removed). Standard streams are disconnected. Windows helpers must be .exe files. At most 32 handles per plugin; forget completed handles. Stop, disable and reload terminate children; Windows job objects also terminate descendants and survive abrupt game exit, Linux uses a parent-death signal for direct children. No native helper runs before player approval. OMSI_PLUGIN_PROCESS_ALLOW can optionally name an absolute JSON grant manifest for preapproved content. Packages must also declare process. Unload/reload cancels unanswered requests. Approved helpers run with the game account's permissions.

Parameters: `file` string; `args` table, optional; `options` table, optional.

*Since unreleased. Permission: `process`.*

```lua
local h, why = omsi.process.start("bin/helper.exe", { "--fast" })
if h then
  local code, state = omsi.process.wait(h)
  omsi.process.forget(h)
end
```

### omsi.process.status

`omsi.process.status(handle)` → table, or nil and reason

Nonblocking status: `{pid, starting, awaiting_permission, running, exited, failed, error, code, success, signal}`. While verification/start is pending, starting and running are true and pid is nil. Failed preparation sets failed/error without an exit code. While running `code`, `success` and `signal` are nil. After exit `code` is the exit code (including nonzero); termination by a signal has no code, and `signal` is available on Unix. Results remain readable until forgotten.

Parameters: `handle` integer.

*Since unreleased. Permission: `process`.*

### omsi.process.wait

`omsi.process.wait(handle)` → exit code or nil, state or reason

Lua: suspend this plugin through queued verification/start and until its direct child exits, then return `code, "exited"` (nonzero codes retained; nil code on signal exit). The game and other plugins keep running; no game-thread wait occurs. Works in the main chunk, start/frame/event handlers and timers. Queued callbacks resume afterwards (limit 256), and normal frames/timers pause during the wait. Stop/reload cancels the continuation and terminates the child. A nested `emit` callback or manually resumed coroutine cannot suspend. Preparation failure returns `nil, "failed", reason`. Other language bindings should poll `process.exit_code` instead.

Parameters: `handle` integer.

*Since unreleased. Permission: `process`.*

## Local UDP

`omsi.send` talks to other programs on the same computer. It never reaches the network.

### omsi.send

`omsi.send(port, data)` → true, or false and the reason

Sends `data` as one UDP datagram to `127.0.0.1:port`: to another program on this computer, never over the network. Not sent when the port is below 1024 or one of the game's multiplayer ports (27015-27024), the message is longer than 8 KB, or the plugin sent 100 in the last second.

Parameters: `port` integer; `data` string.

*Since 0.2.21. Permission: `network_local`.*

```lua
omsi.every(0.5, function()
  omsi.send(47800, omsi.json.encode({ speed = omsi.speed() }))
end)
```

## Read-only WebSocket feeds

`omsi.ws.*` listens to `wss://` feeds from hosts the player allowed in `websocket-hosts.txt` in the plugins folder. The connection runs on its own thread and nothing is sent to the server.

### omsi.ws.close

`omsi.ws.close(slot)` → true

Closes the feed of `slot` and frees it for another one.

Parameters: `slot` integer.

*Since 0.2.22. Permission: `network_remote`.*

### omsi.ws.open

`omsi.ws.open(url)` → the feed's slot (0-2), or an error

Opens a read-only WebSocket feed to `url` in the first free slot. Only `wss://` addresses are opened, with no port of its own, no user name or password and no literal IP address, and only to a host the player allowed in `websocket-hosts.txt` in the plugins folder; the plugin never sends anything, it only drains what the server pushes. A plugin holds 3 feeds at the same time.

Parameters: `url` string.

*Since 0.2.22. Permission: `network_remote`.*

### omsi.ws.poll

`omsi.ws.poll(slot)` → a list of the text messages that arrived

Takes what arrived in the feed of `slot` since the last call (oldest first, at most a few dozen are kept: a slow plugin loses the oldest). Each message is the text of one WebSocket message, at most 64 KB.

Parameters: `slot` integer.

*Since 0.2.22. Permission: `network_remote`.*

### omsi.ws.state

`omsi.ws.state(slot)` → `connecting`, `open`, `reconnecting`, `failed` or `closed`

What the feed of `slot` is doing: it connects on its own thread, so the game never waits, and a connection that breaks is opened again after 5 seconds.

Parameters: `slot` integer.

*Since 0.2.22. Permission: `network_remote`.*

## LAN games

`omsi.lan.*` reads the LAN session and sends short texts to the same plugin on the other players' games.

### omsi.lan.active

`omsi.lan.active()` → boolean

Whether this game is in a LAN session.

*Since 0.2.22.*

### omsi.lan.chat

`omsi.lan.chat(text)` → true, or false and the reason

Says a line in the session's chat, as the player would (every player sees it; at most one a second).

Parameters: `text` string.

*Since 0.2.22. Permission: `lan`.*

### omsi.lan.me

`omsi.lan.me()` → id, name, host

This player in the session: its id (the host is 1), its name, whether it hosts.

*Since 0.2.22.*

### omsi.lan.players

`omsi.lan.players()` → list of tables

The other players of the session: `{id, name, host, bus, line, tour, x, y, z, heading, speed, on_foot, passengers}`.

*Since 0.2.22.*

```lua
for _, p in ipairs(omsi.lan.players()) do print(p.name, p.line) end
```

### omsi.lan.send

`omsi.lan.send(to, text)` → true, or false and the reason

Sends a short text (at most 120 characters, no `|`) to the same plugin on another player's game (`to` its id; 0: every other player); it hears `lan_message(from, text)`. A player sends at most about ten a second; a game without the plugin ignores them.

Parameters: `to` integer; `text` string.

*Since 0.2.22. Permission: `lan`.*

```lua
omsi.lan.send(0, "hello")  -- this plugin on every other player's game
```

## The game

`omsi.game.*`, `omsi.info` and the log functions: the game's state, version, settings, commands and logging.

### omsi.command

`omsi.command(name)` → boolean

Does what a line of the game menu does: `refuel`, `wash`, `repair`, `shot`, `save`, `load`, `weather`, `later`, `earlier`, `info`, `timetable`, `reset`, `couple`, `uncouple`; `true` when the game knows it (it runs after the frame).

Parameters: `name` string.

*Since 0.1.10. Permission: `world_write`.*

### omsi.debug

`omsi.debug([...])` → nil

A line of the log's debug level (shown with `RUST_LOG=debug`).

Parameters: `...` any, optional.

*Since 0.2.22.*

### omsi.error

`omsi.error([...])` → nil

The same as an error line (the plugin goes on).

Parameters: `...` any, optional.

*Since 0.2.22.*

### omsi.game.action

`omsi.game.action(name)` → boolean

A game action of keyboard.cfg's `[game]` (`sim_pause`, `view_set_map`, `view_toggle_informationdisplay`, `view_set_schedule`, ...), as its key does; `true` when the game knows it.

Parameters: `name` string.

*Since 0.2.22. Permission: `world_write`.*

### omsi.game.api

`omsi.game.api()` → integer

The version of the plugin interface (`api_abi` of an `.oop`; raised only when something is taken away or changes).

*Since 0.2.22.*

### omsi.game.fps

`omsi.game.fps()` → number or nil

Frames a second now.

*Since 0.2.22.*

### omsi.game.has

`omsi.game.has(name)` → boolean

Whether this game has an API function (`"weather.set"`): for a plugin that should also run in an older openOMSI.

Parameters: `name` string.

*Since 0.2.22.*

```lua
if omsi.game.has("weather.set") then omsi.weather.set({ temperature = 20 }) end
```

### omsi.game.menu_open

`omsi.game.menu_open()` → boolean

Whether the game menu is open.

*Since 0.2.22.*

### omsi.game.notify

`omsi.game.notify(text, [kind], [seconds])` → boolean

A notification of the game's own (the cards the server's messages use): `kind` `"info"`, `"warning"` or `"alert"`.

Parameters: `text` string; `kind` string, optional; `seconds` number, optional.

*Since 0.2.22. Permission: `ui`.*

### omsi.game.platform

`omsi.game.platform()` → string

The system the game runs on: `"windows"`, `"macos"`, `"linux"`, `"android"`.

*Since 0.2.22.*

### omsi.game.screenshot

`omsi.game.screenshot()` → string or nil

Takes a screenshot with the next frame, as the camera key does; the file it goes to (the `screenshot` event says when it is there).

*Since 0.2.22. Permission: `ui`.*

### omsi.game.settings

`omsi.game.settings()` → table

The settings a plugin may read: `graphics` (`"vanilla"`, `"vanilla_plus"`, `"enhanced"`), `language` (the cockpit's, `"ENG"`), `ui_language`, `ui_scale`, `volume`, `fov`, `render_scale`, `max_fps`, `fullscreen`, `vsync`, `msaa`, `shadows`, `time_speed`, `units` (`"metric"`: speeds are km/h everywhere), ...

*Since 0.2.22.*

### omsi.game.stats

`omsi.game.stats()` → table

This session's counts, as the personnel file has them: `km`, `stops_served`, `stops_skipped`, `crashes`, `heavy_crashes`, `pedestrians`, `tickets`, `cash`, `passengers`, ...

*Since 0.2.22.*

### omsi.game.version

`omsi.game.version()` → string

The game's version (`"0.2.22"`).

*Since 0.2.22.*

### omsi.info

`omsi.info()` → table

What the game is doing: `map`, `clock` (seconds since midnight), `day`, `year`, `view`, `paused`, `on_foot`, `multiplayer`, `traffic`, `speed`, `delay`, `map_path`, `version`; with a bus also `tile_x`, `tile_y`, `tile_pos_x`, `tile_pos_y`, `heading`, `vehicle_manufacturer`, `vehicle_model`, `destination`, `passengers`; `crashes`, `heavy_crashes`, `pedestrians_hit`; `situation`; on a duty also `line`, `tour`, `trip`, `trips`, `trip_name`, `terminus`, `stops`, `trip_done`, `next_stop`, `next_stop_number`, `next_stop_arrival`, `next_stop_departure`, `next_stop_id`, `at_stop`, `previous_stop`, `previous_stop_id`, `next_stop_distance`, `next_stop_distance_nav`, `previous_stop_distance` (see the plugin docs for each).

*Since 0.1.10.*

| Key | Value |
| --- | --- |
| `map` | the map's name |
| `map_path` | the map's `global.cfg` |
| `clock` | seconds since midnight |
| `day`, `year` | the game's date |
| `view` | the view (`driver`, `pax`, `outside`, `free`, `foot`) |
| `paused`, `on_foot`, `multiplayer` | booleans |
| `traffic` | number of AI vehicles |
| `speed` | km/h |
| `delay` | seconds, late positive |
| `version` | openOMSI's version |
| `situation` | the situation file the game started from; `nil` for a new game |

With a bus:

| Key | Value |
| --- | --- |
| `tile_x`, `tile_y` | its tile, numbered as `global.cfg`'s `[map]` list |
| `tile_pos_x`, `tile_pos_y` | metres inside that tile (x east, y north) |
| `heading` | degrees clockwise from north |
| `vehicle_manufacturer`, `vehicle_model` | the parts of its name |
| `destination` | the terminus the bus shows |
| `passengers` | people aboard |
| `crashes`, `heavy_crashes`, `pedestrians_hit` | this session, as the personnel file counts them |

On a duty:

| Key | Value |
| --- | --- |
| `line`, `tour` | the duty |
| `trip`, `trips` | the trip's number in the duty, and how many there are |
| `trip_name`, `terminus` | the timetable's name of the trip, its terminus |
| `stops` | stops of the trip |
| `trip_done` | `true` once the trip is over (see below) |
| `next_stop`, `next_stop_number` | name, and number from 1 |
| `next_stop_arrival`, `next_stop_departure` | timetable times |
| `next_stop_id` | the stop's map object id |
| `at_stop` | `true` while the bus stands at the next stop |
| `previous_stop`, `previous_stop_id` | the last stop before the next one; none before the first |
| `next_stop_distance`, `previous_stop_distance` | straight-line metres, where the stop's place is known |
| `next_stop_distance_nav` | the navigator's route distance in metres |

`trip_done` is `true` once the bus has reached the trip's last stop, the stop was skipped, or a saved game was left there. The duty moves on to the next trip only a minute before it leaves.

A stop's name can stand for two stops; its id cannot.

`next_stop_distance_nav` is `nil` when both the navigator and the route arrows are off (the pause menu included). While the navigator says "Recalculating route", it keeps the last value.

```lua
local i = omsi.info()
print(i.map, i.line, i.next_stop, i.delay)
```

### omsi.info_value

`omsi.info_value(key)` → any

One value of `info()` without building the whole table: cheaper for a plugin that reads one or two every frame.

Parameters: `key` string.

*Since 0.2.22.*

```lua
local delay = omsi.info_value("delay")
```

### omsi.log

`omsi.log([...])` → nil

A line in `game.log`, tagged `[lua <name>]` (`print` does the same).

Parameters: `...` any, optional.

*Since 0.1.5.*

```lua
omsi.log("trip done", 136, 3)
```

### omsi.warn

`omsi.warn([...])` → nil

The same as a warning.

Parameters: `...` any, optional.

*Since 0.1.5.*

## Helpers

`omsi.fmt.*` formats numbers, times and distances, `omsi.json.*` reads and writes JSON, `omsi.vec.*` does map geometry, `omsi.util.*` gives real time and random numbers. None of them touch the game.

### omsi.fmt.clock

`omsi.fmt.clock(seconds, [with_seconds])` → string

Seconds since midnight as `"HH:MM"` (`"HH:MM:SS"` with `true`); past midnight wraps.

Parameters: `seconds` number; `with_seconds` bool, optional.

*Since 0.2.22.*

```lua
omsi.fmt.clock(27000)        --> "07:30"
omsi.fmt.clock(27015, true)  --> "07:30:15"
```

### omsi.fmt.delay

`omsi.fmt.delay(seconds)` → string

A delay as a timetable display shows it: `"+2:30"` late, `"-0:45"` early, `"0:00"`.

Parameters: `seconds` number.

*Since 0.2.22.*

```lua
omsi.fmt.delay(150)  --> "+2:30"
```

### omsi.fmt.distance

`omsi.fmt.distance(metres)` → string

A distance as `"350 m"` or `"2.4 km"`.

Parameters: `metres` number.

*Since 0.2.22.*

### omsi.fmt.duration

`omsi.fmt.duration(seconds)` → string

A length of time as people read it: `"45 s"`, `"3 min 05 s"`, `"1 h 20 min"` (negative with a minus).

Parameters: `seconds` number.

*Since 0.2.22.*

### omsi.fmt.money

`omsi.fmt.money(amount, [symbol])` → string

An amount with two decimals and a currency symbol after it (`"12.50 €"`; the game knows no currency of its own).

Parameters: `amount` number; `symbol` string, optional.

*Since 0.2.22.*

### omsi.fmt.number

`omsi.fmt.number(x, [decimals], [separator])` → string

A number with `decimals` (0) and a thousands `separator` (`","`, `" "`; none by default).

Parameters: `x` number; `decimals` integer, optional; `separator` string, optional.

*Since 0.2.22.*

### omsi.fmt.pad

`omsi.fmt.pad(text, width, [right])` → string

A text made `width` characters long with spaces (on the left with `right` = true, to line numbers up), or cut to it.

Parameters: `text` string; `width` integer; `right` bool, optional.

*Since 0.2.22.*

### omsi.fmt.speed

`omsi.fmt.speed(kmh, [unit])` → string

A speed as `"42 km/h"`, or in `"mph"` or `"m/s"`.

Parameters: `kmh` number; `unit` string, optional.

*Since 0.2.22.*

### omsi.fmt.split

`omsi.fmt.split(text, [separator])` → list of strings

A text cut at every `separator` (`","` by default; plainly, no patterns).

Parameters: `text` string; `separator` string, optional.

*Since 0.2.22.*

### omsi.fmt.trim

`omsi.fmt.trim(text)` → string

A text without the spaces at its ends.

Parameters: `text` string.

*Since 0.2.22.*

### omsi.json.decode

`omsi.json.decode(text)` → value, or nil and the reason

Reads JSON text: objects and arrays become tables, `null` nil.

Parameters: `text` string.

*Since 0.2.22.*

### omsi.json.encode

`omsi.json.encode(value, [pretty])` → string

A value as JSON text (a list as an array, a table with keys as an object; `pretty`: indented).

Parameters: `value` any; `pretty` bool, optional.

*Since 0.2.22.*

```lua
local text = omsi.json.encode({ line = "136", stops = { 1, 2 } })
local t = omsi.json.decode(text)
```

### omsi.util.date

`omsi.util.date([seconds])` → table

A real time (`util.now()` by default) as `{year, month, day, hour, minute, second, weekday}` in UTC (`weekday` 1 Monday).

Parameters: `seconds` number, optional.

*Since 0.2.22.*

### omsi.util.ms

`omsi.util.ms()` → number

Milliseconds of real time since the game started: for timing a plugin's own work.

*Since 0.2.22.*

### omsi.util.now

`omsi.util.now()` → number

The real time: seconds since 1970 (UTC), with fractions.

*Since 0.2.22.*

### omsi.util.random

`omsi.util.random([min], [max])` → number

A random number: 0 to 1 without arguments, else a whole number from `min` to `max` (as `math.random`, but not repeating the same row in every plugin).

Parameters: `min` number, optional; `max` number, optional.

*Since 0.2.22.*

### omsi.vec.angle_diff

`omsi.vec.angle_diff(a, b)` → number

The turn from heading `a` to heading `b`, -180 to 180 degrees (right positive).

Parameters: `a` number; `b` number.

*Since 0.2.22.*

### omsi.vec.bearing

`omsi.vec.bearing(x1, y1, x2, y2)` → number

The heading from one map point to another.

Parameters: `x1` number; `y1` number; `x2` number; `y2` number.

*Since 0.2.22.*

```lua
local h = omsi.vec.bearing(x1, y1, x2, y2)
```

### omsi.vec.clamp

`omsi.vec.clamp(x, min, max)` → number

`x` held between `min` and `max`.

Parameters: `x` number; `min` number; `max` number.

*Since 0.2.22.*

### omsi.vec.distance

`omsi.vec.distance(x1, y1, x2, y2)` → number

Metres between two map points (on the ground: x and y).

Parameters: `x1` number; `y1` number; `x2` number; `y2` number.

*Since 0.2.22.*

### omsi.vec.distance3

`omsi.vec.distance3(x1, y1, z1, x2, y2, z2)` → number

Metres between two points in space.

Parameters: `x1` number; `y1` number; `z1` number; `x2` number; `y2` number; `z2` number.

*Since 0.2.22.*

### omsi.vec.dot

`omsi.vec.dot(x1, y1, x2, y2)` → number

The dot product of two 2D vectors.

Parameters: `x1` number; `y1` number; `x2` number; `y2` number.

*Since 0.2.22.*

### omsi.vec.heading

`omsi.vec.heading(dx, dy)` → number

The heading of a direction on the map, degrees clockwise from north (0 to 360), as the game's headings.

Parameters: `dx` number; `dy` number.

*Since 0.2.22.*

### omsi.vec.length

`omsi.vec.length(x, y, [z])` → number

The length of a vector (2D, or 3D with `z`).

Parameters: `x` number; `y` number; `z` number, optional.

*Since 0.2.22.*

### omsi.vec.lerp

`omsi.vec.lerp(a, b, t)` → number

From `a` to `b` by `t` (0 to 1, not held to it).

Parameters: `a` number; `b` number; `t` number.

*Since 0.2.22.*

### omsi.vec.normalize

`omsi.vec.normalize(x, y, [z])` → x, y, z

The vector made 1 long (0 stays 0).

Parameters: `x` number; `y` number; `z` number, optional.

*Since 0.2.22.*

### omsi.vec.rotate

`omsi.vec.rotate(x, y, degrees)` → x, y

A vector turned clockwise by `degrees` (as headings turn).

Parameters: `x` number; `y` number; `degrees` number.

*Since 0.2.22.*

### omsi.vec.to_local

`omsi.vec.to_local(x, y, ox, oy, heading)` → right, ahead

A map point seen from a place facing `heading`: metres to the right and ahead of it.

Parameters: `x` number; `y` number; `ox` number; `oy` number; `heading` number.

*Since 0.2.22.*

```lua
local x, y, _, heading = omsi.position()
local right, ahead = omsi.vec.to_local(px, py, x, y, heading)
```

## Events

Register a handler with `omsi.on(name, fn)` or define a global function `on_<name>`. The handler gets the arguments listed after the name. Events that compare two frames (doors, gears, stops, the light ahead) are only worked out while some plugin listens.

```lua
function on_frame(dt) end            -- a global handler
omsi.on("door", function(door, open) -- or as many as you like
  print(door, open)
end)
```

### Plugin lifecycle

- **`start`**`()`: Right after the plugin was loaded (also after a reload). *Since 0.1.5.*
- **`stop`**`()`: The game ends, or the plugin is about to be loaded again. *Since 0.1.5.*
- **`frame`**`(dt)`: Every frame of the game, after the bus's own scripts; not while paused. `dt` is the frame's seconds of game time. *Since 0.1.5.*
- **`pause`**`()`: The game was paused (the plugins stand still until `resume`; nothing else comes in between). *Since 0.2.22.*
- **`resume`**`()`: The game goes on after a pause. *Since 0.2.22.*

### Vehicle

- **`vehicle`**`(name)`: The player got into a vehicle, changed it, or left it (`nil`). *Since 0.1.5.*
- **`engine_start`**`()`: The player's bus's engine started running. *Since 0.2.22.*
- **`engine_stop`**`()`: Its engine stopped. *Since 0.2.22.*
- **`gear`**`(new, old)`: The gear engaged changed (-1 reverse, 0 neutral), where the bus has a gearbox it shows. *Since 0.2.22.*
- **`door`**`(door, open)`: A door leaf of the player's bus opened (`true`) or closed: its number from 1, front to back. *Since 0.2.22.*
- **`doors`**`(open)`: The bus's doors opened (`true`: one or more open) or were all closed. *Since 0.2.22.*
- **`indicator`**`(new, old)`: The indicators changed: `"off"`, `"left"`, `"right"` or `"hazard"`. *Since 0.2.22.*
- **`headlights`**`(new, old)`: The headlights changed: 0 off, 1 side lights, 2 dipped, 3 high beam. *Since 0.2.22.*
- **`horn`**`(down)`: The horn started (`true`) or stopped. *Since 0.2.22.*
- **`handbrake`**`(on)`: The parking brake was put on (`true`) or released. *Since 0.2.22.*
- **`stop_request`**`(on)`: A passenger asked to stop (`true`), or the request went out. *Since 0.2.22.*
- **`destination`**`(new, old)`: The destination the bus shows changed. *Since 0.2.22.*
- **`coupled`**`(parts, old)`: Something was coupled to or uncoupled from the bus: the parts behind it now. *Since 0.2.22.*
- **`crash`**`(energy_kj, speed_kmh)`: The player's bus crashed: every crash, also one the same as the last (the screen's "Crash: 136 kJ"); above 50 kJ it is a heavy one. *Since 0.2.21.*
- **`pedestrian`**`(count)`: The bus knocked people down. *Since 0.2.21.*
- **`jolt`**`(along, across, speed_kmh, passengers)`: The bus braked, sped up or cornered hard enough to cost driving rating (m/s², signed); at most one a second. *Since 0.2.21.*
- **`service`**`(kind, by, amount)`: The player's bus was serviced or moved. `kind`: `"refuel"` (amount: litres put in), `"wash"` (amount: the dirt left), `"repair"` (amount: the game minutes it took), `"reset"` or `"teleport"`; `by`: `"player"`, `"plugin"`, `"host"` or `"game"`. *Since 0.2.21.*
- **`ai_collision`**`(id, energy_kj)`: The player's bus hit an AI vehicle (its id, as `traffic.list` gives it) with this energy. *Since 0.2.22.*

### Duty and stops

- **`duty`**`(line, tour)`: A line and tour were taken (or given up: `nil`). *Since 0.1.10.*
- **`duty_start`**`(line, tour)`: A duty was taken. *Since 0.2.22.*
- **`duty_end`**`(line, tour)`: The duty was given up (or another taken: `duty_end` of the old one comes first). *Since 0.2.22.*
- **`trip_start`**`(trip, name, terminus)`: The duty moved on to another trip: its number in the duty (from 1), the timetable's name of the trip and its terminus. *Since 0.2.22.*
- **`trip_done`**`(trip, how, driving, comfort, tickets)`: A trip of the duty ended, once: its number in the duty, `"arrived"`, `"skipped"` or `"given_up"`, then its ratings in per cent (driving, comfort, ticket selling). *Since 0.2.21.*
- **`next_stop`**`(new, old)`: The duty's next stop changed, also to one of the same name (`omsi.info().next_stop_number` tells them apart). *Since 0.1.10.*
- **`stop_arrive`**`(name, id, number, delay)`: The bus came to the duty's next stop (within 25 m of it): its name, map object id, number in the trip (from 1) and the delay in seconds (late positive). *Since 0.2.22.*
- **`stop_depart`**`(name, id, number, delay)`: The bus left the stop it stood at (more than 35 m from it, or the duty moved on): the same values, the delay as it left. *Since 0.2.22.*
- **`stops_skipped`**`(count, due_at, now_at)`: The duty jumped ahead: the bus passed stops of its trip without stopping (or was moved) and is now at a later one; the stops are numbered in the trip from 1. *Since 0.2.21.*

### Passengers and tickets

- **`passengers`**`(count, old)`: The number of passengers aboard the player's bus changed. *Since 0.2.22.*
- **`passenger_board`**`(count, stop_id)`: People got into the player's bus: how many, and the stop they waited at (its map object id, `nil` when none). *Since 0.2.22.*
- **`passenger_alight`**`(count)`: People got out of the player's bus. *Since 0.2.22.*
- **`passenger_ticket`**`(passenger)`: Somebody boarding the player's bus has decided about their ticket, on their way to the door - they wait in the doorway for this frame, so it can still change before they get in and ask for it at the cash desk: `{id, age, kind, ticket, name, price, stop, stop_name, destination, stop_number, destination_number}` - `kind` `"none"`, `"stamp"` or `"buy"`, `ticket` the ticket pack's (from 1; 0 none fits their age - the desk then asks for the pack's first) with the `name` and `price` they will ask for, `stop` the map object id they board at and `stop_name` its timetable name, `destination` the stop they ride to, and where both are in the duty's trip (from 1, `nil` where the trip does not have them). `people.set_ticket(id, ...)` changes it. *Since 0.3.*
- **`ticket_sold`**`(name, price)`: A ticket was sold at the cash desk: its name and price as the bus's ticket list has them. *Since 0.2.21.*

### Road and world

- **`tile`**`(x, y, old_x, old_y)`: The player's bus drove onto another tile of the map (numbered as global.cfg's `[map]` list). *Since 0.2.22.*
- **`speed_limit`**`(kmh, old)`: The speed limit of the lane the bus drives on changed. *Since 0.2.22.*
- **`light_ahead`**`(aspect, distance)`: The traffic light ahead of the bus (within 80 m) changed or a new one came: its aspect (`"red"`, `"red_yellow"`, `"green"`, `"green_yellow"`, `"yellow"`, `"dark"`, `nil` when none is ahead any more) and its distance in metres. *Since 0.2.22.*
- **`red_light`**`(speed_kmh)`: The bus drove past a traffic light showing red (or red and yellow) at more than 5 km/h. *Since 0.2.22.*
- **`weather`**`(name)`: The weather changed: another weather file, rain or snow beginning or ending, the snow cover, the clouds. *Since 0.2.22.*
- **`minute`**`(hour, minute)`: The game's clock reached a new minute (also when it was set). *Since 0.2.22.*
- **`hour`**`(hour)`: The game's clock reached a new hour. *Since 0.2.22.*
- **`day`**`(year, month, day)`: The game's date changed. *Since 0.2.22.*

### Input, camera and game

- **`key`**`(key, down, combo)`: A key went down (`true`) or came up: winit's name of it (`"KeyH"`, `"F5"`, `"Numpad8"`), and the combination it made with the modifiers held, spelled as `input.hotkey` takes them (`"Ctrl+KeyH"`, `"F5"`). *Since 0.1.10.*
- **`controller_button`**`(device, button, down)`: A button of a steering wheel, joystick or gamepad went down (`true`) or up: the device's name and the button's number. *Since 0.2.22.*
- **`view`**`(new, old)`: The view changed (`"driver"`, `"pax"`, `"outside"`, `"free"`, `"foot"`). *Since 0.1.10.*
- **`menu_open`**`()`: The game menu opened. *Since 0.2.22.*
- **`menu_close`**`()`: The game menu closed. *Since 0.2.22.*
- **`screenshot`**`(file)`: A screenshot was taken: its file. *Since 0.2.22.*

### Panels and plugins

- **`ui_click`**`(panel, element)`: A button (or another clickable part) of one of the plugin's panels was clicked; `element` is `nil` for the panel itself. Enter in a text field is a click on it. *Since 0.2.21.*
- **`ui_change`**`(panel, element, value)`: A checkbox (`true`/`false`), slider (its number), text field (its text, at every key) or tabs element (the tab's number from 1) of the plugin's panels was changed by the player. *Since 0.2.22.*
- **`ui_focus`**`(focused)`: The panels got the mouse or gave it back (also by Esc, or a menu of the game opening). *Since 0.2.21.*
- **`menu_item`**`(id)`: The player chose one of the plugin's lines in the pause menu (`ui.set_menu_item`); the menu has closed. *Since 0.3.36.*
- **`setting`**`(key, value)`: The player changed one of the plugin's settings in its settings panel (`plugin.settings`). *Since 0.2.22.*
- **`message`**`(from, topic, data)`: Another plugin sent this one a message (`plugin.send`, `plugin.broadcast`): its name, the topic and the data (numbers, texts, tables). *Since 0.2.22.*

### LAN session

- **`lan_join`**`(id, name)`: A player joined the LAN session (or came back). *Since 0.2.22.*
- **`lan_leave`**`(id, name)`: A player left the LAN session. *Since 0.2.22.*
- **`lan_message`**`(from, data)`: The same plugin on another player's game sent this one a message (`lan.send`): the player's id and the text. *Since 0.2.22.*
- **`lan_chat`**`(name, text)`: A line was said in the LAN session's chat (by another player or this one). *Since 0.2.22.*
- **`chat_command`**`(text)`: The player typed a `/`-line into the chat (`"/login name pass"`): while a plugin hears this, such a line is not said to the session at all (a password never reaches the other players). Say something back with `lan.chat`. *Since 0.2.22.*

### Scenery object scripts

- **`object_init`**`(id, type, x, y, z, heading)`: A scenery object this script drives came within its range (once, again if it leaves and comes back): the map's id of the object, its type (the `.sco` file's name), where it stands and the way it faces. Only a script a `.sco` names with `[lua_script]` hears the `object_*` events. *Since 0.2.22.*
- **`object_frame`**`(id, dt)`: A frame of an object this script drives, while it is in range; `dt` is the game seconds since the last frame of that object (at the rate of the `.sco`'s `[lua_rate]`, else every game frame). *Since 0.2.22.*
- **`object_gone`**`(id)`: An object this script drives left its range or was unloaded with its tile; its state in the script can go. *Since 0.2.22.*
- **`object_click`**`(id, event)`: The player clicked the `[mouseevent]` of a mesh of an object this script drives. *Since 0.2.22.*
