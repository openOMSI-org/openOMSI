# User guide

Install and run openOMSI, drive, use the launcher, change settings, install mods and play together.
To build from source, see [BUILDING.md](BUILDING.md). For OpenXR headsets on Windows, see [VR.md](VR.md).

> openOMSI runs on the content of an **original OMSI 2 installation**. Without one the game does not start.

## Quick start: from download to driving

1. **Install OMSI 2**<br>openOMSI plays on your own copy. The Steam version works.
2. **Download openOMSI**<br>Get the build for your system on the [download page](https://openomsi.org/download/). Windows: run the setup, no administrator rights needed. macOS: drag `openOMSI.app` onto Applications. Linux: unpack and run `./openomsi`. Android: install the `.apk`, allow access to all files, copy the OMSI 2 folder to the phone and, if it is not found, choose it under **Setup → Browse** ([ANDROID.md](ANDROID.md)).
3. **Open the launcher**<br>Start openOMSI. The launcher usually finds OMSI 2 by itself; if not, choose the folder with `maps` and `Vehicles` on the **Setup** page.
4. **Build your duty**<br>On the **Drive** page, pick a map and a start point, then **Timetable** with a line and a tour (or **Free drive**), a bus, and the time, date and weather. Click **Start the duty** (**Start driving** for a free drive).
5. **Start the bus**<br><kbd>E</kbd> battery and ignition → hold <kbd>M</kbd> one second to start the engine → <kbd>Shift</kbd>+<kbd>D</kbd> drive → <kbd>.</kbd> release the parking brake. Or <kbd>Shift</kbd>+<kbd>U</kbd> for the whole start-up (a stock SD200; other buses may differ).
6. **Drive**<br><kbd>W</kbd> or <kbd>↑</kbd> throttle, <kbd>S</kbd> or <kbd>↓</kbd> brake, <kbd>A</kbd> <kbd>D</kbd> steer, <kbd>Z</kbd> <kbd>X</kbd> <kbd>C</kbd> indicators.
7. **Serve the stops**<br>The [navigator](#navigator) shows the next turn, the next stop and whether you are early or late; click it or press <kbd>Shift</kbd>+<kbd>M</kbd> for the city map. At a stop, open the doors with <kbd>Shift</kbd>+<kbd>1</kbd>, <kbd>Shift</kbd>+<kbd>2</kbd>, ..., let the passengers board and pay, close the doors and drive on.
8. **Finish**<br><kbd>Esc</kbd> opens the game menu, with *End the tour* and *End the session* (*Leave the server* on a server).

All the keys: [Basic keys](#basic-keys). Other key layouts: [Driving key presets](#driving-key-presets).

## Install and first start

### Start the game

```bash
openomsi --root "/path/to/OMSI 2"
```

- `--root` is needed once; the path is remembered. Without it the game searches `$OMSI_ROOT`, an `OMSI 2 Original` or `OMSI 2` folder beside openOMSI, and the usual Steam locations.
- No arguments: opens [the launcher](#the-launcher).
- `--menu`: the in-game start menu instead (map, vehicle, time, traffic, passengers, timetable, weather, date). Arrows change values, Enter starts, Esc quits.
- Everything given on the command line skips both. See [Command-line flags](#command-line-flags).

### Updates

The launcher asks [github.com/openOMSI-org/openOMSI](https://github.com/openOMSI-org/openOMSI) for the latest release when it starts, every 30 minutes while open, and shortly after a game it started ends. A newer release is offered in a dialog.

- **Update now** downloads it, checks the SHA-256 GitHub lists, replaces the program and restarts the launcher. Mods, content and settings stay.
- Android: the system installer asks. Update replaces and restarts openOMSI; Cancel keeps it.
- Nothing is installed while a game runs.
- In a session the game checks after one minute, then hourly. It downloads in the background and shows a card over the navigator ("openOMSI X is out"). When the session ends, the launcher installs it (by itself with *Install updates without asking*, otherwise it asks).
- A folder openOMSI cannot write to (Program Files, an app opened straight from Downloads on macOS) is reported with what to do.
- A broken download resumes (up to four times). A failed request is retried after 3 and 10 s. If the GitHub API does not answer or hits its hourly limit, the version is read from github.com itself.
- Manual download: [latest release](https://github.com/openOMSI-org/openOMSI/releases/latest).

Settings → General → *Updates*:

- *Look for updates when the launcher starts*: on by default.
- *Update channel*: Stable (tested releases, default) or Beta (every beta at once, for testers).
- *Install updates without asking*: off by default.
- *Tell me about a new version during a session*: off hides the cards; the download still runs.
- Version line with **Check now**.

`OMSI_NO_UPDATE=1` switches checks off. `OMSI_UPDATE_URL` points them at another release description (GitHub's format; `file://` works for testing, and the game then checks after 3 s).

### When a version does not start

On start (double click, launcher) openOMSI first runs a small windowless watcher. It steps in when the launcher crashes, ends on an error, or shows no frames after two minutes (it is then ended), and that version never started on this computer or failed twice in a row. It tries, in order:

1. Install a newer release, if any, and restart. The new program must start before it replaces anything.
2. Restore the version the last update replaced (kept until the new one has started well once), and restart.
3. Restart once in **safe mode** (basic render pipelines, no plugins).
4. Show what to do (download the latest release, where the log is).

- A message box reports each step.
- A failed version is never installed again on this computer; only a newer one.
- A release the project lists as broken (`release/health.json`) goes back to the previous version, or on to a fixed one, at the next start, even if it starts well.
- A crash mid-drive is not a broken start; nothing changes.
- Records: `~/.openomsi/guard/` (`guard.log`, `starts.json`, `bad-versions`). `OMSI_NO_GUARD=1` switches the guard off.
- Android: after a start that never reached the launcher, the next start checks for an update at once (the system installer asks). Rolling back is up to the system.

### "Playing now" counter

During a session the game pings the project's counter (a Cloudflare Worker, `services/presence/`) every ten minutes, and says goodbye at the end. The website and README show how many play right now.

- Sent: a random per-session id, the version, the kind of system. Nothing else; no addresses are kept.
- Off: Settings → General → "Count me in the website's \"playing now\"" (on by default), or `OMSI_NO_PRESENCE=1`.
- Dedicated servers are never counted.

## The launcher

A separate program, `openomsi-launcher-app` (a Tauri window, `launcher/`, see its README). `openomsi` with no arguments opens it.

### Plugins are off by default

For stability, **Settings > General > Disable plugins** is **on by default** for new launcher installations.

- The game then starts without Lua plugins (vehicle and scenery plugins included) and legacy OMSI plugins. Normal OMSI bus scripts still run; plugin-dependent vehicle features may be missing.
- The choice is saved. Existing settings files, including older ones with no `disable_plugins` key, keep their previous behavior. An explicit `disable_plugins=0` or `1` is kept.
- To use a trusted plugin, switch **Disable plugins** off and start a new session.
- Command line: `--no-plugins` (still required for direct CLI runs). `OMSI_NO_PLUGINS=1` forces plugins off even when the launcher allows them.

### If the launcher window does not open

If the launcher is missing or its window cannot open, the game says so and where it belongs (on Linux in a desktop dialog as well as in the terminal).

- **Old Windows without WebView2**: the game offers Microsoft's installer.
- **Linux** needs WebKitGTK 4.1:

| System | Package |
| --- | --- |
| Debian, Ubuntu | `libwebkit2gtk-4.1-0` |
| Fedora | `webkit2gtk4.1` |
| Arch | `webkit2gtk-4.1` |

**SteamOS (Steam Deck)** lacks it and is read-only. In Desktop Mode, open Konsole and run:

```bash
sudo steamos-readonly disable
sudo pacman -S webkit2gtk-4.1
sudo steamos-readonly enable
```

- Set a password first with `passwd`.
- If pacman complains about keys, first run `sudo pacman-key --init` and `sudo pacman-key --populate archlinux holo`.
- A SteamOS update can remove the package; run the commands again.

### Other platforms

Android shows the same page in a web view over the game window. iPhone, iPad and the Mac app have their own launcher (SwiftUI).

### Bus preview

The chosen bus is drawn by the game's renderer, running beside the launcher as its helper (`openomsi --launcher-service`): model, paint, materials, reflections and shadows exactly as in game, under the chosen time and weather. Drag to turn, scroll to zoom.

### Pages

The left menu groups the pages: **Play** (Drive, Depot, Multiplayer, Running games), **Content** (Mods, Timetable, Add-ons), **Options** (Settings, Controls, Setup), **Help** (Tutorials, Troubleshooting). The driver's **Profile** is at the bottom.

- **Drive**: build the duty. Bus (search, liveries, depot file, number plate), map and start point, line and tour (lines running on the chosen date) or free drive, start time, date, season, weather, traffic, passengers, and the roadbook with IBIS codes. Then **Start the duty** (or **Start driving** for a free drive). Settings → General → *Launcher layout*: *Big Map*, *Control Room*, *Classic+* or *Classic* (in steps).
- **Depot**: your own buses (`depot.json`), each keeping its number plate, livery, fleet number, kilometres, fuel, dirt and faults from one drive to the next. Refuel, wash, repair, see its past duties, or **Drive this bus**.
- **Multiplayer**: host a duty, join by code, or pick an always-on server. See [Multiplayer (LAN play)](#multiplayer-lan-play) and [SERVER.md](SERVER.md).
- **Running games**: every game the launcher started, with its log and **Stop**. Stop asks the game to end so it saves its run, waits up to 8 s, and kills only a stuck game. Multiplayer sessions also show the code to copy, the players and the chat.
- **Mods**: install a bus, map or folder; drop a folder, .zip, .7z or .rar on the window. See [Mods and the content folder](#mods-and-the-content-folder).
- **Timetable**: a map's TTData lines, tours and trips with departures. Edit and save, add lines and tours, repeat a tour at an interval. **Reset timetable** restores the map's own.
- **Add-ons**: links to projects others make for openOMSI; nothing is downloaded or installed. See [ADDONS.md](ADDONS.md).
- **Settings**: everything in `settings.cfg` ([Settings reference](#settings-reference)), saved on change. Keys the page does not manage are kept. Tabs:
  - *Graphics*: graphics mode and its quality presets first (a preset tunes the chosen mode and keeps it), then display, distances, mirrors, memory, saved graphics profiles.
  - *Driving*: keys, mouse, wheel, pedals, gamepad, force feedback, link to Controls.
  - *Camera*: seat, views, head tracking, triple screens, VR on Windows.
  - *Sound*: volumes, radio stations.
  - *Gameplay*: passengers, traffic, collisions, maintenance, clock and weather sync.
  - *General*: launcher look and layout, language, in-game interface size, chat, photo mode, navigator, updates, reset all settings.
  - *Troubleshooting*: opens Help → Troubleshooting at its *Model test*; not on phones and tablets.
  - **Discord Rich Presence** (General, off by default): shows the launcher, or your map, bus, line and multiplayer status, in Discord. Discord must run on the same computer.
  - **Voice chat** (Sound, off by default): in multiplayer hold *Voice: nearby* (V) to reach nearby players, or *Voice: radio* (B) for the whole session. Keys are on Controls. Enabling it triggers the system's microphone prompt. See [VOICE.md](VOICE.md).
- **Controls**: `Inputs/keyboard.cfg`. Click a key, press the new one; clashes show red.
  - Keys and controllers live in `~/.openomsi/Inputs` (Windows: `C:\Users\<you>\.openomsi\Inputs`), so updates and new program folders keep them. Keys saved beside the program are brought over there.
  - The list applies with *Driving keys: Custom controls* (Settings → Driving). A ready-made layout (W A S D, arrows) wins over the list; the page says so, and changing a key switches to Custom controls.
  - *Game controllers*: see [Wheels, pedals and force feedback](#wheels-pedals-and-force-feedback).
- **Setup**: locations of the original installation, the game program, the content folder (mods), settings and logs. The OMSI 2 folder can be a quoted path, `Omsi.exe` itself, or a folder inside it. Unpacked into the OMSI 2 folder, openOMSI keeps its content in an `openOMSI` folder there and never writes to the game's files.
- **Tutorials**: OMSI 2's four lessons, each in its own situation (`--tutorial 1..4`).
- **Troubleshooting**: what is wrong with the installed content (packs waiting for their bus, buses missing parts, map HOF files, failed installs), with **Scan again**; its *Model test* tab loads one bus, map, mod or file the way the game does and lists every warning and error (not on phones and tablets).
- **Profile**: the driver whose personnel file the game writes: hours, experience, level and recent runs, from OMSI's `.odr` files plus session summaries in `~/.openomsi/sessions`.

## Driving

### Basic keys

All vehicle keys of `Inputs/keyboard.cfg` work as in OMSI. openOMSI adds simpler driving keys ([presets](#driving-key-presets)).

| Key | Action |
| --- | --- |
| **W** / **Up** | Throttle (default preset) |
| **S** / **Down** | Brake (default preset) |
| **A** / **D** | Steer (default preset) |
| **Left** / **Right** | Step through interior cameras (as in OMSI; default preset) |
| Shift+Num 8 / Shift+Num 2 | Throttle / brake (OMSI) |
| Shift+Num 4 / 6 | Steer (OMSI) |
| **Num 5** | Wheel back to centre (`steering_neutral`) |
| **E** | Battery and ignition |
| **M** | Starter |
| **N** / **R** | Automatic gearbox (N: neutral) |
| **Shift+D** | D of the automatic gearbox |
| **.** | Parking brake |
| **Shift+U** | Full start-up |
| **O** | Mouse steering on/off |
| **Z** / **X** / **C** | Indicators |
| **Shift+1**, **Shift+2**, ... | Doors, front to back |
| **T** | Sell the requested ticket on a bus without a printer (OMSI's `ticket_give`) |
| **F** | High beam |
| **Alt+F** | Flashlight: a lamp at the eye, shining where you look |
| F5-F8 | Destination sign and roller blind (as in OMSI) |
| **Ctrl+S** (or Alt+S) | Quick save (OMSI's `quicksave`) |
| F9 | Write the run into the personnel file |
| **Ctrl+Shift+H** | Skip the next stop |
| **Ctrl+Shift+G** | Get out (`get_up`) |
| **Esc** | Game menu |
| Home | Ticket desk camera (as OMSI's keyboard.cfg binds it) |
| Insert | Timetable view (as OMSI's keyboard.cfg binds it) |
| **/** | Chat in a LAN session (default, see Controls) |

- OMSI uses W, S and D for wiper, viewpoint and gearbox D. Hold Shift for those: **Shift+D** selects D.
- The keyboard.cfg change keys hand out or take back change.
- Camera keys: [Views and camera](#views-and-camera).

### Driving key presets

Settings → Driving → *Driving keys* (`drive_keys`, `--drive-keys`):

| Launcher | `drive_keys` | What drives |
| --- | --- | --- |
| *W A S D + arrows* (default) | `simple` | W/S/A/D and Up/Down. Left/Right keep OMSI's interior camera switch. |
| *W A S D only* | `wasd` | W/S/A/D only. |
| *Arrow keys only* | `arrows` | All four arrows. W/A/S/D stay with OMSI (W, S, D keep their OMSI jobs). |
| *Custom controls* | `omsi` | Only the `Inputs/keyboard.cfg` layout (OMSI's Shift + numpad, or your Controls page edits). Nothing added. |

### Start a stock SD200

1. **E**: battery (also inserts the ignition key).
2. Hold **M** one second: starter.
3. **Shift+D**: drive.
4. **.**: release parking brake.
5. Throttle.

**Shift+U** does it all (main switch, ignition, starter, gearbox to neutral). `--autostart` does the same for offscreen runs.

### Mouse steering

**O** toggles mouse steering, as in OMSI (Omsi.exe's own formula).

- Cursor across the whole window = full left to full right lock (`[inv_min_turnradius]` of the bus).
- Middle to top edge = throttle; middle to bottom edge = brake.
- Above 10 km/h the same movement turns the wheels less (at 50 km/h, a fifth as far), so steering feels heavier with speed.
- For the first second after enabling, wheel and pedals ease towards the cursor.
- Works in driver, passenger and outside views.
- The cursor is held while steering: full lock lies past the window edges, and a cross marks the steering point.

Settings → Driving:

| Setting | Key | Effect |
| --- | --- | --- |
| *Mouse steering sensitivity* | `mouse_sens` | 100 % (`1`) = OMSI. |
| *Mouse pedal strength* | | Cursor travel to full throttle or brake. Above 100 %: full sooner. Below: more travel. |
| *The mouse works the pedals* | | Off: mouse only steers; pedals stay on keys. |
| *Smooth mouse steering* | `mouse_smooth` | On: short easing, no steps. Off (`0`): wheel and pedals jump to the cursor, as in OMSI. |
| *Hold the cursor while the mouse steers* | `mouse_hold` | Off (`0`): free cursor with the system crosshair (no frame delay; works with a tablet pen), kept inside the window while steering; window edges are the lock, as in OMSI (Windows and Linux only: macOS does not let a game keep the cursor inside its window). |

### Steering keys

Settings → Driving, all off by default:

| Setting | Key | Effect |
| --- | --- | --- |
| *Steering linearity* | `steering_linear` | OMSI's steady pace: curvature grows by the same amount each millisecond the key is held, whatever the bus. |
| *Old Steering* | `old_steering` | OMSI's wheel that stays where you leave it; turn it back yourself. |
| *Dynamic steering* | `red_steer_spd` | Keys turn the wheel slower at speed (OMSI's `redSteerSpd`). |

- **Num 5** (`steering_neutral`) centres the wheel in a straight line, as in OMSI, at the key turning pace (never slower than the wheel returns by itself).
- Clutch key, as in OMSI: the pedal drops at once and rises at 0.7 per second on release.

### Wheels, pedals and force feedback

Launcher Controls → *Game controllers*: wheels, pedals, joysticks and button boxes, as in OMSI's `gamectrler.cfg`.

**Setup**

- **Set up step by step** finds the axes (turn the wheel left, press each pedal).
- All buttons are listed; press one to jump to it.
- Windows reads devices through DirectInput, as OMSI does, so wheels Windows lists can be configured for steering.
- An unconfigured wheel steers with its X axis.
- A wheel that a community mapping also exposes as a gamepad (Logitech G29) is listed once.
- *Use this device* off: the device is neither read nor listed as steering. Stored by name in `ctrl_off` (`settings.cfg`), separated by `|`.
- **Remove this device** (after confirmation) drops it from the list.

**Direction.** The Logitech G923 pushes against DirectInput's force, so the game reverses it automatically. For other wheels that feel wrong, use *Invert force feedback* (`ff_invert`); **Test the direction** checks it.

**What you feel** (needs a driver with constant-force support). The bus is felt all the time, not only on impacts:

- parking resistance, easing as the bus rolls;
- centring and sideways-acceleration feedback;
- short bumps when the front wheels cross an edge;
- script shaking (`FF_Vib_Amp`);
- road texture: mostly long waves (a few metres) read from distance driven, not time, so they feel like the bus's weight and repeat on the same stretch; plus fine surface grain, louder on wet or snowy roads than dry asphalt; strongest at speed;
- engine buzz through the frame: rises with revs and load, present at standstill (crankshaft about 11.7 Hz at idle, firing pulses above);
- jolts and scripted shakes fade over the fade time instead of stopping dead, so the wheel never clunks.

Settings → Driving:

| Setting | Values | Effect |
| --- | --- | --- |
| *Force feedback and vibration* | on/off | Global switch. |
| *Road texture vibration* | off, normal, up to 400 % | Road tremble. What a wheel can show depends on its motor. |
| *Engine vibration* | off, normal, up to 400 % | Engine buzz. |
| *Vibration fade-out* | off, up to 1.5 s | Jolt fade time. Off: stops at once. |

**Per device.** Select a device for *Steering force* (centering and resistance), *Vibration*, and *Fine tuning*:

| Setting | Range | Default | Effect |
| --- | --- | --- | --- |
| *Self-centring* | 0-200 % | 100 % | Return to straight after a turn while moving. Never pulls when stopped. |
| *Damping* | 0-300 % | 100 % | Resists fast movement so a released wheel settles instead of overshooting. Raise if it oscillates. |
| *Parking resistance* | 0-200 % | 100 % | Weight when standing or crawling; eases as the bus rolls. |
| *Minimum force* | 0-20 % | 0 % | Lifts small forces past the wheel's dead zone (belt and gear wheels). Keep 0 on direct drive. |
| *Smoothing* | 0-50 ms | 0 ms | Softens grainy force steps on strong wheels at low frame rates. More adds slight delay. |

- Defaults feel as before. **Reset tuning** restores them.
- Same rows in game: Controls → *Game controllers* → device → *Force feedback*; changes apply at once.
- Saved automatically per device in `~/.openomsi/Inputs/gamectrler.cfg`, tuning in `[openOMSI.FFTuning]` (ignored by OMSI 2, so the file still works there; omitted for a wheel at defaults).
- A running game picks up launcher changes at the next drive, or at once via **Reload saved controllers** in its controller menu.

**Wheel rotation** (`settings.cfg`):

| Key | Effect |
| --- | --- |
| `wheel_range` | Wheel rotation lock to lock; 900° by default. |
| `wheel_lock` | Wheel turn for the bus's full lock; 0 = whole wheel, as in OMSI. |
| `cab_wheel_rotation` | Cab steering wheel model rotation lock to lock: 180-1800° in steps of 30, to match `wheel_range`. 0 (default) = the bus's own animation (*Automatic* in launcher and in-game Options). Buses that turn the wheel by script keep their own. |

### Cockpit switches

Left-click a switch to operate it. Hold and move the mouse to turn a knob, or roll the wheel over it (OMSI's `<event>_drag`). The HUD names the switch under the cursor.

### Indicators and doors

- **Z / X / C**: indicators.
- Controls offers **Indicator left (toggle)** and **Indicator right (toggle)** for keys or wheel buttons such as shift paddles. Unbound by default. Press once for on, again for off; the other side switches direction. A script's automatic cancel is respected.
- **Shift+1**, **Shift+2**, ...: doors, front to back, as the bus script's `bus_doorfront<n>` triggers define, `bus_dooraft` last. The HUD control reminder shows how many.
  - SD200/SD202/EN92 (two-leaf front door, combined aft/stop-brake-release door): Shift+1/2/3.
  - Low-floor mod with three or four independent doors (O530 Facelift): Shift+1 to Shift+4/5.

### Passengers and tickets

`boarding`:

| Value | Behavior |
| --- | --- |
| `auto` (default) | Walk to the standing place named by the cabin's `[ticket_sale]`, face the driver, put money down, take the ticket after a moment, walk on. |
| `pay` | Wait for the driver to sell (bus printer or **T**); give up after 25 s. |
| `walk` | Straight into the saloon, no cash desk (flat fare, ticket machines). |

- `exact_fare=0`: passengers overpay, so change is due.
- `pax_bad_driving=0`: no complaints about rough driving, and nobody leaves early because of it.
- Outside, people keep a body's width apart.

Who boards your bus:

- **Duty** (line and tour, or trip) with a destination shown: people waiting for a later stop of the trip (also when the bus's `.hof` names the terminus differently from the map timetable), and people whose line record lists the shown terminus. All get off at the last stop.
- **Free drive**: like a timetable bus, people whose line record lists the shown terminus, and people with no line record.
- **No destination** (or "not in service"): nobody boards. Riders aboard still get off at their stops.

**Passenger animations**: **Settings → Gameplay → Passengers → Passenger animations**: **Original OMSI** (default) or **Natural (procedural)** with IK-placed feet and limbs. `OMSI_PAX_ANIMATION=enhanced` or `=original` overrides it for one session.

**Character models**: **Settings → Gameplay → Passengers → Character models**:

- **Original** (default): OMSI's content people.
- **High quality (improved)**: [Microsoft Rocketbox avatars](https://github.com/microsoft/Microsoft-Rocketbox), 91 of them (everyday people, office workers, workers, nurses, pilots, police officers, children), each matched by kind. Living faces: they blink, look about, smile when paying, sometimes raise brows, frown in thought or yawn late at night, and lip-sync speech.
  - Local look only. Passenger behavior (routes, seats, payment, tickets) is unchanged. In multiplayer others see their own people.
  - Downloaded once when first enabled (about 310 MB, into `~/.openomsi/avatars`). Until then, content people show.
- `OMSI_CHARACTER_MODELS=improved` or `=original` overrides for one session. `OMSI_AVATARS_DIR` sets another avatar folder.

**Request a passenger door.** On foot, press **Ctrl+Alt+S** within 3.2 m of an entry point outside a bus, or of an exit point inside the bus you ride.

- Feeds that door's `PAX_Entry<n>_Req` or `PAX_Exit<n>_Req`, alongside simulated passengers.
- Active until the door opens, or you leave the point, change buses or start stepping through a door.
- Works on AI buses and other players' buses. An exit request also makes an AI bus serve its next stop.
- Rebind or clear it on Controls (Keyboard → The game): "Passenger: request entry / exit".
- In multiplayer it goes to the bus owner (the host for AI buses).

### Journey log

On a duty the game writes a **journey log** to the content folder's `Journeys`: one text file per duty, named by the real start date and time and the line and tour.

- Each trip with its stops: planned and actual arrival and departure, and the difference.
- Late (over 3 minutes), early departure (over 2 minutes, as the personnel file counts), missed stops.
- Rewritten at every stop, so a crash loses nothing. Virtual bus companies ask drivers for this.

### Trolleybuses

A map with an overhead wire (`trolley.txt`) and a bus with poles (`[trolley_pole]`) make a trolleybus that moves only on current. Raise the poles from outside:

1. Get out (Ctrl+Shift+G, the `get_up` setting) or use the outside camera (F3).
2. Go to the back of the bus.
3. Hold **R** and look at the wire; the nearest pole rises and follows your look.
4. Release R when the boom is under the wire.

- **T** lowers the nearest pole. Shift with R or T moves both.
- Motorised poles go up and down from the cab with R and T.

Details, formats and checks: [TROLLEYBUS.md](TROLLEYBUS.md).

## Views and camera

| Key or mouse | Action |
| --- | --- |
| F1 / F2 / F3 / F4 | Driver / passenger / outside / map (free) camera |
| **Left** / **Right** | Step through interior cameras |
| Right-drag (or middle-button drag, OMSI's pan) | Look around: head turns inside, camera orbits outside |
| **I / J / K / L** | Look around |
| Shift+right-drag | Zoom |
| Wheel, **=** / **-**, phone pinch | Zoom: outside camera moves closer; inside view narrows, as in OMSI |
| **Ctrl**+wheel (outside) | Narrow the view instead (telephoto; camera stays put) |
| **Space** | Look ahead again in every view (OMSI's `view_reset_all_directions`) |
| Home | Recentre the current view, unless keyboard.cfg makes Home the ticket desk camera |
| WASD + Q/E | Move the free camera |
| **Ctrl**+click on ground (free camera) | Move the bus to the nearest street |

- Right-drag look and Shift+right-drag zoom are OMSI's `[altView]` mode: Camera setting "Right mouse button turns the view". Off: the right button zooms as in OMSI's default (up: outside camera backs away, inside view widens up to the seat's own), and only the middle button turns the view.
- Each view keeps its own direction; turning the outside camera leaves the driver's head alone.

### Camera settings

Settings → Camera:

- **Driver's view turns with the steering** (off by default): turns the driver's view smoothly towards the steering direction, independent of the bus's head-motion simulation.
  - **Steering view angle**: turn at full lock, 0 to 60°, default 30°.
  - **Steering view response**: smoothing, 50 to 1000 ms, default 250 ms (higher = slower).
  - Manual looking still works. Suppressed while VR or an active head tracker drives the view.
- **Seat position** → **Head pitch**: neutral view angle, -45° to +45°. Applies to the driver's view with any display setup (not just triple screens) and to offscreen screenshots. Manual looking and head tracking are relative to it. **Reset the seat position** resets it with the seat offsets.
- **Seat per bus**: the seat set in the game menu is saved per bus (`seats.cfg` in `.openomsi`). Other buses keep their `.bus` views (or the launcher's seat setting, if never fitted). Reset restores that bus's file views.
- **Right stick turns the view** (on by default): automatic gamepad camera. Turn off to steer and use pedals on an Xbox controller without the right stick moving the camera. Saved as `right_stick_look=0` (off) or `right_stick_look=1` (on). Explicitly assigned look axes and camera buttons still work.

| Key | Effect |
| --- | --- |
| `fov` | View angle from the bus, degrees. 0 = the bus's own cameras. |
| `head_idle` | 0..1, default 0. Idle sway (a resting head breathes and shifts weight), mostly visible at stops. 0 = exactly OMSI. |
| `head_idle_pace` | 0.5..2. Sway speed; 1 = designed pace. |

## Game menu and HUD

**Esc** opens the game menu in the launcher's style: lines on a left rail, the game picture beside it with a card of your vehicle and **Resume**.

- *Resume*, *Options* (launcher settings), *Controls*
- *Camera*: seat, field of view, view and mirror editor
- *Vehicle options*:
  - destination display, depot file, fleet number, driver;
  - drive another placed vehicle, place any installed vehicle, couple/uncouple, get out, swap or reload;
  - service: refuel and wash (only at a petrol station or the depot's wash yard, as in OMSI), repair, put the bus back on its wheels, move it on the map.
- *World options*: clock and date, weather (and custom weather), temperature, wind, traffic, passengers, [object editor](#object-editor)
- *City map*, *Find a street...* (look a street up by name on the city map), *Line and tour*
- *Skip the next stop* (**Ctrl+Shift+H**): for a stop the bus cannot reach or never registers at
- *End the tour*, *Save the situation*, *Save to a slot*, *Load the quicksave*, *Screenshot*, [*Photo mode*](#photo-mode), *End the session*

On a server the menu is shorter: clock and weather belong to the server, there is no saving or loading, and *Leave the server* replaces *End the session*. Use the mouse, or **Up**/**Down** and **Enter**, with **Left**/**Right** to step a value. Settings are kept for the next game.

The **HUD** shows time, speed, line, next stop, delay, the last workshop action, and why the bus is held (parking brake, low air pressure, a line the date's chrono removes). The controls show for the first seconds.

### Photo mode

*Photo mode* in the game menu (or **Ctrl+F12**, key `photo_mode`) pauses the game and gives you a free camera, like racing-game photo modes.

| Keyboard and mouse | Gamepad | Action |
| --- | --- | --- |
| **WASD** | Left stick | Fly (level over the ground) |
| **Q** / **E** (or Space) | Triggers | Down / up |
| Hold right button, left button over the picture, or arrows | Right stick | Look around |
| Wheel, **+** / **-** | D-pad | Zoom |
| **Z** / **X** | Shoulder buttons | Roll |
| **R** | X | Level |
| **Shift** / **Alt** | | Faster / slower |
| **Tab** | | Next panel page |
| **H** | Y | Hide panel |
| **Enter** (or F12) | A | Take photo |
| **Esc** | B | Leave |

The camera stays above ground and within a set distance of the bus (25 m to unlimited). *Orbit* circles the bus instead.

Panel pages:

- *Camera*: free or orbit, focal length (10-400 mm full-frame), roll, speed, allowed distance, frame (window, 16:9, 21:9, 3:2, 4:3, 1:1, 4:5, 9:16; the photo is cropped to it), guides (thirds, golden ratio, centre).
- *Lens*: depth of field (aperture f/1.2-f/22, focus on the bus or a manual distance, bokeh round or 5, 6, 8 blades); motion blur with shutter speed for rolling shots (bus sharp, street streaked; the bus must have been moving when paused); sample count.
- *Colour*: exposure, contrast, highlights, shadows, saturation, vibrance, temperature, tint.
- *Effects*: film look (vivid, warm, cool, vintage, sepia, black and white, noir, bleach bypass, teal and orange, faded film) and strength, grain, vignette, colour fringes, sharpening.
- *Scene*: clock an hour or a quarter forward or back; photo size (window or double).

Depth of field and motion blur are real: each photo averages many frames through different aperture points and shutter moments, so the image sharpens over a second or two once the camera is still (the panel shows progress). The preview is the saved photo: a PNG in `Screenshots`.

Settings → General → *Photo mode* → *Advanced (for photographers)* swaps in a camera body on six pages (*Body*, *Exposure*, *Lens*, *Colour*, *Develop*, *Scene*): sensor size, P/A/S/M exposure with ISO and metering, white balance in kelvin, tilt-shift, histogram, focus peaking, bracketing, 16-bit PNG.

## Navigator and city map

### Navigator

The navigator sits in a screen corner (lower left by default, `navigator_corner`), modelled on Euro Truck Simulator 2's Route Advisor: small, dark, half transparent, a tilted 3D map that turns with the bus and zooms out with speed.

- Lane network roads.
- The trip's route with arrows, coloured live by traffic: blue empty, green light, yellow busy, red heavy, dark red jammed.
- Upcoming stops; other vehicles as blue dots.
- Other players (LAN session or server) as purple arrows with names, also on the city map.
- Next turn with distance and street name; the current street.
- Above: speed and limit, line, time. Below: next stop, distance, time to it, planned time, early/late, in the interface language (`language`).

Routing:

- Uses the whole map's road network, read from the tile files in the background at start, so the route shows however far it is from loaded tiles.
- From the depot it leads straight to the next stop (never past it to a later part of the route).
- Off route, it recalculates after two seconds (Dijkstra over the lanes, back onto the route ahead).
- A duty starts with the first tour trip whose first stop the bus can still reach before departure.

### City map

Click the navigator (or **Shift+M**) to open the city map: a large overlay of the whole map from above.

- All roads with street names (from the map's street name signs), the route by traffic with arrows, its stops with times, the bus and traffic.
- Drag to pan, wheel to zoom, buttons centre on the bus and zoom.
- Escape or a click outside closes it.

**Shift+N** cycles map → map with next-stop schedule → off (N alone is gearbox neutral). `OMSI_DEBUG_NAV=1` logs it.

### Navigator settings

Settings → General → *Navigator*:

| Setting | Key | Default | Effect |
| --- | --- | --- | --- |
| *Navigator* | `navigator` | | Show the navigator. |
| Corner | `navigator_corner` | `bottom-left` | Or `bottom-right`, `top-left`, `top-right`. |
| *Navigator as a widget* | `nav_widget` | on | Drag anywhere, resize at its corner. |
| *Navigator size* | `nav_size` | | |
| *Classic 2D navigator* | `nav_classic_2d` | off | Lighter top-down street map, no detailed terrain or buildings. |
| *3D buildings on the map* | `nav_3d` | on | |
| *Route arrows (as in OMSI 2)* | `nav_arrows` | off | Arrows over the road. |
| *AI vehicles on the map* | `nav_ai` | on | AI vehicles as dots on navigator and city map. |
| *Map surface under the roads* | `nav_surface` | on | The map's water, green land, rivers and rail under the roads, when the map has a package ([MAP_PACKAGE.md](MAP_PACKAGE.md)). |

`nav_light` (`settings.cfg`, off by default): the navigator's maps in a light look.

## Settings reference

Settings live in `~/.openomsi/settings.cfg`, written by the launcher's Settings page or by hand. The file also carries a `version`.

Keys covered elsewhere: [driving](#driving), [camera](#camera-settings), [navigator](#navigator-settings), [radio](#the-radios-own-sound), [mirror panels](#mirror-panels).

### Display and quality

| Key | Values | Effect |
| --- | --- | --- |
| `graphics` | `vanilla`, `vanilla_plus` (default), `enhanced`, `enhanced_plus` | [Graphics mode](#graphics-modes-and-quality-presets). |
| `fullscreen`, `vsync` | | |
| `msaa` | 1/2/4/8 | Falls back to the next lower count the GPU supports. Real-Time Ray Tracing draws 8 at 4. |
| `anisotropy` | 1..16 | |
| `ssao`, `shadows`, `shadow_size` | | |
| `shadow_blobs` | | Models' `[isshadow]` meshes: OMSI's flat blob under a vehicle, laid on the road under its wheels. Off: only the sun shadow map shades under vehicles. |
| `render_scale` | `auto` or a fraction | Draw smaller, then upscale. |
| `post_aa` | `fxaa`, `off` | FXAA (the enhanced renderer's) or none. |
| `view_distance` | metres | How far tiles stay loaded. |
| `detail_textures` | | Procedural (fractal) grain on ground and roads up close, in Vanilla+ and enhanced modes. |
| `reflections` | 0..1 | Strength of material reflection maps (`[matl_envmap]`) on paint, chrome and glass. 0 = off. |
| `led_glow` | 0..15 | Brightness of LED destination matrix dots in the enhanced picture. The dots are the panel's own light; the glow draws a halo around them. 0 = off. |
| `led_mips` | 0..4, step 0.05, default 1.3 | How much of the mip chain an LED matrix is held at: its picture and `\S:n` mask are sampled at the level their screen footprint asks for, never coarser than this. 0 point-samples (sharpest dots, worst shimmer). 1.3 keeps dots a couple of pixels wide where the full chain would blur them. 4 is near the full chain's calm. |
| `graphics_api` | `auto`, `vulkan`, `dx12` (Windows), `gl`, `angle` (Windows) | Graphics interface to try first. See [Graphics API](#graphics-api). |

- **`[matl_glow]`** (modders): `[matl_glow] <texture> <value>` (openOMSI's own keyword, see docs/FORMATS.md) makes a material emit light. The texture is a greyscale mask; the light takes the material's colour. `value` uses the `led_glow` scale (`6` = default brightness, `20` beyond the slider's top). The slider does not affect it.
- **Bus screens at night**: in Enhanced, IBIS, ticket and html terminals and dashboard LCDs dim at night like a real dashboard, and eye adaptation to the dark cab never lifts them above their own colour. Gauge backlights and LED matrices (`led_glow`) are unchanged.

### Memory and textures

| Key | Default | Effect |
| --- | --- | --- |
| `video_memory` | unset | "Video memory" in MB: all GPU memory the game uses (textures, meshes, render targets, driver overhead). Unset: nine tenths of a discrete card, capped at the driver's budget; textures may take half on a dedicated card. `texture_memory=` and OMSI's `texmemlimit` are ignored. |
| `texture_compression` | on | Compress loose pictures to BC1-BC3 on load. |
| `gpu_texture_compression` | on | Keep DXT/BC textures compressed on the GPU. Off decodes to RGBA (for drivers that mishandle block formats) at 4-8x the memory, like `OMSI_NO_BC=1`. |

### Interface

| Key | Values | Effect |
| --- | --- | --- |
| `ui_scale` | 0.5-2, default 1 | Size of the in-game interface (texts, menu, timetable, navigator, city map), on top of system scaling. Game menu *Options* changes it in quarters while driving (Left/Right). |
| `ui_scale_window` | on | On windows taller than 1080 lines, the interface also grows, up to 2x. |
| `ui_opacity` | 0.2-1, designed 0.85 | Opacity of interface backgrounds (navigator, menu, timetable, note plates). Text stays solid. |
| `notes` | | Off hides the notes in the top left corner. |
| `language` | `ENG`, `DEU`, `FRA`, `RUS` and others listed in Settings → General | Interface language. OMSI's own texts (e.g. cockpit switch names) exist only in English, German and French; other languages show them in English. |

### Gameplay

| Key | Default | Effect |
| --- | --- | --- |
| `drive_keys` | `simple` | [Driving key presets](#driving-key-presets). |
| `boarding`, `exact_fare`, `pax_bad_driving` | | [Passengers and tickets](#passengers-and-tickets). |
| `collision_objects` | | Walls, poles and bridges stop the bus. Off = OMSI's `no_collision`, imported from OMSI's options at openOMSI's first start. |
| `collision_vehicles` | | Impacts on AI traffic run the struck vehicle's collision scripts and locally deform its body mesh, where the model has editable body geometry. |
| `ai_wait_timed_stops_only` | off | An early AI timetable bus waits for departure only at its first and last stop, railway stations, and stops with their own timetable time; elsewhere it leaves once boarding is done. Off: waits at every stop it serves, as in OMSI. |
| `volume` | | |

### Graphics modes and quality presets

`graphics`:

- `vanilla`: as OMSI 2 draws it, without openOMSI's sun shadows, ambient occlusion, detail grain, snow cover or rain drops.
- `vanilla_plus` (default): same renderer with those extras.
- `enhanced`: openOMSI's physically based renderer.
- `enhanced_plus`: legacy value, read as Enhanced with Real-Time Ray Tracing on.

*Quality preset* sets the mode and options:

| Preset | Mode and options |
| --- | --- |
| Low | Vanilla |
| Medium | Vanilla+ (the starting preset) |
| High | Enhanced, street lamp shadows |
| Extra | + cloud shadows, light glow, street light, full reflections |
| Max | + shadows from every light, mirrors as the picture, 4096 shadow maps |
| Ultra | + Real-Time Ray Tracing |

### Enhanced renderer

`graphics=enhanced` (Enhanced in the launcher, `--enhanced`): openOMSI's physically based renderer.

- High-range lighting: energy-conserving diffuse, GGX reflections (roughness from `[matl_envmap]`).
- Computed atmosphere that also lights the scene; aerial perspective, height fog.
- Contact-hardening sun shadows.
- Automatic exposure, like a camera: for daylight, partway towards the current light, with a camera's mid-tone contrast.
- Glow only from real highlights; photographic tone curve with FXAA (`post_aa`). No light shafts or colour grading.

Light comes from physics, not colour settings:

- **Sky**: Rayleigh scattering, ozone, a weather-dependent aerosol boundary layer (amount, particle size via Ångström exponent, depth), a stratospheric aerosol layer, multiple scattering (Hillaire's method). Deep blue hour, purple twilight, sunsets that differ each evening.
- **Clouds**: lit by the sun at their own height, so they glow pink after street-level sunset. High cloud veils dim the sun into a white aureole (milky sun, soft pale shadows). A passing cumulus shades the street. Clouds brighten the sky light.
- **Night**: the moon at its real position and phase, lighting through the same atmosphere. Stars where the sky is dark enough. City lamps light their own haze and clouds (brightest when overcast).
- **Street lamps**: bright points with slight glare in clear air, wide halos in mist and rain.

Graphics options (Enhanced unless the row says otherwise):

| Setting | Key | Default | Effect |
| --- | --- | --- | --- |
| *Cloud quality* | `cloud_quality=low` | | Fewer ray-march steps through the cloud volume: same clouds, slightly grainier edges, less GPU time (about 0.8 ms per frame on an M4 at 1080p). |
| *Street light* | `street_light=auto\|on\|off` | Off | Shade of houses and trees across a street, and light bounced between surfaces. About 0.5 ms per frame on an M4 at 1080p. Auto enables it except on small or shared GPUs. The quality optimiser turns it off when GPU-bound. |
| *Light has shadows* | `light_shadows=off\|street\|all` | Off | [Lights and shadows](#lights-and-shadows). |
| *Cloud shadows and light shafts* | `cloud_shadows=0\|1` | | Cloud shadows on the land and sun shafts. |
| *Glow of lights* | `light_glow=0\|1` | | Lens glare around bright lights (falling with the cube of distance), and lamp halos in night haze. |
| *Road glare* | `road_glare=0..2` | 1 | Scales sun, headlight and street lamp highlights on roads and ground (wet roads included) in Enhanced, without changing diffuse daylight. Applies while driving. |
| *Traffic headlight brightness* | `traffic_headlights=0..2` | 1 | White headlamp glows and projected road beams of other vehicles, in Vanilla, Vanilla+ and Enhanced. Not your lamps or street lamps. Applies while driving. |
| *Mirror quality* | | *Standard* | *As the picture*: Enhanced mirrors with its light, sky and sun shadows. *Standard*: plain light, cheaper on weak cards. |
| *Real-Time Ray Tracing* | `ray_tracing=1` | | [Real-Time Ray Tracing](#real-time-ray-tracing). |

### Lights and shadows

*Light has shadows* (`light_shadows`, Enhanced, *Off* by default) gives every street light a shadow.

With it on (`street`, and also under `all`):

- The twenty street lamps, floodlights and platform lights that light the view most: a shadow map looking down from each head.
- Headlamps of your bus and of cars in front of the camera: one looking along the road.
- Your saloon lamps: sixteen more, in the bus's frame so they hold still while driving. Static parts (seats, rails, panels) cast shadows; moving parts (wheels, steering wheel, swinging doors, people) are skipped until they stop.
- Poles, cars, walls and handrails block lamp light, and the same maps cut lamp beams in fog, rain and snow.
- Ground is lit only by these lamps. The map's baked lamp pools (tile light maps, shining through walls, no shadow) belong to Vanilla and Vanilla+.
- Maps are cached while nothing in them moves (a tile with a tree in the wind updates every third frame).
- Memory, Enhanced modes only: about 8 MB at the default shadow size 2048, 33 MB at 4096.
- `OMSI_NO_CABIN_SHADOWS=1`: no saloon lamp shadows (A/B).

`light_shadows=all` (*Every light*) adds:

- Spots that are not headlamps (floodlights, door lamps) cast along their own axis over their cone.
- The two strongest omnidirectional lamps (bare point lights, not street lamp heads) get a six-tile cube map each, so a wall to their side blocks their light too.
- Fifty-six tiles in all, seven rows under the far shadow map.

**Cloud shadows** (*Cloud shadows and light shafts*, `cloud_shadows=0|1`): a 9 km square shadow field built from the visible cloud heaps, in cloud space so the wind moves it for free. Ground sunlight and the air read it; the air scatters sun shafts through gaps in a broken sky out to 7 km. `OMSI_NO_CLOUD_SHADOW_FIELD=1` for A/B.

### Real-Time Ray Tracing

*Real-Time Ray Tracing* (`ray_tracing=1`, `--enhanced-plus`) is Enhanced with hardware ray tracing. Old files with `graphics=enhanced_plus` read as Enhanced with it on.

- Hardware: Apple M3/M4 and newer; RTX and RDNA 2 and newer via Vulkan and Direct3D 12. Elsewhere, or if the driver refuses, it falls back to Enhanced.
- Every solid mesh within 420 m of the camera enters an acceleration structure each frame.
- Traced per pixel: sun shadows (soft away from the caster, crisp at contact; cut-out leaves and fences keep the shadow map, which needs their texels), sky occlusion within 2 m, and reflections (wet roads, water, glass, envmapped and lacquered paint mirror their real surroundings, off-screen too, and the sky where nothing stands).
- Shadows, occlusion and reflections cannot be switched off separately.
- Enhanced's tone curve, slightly warmer light, light vignette.
- Tests: `OMSI_NO_RT=1` (no ray queries), `OMSI_NO_RT_GRADE=1` (no grade), `OMSI_RT_REFL_HALF=1` (half-size reflections), `OMSI_DEBUG_RT=n` (buffers, see `crates/omsi-render/src/rt.rs`).

### Rain, spray, puddles, snow and smoke

**Rain quality** (Graphics → *Rain quality*, `rain_quality`):

| Value | Drops on glass | Spray | Falling streaks |
| --- | --- | --- | --- |
| **High** (default) | Painted 30 times a second | All vehicles | Full curtain |
| **Medium** | 15 times a second | Your bus only | Half |
| **Low** | OMSI 2's texture rain (from the next bus loaded) | None | A quarter |

**Puddles** (Graphics → *Puddles*, `puddles`, also *Real-Time Reflections*: `puddles=high|low|off`): wet roads and still water mirror their surroundings; at night lamps and lit windows stream down the road.

- Vanilla, Vanilla+ and Enhanced reflect buses, buildings and scenery when `reflections` is above 0, each with its own lighting. Enhanced adds shallow rain ripples.
- Your bus (up to three coupled sections) is mirrored as real geometry at the road's height and slope, out to 60 m. Everything else comes from screen-space rays; what they miss shows the sky.
- From inside, reflections show through the windows, dimmed by glass tint and rain.
- No puddle reflections on dry or snowy roads or in mirror views. On OpenGL puddles show only the sky.
- Levels (costly on weak cards; applies at once):
  - **High** (default): rays to 100 m at half resolution; mirrors your bus.
  - **Low**: rays within 40 m, quarter resolution, half the steps; your bus left to the rays.
  - **Off**: none (paint and glass reflections stay).

**Tyre spray** on wet roads, in all graphics modes, from every vehicle (yours, AI cars and buses, other players).

- Standing water: a fan of water back and up behind each wheel, plus mist that hangs, drifts and settles. A merely wet road gives thin haze at speed.
- Grows with speed squared and puddle depth (almost none at walking pace, a cloud at 50 km/h). Buses and lorries throw more than cars.
- Some vehicles spray through their own wheel `[smoke]` driven by `tire_wet_freq`/`tire_wet_live`: stock AI cars (`main_AI` scripts) and stock SD200, SD202, NL/NG, including the AI SD84 (`spray.osc`). That spray appears only on a fully wet road (`StreetCond` 1), as in OMSI, with puddle spray on top.
- Within 100 m of the camera (less farther away), max 1400 puffs. None inside the bus the camera is in.

**Built-up snow on roads and roofs** (`road_snow=0|1`, Graphics), **off by default**: gradual snow build-up on roads and roofs with tyre tracks. Off keeps the weather's conventional OMSI 2 road snow. Saved preferences are kept.

**Vehicle smoke** (Graphics → *Vehicle smoke*, `vehicle_smoke`): **All vehicles** (`all`, default), **Player only** (`player`: your bus and its articulated sections), **Off** (`off`). Hides non-glowing smoke from other vehicles (AI cars and LAN players included), or from all with Off. Scenery smoke, glowing effects and rain spray are unaffected. Applies at once; only visibility changes, the particle simulation continues.

### Graphics API

Settings → Graphics → Display, `graphics_api` (not on a Mac). The chosen interface is tried first, then the others, so the game still starts without it.

- Windows `auto`: DirectX 12, Vulkan, OpenGL, ANGLE.
- Linux and Android: Vulkan, OpenGL.
- Mac, iPhone, iPad: always Metal.

**ANGLE (DirectX 11)** (Windows) runs OpenGL ES via Google's ANGLE on the DirectX 11 driver, for chips whose DirectX 12, Vulkan and OpenGL drivers are missing or broken (Intel HD Graphics 2000-4000, AMD Radeon HD 5000/6000).

- Tried last, after the other three fail.
- Needs `libEGL.dll` and `libGLESv2.dll` (Google's ANGLE, BSD licence, Direct3D 11 renderer) next to `openomsi.exe`; otherwise skipped (logged).
- The Windows x64 package ships them, built from ANGLE source by [angle-openomsi](https://github.com/openOMSI-org/angle-openomsi). No ARM64 build.
- When active, the log names the adapter `ANGLE (…Direct3D11…)` on the `Gl` backend.

## Performance

- Settings → Graphics → **Optimise for my PC**: measures for about a minute on your next drive and tunes graphics to your target frame rate. Your settings are saved first; *Restore my previous settings* brings them back.
- Settings → General → *Frame rate in the corner* and *Performance details* (off, compact, detailed): where frame time goes.
- **Video memory** (Settings → Graphics): total GPU memory the game may use (textures, models, picture). *Automatic* = nine tenths of the card. Big maps fit through compressed textures and tiles that free everything on unload.

Measuring:

- `OMSI_PROFILE=1` logs time per frame stage.
- With `--exit-after N` (quit after N seconds), or on close, it writes a summary: frame-time percentiles and frames over 16.7, 33.3, 50 and 100 ms, counted from 15 s after map load.
- `OMSI_PROFILE_JSON=<file>` also writes the summary to a file.

**Logs**: `~/.openomsi/game.log`, with the launcher's `launcher.log` beside it. Line one of each is the build.

## Season and weather

### Weather choices

Drive page → **Day & weather** step (compact layout: the *Day & weather* button by the date):

- **Natural weather** (default): physical weather model ([below](#natural-weather)).
- **Custom weather**: set visibility, brightness, wind, temperature, humidity, cloud cover, wet roads, rain or snow, snow cover and road snow by hand. *Edit selected weather as custom* starts from the current choice.
- **Current weather**: METAR of the airport nearest the map, fetched at game start. Shown when an airport is near; another can be chosen.
- **Weather cycle**: every 25-60 game minutes, switches to another installed weather that suits the month (snow only November to March, no frost in summer, no heat in winter) and is close to the last. Each change blends over a few minutes.
- **Tomorrow.io**: live weather for the nearest area configured in the map's `openomsi_weather.cfg`. Each player needs a private API key in `~/.openomsi/tomorrow-api-key.txt`. See [setup and map-author instructions](WEATHER_REGIONS.md) and the [community config editor](https://openomsi.mxnticek.eu/weather/).
- Installed presets (`Weather/*.owt`) with names and descriptions.

Command line: `--weather natural`, `cycle`, `tomorrow`, or a file like `Weather/Bodennebel.owt`.

- Presets change the light: overcast hides the sun, rain and fog thicken the air, a snow preset switches any map to winter textures with snow cover.
- Rain and snow stay outside your bus (its `[boundingbox]`). Heavy rain darkens the day enough for saloon lights to matter.

### Natural weather

Natural weather (no weather chosen, or `--weather natural`) simulates a column of atmosphere over the map, running with the clock:

- Highs and lows pass. Falling pressure lifts air: cirrus veil, grey deck, then rain. Behind a low the air sinks and clears.
- The sun warms the ground through clouds; at night the ground cools by radiation, much more under clear skies.
- Daytime heat mixes the air upward; cumulus forms at the condensation level and dissolves by evening.
- Calm clear nights cool to the dew point, leaving fog the morning sun burns off.
- Rain washes dust out, a still high collects it, humid air swells it into milky haze.

So days vary (all grey; foggy then clear; afternoon showers then a blue evening), each following the last (the model starts three weeks back), within the season's and latitude's climate. It drives everything a weather sets (visibility, wind, temperature, rain or snow, wet road) in all graphics modes. Enhanced and Enhanced+ also use its cloud amounts and air.

### Season

The same step has **Season**: *By date* (as in the original), or spring, summer, autumn, winter, each with **Early / Mid / Late**.

- Phases are the meteorological season's three months (northern spring: March, April, May; shifted half a year on maps south of the equator).
- A phase moves the date to a typical day (10th, 15th, 20th), keeping year and time. Sun, day length, natural temperature and snow, street lamps, timetable and holidays follow.
- Command line: `--season autumn-late`; `--season autumn` is Mid. A `--date` already in the phase's month is kept.
- Seasonal textures follow: early spring bare (`Winter`), late summer dry (`SummerDry`), early autumn green, mid autumn coloured (`Fall`), late autumn bare.
- Trees and bushes mix the two looks around the phase: early autumn a quarter turned, mid autumn four in five, late autumn mostly bare with some still in leaf. Each plant decides by its own number, the same every time and for every LAN player.

## Radio

OMSI plays no music: a bus radio only sets variables for radio plugins. openOMSI plays internet radio for them, no plugin needed.

### Which buses

- Radios that set `Snd_Radio` (cassette player of the stock SD200/SD202/NL and many mods): play the first station.
- Radios that set `SndExt_Radio` (made for the Sound Extension plugin): button *n* plays station *n*; the `SndVol_Radio` knob sets volume.

Switch the radio on in the cockpit as in OMSI. The screen shows the station, and the song when the stream names it.

### Your stations

`~/.openomsi/radio.cfg` (Windows: `C:\Users\<you>\.openomsi\radio.cfg`) is created with a few stations on first start. One station per line; the first line is button one:

```
volume = 0.7
Radiozurnal = https://rozhlas.stream/radiozurnal.mp3
Evropa 2 = https://ice.actve.net/fm-evropa2-128
My playlist = https://example.org/station.m3u
```

- Formats: MP3, AAC or Ogg streams, or `.m3u` / `.pls` playlists pointing to one.
- Use the address a media player opens (station website, or a directory such as radio-browser.info).
- HE-AAC with a program config element (some `.aacp` stations) cannot play.
- `volume` (0..1): radio loudness on top of the knob.
- Read at game start.
- Launcher Settings → Sound → *Radio stations* edits the same list: name and address per station, bin to remove, *Add a station*. Saves at once; comments, `volume` and frequencies in the file stay.

**Radio plugin stations** (SuperRadio and similar) are imported: every http(s) line in text files under `plugins` becomes a station, after those in `radio.cfg`.

**Shift+R** shifts the whole list by one station, so buttons reach stations beyond the first few.

**A map's stations**: a map may ship its own `radio.cfg` beside `global.cfg`. Its stations come first while you drive there, yours after. It may also set per-place frequencies. See [Modding: Radio](MODDING.md#radio-a-maps-stations-and-a-buss-display).

### The radio's display

Text displays show station and song, scrolling when too long. Example: Dmitrij's "Magnitola" in P3ta's SOR buses and similar (ten characters), whose first line then shows the map's local frequency (e.g. `94.6 MHz`). With no stations or the radio off, the display shows its own texts.

### Where it is heard

- In the cab: from the driver's corner, slightly ahead of the eyes, at half its real distance (speakers run the length of a bus). From the back it still sounds from the front, without fading.
- Changing view, the sound glides to its new side in a quarter second.
- Outside: the bus playing through its windows.

### The radio's own sound

Settings → Sound (in game: Options → Sound), three sliders, all 0 at first (0 = untouched stream):

| Slider | Key (0..1) | Effect |
| --- | --- | --- |
| *Radio: small speaker* | `radio_speaker` | Small dashboard speaker: no deep bass or bright treble, nearly mono, slightly compressed peaks. |
| *Radio: FM hiss* | `radio_hiss` | Hiss and crackle of a weak signal. |
| *Radio: signal dropouts* | `radio_dropouts` | Brief signal loss now and then (bridges, tall buildings). |

### When nothing plays

Check `~/.openomsi/game.log`:

- `radio: 23 stations`: the list.
- `radio: station 1 Radiozurnal (https://…)`: a button press.
- `Radio 1 plays Radiozurnal (buffering …)` and the song, or `no signal` with the reason in a `radio <url>: …` line (usually not a stream address, or the station is down).
- No `radio: station` line at all: the bus radio sets neither variable.

## Mods and the content folder

### The content folder

The content folder sits beside the game (beside `openOMSI.app` on macOS), or in `~/.openomsi/content` if that place is not writable (for example Program Files).

- Laid out like OMSI 2: `Vehicles`, `maps`, `Sceneryobjects`, `Splines`, `Texture`, `Fonts`, `Plugins`, `TicketPacks`, `Drivers`, `Weather`, ...
- Searched *before* the original folder: mod files work as if copied into OMSI 2, and same-named files replace stock ones.
- The original installation is never written to.
- `OMSI_CONTENT=/some/dir` moves it. The Mods page's **Content folder** button opens it.

### Install a mod

On the launcher's **Mods** page:

- **Choose a folder** or **Choose archive** (`.zip`, `.7z`, `.rar`), or drop one on the window.
- The launcher sorts it: OMSI-style folders anywhere inside are merged; a lone bus, map, object or spline folder is recognised by its `.bus` / `global.cfg` / `.sco` / `.sli` files and placed correctly.
- Files put into `Mods/` (shown on the page) install by themselves once copying ends.
- Installs run in the background: disk space is checked first, files unpack to a staging folder and move into place in one step, and a cancelled or failed install leaves nothing.
- A repaint for a missing bus waits under *Waiting for their bus* and installs when the bus arrives.
- Installed mods are listed; each can be switched off (kept, unused) or deleted.
- Shell: `openomsi-launcher --cli install '{"path":"/path/to/mod.7z"}'` and `--cli mods`.

### Archives used in place

A `.zip` laid out like OMSI 2 can be **used in place**: stored in the content folder's `Archives/` (hard-linked on the same disk, moved from `Mods/`, otherwise copied after a space check) and read without unpacking.

- `--content-zip` and `OMSI_CONTENT_ZIP` mount further archives.
- *Archive install mode*: **Auto** (default) unpacks what fits on disk and uses the archive in place when its unpacked size does not; **Unpacked** always unpacks; **Used in place** keeps the `.zip`.
- `.7z` and `.rar` are always unpacked.
- Shell: `--cli install '{"path":…,"mode":"inplace"}'` (or `extract` / `auto`), `--cli modinfo '{"path":…}'`.
- Launcher lists include maps and buses inside archives.

### Depot files (HOFs)

- A top-level `HOFs/` folder shares `.hof` files with every vehicle, so no copy is needed in each `Vehicles/<bus>/`.
- Same file name in a vehicle folder and `HOFs/`: the vehicle's copy wins. The launcher lists shared depots after the bus's own.
- **Add HOF files to buses** (Mods page, desktop launcher) copies chosen `.hof` files into `HOFs/`; existing files are left untouched.

A map lists the depot files its buses need in `maps/<map>/ailists.cfg`. If a mod map in the openOMSI content folder needs files missing from `HOFs/`, the **Mods** page asks:

- **Share with every bus**: copies them from the map's folder or any bus that has them, so every bus (stock included) can use the map's depot. The copy belongs to the map's mod and goes when the map is switched off or removed. The original OMSI installation is not changed.
- **Not for this map**: leaves them. Some maps (Grand Paris Moulon, for example) have one depot per company or bus, and buses with their own IBIS show wrong destinations with another.
- Without the copy, a bus missing a depot still finds it in the map's folder, and choosing a line makes the launcher pick the bus's or map's depot that has it.
- A needed depot that no installed content provides shows as a warning on the Mods page.

## Multiplayer (LAN play)

### In the launcher

**Multiplayer** page:

- **Connect by Code → Invite friends**: *Set up a hosted duty*, choose map, bus and duty on Drive, start the duty. The **session code** appears on Multiplayer and Running games with *Copy code*; send it to friends.
- **Connect by Code → Join friends**: paste the code, confirm the session (host's map, time, weather, players), choose your bus on Drive and press **Join and drive**. Map, time, date and weather are the host's.
- **Servers**: dedicated servers ([SERVER.md](SERVER.md)) to add, query and join, over UDP or WebSocket.

### From the command line

| Flag | Meaning |
| --- | --- |
| `--lan-host [port]` | Host over UDP. Default port 27015, or the first free of 27015-27024 when none is given. A given port is used as is. |
| `--lan-join <where>` | Join: session code, `ip`, `ip:port`, host name, bare port (host on this machine) or `auto` (find a LAN host by broadcast). |
| `--lan-name` | Your name. |

### Connecting

A session code looks like `OMSI-7Q4K-2M9X-HD3P-R8TZ-KC5W-NB6E` (no look-alike characters). It holds the session id and up to three host addresses, VPN first (Hamachi 25.x.x.x, Radmin VPN 26.x.x.x, ZeroTier, Tailscale 100.64.0.0/10), then LAN. The joiner tries all at once and takes the first answer.

Over the internet no VPN or port forwarding is needed:

1. The host asks its router to forward the port (UPnP).
2. Both games learn their public address (STUN) and meet through a public relay that passes only addresses (UDP hole punching).
3. Failing that, the host's free Cloudflare quick tunnel is used (`cloudflared` is fetched automatically the first time you host).

- No answer within 10 s: the joiner gives up and suggests why.
- Two routers that both change port per destination (some mobile networks) cannot be punched through; use a dedicated server or VPN.
- The host's firewall must allow inbound UDP on its port (27015 by default). Windows treats a Hamachi network as public.
- A busy host (loading a heavy map area) still answers; the joiner waits up to 8 s before loading.
- The joiner plays on the host's map regardless of its own choice, if the map is installed or comes with the host's mods. Otherwise the HUD says so.
- All players need the same LAN protocol version; a host rejects others with a message.
- Windows, macOS and Linux play together.

### Chat

- **/** opens the chat line; Enter sends, Esc cancels. If your keyboard file uses **/** for something else (e.g. a manual gearbox), the key left of 1 or `\` opens it.
- **V** hides/shows the chat. Both keys are on the Controls page.
- Joins and leaves are announced.
- Chat scales with the window, plus its own size: **Ctrl + mouse wheel** over it, or Settings → General → *Chat size* (50-300 %).

### What is shared

**Bus state**: each player sends their bus up to twenty times a second (five when idle), about 60 bytes: a bit-packed datagram with pose, pedals, lights, indicators, doors, wheel travel, rear sections of articulated buses, and the vehicle's own lamp, switch and sound variables. Everything else is text.

- The host relays and owns the world: joiners take its date, time, weather and season, and its clock keeps everyone in step.
- Other players' buses run their own AI scripts with the sender's inputs, are drawn and heard in place, and block AI traffic like yours, if that bus type is installed locally. Otherwise your own type stands in.
- The host validates all input and rate-limits each player.

**All script variables** (floats and strings: gearbox state, display or IBIS text, ticket printer, plugin or `setvar` values):

- Changed ones ten times a second, the rest in rotation, so lost datagrams are repaired and late joiners get full state.
- Strings up to 1 MiB arrive whole.
- Remote copies keep the owner's values over what their own scripts compute.
- Different bus versions: shared variables still sync by name if model and sound configs use them the same way; others are skipped.
- `OMSI_NO_VAR_SYNC=1` disables it. `OMSI_DEBUG_VAR_SYNC=<variable>` logs one variable.

**Animated parts**: animation variables (doors, wipers, needles, rear sections) move in step with the remote bus, also after a pause, and hold their last state during packet loss.

**Script textures**: matrix displays and other `[scripttexture]` surfaces come from the driving player, exactly as their scripts drew them, also for late joiners. Both games need the same bus with the same texture slots; a stand-in bus shows none.

**Script sound events**: `(T.L.name)` and `(T.F.name)` sounds come from the driving player (the host for AI vehicles), in order, once each. The receiver plays its own installed sound file (sound files are never sent), with its own interior, exterior and position rules. Stand-in buses play none. Late joiners do not hear earlier events.

**AI scripts**: AI vehicles read game date, time, weather and sun through `L.S.*` from initialization on. Clients' AI bus copies get the host's timetable state (`schedule_active`, the full `GetTT*` stop list and times, current stop, terminus, delay) before their scripts start.

### One world

The host simulates, for everyone and around every player: AI traffic, timetable buses, pedestrians and people at stops, timetable bus riders, traffic lights. It loads ground and fills streets around the other players too.

- Clients simulate none of this. They draw the host's world 160 ms in the past to glide between host frames.
- A client owns only its bus and the passengers who board it. Stopped with a door open, it asks the host for the people waiting. The host hands over those still waiting and keeps those who left for another bus, so nobody is on two buses.
- People walking up to or getting off a client's bus are sent back to the host, which sees them too.
- Money and your duty's timetable stay local. The host's timetable gives each player's tour to that player: its AI bus goes, and comes back from the next departure when the player leaves.
- Everyone sees everyone's passengers: riders travel with the world frames and sit in that bus in every game.

### The host's mods

Content the session uses that is not in the OMSI 2 folder itself is shared: map, bus, objects, splines, AI vehicles and people from the content folder or archives, including a content folder inside the OMSI 2 folder.

- Listed with a SHA-256 per file and served over TCP on the session port.
- Joiners fetch what they lack before loading the map and keep it for next time (`~/.openomsi/lan-store`).
- Listing a big add-on map takes the host a while after start; joiners wait.
- Maps installed directly into the OMSI 2 folder are not shared; both players need them.

Shared mods are for playing, not keeping:

- Sent encrypted (ChaCha20-Poly1305, key agreed per connection and bound to the session).
- Never written to disk in plain form: `~/.openomsi/lan-store` keeps each file sealed and named by a keyed hash, decrypted into memory only. Copying files out, or sniffing the network, yields noise.
- Limits: openOMSI is open source and decrypts on the player's computer, so a modified build or memory dump still gets the files. The store key is on the same computer (sealed by Windows for the user account, elsewhere a user-only file): obfuscation, not a lock.
- The host's own copy is its unprotected mod folder.

## Object editor

A small part of OMSI's map editor, in game. **Ctrl+Shift+E** (or *Object editor* in the game menu) turns it on, with the free camera at the current view.

| Key | Action |
| --- | --- |
| **Enter** (or click) | Pick the scenery object nearest the view centre (magenta glow) |
| **Tab** | Pick the next nearest |
| **I / K / J / L** | Move forward, back, left, right (camera-relative) |
| **U / O** | Lower / raise |
| **N / M** | Rotate |
| Drag / wheel / Shift+wheel | Move / rotate / raise or lower |
| **Delete** | Delete (again: restore) |
| **Backspace** | Undo all changes to it |
| **C** | Copy the picked object (the copy becomes the edited one) |
| **V** | Give the copy the shape of the next object in the same folder |
| **Page Up / Page Down** | Raise / lower ground at the view centre |
| **F** | Flatten ground to the centre height |
| **[** / **]** | Smaller / larger brush |
| **Ctrl+S** | Save |
| **Esc** | Leave (unsaved changes last until the session ends) |

- Objects: half a metre and five degrees per press; a tenth with Shift.
- Ground: a quarter metre per press; a twentieth with Shift.
- Saving writes changed tiles as copies into the content folder (`<content>/maps/<map>/tile_x_y.map`, ground as `.map.terrain`), read before the installation. The original map is never written; delete the copy to restore it.
- Splines are not editable here. Edit the timetable on the launcher's Timetable page.
- In a LAN session only the host edits, and edits reach the other players.

## Mirror panels

Copies of the bus mirrors laid over the picture, so you see behind without looking at the glass.

- **Ctrl+M** (in the cab) shows or hides them. The first time, a panel appears for each bus.
- **Ctrl+Shift+M** starts and ends the editor.
- Outside the editor, panels are only pictures; mouse and keys work as usual.
- A panel shows its mirror as the model's glass does, however it maps the picture (flipped, or sideways).

In the editor each panel has a yellow frame:

| Key or mouse | Action |
| --- | --- |
| Left drag | Move a panel |
| Wheel / **Shift+wheel** over a panel | Taller or shorter / wider or narrower |
| **[** / **]** | Narrower / wider (panel under cursor; repeats when held) |
| **;** / **'** | Shorter / taller (repeats when held) |
| Arrows | Turn the mirror of the panel under the cursor (like Ctrl+Alt+arrows for the mirror the driver looks at) |
| **Alt+arrows** | Shift it across and up |
| **Page Up/Down** | Shift it forward and back |
| **-** / **+** | Narrow / widen its field of view |
| **R** / **Shift+R** | Reset that mirror / every mirror to the bus's values |
| **Insert** | Add a panel (main side mirrors first, then the rest) |
| **Delete** | Remove the panel under the cursor |
| **C** | Show another mirror in it |
| **Esc** (or Ctrl+Shift+M) | End the editor, keep the layout |

- Mirror turns, shifts and fields of view are saved per bus in `mirrors.cfg`.
- New panels take the shape of the mirror glass in the model.
- Layouts are saved per bus in `~/.openomsi/mirror_hud.cfg`.

| Key | Effect |
| --- | --- |
| `mirror_hud` | First panels for a bus without a layout: 0 off, 1 right mirror, 2 left, 3 both. |
| `mirror_size` | Panels need mirrors drawn: must not be 0. |
| `mirror_refresh` | Panel redraw rate, also when the glass is out of view. |
| `mirror_hud_every_frame=1` | **Mirror panels every frame** (Settings → Graphics): panel mirrors drawn every frame, like ETS2's screen mirrors, regardless of `mirror_refresh` (even without real-time reflections). Other mirrors take turns without them. Costs one mirror render per frame per panel mirror (none in VR, where panels are not drawn). |

## Developer tools

For vehicle builders. **Ctrl+Shift+D** opens a panel on the right over the running game (again to close; closed, it costs nothing). The game keeps running. The mouse works the panel over it and the cab elsewhere.

- **Variables**: search the bus's script variables (`$` prefix: string variables) with values. Click to add one to the watch list at the top, saved per bus in `~/.openomsi/devtools/<bus file>.watch`. A watched variable can be **Set** once, or **Held**: written into the bus before and after its scripts every frame, until *Let go* or the panel closes.
- **Overlays**: paths and points (with entries and exits), seats and standing places (and facing), bounding boxes, axles and wheels, interior lights with their variables and values, ticket and stamper points, mirrors, centre of gravity, views. Each part of an articulated bus in its own place.
- **Mirrors**: move, turn and widen each mirror while looking at it. *Copy the block* copies the resulting `[add_camera_reflexion_2]` block to paste into the `.bus` (also shown under the buttons and written to the log).
- **Files**: bus files reload within a second of a change while the panel is open (or via its button):
  - paths.cfg and `[passengercabin]` (not while people are aboard and places, doors or points would go);
  - sound.cfg;
  - the `.bus` (physics, cameras, mirrors, bounding box; not when axles change);
  - model.cfg (animations, visibility, lights, if it names the same meshes);
  - constfiles (if scripts and their variables stay the same).
  - A changed script needs the bus placed again. *Open the log* opens the game log.
  - Edit a copy of a bus in your content folder, never the installation's files.

## Advanced

### Check an installation

```bash
cargo run --release -p omsi-check -- "/path/to/OMSI 2"
```

Loads every content file with openOMSI's loaders and reports what failed.

### Command-line flags

Everything can also be given on the command line, which then skips both the launcher and the start menu.

| Flag | Meaning |
| --- | --- |
| `--root "/path/to/OMSI 2"` | OMSI 2 installation (remembered) |
| `--map maps/Grundorf/global.cfg` | Map |
| `--bus Vehicles/MAN_SD200/MAN_SD80.bus` | Player vehicle (`--paint name`, `--hof name`, `--plate "B-AB 1234"`) |
| `--entry N` / `--spawn x,y,heading` | Start position |
| `--time HH:MM --date YYYY-MM-DD --weather Weather/x.owt` | Time, date (sets the season), weather; e.g. `--weather Weather/Schmuddelwetter.owt --date 1989-01-15` |
| `--traffic N --schedule --line 76 --tour 1 --passengers` | AI cars, timetable buses, your tour, people at stops |
| `--radius N` / `--all` | Tiles around the start / whole map |
| `--view-distance M` | Tile loading radius around the camera (m, default 1200) |
| `--content-zip mod.zip` | Read an archive in place as a content root (repeatable; same as `OMSI_CONTENT_ZIP`) |
| `--offscreen out.png --cam x,y,z,yaw,pitch --drive secs` | Render one frame to a file |
| `--snapshots 7,9,11` / `--follow auto\|bus\|moving\|type:X` | More pictures during a `--drive` run / camera behind an AI vehicle |
| `--riders N --refuel --wash --repair --dirt 0..1` | Passengers aboard, depot services, starting dirt |
| `--driver Drivers/OMSI-Fan.odr` | Personnel file; the run is added to it |
| `--drive-keys simple\|wasd\|arrows\|omsi` | Control preset (`drive_keys`; default `simple`) |
| `--autostart` | Put the bus into service first (like Shift+U) |
| `--click x,y[,dx,dy]` | Press (and drag) the cockpit switch at that pixel, offscreen |
| `--season winter` (`autumn-late`, `spring-early`, ...) | Season override |
| `--situation x.osn` | Load a saved situation |
| `--physics simple` | Kinematic dynamics instead of rigid body |
| `--enhanced` | Physically based renderer |
| `--enhanced-plus` | Enhanced+: physically based renderer with ray-traced shadows, ambient occlusion and reflections |
| `--export-glb bus.glb` | Write the bus as glTF (the launcher preview) and quit |
| `--menu` / `--no-menu` | In-game start menu instead of the launcher (opened by a start without arguments), or neither |
| `--no-plugins` | No Lua, vehicle or legacy OMSI plugins this session |
| `--tutorial 1..4` | One of OMSI 2's four lessons |
| `--exit-after N` | Quit after N seconds |
| `--lan-host`, `--lan-join`, `--lan-name` | [Multiplayer](#from-the-command-line) |

### Launcher from a terminal

```bash
scripts/build-macos.sh   # or build-windows.cmd / build-linux.sh: the game opens the launcher
```

- `OPENOMSI_LAUNCHER_SELFTEST=1 openomsi-launcher-app` checks the page unattended in a hidden window and prints a report (see `launcher/README.md`).
- The data side is `crates/omsi-launcher-core`. `openomsi-launcher --cli lines '{"map":"maps/Grundorf/global.cfg"}'` runs its commands from a terminal: `config`, `maps`, `vehicles`, `weather`, `lines`, `ibis`, `profiles`, `profile`, `mods`, `modinfo`, `install`, `instances`, `stop`, `log`, `join`, `settings`, `save_settings`, `keybindings`, `save_keybindings`, `preview`, `args`, `launch`.
- `lines` also takes `"date":"YYYY-MM-DD"`: chrono folders active that day add and remove lines, as in game (default date 1989-05-30).

### Debug and test switches

Environment variables, all off unless set. The most useful are below; [DEBUG_FLAGS.md](DEBUG_FLAGS.md) lists all.

| Variable | Effect |
| --- | --- |
| `OMSI_PROFILE=1`, `OMSI_GPU_TIMERS`, `OMSI_DEBUG_DRAWS` | Time per frame stage, GPU time per pass, instances changed per frame |
| `OMSI_MIRROR_HUD=n` | Offscreen: draw mirror panels (`mirror_hud` 1..3) |
| `OMSI_SEED=n` | Repeat a session: seeds the scripts' `random` (the log shows the seed) |
| `OMSI_INPUT="t=3 move x,y; t=3.2 press; t=4 key F3; …"` | Drive the real window handlers (mouse, keys, `look`/`turn`) from a script |
| `OMSI_CHURN=x,y` | Offscreen tile streaming check: load tiles at that far point, unload the start area, unload the far tiles, reload the start, so the picture uses recycled GPU slots |
| `OMSI_HIDE_WINDOW=from,to` | Pretend the window is hidden for those seconds |
| `OMSI_TEXTURE_MEMORY=MB`, `OMSI_BUDGET_FROM=x,y[,MB]` | Texture budget, and meeting it from elsewhere first |
| `OMSI_FLEET_IDLE=s`, `OMSI_FLEET_AHEAD=min` | How long an unused vehicle set is kept; how far ahead the fleet is read |
| `OMSI_NO_BC=1`, `OMSI_NO_TEXCOMPRESS=1`, `OMSI_KEEP_ALLOCATOR=1` | RGBA textures; no loose-picture compression; no allocator restart |
| `OMSI_NO_SHADOWS`, `OMSI_NO_CORONAS`, `OMSI_NO_ENVMAP`, `OMSI_NO_BUMP`, `OMSI_NO_CULL`, `OMSI_ENV_PHOTO=0` | Leave one part of the picture out (A/B) |
| `OMSI_NO_SURF=1` | Roads without their textures' `.surf` bumps (A/B) |
| `OMSI_NO_PUDDLE_REFLECTIONS=1` | No wet-road scene reflections (screenshots, performance comparison) |
| `OMSI_NO_SPRAY=1` | No tyre spray (A/B); `OMSI_DEBUG_RAIN=1` logs how many tyres throw water |
| `OMSI_DEBUG_ENHANCED`, `OMSI_DEBUG_SKY`, `OMSI_DEBUG_EXPOSURE`, `OMSI_METER=…` | Enhanced renderer lamps, sky, adaptation, metering |
| `OMSI_DEBUG_TRAFFIC`, `OMSI_DEBUG_PAX`, `OMSI_DEBUG_PHYSICS`, `OMSI_DEBUG_LAN`, `OMSI_DEBUG_IBIS`, `OMSI_DEBUG_VARS=a,b` | Why a car, passenger, wheel, peer, IBIS or script variable does what it does |
| `OMSI_CHECK_ROADS=1`, `OMSI_ROAD_PHOTO=1`, `OMSI_CHECK_ENTRIES=1` | Map checks: lanes without road surface and road points underground; aerial road photo reporting grass where a carriageway should be; ground under player spawn points |
| `OMSI_CONTENT=/dir`, `OMSI_CONTENT_ZIP=a.zip:b.zip`, `OMSI_ROOT=/dir` | Content folder, archives, original installation |
| `OMSI_DEBUG_CONES`, `OMSI_NO_LIGHT_MAP`, `OMSI_DEBUG_LIGHT_GRID` | Log light coronas and beams per frame; drop tile night light maps; log lights a full light-grid cell drops |
| `OMSI_PARKED_PULL_OUT=p` | Chance per population pass (about 2 s) that a parked car pulls out (default 0.035), logging why one does not |
| `OMSI_NO_BRIDGE=1` | LAN without internet: no rendezvous, UPnP or tunnel (tests, LAN parties) |
| `OMSI_NO_LAN_MODS=1` | LAN host serves no mods; joiners fetch none |
| `OMSI_BACKEND=vulkan\|dx12\|metal\|gl\|angle` | Graphics interface to try first, overriding the setting (log lists each one's adapters; `angle`, also `dx11`/`d3d11`: OpenGL ES on ANGLE over DirectX 11, Windows, with ANGLE's DLLs beside the game) |
| `OMSI_GPU_LIMITS=default\|downlevel` | Pretend the GPU can only do this much (old-card tests) |
| `OMSI_GPU_ARRAYS=textures\|nostorage\|lighttextures` | Read scene arrays from textures, as on OpenGL chips without vertex-shader storage buffers (or none at all: no per-pixel lamp light; `lighttextures`: lamps from textures too, as on ANGLE); old-card tests |
| `OMSI_GL_TEXTURE_UNITS=1` | OpenGL backend texture-unit layout on any device (old-card tests) |
| `OMSI_RENDER_OCCLUDED=1` | Draw while the window is hidden (tests) |
| `OMSI_CHECK_OBSTACLES=1` | Offscreen: drive every lane as a bus and list objects that would stop it |
| `OMSI_DEBUG_REPEATERS=1` | List spline object rows whose start the map and spline chain disagree on |
| `OMSI_TRACE_STEER=file.csv` | Log mouse steering per frame (cursor, target, wheel, speed) |
| `OMSI_DAY_AIR=haze,angstrom[,height,strat]` | Fix the air (and twilight colours) for comparisons |
| `OMSI_MUTE=1` | Mix all sound, play nothing |

`OMSI_LAN_AUDIO` and `OMSI_LAN_SAY` are LAN test switches.
