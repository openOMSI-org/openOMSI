# Lines and traffic

This tutorial puts a public transport service on a map: stops, lines with trips there and back,
tours with a full day's departures, the depot files that feed the buses' displays, AI traffic and AI
paths. It is for map makers who have done [Your first map](MAPPER_FIRST_MAP.md).

## What you will build

On a map with a few roads:

- four stops (with twins across the road) and a start point,
- line 1 with two trips and three tours, running weekdays and weekends,
- a depot whose AI buses drive the tours you do not drive,
- a depot file, so the buses show the right destinations and the IBIS knows the routes,
- traffic that follows the hours of the day, parked cars, and an AI path for cars.

## What you need

- A map with roads, for example the one from [Your first map](MAPPER_FIRST_MAP.md).
- OMSI 2 installed: buses, cars and depots come from it.

openMapper writes all of this in OMSI 2's own files: `TTData` (`.ttl`, `.ttp`, `.ttr`), `StnLinks.cfg`,
`Busstops.cfg`, `ailists.cfg` and `.hof` files. A stock map's files stay as they are until you edit
its timetable.

## 1. Place the stops

1. Press `4` (**Game logic**) and click **Bus stops**.
2. Pick a stop object in the list.
3. Keep **On both sides of the road** ticked for a twin across the road.
4. Click beside a road, on the side where the doors open. The stop goes to the kerb of the nearest
   lane.
5. Type the name under **Stop name** in the properties.
6. Repeat for three more stops along the way the line should take.

Each stop's properties also hold:

| Setting | What it does |
| --- | --- |
| **Kind of stop** | **Compulsory stop**, **Request stop** or **Terminus** |
| **Waiting, at least** / **Waiting, at most** | How many people wait |
| **Getting off** | How many riders get off here, compared with other stops |
| **Length** | How far along the stop people wait and a bus may stand |
| **Platform** | The platform's side |
| **Split** | Gives a twin its own name, so renaming one no longer renames the other |

A bus serves a stop only within 15 m of its lane. Two stops far apart cannot share a name: the
properties warn you.

## 2. Place a start point

1. In **Game logic**, click **Start points** and pick a start point object.
2. Click on a lane where your bus should appear: the depot yard or before the first stop, facing the
   way to drive.
3. Name it in the properties. Its name is shown where a drive starts.

## 3. Make a line and its trips

1. Press `L` (**Map → Lines and timetable…**).
2. Click **New line**. Rename it in the **Line** field if you like.
3. Leave **Players can drive it** ticked.
4. In the **Trips** tab, click **New trip**. The stops on the map turn orange.
5. Click the stops in the view in the order the bus calls at them.
6. Click **Done picking**. The way along the roads is found and drawn in blue, with its length.
7. Click **Make the way back** for the trip in the other direction.

Each trip's panel holds:

| Field | What it is |
| --- | --- |
| **Trip** | The trip's name (also its file name) |
| **Destination sign** | The terminus the bus shows |
| **Line** | The line number the trip shows |
| **Drives on** | **Roads (bus)** or **Tracks (tram, train)**, found from where the stops stand |
| Stops | In order, with **Move up**, **Move down**, **Take out**, **Show on the map** and **Add stop…** |

If the way breaks, the trip names the reason at the stop: no road within 15 m, roads not joined or
running the other way (see **View → AI lanes**), or a one-way lane driven against its direction.

### Times between stops

Under **Times from stop to stop** each stop gets the minutes from the stop before. Times you do not
set follow the distance. **Spread all by distance** spreads the whole trip again. A trip can have
several **Time profile**s (rush hour, night); each departure of a tour says which one it runs to.

### Choosing the way by hand

If the found way is not the one you want, click **Pick the paths on the map** and click lanes in the
view. Click **Done picking paths** when it is right. Paths picked by hand are kept when the stops
change. **Find the way automatically** goes back to the found way.

### A variant of a line

The copy button next to the line's name makes **a copy of the line with its trips and tours**, to
change into a variant.

## 4. Add a depot

AI buses drive the tours that no player drives. They come from a depot group in `ailists.cfg`.

1. Open the **Tours and times** tab.
2. If it says the map has no depot, click **Add a depot with OMSI 2's MAN buses**.

The depot then shows in **Traffic and AI** (step 8), where you can change its buses.

## 5. Make tours

A tour is one bus's day: the trips it drives one after the other, and when each leaves.

1. In **Tours and times**, click **New tour**.
2. Set **Fill from** and **to** (06:00 to 20:00 by default) and a **break** (5 min by default).
3. Click **Fill**. The tour's departures are replaced by the line's trips one after another, with a
   break after each.
4. Tick the days the tour runs. Make a second tour starting later, so two buses share the line.
5. For a weekend tour, make a third one, tick only Saturday and Sunday, and fill it with a longer
   break.

To add one departure, click **Next departure**: the next trip of the line, after the bus is back and
has had its break. Each departure can be deleted or changed by hand.

### Holidays

Open **Holidays and school holidays** in the Lines window. On a public holiday only the tours with
**Holidays** ticked run. School holidays pick the tours for school holidays or school days. The dates
come from the map's `Holidays.txt` (also in **Map → Calendar and place…**).

