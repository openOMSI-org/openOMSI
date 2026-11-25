# Driving

Everything you do in the game window: the keys, the mouse and the gamepad, starting a bus,
serving the stops, the game menu, the navigator, photo mode and driving a trolleybus. For the
launcher and its settings see [Launcher and settings](LAUNCHER.md).

## Driving keys

openOMSI keeps OMSI 2's keys (`Inputs/keyboard.cfg`) and adds a few of its own. On top of them a
**driving keys preset** puts the throttle, brake and steering where most players reach for them.
Choose it in **Settings → Driving → Driving keys** (or `--drive-keys` on the command line).

| Preset | Throttle / brake | Steering | Notes |
| --- | --- | --- | --- |
| **W A S D + arrows** (default) | `W` / `S`, `Up` / `Down` | `A` / `D` | `Left` / `Right` keep switching the interior camera |
| **W A S D only** | `W` / `S` | `A` / `D` | the arrows keep OMSI's jobs |
| **Arrow keys only** | `Up` / `Down` | `Left` / `Right` | `W`, `S` and `D` keep their OMSI jobs |
| **Custom controls** | as on the Controls page | as on the Controls page | nothing added |

In OMSI 2, `W` is the wipers, `S` the viewpoint and `D` the D of the automatic gearbox. With a
W A S D preset, **hold Shift** to reach a key's OMSI meaning: `Shift+W` wipers, `Shift+S`
viewpoint, `Shift+D` drive.

The ready-made presets also add these keys, unless you bound the key to something else:

| Key | What it does |
| --- | --- |
| `Z` / `X` / `C` | left indicator / hazard lights / right indicator (press again to turn off) |
| `Shift+1`, `Shift+2`, … | open or close door 1, 2, … counted front to back |
| `I` | every saloon light circuit at once |
| `F1`–`F4` | the views (where `keyboard.cfg` has no view keys) |

