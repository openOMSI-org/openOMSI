# Weather and season

How to choose the weather and the season for a drive, how the natural weather model works, and how
to drive in the live weather of a real place (METAR and Tomorrow.io). Map authors find the
Tomorrow.io configuration format at the end.

## Choosing the weather

In the launcher, open **Drive → Day & weather** (in the Classic layouts: the *Day & weather*
button beside the date). The weather tiles:

| Weather | What it is |
| --- | --- |
| **Natural weather** (default) | a physical weather model that develops through the day and the season |
| **Custom weather** | visibility, brightness, wind, temperature, humidity, clouds, rain or snow and road state, set by hand |
| **Current weather** | the METAR report of the airport nearest the map, fetched when the game starts |
| **Weather cycle** | a new installed weather every 25–60 game minutes, as the month allows |
| **Tomorrow.io** | live weather for the map's configured areas (see [Tomorrow.io](#tomorrowio-live-weather)) |
| installed presets | the `Weather/*.owt` files, with their names and descriptions |

Presets that do not fit the season are hidden; **Show the N that do not fit the season** shows
them. In the game, **World options** in the game menu changes the weather while you drive.

On a server the weather, time and date are the server's.

### Custom weather

Press **Custom weather**, or **Edit selected weather as custom** to start from the weather chosen.
The editor sets:

- visibility and brightness;
- wind direction and speed;
- temperature and humidity (it shows the dew point);
- cloud cover;
- wet roads;
- precipitation (none, rain or snow) and its strength;
- snow cover and snow on the road.

### Current weather (METAR)

**Current weather** appears when the map has an airport near it. Another airport can be chosen by
its four-letter ICAO code under **Airport**.

To follow the METAR report all the time, switch on **Settings → Gameplay → Sync the weather with
METAR**. The weather is then refreshed every few minutes and cannot be changed by hand.
`metar_station` in `settings.cfg` names another airport.

### Weather cycle

The cycle moves on to another installed weather every 25–60 minutes of game time. It picks one
that suits the month (snow only from November to March, no frost in summer, no heat in winter)
and is close to the one before. Every change comes in over a few minutes.

### Weather presets

The installed presets change the light: overcast takes the sun away, rain and fog thicken the air,
and a snow preset puts any map into its winter textures with snow cover.

### From the command line

`--weather natural`, `--weather cycle`, `--weather tomorrow` or a file such as
`--weather Weather/Bodennebel.owt` (see [Command line](COMMAND_LINE.md)).

## Natural weather

**Natural weather** is a physical model of a column of the atmosphere over the map that runs with
the clock:

- Highs and lows pass through. Falling pressure brings a veil of cirrus, then a grey deck, then
  rain. Behind a low the air sinks and clears.
- The sun warms the ground through the clouds; the ground cools by radiation at night, far more
  under a clear sky.
- The day's heat mixes the air up from the ground. Where it reaches the condensation level,
  cumulus forms, and dissolves again in the evening.
- A calm clear night cools the air to its dew point and leaves fog that the morning sun burns off.
- Rain washes dust out of the air, a still high collects it, and humid air swells it into haze.

So one day is grey from morning to night, the next opens up after a foggy morning, and an
afternoon of showers is followed by a clear evening. Each day follows from the one before (the
model starts three weeks back), with the climate of the season and the latitude.

It sets everything a weather sets: visibility, wind, temperature, rain or snow, and the wet road,
in every graphics mode. Enhanced also takes its cloud amounts and its air.

## Season

The **Season** choice in **Day & weather**:

- **By date** (default): as in OMSI, the date decides.
- **Spring**, **Summer**, **Autumn** or **Winter**, each with **Early**, **Mid** or **Late**.

The phases are the meteorological season's three months (spring: March, April, May; half a year
later on a map south of the equator). Choosing one moves the date to a typical day of it (the
10th, 15th or 20th), keeping the year and the time of day. So the sun, the length of the day, the
natural weather's temperature and snow, the street lamps, the timetable and the holidays all
follow.

The map's seasonal textures follow the phase:

| Phase | Look |
| --- | --- |
| early spring | still bare (`Winter`) |
| late summer | dry (`SummerDry`) |
| early autumn | green, a quarter of the trees turned |
| mid autumn | coloured (`Fall`), four in five turned |
| late autumn | bare, some still with leaves |