## 6. Choose the vehicles of a line

The **Vehicles** tab limits which buses drive the line, by date:

- **Only these types** or **Mostly these types** (with a **share of the tours**),
- **Fleet numbers of single tours**: a fixed bus for one tour.

Click **Add a vehicle…** to add a type. This writes OMSI 2's `car_use` files. A line with nothing set
takes any bus of the depot group.

## 7. Make the depot file

The depot file (`.hof`) gives the buses their destinations, the stop names on their displays and the
routes their IBIS knows.

1. Choose **Map → Depot files (destinations)…** (or the **Depot files** tab of **Traffic and AI**).
2. Click **Make one from the lines**. You get a destination for each trip's terminus, the stops it
   calls at, and a route with its code for each way a line goes. Long sign texts are split into two
   lines.
3. Check the lists:
   - **Destinations:** each with its **Code** and sign texts.
   - **Stops:** the names the displays show.
   - **Routes:** **Line**, **IBIS text**, **Destination** and **Stops in order**. The code is what
     the driver types: the line's number and two digits (7601 is line 76, route 01).
4. If you add lines later, the file says what **The lines need**. Click **Add them**; what is there
   stays.
5. Under **Used by the depot**, pick the depot group that uses the file, or click **Use it for every
   depot**.

The file is saved into the map's folder (where OMSI 2 looks) and into the content folder's `HOFs`
folder (where openOMSI looks). The map check names a depot file that is missing, belongs to another
map, or lacks what the lines need, and fixes each with one click.

## 8. Set the traffic

Press `T` (**Map → Traffic and AI…**). The window has four tabs.

### Over the day

How busy the map is at each hour: **Road traffic** and **Passengers**. 1 is normal; the game goes
along the line between the points.

1. Click **Typical day…** and pick **Quiet village**, **Town with rush hours** or **The same all
   day**.
2. Drag the points to change the curve.

### Vehicle groups

The AI's vehicles come in groups. A car group drives about on the roads; a depot sends out the buses
of the tours.

1. Click **New car group**, or pick an existing one on the left.
2. Click **Add a vehicle…** and pick cars from your install. A vehicle with more weight comes along
   more often.
3. Tick **Drives about on its own** for a group that should fill the streets.
4. A group can have **Its own traffic over the day**, for some days of the week only.

A depot group (**New depot**) lists its bus types with each bus's fleet number and plate, and its
**Depot file (.hof)**. If a group the map names is missing, **Make it with OMSI 2's vehicles** copies
the group of that name from OMSI 2's own maps.

### Parked cars and people

- **Parked cars:** what stands on the car parks.
- **People:** who walks about and waits at the stops.
- **Drivers:** who drives the AI's buses.

A car or person listed more often appears more often.

### Car parks

Parked cars need car parks on the map:

1. Press `4`, click **Car parks** and pick a car park object.
2. Click for one space. Or tick **In a row along a line**, set **Every**, then click the start and
   end of a row along the kerb.

## 9. Draw AI paths

Roads and junctions bring their own lanes. Draw AI paths where the AI must drive or walk off the
roads: a depot yard, a petrol station, a pedestrian square.

1. Press `4` and click **AI paths**.
2. Choose **Cars**, **People** or **Trains** (an invisible rail lane).
3. With **Draw new** selected, draw like a road: click the start, click the end. The path snaps to
   lane ends (green: joined, red: dead end). A car path runs the way you draw it.
4. Turn on **View → AI lanes** to see every lane with small arrows.

To change a path, click **Edit existing** and click it. Drag its dots; double-click the path to add a
point; click a dot and press `Del` to remove it. **Connect to** joins a point to another.

### Rules on a lane

A road's or object's properties hold **Path rules**: a **Speed limit, km/h**, **Traffic density ×**,
**Priority**, **Trucks (0: none)**, **No cars** or **Buses only**, each for one path (lane). The view
shows the lanes' numbers while the rules are open.

## 10. Check and drive

1. **Map → Check map…** names trips no way is found for, tours without departures or a depot,
   traffic groups missing from the AI lists, and stops no trip calls at. Use **Fix** where offered.
2. Save with `Ctrl+S`.
3. Press `F5`, choose your start point and a bus, and click **Start**. Set your line on the bus in
   the game (see [Driving](DRIVING.md)). AI buses drive the other tours.

## What you learned

- Stops, start points and car parks are in **Game logic** (`4`).
- A line has trips (a way from stop to stop) and tours (one bus's day).
- A depot lets AI buses drive tours; the depot file feeds displays and IBIS.
- **Traffic and AI** (`T`) sets how busy the map is and what drives on it.
- AI paths fill the places roads do not reach.

## Next

- [Terrain](MAPPER_TERRAIN.md): shape and paint the ground round your line.
- [openMapper tools](MAPPER_TOOLS.md): every option of the Lines, Traffic and Depot files windows,
  plus signal routes, chrono scenarios and the trolleybus wire.
- [Routes](ROUTES.md) for how the game reads a map's lines.
