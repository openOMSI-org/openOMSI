# HTML textures

An `[htmltexture]` draws a texture from an HTML page with JavaScript: an IBIS, a passenger
information screen, a ticket machine, a road sign. It replaces OMSI 2's pixel-by-pixel
`[scriptTexture]` drawing for most displays. This page is for vehicle and scenery modders.

> **Note:** This API is still under development. Breaking changes may occur.

## Which engine

openOMSI has two HTML engines. Both use the same `[htmltexture]` setup and the same
`window.omsi` API described on this page.

| | Classic engine (default) | Modern engine (opt-in) |
|---|---|---|
| Use it for | Simple screens, fastest on slow devices | Complex layouts, code from the web |
| CSS | Block flow, basic properties | Flexbox, positioning, borders, gradients, `calc()` |
| JavaScript | ES5 subset plus arrow functions | Full modern JavaScript, modules, `fetch` of the page's files |
| Files | One HTML file with images | A small site: `.html`, `.css`, `.js` files |
| Turn on | Nothing to do | `<meta name="omsi-engine" content="modern">` in the page |
| Reference | [Classic engine](#classic-engine) | [Modern HTML engine](HTMLTEXTURE_MODERN.md) |

Start with the classic engine. Switch a page to the modern one when you miss flexbox,
positioning or modern JavaScript. Pages without the `<meta>` line always run on the classic
engine.

## Minimal example

A speed display on a bus. A working demo page is in `docs/examples/htmltexture/demo.html` in
the openOMSI repository.

### 1. The model.cfg

```text
[htmltexture]
512
256
html\speed.html

##########################################

[mesh]
display.o3d

[matl]
display.dds
0

[useHtmlTexture]
0
```

1. `[htmltexture]` declares the page: width and height in pixels, then the page's path.
2. On the display's `[matl]`, `[useHtmlTexture]` names the page by its number, counted from 0.
   The material's texture (`display.dds`) is replaced by the page.

### 2. The page

`html\speed.html`:

```html
<style>
  body { background: black; color: #ffb000; font-size: 96px; text-align: center; }
</style>
<div id="speed">--</div>
<script>
  window.omsi = window.omsi || {};
  window.omsi.update = function (d) {
    document.getElementById('speed').textContent =
      d.vehicle.motion.speedKmh + ' km/h';
  };
</script>
```

The game calls `omsi.update` whenever something the page might care about has changed. Do not
poll: wait for `update`, read the data and redraw what changed.

### Where the page is found

- The path is relative to the vehicle. `Vehicles\MAN_NewLionsCity\html\index.html` is written as
  `html\index.html`.
- The game looks in the model's folder first, then in the folder of the `.bus` (or the `.sco` for
  scenery objects).
- Images and other files the page names are looked up next to the page.
- Pages are read as UTF-8.

### Script texture numbers

An `[htmltexture]` is also counted as a `[scriptTexture]`. If it follows script texture 6, it is
script texture 7, but still HTML texture 0. So `[matl_transmap]` `\S:7` uses its alpha, and
`[useScriptTexture] 7` binds it as well.

A full example with a light map, a freetex and a transparency mask:

```text
[htmltexture]
1024
512
html\index.html

[mesh]
generic\AFR4\AFR4_Display.o3d

[visible]
AFR4_Display
1

[matl]
afr4.dds
0

[useHtmlTexture]
0

[matl_lightmap]
1px_lm.bmp
0.5

[matl_freetex]
afr4.dds
AFR4_ST_texture

[matl_transmap]
\S:7

[viewpoint]
3
```

## Displays and surfaces

### Displays (default)

A page is a **display** by default: drawn unlit, at its own brightness, like a bus's own screen.

### Surfaces: `lit`

A road sign, a poster or a notice board should take the world's light instead. Put `lit` on the
line after the page's path:

```text
[htmltexture]
512
160
sign\route.html
lit
```

- The page is shaded like every other material: sun, street lamps, fog and eye adaptation. A sign
  dims at night.
- Its bright pixels glow as a surface's do, and the slot is no longer one of the bus's own screens.
- `lit` belongs to the page. It applies to every material that shows the page, on scenery objects
  and vehicles alike.
- Omsi.exe knows neither `[htmltexture]` nor `lit`.

### LED destination signs

A page on an LED matrix glows like the Krueger and K++ panels in the Enhanced renderers when:

