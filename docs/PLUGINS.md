# Plugins

Plugins add features to openOMSI: panels on the screen, dispatchers, fare systems, HUDs, telemetry for other programs, weather and time control. This guide is for players who install plugins and for anyone who wants to write one. Every function is listed in the [plugin API reference](PLUGIN_API.md).

## Kinds of plugins

openOMSI runs three kinds of plugins.

| Kind | File | Made with | Runs on |
| --- | --- | --- | --- |
| Lua plugin | `.lua` | any text editor | Windows, macOS, Linux, Android |
| Compiled plugin | `.oop` | the openOMSI Development Tools (Lua or Rust) | Windows, macOS, Linux, Android |
| OMSI 2 plugin | `.opl` + DLL | as for OMSI 2 | Windows; macOS and Linux through Wine |

**Lua plugins** are plain text. There is nothing to compile, and the game loads a plugin again within a second after you save its file. They reach the whole game through the plugin API: the player's bus, the duty and timetable, the map, AI traffic and people, weather and time, camera, input, sound, panels on the screen, storage, settings and the LAN session. That is about 310 functions and 65 events.

**Compiled plugins** (`.oop`, "openOMSI plugin") are build output, not sources. One file works on every system. The code is either Lua compiled into an obfuscated chunk, or WebAssembly (usually from Rust). They run in a sandbox with only the permissions they declare. See [Compiled plugins](#compiled-plugins).

**OMSI 2 plugins** are the original game's `.opl` files with their DLLs, loaded unchanged. See [OMSI 2 DLL plugins](#omsi-2-dll-plugins).

A vehicle can also carry its own scripts, which run as plugins while the player drives it. See [Scripts in a vehicle](#scripts-in-a-vehicle).

## Installing and managing plugins

### Turn plugins on

On a fresh install, plugins are off.

1. Open the launcher's **Settings → General**.
2. Under **Interface & online**, switch off **Disable plugins**.

The setting applies from the next game. With it on, no Lua plugins and no OMSI 2 plugins run, including vehicle scripts. The vehicles' normal `.osc` scripts always run.

### Where plugins go

Plugins live in the `plugins` folder of the game, and of every other content root.

| Path | What it is | Its data folder |
| --- | --- | --- |
| `plugins/<name>.lua` | a one-file plugin | `plugins/<name>.data/` |
| `plugins/<name>/main.lua` | a plugin with several files | `plugins/<name>/data/` |
| `plugins/<name>.oop` | a compiled plugin | `plugins/<name>.data/` |
| `plugins/**/<name>.opl` | an OMSI 2 plugin and its DLL | - |

In a folder plugin, `require("util")` loads `plugins/<name>/util.lua` (or `util/init.lua`). Its pictures, sounds and data files sit next to the code. Other `.lua` files in a folder are not started on their own, so modules stay modules.

When a `.lua` plugin and an `.oop` have the same name, the `.lua` wins. That lets an author work on the source while the compiled copy is installed.

### The launcher's Plugins page

The launcher's **Plugins** page has two tabs.

- **Installed** lists every plugin in the `plugins` folders: Lua, `.oop` and `.opl`. Switch each one on or off without moving its files, open its **Settings**, or press **Plugins folder** to open the folder.
- **Browse** lists the plugins of the [plugin catalog](#distributing-a-plugin). **Install**, **Update** and remove them from there.

A plugin switched off is left out of the next game. Its files and settings stay.

> **Note:** The **Settings** button appears only after the plugin has run once, because the plugin declares its settings when it starts.

### Files the launcher and game share

| File | Contents |
| --- | --- |
| `~/.openomsi/plugins-off.txt` | the plugins switched off, one per line, in lower case |
| `~/.openomsi/plugins-status.json` | what the last game did with each plugin |
| `<data folder>/settings.schema.json` | the settings a plugin declared |
| `<data folder>/settings.json` | the values the player chose |

In `plugins-off.txt` each plugin is named by its path under `plugins`: `foo.lua`, a folder plugin as `foo/` (for `foo/main.lua`), `foo.oop`, or an `.opl` path such as `pablo/mbas.opl`. So `foo.lua` and `foo/main.lua` are two different plugins. A bare name like `foo`, written by an older launcher, still switches off every Lua or `.oop` plugin of that name.

`plugins-status.json` lists every plugin by the same names with its `state`: `loaded`, `off`, or `failed` with the load error (for example a syntax error). A plugin the game switched off during play for its errors also has `switched_off` with the error.

On Windows `~` is `%USERPROFILE%`.

## Your first Lua plugin

This takes about five minutes. You need openOMSI with plugins turned on (see [Turn plugins on](#turn-plugins-on)) and a text editor.

1. Open the `plugins` folder next to the game. The launcher's **Plugins** page has a **Plugins folder** button. Create the folder if it is missing.
2. Create a file `hello.lua` in it and paste this:

   ```lua
   -- plugins/hello.lua

   -- Greet the driver when they get into a bus.
   omsi.on("vehicle", function(name)
     if name then
       omsi.message("Good morning! Today you drive the " .. name, 6)
     end
   end)

   -- A small panel with the speed, top left.
   local function show()
     if not omsi.has_vehicle() then
       omsi.ui.remove("speed")
       return
     end
     omsi.ui.set("speed", {
       anchor = "top_left", x = 16, y = 60, width = 200,
       children = {
         { type = "text", text = "Speed", size = 12 },
         { type = "text", text = omsi.fmt.speed(omsi.speed()), size = 24, weight = "bold" },
       },
     })
   end
   omsi.every(0.5, show)

   -- A warning above 50 km/h, checked once a second.
   omsi.every(1, function()
     local kmh = omsi.var("Velocity")
     if kmh and kmh > 50 then
       omsi.message(string.format("Slow down: %.0f km/h", kmh), 1)
     end
   end)
   ```

3. Start a map with a bus. The greeting appears, the panel shows your speed, and above 50 km/h you get a warning.
4. While the game runs, change `50` to `30` and save the file. Within a second the game says "Lua plugin hello reloaded" and the new limit applies.
5. Make a mistake on purpose, for example delete an `end`, and save. The error appears on the screen and in `game.log`. Fix it and save again.

Next steps:

- Read [How a plugin runs](#how-a-plugin-runs) for events, timers and reloading.
- Look at the [example plugins](#example-plugins).
- Browse the [API reference](PLUGIN_API.md) for what else you can read and change.

## How a plugin runs

The file's top level runs once, when the game starts (and again after each reload). After that the plugin reacts to **events**. Register a handler with `omsi.on(event, fn)`, as many as you like, or define a global function `on_<event>`:

```lua
function on_frame(dt)
  -- about 60 times a second: keep it short
end

omsi.on("stop_arrive", function(name, id, number, delay)
  omsi.ui.toast(string.format("%s, %s", name, omsi.fmt.delay(delay)))
end)
```

The [event list](PLUGIN_API.md#events) has all 65 events: the game's (a crash, a ticket sold, a trip done), state changes (a door opened, the engine started, the bus arrived at a stop, a new minute, a red light), panel clicks, messages from other plugins and from other players. An event the game has to work out by comparing two frames (doors, gears, stops, the light ahead) is only worked out while a plugin listens to it.

### The plugin's life

| Event | When |
| --- | --- |
| `start` | right after the plugin was loaded, also after a reload |
| `vehicle` | the player got into a vehicle, changed it, or left it (`nil`) |
| `frame` | every frame, after the bus's own scripts; not while paused |
| `pause`, `resume` | the game was paused, and goes on |
| `stop` | the game ends, or the plugin is about to be reloaded |

When you save a plugin's file, the game stops it (`stop`), loads it again and runs it from the top. Its panels go, its `omsi.data` and storage are written first and read back.

### Timers, watches and your own events

| Function | What it does |
| --- | --- |
| `omsi.after(seconds, fn)` | runs `fn` once, later |
| `omsi.every(seconds, fn)` | runs `fn` regularly |
| `omsi.watch(name, fn)` | runs `fn(new, old)` when a bus variable changes |
| `omsi.watch(kind, name, fn)` | the same for `"var"`, `"str"`, `"sys"` or `"info"` values |
| `omsi.cancel(id)` | stops a timer, watch or hotkey |
| `omsi.emit(event, ...)` | sends an event to this plugin's own handlers |

Times are game time: they stand still while the game is paused. `omsi.emit` is handy between the modules of a bigger plugin. To reach another plugin, use `omsi.plugin.send` (see [Talking to other plugins and programs](#talking-to-other-plugins-and-programs)).

### Saved data

`omsi.data` is a table that survives the session. It is written when the game ends and before a reload, and read back on the next start. Numbers, strings, booleans and tables of them are kept. `omsi.save()` writes it at once.

The file is `<name>.save.lua` next to a one-file plugin, or `data.save.lua` in a folder plugin's folder.

```lua
-- plugins/odometer.lua: kilometres driven, over every session
omsi.data.km = omsi.data.km or 0
function on_frame(dt)
  omsi.data.km = omsi.data.km + math.abs(omsi.var("Velocity") or 0) * dt / 3600
end
omsi.every(60, function()
  omsi.message(string.format("Odometer: %.1f km", omsi.data.km), 3)
end)
```

`omsi.storage` is the same idea as keys and values, kept as `storage.json` in the plugin's data folder. It works in every plugin language. `omsi.files` reads and writes files in the data folder (at most 16 MB per write, 256 MB in all).

## Panels

`omsi.ui` puts the plugin's own panels on the screen, in the game's look: the Inter font, the game's icons, dark rounded cards with a shadow. They scale with the interface size setting and the window height.

A plugin describes a panel as a table and sets it again when its content changes, for example every half second. Setting the same table again changes nothing. `omsi.ui.update(panel, element, {...})` changes one element in place, which is cheaper. The game lays out and redraws a panel only when it changed.

```lua
-- plugins/trip_panel.lua: the next stop, the speed and a button
local function show()
  local i = omsi.info()
  omsi.ui.set("trip", {
    anchor = "top_left", x = 16, y = 60, width = 300, accent = "#F47F30",
    children = {
      { type = "row", children = {
        { type = "icon", name = "directions_bus", color = "#F47F30" },
        { type = "text", text = "Line " .. (i.line or "-"), weight = "bold", grow = true },
        { type = "badge", text = string.format("%.0f km/h", i.speed or 0) },
      } },
      { type = "text", text = "Next stop: " .. (i.next_stop or "-") },
      { type = "button", id = "horn", text = "Horn", icon = "campaign" },
    },
  })
end

omsi.every(0.5, show)
omsi.on("key", function(key, down)
  if key == "F10" and down then omsi.ui.focus(not omsi.ui.focused()) end
end)
omsi.on("ui_click", function(panel, element)
  if element == "horn" then omsi.trigger("horn") end
end)
```

**F10** gives the panels the mouse, and a click on the button sounds the horn. The full example is [`trip_panel.lua`](examples/plugins/trip_panel.lua).

Panels belong to their plugin. One plugin cannot change or remove another's. When the plugin stops or reloads, its panels go (its notifications run out their time). Panels show over the picture and the navigator, under the game's own text, menus and windows. They are hidden while the game menu, one of its lists or the city map is open, and in VR.

Panel texts are shown as they are. They are not translated into the game's language.

### The mouse

Panels take clicks only while they have the mouse. `omsi.ui.focus(true)` gives it to them: the cursor shows, and a click goes to the panel under it, not to the bus. `omsi.ui.focus(false)`, **Esc** or a game menu gives it back. The `ui_focus` event reports both.

A panel with `grab = true` takes a left click even while the panels do not have the mouse. Its buttons, boxes, sliders and tabs answer, and the cab behind gets nothing. Such a click does not start typing in a text field or drag the panel. It is meant for small cards: a panel that covers more than a quarter of the screen, or whose background is less than half opaque, does not grab clicks.

### The panel table

```lua
omsi.ui.set("trip", {
  anchor = "top_left",      -- see below
  x = 16, y = 16,           -- offset from the anchor towards the middle
  width = 340,              -- the height follows the content
  padding = 12,             -- default 12
  gap = 6,                  -- space between children, default 6
  background = "#14161ACC", -- default: the game's card colour
  radius = 12,              -- corner radius, default 12
  accent = "#F47F30",       -- a stripe along the left edge
  visible = true,           -- false hides it and keeps it
  clickable = false,        -- true: a click anywhere is ui_click(id, nil)
  draggable = false,        -- true: the player can move it while panels have the mouse
  grab = false,             -- true: takes clicks without the mouse (ui.version 5)
  frame = false,            -- with anchor "navigator": the whole frame (ui.version 6)
  children = { ... },       -- elements, top to bottom
})
```

| Key | Values |
| --- | --- |
| `anchor` | `top_left`, `top`, `top_right`, `left`, `center`, `right`, `bottom_left`, `bottom`, `bottom_right`, `navigator` |
| `x`, `y` | from a corner or edge: towards the middle. From a middle: right and down |
| colours | `"#RRGGBB"` or `"#RRGGBBAA"` |

Sizes are pixels of a 1080p screen at the normal interface size. A panel always stays on the screen. One the player dragged keeps its new place when it is set again; `omsi.ui.moved(id)` says how far it was moved.

### Elements

Every element can have an `id`, a `color`, and `visible = false` (left out, no room kept). Elements marked (4) need `omsi.ui.version` 4 or later.

| `type` | Draws |
| --- | --- |
| `text` | a line or paragraph, wrapped to its width |
| `icon` | one of the game's icons |
| `row` | its children side by side |
| `bar` | a progress bar, or a bar of coloured segments |
| `badge` | a small rounded label |
| `box` (4) | its children in a tinted box with a stripe on the left |
| `stops` (4) | stops on a rail, one row each |
| `divider` | a thin line |
| `map` (6) | the navigator's map, in a panel that fills the navigator |
| `space` | empty room |
| `button` | a button |
| `image` | a picture from the plugin's folder |
| `checkbox` | a box ticked by a click |
| `slider` | a track with a knob |
| `input` | a text field |
| `tabs` | a row of tabs |
| `chart` | a line or bar chart |
| `table` | columns of text with a bold header |

The keys of each element:

- **`text`**: `text`; `size` (default 14); `weight` (`regular`, `medium`, `bold`); `align` (`left`, `center`, `right`); `wrap` (default `true`; `false` keeps one line and cuts it with "…"). A line is 1.3 × `size` high.
- **`icon`**: `name`, a Material Symbols name such as `directions_bus`, `schedule`, `payments`, `warning`, `star`, `emoji_events` (every icon is in [`assets/icons/material`](../assets/icons/material)); `size` (default 20). An unknown name draws nothing.
- **`row`**: `children`; `gap` (default 8); `align` (`start`, `center`, `end`, `between`). Children are centred on each other. A child with `grow = true` takes the width the others leave; several share it. When the children do not fit, texts, labels and buttons give up width alike.
- **`bar`**: `value` (0 to 1); `height` (default 6); `color` (the filled part, default amber); `background`. Or (4) `segments`: up to 16 parts side by side, `{ { value = 3, color = "#34B26E" }, ... }`, each its share of the sum. In a row a bar is 60 wide unless it grows.
- **`badge`**: `text`; `color` (its fill, default amber); `text_color`; `weight` (4; default `medium`). It is 20 high.
- **`box`** (4): `children`; `gap` (default 4); `color` (default amber). For a note or a piece of advice.
- **`stops`** (4): `stops`, up to 128 of `{ name, time, expected, expected_color, state, tag }`; `size` (13); `color` (the rail, default amber). `state` is `"done"` (grey), `"now"` (marked, bold) or `"ahead"` (default). `time` is on the right, `expected` before it in `expected_color`, `tag` (for example `"terminus"`) before them.
- **`divider`**: `color`. In a row it is upright.
- **`map`** (6): `height` (default: the navigator map's, 0.8 × the width); `cards` (default `true`); `overlay` (8: cards of the plugin over the map). See [In the navigator](#in-the-navigator).
- **`space`**: `size` (default 8). In a row it is across.
- **`button`**: `id` (required); `text`; `icon`; `color`. 34 high and as wide as the panel; in a row as wide as its label or what it grows to. A click is `ui_click(panel, id)`.
- **`image`**: `src` (a PNG, JPEG, BMP or TGA in the plugin's folder); `width`; `height` (default 64). Shrunk to fit its width.
- **`checkbox`**: `id` (required); `text`; `checked`. A click sends `ui_change(panel, id, true/false)`.
- **`slider`**: `id` (required); `value`; `min` (0); `max` (1); `step` (0: any). It follows the mouse while the button is held and sends `ui_change(panel, id, number)`.
- **`input`**: `id` (required); `text`; `placeholder`; `max` (characters, default 100). Clicked, it takes typing, and the keys do not reach the bus. Every key sends `ui_change`; **Enter** is a `ui_click` on it; **Esc** or a click elsewhere leaves it.
- **`tabs`**: `id` (required); `tabs` (a list of texts); `selected` (from 1). Choosing one sends `ui_change(panel, id, number)`.
- **`chart`**: `values` (up to 512 numbers); `min`, `max` (default: from the values); `height` (48); `style` (`line` or `bars`); `fill` (fills below the line).
- **`table`**: `columns` (header texts); `rows` (a list of lists of texts; (4) a cell can be `{ text = "+3:12", color = "#FF5F57" }`); `widths` (shares of the width); `size` (13); `aligns` (4: one per column, `"left"`, `"right"`, ...).

Any other element with an `id` and `clickable = true` (a whole row, say) is clicked like a button.

Unknown keys are ignored and an element of an unknown `type` is left out, so a plugin written for a newer openOMSI still shows its panels. A key of the wrong kind makes `ui.set` return `false` and the reason, such as `children[2].size: a number is expected`.

**Limits per plugin:** 16 panels; 200 elements per panel (counting those in rows, boxes and map cards, and each card); 500 characters per text; 64 per id; rows and boxes 8 levels deep; 512 chart values or table cells; 8 notifications at a time.

### Panel versions

`omsi.ui.version` says which panel features the game has. Test it in a plugin that should also run on an older openOMSI: `if omsi.ui and omsi.ui.version >= 2 then`.

| Version | Added |
| --- | --- |
| 1 | panels |
| 2 | controls: buttons, checkboxes, sliders, text fields, tabs |
| 3 | the navigator's board (`anchor = "navigator"`) |
| 4 | boxes, stop lists and the other elements and keys marked (4) |
| 5 | `grab` |
| 6 | `frame` (a panel in the whole navigator) and the `map` element |
| 7 | interface themes |
| 8 | cards over the map (`overlay`) |

### In the navigator

A panel with `anchor = "navigator"` goes into the navigator's board under its map, in place of the list of next stops. It is as wide as the navigator, at its scale, and sits on the navigator's card: no card, shadow, accent or rounded corners of its own, though `background` still paints it. `x`, `y`, `width` and `draggable` are ignored. The navigator grows as tall as the panel needs, as far as the window has room.

**Shift+N** cycles the navigator through map, map and board, and off. The first time a plugin asks for the board, the navigator opens it. Only one plugin's panel is in the board at a time; another waits until it goes.

```lua
if (omsi.ui.version or 0) >= 3 then
  omsi.ui.set("board", { anchor = "navigator", padding = 10, children = { ... } })
else
  omsi.ui.set("board", { anchor = "left", x = 16, width = 340, children = { ... } })
end
```

`omsi.ui.dock()` returns the board's width, the most height there is room for, the height the panel came to, and whether the panel in the board is this plugin's. A panel taller than the room is cut off at the bottom, and the cut-off part takes no clicks. A plugin can leave out rows that do not fit.

#### The whole navigator frame

With `frame = true` the panel fills the navigator's whole frame instead: from its top, as wide as the navigator, as tall as the panel. It sits on the navigator's rounded card. A `background` is painted as that card, with its corners and the navigator's opacity setting.

The map, the place row and the stop list make way. The map shows only where the panel has a `map` element, with its own rounded corners, and with the navigator's cards over it (the next turn, the clock, the speed, the next stop) unless the element says `cards = false`.

The navigator stays the navigator: it is moved and sized as before, and **Shift+N** shows and hides it. A press on the map, or on a part of the panel that takes no click, is the navigator's: a drag moves it, a click opens the city map. A `grab` panel here takes clicks without the mouse only on its buttons and other clickable parts.

`omsi.ui.dock()` then describes the whole frame and returns `true` as a fifth value. A game that does not know `frame` lays the panel out under the map, leaves out the `map` element and returns no fifth value, so a plugin can test for that.

```lua
omsi.ui.set("phone", { anchor = "navigator", frame = true, grab = true, padding = 10, children = {
  { type = "text", text = "Line 5 → Hauptbahnhof", weight = "bold" },
  { type = "map", height = 200, cards = false },
  { type = "button", id = "next", text = "Next stop: Markt 09:14" },
} })
```

#### Cards over the map

With `overlay` the `map` element carries the plugin's own cards over the map, instead of or beside the navigator's. `overlay` is a list of small panels. Each card takes these keys:

| Key | Default |
| --- | --- |
| `place` | `top_left` (any anchor except `navigator`) |
| `x`, `y` | 8: distance from the map's edges |
| `width` | what the content needs, never more than the map less `x` on both sides |
| `padding` | 10 |
| `gap` | 6 |
| `background` | the game's card colour |
| `radius` | 10 |
| `children` | any elements |

Unknown keys are ignored. A card is held inside the map: never wider than it, never above its top. What still crosses the map's edge (the bottom of a card taller than the map) is cut: the background stops at the edge with rounded corners, and a text, icon or picture that does not fully fit is not drawn. Card buttons take clicks as in a panel; a press on the map beside them is still the navigator's.

A `map` element outside a whole-frame panel is empty room, and its cards are laid out in that room. An older openOMSI ignores `overlay` and shows the map without cards. [`navigator_map.lua`](examples/plugins/navigator_map.lua) is a complete example.

```lua
omsi.ui.set("phone", { anchor = "navigator", frame = true, padding = 10, children = {
  { type = "map", height = 360, cards = false, overlay = {
    { place = "top_left", width = 220, children = { { type = "text", text = "Line 5 → Hauptbahnhof", weight = "bold" } } },
    { place = "bottom_right", background = "#14161ACC", children = { { type = "text", text = "48 km/h" } } },
  } },
} })
```

### Notifications and messages

- `omsi.message(text, seconds)` shows a line of text on the screen.
- `omsi.ui.toast(text, { title, icon, color, seconds })` shows a notification card at the top right. A plugin shows 8 at most; a ninth pushes out its oldest.
- `omsi.game.notify(text, kind, seconds)` shows one of the game's own notification cards, `kind` being `"info"`, `"warning"` or `"alert"`.

### A line in the pause menu

A plugin can put up to 3 lines of its own in the pause menu (**Esc**), in a group before the save lines. Choosing one closes the menu and sends the plugin `menu_item`:

```lua
omsi.ui.set_menu_item("board", "Duty board", "departure_board")
omsi.on("menu_item", function(id)
  if id == "board" then omsi.ui.toggle("board") end
end)
```

The label (at most 40 characters) is shown as given, so write it in the player's language. The icon is a Material Symbols name; without one, or with an unknown one, the line gets the plugins' icon. The lines go when the plugin stops; `omsi.ui.remove_menu_item` takes one out earlier.

### An interface theme

A plugin can bring a look for the game's own screens: menu colours, frosted-glass panels over the picture, a loading screen with the plugin's logo (which can itself be the loading bar). A `theme.json` in the plugin's folder makes it a theme with the plugin's name; see [Interface themes](THEMES.md).

The player chooses it in **Options › Interface**. The plugin may switch to it with `omsi.ui.set_theme`, but should ask the player and do it only once:

```lua
if omsi.ui.set_theme and not omsi.data.theme_offered then
  omsi.data.theme_offered = true
  if omsi.ui.theme() == "" then omsi.ui.set_theme("myplugin") end
end
```

From then on the choice is the player's setting, kept for the next start. `omsi.ui.themes()` lists the themes there are.

## Settings

`omsi.plugin.settings` declares the plugin's settings and returns the values the player chose:

```lua
local settings = omsi.plugin.settings({
  { key = "send", type = "bool", label = "Send the values to a program", default = false },
  { key = "port", type = "number", label = "UDP port", default = 47800, min = 1024, max = 65535, step = 1 },
  { key = "unit", type = "choice", label = "Speed in", choices = { "km/h", "mph" }, default = "km/h" },
  { key = "name", type = "text", label = "Your name", default = "" },
}, "Telemetry HUD")

omsi.on("setting", function(key, value)
  settings[key] = value
end)
```

| Field | Meaning |
| --- | --- |
| `key` | the setting's name (required, unique) |
| `type` | `"bool"`, `"number"`, `"text"` or `"choice"` |
| `label` | the text shown; default the key |
| `default` | the value before the player changes it |
| `min`, `max`, `step` | for `number`; defaults 0, 100, 0 (any) |
| `choices` | for `choice`: a list of texts (required) |

The second argument is the title; the default is the plugin's name. A plugin can declare up to 40 settings. Values are kept in range: a number is clamped and rounded to `step`, a choice must be one of `choices`.

The player can change settings in two places:

- **In the game:** `omsi.plugin.show_settings()` opens a panel with checkboxes, sliders, text fields and tabs. It can be dragged and closed while the panels have the mouse.
- **In the launcher:** **Plugins → Installed → Settings**. The launcher reads `settings.schema.json` and writes `settings.json` in the plugin's data folder. The plugin gets the values the next time it starts.

Every change in the game sends the `setting(key, value)` event. `omsi.plugin.setting(key)` reads one value, `omsi.plugin.set_setting(key, value)` changes one.

## Talking to other plugins and programs

### Other plugins

`omsi.plugin.send(to, topic, data)` sends to one plugin by name, `omsi.plugin.broadcast(topic, data)` to all others. They receive it in their next frame as the `message(from, topic, data)` event. Data can be numbers, texts, booleans and tables of them. `omsi.plugin.list()` names the loaded plugins.

### Other players

In a LAN game, `omsi.lan.send(player, text)` reaches the same plugin on another player's game (`0`: every other player). It arrives as `lan_message(from, text)`. Texts are short (120 characters, no `|`), about ten a second at most, carried in the session's own messages. A game without the plugin ignores them.

### Programs on this computer

`omsi.send(port, text)` sends `text` as one UDP datagram to `127.0.0.1:port`: to an overlay, a dashboard or a company's tracker on the same computer. It never reaches the network. It does not wait and nothing comes back. A message sent while no program listens is lost, so keep what must not be lost in `omsi.storage` too.

`omsi.send` returns `true` when the message was handed to the system. It returns `false` and the reason when:

- the port is below 1024, or one of the game's multiplayer ports (27015 to 27024);
- the message is longer than 8 KB;
- the plugin already sent 100 messages in the last second;
- the system refused it.

```lua
-- the speed and the next stop, twice a second, for a program on port 47800
omsi.every(0.5, function()
  local d = omsi.duty.get()
  omsi.send(47800, omsi.json.encode({
    speed = omsi.speed(),
    next_stop = d and d.next_stop and d.next_stop.name,
  }))
end)
```

To see what arrives, run `nc -lu 47800` in a terminal.

### The telemetry file

A program that only wants to follow the player's bus (a fleet map, an in-vehicle terminal, a stream overlay) can read `~/.openomsi/telemetry.json` instead of being a plugin. On Windows it is `%USERPROFILE%\.openomsi\telemetry.json`.

1. Create the file once, empty.
2. The game fills it from then on, about twice a second. It replaces the whole file each time, so a reader never sees half of it.
3. Delete the file to stop the writing.

The file stays on this computer. A file whose `updated` stops changing belongs to a game that ended.

| Field | Contents |
| --- | --- |
| `version` | `2` |
| `pid`, `updated`, `paused` | the game's process, Unix seconds, paused or not |
| `player` | `null` without a bus, else `x`, `y`, `z`, `tile_x`, `tile_y`, `local_x`, `local_y`, `heading`, `speed_kmh`, `delay_s`, `passengers` |
| `duty` | `null` without a duty, else the fields below |
| `ai_buses` | the timetable buses on the road |

`duty` holds `line`, `tour`, `trip`, `trip_index`, `trip_count`, `terminus`, `next_stop_index`, `next_stop_id`, `next_stop_name`, `next_stop_dist`, `prev_stop_id`, `at_stop` and `stops`. Each stop has `object_id`, `name`, `stops`, `arr`, `dep`, `x`, `y`.

Each entry of `ai_buses` has `id`, `line`, `tour`, `trip`, `terminus`, `depart`, `next_stop_id`, `at_stop`, `trip_done`, `delay_s`, `x`, `y`, `number`. Stop ids are the map's object ids, as in `omsi.info()`.

## Passengers' tickets

What a passenger asks for at the cash desk is normally the ticket pack's choice. A plugin can change it, for example to charge by zones, by fare units, or to accept a pass.

Everybody boarding the player's bus is reported once, on the way to the door, as the `passenger_ticket` event. It says who they are (`id`, `age`), what they do about a ticket (`kind`: `"none"`, `"stamp"` or `"buy"`), the pack's ticket and price, the stop they board at and the stop they ride to, and where both are in the duty's trip. The passenger waits in the doorway until the plugins have had that frame (normally the next one, at most 5 s of game time). In that time a plugin can hand over its own fare with `omsi.people.set_ticket`:

```lua
omsi.on("passenger_ticket", function(p)
  if p.kind ~= "buy" or not p.destination_number then return end
  local stops = p.destination_number - p.stop_number
  omsi.people.set_ticket(p.id, {
    name = stops <= 3 and "Short hop" or "Single",
    price = stops <= 3 and 1.5 or 3,
    voice = "Zone_" .. stops,   -- Zone_<n>.wav of the passenger's voice folder
    tag = "my_tariff",          -- kept with the passenger: omsi.people.get(id).tag
  })
end)
```

The rules:

- `kind` can change only until the passenger is in the bus. The ticket, name, price, payment and voice can change until they have asked at the desk (`omsi.people.get(id).ticket_asked`).
- `ticket` picks another ticket of the pack, with its name, price and `GivenTicket` number. `name` and `price` alone keep the pack's number, so the bus's ticket printer and the driver's ticket key keep working.
- A buyer always has a ticket of the pack that the driver can give. `ticket = 0` is refused for them. Somebody made a buyer without `ticket` gets the pack's ticket for their age; when the pack has none for their age, the change is refused.
- `payment` is the money handed over. It does not change the price and is at least the price. The amount may be higher when the currency's coins and notes cannot make it exactly.
- A voice line the passenger's voice folder does not have falls back to the usual `Ticket_<n>_<k>`.
- `omsi.people.get(id)` shows the passenger's ticket now and which plugin changed it (`ticket_by`).

Without a plugin listening, passengers ask for what the ticket pack gives them and nobody waits.

[`reduced_tickets.lua`](examples/plugins/reduced_tickets.lua) has every adult ask for the reduced fare of the stock Berlin ticket packs. [`passenger_log.lua`](examples/plugins/passenger_log.lua) writes what a plugin learns of each passenger into `game.log`.

## Reading timetable and depot data

### Game data getters

`omsi.game.query(name, args)` reads the same data as the vehicle scripts' getters (`GetTerminusString`, `GetTTBusstopArr` and so on), without running a script. `omsi.game.queries()` lists the names, argument names and types, and result types; the full list is in the [reference](PLUGIN_API.md#omsigamequery). Getter arguments keep OMSI's zero-based indices.

Reads use the player's bus and its current HOF and timetable. Without a bus they return `nil` and a reason. As a third argument, pass the map id of a loaded scripted scenery object to read that object instead; this lets `GetArrBus*` read a departure board's arrivals without a player vehicle. Find object ids with `omsi.map.objects_near(x, y)`.

### The whole tour

`omsi.duty.tour()` returns the complete source tour of the current duty, even when only one trip was chosen. `omsi.timetable.tour(line, tour)` reads any tour without taking it. Times are seconds from midnight of the service day, can go past 24:00, and passing stops are included.

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

### The loaded HOF file

`omsi.hof.get()` returns a snapshot of every field of the player's loaded depot (HOF) file: its name and path, the service-trip identifier, the global strings and string counts, all termini and bus stops with their identifiers and strings, and the info-system trips, stop lists and stop rows. Empty strings and the original order are kept. `info_trips[].code` stays a string, leading zeroes included. Comments and unknown directives are not part of it.

`omsi.hof.terminus(identifier)` and `omsi.hof.busstop(identifier)` look up one entry without copying the whole depot. Both match the identifier exactly, case-sensitively, and return the first match. For a terminus, the identifier is the HOF's internal one, not a display text or IBIS code.

`index` fields are zero-based, as `GetTerminusString` and `GetBusstopString` expect, while Lua lists start at 1. So getter field 8 is `strings[9]`:

```lua
local tour = omsi.duty.tour()
if tour then
  for _, trip in ipairs(tour.trips) do
    local t = omsi.hof.terminus(trip.terminus)
    if t then
      print(trip.name, t.index, t.code, t.strings[9] or "", t.strings[10] or "")
    end
  end
end
```

`GetTTTerminusIndex` reads the current timetable state and takes no trip, so the code above is how to resolve every trip of a tour without changing the duty.

Snapshots are copies: changing them changes nothing in the game. Keep a snapshot for repeated reads instead of copying the depot every frame, and read a new one when the vehicle or its HOF changes. The API cannot open other HOF files or change the depot, needs no permission, and works the same in Lua and WebAssembly.

## Scripts in a vehicle

A vehicle can carry its own scripts, written in Lua or compiled to an `.oop`, so a mod needs no plugin installed by the player. Name them in the vehicle's `.bus` or `.ovh` file, relative to the vehicle's folder:

```text
[vehicle_script]
scripts/cab.lua

[vehicle_script]
scripts/ibis.oop
```

The scripts start when the vehicle becomes the player's and stop when the player changes to another vehicle or none. They see the same `omsi.*` API as any plugin, and `omsi.var` reads their own bus. In the log they are named `vehicle:<file>`.

A vehicle is content from anywhere, so its scripts get only these permissions: `ui`, `storage`, `vehicle_write` and `audio`. The world, the traffic, the camera, the LAN, UDP and native programs are only for the player's own plugins. An `.oop` that declares more keeps only these four.

OMSI 2 ignores the keyword and runs the vehicle's `.osc` scripts as always. A vehicle can have both.

Scenery objects can also have Lua scripts, named with `[lua_script]` in the `.sco` file. See [Scripts of map objects](OBJECT_SCRIPTS.md).

## Permissions and sandbox

### Permissions

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
| `lan` | sending in a LAN session |
| `process` | starting native helper programs |

- A plain `.lua` plugin has every permission.
- A compiled `.oop` has only the permissions it declares. A function that needs another one raises an error the plugin can catch. An unknown permission name grants nothing.
- Vehicle scripts have only `ui`, `storage`, `vehicle_write` and `audio`.
- Reading the game needs no permission.
- `process` also always needs the player's approval for each helper.

`omsi.plugin.permissions()` lists what the plugin has. The [API reference](PLUGIN_API.md) names the permission of each function.

### The Lua sandbox

A Lua plugin gets Lua 5.4 with the safe libraries only: `string`, `table`, `math`, `utf8`, `coroutine`, `require` for its own folder, and `os.clock`, `os.time`, `os.date`, `os.difftime`.

- No `io`, no `os.execute` (not through `require("os")` either), no C modules, no `dofile` or `loadfile`.
- No binary chunks anywhere. Plugin files, modules and `load` read text only, because Lua does not check compiled code. `string.dump` gives no code.
- A plugin writes files only in its data folder, and reads only that and its own folder. `..` and absolute paths are refused.
- It cannot reach the network. `omsi.send` talks only to programs on this computer, on ports from 1024 up.
- It holds at most 256 MB of memory.

### Errors and limits

- An error in a handler is written to `game.log` and shown on the screen. The other plugins and the game carry on.
- After 10 errors the plugin is switched off until you change its file or restart the game. `omsi.plugin.errors()` counts them.
- A call into the plugin that runs longer than 50 ms (an endless loop, say) is stopped and the plugin switched off, with a line in `game.log`. Loading may take up to a second.
- A file that does not compile is left out, with the Lua error in `game.log`.
- `omsi.plugin.disable(reason)` switches a plugin off by its own choice.

### Native helper programs

`omsi.process.start("bin/helper.exe", args, options)` starts a program from the plugin's own folder. This is the only way out of the sandbox, so the player decides every time.

**The approval window.** The first time a plugin tries to start a helper, the game asks. The window shows the plugin's name, the program's file name and the number of arguments, with a safety warning. **Details** shows the full program path, the plugin's entry file, every argument and the working directory; **Hide details** folds them again. Opening the details never approves anything.

| Button | Effect |
| --- | --- |
| **Allow once** | this one start |
| **Always allow** | saved, bound to this plugin's code and this helper |
| **Deny** | no helpers for this plugin until the game restarts |

**Esc** also denies; **Enter** never approves. The game and other plugins keep running while the window waits. Reloading or unloading the plugin cancels the request. The window is translated into every interface language; plugin names, paths and arguments are shown literally.

**What "Always allow" binds to.** The approval is saved in `~/.openomsi/plugin-process-permissions.json`. It covers the plugin's entry file, its folder's executable content (Lua modules, native `.dll`, `.so`, `.dylib` libraries and, for a folder plugin, the other package files) and the helper, each by SHA-256. Changing any of these, or moving the plugin, asks again. Saved data (`*.save.lua`, non-executable files in `data/` or `*.data/`) is not included, so `omsi.save()`, `omsi.storage` and `omsi.files` do not invalidate it. For a one-file plugin in the top-level `plugins` folder, sibling Lua files are included because `require` can load them; use a folder plugin to keep them apart. To revoke an approval, close the game and remove it from that file.

**Deny** blocks every helper of that plugin for the rest of the session, even if the helper's name, content or arguments change.

**Rules for helpers:**

- `file` must be a relative path inside the plugin's folder. On Windows it must be an `.exe`, not a script.
- `args` are separate literal strings; there is no shell.
- `options` may set `cwd` and `env` (overrides; other variables are inherited). `OMSI_PLUGIN_PROCESS_ALLOW` is always removed from the helper's environment.
- Standard input and output are disconnected.
- A compiled plugin must also declare the `process` permission.
- Helpers run with the player's account rights. The hash check is not a sandbox and not a lock: approved code can choose its arguments, working directory and environment.

**Handles and waiting.** `process.start` returns a handle at once; the check and the start happen on background workers. At most 32 starts are queued in the whole game, and each plugin has at most 32 handles (queued and failed ones count). A request that cannot be queued returns `nil` and a reason. One that fails later shows it in `process.status(h).error`, or `nil, "failed", reason` from `exit_code` and `wait`. `pid` is `nil` until the program runs. Requests are limited to 256 arguments, 128 environment overrides and 64 KiB of text.

`process.status` and `process.exit_code` never block. In Lua, `process.wait(h)` suspends the plugin's current top-level callback (the main chunk, an event handler or a timer) until the helper exits, while the game and other plugins go on. Callbacks that arrive meanwhile wait (up to 256). A nested `emit` handler or a hand-made coroutine cannot wait. Call `process.forget(h)` when you are done with a finished handle.

**Cleanup.** Stopping, disabling or reloading the plugin cancels pending waits and ends its helpers.

| System | Cleanup |
| --- | --- |
| Windows | a job object ends the helper and its children, even if the game crashes; `forget` also ends leftover children |
| Linux | the direct helper gets a signal when the game exits; its children do not |
| macOS | only normal plugin shutdown |

On macOS, a helper unpacked from a ZIP may need its executable bit (`chmod +x /path/to/helper`) and may be blocked by Gatekeeper quarantine. Check the helper and use macOS's normal approval; the game does not remove the quarantine.

**Pre-approval (advanced).** The environment variable `OMSI_PLUGIN_PROCESS_ALLOW` can name an absolute path to a JSON manifest of pre-approved plugins and helpers, for example on a managed machine. Generate it from the deployed plugin with the repository's tool, and keep it outside the plugin folder:

```bash
cargo run -p omsi-plugin --bin omsi-plugin-grant -- /plugins/example/plugin.lua /plugins/example bin/helper.exe > /plugins/process-grants.json
```

For an `.oop`, the entry is the archive and the folder is its extracted asset folder (or the archive's folder when it has no assets). Several entries go in the manifest's `grants` list. The game still re-checks every hash before each start. Grants from an older full-folder snapshot may need approval once more.

## Compiled plugins

An `.oop` is to a plugin what a DLL is to a program, but sandboxed. It holds build output, not sources, and one file runs on every system.

| Kind | Source | In the `.oop` |
| --- | --- | --- |
| Lua | `.lua` files | one obfuscated chunk: locals renamed, strings encoded, modules bundled |
| WebAssembly | usually Rust, built for `wasm32-unknown-unknown` | a stripped module |

A compiled Lua plugin behaves as its sources did, through the same API. Compiled Lua *bytecode* is refused, because crafted bytecode could break out of the sandbox.

A WebAssembly module calls every API function by its name (`openomsi.call("ui.set", ...)`, arguments and results as JSON) and gets the same events, timers and watches. It runs in an interpreter (wasmi) on every system the game runs on. A call into it that runs too long stops the plugin, not the game, and it may use 256 MB of memory. A callback the module's own call causes (its `emit` reaching its own handler) runs right after that call returns.

### Build one

Get the [openOMSI Development Tools](https://github.com/openOMSI-org/openOMSI-Development-Tools/releases): a desktop app with an editor, and `oopc`, the same compiler on the command line.

In the app:

1. Press **New plugin**, give it a name, choose Lua or Rust, and press **Create plugin**.
2. Press **Run in openOMSI**. The first time, the app asks where openOMSI is. It copies the plugin into the game's `plugins` folder and starts the game. With **Live** on, every save reaches the running game.
3. On the **Details** page, tick the permissions the plugin needs. **Match my code** ticks exactly what the code uses.
4. On **Build and publish**, choose whether to sign, and press **Build**. The result is one `.oop` file.

From a terminal:

```bash
oopc new hello --lua     # or --rust
oopc build hello         # writes the .oop into hello/dist/
oopc check hello         # lint: ids, versions, Lua syntax, permissions
```

| Command | What it does |
| --- | --- |
| `oopc new <dir> --lua` / `--rust` | start a project from a template |
| `oopc build [dir]` | compile and pack into `dist/`; `--sign <key>` signs, `-o <file>` names the output |
| `oopc check [dir]` | lint the project |
| `oopc keygen` | make a signing key |
| `oopc sign <file.oop> --key <key>` | sign or re-sign a file |
| `oopc verify <file.oop> [--key <pub>]` | check the signature |
| `oopc inspect <file.oop>` | show the header and the file list |
| `oopc pack <dir>` | pack already prepared files |

A Rust plugin also needs the Rust toolchain and the `wasm32-unknown-unknown` target; the app's **Settings** installs the target.

### A minimal Rust plugin

`Cargo.toml`:

```toml
[package]
name = "hello"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
openomsi-plugin-sdk = { git = "https://github.com/openOMSI-org/openOMSI-Development-Tools" }

[workspace]

[profile.release]
opt-level = "z"
lto = true
strip = true
panic = "abort"
```

`src/lib.rs`:

```rust
use openomsi_plugin_sdk as omsi;

omsi::plugin!(|| {
    // When the player gets into a bus, greet them by its name.
    omsi::on_vehicle(|name| {
        if let Some(name) = name {
            let _ = omsi::api::message(&format!("Good morning! Today you drive the {name}"), Some(6.0));
        }
    });

    // Once a second, warn if the bus is going too fast.
    omsi::every(1.0, || {
        if let Ok(Some(kmh)) = omsi::api::var("Velocity")
            && kmh > 50.0
        {
            let _ = omsi::api::message(&format!("Slow down: {kmh:.0} km/h"), Some(1.0));
        }
    });
});
```

The SDK's `plugin!` defines the exports the game calls. `omsi::api` has a typed function for every API call; `omsi::call(name, json!([...]))` reaches any function by name, even one newer than the SDK.

### The plugin manifest

A compiled plugin's project has an `openomsi-plugin.toml`. Its `[plugin]` table becomes the `.oop`'s header.

```toml
[plugin]
id = "com.author.hello"
name = "Hello"
version = "1.0.0"
kind = "lua"              # or "rust"
entry = "main.lua"        # Lua only
authors = ["you"]
description = "Greets the driver and warns above 50 km/h."
min_openomsi = "0.2.21"
permissions = ["ui"]
license = "MIT"
homepage = "https://example.org/hello"

[lua]
obfuscation_level = 2
```

| Field | Required | Meaning |
| --- | --- | --- |
| `id` | yes | reverse-domain id, `com.author.name` |
| `name` | yes | display name |
| `version` | yes | `MAJOR.MINOR.PATCH` |
| `kind` | yes | `lua` or `rust` |
| `entry` | no | Lua entry file, default `main.lua` |
| `authors` | no | list of names |
| `description` | no | a sentence or two |
| `min_openomsi` | no | the oldest openOMSI it supports |
| `permissions` | no | what it may do (see [Permissions](#permissions)) |
| `license` | no | SPDX licence id |
| `homepage` | no | a web address |

| `[lua]` option | Meaning |
| --- | --- |
| `obfuscation_level` | 0 strips formatting only, 1 renames, 2 (default) also encodes strings and numbers |

| `[rust]` option | Meaning |
| --- | --- |
| `features` | extra Cargo features for the build |

A Lua plugin in the [catalog](#distributing-a-plugin) uses a different file, `plugin.json`. A theme uses `theme.json` ([Interface themes](THEMES.md)).

### What the game does with an `.oop`

- It checks the whole file before any of it runs. A file changed after it was built, cut short, larger than 512 MB, or made for a newer plugin API is refused, and the log says why.
- The code stays in memory. The plugin's pictures, sounds and data files are unpacked to `plugins/.oop-cache/<name>/`, where its API functions read them.
- It applies the declared permissions.
- A signed plugin is logged with its author's key fingerprint (`signed by 3097:e2de:e2cb:4a34`). An unsigned one is logged with a warning that its builder is unknown.

The file is encrypted, but the key is in the game, so it can be decrypted with effort. What one gets then is the stripped or obfuscated build output, never the author's sources.

### Signing

1. In the app, open **Signing keys** and press **New key**, once. Keep the secret key private and back it up. The public half (`.oopkey.pub`) is safe to share.
2. Turn on **Sign this build** when you build. From a terminal: `oopc build --sign <your.oopkey>`.
3. Publish your key's fingerprint somewhere players trust. They can then confirm in the log that a plugin is yours and unchanged.

## OMSI 2 DLL plugins

openOMSI loads the original game's plugins: every `*.opl` under the `plugins` folder of each content root, searched recursively. The first content root's copy of an `.opl` wins.

### The `.opl` file

| Tag | Contents |
| --- | --- |
| `[dll]` | the DLL, relative to `plugins` |
| `[varlist]` | a count, then that many vehicle variable names |
| `[stringvarlist]` | a count, then string variable names |
| `[systemvarlist]` | a count, then system variable names |
| `[triggers]` | a count, then trigger names |
| `[scripttexturelist]` | openOMSI only: a count, then script texture numbers |

### The DLL interface

The game loads the DLL and looks up its procedures, all `stdcall`:

| Procedure | Required |
| --- | --- |
| `PluginStart(AOwner)` | yes |
| `PluginFinalize` | yes, runs when the game ends |
| `AccessVariable`, `AccessStringVariable`, `AccessSystemVariable`, `AccessTrigger` | no |
| `AccessScriptTexture`, `AccessScriptTexturePointer`, `AccessTicketRequest` | no, openOMSI only |

A missing required procedure gives "Could not load plugin …: procedure … not found!". A missing optional one gives "Loading plugin …: procedure … not found!".

Every frame, plugin by plugin:

1. Each listed system variable: `AccessSystemVariable(index: Word; var value: Single; var write: Boolean)`.
2. With a player vehicle, each listed vehicle variable: `AccessVariable`, same shape. Written back when `write` is true.
3. Each string variable: `AccessStringVariable(index: Word; text: PWideChar; var write: Boolean)`. The buffer holds length + 1 wide characters, the text and its terminating zero. Read back when `write` is true.
4. Each trigger: `AccessTrigger(index: Word; var active: Boolean)`, with `active` false before the call. A change from the last frame is a key event: down fires the trigger, up fires `<trigger>_off`.

`index` is the position in the plugin's own list. Names the vehicle does not have are skipped.

### Differences in openOMSI

- A DLL the game can load directly (same system and architecture, for example a 64-bit DLL in the 64-bit game) runs in-process.
- Otherwise it runs in `omsi-plugin-host32.exe`, a 32-bit Windows helper that loads the DLL and talks to the game once per frame. On Windows it runs directly; on macOS and Linux through Wine (`wine` on the `PATH`, or the `OMSI_WINE` variable). The game looks for the helper next to itself; `OMSI_PLUGIN_HOST32` overrides the path.
- A plugin's writes to system variables are not applied: the clock, weather and input stay the game's.
- `PluginStart` gets a nil owner, since there is no Delphi application object. Plugins that open windows do so without a parent.
- A plugin whose helper stops answering is left out for the rest of the session.
- The launcher's **Disable plugins** setting, or `OMSI_NO_PLUGINS=1`, leaves out every plugin.

### Game values as variables

Many OMSI 2 plugins read the bus's position, its type or the map from fixed memory addresses of `Omsi.exe`, which cannot work in openOMSI. Instead, list **`openomsi_<key>`** in `[varlist]` or `[stringvarlist]`, where `<key>` is any key of [`omsi.info()`](PLUGIN_API.md#omsiinfo): `openomsi_tile_x`, `openomsi_tile_pos_x`, `openomsi_heading`, `openomsi_map_path`, and so on. OMSI 2 has no variables by these names and skips them, so one `.opl` serves both games.

- Values are available while the player drives a bus. Numbers and booleans come as variables, texts as string variables.
- Names are matched without regard to case. A real vehicle script variable of the same name takes precedence.
- The values are read-only.
- Booleans are `0` or `1`. A text requested as a number (or the reverse), an unknown key, or a number that is not a finite 32-bit float is unavailable.
- `destination` is the selected HOF terminus's texture identifier (empty for an all-exit terminus). `passengers` is zero when no passenger simulation runs. `version` is the game's displayed version.

### Script textures

OMSI 2 gives plugins no way into the vehicle's `[scripttexture]`s; tools such as OmsiHook reach them in `Omsi.exe`'s memory. openOMSI adds a tag and a procedure for it. OMSI 2 skips both.

```text
[scripttexturelist]
2
0
3
```

This lists the player vehicle's script textures the plugin draws, by number (0-based, as `[useScriptTexture]` and `\S:n` count them). Every frame, after the triggers, the game calls this for each entry the vehicle has:

```pascal
procedure AccessScriptTexture(index: Word; width, height: Integer; pixels: Pointer;
  var write: Boolean); stdcall;
```

- `index` is the entry's position in the list, not the texture's number.
- `pixels` is `width` × `height` × 4 bytes: RGBA, top row first, rows `width * 4` bytes apart, straight (not premultiplied) alpha. The size is the texture's own (its `[scripttexture]` entry, or what `STLoadTex` made it).
- The buffer belongs to the entry and keeps what the plugin drew last, so it can redraw only what changed. It starts zeroed and is zeroed again when the texture's size changes. It is valid only during the call.
- Set `write` to show the buffer from the next picture on. Leave it false when nothing changed: a written picture is copied, and through the 32-bit helper it travels over a pipe (a 512 × 256 texture is 512 KiB).
- If a script also draws into the texture (`STLock` … `STUnlock`), the last to finish is shown. Give the plugin textures the scripts leave alone.
- Without a player vehicle, or for a number the vehicle has no texture for, the entry is skipped.

A 64-bit DLL loads in-process in the 64-bit game, without the pipe. Textures go over the pipe only for a library that exports `AccessScriptTexture`. An older `omsi-plugin-host32.exe` without this still serves every other plugin.

#### The mouse on script textures

```pascal
procedure AccessScriptTexturePointer(index: Word; kind: Integer; x, y: Single); stdcall;
```

A library that exports this hears the mouse on its `[scripttexturelist]` textures. The game finds where a click lands on the bus's meshes as it does for `[htmltexture]` pages (the nearest mesh whose material shows the texture with `[useScriptTexture]`). It calls the procedure in the next frame, before `AccessScriptTexture`, so the picture drawn then can already respond.

- `kind`: 0 the button went down on the texture; 1 the cursor moved while held (at most once a frame, the latest); 2 it came up (off the texture: where it last was on it).
- `x`, `y`: the place in texture pixels from the top left, fractional.
- `index`: the entry's position in the list.

A press on such a texture belongs to the plugin: a `[mouseevent]` mesh under it does not fire. Events are not kept while the game is paused.

### Passengers' tickets in DLL plugins

```pascal
procedure AccessTicketRequest(person: Cardinal; age: Single;
  stop, destination: PWideChar; stopNumber, destinationNumber: Integer;
  var kind, ticket: Integer; var price: Single; name, voice: PWideChar;
  var write: Boolean); stdcall;
```

A library that exports this hears every passenger boarding the player's bus once, in the frame after they decided about their ticket (the Lua `passenger_ticket` event). It may change what they ask for; they wait in the doorway for that frame.

- `person`: the passenger's id (as `omsi.people` knows it); `age` in years.
- `stop`, `destination`: the timetable names of the boarding stop and the stop they ride to (`""` when unknown). `stopNumber`, `destinationNumber`: where those are in the duty's trip, from 1 (0: not in it).
- `kind`: 0 nothing to do (a pass), 1 validates a ticket, 2 buys one. `ticket`: the pack's ticket from 1 (0: none fits their age, and the desk asks for the pack's first, as in OMSI). `price`: the price asked. A buyer keeps a ticket of the pack: 0 is not taken for them.
- `name`: a buffer of 256 wide characters (the terminating zero included) with the ticket's name. `voice`: one as long, empty; a line of the passenger's voice folder said at the desk instead of `Ticket_<n>_<k>` (`Zone_4001` for `Zone_4001.wav`).
- Set `write` to apply the changes. Only what differs is applied, the kind only while the passenger is not yet in the bus. The DLL's name is kept as who changed it.

An older `omsi-plugin-host32.exe` without this procedure does not pass it on, and the plugin then does not hear of passengers.

### Building the 32-bit helper

```bash
scripts/build-plugin-host.sh
```

It needs `rustup target add i686-pc-windows-gnu` and MinGW (`brew install mingw-w64` or `apt install mingw-w64`). Copy `dist/omsi-plugin-host32.exe` next to the game. See [Building from source](BUILDING.md) for building openOMSI itself.

## Distributing a plugin

You can share a plugin in two ways:

- **Any file:** send the `.lua`, the plugin folder, or the `.oop`. Players drop it into their `plugins` folder.
- **The plugin catalog:** the launcher's **Plugins → Browse** tab lists the plugins of [openOMSI-org/openOMSI-Plugins](https://github.com/openOMSI-org/openOMSI-Plugins). Players install and update them with one click.

### Adding a plugin to the catalog

The catalog takes open-source folder plugins (a `main.lua` and its files). A maintainer reads the code before it is merged.

1. Fork the repository and make a folder `plugins/<id>/`. The id is 2 to 40 characters of `a-z`, `0-9` and `-`, not starting with `-`. It becomes the folder name in the game: `plugins/<id>/main.lua`.
2. Put the plugin in it: `main.lua` and whatever it `require`s or reads. Allowed file types: `.lua`, `.json`, `.txt`, `.md`, `.csv`, `.png`, `.jpg`, `.ogg`, `.wav`, `.ttf`, `.otf`. At most 2 MB per file, 10 MB in all, 200 files. No programs, libraries or archives.
3. Add `plugins/<id>/plugin.json` (see below).
4. Run `python3 tools/build_catalog.py` and commit `catalog.json` with the plugin. Check the Lua with `luac -p plugins/<id>/*.lua` (Lua 5.4).
5. Open a pull request. The checks must pass.

An update is a pull request that changes the files and raises `version`. The launcher offers it to everyone who installed the plugin. Their settings and saved data (`plugins/<id>/data/`) stay.

### `plugin.json`

```json
{
  "id": "my-plugin",
  "name": "My plugin",
  "author": "Your name",
  "version": "1.0.0",
  "description": "One line: what it does for the player.",
  "license": "MIT",
  "tags": ["driving"],
  "homepage": "https://github.com/you/my-plugin",
  "min_game": "0.3.55"
}
```

| Field | Required | Meaning |
| --- | --- | --- |
| `id` | yes | the folder name, as above |
| `name` | yes | display name, at most 60 characters |
| `author` | yes | at most 60 characters |
| `version` | yes | `MAJOR.MINOR.PATCH` |
| `description` | yes | one line, at most 300 characters |
| `license` | yes | SPDX id of an open-source licence, at most 40 characters |
| `tags` | no | a list of words |
| `homepage` | no | a web address |
| `min_game` | no | the oldest openOMSI it needs |

### What the catalog does not take

- Code that is hidden (obfuscated, minified, encoded) or downloads code to run.
- Anything that sends the player's data somewhere without saying so in its description.
- Content of OMSI 2 or of other people's mods (textures, sounds, scripts) without their permission.

Plugins that only work with paid add-ons are fine; say so in the description.

### How the launcher installs from the catalog

- It installs a plugin as `plugins/<id>/` only when every file's size and SHA-256 match `catalog.json`.
- It refuses to overwrite a `plugins/<id>/` folder that did not come from the catalog.
- It offers an update when `version` goes up, and keeps `plugins/<id>/data/` on an update.
- It reads `catalog.json` each time **Browse** opens: a plugin added to the catalog shows up at once, without a new openOMSI version.
- **Remove** deletes the plugin's files, settings and saved data.
- A plugin taken out of the catalog stays in the game of those who installed it (with its saved data); **Browse** marks it *No longer in the catalog* and offers **Remove**.
- An entry that breaks a rule is left out of the list. A catalog newer than the launcher asks you to update openOMSI.

## Debugging

- **Watch `game.log`** (in `~/.openomsi/`) while you work. Every `omsi.log` and `print` line and every error is there, tagged `[lua <name>]`. `omsi.warn`, `omsi.error` and `omsi.debug` write at other levels; debug lines show with `RUST_LOG=debug`.
- **Save to reload.** The game reloads a changed plugin within a second and says "Lua plugin <name> reloaded". The Development Tools' **Live** switch does the same for a project.
- **Check the status.** `~/.openomsi/plugins-status.json` and the launcher's **Plugins** page show whether each plugin loaded, failed (with the error) or was switched off.
- **Find variable names.** The bus's `.osc` scripts list them, and so does `omsi.vars()`. The environment variable `OMSI_WATCH_VARS=Velocity,throttle` logs every change of those variables (see [Environment variables](DEBUG_FLAGS.md)).
- **Catch expected errors** with `pcall`. A wrong argument is an error that names the function and the argument.
- **Test for features** with `omsi.game.has("weather.set")` and `omsi.ui.version` when a plugin should run on older versions too.
- **Rule out plugins.** Turn on **Disable plugins** in the launcher (or set `OMSI_NO_PLUGINS=1`) to check whether a problem comes from a plugin.

Keep plugins light:

- Keep `on_frame` short. Use `omsi.every`, `omsi.watch` and events for everything that does not need every frame.
- Read only what you need, such as `omsi.bus.velocity()` or `omsi.info_value("delay")`, instead of building whole tables every frame.
- Use `omsi.ui.update` to change one element; setting the whole panel every frame costs more.

## Example plugins

Complete plugins to start from. Each one runs in the game's tests against a stand-in game.

| Plugin | What it shows |
| --- | --- |
| [`trip_panel.lua`](examples/plugins/trip_panel.lua) | a panel with the line, the next stop and a button |
| [`navigator_map.lua`](examples/plugins/navigator_map.lua) | the navigator's frame with its map and three cards over it: line, next stop, speed |
| [`dispatcher/`](examples/plugins/dispatcher/main.lua) | a dispatcher and small career: draggable panel with tabs, next stops in a table, a delay chart, pay kept in storage, duties offered and taken with a click, settings, hotkeys |
| [`telemetry_hud.lua`](examples/plugins/telemetry_hud.lua) | a HUD with speed chart, gear, rpm, fuel, passengers, speed limit and the light ahead; a red-light warning; the values as JSON over UDP |
| [`passenger_log.lua`](examples/plugins/passenger_log.lua) | every boarding passenger in `game.log`: age, stops, the ticket they will ask for |
| [`reduced_tickets.lua`](examples/plugins/reduced_tickets.lua) | every adult asks for the reduced fare |
| [`weather_controller.lua`](examples/plugins/weather_controller.lua) | weather files as tabs, sliders for temperature, visibility and rain, snow cover, clock buttons, a day cycle |