Each plant turns by its own number, the same every time and for every player in a session.

From the command line: `--season autumn-late` (`--season autumn` is mid autumn). A `--date`
already in the phase's month is kept.

## Tomorrow.io live weather

**Tomorrow.io** follows the real weather of the area nearest to you on the map while you drive.
The map needs an `openomsi_weather.cfg` that places its areas on the globe, and each player needs
their own Tomorrow.io API key.

### Setting it up

1. Get a Tomorrow.io API key for your account.
2. Save it as plain text in `~/.openomsi/tomorrow-api-key.txt` (on Windows
   `%USERPROFILE%\.openomsi\tomorrow-api-key.txt`).
3. Put the map author's `openomsi_weather.cfg` beside the map's `global.cfg`, or make one with the
   editor (see [For map authors](#for-map-authors)). OMSI 2 ignores the file; `global.cfg` is not
   changed.
4. On **Drive → Day & weather**, choose **Tomorrow.io**, then the map and the bus, and start.

Or start with `--weather tomorrow`.

### What it sends and when

- Only the active area's coordinates and your API key go to Tomorrow.io, never your position.
  Never put a key into a map config, a shared preset or a bug report.
- Only the active area is fetched (Tomorrow.io's metric Realtime endpoint). Places at the same
  coordinates share a cache across areas, maps and restarts, kept in `~/.openomsi/weather-cache`.
- At most one new request every 180 s, 20 an hour and 450 a day, counting failed ones. Check your
  [account's allowance](https://support.tomorrow.io/hc/en-us/articles/20273728362644-Free-API-Plan-Rate-Limits);
  other programs using the same key are not counted here.
- A rate limit (HTTP 429) or a rejected key pauses requests for an hour. Requests time out after
  8 s.
- A missing config or key, or a failed download, keeps the current weather and shows a message,
  once per cause.

### How it plays

The closest area anchor selects the area. Near a border a 200 m tolerance keeps the previous area.
A change blends over 60 simulation seconds, updating temperature and snow. The measured
precipitation, visibility and cloud cover go into the weather model, which decides how it looks;
roads wet and dry over time. Falling snow does not create old snow depth on the road.

Tomorrow.io takes precedence over the METAR sync. Choosing a custom weather ends it. In
multiplayer a joining player follows the host and fetches nothing.

`OMSI_TOMORROW_API_KEY` and `OMSI_TOMORROW_KEY_FILE` can give the key another way (see
[Environment variables](DEBUG_FLAGS.md)).

### For map authors

Use the independent [community weather editor](https://openomsi.mxnticek.eu/weather/)
([English](https://openomsi.mxnticek.eu/en/weather/)):

1. Select your map's `global.cfg`. The editor reads it locally in your browser.
2. Create areas, assign spawn points to them and enter `latitude;longitude`. Several spawn points
   can share a location. With real place names you can group by the name before the first comma;
   review the result.
3. For map 400 v1.5.2 the website offers approximate town-centre coordinates (check them for
   other versions). Other maps use GPS by hand, or the optional online search after your consent.
4. Export `openomsi_weather.cfg` beside `global.cfg`, and reload the map after editing.

The website is maintained separately from openOMSI; the game reads only the exported file. The
optional place search sends place names and the country to its provider, and needs no
Tomorrow.io key. Game X/Y coordinates are not latitude and longitude.

### The config format

UTF-8 JSON, version 1, beside `global.cfg`. A copy at the same `maps/<map>/` path in the content
folder overrides it.

```json
{
  "version": 1,
  "refresh_minutes": 15,
  "groups": [{
    "name": "City centre",
    "latitude": 50.0755,
    "longitude": 14.4378,
    "spawnpoints": [{"object_id": 7, "tile": [0, 0], "name": "Example"}]
  }]
}
```

- `refresh_minutes`: 15 to 1440.
- A spawn point is identified by its object ID and tile; its position comes from the map's entry
  points. A spawn point belongs to one group at most. Spawn points in no group are no weather
  anchors.
- Unknown fields, invalid GPS, empty groups and duplicate assignments are rejected.