To change any key, open the launcher's **Controls** page (see
[Launcher and settings](LAUNCHER.md#controls)). Changing a key there switches the preset to
**Custom controls**.

## Keys

The tables list the default keys: OMSI 2's own `keyboard.cfg` plus openOMSI's additions. "Num"
means the numeric keypad. A bus can use only some of them, and a mod bus may add its own.

### Driving

| Key | What it does |
| --- | --- |
| `Num 8` | throttle (held) |
| `Num 2` | brake (held) |
| `Num 4` / `Num 6` | steer left / right (held) |
| `Num 5` | steering back to the middle |
| `Num +` | kickdown (throttle amplify) |
| `Tab` | clutch |
| `.` | parking brake on / off |
| `E` | battery main switch (on stock buses also the ignition) |
| `M` | starter (hold about a second) |
| `Shift+U` | start the bus by itself, or shut it down (see [Starting a bus](#starting-a-bus)) |
| `H` | horn |

### Gearbox

| Key | What it does |
| --- | --- |
| `R` / `N` / `D` | automatic: reverse / neutral / drive (`Shift+D` with a W A S D preset) |
| `1` / `2` | automatic: hold in 1st / 2nd |
| `1`–`6` | manual gearbox: gears 1 to 6 |
| `R` / `N` | manual gearbox: reverse / neutral |
| `Ctrl+Up` / `Ctrl+Down` | manual gearbox: gear up / down |
| `[` / `/` | sequential gearbox: up / down (OMSI's `kw_s_plus` / `kw_s_minus`) |

### Lights and indicators

| Key | What it does |
| --- | --- |
| `L` | headlights |
| `Shift+L` | parking lights |
| `F` | high beam |
| `Ctrl+F` | rear fog light |
| `Alt+F` | flashlight: a lamp at your eye that shines where you look |
| `Num 7` / `Num 9` | left / right indicator |
| `Num .` | indicators off |
| `B` | hazard lights |
| `6` | driver's light |
| `7` / `8` / `9` | saloon light circuits |
| `W` / `Shift+W` / `Ctrl+W` | wipers / wiper interval / washer |

Controls also offers **Indicator left (toggle)** and **Indicator right (toggle)**, unbound by
default: one press turns that side on, the next turns it off, the other side switches over. They
suit wheel buttons such as shift paddles. A script's own cancelling still works.

### Doors and passengers

| Key | What it does |
| --- | --- |
| `Shift+1` … `Shift+9` | open or close that door, front to back |
| `Num /` | front door (first leaf) |
| `Num *` | front door (second leaf) |
| `Num -` | rear doors |
| `Num +` | another front door on some buses |
| `F12` | pram / wheelchair button |
| `Scroll Lock` | the bus's 20h switch |
| `Q` | microphone (held) |

How many `Shift+number` doors a bus has depends on its scripts. The SD200, SD202 and EN92 have
`Shift+1` to `Shift+3`; a low-floor mod with independent doors can have more. The control
reminder at the start of a drive says how many.

### Tickets and change

| Key | What it does |
| --- | --- |
| `T` | sell the ticket the passenger asks for (on a bus without a ticket printer) |
| `Ctrl+T` / `Shift+T` | hand out the change / take it back |
| `Ctrl+5` … `Ctrl+0` | change tray: 5.00, 2.00, 1.00, 0.50, 0.10, 0.05 |
| `Ctrl+Shift+5` … `Ctrl+Shift+0` | change tray: 15.00, 8.00, 4.00, 1.50, 0.30, 0.15 |
| `Home` | ticket desk camera |
| `Insert` | timetable view |

### Destination sign and IBIS

| Key | What it does |
| --- | --- |
| `F5` / `F8` | line number down / up (roller blind 1 / tour) |
| `F6` / `F7` | destination down / up (roller blind 2 / 3) |
| `Page Up` / `Page Down` | roller blind up / down (held: runs, tapped: one step) |
| `Ctrl+Num 0`–`Ctrl+Num 9` | IBIS digits |
| `Ctrl+Num Enter` | IBIS enter |
| `Ctrl+Num .` | IBIS clear |
| `Ctrl+Num *` / `Ctrl+Num /` / `Ctrl+Num -` | IBIS mode: line and tour / route / destination |
| `Q` | IBIS next stop with its announcement (only when `Q` is free in your keyboard file) |

`Shift+U` types the duty's line, route and destination into the IBIS for you.

### Cameras and looking around

| Key | What it does |
| --- | --- |
| `F1` | driver's view |
| `F2` | passenger view |
| `F3` | outside view |
| `F4` | map view: a free camera that starts where you are looking |
| `F11` | on foot, from where the camera is |
| `S` | next viewpoint (`Shift+S` with a W A S D preset) |
| `Left` / `Right` | previous / next interior camera (also `Ctrl+Left` / `Ctrl+Right`) |
| `C` | look ahead again in this view (with a ready-made preset `C` is the right indicator) |
| `Space` | look ahead again in every view |
| `Home` (held) | recentre the view, where `keyboard.cfg` gives `Home` no job |
| `Alt+I` / `Alt+J` / `Alt+K` / `Alt+L` | look up / left / down / right |
| `=` / `-` | zoom in / out |
| `Num +` / `Num -` | zoom outside, when nothing is bound to them |
| `Ctrl+Alt+arrows` | aim the mirror you are looking at (kept per bus) |

The free camera (`F4`) flies with `W` `A` `S` `D`, `Q` down, `E` or `Space` up, `Shift` faster.
**Ctrl+click** on the ground there moves the bus to the nearest street.

Unbound by default, for you to give a key on the Controls page: **Zoom while held**
(`view_zoom_hold`), **cabin / outside view** in one key (`view_toggle_interior`) and
**recentre head tracking** (`head_tracking_recenter`).

### Game and interface

| Key | What it does |
| --- | --- |
| `Esc` | game menu (pauses, except in multiplayer) |
| `Alt` (tap) | quick menu |
| `P` | pause |
| `Ctrl+Q` | end the game |
| `Ctrl+S` or `Alt+S` | quick save |
| `F9` | write the run into the driver's personnel file |
| `Ctrl+Shift+P` or `F12` | screenshot (`F12` only where the bus has no key of its own on it) |
| `Ctrl+F12` | photo mode |
| `Shift+Y` | information bar along the top |
| `Alt+U` | hide the whole interface (clean screen) |
| `Shift+M` | city map |
| `Shift+N` | navigator → navigator with the schedule → off |
| `Shift+R` | next radio station (see [Radio](RADIO.md)) |
| `Ctrl+M` / `Ctrl+Shift+M` | mirror panels on / off, their editor (see [Graphics](GRAPHICS.md#mirror-panels)) |
| `Ctrl+Shift+H` | skip the next stop of the duty |
| `Ctrl+Shift+Page Up` / `Page Down` | clock forwards / backwards (held: faster the longer) |
| `O` | mouse steering on / off |
| `K` | game controllers on / off |
| `Alt+Enter` | full screen on / off |
| `Ctrl+Shift+E` | object editor (see [Mods](MODS.md#object-editor)) |
| `Ctrl+Shift+D` | developer tools (see [Command line](COMMAND_LINE.md#developer-tools)) |
| `Ctrl+F11` | show the camera's position and tile |

In a tutorial, `Enter` or `Page Down` turns its page, `Page Up` goes back and `Ctrl+T` hides it.

### Multiplayer

| Key | What it does |
| --- | --- |
| `/` | open the chat line (`Enter` sends, `Esc` drops it) |
| `V` | show or hide the chat; hold to talk to nearby players (voice chat) |
| `B` | hold to talk to the whole session (voice chat; only when `B` is free, OMSI's file puts the hazard lights there) |
| `Ctrl+Alt+S` | on foot: ask for a passenger door (see [Service at the stops](#service-at-the-stops)) |

When your keyboard file gives `/` to something else (OMSI's own file gives it to a manual
gearbox), the chat opens with the key left of `1`, or `\`. See [Playing together](MULTIPLAYER.md)
and [Voice chat](VOICE.md).

### On foot

Switch on **Settings → Gameplay → Ability to get up** first.

| Key | What it does |
| --- | --- |
| `Ctrl+Shift+G` | get up from the driver's seat and out of the front door |
| `W` `A` `S` `D` | walk, relative to where you look |
| `Shift` | run |
| `Space` | jump |
| `C` | kneel / stand up |
| right mouse button | look round |
| `F1` / `F4` | your eyes / free camera (the walker waits) |
| `G` | by a door: take a passenger seat; again: get out; at your bus's front door: sit at the wheel |

### Rail vehicles and trolleybuses

| Key | What it does |
| --- | --- |
| `J` | set the points ahead to their next branch (rail vehicles) |
| `Shift+P` | pantograph up / down |
| `R` (held, outside) | take the nearest trolley pole's rope and aim it at the wire |
| `T` (outside) | pull the nearest pole down |
| `Shift+R` / `Shift+T` (outside) | both poles |
| `Ctrl+R` (in the cab) | poles with a motor up / down |

VR keys (recentre, desktop mirror, navigator) are in [Virtual reality](VR.md).

## Gamepad

A gamepad works without any setting up: the left stick steers, the right trigger is the
throttle, the left trigger the brake and the right stick looks round. With **Default gamepad
buttons** on (Settings → Driving, the default) the buttons do this:

| Button (Xbox / PlayStation) | What it does |
| --- | --- |
| A / Cross | open or close all doors |
| B / Circle | cabin / outside view |
| X / Square | next view |
| Y / Triangle | reset the view direction |
| LB / L1 | gear down (manual gearbox) |
| RB / R1 | gear up (manual gearbox) |
| D-pad left / right | left / right indicator |
| D-pad up / down | next / previous interior camera |
| Menu / Options | open or close the game menu |
| View / Share | pause |
| Right stick click | reset all view directions |

Change the buttons on **Controls → Game controllers**. Wheels, pedals and button boxes are set up
there too (see [Launcher and settings](LAUNCHER.md#game-controllers)).

## Mouse

### The cockpit

- **Left-click** a cockpit switch to work it. The name of the switch under the cursor shows in
  the HUD.
- **Hold the left button and move** to turn a knob, or roll the mouse wheel over it.
- **Right-drag** (or drag with the wheel pressed) looks round in any view: the head turns inside,
  the camera swings round the bus outside. **Shift+right-drag** zooms.
- The **mouse wheel** zooms: outside the camera comes closer, inside the view narrows.
  **Ctrl+wheel** outside narrows the view instead (a telephoto).

With **Settings → Camera → Right mouse button turns the view** off, the right button zooms as in
OMSI and only the wheel button turns the view. Each view keeps its own direction.

### Mouse steering

**O** switches mouse steering on and off, as in OMSI.

- The cursor's place across the window is the steering, from full left to full right lock.
- From the middle up to the top edge is the throttle, down to the bottom edge the brake.
- Above 10 km/h the same hand movement turns the wheels less (a fifth as far at 50 km/h), so the
  wheel feels heavier at speed.
- For the first second the wheel and pedals ease towards the cursor.
- While the mouse steers, the cursor is held and a cross shows where it steers. The full lock is
  reached past the window's edges.

The settings for it are in **Settings → Driving** (see
[Launcher and settings](LAUNCHER.md#driving-settings)): sensitivity, pedal strength, smoothing, whether
the mouse works the pedals, and whether the cursor is held. With *Hold the cursor* off, the
system's crosshair is shown and the window's edges are the lock, as in OMSI (Windows and Linux;
macOS does not let a game keep the cursor in its window). A graphics tablet's pen works this way.

### The steering keys

Three switches in **Settings → Driving** change how the steering keys behave (all off by
default):

- **Steering linearity**: the wheel turns at OMSI's own steady pace, the same for every bus.
- **Old Steering**: OMSI's wheel that stays where you leave it. Turn it back yourself.
- **Dynamic steering**: the keys turn the wheel more slowly at speed (OMSI's `redSteerSpd`).

`Num 5` brings the wheel back to the middle in a straight line. The clutch key works as in OMSI:
the pedal goes down at once and comes up slowly (0.7 per second) when you let go.

## Starting a bus

On a stock SD200:

1. **E**: battery main switch (it puts the ignition key in as well).
2. **M**, held for about a second: the starter.
3. **Shift+D** (or **D** with Custom controls or the arrows preset): drive.
4. **.**: parking brake off.
5. Throttle.

**Shift+U** does the start-up by itself: main switch, ignition, starter, gearbox to neutral. It
also types the current duty into the IBIS, as a driver putting the bus into service would. On a
running bus it shuts the bus down. The launcher's **Bus ready to go** starts the drive with this
done.

When the bus does not move, the notes in the top-left corner say why: the parking brake, low air
pressure, or a line that the date's timetable takes off.

## Service at the stops

### Passengers boarding

How passengers board is set in **Settings → Gameplay → Boarding**:

| Choice | What happens |
| --- | --- |
| **Pay and take the ticket** (default) | they walk to the ticket place, put the money down and take their ticket after a moment |
| **The driver sells the ticket** | they wait for you to sell it (the bus's printer, or **T**) and give up after 25 s |
| **Just walk in** | straight into the saloon: flat fare or ticket machines |

Who gets on your bus:

- **On a duty** with a destination on the display: the people waiting for a stop the trip calls
  at later, and those whose line record lists the terminus shown. Everybody gets off at the
  trip's last stop.
- **In free drive**: whom the destination display takes, as for a timetable bus.
- **With no destination set**, or "not in service": nobody gets on. Riders already aboard still
  get off at their stops.

With **Passengers pay the exact fare** off, they overpay and you hand out change. With
**Passengers react to bad driving** off, nobody complains or gets off early because of rough
driving.

### Skipping a stop

For a stop the bus cannot reach, or one where it never registers: **Ctrl+Shift+H** or the game
menu's **Skip the next stop**. The IBIS moves on with it.

### Asking for a door on foot

On foot, press **Ctrl+Alt+S** within 3.2 m of an entry point outside a bus, or of an exit point
inside the bus you ride in. The request stays until the door opens, or until you leave the
point, change buses or step through a door. AI buses and other players' buses answer it; an exit
request also makes an AI bus serve its next stop.

### The journey log

On a duty the game keeps a **journey log** in the content folder's `Journeys` folder: one text
file a duty, named by the real date and time it began and the line and tour. For each trip it
lists the stops with their planned and actual times, and whether the bus was late (over 3
minutes), left early (over 2 minutes) or missed a stop. It is written again at every stop, so a
crash loses nothing.

## The game menu

**Esc** opens the game menu: its lines on the left, the game picture beside it with a card of
what you drive, and **Resume**. The mouse works it, and so do `Up` / `Down`, `Enter`, and
`Left` / `Right` to step a value.

| Line | What it holds |
| --- | --- |
| **Resume** | back to the drive |
| **Options…** | the settings, as in the launcher |
| **Controls…** | keys and game controllers |
| **Camera…** | the seat, the field of view, the view and mirror editor |
| **Vehicle options…** | destination display, depot file, fleet number, driver; other vehicles; service |
| **World options…** | clock and date, weather, temperature and wind, traffic, passengers, object editor |
| **City map** | the whole map from above |
| **Find a street…** | a street by its name |
| **Line and tour…** | change or choose the duty |
| **Skip the next stop** | see [Skipping a stop](#skipping-a-stop) |
| **End the tour** | end the duty, keep driving |
| **Save the situation** | the quick save |
| **Save to a slot…** | a new slot, one under a name you type, or one to save over |
| **Load a situation…** | the quick save, or any save of the map |
| **Screenshot** | a picture into `Screenshots` |
| **Photo mode** | see [Photo mode](#photo-mode) |
| **End the session** | back to the launcher |

**Vehicle options** also lets you drive another placed vehicle, place any vehicle of the
installation, couple and uncouple, get out, swap or reload the vehicle. Its service part refuels
and washes (only at a petrol station or in the depot's wash yard, as in OMSI), repairs, puts the
bus back on its wheels and moves it on the map.

On a server the menu is shorter: the clock and the weather are the server's, and **Leave the
server** replaces *End the session*.

## Navigator and city map

The **navigator** sits in a corner of the screen (bottom left by default). It is a small tilted
3D map that turns with the bus and zooms out with speed. It shows:

- the roads, and the trip's route with arrows, coloured by traffic (blue empty, green light,
  yellow busy, red heavy, dark red jammed);
- the stops ahead, the other vehicles as dots, and other players as purple arrows with names;
- the next turn with its distance and street, and the street you are on;
- above the map: speed and limit, line, time; below it: the next stop, its distance, the time to
  it, its planned time, and whether you are early or late.

It routes on the whole map's road network, so the way shows however far the next stop is. From
the depot it leads to the next stop. Leave the route and it is recalculated after two seconds.
A duty begins with the first trip of the tour whose first stop the bus can still reach in time.

**Shift+N** steps through navigator → navigator with the schedule of the next stops → off.

A click on the navigator, or **Shift+M**, opens the **city map**: the whole map from above with
street names, the route with its stops and times, the bus and the traffic. Drag to move, the
wheel zooms, the buttons centre on the bus. `Esc` or a click outside closes it.

The navigator's options (corner, size, widget, 2D, 3D buildings, arrows, AI vehicles) are in
**Settings → General → Navigator** (see [Launcher and settings](LAUNCHER.md#general-settings)). A map can
bring water, green land and rail for the navigator, see [Map packages](MAP_PACKAGE.md).

## HUD and notes

The HUD shows the time, speed, line, next stop and delay, what the workshop just did, and the
controls for the first seconds of a drive. The **notes** in the top-left corner say why the bus
does not move, the bus's own messages, the end of a trip, and tickets and change. Each kind can
be switched off in **Settings → General**. **Shift+Y** shows OMSI's information bar along the
top.

## Photo mode

**Photo mode** (game menu, or **Ctrl+F12**) stops the game and gives you a camera of its own.

### Photo mode keys

| Key | What it does |
| --- | --- |
| `W` `A` `S` `D` | fly (level over the ground) |
| `E` or `Space` / `Q` or `C` | up / down |
| right mouse button, or left over the picture | look round (the arrows too) |
| wheel, `+` / `-` | zoom the lens |
| `Z` / `X` | roll |
| `R` | level the camera again |
| `Shift` / `Alt` | faster / slower |
| `Tab` | next page of the panel (`Shift+Tab` back) |
| `H` | hide the panel |
| `Enter` or `F12` | take the photo |
| `Esc` | leave photo mode |

On a gamepad: the left stick moves, the right one looks, the triggers go down and up, the
shoulder buttons roll, the d-pad zooms, A takes the photo, Y hides the panel, X levels, B leaves.

The camera stays within a set distance of the bus (25 m to unlimited) and above the ground.
**Orbit** turns it round the bus instead.

### The panel

| Page | What it sets |
| --- | --- |
| **Camera** | free or orbit, focal length (10–400 mm full frame), roll, speed, distance allowed, frame (window, 16:9, 21:9, 3:2, 4:3, 1:1, 4:5, 9:16), guides |
| **Lens** | depth of field (f/1.2–f/22), focus on the bus or by hand, bokeh shape, motion blur with shutter speed, samples per photo |
| **Colour** | exposure, contrast, highlights, shadows, saturation, vibrance, temperature, tint |
| **Effects** | film look and its strength, grain, vignette, colour fringes, sharpening |
| **Scene** | the clock an hour or a quarter on or back, the photo's size (window or double) |

Depth of field and motion blur are real: the photo is the average of many pictures, so it
sharpens over a second or two once the camera stands still. Motion blur needs the bus moving
when the game was stopped. What you see is the photo as it is saved: a PNG in the `Screenshots`
folder.

**Settings → General → Photo mode → Advanced (for photographers)** gives the panel a camera body
on six pages (*Body*, *Exposure*, *Lens*, *Colour*, *Develop*, *Scene*): sensor size, P/A/S/M
exposure with ISO and metering, white balance in kelvin, tilt-shift, histogram, focus peaking,
bracketing and 16-bit PNG.

## Driving a trolleybus

A map with an overhead wire and a bus with poles make a trolleybus that moves only on current.

1. Get out of the bus (**Ctrl+Shift+G**, with *Ability to get up* on) or use the outside camera
   (**F3**).
2. Go to the back of the bus.
3. Hold **R** and look at the wire. The nearest pole rises and follows your look.
4. Let go of **R** when the boom is under the wire. It springs up to the wire you aimed at.

**T** pulls the nearest pole down. With **Shift**, **R** and **T** take both poles. Poles with a
motor go up and down from the cab with **Ctrl+R**.

The formats and the checks are in [Trolleybuses](TROLLEYBUS.md). Trams and trains:
[Trams, metros and trains](RAIL.md).

## Wheels and force feedback

On a force-feedback wheel you feel the bus all the time, not only when it hits something:

- **The road**: waves a few metres long, read from the distance driven, so the same stretch
  feels the same each time. A wet or snowy road hums louder than dry asphalt.
- **The engine**: a buzz that grows with the revs and the load, and stays at a standstill.
- **Jolts**: kerbs, bumps and the scripts' shakes ease away over the fade time.
- **Steering force**: parking resistance that eases as the bus rolls, centring, and the bus's
  sideways acceleration.

Each is set in **Settings → Driving → Force feedback**, and per wheel on **Controls → Game
controllers** (see [Launcher and settings](LAUNCHER.md#game-controllers)).
