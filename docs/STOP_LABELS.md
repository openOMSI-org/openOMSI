# Stop labels: what a stop's sign says, from the timetable

A `Haltestellenschild` on a map lists three things: the stop's name, the lines that call
there, and where those lines go from there. All three are already in the map's `TTData` — the
name in `Busstops.cfg`, the lines and the directions in the trips (`*.ttp`) that call at the
stop. openOMSI can write them out, so a map author (or a sign generator) does not have to copy
them by hand.

```sh
openomsi --root "C:\Program Files (x86)\Steam\steamapps\common\OMSI 2" \
         --map maps/Spandau/global.cfg \
         --export-stop-labels out/spandau-stops.csv
```

`--map` defaults to `maps/Grundorf/global.cfg`; `--root` is found by itself when it is left
out. Nothing of the game is started: the command reads `TTData` and writes the file.

## The file

CSV, RFC 4180 (a field with a comma, a quote or a line break is quoted, quotes doubled), one
row per **line at a stop**:

| column | what it is |
|---|---|
| `stop_id` | the stop's id, as `Busstops.cfg` and the trips use it |
| `platform` | the group in `Busstops.cfg`: a stop served on both sides of a street is two entries with one name and two groups |
| `name` | the stop's name |
| `line` | the line calling there, as the `.ttp` writes it |
| `destination` | where that line goes from here |

```csv
stop_id,platform,name,line,destination
101,0,Rathaus,5,Hauptbahnhof
101,0,Rathaus,58,Rathaus
101,0,Rathaus,58,Spandau
7,0,Betriebshof,,
```

## The rules behind it

* **A stop nobody calls at is still listed**, with the two last fields empty: the sign exists
  on the map whether or not a line uses it (out of service, or a timetable not written yet),
  and hiding such stops hides exactly the ones worth looking at.
* **Both directions of a line appear**: a stop in the middle of a route is reached by trips
  going each way, and a sign names both destinations.
* **A trip with no terminus** (the `.ttp`'s second line empty) takes the name of the last stop
  it calls at as its direction.
* **A trip that calls at the same stop twice** (a loop) adds its destination once.
* Lines are ordered the way a sign is read: `5`, `12`, `58`, `N1`, `X30` — numbers by value,
  then the ones with a prefix.
* Stops come out in the map's own order.

## What this does not do

It does not draw the sign. The design of the plate, its colours, the operator's logo, the size
and where it stands are the map's — this is the data the sign is made from, which is the part
that can be wrong. A generator would take this CSV (or `omsi_timetable::stop_labels::labels`
directly) and lay it out; the `[texttexture]` a map uses for a stop's name can be fed the same
way.