- the page is not `lit`, and
- its material has a dot mask: the `\S:n` mask above, or a picture of the dots as
  `[matl_transmap]` (black between the dots), and
- a `[matl_lightmap]` that is white all over.

The lit dots then glow at the **LED glow** setting's brightness, with bloom. The sign keeps that
brightness at night, even inside the cab (behind a coach's windscreen). A script texture panel in
the cab, such as a dashboard LCD, dims instead. `[led_glow_effect]` sets the strength per material
(see [Modding](MODDING.md#glowing-materials)).

## The omsi object

The game gives the page one global object, `window.omsi` (also just `omsi`). The page uses it to:

1. **read** what the vehicle is doing: speed, doors, line, next stop;
2. **write** back: set a script variable, press a trigger, choose a route;
3. **react** to touches and clicks on the texture.

| Member | What it is |
|---|---|
| `omsi.update = function (d) {…}` | Your function. Called when data changed. |
| `d.num`, `d.str` | Script variables that changed since the last call (all on the first call) |
| `d.vehicle` | The same object as `omsi.vehicle` |
| `d.vars` | The same object as `omsi.vars` |
| `omsi.vehicle` | The vehicle's state with the same names on every bus. Readable from timers too. |
| `omsi.vars.num`, `omsi.vars.str` | Latest value of every variable of the bus's variable lists, by lower-case name |
| `omsi.getVar(name)` | One variable, any letter case: a number, else text, else `undefined` |
| `omsi.apiVersion` | `1` |

### Why `omsi.vehicle`

OMSI buses name their variables as their authors like (`door_0`, `Fahrertuer_Rechts`,
`elec_busbar_main` …). `omsi.vehicle` translates them into one fixed layout, so a page works on
every bus.

- Things a bus does not have are `null` (numbers) or `false` (switches), never missing. So
  `omsi.vehicle.engine.rpm` never throws.
- Numbers are rounded (speed to 0.1, rpm to 1 …), so a value that only wobbles in the last digit
  does not cause a redraw every frame.
- Values specific to one bus (a mod's own switch, a battery voltage) are not standardised. Read
  them with `omsi.getVar("their_variable_name")`.

## Time, date and weather

| Member | What it is |
|---|---|
| `omsi.time.hour`, `.minute`, `.second` | Simulation clock, as numbers |
| `omsi.time.asString` | `HH:MM:SS` |
| `omsi.timestamp` | Seconds since 1970-01-01 on the simulation calendar, no time zone |
| `omsi.date.day`, `.month`, `.year` | Simulation date, as numbers |
| `omsi.date.asString` | `DD.MM.YYYY`, or `MM/DD/YYYY` when `locale` is `en` |
| `omsi.locale` | Interface language as an ISO 639-1 code (`en`, `de` …) |
| `omsi.weather` | The weather, `null` until known (below) |

These are set before `omsi.update` runs and change with the simulation clock, once per second.
Use `omsi.locale` to pick your texts, with English as fallback.

| `omsi.weather.` | Meaning |
|---|---|
| `temperature` | °C |
| `humidity` | Relative, 0-1 |
| `visibility` | Fog range, m |
| `clouds` | The weather's cloud type, for example `-1` (clear) or `Cumulus 2` |
| `precip` | 0 none, 1 rain, 2 snow |
| `precipRate` | 0-1 |

## Vehicle state

`omsi.vehicle` (version 1):

| Path | Meaning |
|---|---|
| `api` | `1` |
| `info.number`, `.ident`, `.yard`, `.route`, `.nextStop` | Text variables of the bus |
| `motion.speedKmh`, `.heading`, `.pitch`, `.bank`, `.steeringDeg` | Movement |
| `motion.x`, `.y`, `.z`, `.odometerKm` | Position and distance |
| `engine.running`, `.rpm`, `.throttle`, `.brake`, `.clutch`, `.gear`, `.tankContent` | Drive train and pedals |
| `electrics.on`, `.busbarMain`, `.busbarAvailable`, `.failure` | On-board electrics |
| `battery.on` | Battery switch, `null` if the bus has none |
| `doors.count`, `.anyOpen` | Number of door leaves (`door_0`, `door_1` …), whether any is open |
| `doors.list[i].number`, `.open` (0-1), `.isOpen` | One door; `list[0]` is door 1 |
| `passengers.onboard` | People aboard |
| `passengers.entries[i]`, `.exits[i]` | `number`, `open`, `requested` (from `PAX_Entry/Exit<n>_Open/_Req`) |
| `lights.headlights` | 0 off, 1 parking, 2 dipped, 3 main beam |
| `lights.brake`, `.reverse`, `.fog`, `.interior` | Lamps (`interior` 0-1) |
| `lights.indicator` | 0 off, 1 left, 2 right, 3 hazard |
| `lights.indicatorLeft`, `.indicatorRight`, `.hazard` | The indicator lamps and the hazard switch |
| `brakes.parking`, `.stop`, `.kneeling` | Switches |
| `wipers.running` | Windscreen wipers |
| `cabin.temperature` | Cabin air, °C |
| `condition.dirt`, `.crashes`, `.lastImpactKJ`, `.streetCondition` | Wear and road |
| `train.trailers` | Coupled vehicles behind this one |
| `route.*` | Line, stops and delay (next section) |

`doors.anyOpen` follows the passenger door variables (`PAX_*_Open`) when the bus has them, and
the door leaves (`door_<n>`) otherwise. Some mods use `door_<n>` for other things, so a few buses
may behave oddly here.

## Route and timetable

Everything about the current trip is in `omsi.vehicle.route`.

| Path | Meaning |
|---|---|
| `route.active` | `true` when the bus has a timetable (a duty) |
| `route.source` | Where the stops come from: `"timetable"`, `"ibis"` or `"none"` |
| `route.line` | The line text |
| `route.destination` | The text on the destination sign |
| `route.stops[i]` | One stop (fields below) |
| `route.current` | The stop the bus is at or heading for |
| `route.terminus` | The last stop |
| `route.nextIndex` | Index of the next stop; `null` when `active` is `false` |
| `route.delaySec` | Delay in seconds; negative is early |
| `route.ibis.line`, `.suffix`, `.routeIndex`, `.terminusIndex`, `.terminusCode` | The raw numbers the IBIS holds |

A stop has `index`, `name`, `arrival` and `departure` (`"HH:MM"`), `arrivalSec` and
`departureSec` (seconds after midnight), `served` and `current`.

- `served` becomes `true` once a stop has been handled. When the last stop is served, the trip is
  over. A page can use that to start the return trip.
- `route.stops` can be empty: without a timetable and with nothing typed into the IBIS, there is
  nothing to show. Check `stops.length` before drawing.
- Without a duty (`active` is `false`) the game does not advance the stops. A page that shows
  progress must keep its own counter.

## Departures of a stop

`omsi.getDepartures(stop)` returns the departures at a stop in the next 2 hours, soonest first,
at most 20. `stop` is the stop's name as in the timetable (`Busstops.cfg`), any letter case. It
works on vehicle and scenery pages.

```js
omsi.update = function () {
    var list = omsi.getDepartures('Hauptbahnhof');
    for (var i = 0; i < list.length; i++) {
        var minutes = Math.round((list[i].time - omsi.timestamp) / 60);
        console.log(list[i].line + ' ' + list[i].destination + ' in ' + minutes + ' min');
    }
};
```

| Field | Meaning |
|---|---|
| `line` | The line text |
| `destination` | The destination text |
| `time` | Departure as a timestamp (scale of `omsi.timestamp`), delay included |
| `stopsAway` | Stops until this one, this one counted (`1`: next stop, `0`: standing here); `null` while not on the road |
| `load` | How full, 0-1 of seats and standing places; `null` unless passengers are simulated |
| `delaySec` | Delay in seconds once on the road, else `null` |
| `lastTrip` | `true` for the line's last departure at this stop today |

- The first call for a stop returns `[]`. The game fills the stop a moment later and calls
  `omsi.update` again, so ask inside `update`.
- The game keeps the 8 stops asked for most recently. Asking for a ninth drops the oldest; asking
  for that one again gives `[]` for a moment.
- The lists are renewed about once a second.
- Without a timetable on the map the list stays empty.

## Controlling the vehicle

| Function | What it does |
|---|---|
| `omsi.setVar(name, value)` | Writes a script variable |
| `omsi.trigger(name)` | Presses a trigger of the bus's scripts, like a cab button |
| `omsi.setRoute(index)` | Types route `omsi.depot.routes[index]` into the IBIS: line, route and destination |
| `omsi.setLine(text)` | Sets the first route of that line in the depot file. Matches the whole text (any case) or the leading digits. |
| `omsi.setDestination(index)` | Changes only the destination sign, to `omsi.depot.destinations[index]` |
| `omsi.clearLine()` | Clears the IBIS: no line, no route |
| `omsi.setNextStop(index)` | With a timetable: jumps to stop `index` of the trip, forwards or backwards |

- These calls are applied by the game a moment later, not within the same script run. Read
  `omsi.vehicle` again in the next `update`.
- `setNextStop` works **both ways**. Forwards, the stops in between are skipped (not served).
  Backwards (a "back one stop" button), the stops from `index` on are due again and their `served`
  is `false`. Asking for the stop the bus already heads for changes nothing.
- `clearLine()` only touches the IBIS. It does not end the duty, so `route.active` may still be
  `true` afterwards. Do not switch screens on that alone.

## The depot file

`omsi.depot` describes what the bus's depot file (`.hof`) offers, so a page can build a line
picker or destination list without hard-coding anything. It is filled once, before the first
`update`. Without a depot file it stays empty.

| Member | What it is |
|---|---|
| `omsi.depot.name` | The depot file's name |
| `omsi.depot.routes[i]` | Every route (fields below) |
| `omsi.depot.lines[]` | The same routes grouped by line: each entry has `line` and its `routes` |
| `omsi.depot.destinations[i]` | `index`, `code`, `id`, `name` |

A route has:

| Field | Meaning |
|---|---|
| `index` | Position in the list; the number for `setRoute` |
| `code` | The IBIS route code (line × 100 + route number, by the depot file's rule) |
| `name` | Name of the trip |
| `line` | The line text |
| `terminusCode` | The route's destination code |
| `destinationIndex` | Index in `destinations`, or `-1` |
| `destination` | Text of that destination sign |
| `first`, `last` | Names of the first and last stop |
| `stops` | Names of all stops, in order |

Many depot files leave the line column as a placeholder like `XXX`. The game then takes the line
from the route code (code 1203 gives line `12`).

A line picker that starts the first route of a line:

```js
function pickLine(text) {
    var routes = omsi.depot.routes, i;
    for (i = 0; i < routes.length; i++) {
        if (routes[i].line === text) {
            omsi.setRoute(routes[i].index);
            return;
        }
    }
}
```

A return trip is usually a route on the same line whose `first` is your `last` and whose `last`
is your `first`.

## Touch and mouse

Presses, releases and moves of the pointer on the texture become events on the element under the
pointer, bubbling up through its parents: `pointerdown`, `pointerup`, `mousedown`, `mouseup`,
`click`, `mousemove`. A click needs both press and release on the page.

Three ways to listen:

```html
<button onclick="doSomething()">Press</button>
```

```js
el.onclick = function (event) { /* ... */ };
el.addEventListener('click', function (event) { /* ... */ });
```

The event has `type`, `x`, `y` (texture pixels), `target`, `stopPropagation()` and
`preventDefault()`.

There is no keyboard input, no hover and no drag and drop. Build keypads and lists from buttons
or boxes and react to `click`.

## Scenery objects

A scenery object (`.sco`) can show pages too: `[htmltexture]` in its model config and
`[useHtmlTexture]` on the material, as in a vehicle. `lit` works the same way. The page is looked
up in the model's folder, then in the `.sco`'s folder.

- A scenery page has everything above **except** `omsi.vehicle`, `omsi.depot` and the IBIS
  functions (`setRoute`, `setLine`, `setDestination`, `clearLine`, `setNextStop`). `d.vehicle` is
  absent too.
- The variables are those of the object's script. An object without a script still shows its page.
- Pages run only within 60 m of the camera.
- Clicks reach them from up to 4 m away, unless the bus is in front of the page.

## Classic engine

The classic engine is small and fast. It is not a browser, so a page that looks fine in Chrome may
need changes. When something does not show, check these lists first. For more, use the
[modern engine](HTMLTEXTURE_MODERN.md).

### HTML

- Nested elements, `<style>`, `<script>`, inline `style=""`, `id`, `class`, entities such as
  `&amp;`.
- Block elements: `div`, `p`, `h1`, `h2`, `h3`, with browser-like default sizes and margins.
- Inline elements: `span`, `b`, `strong`, `i`, `em`, `a`, `small`, `u`, `label`, `code`, `<br>`.
- `<button>` has a default look: grey, rounded, padded.
- `<img src width height>` with bmp, dds, tga, jpg and png, looked up next to the page. `img.src`
  can be changed from a script. Pictures are cached, missing ones too.
- `<head>`, `<title>`, `<meta>`, `<link>` are read and ignored.

Not available: canvas, tables, lists (`ul`/`li`), forms and `input`, iframes, video, SVG.

### CSS

- Selectors: tag, `#id`, `.class`, `*`, compounds (`div.a#b`), descendant chains
  (`.panel span`) and comma lists.
- Properties: `color`, `background`, `background-color`, `background-image: url()`,
  `background-size`, `background-repeat`, `background-position`, `font-size`, `font-weight`,
  `text-align`, `line-height`, `margin` and `padding` (with `-top`/`-right`/`-bottom`/`-left`),
  `width`, `height`, `display` (`none`, `inline`, `inline-block`), `visibility`, `border-radius`.
- Units: `px`, `%`, `em`, `rem`, `pt`, `vw`, `vh`.
- Layout is block flow with wrapped inline text. `display: inline-block` boxes (and buttons) are
  laid out in rows that wrap, good for keypads and lists. A box without `width` is as wide as its
  content.

Not available: flexbox, grid, floats, `position`, borders, gradients, shadows, `opacity`,
`overflow`, animations and transitions, custom fonts. A box does not clip its content, so shorten
texts that are too long yourself.

### JavaScript

ES5 plus arrow functions:

| Area | Available |
|---|---|
| Language | `var`, `let`, `const`, functions, `if`, `for`, `while`, `break`, `continue`, `typeof`, objects, arrays |
| Globals | `Math` (`round`, `floor`, `ceil`, `abs`, `min`, `max`, `sqrt`, `pow`, `trunc`, `sin`, `cos`, `PI`), `parseInt`, `parseFloat`, `String`, `Number`, `isNaN`, `NaN`, `Infinity`, `Object.keys`, `console.log/warn/error` |
| Strings | `split`, `replace`, `trim`, `indexOf`, `charAt`, `slice`, `substring`, `startsWith`, `endsWith`, `includes`, `toUpperCase`, `toLowerCase`, `padStart`, `padEnd`, `repeat` |
| Numbers | `toFixed`, `toString` |
| Arrays | `push`, `pop`, `shift`, `slice`, `join`, `indexOf`, `includes`, `map`, `filter`, `forEach` |
| Timers | `setTimeout`, `setInterval`, `clearTimeout`, `clearInterval` (real time, at most 64 active) |
| DOM | `document.getElementById`, `document.querySelector`, `document.body`, `createElement`, `appendChild`, `removeChild`, `remove`, `parentNode`, `getAttribute`, `setAttribute`, `classList` (`add`, `remove`, `toggle`, `contains`), `textContent`, `innerText`, `innerHTML` (with markup), `className`, `id`, `element.style.*` |

Not available: `Date` (use `omsi.time`), `JSON`, `fetch` and any network access, `localStorage`,
`Promise`/`async`, classes, `new`, regular expressions, `switch`, `do … while`, `try/catch`. Use
`if/else` chains and `while` loops instead.

- Timers run while the page is drawn and use real time, not simulation time. Calls beyond 64
  active timers are ignored.
- A single script run may execute about 400,000 steps, and nesting (function calls, elements) is
  capped at 48 levels. An endless loop is cut off.

### Redrawing

A page is redrawn when the DOM or the styles change. The best pattern: in `update`, rebuild a
block with `innerHTML`, and only when the data actually changed (compare with the last values).
Redrawing on every call wastes time on a slow phone.

## Tips

- **Start from `demo.html`** and change one thing at a time. If the texture goes blank, the last
  change is the culprit.
- **Log.** `console.log(...)` writes to the game's log with the prefix `htmltexture console:`. It
  is the quickest way to see what `omsi.vehicle` or `omsi.depot` contain on your bus.
- **Guard for missing data.** A bus without a depot file has no routes, a bus without a timetable
  has no stops. Check lengths before looping.
- **Test on the real bus.** Each bus fills its variables differently, and some values will be
  `null`.
- **Keep buttons big.** The texture is often small on screen. 60 px high is a good minimum.
- **Text is drawn in Inter.** Characters Inter lacks (Hangul, CJK …) use Malgun Gothic on
  Windows or AppleGothic on macOS. `OMSI_HTML_FALLBACK_FONT` (and `OMSI_HTML_FALLBACK_FONT_BOLD`)
  can name another font file.
