# User guide

How to run openOMSI, drive, use the launcher, install mods and play over LAN. For building
from source see [BUILDING.md](BUILDING.md).
For OpenXR headset setup and controls on Windows, see [VR.md](VR.md).

> openOMSI runs on the content of an **original OMSI 2 installation**. Without one the game does not start.

## Checking an installation

```bash
cargo run --release -p omsi-check -- "/path/to/OMSI 2"
```

This loads every content file in the install with openOMSI's loaders and reports what failed.

## Running

```bash
openomsi --root "/path/to/OMSI 2"
```

`--root` is only needed once: the path is remembered, so afterwards the program can be
started with no arguments at all. Without it the installation is looked for in `$OMSI_ROOT`,
next to the program (an `OMSI 2 Original` or `OMSI 2` folder beside openOMSI) and in the
usual Steam locations.

Started without arguments the program opens the launcher (see below). `--menu` shows the
in-game start menu instead, which asks for map, vehicle, time, traffic, passengers, the
timetable, the weather and the date (arrow keys change values, Enter starts, Esc quits).
Everything can also be given on the command line, which then skips both:

| Flag | Meaning |
| --- | --- |
| `--map maps/Grundorf/global.cfg` | map to load |
| `--weather Weather/Schmuddelwetter.owt --date 1989-01-15` | weather, and the date that decides the season |
| `--bus Vehicles/MAN_SD200/MAN_SD80.bus` | player vehicle (`--paint name`, `--hof name`, `--plate "B-AB 1234"`) |
| `--entry N` / `--spawn x,y,heading` | where the vehicle starts |
| `--time HH:MM --date YYYY-MM-DD --weather Weather/x.owt` | time, date, weather |
| `--traffic N --schedule --line 76 --tour 1 --passengers` | AI cars, timetable buses, the player's tour, people at the stops |
| `--radius N` / `--all` | tiles around the start / the whole map |
| `--view-distance M` | how far around the camera the window keeps tiles loaded (m, 1200 by default) |
| `--content-zip mod.zip` | read an archive in place as a content root (repeatable; `OMSI_CONTENT_ZIP` does the same) |
| `--offscreen out.png --cam x,y,z,yaw,pitch --drive secs` | render one frame to a file |
| `--snapshots 7,9,11` / `--follow auto\|bus\|moving\|type:X` | more pictures during a `--drive` run, camera behind an AI vehicle |
| `--riders N --refuel --wash --repair --dirt 0..1` | passengers already aboard, the depot services, how dirty the bus starts |
| `--driver Drivers/OMSI-Fan.odr` | the driver's personnel file; the run is added to it |
| `--drive-keys simple\|wasd\|arrows\|omsi` | the control preset (`drive_keys` below; `simple` by default) |
| `--autostart` | put the bus into service before the run (as Shift+U does) |
| `--click x,y[,dx,dy]` | press (and drag) the cockpit switch at that pixel, offscreen |
| `--season winter` (`autumn-late`, `spring-early`, …) / `--situation x.osn` / `--physics simple` | season override, a saved situation, the kinematic dynamics instead of the rigid body |
| `--enhanced` / `--export-glb bus.glb` | the physically based renderer; write the bus as glTF (the launcher's preview) and quit |
| `--enhanced-plus` | Enhanced+: the physically based renderer with ray-traced shadows, ambient occlusion and reflections |
| `--menu` / `--no-menu` | the in-game start menu instead of the launcher (which a start without arguments opens), or neither |

**Passenger animations.** **Settings → Gameplay → Passengers → Passenger animations** chooses
how the people move: **Original OMSI** (the default) or **Natural (procedural)**, with
procedurally placed feet and limbs (IK). `OMSI_PAX_ANIMATION=enhanced` or `=original`
overrides the setting for one session.

**Character models.** **Settings → Gameplay → Passengers → Character models** chooses how
the people look. **Original** (the default) draws the people of OMSI's content. **High
quality (improved)** draws them with the [Microsoft Rocketbox avatars](https://github.com/microsoft/Microsoft-Rocketbox)
(91 of them: everyday people, office workers, workers, nurses, pilots, police officers,
children): each person an avatar of the same kind, with a living face - they blink, look
about, smile when they pay, now and then raise their brows, frown in thought or yawn late
at night, and their lips move with what they say. This is drawn on your computer only: the
passengers themselves (where they go, sit and pay, their tickets) are not changed, and in
multiplayer the others see their own people. The avatars are downloaded once, the first time
the setting is on (about 310 MB, into `~/.openomsi/avatars`); until they are there the people
stay the content's own. `OMSI_CHARACTER_MODELS=improved` or `=original` overrides the setting
for one session, `OMSI_AVATARS_DIR` names another folder of avatars.

Keys in the window: **W** throttle, **S** brake, **A**/**D** steering, and **Up**/**Down** throttle
and brake as well (**Left**/**Right** keep OMSI's job of stepping through the interior
cameras). Every vehicle key of `Inputs/keyboard.cfg` works as it does in OMSI: throttle
Shift+Num 8, brake Shift+Num 2, steering Shift+Num 4/6, **E** battery and ignition, **M**
starter, **N**/**R** the automatic, **.** the parking brake. W, S and D are OMSI's wiper,
viewpoint and **D of the automatic gearbox**, so hold shift for those: **Shift+D** selects D.
Settings → Driving → *Driving keys* (`drive_keys`, `--drive-keys`) changes this: *W A S D +
arrows* (the default, as described here), *W A S D only*, *Arrow keys only* (all four arrows drive and W/A/S/D are left to OMSI) or *Custom
controls* (only the layout of the Controls page).

To start a stock SD200: **E** (battery - it puts the ignition key in as well), **M** held for a
second (starter), **Shift+D** (drive), **.** (parking brake off), then throttle. **Shift+U** does the
whole start-up by itself (main switch, ignition, starter, gearbox to neutral); `--autostart`
is the same thing for an offscreen run.

**Updates.** When the launcher starts it asks
[github.com/openOMSI-org/openOMSI](https://github.com/openOMSI-org/openOMSI) for the latest release
and, when there is a newer one, offers it in a dialog: **Update now** downloads it (checked
against the SHA-256 GitHub lists), puts the new program in place of the old one and starts the
launcher again; mods, content and settings stay. On Android the system's installer asks
whether to update the app; Update replaces openOMSI and starts it again, Cancel leaves it as
it was. No update is installed while a game runs. Settings → General → *Updates*: *Look for
updates when the launcher starts* (on by default), *Update channel* (Stable, the tested
releases, by default; Beta, every beta as soon as it is published, for testers), *Install
updates without asking* (off by default), and the version line with **Check now**. A folder
openOMSI cannot write to (Program Files, an app opened straight from Downloads on macOS) is
reported with what to do. A download that breaks goes on where it stopped (up to four times),
a request that fails is tried again after 3 and 10 s, and when the GitHub API does not answer
(or says its hourly limit is reached) the latest version is taken from github.com itself.
A new release can always be downloaded by hand from the
[latest release](https://github.com/openOMSI-org/openOMSI/releases/latest) page as well.

While it is open the launcher looks again every 30 minutes, and shortly after a game it
started ends. During a session the game looks a minute in and every hour after: a newer
version is downloaded in the background and a card over the navigator says so ("openOMSI
X is out"); when the session ends the launcher installs the file already downloaded (by
itself with "Install updates without asking", else it offers it), and never while a game
runs. "Tell me about a new version during a session" switches the cards off (the download
goes on). `OMSI_NO_UPDATE=1` switches the checks off; `OMSI_UPDATE_URL` points them at another
release description (GitHub's format; `file://` works, for testing - the game then looks after
3 s).

**When a version does not start.** openOMSI heals a start that breaks on its own, before
anything that could break has run: when it is started (a double click, the launcher) it first
starts a small watcher without a window. If the launcher crashes, ends on an error or has not
drawn its first frames after two minutes (it is then ended), the watcher steps in - when that
version never started on this computer, or failed twice in a row:

1. a newer release, when there is one, is downloaded and installed (its program must start
   before it replaces anything) and openOMSI starts again;
2. else the version the last update replaced - kept beside it until the new version has
   started well once - is put back, and openOMSI starts again;
3. else openOMSI starts once more in **safe mode** (the basic render pipelines, no plugins);
4. else a message says what to do (download the latest release, where the log is).

Each step is told in a message box. A version that failed is never installed again on this
computer, only a newer one. The project can also list a released version as broken
(`release/health.json`); such a version goes back to the one before, or on to a fixed one, at
the next start even when it starts well. A crash in the middle of a drive is not a broken
start: nothing is changed then. The guard keeps its own record in `~/.openomsi/guard/`
(`guard.log`, `starts.json`, `bad-versions`). `OMSI_NO_GUARD=1` switches it off. On Android
the next start after one that never reached the launcher looks for an update at once (the
system's installer asks); going back is the system's business there.

**Playing now.** While a session runs the game tells the project's counter (a Cloudflare
Worker, `services/presence/`) every ten minutes that it is being played, and says goodbye
when it ends; the website and the README show how many play right now. What goes out is a
random id made new for each session, the version and the kind of system - nothing else, and
the counter keeps no addresses. Settings → General → "Count me in the website's \"playing
now\"" (on by default) or `OMSI_NO_PRESENCE=1` switch it off; a dedicated server is never
counted.

**O** switches mouse steering on and off, as in OMSI (Omsi.exe's own formula): the cursor's
place across the whole window is the steering from full left to full right lock
(`[inv_min_turnradius]` of the bus), from the middle up to the top edge is the throttle and
down to the bottom edge the brake (with *The mouse works the pedals* off, in Settings →
Driving, the mouse only steers and the pedals stay on the keys). Above 10 km/h the same hand movement turns the wheels less
and less (at 50 km/h a fifth as far), so the wheel feels heavier the faster the bus goes; for
the first second after switching it on the wheel and the pedals ease towards the cursor.
Settings → Driving → *Mouse steering sensitivity* makes it more or less sensitive (100 % = OMSI).
*Mouse pedal strength* sets how much cursor travel is needed to reach full accelerator or
braking: above 100 % the pedal reaches full sooner and below 100 % it takes more travel.
Mouse steering works in the driver's, the passenger and the outside view; the wheel follows
the cursor smoothly (a short easing, no steps). With *Smooth mouse steering* off (Settings →
Driving) the wheel and the pedals are where the cursor says at once, as in OMSI. While the mouse
steers the cursor is held, so the wheel reaches its full lock past the window's edges and a
cross shows where it steers; with *Hold the cursor while the mouse steers* off the system's
crosshair stays free (it follows the hand without the frame's delay, and a graphics tablet's
pen works with it) and the window's edges are the lock, as in OMSI.

Three switches there change the steering keys (all off by default): *Steering linearity* turns
the wheel at OMSI's own steady pace (the curvature grows by the same amount every millisecond
the key is held, whatever the bus), *Old Steering* is OMSI's wheel that stays where you
leave it - turn it back yourself - and *Dynamic steering* makes the keys turn the wheel more
slowly at speed (OMSI's `redSteerSpd`). **Num 5** (`steering_neutral`) brings the wheel back to the
middle in a straight line, as in OMSI at the pace the keys turn it (never slower than the
wheel comes back by itself). The clutch key works as in OMSI: the pedal goes down at once
and comes up slowly (0.7 per second) when the key is released.

On a force-feedback wheel the bus is felt all the time, not only when it hits something: the
steady road under the tyres and the engine's buzz come up as a tremble that is strongest at
speed and on a wet or snowy road, and the engine's is there even at a standstill, as a real
one is. Settings → Driving turns each of them off or up to four times as strong, and sets how
long a jolt or a scripted shake takes to ease away (off = it stops at once).

Left-click a cockpit switch to operate it, hold the button and move the mouse to turn a knob,
or roll the mouse wheel over it (that is the `<event>_drag` OMSI fires); the name of the switch
under the cursor is shown in the HUD.
Right-drag the mouse (or drag with the wheel pressed, OMSI's pan) to look around in any view
(the head turns inside, the camera swings around the bus outside), I/J/K/L does the same from
the keyboard, and Shift+right-drag zooms - this is OMSI's `[altView]` mode, the Camera setting
"Right mouse button turns the view". Switched off, the right button zooms as in OMSI's default
(up: the outside camera backs away, the view inside widens up to the seat's own) and only the
wheel button turns the view; each view keeps its own
direction (turning the outside camera leaves the driver's head where it was), **Space** looks
ahead again in every view (OMSI's `view_reset_all_directions`), Home recentres the view shown
where keyboard.cfg does not make it the ticket desk camera.
The mouse wheel (and **=** / **-**, a pinch on a phone) zooms: outside the camera comes closer,
inside the bus the view narrows, as in OMSI; **Ctrl**+wheel outside narrows the view instead
(a telephoto, the camera stays where it is). F1-F4 driver / passenger / outside / map (free) camera, F5-F8 the destination
sign and roller blind keys as in OMSI, Ctrl+S quick save (OMSI's `quicksave`; Alt+S works too), F9 write the
run into the personnel file, WASD+QE in the free camera (**Ctrl**+click on the ground there
moves the bus to the nearest street), left click on cockpit elements, the chat key (**/** by default, see Controls) opens the chat line in a
LAN session.

**Esc** opens the game menu, drawn in the launcher's style: its lines on a rail on the left,
the picture of the game beside it with a card of what is being driven and **Resume**. Its
lines: *Resume*, *Options* (the settings, as in the launcher), *Controls*, *Camera* (the seat,
the field of view, the view and mirror editor), *Vehicle options*, *World options*, *City
map*, *Line and tour*, *Skip the next stop* (also **Ctrl+Shift+H**: for a stop the bus cannot
reach or never registers at), *End the tour*, *Save the situation*, *Save to a slot*, *Load
the quicksave*, *Screenshot*, *Photo mode* (below) and *End the session*. *Vehicle options*
has the destination display, the depot file, the fleet number and the driver; driving
another placed vehicle, placing any vehicle of the installation, coupling and uncoupling,
getting out, swapping or reloading the vehicle; and the service - refuel and wash (only at a
petrol station or in the depot's wash yard, as in OMSI), repair, putting the bus back on its
wheels and moving it on the map. *World options* has the clock and the date, the weather
(and a custom weather), temperature and wind, the traffic and the passengers, and the object
editor (below). On a server the menu is shorter: the clock and the weather are the server's.
The mouse works the menu, and so do **Up**/**Down** and **Enter**, with **Left** and **Right**
stepping a value; the settings are kept for the next game. Home is the ticket desk camera and
Insert the timetable view (as OMSI's keyboard.cfg binds them), and the change keys of
keyboard.cfg hand out or take back the change. The HUD
shows time, speed, line, next stop, delay and what the workshop just did (and why the bus
stands: the parking brake, low air pressure, a line the date's chrono takes off), and the
controls for the first seconds.

### Photo mode

*Photo mode* in the game menu (or **Ctrl+F12**, the `photo_mode` key) stops the game and hands
over a camera of its own, as the racing games' photo modes do. **WASD** fly it (level over the
ground whichever way it looks), **Q**/**E** (or Space) go down and up, the right mouse button
held - or the left one over the picture - looks round (the arrows too), the wheel and **+**/**-**
zoom the lens, **Z**/**X** roll it and **R** levels it again, **Shift** is faster and **Alt**
slower, **Tab** steps through the panel's pages, **H** hides the panel, **Enter** (or F12) takes
the photo and **Esc** leaves. A gamepad: the left stick moves, the right one looks, the triggers
go down and up, the shoulder buttons roll, the d-pad zooms, A takes the photo, Y hides the
panel, X levels, B leaves. The camera stays within a distance of the bus (25 m to unlimited)
and above the ground; *Orbit* turns it round the bus instead.

The panel has five pages. *Camera*: free or orbit, the focal length (10-400 mm, a full-frame
camera's), the roll, the speed, the distance allowed, the frame (the window's or 16:9, 21:9,
3:2, 4:3, 1:1, 4:5, 9:16 - the photo is cut to it) and the guides (thirds, golden ratio,
centre). *Lens*: depth of field with the aperture (f/1.2-f/22), the focus on the bus or at a
distance set by hand and the bokeh's shape (round, 5, 6 or 8 blades); motion blur for a
rolling shot with the shutter speed (the bus stays sharp, the street passes - it needs the
bus moving when the game was stopped); how many samples a photo is made of. *Colour*:
exposure, contrast, highlights, shadows, saturation, vibrance, temperature, tint. *Effects*:
a film look (vivid, warm, cool, vintage, sepia, black and white, noir, bleach bypass, teal and
orange, faded film) and its strength, film grain, vignette, colour fringes, sharpening.
*Scene*: the clock an hour or a quarter on or back, and the photo's size (the window's or
double). Depth of field and motion blur are real: the photo is the average of many pictures,
each taken through another point of the aperture and at another moment of the shutter, so
what is shown sharpens over a second or two once the camera stands still (the panel shows
how far). What is shown is the photo as it is saved - a PNG in the `Screenshots` folder.

Settings → General → *Photo mode* → *Advanced (for photographers)* gives the panel a camera
body instead, on six pages (*Body*, *Exposure*, *Lens*, *Colour*, *Develop*, *Scene*): sensor
size, P/A/S/M exposure with ISO and metering, white balance in kelvin, tilt-shift, a
histogram, focus peaking, bracketing and 16-bit PNG.

On a duty the game keeps a **journey log** in the content folder's `Journeys` (one text file
a duty, named by the real date and time it began and the line and tour): each trip driven
with its stops, their planned and actual arrival and departure and how far off those were,
and whether the bus came late (over 3 minutes), left early (over 2 minutes, as the personnel
file counts them) or missed a stop. It is written again at every stop, so a crash loses
nothing - what virtual bus companies ask their drivers for.

### Trolleybuses

A map with an overhead wire (`trolley.txt`) and a bus with poles (`[trolley_pole]`) make a trolleybus
that moves only on current. Put the poles on the wire from outside the bus: get out (Ctrl+Shift+G, the
`get_up` setting) or use the outside camera (F3), go to the back of the bus, hold **R** and look at the
wire (the pole nearest to you rises and follows your look), let go of R when the boom is under the wire.
**T** pulls the nearest pole down; Shift with R or T takes both poles. Poles with a motor go up and down
from the cab with R and T. Details, the formats and the checks: [TROLLEYBUS.md](TROLLEYBUS.md).

## The launcher

The launcher is a program of its own beside the game, `openomsi-launcher-app` (a Tauri window,
`launcher/`, see its README): `openomsi` started without arguments opens it, and where it is
missing - or its window cannot open, for want of WebView2 on an old Windows (the game offers
Microsoft's installer) or of WebKitGTK 4.1 on Linux - the game says so and where it belongs
(on Linux in a dialog of the desktop as well as in the terminal).

**Linux needs WebKitGTK 4.1** for the launcher's window: `libwebkit2gtk-4.1-0` on Debian and
Ubuntu, `webkit2gtk4.1` on Fedora, `webkit2gtk-4.1` on Arch. **SteamOS** (Steam Deck) does not
include it and its system is read-only: in Desktop Mode open Konsole and run
`sudo steamos-readonly disable`, `sudo pacman -S webkit2gtk-4.1`, `sudo steamos-readonly enable`
(a password must be set first with `passwd`; when pacman complains about keys, run
`sudo pacman-key --init` and `sudo pacman-key --populate archlinux holo` first; a SteamOS
update can remove the package again, then run the same commands once more).
On Android the same page shows in a web view over the game's window; iPhone, iPad and the Mac
app have a launcher of their own (SwiftUI). The chosen bus is shown as a picture **drawn by
the game's renderer** (the game program runs beside the launcher as its helper, `openomsi
--launcher-service`) - its model, paint, materials, reflections and shadows exactly as in the
game, under the light of the chosen time and weather; drag on it to turn the bus, scroll to
zoom.

The menu on the left groups the pages: **Play** (Drive, Multiplayer, Running games),
**Content** (Mods, Timetable), **Options** (Settings, Controls, Setup) and **Help**
(Tutorials); the driver's **Profile** is at its foot.

* **Drive** - the duty is put together here: the bus (search, liveries, depot file, number
  plate), the map and start point, the line and tour (the lines that run on the chosen date)
  or a free drive, the start time, date, season and weather, the traffic and passengers, and
  the roadbook with the IBIS codes; then **Start the duty** (or **Start driving** for a free
  drive). Settings → General → *Launcher layout* chooses how the page is laid out: *Big Map*,
  *Control Room*, *Classic+* or *Classic* (in steps).
* **Multiplayer** - hosting a duty for friends (start it on the Drive page, then copy its
  session code here), joining a friend's session by its code (then choose the bus on the
  Drive page and press **Join and drive**), and a list of servers that are always on. See
  *LAN play* below and [SERVER.md](SERVER.md).
* **Running games** - every game started from the launcher, with its log and a **Stop** that
  lets it save its run (it is asked to end, given up to 8 s, and only a stuck game is killed);
  for a multiplayer session also the session code to copy, who is playing and the chat.
* **Mods** - installing a bus, a map or a whole folder (see *Mods and the content folder*);
  a folder or a .zip, .7z or .rar dropped on the window is installed.
* **Timetable** - a map's timetable (its TTData lines): its lines, their tours and the trips
  with their departures, edited and saved; new lines and tours can be added, a tour repeated
  at an interval, and **Reset timetable** puts the map's own timetable back.
* **Settings** - everything in `settings.cfg` below, saved as it changes; keys the page
  does not manage are kept as they are. Its tabs: *Graphics* (the graphics mode and the
  quality presets for it first - a preset tunes the mode chosen and keeps it - then the
  display, distances, mirrors and memory, and saved graphics profiles), *Driving* (keys,
  mouse, wheel, pedals, gamepad and force feedback, with the way to the Controls page),
  *Camera* (the seat, the views, head tracking, triple screens, VR on Windows), *Sound*
  (volumes and the radio stations), *Gameplay* (passengers, traffic, collisions, maintenance,
  the clock and the weather sync), *General* (the launcher's look and layout, language, the
  game's interface size, chat, photo mode, the navigator, updates, resetting every setting)
  and *Content test* (not on phones and tablets).
  **Discord Rich Presence** (Settings → General, on by default) shows the launcher or your
  map, bus, line and multiplayer status in Discord; Discord must be running on the same
  computer.
  **Voice chat** (Settings → Sound, off by default): in multiplayer, hold *Voice: nearby*
  (V) to be heard by the players close to you, or *Voice: radio* (B) to be heard by the whole
  session; the keys are in Controls. Switching it on makes the system ask for the
  microphone. See [VOICE.md](VOICE.md).
* **Controls** - `Inputs/keyboard.cfg`: click a key, press the new one; clashes are red. The
  keys and the game controllers are kept in `~/.openomsi/Inputs` (on Windows
  `C:\Users\<you>\.openomsi\Inputs`), so an update or a new program folder keeps them; keys
  saved beside the program are brought over there. The keys are the game's with *Driving
  keys: Custom controls* (Settings → Driving); with a ready-made layout (W A S D, arrows)
  those keys drive and win over the list - the page says so, and changing a key switches to
  Custom controls by itself. *Game controllers*: wheels, pedals, joysticks and button boxes
  as in OMSI's `gamectrler.cfg` - **Set up step by step** finds a device's axes (turn the
  wheel left, press each pedal); every button of the device is listed (press one to jump to
  it). On Windows the devices are read through DirectInput, as OMSI does, so wheels Windows
  lists can be configured for steering. A Logitech G923 pushes the other way from
  DirectInput's force, so the game turns its force round by itself; for any other wheel that
  feels wrong use *Invert force feedback* (**Test the direction** checks it).
  Force feedback needs a driver that supports constant force:
  parking resistance eases as the bus rolls, with centring and
  feedback from the bus's sideways acceleration, short bumps when the front wheels cross
  an edge, plus the scripts' shaking, `FF_Vib_Amp`. On top of that the wheel carries the road
  and the engine all the time. Most of it is the road itself, read from how far the bus has
  driven rather than from the clock, so it is felt as the bus's own weight and not as a
  rattle: waves a few metres long, arriving in the same order every time the bus covers the
  same stretch. The rest is the fine grain of the surface, so a wet or a snowy road hums
  louder than dry asphalt. Alongside it comes the engine's buzz through the frame, which grows
  with the revs and with how hard the engine is working and stays there at a standstill
  (the crankshaft's turning, about 11.7 Hz at idle, with the firing pulses above it). A jolt
  or a scripted shake eases away over the fade time rather than stopping dead, so the wheel
  never clunks when it lets go. Settings → Driving → *Road texture vibration*, *Engine
  vibration* and *Vibration fade-out* set how strong the tremble is (off, normal, or up to
  400 %, since how much of it a wheel can show depends on its motor) and how long the fade
  lasts (off, or up to 1.5 s). A wheel nobody has set up steers with its X axis. A wheel that
  a community controller mapping also makes a gamepad (a Logitech G29) is listed once, and
  *Use this device* switches any device off (it is then neither read nor listed as
  steering); **Remove this device** (after a confirmation) takes one out of the list. Select a
  device to adjust *Steering force* (centering and resistance) and *Vibration* separately.
  Changes are saved by themselves, for that device, in `~/.openomsi/Inputs/gamectrler.cfg`;
  restart a running game to use the new values. *Force feedback and vibration* in Settings →
  Driving is the global on/off switch.
* **Setup** - where the original installation and the game program are, and where the
  content folder (mods) and the settings and logs are. The OMSI 2 folder may be given as a
  path with quotes, as `Omsi.exe` itself or as a folder inside it; unpacked into the OMSI 2
  folder itself, openOMSI keeps its own content in an `openOMSI` folder there and never
  writes to the game's.
* **Tutorials** - OMSI 2's four lessons, each started in its own situation (`--tutorial 1..4`).
* **Profile** - the driver whose personnel file the game writes: hours, experience and level,
  from OMSI's own `.odr` personnel files plus the session summaries the game writes to
  `~/.openomsi/sessions`, and the recent runs.

```bash
scripts/build-macos.sh   # or build-windows.cmd / build-linux.sh: the game opens the launcher
```

Without a person at it: `OPENOMSI_LAUNCHER_SELFTEST=1 openomsi-launcher-app` checks the page in
a hidden window and prints a report (see `launcher/README.md`). The data side is
`crates/omsi-launcher-core`:
`openomsi-launcher --cli lines '{"map":"maps/Grundorf/global.cfg"}'` runs any of its commands
from a terminal - `config`, `maps`, `vehicles`, `weather`, `lines`, `ibis`, `profiles`,
`profile`, `mods`, `modinfo`, `install`, `instances`, `stop`, `log`, `join`, `settings`,
`save_settings`, `keybindings`, `save_keybindings`, `preview`, `args`, `launch`. `lines`
takes a `"date":"YYYY-MM-DD"` as well: the chrono folders active that day add and remove
lines, as in the game, whose default date is 1989-05-30.

## Settings, enhanced graphics, the navigator

`~/.openomsi/settings.cfg` (written by the launcher's settings page, or by hand) holds
`msaa` (1/2/4/8; a count the GPU cannot do falls back to the next lower one, and Real-Time Ray Tracing draws 8 at 4), `anisotropy`
(1..16), `ssao`, `shadows`, `shadow_size`, `shadow_blobs` (the models' `[isshadow]` shadow
meshes, OMSI's flat blob under a vehicle, laid on the road its wheels stand on; off, only the
sun shadow map shades under a vehicle), `navigator`, `ui_opacity` (how much of the interface's backgrounds shows - the navigator's, the
menu's, the timetable's, the plates under the notes - 0.2 to 1, the texts staying solid; 0.85
as designed),
`navigator_corner` (`bottom-left` default, `bottom-right`, `top-left`, `top-right`),
`nav_ai` (the other AI vehicles as dots on the navigator and the city map; on by default),
`nav_surface` (the map's own water, green land, rivers and rail under the roads of the navigator and the city map, when the map has a package, see [MAP_PACKAGE.md](MAP_PACKAGE.md); on by default), `nav_light` (the navigator's maps in a light look; off by default),
`ai_wait_timed_stops_only` (an AI timetable bus ahead of its time waits for its departure only
at its first and last stop, railway stations and the stops the timetable gives a time of their
own, and drives on from the others once boarding is done; off by default: it waits at every
stop it serves, as in OMSI),
`graphics` (the graphics mode: `vanilla` - as OMSI 2 draws it, without sun shadows, ambient
occlusion, detail grain, snow cover or rain drops of openOMSI's own -, `vanilla_plus` - the
same renderer with those extras, the default -, `enhanced` or `enhanced_plus`, see below),
`boarding`, `detail_textures`, `exact_fare`, `pax_bad_driving`, `fullscreen`, `vsync`, `volume`
and `drive_keys`, plus `render_scale` (`auto` or a fraction: the picture is drawn smaller
and upscaled), `post_aa` (`fxaa`, the enhanced renderer's, or `off`), `view_distance` (m,
how far the tiles are kept loaded), `video_memory` ("Video memory", MB: all the game holds
on the graphics card - textures, meshes, render targets, the driver's own; when unset, nine
tenths of a discrete card's memory, and never more than the driver's budget; on a card of
its own the textures may take half of it; a `texture_memory=` line and OMSI's `texmemlimit`
are not read), `texture_compression`
(loose pictures compressed to BC1-BC3 on loading, on by default), `gpu_texture_compression`
(DXT/BC textures kept compressed on the GPU, on by default; off decodes them to RGBA, for a
driver that mishandles block formats, at four to eight times the memory - as
`OMSI_NO_BC=1`), `reflections` (0..1, how strongly the materials' reflection maps,
`[matl_envmap]`, show on paint, chrome and glass; 0 = off, they mirror nothing), `led_glow` (0..15: how
bright an LED destination matrix's dots burn in the enhanced picture, 0 = off - they are
the panel's own light, and the glow draws a halo around them), `led_mips` (0..4, 0.05 steps,
1.3 by default: how much of the mip chain an LED matrix is held at - its picture and its
`\S:n` mask are sampled at the level their screen footprint asks for, never coarser than
this. 0 point-samples them, the sharpest dots and the worst shimmer; 1.3 keeps a matrix's
dots a couple of pixels across where the full chain has run them together; 4 is near the
calm of the full chain). The bus's own screens in Enhanced (the IBIS, ticket and
html terminals, the dashboard's LCDs) dim at night as a real dashboard's do, and are never
lifted over their own colour by the eye's adaptation to the dark cab; the gauges' backlight
and the destination LED matrices (`led_glow`) are left as they are. A mod can also declare a
material its own light outright - `[matl_glow] <texture> <value>` (openOMSI's own keyword, see
docs/FORMATS.md): the texture is a greyscale mask of where it shines (the light is the
material's own colour) and `value` is on this slider's scale, so `6` is as bright as its default
and `20` brighter than its top; the slider does not change it. `mouse_sens` (mouse steering,
1 = OMSI's), `mouse_smooth` (0: the mouse's wheel follows the cursor without easing), `mouse_hold` (0: the
cursor stays free while the mouse steers),
`ui_scale` (the size of the game's interface over the picture - its texts,
the menu, the timetable, the navigator and the city map - from 0.5 to 2, 1 by default, on
top of the screen's own scaling; on a window taller than 1080 lines the interface grows with
it as well, up to twice, unless `ui_scale_window` is off; `notes` off hides the notes in the
top left corner; *Options* in the game menu changes it in quarters while driving, Left
and Right), `steering_linear`, `old_steering` and `red_steer_spd` (the steering switches above),
`ff_invert` (force feedback the other way round), `wheel_range` (the wheel's own rotation,
lock to lock, 900° by default) and `wheel_lock` (how far it is turned for the bus's full
lock; 0 = the whole wheel, as OMSI), `fov` (degrees for the views from the bus; 0 = the bus's
own cameras), `head_idle` (0..1, 0 by default: how far the view sways on its own - a head at
rest breathes and shifts its weight, and 0 of it is exactly as OMSI; most of it is seen while
the bus waits at a stop) and `head_idle_pace` (0.5..2: how fast that sway moves, 1 being the
pace it is designed at), `collision_objects` (walls, poles and bridges stop the bus; off is OMSI's `no_collision`
option and is taken from OMSI's options when openOMSI starts the first time), `graphics_api`
(`auto`, `vulkan`, `dx12` on Windows, `gl`, `angle` on Windows: which graphics interface the
game asks first - with `auto` on Windows DirectX 12, then Vulkan, then OpenGL, then ANGLE;
on Linux and Android Vulkan, then OpenGL; a Mac, iPhone and iPad always use Metal; see
*Graphics API* below), `ctrl_off` (game controllers switched off on the Controls page, by
name, separated by `|`) and `language` (the language of the interface - `ENG`, `DEU`, `FRA`,
`RUS` and the others Settings → General lists; OMSI's own texts, such as the names of cockpit
switches, exist in English, German and French only, and other languages show them in
English). The file also carries a `version`.
With `collision_vehicles` enabled, impacts against AI traffic are delivered to the struck
vehicle's collision scripts and deform a localized part of its body mesh where the model
has editable body geometry.

**Graphics API** (Settings → Graphics → Display, `graphics_api`; not on a Mac): the interface named is asked first and
the others follow, so a machine without it still starts. *ANGLE (DirectX 11)* (Windows) runs
OpenGL ES through Google's ANGLE on the card's DirectX 11 driver: for graphics chips whose
DirectX 12, Vulkan and OpenGL drivers are missing or broken (Intel HD Graphics 2000-4000, AMD
Radeon HD 5000/6000). It is tried last, after the other three failed, and only when
`libEGL.dll` and `libGLESv2.dll` (Google's ANGLE, BSD licence, with its Direct3D 11 renderer)
are next to `openomsi.exe`; without them it is skipped (the log says so). The Windows x64
package ships them, built from ANGLE's own source by
[angle-openomsi](https://github.com/openOMSI-org/angle-openomsi) (there is no ARM64 build);
with them the log names the adapter `ANGLE (…Direct3D11…)` on the `Gl` backend.

`drive_keys` is a control preset: `simple` (W/S/A/D and Up/Down drive, Left/Right keep
OMSI's interior camera switch; the default), `wasd` (W/S/A/D only), `arrows` (the four arrow
keys only; W, S and D keep their OMSI jobs), or `omsi` ("Custom controls") - only the layout
of `Inputs/keyboard.cfg` (OMSI's Shift + numpad, or what the Controls page made of it),
nothing added. **T** sells the ticket a passenger asks for on a bus without a
ticket printer (the original's `ticket_give` key).

`boarding` is how passengers board: `auto` (default) - they walk to the standing place the
cabin's `[ticket_sale]` names, turn to the driver, put the money down, take their ticket by
themselves after a moment and walk on; `pay` - they wait for the driver to sell the ticket
(the bus's printer, or **T**) and give up after 25 s; `walk` - straight into the saloon,
no cash desk (flat fare / ticket machines). People keep a body's width apart outside.
Who gets on the player's bus: on a duty (a line and tour, or a trip) with a destination on
the display, the people waiting for a stop the trip calls at later - also where the bus's
depot file (`.hof`) names the terminus otherwise than the map's timetable does - and those
whose line record lists the terminus shown; everybody gets off at the trip's last stop. In
free drive the bus takes whom its destination display takes, as a timetable bus: those
whose line record lists the terminus shown, and those without one. With no destination set
(or a "not in service" one) nobody waiting gets on; the riders aboard still get off at their
stops.
`exact_fare=0` makes them overpay so that change is due. `pax_bad_driving=0` keeps the riders
quiet about rough driving: no complaints, and nobody decides to get off early because of it. Rain and snow stay outside the
player's bus (its `[boundingbox]`), and heavy rain darkens the day enough for the saloon
lights to matter.

`detail_textures` lays procedural (fractal) grain over the ground and the roads up close,
in Vanilla+ and the enhanced modes alike. `graphics=enhanced` (Enhanced in the launcher, or
`--enhanced`) switches to openOMSI's own physically based renderer: high-range lighting with energy-conserving diffuse and GGX
reflections (roughness from `[matl_envmap]`), a computed atmosphere that also lights the
scene, contact-hardening sun shadows, aerial perspective and height fog, automatic
exposure, a glow only real highlights produce and a photographic tone curve with FXAA
(`post_aa`); no light shafts or grading.

Its light comes from physics, not from colour settings. The atmosphere is computed for the
moment: Rayleigh scattering, ozone, a boundary layer of aerosol whose amount, particle size
(Ångström exponent) and depth change with the weather, a stratospheric aerosol layer, and
light scattered many times over (Hillaire's method) - which is what gives the blue hour its
depth, the twilight its purple and the sunset its colour, different every evening. Clouds
are lit by the sun as it reaches their own height, so they glow pink after the sun has set
for the street; a veil of high cloud dims the sun and spreads it into a white aureole (a
milky sun, soft pale shadows); a passing cumulus takes the sun away from the street, and
the clouds themselves brighten the sky light. The moon stands where it really is with its
real phase and lights the night through the same atmosphere; the stars show where the sky
is dark enough, and a city's lamps light its own haze and clouds (brightest on an overcast
night). The clouds are a volume marched through in steps; `cloud_quality=low` (Graphics:
Cloud quality) takes fewer of them - the same clouds a little grainier at their edges, for
less of the GPU's time (on an M4 at 1080p about 0.8 ms of a frame). The shade of houses and trees over a whole street and the light the surfaces send to each other (Graphics: Street light, `street_light=auto|on|off`) cost about half a millisecond a frame on an M4 at 1080p. Off is the default; On enables them, Auto enables them except on a small or shared graphics chip, and the quality optimiser turns them off when the GPU is the limit. The camera exposes like one: for daylight, part of the way towards the light of the
moment, with a camera's middle-tone contrast; street lamps are bright points with a little
glare in clear air and wide halos in mist and rain.

Every light that lights the street casts a shadow (Enhanced; Graphics: *Light has shadows*, `light_shadows=off|street|all`, *Street lamps and vehicles* by default): the twenty street
lamps, floodlights and platform lights lighting the view most each have a shadow map that looks
down from their head, the headlamps of the player's bus and of the cars in front of the camera
one that looks along the road, and the lamps of the player's own saloon sixteen more, drawn in
the bus's own frame so that they hold still while the bus drives (they hold what stands still in
the bus: the seats, the rails, the panels; what moves - the wheels, the steering wheel, a door
that swings, the people - is left out until it stops). A pole, a car, a wall or a handrail
keeps the light of a lamp off what lies behind it, and the beams the lamps make in fog, rain
and snow are cut by the same maps. The ground is lit by those lamps and by nothing else - the
baked lamp pools of the map (the tile light maps, drawn through walls and without shadow)
belong to Vanilla and Vanilla+. The maps are drawn once and kept while nothing in them
moves (a tile with a tree in the wind in it every third frame), and they take memory only in
the Enhanced modes: about 8 MB at the default shadow size 2048, 33 MB at 4096. *Mirror
quality* (Graphics) *As the picture* draws the mirrors of an Enhanced picture with its light, sky
and sun shadows, *Standard* (the default) with the plain light, which costs a weak card less.
`OMSI_NO_CABIN_SHADOWS=1` leaves the saloon's lamps without shadows for an A/B.

*Every light* (`light_shadows=all`) adds what the street lamps' map does not take: a spot that
is no headlamp - a floodlight, a lamp that lights a door - looks along its own axis over its
cone, and the two strongest lamps that shine every way (a bare point light, no street lamp's
head) have a cube map of six tiles each, so a wall far to their side keeps their light off what
lies behind it, not only what lies under them. There are fifty-six tiles in all, seven rows
under the far shadow map. The clouds throw their shadow over the land as a field (*Cloud
shadows and light shafts*, `cloud_shadows=0|1`; `OMSI_NO_CLOUD_SHADOW_FIELD=1` for an A/B): a
square of nine kilometres worked out from the heaps the sky shows, in cloud space so that the
wind moves it for free, read by the ground's sun and by the air, which scatters the sun's shafts
through the gaps of a broken sky out to seven kilometres. *Glow of lights* (`light_glow=0|1`)
draws the glare of a lens round every bright light, falling as the cube of the distance, and the
lamps' halo in the haze of the night air. Wet roads and still water mirror what stands round
them (*Real-Time Reflections*, `puddles=high|low|off`): the lamps and the lit windows of a
street stream down a wet road at night.

*Real-Time Ray Tracing* (`ray_tracing=1`, `--enhanced-plus`; a file of the days of Enhanced+,
`graphics=enhanced_plus`, reads as Enhanced with it on) is Enhanced with
hardware ray tracing, where the graphics card traces rays (Apple M3/M4 and newer, RTX and
RDNA 2 cards and newer through Vulkan and Direct3D 12; elsewhere it draws as Enhanced, and
should a driver refuse the ray tracing it falls back to Enhanced as well). Every solid mesh within
420 m of the camera goes into an acceleration structure each frame, and the window's
picture traces the sun's shadow per pixel (soft away from its caster, crisp at the
contact; cut-out leaves and fences keep the shadow map, whose texels they need), the
sky's occlusion within two metres, and reflections: wet roads, water, glass, envmapped
and lacquered paint mirror what really stands around them, off the screen too, and the
sky where nothing does. Its shadows, occlusion and reflections cannot be switched off
apart.

The quality presets (*Quality preset*: Low, Medium, High, Extra, Max, Ultra) set the graphics mode
as well as the options: Low is Vanilla, Medium Vanilla+ (what the game starts with), High and above
Enhanced - High with the street lamps' shadows, the clouds' and the glow, Extra with the street light
and the reflections in full, Max with every light's shadows, the mirrors drawn as the picture and
shadow maps of 4096, Ultra with Real-Time Ray Tracing on top of all of it. It shares Enhanced's tone curve, with the light a shade warmer and a light
vignette.
`OMSI_NO_RT=1` opens no ray queries, `OMSI_NO_RT_GRADE=1` leaves its grade out,
`OMSI_RT_REFL_HALF=1` traces the reflections at half size, `OMSI_DEBUG_RT=n` (see
`crates/omsi-render/src/rt.rs`) shows its buffers.

Vanilla, Vanilla+ and Enhanced reflect buses, buildings and scenery in wet road puddles
when `reflections` is above 0, each with its own lighting; Enhanced also shades shallow rain
ripples. Your own bus (with up to three coupled sections) is mirrored as real geometry around
the road's height and slope, out to 60 m; everything else is found by rays traced over the
picture at half resolution, and what those rays cannot see keeps the sky's reflection. From
inside the bus the reflections are seen through its windows, dimmed by the glass tint and
the rain on it. Dry and snow-covered roads and the mirror views draw no puddle reflections,
and on OpenGL the puddles show the sky's reflection only.

On a wet road every vehicle's tyres throw up spray, in every graphics mode: the player's
bus, the AI cars and buses, other players' buses. Through standing water a wheel throws a fan
of water back and up behind it and a mist that hangs behind the vehicle, drifts with the air
and settles; a road that is only wet through gives a thin haze at speed. It grows with the
square of the speed and with the depth of the puddle (next to nothing at walking pace, a cloud
at 50 km/h), and a bus or a lorry throws more than a car. Some vehicles spray through their own
wheel `[smoke]` driven by `tire_wet_freq`/`tire_wet_live`: the stock AI cars (set by their
`main_AI` scripts) and the stock SD200, SD202 and NL/NG buses, the AI SD84 among them (set by
their `spray.osc`). They get that spray only on a road wet through (`StreetCond` 1), as in
OMSI, and the puddles' spray on top. Spray is thrown
within 100 m of the camera (less of it farther off), at most 1400 puffs at once; none shows
inside the bus the camera is in.

Rain costs frames, so Graphics → *Rain quality* (`rain_quality`) sets how much of it is drawn:
**High** (the default) paints the drops on the glass 30 times a second, lets every vehicle
throw spray and fills the air with the full curtain of falling streaks; **Medium** paints the
glass 15 times a second, keeps only your own bus's spray and draws half the streaks; **Low**
shows OMSI 2's own texture rain on the glass instead of the drops (from the next bus loaded),
throws no spray and draws a quarter of the streaks.

Graphics → *Vehicle smoke* (`vehicle_smoke`) chooses **All vehicles** (`all`, the default),
**Player only** (`player`, your bus and its articulated sections), or **Off** (`off`).
It hides non-glowing smoke particles from the other vehicles (including AI cars and LAN
players), or from every vehicle when Off. Scenery smoke, glowing effects and rain spray
keep their own settings and behaviour. The choice takes effect immediately and changes
visibility only; particle simulation continues.

The wet roads' reflections - the puddles mirroring the street, in every graphics mode once
the roads are wet and the reflection maps are on - cost a good share of a weak card's frame
too: Graphics → *Puddles* (`puddles`) **High** (the default) traces them up to 100 m in rays
at half the picture's size and mirrors your own bus in them; **Low** traces them only within
40 m (near the bus), at a quarter of the size and in half the steps, and leaves your bus to
the rays; **Off** draws none (the reflections on paint and glass stay). It takes effect at
once.

The navigator sits in a corner of the screen (lower left by default, `navigator_corner`),
after the Route Advisor of Euro Truck Simulator 2: small, dark and half
transparent, a tilted 3D map that turns with the bus and zooms out with speed - the roads
of the lane network, the trip's route with arrows along it, coloured stretch by stretch by
how busy the road is (blue empty, green light, yellow busy, red heavy, dark red jammed; it
changes as the traffic does), the stops ahead, the other vehicles as blue dots, the other
players of a LAN session or server as purple arrows with their names (on the city map too), the next
turn with its distance and the street it turns into, and the street the bus is on. It
routes on the whole map's road network, read from the tile files in the background at the
start, so the way shows however far the route or the next stop is from the loaded tiles.
From the depot it leads at once to the next stop (never past it onto a later
part of the route); leave the route and it is recalculated after two seconds (Dijkstra
over the lanes, back onto the route ahead). A duty begins with the first trip of the tour
whose first stop the bus can still reach before it leaves. Above the map the speed with
the limit, the line and the time; below it the next stop, its distance, the time to it, its
planned time and whether the bus is early or late, in the interface's language
(`language`). Settings → General → *Navigator* also has *Navigator as a widget* (on by
default: drag it anywhere and size it at its corner; `nav_widget`), *Navigator size*
(`nav_size`), *3D buildings on the map* (`nav_3d`, on by default) and *Route arrows (as in
OMSI 2)* over the road (`nav_arrows`, off by default). A click on the navigator (or
**Shift+M**) opens the city map: a large window over the game
with the whole map from above - every road with its street name (read from the map's
street name signs), the trip's route by traffic with arrows, its stops with their times,
the bus and the traffic; drag to move, the wheel zooms, the buttons centre on the bus and
zoom, Escape or a click outside closes it. **Shift+N** cycles
map → map with the schedule of the next stops → off (N alone is the gearbox's neutral);
`OMSI_DEBUG_NAV=1` logs it.
**Z / X / C** are the indicators. Controls also offers **Indicator left (toggle)** and
**Indicator right (toggle)** for keyboard keys or wheel buttons such as shift paddles.
They start unbound: one press turns that side on, another turns it off, and pressing the
other side switches direction. A script's automatic cancellation is respected.
**Shift + 1**, **Shift + 2**, … open or close a door, front
to back: a bus like the SD200/SD202/EN92 with one two-leaf front door and a combined
aft/stop-brake-release door answers to Shift+1/2/3, a low-floor mod with three or four
independent doors (the O530 Facelift) to Shift+1 through Shift+4/5 - whatever
`bus_doorfront<n>` triggers the bus's own script defines, `bus_dooraft` last (the HUD's
control reminder says how many).

In Settings → Camera, **Driver's view turns with the steering** smoothly turns the driver's
view into the steering direction, independently of the bus's head-motion simulation.
**Steering view angle** sets the full-lock rotation (0–60°, default 30°), and **Steering
view response** sets the smoothing time (50–1000 ms, default 250 ms; larger values follow
more slowly). Manual looking remains available. The automatic turn is suppressed while
VR or an active head tracker controls the view. It is off by default.

Under **Seat position**, **Head pitch** adjusts the driver's neutral view angle up or down
(-45° to +45°). It applies to the driver's view with any display setup, not just triple
screens, and is included when taking offscreen screenshots. Manual looking and head tracking
remain relative to this setting; **Reset the seat position** resets it along with the seat
offsets. The seat set in the game menu is kept for each bus on its own (`seats.cfg` in the
`.openomsi` folder): fitted to one bus, the others keep the views their `.bus` files give
(or the seat of the launcher's settings, for a bus never fitted), and resetting it puts
that bus's views back as its file has them.

In Settings → Camera, **Right stick turns the view** switches automatic gamepad
camera movement on or off. It is on by default. Switch it off to keep using the
Xbox controller for steering and pedals without the right stick moving the camera.
The choice is saved as `right_stick_look=0` (off) or `right_stick_look=1` (on) in
`settings.cfg`. Explicitly assigned look axes and camera buttons continue to work.

## Mods and the content folder

The content folder lies beside the game (beside `openOMSI.app` on macOS). When that place
cannot be written to (for example under Program Files), it is `~/.openomsi/content`
instead. It is laid out like an OMSI 2 installation - `Vehicles`, `maps`, `Sceneryobjects`,
`Splines`, `Texture`, `Fonts`, `Plugins`, `TicketPacks`, `Drivers`, `Weather`, … - and is
searched *before* the original folder: whatever a mod puts there is found exactly as if it
had been copied into OMSI 2, and a file of the same name replaces the stock one. The
original installation is never written to. `OMSI_CONTENT=/some/dir` moves the content
folder; the **Content folder** button on the Mods page opens it.

Depot files can also be placed in a top-level `HOFs/` folder. Every vehicle can use those
`.hof` files without keeping a separate copy in each `Vehicles/<bus>/` folder. If a
vehicle folder and `HOFs/` contain the same file name, the vehicle's own copy takes
priority (the launcher's depot list shows the shared ones after the bus's own).

A map names the depot files its buses need in `maps/<map>/ailists.cfg`. When a mod map is
installed in the openOMSI content folder and some of those files are missing from `HOFs/`, the
launcher's **Mods** page asks whether to share them with every bus. **Share with every bus**
copies them there from the map's folder or from any bus that brought them, so every bus, the
stock ones included, can use the map's depot without copying `.hof` files by hand. **Not for
this map** leaves them where they are: some maps (Grand Paris Moulon, for example) have one
depot per company or per bus, and buses with their own IBIS show wrong destinations with
another one. Without the copy, a bus that lacks a depot still finds it in the map's folder,
and when you choose a line the launcher picks the bus's or the map's depot that carries it.
The copy belongs to the map's mod: switching the map off or removing it takes the copy with it. The original OMSI installation is not changed. A depot file the map needs
that no installed content provides is shown as a warning on the launcher's
**Mods** page. The **Add HOF files to buses** button on that page (desktop launcher) copies
`.hof` files you choose into the content folder's `HOFs/`; files already there are left
untouched.

Installing a mod: on the launcher's **Mods** page, **Choose a folder** or **Choose archive**
opens the system's picker for a mod folder or a `.zip`, `.7z` or `.rar` archive, or you can
drop one onto the window. The launcher sorts it into place: OMSI-style folders anywhere
inside are merged, and a lone bus, map, object or spline folder is recognised by its
`.bus` / `global.cfg` / `.sco` / `.sli` files and put under the right folder. Anything put
into the `Mods/` folder (shown on the page) is installed by itself once it has finished
copying. `openomsi-launcher --cli install '{"path":"/path/to/mod.7z"}'` and `--cli mods`
do the same from a shell. An installation is a background job: the disk is checked for
room before anything is unpacked, everything is unpacked into a staging folder and moved
into place in one step, and a cancelled or failed install leaves nothing behind. A repaint
for a bus that is not installed waits (listed under *Waiting for their bus*) and is
installed when the bus arrives. The page lists the installed mods: each can be switched off
(kept, but not used) or deleted.

Archives can also be **used in place**: a `.zip` laid out like OMSI 2 is put into the
content folder's `Archives/` (hard-linked when it is on the same disk, moved from the
`Mods/` folder, else copied after a free-space check) and read by the game without
unpacking (`--content-zip` and `OMSI_CONTENT_ZIP` mount further archives). The page's
*Archive install mode* chooses: **Auto** (the default) unpacks what fits on the disk and
uses an archive in place when its unpacked size does not, **Unpacked** always unpacks,
**Used in place** keeps the `.zip`. `.7z` and `.rar` archives are always unpacked.
`--cli install '{"path":…,"mode":"inplace"}'` (or `extract` / `auto`) and
`--cli modinfo '{"path":…}'` do the same from a shell. The launcher's lists see the maps
and buses inside the archives.

## Season and weather

The Drive page's **Day & weather** step (in the compact layout: the *Day & weather* button
beside the date) offers these weathers:

* **Natural weather** (the default): the physical weather model described below.
* **Custom weather**: visibility, brightness, wind, temperature, humidity, cloud cover, wet
  roads, rain or snow, snow cover and snow on the road, set by hand. *Edit selected weather
  as custom* starts from the weather chosen.
* **Current weather**: the METAR report of the airport nearest to the map, fetched when the
  game starts (shown when the map has an airport near it; another airport can be chosen).
* **Weather cycle**: the weather moves on to another installed one every 25-60 minutes of
  game time, one that suits the month (snow only from November to March, no frost in
  summer, no heat in winter) and is close to the one before, and every change comes in
  over a few minutes.
* **Tomorrow.io**: live weather for the nearest area the map's `openomsi_weather.cfg`
  configures. Each player needs a private API key in `~/.openomsi/tomorrow-api-key.txt`;
  see [setup and map-author instructions](WEATHER_REGIONS.md), including the
  [community config editor](https://openomsi.mxnticek.eu/weather/).
* the installed weather presets (`Weather/*.owt`), with their names and descriptions.

`--weather natural`, `cycle`, `tomorrow` or a file such as `Weather/Bodennebel.owt` does
the same from the command line.

The same step has a **Season** choice (*By date* as in the original, or spring / summer /
autumn / winter), and under a chosen season its **Early / Mid / Late** part. The phases are the meteorological season's three months (northern spring: March,
April, May; half a year later on a map south of the equator), and choosing one moves the
date to a typical day of it (10th, 15th, 20th; the year and the time of day stay), so the
sun, the length of the day, the natural weather's temperature and snow, the street lamps,
the timetable and holidays all follow. `--season autumn-late` does the same from the
command line (`--season autumn` is the middle; a `--date` already in the phase's month is
kept). The map's seasonal textures follow the phase - early spring still bare (`Winter`),
late summer dry (`SummerDry`), early autumn green, mid autumn coloured (`Fall`), late
autumn bare - and the trees and bushes are mixed between the two looks the phase lies
between: in early autumn a quarter of them have turned, in mid autumn four in five, in
late autumn most stand bare and some still carry their leaves (each plant by its own
number, the same every time and for every LAN player).

Weather presets (`Weather/*.owt`) change the light: overcast takes the sun away, rain and
fog thicken the air, a snow preset puts any map into its winter textures with snow cover.

**Natural weather** (no weather chosen, or `--weather natural`) is a physical weather
model instead of one fixed state: a column of the atmosphere over the map that runs with
the clock. Highs and lows pass through (falling pressure brings rising air: first a veil of
cirrus, then a grey deck, then rain; behind a low the air sinks and clears); the sun warms
the ground through the clouds and the ground cools by radiation at night (far more under a
clear sky); the day's heat mixes the air up from the ground, and where it reaches the
condensation level cumulus forms, to dissolve again in the evening; a calm clear night
cools the air to its dew point and leaves fog that the morning sun burns off; rain washes
the dust out of the air and a still high collects it, and humid air swells it into a milky
haze. So one day is grey from morning to night, the next opens up after a foggy morning,
an afternoon brings showers and a clear blue evening follows - each following from the day
before (the model starts three weeks back), with the season's and the latitude's climate.
It sets everything a weather sets - visibility, wind, temperature, rain or snow, the wet
road - for every graphics mode; Enhanced and Enhanced+ also take its cloud amounts and its
air.

## Radio

OMSI plays no music itself: a bus's radio only sets variables that radio plugins turn into
sound. openOMSI plays internet radio for them, with no plugin needed.

**Which buses.** Every bus whose radio sets `Snd_Radio` (the cassette player of the stock
SD200/SD202/NL and of many mods: it plays the first station) or `SndExt_Radio` (the radios
made for the Sound Extension plugin: station button *n* plays the *n*-th station, the volume
knob `SndVol_Radio` sets how loud). Switch the radio on in the cockpit as in OMSI; the
screen says which station plays and the song when the stream names it.

**Your stations.** `~/.openomsi/radio.cfg` (on Windows `C:\Users\<you>\.openomsi\radio.cfg`)
is written with a few stations the first time the game starts. One station a line, the
first line on the first button:

```
volume = 0.7
Radiozurnal = https://rozhlas.stream/radiozurnal.mp3
Evropa 2 = https://ice.actve.net/fm-evropa2-128
My playlist = https://example.org/station.m3u
```

A station is an MP3, AAC or Ogg stream, or an `.m3u` / `.pls` playlist that points to one.
The address is the one a media player opens - on the station's website, or in a directory
such as radio-browser.info. Streams in HE-AAC with a program config element (some `.aacp`
stations) cannot be played. `volume` (0..1) is the radio's loudness on top of the knob.
The file is read when the game starts. The launcher's Settings → Sound → *Radio stations*
edits the same list: a name and an address a station, the bin removes one, *Add a station*
adds one, saved at once (the file's comments, `volume` and frequencies stay as they are).

**Stations of a radio plugin.** Stations already set up for an OMSI radio plugin (SuperRadio
and the like) are taken over: every line with an http(s) address in the text files under
`plugins` is a station, after those of `radio.cfg`.

**Shift+R** moves the whole list one station on, so that the buttons reach the stations
behind the first ones.

**A map's stations.** A map may bring a `radio.cfg` of its own beside its `global.cfg`: its
stations come first on the buttons while you drive on that map, yours after them. It may
also name the frequency each station is on at places of the map - see
[Modding: Radio](MODDING.md#radio-a-maps-stations-and-a-buss-display).

**The radio's display.** A radio with a text display shows the station and the song,
running through its line where they do not fit (ten characters on Dmitrij's "Magnitola" of
P3ta's SOR buses and its kin, whose first line then shows the map's frequency for the
place, e.g. `94.6 MHz`). Without stations, or with the radio off, a display shows its own texts.

**Where it is heard.** In the cab the radio sounds from the driver's corner, a little ahead
of the driver's eyes - from half as far as it really is, since a bus's speakers are spread
along it: towards the back it is heard from the front but does not fade away. Changing the
view, the sound glides to its new side in a quarter of a second. From outside it is the bus
playing through its windows.

**The radio's own sound.** Settings → Sound (in the game: Options → Sound) has three sliders,
all off at first: *Radio: small speaker* plays it through a small dashboard speaker (no deep
bass, no bright treble, nearly mono and a little pushed on the loud parts), *Radio: FM hiss*
adds the hiss and crackle of a weak signal, *Radio: signal dropouts* loses the signal now
and then for a moment (a bridge, a tall building). With all three at 0 the stream is played
untouched. They are `radio_speaker`, `radio_hiss` and `radio_dropouts` (0..1) in
`settings.cfg`.

**When nothing plays.** `~/.openomsi/game.log` says what happened:
`radio: 23 stations` (the list), `radio: station 1 Radiozurnal (https://…)` (a button
pressed), then `Radio 1 plays Radiozurnal (buffering …)` and the song, or `no signal`
with the reason in a `radio <url>: …` line - mostly an address that is not a stream or a
station that is down. No `radio:
station` line at all means the bus's radio sets neither variable.

## Performance

Settings → Graphics → **Optimise for my PC** measures for about a minute during your next
drive and adjusts the graphics to reach the frame rate you aim at; your settings are saved
first, and *Restore my previous settings* brings them back. Settings → General has
*Frame rate in the corner* and *Performance details* (off, compact, detailed) for the
game's own display of where the time goes. **Video memory** (Settings → Graphics) is how
much of the graphics card the game may use in all (textures, models and the picture;
*Automatic* is nine tenths of the card); big maps stay within it through compressed
textures and tiles that give everything back when they unload.

For measuring: `OMSI_PROFILE=1` logs the time each stage of the frame takes, and with
`--exit-after N` (quit after N seconds), or when you close the game, it writes a summary at the end - frame-time
percentiles and the number of frames over 16.7, 33.3, 50 and 100 ms, counted from 15 s
after the map has loaded. `OMSI_PROFILE_JSON=<file>` also writes the summary to a file.

The game's log is `~/.openomsi/game.log` (the launcher's `launcher.log` beside it), and
the first line of both is the build they were made from.

## Object editor

A small part of what OMSI's map editor does, inside the game: **Ctrl+Shift+E** (or *Object
editor* in the game menu) turns it on, with the free camera where the view is.

* **Objects.** **Enter** (or a click) picks the scenery object nearest the middle of the
  view (a magenta glow marks it), **Tab** the next nearest. **I/K/J/L** move it forward,
  back, left and right as the camera faces, **U/O** lower and raise it, **N/M** turn it
  (half a metre and five degrees a press, a tenth with Shift); with the mouse, drag moves
  it, the wheel turns it and Shift+wheel raises or lowers it. **Delete** deletes it (again:
  back), **Backspace** undoes everything done to it.
* **New objects.** **C** makes a copy of the picked object (the copy is then the one
  edited), **V** gives the copy the shape of the next object of the same folder.
* **Ground.** **Page Up / Page Down** raise and lower the ground under the middle of the
  view (a quarter metre a press, a twentieth with Shift), **F** flattens it to the height
  at the middle, **[** and **]** make the brush smaller and larger.
* **Ctrl+S** saves, **Esc** leaves the editor (unsaved changes stay until the end of the
  session).

Saving writes each changed tile as a copy into the content folder's map folder
(`<content>/maps/<map>/tile_x_y.map`, and the ground as its `.map.terrain` copy), which the
game reads before the installation - the original map is never written; delete the copy to
have the original back. Splines are not part of it, and the timetable is edited on the
launcher's Timetable page. In a LAN session only the host edits the map, and its edits
reach the other players' games.

## Developer tools

For those who build vehicles: **Ctrl+Shift+D** opens a panel at the right of the picture
over the running game (again: closes it; closed it costs nothing). The game goes on under it;
the mouse works the panel where it is over it and the cab everywhere else. Four pages:

* **Variables**: the search lists the bus's script variables (`$` before the text: its string
  variables) with their values; a click puts one on the watch list at the top, which is kept
  per bus in `~/.openomsi/devtools/<bus file>.watch`. A watched variable chosen can be **Set**
  to a value once, or **Held** at it - written into the bus before its scripts run and after
  them, every frame, until *Let go* or until the panel closes.
* **Overlays** over the picture: the paths and their points (with the entries and exits), the
  seats and standing places (and the way they face), the bounding boxes, the axles and
  wheels, the interior lights with their variables and values, the ticket and stamper
  points, the mirrors, the centre of gravity and the views - of every part of an articulated
  bus in its own place.
* **Mirrors**: each mirror moved, turned and widened while you look at it, and *Copy the
  block* puts it on the clipboard as the `[add_camera_reflexion_2]` block it has become, to
  paste into the `.bus` (the block is shown under the buttons, and written to the log too).
* **Files**: what changes in the bus's files is read again within a second while the panel
  is open (or on its button): paths.cfg and `[passengercabin]` (not while people are aboard
  and places, doors or points would go), sound.cfg, the `.bus` (physics, cameras, mirrors,
  bounding box; not when its axles change), model.cfg (animations, visibility and lights, as
  long as it names the same meshes) and the constfiles (as long as the scripts and their
  variables stay). A changed script needs the bus placed anew. *Open the log* opens the
  game's log. Change a copy of a bus in your content folder, never the installation's own
  files.

## Mirror panels

Copies of the bus's mirrors can be laid over the picture, so that the street behind is in
view without looking at the glass. In the cab **Ctrl+M** shows or hides them (the first time a
panel appears for each bus); **Ctrl+Shift+M** starts and ends their editor. The panels are only
pictures until the editor is on, so the mouse and the keys work as always. A panel shows its
mirror as the glass in the bus's model does, however that glass lays the picture on (turned over,
or on its side). In the editor each
panel has a yellow frame, and:

* the left button drags a panel, the wheel over it makes it taller or shorter and **Shift+wheel**
  wider or narrower; **[** and **]** make the panel under the cursor narrower and wider, **;**
  and **'** shorter and taller (held, they repeat);
* the arrows turn the mirror of the panel under the cursor (as Ctrl+Alt+arrows turns the one
  the driver looks at), **Alt+arrows** shift it across and up and **Page Up/Down** forward and
  back, **-** and **+** narrow and widen its field of view; **R** puts that mirror back as the
  bus has it and **Shift+R** every mirror (turns, shifts and fields of view are kept per bus in
  `mirrors.cfg`);
* **Insert** adds a panel (the main side mirrors first, then the others the bus has), **Delete**
  takes the one under the cursor away and **C** shows another mirror in it;
* **Esc** (or Ctrl+Shift+M again) ends the editor and keeps the layout.

A new panel has the shape of the mirror's glass in the model. The layout is kept per bus in
`~/.openomsi/mirror_hud.cfg`. The setting `mirror_hud` (0 off, 1 the right mirror, 2 the left,
3 both) gives a bus with no layout of its own its first panels. The panels need the mirrors
themselves to be drawn (`mirror_size` not 0); they are redrawn at the rate `mirror_refresh`
sets, also when the glass is not in the view. With **Mirror panels every frame** (Settings →
Graphics, `mirror_hud_every_frame=1`) the mirrors the panels show are drawn every frame instead,
as ETS2 draws its screen mirrors, whatever `mirror_refresh` says (even with no real-time
reflections); the other mirrors take their turns without them. It costs one mirror picture a
frame for each mirror on the panels (none in VR, where the panels are not drawn).

## Debug and test switches

Environment variables, all off unless set. The most useful ones are below; [DEBUG_FLAGS.md](DEBUG_FLAGS.md)
lists every switch.

| Variable | What it does |
| --- | --- |
| `OMSI_PROFILE=1`, `OMSI_GPU_TIMERS`, `OMSI_DEBUG_DRAWS` | time per stage of the frame, GPU time per pass, how many instances changed per frame |
| `OMSI_MIRROR_HUD=n` | offscreen: lay the mirror panels (`mirror_hud` 1..3) over the picture |
| `OMSI_SEED=n` | repeat a session: the scripts' `random` is seeded per session (the log says which seed) |
| `OMSI_INPUT="t=3 move x,y; t=3.2 press; t=4 key F3; …"` | drive the real window handlers (mouse, keys, `look`/`turn`) from a script |
| `OMSI_CHURN=x,y` | offscreen check of tile streaming: load the tiles around that far point, unload the start area, unload the far tiles and load the start area again, so the picture is drawn from recycled GPU slots |
| `OMSI_HIDE_WINDOW=from,to` | pretend the window is hidden for those seconds |
| `OMSI_TEXTURE_MEMORY=MB`, `OMSI_BUDGET_FROM=x,y[,MB]` | the texture budget, and meeting it from somewhere else first |
| `OMSI_FLEET_IDLE=s`, `OMSI_FLEET_AHEAD=min` | how long an unused vehicle set is kept, how far ahead the fleet is read |
| `OMSI_NO_BC=1`, `OMSI_NO_TEXCOMPRESS=1`, `OMSI_KEEP_ALLOCATOR=1` | textures as RGBA, no compression of loose pictures, no allocator restart |
| `OMSI_NO_SHADOWS`, `OMSI_NO_CORONAS`, `OMSI_NO_ENVMAP`, `OMSI_NO_BUMP`, `OMSI_NO_CULL`, `OMSI_ENV_PHOTO=0` | leave one part of the picture out for an A/B |
| `OMSI_NO_SURF=1` | roads without the bumps of their textures' `.surf` maps (A/B) |
| `OMSI_NO_PUDDLE_REFLECTIONS=1` | leave wet-road scene reflections out for a screenshot or performance comparison |
| `OMSI_NO_SPRAY=1` | leave the tyres' spray on wet roads out (A/B); `OMSI_DEBUG_RAIN=1` logs how many tyres throw water |
| `OMSI_DEBUG_ENHANCED`, `OMSI_DEBUG_SKY`, `OMSI_DEBUG_EXPOSURE`, `OMSI_METER=…` | the enhanced renderer's lamps, sky, adaptation and metering |
| `OMSI_DEBUG_TRAFFIC`, `OMSI_DEBUG_PAX`, `OMSI_DEBUG_PHYSICS`, `OMSI_DEBUG_LAN`, `OMSI_DEBUG_IBIS`, `OMSI_DEBUG_VARS=a,b` | why a car, a passenger, a wheel, a peer, an IBIS or a script variable does what it does |
| `OMSI_CHECK_ROADS=1`, `OMSI_ROAD_PHOTO=1`, `OMSI_CHECK_ENTRIES=1` | map checks: lanes with no road surface under them and road points under the ground; photograph the road network from above and report grass where a carriageway should be; is there ground to stand on where a player is put down |
| `OMSI_CONTENT=/dir`, `OMSI_CONTENT_ZIP=a.zip:b.zip`, `OMSI_ROOT=/dir` | where the content folder, the archives and the original installation are |
| `OMSI_DEBUG_CONES`, `OMSI_NO_LIGHT_MAP`, `OMSI_DEBUG_LIGHT_GRID` | log the light coronas and beams of each frame; leave the tiles' night light maps out; log the lights a full light-grid cell leaves out |
| `OMSI_PARKED_PULL_OUT=p` | the chance per population pass (about 2 s) that a parked car drives off (0.035 by default), with a log of why one does not |
| `OMSI_NO_BRIDGE=1` | LAN play leaves the internet alone: no rendezvous, no UPnP, no tunnel (tests, LAN parties) |
| `OMSI_NO_LAN_MODS=1` | a LAN host serves no mods and a joining game fetches none |
| `OMSI_BACKEND=vulkan\|dx12\|metal\|gl\|angle` | the graphics interface to ask first, over the setting (the log lists every adapter each one offers; `angle`, also `dx11`/`d3d11`: OpenGL ES on ANGLE over DirectX 11, Windows, with ANGLE's DLLs beside the game) |
| `OMSI_GPU_LIMITS=default\|downlevel` | pretend the graphics card can only do this much (tests of old cards) |
| `OMSI_GPU_ARRAYS=textures\|nostorage\|lighttextures` | read the scene's arrays from textures, as on OpenGL chips without storage buffers in the vertex shader (or without any: no per-pixel lamp light; with `lighttextures` the lamps read from textures too, as on ANGLE) - tests of old cards |
| `OMSI_GL_TEXTURE_UNITS=1` | use the OpenGL backend's texture-unit layout on any device (tests of old cards) |
| `OMSI_RENDER_OCCLUDED=1` | draw even while the window is hidden (tests) |
| `OMSI_CHECK_OBSTACLES=1` | offscreen: drive every lane as a bus and list the objects that would stop it |
| `OMSI_DEBUG_REPEATERS=1` | list the spline object rows whose start the map and the spline chain disagree about |
| `OMSI_TRACE_STEER=file.csv` | write mouse steering, frame by frame (cursor, target, wheel, speed) |
| `OMSI_DAY_AIR=haze,angstrom[,height,strat]` | fix the air (and so the twilight colours) instead of the varying one, for comparisons |
| `OMSI_MUTE=1` | mix all sound as usual but play nothing |

`OMSI_LAN_AUDIO` and `OMSI_LAN_SAY` are switches of the LAN tests.

## LAN play

In the launcher, multiplayer is on the **Multiplayer** page:

* **Connect by Code → Invite friends**: *Set up a hosted duty*, choose map, bus and duty on
  the Drive page and start the duty. The **session code** then appears on the Multiplayer
  and Running games pages, with *Copy code*; send it to your friends.
* **Connect by Code → Join friends**: paste the code, confirm the session (the page shows
  the host's map, time, weather and players), then choose your bus on the Drive page and
  start. The map, time, date and weather are the host's.
* **Servers**: a list of dedicated servers (see [SERVER.md](SERVER.md)) to add, ask and
  join, over UDP or WebSocket.

From the command line, `--lan-host [port]` hosts a session (UDP, port 27015 by default, or
the first free one of 27015 to 27024 when no port is given; a port given is used as it is), `--lan-join <where>` joins one and
`--lan-name` is your name. `--lan-join` takes the session code, an `ip`, `ip:port`, a host
name, a bare port (a host on this machine) or `auto` (find a host on the local network by
broadcast).

A session code looks like `OMSI-7Q4K-2M9X-HD3P-R8TZ-KC5W-NB6E` (no look-alike characters).
It carries the session id and up to three addresses of the host, a VPN's first - Hamachi
(25.x.x.x), Radmin VPN (26.x.x.x), ZeroTier, Tailscale (100.64.0.0/10) - then the LAN's.
The joining game says hello to all of them at once and takes the one that answers. Over
the internet no VPN or port forwarding is needed: the host asks its router to forward the
port (UPnP), both games learn their public address (STUN) and meet through a public
message relay that passes only the addresses (UDP hole punching), and when none of that
gets through, the host's free Cloudflare quick tunnel is the way in (the game fetches
`cloudflared` itself the first time it hosts). When nothing answers within 10 s, the
joining game gives up and says why that may be. Two routers that both change their port
for every destination (some mobile networks) cannot be punched through; a dedicated server
or a VPN works there. Whichever way, the host's firewall must let the game receive UDP on
its port (27015 by default; Windows counts a Hamachi network as public). A busy host (loading a heavy part of the map) still answers, and the
joining game waits up to 8 s for it before it loads anything. The joining game plays on
the host's map whatever map was chosen before, when that map is installed or comes with
the host's mods (below); one that is not is said in the HUD.

Everybody sends the state of their own bus up to twenty times a second (five while nothing
changes, about 60 bytes: a bit-packed datagram with the pose, pedals, lights, indicators,
doors, wheel travel, the rear sections of an articulated bus and the vehicle's own lamp,
switch and sound variables); everything else is text. The host relays and owns the world:
a joining player takes its date, time, weather and season, and its clock keeps everybody
in step. The other players' buses run their own AI scripts with the sender's inputs, are
drawn and heard where they stand, and are obstacles for the AI traffic like your own bus -
as long as that bus type is installed locally, otherwise your own type stands in for it.
**/** opens the chat line (Enter sends, Esc drops it; when your keyboard file gives **/** to
something else, such as a manual gearbox, the key left of 1 or `\` opens it instead) and
**V** hides and shows the chat; both keys can be changed on the Controls page. Joining and
leaving are announced there. The chat grows with the window like the rest of the
interface, and has a size of its own on top: **Ctrl + the mouse wheel** over it, or
Settings → General → *Chat size* (50-300 %). The host checks everything it takes in and
limits how much a player may send.

**Every variable of the other buses.** Besides the pose, each game sends all script
variables of its bus (floats and strings: a gearbox's state, a display's or an IBIS's text,
a ticket printer, what a plugin or `setvar` set): ten times a second the ones that changed,
and all of them in turn in between, so lost datagrams are repaired and a player who joins
late receives the whole state. Strings of up to 1 MiB arrive whole. The copy of a bus in
the other games keeps the owner's values against what its own scripts would make of them.
When the two games have different versions of the bus, the variables both versions have
are still passed on by name, as long as the model and sound configurations use them the
same way; variables only one version has are skipped. `OMSI_NO_VAR_SYNC=1` switches it
off, `OMSI_DEBUG_VAR_SYNC=<variable>` logs what arrives for one variable.

**Request a passenger door.** While on foot, press Ctrl+Alt+S within 3.2 metres of
an entry point outside a bus, or an exit point inside the bus you are riding.
The request feeds that door's `PAX_Entry<n>_Req` or `PAX_Exit<n>_Req`, alongside
simulated passengers. It remains active until the door opens or you leave the point,
change buses or start stepping through a door. AI buses and other players' buses
support this; an exit request also asks an AI bus to serve its next stop.
Change or clear the combination on the Controls page (Keyboard → The game) under
"Passenger: request entry / exit". In multiplayer a request goes to the bus's owner
(or to the host for AI buses).

**AI scripts in LAN play.** AI vehicles read the current game date, time, weather and sun
through `L.S.*`, from their initialization on. A client's copies of the AI buses get the
host's timetable state (`schedule_active`, the whole `GetTT*` stop list and times, the
current stop, the terminus and the delay) before their scripts start.

**The host's mods.** What the host's session uses that is not in the OMSI 2 folder itself
(its map, bus, objects, splines, AI vehicles and people from the content folder or archives,
also a content folder inside the OMSI 2 folder) is listed with a SHA-256 per file and served
over TCP on the session's port; a joining game fetches what it lacks before it loads the map
and keeps the downloads for the next time (`~/.openomsi/lan-store`). Listing a big add-on map
takes the host a while after it starts; a joining game waits for it. Maps installed straight into the OMSI 2 folder are not passed on: both
players need them.

The host's mods are for playing the session, not for keeping: they travel encrypted
(ChaCha20-Poly1305, with a key agreed per connection and bound to the session), and the
joining game never writes them to the disk in plain form. `~/.openomsi/lan-store` holds each
file sealed and named by a keyed hash, and the game reads them decrypted into memory only;
copying them out of the game's folders gives noise, and so does listening on the network.
That is all it does: openOMSI is open source and the decryption runs on the player's own
computer, so a changed build or a dump of the game's memory still gets the files, and the
store's key lies on the same computer (sealed by Windows for the user's account, elsewhere a
file only the user can read: obfuscation, not a lock). The host's own copy is the host's mod
folder, unprotected.

**One world.** The host simulates the AI traffic, the timetable buses, the people on the
pavements and at the stops, the riders of the timetable buses and the traffic lights for
everybody, around every player (it loads the ground and fills the streets around the
others too). A client simulates none of that: it draws the host's world, 160 ms in the
past so that it glides between the host's frames. Only its own bus and the passengers who board it are its
own: when its bus stands at a stop with a door open, it asks the host for the people
waiting there; the host hands over those still waiting and keeps those who meanwhile went
for another bus, so nobody is ever on two buses. The people walking up to a client's bus
and those getting off are sent back up, so the host sees them too. Money and the timetable
of your own duty stay local - but the host's timetable leaves the tour each player drives to
that player (its AI bus goes, and comes back from the next departure when the player leaves),
and everybody sees the passengers in everybody's bus: the riders of a player's bus travel
with the world frames and sit in that bus in every other game.

**Animated parts.** The variables a bus's animations follow (doors, wipers, needles, the
rear sections too) move on the other players' copies in step with the bus itself, also
after a pause, and hold their last state while datagrams are lost.

**Script textures.** Matrix displays and other `[scripttexture]` surfaces come from the
player driving that bus: the other games show exactly what the owner's scripts drew, also
to a player who joins late. Both games need the same bus with the same texture slots; a
stand-in bus (a type you do not have) shows none.

**Script sound events.** `(T.L.name)` and `(T.F.name)` sounds come from the player driving
that bus (and from the host for AI vehicles), in order and once each. The receiving game
plays its own installed sound file - sound files are never transferred - with its own
sound configuration's interior, exterior and position rules; a stand-in bus plays none.
A player who joins later does not hear earlier events again.

All players need the same LAN protocol version: a host turns away a game of another one
(with a message saying so). Windows, macOS and Linux play together.
