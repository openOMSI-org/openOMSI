# The transport folder

A map can describe its public transport network in text files in a `transport` folder next to
its `global.cfg`: stations, routes without tours, depots, track rules, switches, signals, and the
overhead wire or third rail. This page is the reference for map makers. Every file is optional.
A map without the folder runs as it always did, and OMSI 2 ignores the folder.

The files go with the [transport modes](TRANSPORT_MODES.md) a map declares.

## The files

| File | What it describes | What the game does with it |
| --- | --- | --- |
| [`stations.cfg`](#stationscfg) | Stations and platforms | Places the stops of the routes |
| [`routes.cfg`](#routescfg) | Routes, segments, services, departures | Turns them into timetable lines and tours |
| [`depots.cfg`](#depotscfg) | Depots | Supplies the vehicles; the first run of the day leaves from the depot gate |
| [`network.cfg`](#networkcfg) | Which tracks serve which mode | Classes rail lanes: tram, metro or railway, street or mainline, third rail, tunnel, speed limit |
| [`switches.cfg`](#switchescfg) | Track switches | Diverging speed limits, rest position, manual or interlocked points |
| [`signals.cfg`](#signalscfg) | Signals and blocks | Blocks hold AI trains one at a time; their signals show it |
| [`catenary.cfg`](#catenarycfg-and-thirdrailcfg) | Overhead wire | The player's rail vehicle needs a live wire for its pantograph |
| [`thirdrail.cfg`](#catenarycfg-and-thirdrailcfg) | Third rail | The player's rail vehicle needs a live rail for its shoe |

The trolleybus overhead wire has its own file, `trolley.txt` (see [Trolleybuses](TROLLEYBUS.md)).

A complete example is in the source repository under `docs/examples/transport-metro/`.

## Syntax

All files share one syntax.

```text
// a comment
# also a comment, at the start of a line
[station]
id Sokol
name Sokol          // a comment after a value needs a space before //
modes metro tram
```

* A `[block]` line starts a block. It runs to the next block line.
* Inside a block, each line is `key value`. The value is the rest of the line.
* Keys are not case-sensitive. A key may repeat where the tables say so.
* Ids are compared without regard to case.
* Yes/no values take `yes`, `no`, `1`, `0`, `true`, `false`, `on`, `off`.
* Lists (`modes`, `via`, `segments`) are separated by spaces or commas, as noted.

### Units

Write the unit after the number. A number without a unit takes the default unit.

| Kind | Units | Default unit |
| --- | --- | --- |
| Length | `m`, `km`, `cm`, `mm` | `m` |
| Duration | `s`, `sec`, `min`, `h` | `s` |
| Speed | `km/h`, `kmh`, `kph`, `m/s`, `mph` | `km/h` |
| Clock time | `HH:MM` or `HH:MM:SS`; hours past 24 are allowed (`25:00`) | - |

A decimal comma works too: `2,5 min`.

### Modes

Mode names accept these spellings:

| Mode | Accepted names |
| --- | --- |
| bus | `bus`, `coach` |
| tram | `tram`, `streetcar`, `strassenbahn`, `tramway` |
| railway | `railway`, `rail`, `train`, `sbahn`, `s-bahn`, `freight` |
| metro | `metro`, `subway`, `ubahn`, `u-bahn`, `underground` |
| trolleybus | `trolleybus`, `trolley`, `trolleybuses`, `trolleycoach` |

### Checking a folder

The game checks the folder when it loads the map and writes every problem to the log, with
file and line: `transport: routes.cfg:12: error: ...`.

From a source checkout you can also run the checker on its own. Name the modes the map is made
for after the folder:

```bash
cargo run -p omsi-transit --example transit_check -- maps/MyMap metro tram
```

It exits with status 1 when it finds an error.

## stations.cfg

A minimal file:

```text
[station]
id Sokol
modes metro
object 123456
```

A full one:

```text
[station]
id Sokol
name Sokol
modes metro tram
pos 892000 4196000 31.5
object 123456
hub Sokol_bus

[platform]
id 1
side island
length 150 m
height 1.1 m
screen_doors yes
track 4711
```

### [station]

| Key | Meaning | Default |
| --- | --- | --- |
| `id` | The id routes use. Required, unique | - |
| `name` | Display name | the id |
| `modes` | Modes the station serves (repeatable) | none |
| `pos` | `x y z` in the map's coordinates | - |
| `object` | Id code of the map's stop object | - |
| `hub` | Stations a passenger changes to on foot (comma list) | - |

### [platform]

A `[platform]` belongs to the `[station]` above it.

| Key | Meaning | Default |
| --- | --- | --- |
| `id` | Unique within the station | `1` |
| `side` | `left`, `right`, `island` (or `centre`), `both` | `right` |
| `length` | Platform length | 0 |
| `height` | Height above the rail head | 0 |
| `screen_doors` | Platform screen doors | `no` |
| `track` | The map's spline or object of the track beside it | - |

### How the game finds a station

1. By `object`: the stop object with that id code among the map's bus stops.
2. Else by name: a bus stop whose name matches the station's `name` or `id`.

Where the station stands comes from `pos`, else from the stop object. A station with neither is
reported, and its services do not run.

> **Note:** Platforms and `hub` are read and checked, but the game does not use them yet.

## routes.cfg

Routes need no tours. A **route** is one line. It is made of **segments**: runs from one
station to another, with the stops between. A **service** says how vehicles run over some
segments, in order, and when.

A minimal route:

```text
[route]
id T1
mode tram

[segment]
id main
from Depot_Loop
to Central

[service]
id all_day
segments main
headway 10 min
from 06:00
to 22:00
```

A full example:

```text
[route]
id M2
name Green line
mode metro
colour 40 160 60

[segment]
id trunk
from Sokol
to Aeroport
via Voykovskaya, Vodny
skip Vodny
dwell 25 s
speed 80 km/h
direction both

[segment]
id north
from Aeroport
to Khovrino

[service]
id all_day
segments trunk north
headway 3 min
from 05:30
to 25:00
days Mo-Fr
turn_back reverse
consist 6
vehicle Vehicles/Metro/81-717.ovh
depot Depot_Sokol

[service]
id short
segments trunk

[trip]
service short
depart 06:00 06:10 06:20
```

Segments can be joined at any station:

* a **branch** is a shared trunk segment and two services;
* a **short-turn** is a service over part of the line;
* an **express** is a segment with `skip`.

### [route]

| Key | Meaning | Default |
| --- | --- | --- |
| `id` | The route's id. It becomes the timetable line's name | - |
| `name` | Display name | the id |
| `mode` | `bus`, `tram`, `railway`, `metro` or `trolleybus`. Required | - |
| `colour` (or `color`) | `R G B`, 0 to 255 | - |

### [segment]

A `[segment]` belongs to the `[route]` above it.

| Key | Meaning | Default |
| --- | --- | --- |
| `id` | Unique within the route | - |
| `from`, `to` | First and last station ids. Must differ | - |
| `via` | Stations between, in order (comma list) | - |
| `skip` | `via` stations passed without stopping (comma list) | - |
| `dwell` | Standing time at each stop | `30 s` |
| `speed` | Speed limit on this stretch | by mode, see below |
| `direction` | `both`, or `forward` (`oneway`): the way back is another segment | `both` |

### [service]

A `[service]` belongs to the `[route]` above it.

| Key | Meaning | Default |
| --- | --- | --- |
| `id` | Unique within the route | - |
| `segments` | Segment ids in order. Each begins where the one before ends | - |
| `headway` | Time between vehicles. At least 10 s | - |
| `from`, `to` | First and last departure of the headway | `00:00`, `24:00` |
| `days` | Days it runs, see below | every day |
| `turn_back` | `reverse`, `loop`, or `depot:<id>` | `reverse` |
| `depot` | The depot that supplies the vehicles | - |
| `consist` | Cars in the train | - |
| `vehicle` | Vehicle files (comma list) | - |

A service needs a `headway`, or `[trip]` departures, or both.

`days` takes `Mo-Fr`, `Sa,Su`, `Mo We Fr`, `all` (or `daily`), `weekdays`, `weekend`. Day names
may be two or three letters. A range may wrap: `Fr-Mo`.

> **Note:** `colour`, `consist` and `vehicle` are read and checked, but the game does not use
> them yet. The vehicles come from the depot's AI group.

### [trip]

| Key | Meaning |
| --- | --- |
| `service` | The service id in the route above |
| `depart` (or `at`) | Departure times from the first station, separated by spaces or commas. Repeatable |

### How routes run

Each service becomes part of a timetable line named after the route's `id`:

* **Departures.** The headway is expanded from `from` to `to`, and the `[trip]` times are added.
* **Tours.** Each departure is one tour, numbered `<service>-HHMM` (for example `all_day-0530`).
* **The way back.** When the service turns back with `reverse` and its first segment is not
  `direction forward`, each departure gets a second tour (`...r`). It leaves the other end at the
  same time.
* **Days.** The service runs on its days. A service that runs on Sunday also runs on public
  holidays. It runs on school days and in the school holidays alike.
* **Player.** The player may choose these lines in the launcher.
* **Vehicles.** The service's `depot` names a depot of `depots.cfg`. Its `ai_group` is the map's
  AI group (in `ailists.cfg`) that supplies the vehicles.

The launcher's line list, the duty, the IBIS, the navigator and the AI see these lines like lines
from a `.ttl` (see [How routes work](ROUTES.md)).

**The track.** The game reads the whole map's network once at the start. For each station it
takes the lanes of the mode's kind within 25 m: rail lanes for trams, metro and railway, street
lanes for buses and trolleybuses. It routes from station to station over the shortest way and
never against a lane's direction. So the two tracks of a double line are told apart by the way
the trip goes. The departures wait until the track is found.

**The times.** Running times come from each stretch's length, the speed, the vehicle
accelerating and braking, and the dwell at each stop. Without a segment `speed`:

| Mode | Default speed |
| --- | --- |
| Tram | 50 km/h |
| Metro | 70 km/h |
| Railway | 100 km/h |
| Bus, trolleybus | 40 km/h |

### Edge cases

* A line of the same name from a `.ttl` (in `TTData` or a chrono scenario) is the map's own. It
  stays, and the route of that id is skipped. `.ttl` lines and routes can live side by side.
* A service with a station that is not a bus stop of the map is skipped. The log names the
  station.
* A service with fewer than two stops, or with no departures, is skipped.
* A loop (`turn_back loop`) must end at the station where it begins.

### What the checker reports

* A route without a valid `mode`, or a mode the map is not made for.
* Duplicate route, segment or service ids.
* A segment without `from` and `to`, or one that begins and ends at the same station.
* A station that is not in `stations.cfg`, or does not serve the route's mode.
* A `skip` station that is not among the `via` stations (warning).
* Service segments that do not join up, or do not exist.
* A service without a headway or trips, a headway under 10 s, `to` not after `from`, only one of
  `from`/`to` (warning), no valid day, or `consist 0`.
* A depot that is not in `depots.cfg`.

## depots.cfg

```text
[depot]
id Depot_Sokol
name Sokol depot
modes metro
ai_group Metro_North
gate 120.5 -340.0 6.0
station Depot_stop
capacity 12
turning_loop no
```

| Key | Meaning | Default |
| --- | --- | --- |
| `id` | The id services use. Required, unique | - |
| `name` | Display name | the id |
| `modes` | Modes it serves. Required | - |
| `ai_group` | The map's AI group (in `ailists.cfg`) that supplies the vehicles | - |
| `gate` | `x y z` where vehicles leave and enter the network. Repeatable | - |
| `station` | A station of `stations.cfg` vehicles are scheduled from | - |
| `capacity` | Vehicles it holds | 0 |
| `turning_loop` | The depot has a turning loop | `no` |

### In the game

A service whose depot has a `gate` sends its **first departure of the day** out of the first
gate:

1. The vehicle appears at the gate.
2. It runs along the rails to the first station, at a yard pace of 8 m/s at most.
3. It stands there as at any stop, then starts its trip.

A gate with no rail within 25 m, or with no way to the station, is left out. The trip then
begins at its station and the log says so.

`capacity`, `station` and `turning_loop` are read and checked, but the game does not act on them
yet. A depot without a gate gets a warning.

## network.cfg

Tells the game what each track is. This is more exact than guessing from the track's name.

```text
[track]
file Splines/MyMap/tram_*.sli
modes tram
street yes
power catenary
speed 40 km/h
```

| Key | Meaning |
| --- | --- |
| `file` | A glob on the spline profile or object file name (`*` and `?`, case-insensitive). Without a folder in the pattern, only the file name is matched |
| `tile` | `x y`: only on this tile |
| `id` | Only this spline or object |
| `modes` (or `mode`) | Who the track is for. One rail mode sets the class (tram, metro, railway) |
| `street` | `yes`: rails in the street (tram track). `no`: railway |
| `power` | `catenary` (`overhead`, `wire`), `third_rail`, `diesel`, `none` (`unpowered`) |
| `tunnel` | The track is in a tunnel |
| `speed` | Speed limit |
| `gauge` | Track gauge in mm (read, not used yet) |

* A rule needs at least one of `file`, `tile` or `id`. Otherwise it would match everything.
* Later rules win, field by field.
* `power third_rail` marks a third rail. `catenary` and `diesel` mark none.
* These rules come after the rules of `openomsi-rail.cfg`, which still works (see
  [Track](RAIL.md#track)).

## switches.cfg

A minimal switch that limits its diverging branch to 25 km/h:

```text
[switch]
id W12
modes tram
object 123456
tile 12 5
diverging_path 1
diverging_speed 25 km/h
```

A full one:

```text
[switch]
id W12
modes tram railway
kind simple
object 123456
tile 12 5
straight_path 0
diverging_path 1
default straight
drive interlocked
locked_by S12
diverging_speed 25 km/h
branches 2
```

| Key | Meaning | Default |
| --- | --- | --- |
| `id` | Required, unique | - |
| `modes` | Required | - |
| `object` | Id code of the switch object on the map | - |
| `pos` | `x y z`, instead of `object` | - |
| `tile` | `x y`: the object's tile | - |
| `straight_path` | The object's `[path]` that is the straight way | 0 |
| `diverging_path` | The diverging branch paths (repeatable, or several numbers) | - |
| `default` | Rest position: `straight` or `diverging` | `straight` |
| `drive` | `auto` (by route), `manual` (by the driver), `interlocked` | `auto` |
| `locked_by` | A signal id of `signals.cfg` | - |
| `diverging_speed` | Speed limit on the diverging paths | - |
| `kind` | `simple`, `crossover`, `double_slip` (`slip`), `diamond` (`crossing`), `scissors`, `wire` (`overhead`) | `simple` |
| `branches` | 2 to 8 | 2 |

A switch needs `object` or `pos`. `diverging_speed` needs `diverging_path` (and `tile`) to say
which branch it limits.

### In the game

Trains throw the points as they always did, to the branch their route takes. A switch listed
here also:

* stays where it is while a train stands on it (its cars or its tail);
* goes back to its rest position 15 s after the last request;
* with `drive manual`, is not thrown by AI trains at all;
* with `locked_by`, stays still while that signal shows go.

Only switches with an `object` take part. A switch the file does not list, or one given only by
`pos`, behaves as before. `kind` and `branches` are read and checked, not used yet.

## signals.cfg

```text
[signal]
id S12
modes railway metro
kind main
object 123456
block B7

[block]
id B7
modes railway metro
from 892000 4196000 31.5
to 892850 4196000 31.5
```

### [signal]

| Key | Meaning | Default |
| --- | --- | --- |
| `id` | Required, unique | - |
| `modes` | Required | - |
| `kind` | `main`, `distant`, `shunt`, `block`, `speed`, `tram` | `main` |
| `object` | Id code of the map's scenery signal | - |
| `block` | The block it guards | - |

### [block]

| Key | Meaning |
| --- | --- |
| `id` | Required, unique |
| `modes` | Required |
| `from`, `to` | `x y z` on the rails where the block begins and ends (map coordinates). Give both or neither |
| `length` | Block length, when `from` and `to` are not given |
| `next` | The following block ids |

### In the game

* The game routes the rail lanes between a block's `from` and `to` over the whole map's network
  at the start.
* An AI train stops before a block that another train is in, as at a red light. That counts the
  same lane, the twin lane of the other direction on a single track, and a tail not yet out. It
  goes on when the block is free.
* The player's train in a block also holds AI trains out of it.
* A signal's `object` is a scenery signal of the map: the same `Signal` variable the map's own
  signal routes set. It shows stop (0) while its block is in use and go (1) while it is free.
* Blocks need the timetable to have trains to hold. The signals show for the player's train
  without it.

## catenary.cfg and thirdrail.cfg

The overhead wire and the third rail share one format. The overhead uses `catenary_*` blocks,
the third rail `thirdrail_*` blocks.

```text
[catenary_net]
voltage 600
modes tram

[catenary_section]
id S1

[catenary_span]
section S1
p 395.0 -185.4 6.10
p 300.0 -185.4 6.05
p 200.0 -185.4 6.10
```

A full overhead file:

```text
[catenary_net]
kind dc
voltage 600
wires 1
spacing 0.6
height 5.5
modes tram

[catenary_section]
id S1
name Substation A
live yes
feeder_ohm 0.05
regen_kw 0

[catenary_section]
id S2
live no

[catenary_span]
section S1
p 395.0 -185.4 6.10
p 300.0 -185.4 6.05

[catenary_span]
section S2
p 300.0 -185.4 6.05
p 200.0 -185.4 6.10

[catenary_insulator]
pos 300 -185.4 6.05
length 0.8
between S1 S2

[catenary_switch]
id F1
pos 262 -185.4 6.1
mode power
```

### [catenary_net] / [thirdrail_net]

One per file. Required.

| Key | Meaning | Default |
| --- | --- | --- |
| `kind` | Overhead: `dc` or `ac`. Third rail: `top` or `bottom` contact | `dc` / top |
| `voltage` | Nominal voltage (V) | 600 overhead, 750 third rail |
| `wires` | 1 (tram, railway) or 2 (trolleybus pair) | 1 |
| `spacing` | Distance between two wires | `0.6 m` |
| `height` | Height above the rail head or road | `5.5 m` |
| `side` | Third rail: `left` or `right` of the track | `right` |
| `modes` | Modes the line serves | - |

### [..._section]

A stretch fed by one substation.

| Key | Meaning | Default |
| --- | --- | --- |
| `id` | Required, unique | - |
| `name` | Display name | the id |
| `live` | Fed by a substation | `yes` |
| `feeder_ohm` | Feeder resistance | 0.05 |
| `regen_kw` | Power the feeder takes back from a braking vehicle | 0 |

### [..._span]

The line as points. `section` names its section. Each `p x y z` is one point, in map
coordinates. A span needs at least two points.

### [catenary_insulator] / [thirdrail_gap]

A break between sections. `[catenary_gap]` and `[thirdrail_insulator]` work too.

| Key | Meaning | Default |
| --- | --- | --- |
| `pos` | `x y z`. Required | - |
| `length` | Length of the dead stretch | `0.8 m` overhead, `12 m` third rail |
| `between` | The two section ids | - |

### [catenary_switch]

A frog in the overhead. Keys: `id`, `pos x y z` (required), `mode` (`power`, `indicator` or
`fixed`; default `fixed`). Read and checked, not used by rail vehicles yet.

### In the game

The file powers the **player's rail vehicle**:

* A pantograph needs a live section within 2.5 m sideways of the wire.
* A contact shoe needs a live section within 1.2 m of the third rail.
* Inside a break, or on a dead section, there is no current.
* The drive weakens below 80 % of the nominal voltage and is gone at 50 %.
* The brakes always work.

The vehicle says where it takes its current with `[rail_power]` (see
[Current](RAIL.md#current)). A map without these files takes the current for granted.

> **Note:** AI vehicles are not powered by the line yet. `feeder_ohm` and `regen_kw` are read,
> but rail vehicles get the full voltage or none: there is no voltage drop along the line yet.
