# Transport modes

OMSI 2 has buses and one flag for everything on rails. openOMSI lets a vehicle and a map say
what kind of public transport they are for, so that a tram map holds trams, a metro map metro
trains, and so that modders can make each kind properly (their own physics, wires, stations and
routes are described in the pages for each mode, linked below as they arrive).

Nothing here changes how existing content loads. A map that names no mode takes every
vehicle, a vehicle that names no mode is judged by its shape as before. A file that uses these
keywords still loads in OMSI 2, which skips what it does not know (it may not work correctly
there, which is expected).

## The five keywords

One bare keyword line per mode, in a vehicle file (`.bus`, `.ovh`) or in a map's `global.cfg`:

| Keyword | For |
| --- | --- |
| `[bus_transport]` | buses on streets |
| `[tram_transport]` | trams on street track, with a contact wire |
| `[railway_transport]` | trains on mainline track |
| `[metro_transport]` | metro trains, normally with a third rail |
| `[trolleybus_transport]` | trolleybuses under a two-wire overhead |

A file may name several (a map for buses and trams: two keywords; a dual-mode vehicle too).
The spelling of the keyword's case does not matter.

```
# the Tatra tram's .ovh
[tram_transport]

# a city map's global.cfg
[bus_transport]
[tram_transport]
[trolleybus_transport]
```

## What the game does with them

A vehicle may run on a map when

1. the map names no mode: any vehicle runs (all OMSI 2 maps and mods), or
2. the vehicle and the map share a mode.

A vehicle that names no mode is judged by its file, exactly as the game has always done:

* a rail vehicle (`[type] 2`, `[boogies]`, `[contact_shoe]`, `[rail_body_osc]`) is a tram, a
  metro train or a railway train (`[rail_class]` can say it, see [RAIL.md](RAIL.md));
* a vehicle with `[trolley_pole]` gear is a bus and a trolleybus;
* anything else is a bus.

The rule is applied at four places:

* the **launcher** lists a map's and a vehicle's modes (`MapInfo.transport`,
  `VehicleInfo.transport`; `omsi_launcher_core::transport_refusal` gives the reason a pair does
  not fit);
* the **game** will not start a vehicle on a map built for other modes and says why
  (`OMSI_IGNORE_TRANSPORT=1` overrides it for a test run);
* **timetable vehicles** of the AI are not put on a map built for other modes;
* the **random AI pool** leaves out vehicles that declare a mode the map does not have (cars and
  lorries declare none and stay: they are scenery on any street).

Example of the message: *Tatra T6 cannot run on this map. This map is made for bus only; the
vehicle is made for tram.*

## For modders

* Put the keyword in the vehicle file by hand, or with the Development Tools' project
  templates.
* A map made for several modes lists them all; the stations, tracks and routes of each mode
  are described in the pages for those modes.
* If you are unsure how the game sees an old vehicle, the launcher shows its modes.

## Where each part is described

| Topic | Page |
| --- | --- |
| Stations, routes without tours, overhead wire, third rail, switches, depots, signals | [TRANSPORT_FORMATS.md](TRANSPORT_FORMATS.md) |
| Trams, metro and railway vehicles: classes, track, switches, movement | [RAIL.md](RAIL.md) |
| Trolleybuses: the wire, the poles, the drive | [TROLLEYBUS.md](TROLLEYBUS.md), [TROLLEY_MODS.md](TROLLEY_MODS.md) |
| Leaf, air and progressive suspension | [SUSPENSION.md](SUSPENSION.md) |
| Screens as pages (`[htmltexture]`) | [HTMLTEXTURE.md](HTMLTEXTURE.md) |
| Older additions that lift OMSI 2's limits | [MODDING.md](MODDING.md) |
