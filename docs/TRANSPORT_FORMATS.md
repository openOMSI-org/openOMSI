# Transport folder: stations, routes, wires, switches, depots, signals

A map made for [transport modes](TRANSPORT_MODES.md) can describe its network in text files in
a `transport` folder next to `global.cfg`. Every file is optional; a map without the folder
runs as it always did (tours and `TTData` stay what they are). OMSI 2 ignores the folder.

All files use one syntax: `[block]` lines followed by `key value` lines. A key may repeat,
`//` and `#` start comments, case does not matter in keys. Units are written in the value:
`150 m`, `0.15 km`, `25 s`, `4 min`, `80 km/h`, `05:30` (hours past 24 are allowed).

Check a folder with the example tool, which prints every problem with its line:

```
cargo run -p omsi-transit --example transit_check -- maps/MyMap metro tram
```

A worked example is in `docs/examples/transport-metro/`.

| File | What | Page section |
| --- | --- | --- |
| `network.cfg` | which tracks serve which mode and how they are powered | [Network](#networkcfg) |
| `stations.cfg` | stations and platforms | [Stations](#stationscfg) |
| `routes.cfg` | routes of segments and services (no tours needed) | [Routes](#routescfg) |
| `catenary.cfg` | overhead wire for trams, trolleybuses, railways | [Wires](#catenarycfg-and-thirdrailcfg) |
| `thirdrail.cfg` | third rail for metros | [Wires](#catenarycfg-and-thirdrailcfg) |
| `switches.cfg` | track and wire switches | [Switches](#switchescfg) |
| `depots.cfg` | depots | [Depots](#depotscfg) |
| `signals.cfg` | signals and blocks | [Signals](#signalscfg) |

## What the game does with each file

| File | Used by the game for |
| --- | --- |
| `network.cfg` | the class (tram, metro, railway), street or mainline, third rail, tunnel and speed limit of the rail lanes it matches, after `openomsi-rail.cfg` (which still works) |
| `stations.cfg`, `routes.cfg` | the lines, tours and trips of the timetable, routed over the map's network (see below); the stops, the departure boards, the IBIS |
| `catenary.cfg`, `thirdrail.cfg` | the current of the player's rail vehicle: a pantograph needs a live section of wire under it, a shoe a live section of rail beside it, and a sagging line weakens the motors (a vehicle says `[rail_power]` `catenary`, `third_rail` or `diesel`; else a contact shoe means third rail and a pantograph overhead wire). A map without the file takes the current for granted. AI vehicles are not powered by the line yet |
| `switches.cfg` | the speed limit of the diverging branch (`diverging_speed` with `object`, `tile`, `diverging_path`), and how the points are operated (see [Switches](#switchescfg-in-the-game)) |
| `depots.cfg` | the `ai_group` of a service's depot supplies its vehicles; the first departure of a service with a depot comes out of the depot's first `gate` (see [Depots](#depotscfg-in-the-game)); capacity is read and not yet acted on |
| `signals.cfg` | blocks with `from` and `to` hold AI trains one at a time and their signals show it (see [Signals](#signalscfg-in-the-game)) |

### `signals.cfg` in the game

A block is a stretch of track given by where it begins and ends (`from x y z`, `to x y z`, the
map's coordinates, on the rails). The game routes the rail lanes between them on the whole map's
network at the start. An AI train that would enter a block another train is in (on its lane, on the
twin lane of the other direction of a single track, or with its tail not yet out) stops before the
block's first lane as at a red light, and goes on when it is free. The player's train in a block
holds the AI trains out of it. The `object` of a `[signal]` with a `block` is a scenery signal of
the map (the same `Signal` variable the map's own signal routes set): it shows stop (0) while its
block is in use and go (1) while it is free. The blocks need the timetable (`--schedule`) to have
trains to hold; the signals show for the player's train without it.

### `switches.cfg` in the game

```
[switch]
id W12
modes tram railway
object 123456
tile 12 5
straight_path 0            // the [path] of the object that is the straight way
diverging_path 1           // the diverging branch (repeat), also where diverging_speed applies
default straight           // the rest position: straight | diverging
drive interlocked          // auto | manual | interlocked
locked_by S12              // a signal id of signals.cfg
diverging_speed 25 km/h
```

Trains throw points as they always did (to the branch their route takes). A switch listed here is
also: held where it is while a train stands on it (its pieces and a tail still on it), put back in
its rest position 15 s after the last request, and, with `drive manual`, not thrown by AI trains at
all; `locked_by` keeps it still while that signal shows go. A switch the file does not list behaves
as before.

### `depots.cfg` in the game

A service with a `depot` whose depot has a `gate x y z` sends its first departure of the day out of
the gate: the vehicle appears at the gate, runs to the first station along the rails (a yard pace of
8 m/s at most) and stands there as at any stop. A gate with no rail within 25 m or no way to the
station is left out, the trip then begins at its station and the log says so.

## routes.cfg

Routes need no tours. A **route** is a set of **segments**: a run from one station to another
with the stops between them. A **service** says how vehicles run over some of the segments, in
order. Segments may be joined at any station, so a branch is a shared trunk segment and two
services; a short-turn is a service over only part of the line; an express is a segment with
`skip`.

```
[route]
id M2
name Green line
mode metro                 // bus | tram | railway | metro | trolleybus
colour 40 160 60

[segment]                  // belongs to the route above
id trunk
from Sokol
to Aeroport
via Voykovskaya, Vodny     // stops between, in order
skip Vodny                 // passed without stopping
dwell 25 s                 // default 30 s
speed 80 km/h
direction both             // forward: the way back is another segment

[service]
id all_day
segments trunk north       // each begins where the one before ends
headway 3 min              // or [trip] blocks with departures
from 05:30
to 25:00
days Mo-Fr                 // Mo-Fr | Sa,Su | weekend | all
turn_back reverse          // reverse | loop | depot:<id>
consist 6                  // cars in the train
vehicle Vehicles/Metro/81-717.ovh
depot Depot_Sokol

[trip]                     // departures for a service without a headway
service short
depart 06:00 06:10 06:20
```

### How routes run

A service becomes a timetable line named after its route: one tour for each departure (the
headway expanded between `from` and `to`, plus the listed `[trip]` departures), outward and, when
the service turns back by reversing and its first segment is not `direction forward`, a second
tour for the way back leaving the other end at the same times. The launcher's line list, the
duty, the IBIS, the navigator and the AI buses see them like lines from a `.ttl`. A station is
found among the map's bus stops by its `object` (the stop object's id code), else by name; a
service with a station that is no stop is skipped and the log says which. The track of a trip is found by the game: it reads the whole map's
network once at the start (as the navigator does), takes for each station the lanes of the mode's
kind (rail for trams, metro and railway, street for buses and trolleybuses) within 25 m, and
routes through them station by station over the shortest way, never against a lane's direction
(so the two tracks of a double line are told apart by the direction the trip goes). Where a
station stands comes from `pos` in `stations.cfg`, else from the stop object; a station with
neither is reported and its services do not run. The times come from the length of each stretch,
the segment's `speed` (else 50 km/h for trams, 70 metro, 100 railway, 40 buses), the vehicle
accelerating and braking, and `dwell` at each stop; until the track is found the departures wait.
The vehicles come from the service's `depot`: its `ai_group` names the map's AI group (in
`ailists.cfg`) that supplies them, rail vehicles for a rail service. A line of the same name
in `TTData` is the map's own and stays; `.ttl` files and routes can live side by side.

Rules the checker applies: every station exists in `stations.cfg` and serves the route's mode;
a service's segments join up; a loop ends where it begins; a service has a headway or trips; the
depot exists in `depots.cfg`; the route's mode is one of the map's modes.

## stations.cfg

```
[station]
id Sokol
name Sokol
modes metro tram
pos 892000 4196000 31.5    // optional
object 123456              // optional: id code of the map object
hub Sokol_bus              // optional: stations a passenger changes to
[platform]                 // belongs to the station above
id 1
side island                // left | right | island | both
length 150 m
height 1.1 m
screen_doors yes
track 4711
```

## catenary.cfg and thirdrail.cfg

One format for the overhead wire (`catenary_*` blocks; `wires 2` is the trolleybus's pair) and
one for the third rail (`thirdrail_*`). Both have a `_net` block (voltage, spacing, height, side,
modes), `_section` blocks (a stretch fed by a substation: `live`, `feeder_ohm`, `regen_kw`),
`_span` blocks (the line as points `p x y z` of a section) and `_insulator` or `_gap` blocks (a
break between sections: `pos`, `length`, `between`). The overhead also has `catenary_switch`
(a frog: `mode power|indicator|fixed`). The existing trolleybus `trolley.txt` is still read as
before ([TROLLEYBUS.md](TROLLEYBUS.md)).

## network.cfg

```
[track]
file Splines/MyMap/tram_*.sli
modes tram
street yes
power catenary             // catenary | third_rail | diesel | none
speed 40 km/h
gauge 1435
```

A rule needs `file`, `tile` or `id`; later rules win field by field. This replaces guessing the
mode of a track from its name (`openomsi-rail.cfg` is still read).

## switches.cfg

```
[switch]
id W12
modes tram railway
kind simple                // simple | crossover | double_slip | diamond | scissors | wire
object 123456              // or: pos x y z
branches 2
default straight
drive auto                 // auto (by route) | manual | interlocked
diverging_speed 25 km/h
locked_by S12
```

## depots.cfg

```
[depot]
id Depot_Sokol
modes metro
capacity 12
ai_group Metro_North
gate 0 0 0                 // vehicles enter and leave here (repeatable)
station Depot_stop
turning_loop no
```

## signals.cfg

`[signal]` (`kind main|distant|shunt|block|speed|tram`, `object`, `block`) and `[block]` (a
stretch that holds one vehicle: `length`, `next`).
