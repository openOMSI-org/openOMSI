# Transport modes

OMSI 2 knows buses and has one flag for everything on rails. openOMSI lets a vehicle and a map
say which kind of public transport they are for: bus, tram, railway, metro or trolleybus. This
page is for map makers and vehicle modders.

Existing content loads as before. A map that names no mode takes every vehicle. A vehicle that
names no mode is judged by its shape. OMSI 2 skips the keywords, so files that use them still
load there.

## Declaring a mode

Add one bare keyword line per mode to a vehicle file (`.bus`, `.ovh`) or to a map's
`global.cfg`.

```text
# a tram's .ovh
[tram_transport]
```

```text
# global.cfg of a city map with buses, trams and trolleybuses
[bus_transport]
[tram_transport]
[trolleybus_transport]
```

| Keyword | For |
| --- | --- |
| `[bus_transport]` | Buses on streets |
| `[tram_transport]` | Trams on street track, with a contact wire |
| `[railway_transport]` | Trains on mainline track |
| `[metro_transport]` | Metro trains, usually with a third rail |
| `[trolleybus_transport]` | Trolleybuses under a two-wire overhead line |

* A file may name several modes: a map for buses and trams, or a dual-mode vehicle.
* Case does not matter.

## Which vehicle may run on which map

A vehicle may run on a map when:

1. the map names no mode (all OMSI 2 maps and mods), or
2. the vehicle and the map share at least one mode.

### Vehicles without a keyword

A vehicle that declares no mode is judged by its file:

| The file has | The vehicle counts as |
| --- | --- |
| `[type] 2`, `[boogies]`, `[contact_shoe]` or `[rail_body_osc]` | A rail vehicle: tram, metro or railway (see [Vehicle classes](RAIL.md#vehicle-classes)) |
| `[trolley_pole]` blocks | A bus and a trolleybus |
| Anything else | A bus |

A declared mode always wins over the shape.

## Where the rule applies

| Place | What happens |
| --- | --- |
| Launcher | The map and vehicle pickers show a mode filter when content declares modes. A vehicle that does not fit the map gets a warning icon and a notice with the reason |
| Game start | The game does not start a vehicle on a map built for other modes, and says why |
| Timetable AI | Timetable vehicles are not put on a map built for other modes |
| Random AI traffic | Vehicles that declare a mode the map does not have are left out. Cars and lorries declare none, so they stay on any street |

Example message: *Tatra T6 cannot run on this map. This map is made for bus only; the vehicle is
made for tram.*

> **Tip:** For a test run you can override the rule with the environment variable
> `OMSI_IGNORE_TRANSPORT=1` (see [Environment variables](DEBUG_FLAGS.md)).

## For modders

* Add the keyword to your vehicle file by hand.
* A map made for several modes lists them all.
* Not sure how the game sees an old vehicle? The launcher's vehicle picker shows its modes.

## More on each mode

| Topic | Page |
| --- | --- |
| Stations, routes without tours, overhead wire, third rail, switches, depots, signals | [Transport folder](TRANSPORT_FORMATS.md) |
| Trams, metro and railway vehicles: classes, track, switches, movement | [Trams, metros and trains](RAIL.md) |
| Trolleybuses: the wire, the poles, the drive | [Trolleybuses](TROLLEYBUS.md), [Real trolleybus add-ons](TROLLEY_MODS.md) |
| Timetables, tours and chrono scenarios | [How routes work](ROUTES.md) |
| Leaf, air and progressive suspension | [Suspension types](SUSPENSION.md) |
| Screens as pages (`[htmltexture]`) | [HTML textures](HTMLTEXTURE.md) |
| Older additions that lift OMSI 2's limits | [Modding overview](MODDING.md) |
