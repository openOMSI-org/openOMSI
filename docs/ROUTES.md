# Timetables and routes

This page explains how a map's timetable works: lines, tours and trips, which of them run on a
given day, chrono scenarios, which vehicle drives a tour, and how a bus finds its destination
display. It is for map makers, and for players who want to know why a tour runs or does not.
openOMSI reads the same files as OMSI 2 and follows its rules.

To build a timetable in openMapper, see [Tutorial 3: lines and traffic](MAPPER_LINES.md). For routes on rails
without tours, see [Routes without tours](TRANSPORT_FORMATS.md#routescfg).

## The parts of a timetable

| Term | What it is |
| --- | --- |
| Stop | A bus stop object on the map, listed in `Busstops.cfg` |
| Track | The way between stops, stored in a `.ttr` file |
| Trip | One run from the first stop to the terminus, with its stops and times (`.ttp`) |
| Line | A set of tours with one name (`.ttl`); the file name is the line's name |
| Tour | One vehicle's working day on a line: a list of trips with departure times |
| Depot | An AI group of vehicles that runs the tours (`ailists.cfg`) |

All of these live in the map's `TTData` folder. Chrono scenarios can add their own `TTData`
folders (see [Chrono scenarios](#chrono-scenarios)).

## Which tours run today

Each tour has a **day mask**: a number whose bits say on which days it runs.

| Bit | Value | Day |
| --- | --- | --- |
| 0 | 1 | Monday |
| 1 | 2 | Tuesday |
| 2 | 4 | Wednesday |
| 3 | 8 | Thursday |
| 4 | 16 | Friday |
| 5 | 32 | Saturday |
| 6 | 64 | Sunday |
| 7 | 128 | Public holiday |
| 8 | 256 | Runs on school days |
| 9 | 512 | Runs in the school holidays |

A tour runs on a date when both of these hold:

1. The day bit is set. On a public holiday (a `[holiday]` date of the map) only bit 7 counts.
   On any other day the weekday's bit counts.
2. The school bit is set. Inside a school-holiday range (`[holidays]`) that is bit 9, on any
   other day bit 8.

An empty mask means 1023: every day.

Examples:

* `1023`: every day.
* `287` (Monday to Friday + bit 8): school days only, so not in the school holidays.
* `799` (Monday to Friday + bits 8 and 9): every working day.

A printed timetable marks a tour that lacks bit 9 as not running in the school holidays, and one
that lacks bit 8 as running only in the school holidays.

A tour that starts before midnight and runs past it is checked against the day it started.

## Line files (.ttl)

A `.ttl` file is one line. Its file name (without `.ttl`) is the line's name.

```text
[userallowed]

[priority]
0

[newtour]
1
Busses
1023

[addtrip]
5_out
0
360.000

[addtrip]
5_back
0
410.000
```

| Keyword | Lines after it | Meaning |
| --- | --- | --- |
| `[userallowed]` | none | The player may choose this line. Without it the line is for AI vehicles only |
| `[priority]` | a number | The line's priority |
| `[newtour]` | tour number, AI group, day mask | Starts a tour. The AI group is a depot of `ailists.cfg` |
| `[addtrip]` | trip name, profile index, departure | Adds a trip to the tour above. The departure is in minutes after midnight (`360` = 06:00) |

> **Note:** The player's **select timetable** list shows only lines with `[userallowed]`.
> Berlin-Spandau, for example, offers 5 of its 23 lines. The others are run by AI vehicles.

### The other TTData files

These files are normally written by the timetable editor. openOMSI reads them as OMSI 2 does.

| File | Contents |
| --- | --- |
| `Busstops.cfg` | `[busstop]`: name, group, object id, three parameters |
| `StnLinks.cfg` | `[stnlink]` (length, from id, to id) with its `[stnlink_entry]` lines |
| `*.ttp` | A trip: `[trip]` (track, terminus, line), `[station_typ2]` stops, `[profile]` time profiles, `[trainreverse]` |
| `*.ttr` | A track: `[track_entry]` lines |

The `[trip]` block's terminus and line are what the bus's destination display gets when the
trip starts (see [Destination displays](#destination-displays-hof-files)).

## Chrono scenarios

A chrono scenario changes a map from a date on: new lines, lines taken off, other AI vehicles.
Each scenario is a folder under the map's `Chrono` folder with a `Chrono.cfg` file.

```text
[name]
Timetable change 1994

[startdate]
19940529

[deactivate_lines]
2
5
X34
```

| Keyword | Lines after it | Meaning |
| --- | --- | --- |
| `[name]` | the name | The scenario's name |
| `[description]` | text up to `[end]` | A description |
| `[startdate]` | `YYYYMMDD` | First day in force (inclusive) |
| `[enddate]` | `YYYYMMDD` | Last day in force (inclusive) |
| `[ticketpack]` | a file | The ticket pack while the scenario is in force |
| `[moneysystem]` | a file | The money system while the scenario is in force |
| `[deactivate_lines]` | a count, then that many line names | Takes those lines off |

A scenario folder may also hold:

* `TTData`: its own timetable files (see below);
* `ailists.cfg`: a complete new AI list that replaces the map's;
* `ailists_#upd.cfg`: an update. Each group in it replaces the group of the same name.

### When a scenario is in force

* A scenario needs at least one date. A scenario without `[startdate]` and `[enddate]` is never
  in force.
* With both dates, it is in force from the start day to the end day, both included.
* With one date, the other end is open.

### The order of scenarios

The game finds every `Chrono.cfg` under `Chrono` in a fixed order:

1. Depth first: a folder's subfolders come before its own `Chrono.cfg`.
2. Names sort as Windows lists them (case does not matter).

A later scenario overrides an earlier one. For AI lists, the newest scenario with a full
`ailists.cfg` ends the search. Updates (`ailists_#upd.cfg`) from it onwards are applied oldest
first.

### How the timetable is put together

1. Stops, links, trips and tracks are read from the map's `TTData`, then from each active
   scenario's `TTData` in order. A later file replaces an earlier one of the same name.
2. Lines are read from the last active scenario back to the map's own `TTData`. A `.ttl` is
   skipped when:
   * a line of that name was already read (so a later scenario's file wins), or
   * an active scenario **after** that folder lists the line in `[deactivate_lines]`.
3. Then the map's `car_use/*.ocu` files are read.

So a scenario takes a line off only for the folders before it. A later scenario can bring the
line back with its own `.ttl`.

> **Warning:** `[deactivate_lines]` starts with a **count**. `[deactivate_lines]`, `1`, `17`
> takes line 17 off. Without the count, the first line name is read as the count.

## Vehicles for tours (car_use)

Files in the map's `car_use` folder (`*.ocu`) say which vehicles run which tours.

```text
[valid]
19940529
20991231

[line]
5

[number_tour]
2817	1
2818	2

[onlytypes]
vehicles\MAN_SD200\MAN_SD77.bus
[end]
```

| Keyword | Lines after it | Meaning |
| --- | --- | --- |
| `[valid]` | start date, end date (`YYYYMMDD`) | In force from start to end, both included. A start after the end is never in force |
| `[line]` | a line name | The line this file is for |
| `[number_tour]` | lines of `fleet number<Tab>tour` | That tour runs with that vehicle |
| `[onlytypes]` | vehicle files, up to `[end]` | The line's other tours always take one of these types |
| `[types_prefered]` | a probability (0 to 1), then vehicle files | The other tours take one of these types with that probability |
| `[type_tour]` | a line | Read; kept for OMSI 2 compatibility |

How it works:

* Fleet numbers from `[number_tour]` are given first. A number already in use is skipped.
* The other tours of the line then get a free vehicle of the listed types from the tour's
  depot group. A tour not covered draws its vehicle when it is first due.
* The assignment is made when the map loads and again when the date changes.
* A file whose `[line]` names no loaded line is ignored. The log says it "has no valid [line]
  entry for the current chrono scenario".

The launcher's AI lists page writes its own rules into the content folder. Their names start
with `!`, so they sort first and take priority. A file in the content folder replaces the map's
file of the same name.

## Destination displays (hof files)

A bus's destinations, stops and route codes come from its `.hof` files. The game loads every
`*.hof` in the vehicle's folder, in the order the folder lists them.

| hof keyword | Meaning |
| --- | --- |
| `[name]` | The hof's name. Depots refer to it by this name, not by file name |
| `[addterminus]` | A terminus: code, name and display strings |
| `[addterminus_allexit]` | The same, for a terminus where all passengers get off |

### Which hof a bus uses

* **The player's bus** uses the hof chosen in the vehicle dialog.
* **An AI timetable bus** uses the hof named by its depot in `ailists.cfg`. In an
  `[aigroup_depot]` block, the group name is followed by the hof name. The
  `[aigroup_depot_typgroup]` or `[aigroup_depot_typgroup_2]` entries must come before it.
* The hof name must match a hof's `[name]` exactly, including case.
* Without a match, the bus uses the first hof of its folder.

openOMSI adds two things to this:

* When the bus's own folder has no hof of the depot's name, the game also looks for it in the
  other vehicle folders. So the bus shows the map's termini and stops instead of its own first
  hof.
* A tour whose AI group is a plain `[aigroup_2]` pool (no `[aigroup_depot]` block) still gets a
  hof: the map's own depot hof for that vehicle folder, when there is one. Without this, such a
  bus never ran its `ai_scheduled_settarget` trigger, and displays that switch on in that
  trigger (roller blinds, LED matrices) stayed blank.

### When a trip starts

1. The game takes the trip's line and terminus name from its `.ttp`.
2. It looks for a terminus of exactly that name in the bus's hof. That terminus's index goes
   into `AI_target_index`. With no match the index stays as it was.
3. The line goes into the string variable `SetLineTo`.
4. The bus script's `ai_scheduled_settarget` trigger runs. The script sets the IBIS, the codes
   and the displays.

Coupled parts (an articulated bus's trailer, for example) get the same call.

## Details

These are fixes openOMSI made so that timetables behave as in OMSI 2:

* `[deactivate_lines]` is read as a count followed by names. Before, the count was taken for a
  line name, so lines such as "1", "2" or "17" disappeared.
* Chrono dates are inclusive at both ends. A scenario without dates is never in force.
  Scenario folders are read in the game's order.
* Lines are read from the last folder back and taken off only by later scenarios.
* Tour mask bits 8 and 9 follow the original: bit 8 runs on school days, bit 9 in the school
  holidays. Spandau's school runs (lines 5E and 92E, masks 287 and 288) carry bit 8 alone.
