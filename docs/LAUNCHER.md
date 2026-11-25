# Launcher

The launcher is the window that opens when you start openOMSI: you put a drive together there,
install mods, change keys and settings, and update the game. This page describes every screen
and is the complete reference of every setting with its default.

## The window

The menu on the left groups the pages:

| Group | Pages |
| --- | --- |
| **Play** | Drive, Depot, Servers, Running games |
| **Content** | Mods, Plugins, Timetable, Add-ons |
| **Options** | Settings, Controls, Setup |
| **Help** | Tutorials, Troubleshooting |

The driver's **Profile** is at the foot of the menu. The arrow at the bottom collapses the menu to
icons. A folder or a `.zip`, `.7z` or `.rar` dropped on the window is installed as a mod.

The launcher opens on **Drive**, or on **Setup** when it cannot find OMSI 2. While a game runs
and the launcher is not in front, it rests (shows nothing) so that the game has the graphics card
to itself.

The launcher is a program of its own beside the game. `openomsi` started without arguments opens
it. Where it is missing, or its window cannot open (no WebView2 on an old Windows, no WebKitGTK
4.1 on Linux), the game says so. On Windows it offers Microsoft's WebView2 installer. On Android
the same page shows over the game's window; iPhone, iPad and the Mac app have a launcher of their
own (see [iPhone, iPad and Mac](APPLE.md)).

## Drive

The duty is put together here and started. **Settings → General → Launcher layout** chooses how
the page is laid out:

| Layout | What it looks like |
| --- | --- |
| **Big Map** (default) | the route map fills the page |
| **Control Room** | everything side by side |
| **Classic+** | two tabs, time and weather on top |
| **Classic** | in steps, as before |

### Map and duty

- **Map**: search the installed maps, filtered by *All*, *OMSI 2* or *Mods*. The route map shows
  roads, stops and entry points.
- **Timetable** or **Free drive**. With *Timetable*, choose a **Line** (filter by name) and a
  **Tour** of it. Only lines and tours that run on the chosen date are listed; *Show ended tours*
  adds the rest. A tour that does not run on that date says when it runs next, with *Drive it on
  that day*.
- The **Roadbook** lists the trips of the tour with their departures and IBIS codes (line, route
  code, depot file). Click a trip to start with it. *Shift+U in the game types it for you*.
- **Start at**: the entry point. *Automatic* takes the one nearest the duty's first stop.
- *Show the command line* shows the game command the launcher will run.

### Bus

- Search the buses, filtered by *All*, *Starred* or *Mods*. A bus with parts from packs that are
  not installed says which packs are missing.
- **Livery** (the arrows preview the next and previous one), **Depot file**, **Fleet number** and
  **Number plate** (*Automatic* by default), and the *Onboard computer*.
- The bus picture is drawn by the game's own renderer under the chosen time and weather. Drag to
  turn it, scroll to zoom.
- **Add a note**: your own note on a bus, kept on this computer.

### Day and weather

- **Start time** and **Date**. The clock button sets the current time, the calendar button
  today's date. The mouse wheel turns the hours (left) or the minutes (right).
- **Season**: *By date* (as in OMSI) or *Spring*, *Summer*, *Autumn*, *Winter*, each with
  *Early*, *Mid* or *Late*.
- **Cars around**: AI traffic, 0 (*None*) to 300 (120 on phones).
- **Passengers**: people wait at the stops and ride along.
- **Timetable buses**: other buses of the timetable drive their lines.
- **Bus ready to go**: engine running, doors and lights set.
- **Start on foot**: walk to the bus, or place one later from the menu.
- **Weather**: see [Weather and seasons](WEATHER.md).
- **Start with a comfortable cabin temperature**: the cabin is set to 22 °C (72 °F) at the start
  of a drive. A saved situation keeps its own.

### Starting

Press **Start the duty**, or **Start driving** for a free drive. In multiplayer the button reads
**Host and drive** or **Join and drive** (see [Playing together](MULTIPLAYER.md)). With a game
already running it asks first and then reads **Start another game**. Saved situations can be
continued, renamed and deleted from the same page.

## Depot

Your own buses. Each keeps its number plate, its kilometres, its fuel and its wear from one drive
to the next.

1. Press **Add a bus**, choose the bus, its livery, a name and a number plate.
2. Group buses into depots with **New depot**, **Rename the depot** and **Delete the depot**.
3. **Drive this bus** takes it to the Drive page.

Each bus shows its dirt and its faults, with **Refuel**, **Wash**, **Repair** and **Repair all**,
and its **Recent runs**. Parts never wear out with **Maintenance** set to *Infinite* (the
default): choose *Normal* in **Settings → Gameplay** so that bulbs blow and doors wear as in OMSI.

## Servers

Dedicated servers and friends' sessions: saved servers, public servers, joining by address or by
session code. See [Playing together](MULTIPLAYER.md) and [Dedicated server](SERVER.md).

## Running games

Every game started from the launcher, and the games that ended in the last half hour.

- **Show the log file** opens that game's log. Any number of games can run at once.
- **Stop** asks the game to end as if Escape and Quit were chosen. It is given up to 8 s; only a
  stuck game is killed, and then its run is not saved.
- An ended game shows how it ended (its exit code, or killed).
- For a multiplayer session: the session code to copy, the players, and the chat.

## Mods

Install buses, maps and scenery from a folder or a `.zip`, `.7z` or `.rar`; switch installed
mods off or delete them; share a map's depot files. See [Mods](MODS.md).

## Plugins

Lua and OMSI plugins of the game. The **Installed** tab lists them with how they did in the last
game (*Ran*, *Did not load*, *Stopped after errors*), each with its settings. **Browse** lists
reviewed plugins to install, update or remove, with *Read the code*. **Plugins folder** opens the
folder. Nothing runs while **Settings → General → Disable plugins** is on. See
[Writing plugins](PLUGINS.md).

## Timetable

A map's timetable, edited and saved.

1. Choose the map and a **Line**, or make one with **New line**.
2. Make a **Tour**: the day of one bus, trip after trip, each with its departure time.
3. Add trips that exist, or **New trip on the map**: click the stops one after the other. Only
   the stops the map has a way to are ringed, and the route is drawn as you go. Depots are the
   amber squares.
4. **Copy +min** copies a tour later in the day; **Repeat** copies it every few minutes up to a
   time. `-1 min` / `+1 min` move a whole tour.
5. **Save all**.

Changes are saved as the line's `.ttl` and the trip's `.ttp` files in the content folder. OMSI
2's own files stay as they are. **Reset** puts the map's own timetable back.

## Add-ons

Projects made for openOMSI by others, with a link to each website. The list is read from [openOMSI-org/add-ons-list](https://github.com/openOMSI-org/add-ons-list) each time the page opens. See [Add-ons](ADDONS.md).

## Controls

The keys of `Inputs/keyboard.cfg` and the game controllers. Keys and controllers are kept in
`~/.openomsi/Inputs` (on Windows `C:\Users\<you>\.openomsi\Inputs`), so an update or a new
program folder keeps them. Keys saved beside the program are brought over there.

### Keyboard

The keys are in two lists: the game's (views, menu, chat …) and the bus's (*Driving & the bus*).

1. Click a key, then press the new one. Hold Shift, Ctrl or Alt for a combination. `Esc` cancels,
   `Backspace` or `Delete` clears the key.
2. A key that is used twice is red, and the page says what else uses it.
3. **Add another key for this action**, **Clear the key**, **Remove the entry** and **Add
   binding** edit the list. **Reset to defaults** brings back OMSI 2's keys.

The bus list can be filtered by a bus folder or `.bus` file, and says how many installed buses
use each action. With a ready-made **Driving keys** preset (Settings → Driving), the preset's
driving keys win over the list; changing a key switches to **Custom controls**. Every default key
is listed in [Driving](DRIVING.md#keys).

### Controls profiles

A profile is a named set of every key and every controller setting. One is in use, one is the
default for every bus, and a bus can have its own (**Use for {bus}**). Before each drive the
launcher switches to the bus's profile, or to the default one. Profiles can be duplicated,
renamed and deleted.

### Game controllers

Wheels, pedals, joysticks and button boxes, as OMSI's `gamectrler.cfg` has them. A gamepad works
without setting up (see [Driving](DRIVING.md#gamepad)).

- **Set up step by step** finds a device's axes: turn the wheel left, press each pedal. **Set up
  its buttons only** skips the axes.
- **Axes and pedals**: each axis with its job, *Reversed*, *Dead zone* and *Response curve*.
- **Buttons**: every button of the device, *By action* or *By button*. Press a button on the
  device to jump to it. An action can be *Latching* or *Continuous*. A hat can look round.
- **Use this device** switches a device off: it is neither read nor used for steering. **Remove
  this device** forgets its axes and buttons.
- A device nobody set up steers with its X axis. On Windows devices are read through DirectInput,
  as OMSI does.

**Force feedback** needs a driver that supports constant force. Per wheel:

| Setting | What it does | Default |
| --- | --- | --- |
| Steering force | overall strength of the steering forces | 100 % |
| Self-centring | how strongly the moving bus turns the wheel back straight (0–200 %) | 100 % |
| Damping | resistance to fast movement, so a released wheel settles (0–300 %) | 100 % |
| Parking resistance | how heavy the wheel is at a standstill (0–200 %) | 100 % |
| Minimum force | lifts small forces above a belt or gear wheel's dead zone (0–20 %) | 0 % |
| Smoothing | softens grainy steps in the force (0–50 ms) | 0 ms |
| Vibration | shakes, kerb jolts, road and engine tremble | 100 % |
| Invert force feedback | for a wheel that pushes the wrong way | off |

**Test the direction** checks the direction (take your hands off the wheel). A Logitech G923 is
turned round by itself. **Reset tuning** puts the tuning back. Changes are saved in
`~/.openomsi/Inputs/gamectrler.cfg`, the tuning in a section `[openOMSI.FFTuning]` that OMSI 2
ignores. A running game takes them the next time you drive, or at once with **Reload saved
controllers** in its controller menu. The same rows are in the game under **Controls → Game
controllers**.

## Setup

Where things are.

| Field | What it is |
| --- | --- |
| **OMSI 2 folder** | the folder with `Omsi.exe`, `maps` and `Vehicles`. Empty: the usual places are searched (Steam libraries, Wine bottles) |
| **Game program** | the `openomsi` program the duties start. Empty: the one beside the launcher |

Press **Save**. The page also shows the content folder (mods), the settings and logs folder
(`~/.openomsi`), with buttons to open them. The OMSI 2 folder may be given with quotes, as
`Omsi.exe`, or as a folder inside it. Unpacked into the OMSI 2 folder itself, openOMSI keeps its
own content in an `openOMSI` folder there and never writes to the game's.

On a phone: copy the whole OMSI 2 folder into `openOMSI/OMSI 2` in the internal storage (see
[Android](ANDROID.md)).

## Tutorials

OMSI 2's four lessons, each started in its own situation with its pages beside the picture.
They come from OMSI 2's `Tutorials` folder.

## Troubleshooting page

**Help → Troubleshooting** (not on phones) has two tabs:

- **Overview**: problems by content (packs waiting for their bus, buses missing parts, map depot
  files, failed installs), with a fresh scan. **Disk cache and multiplayer downloads** shows
  their size, with **Clean to limits**, **Clear local cache…** and **Clear multiplayer
  downloads…** (close the games first; mods, saves and settings stay).
- **Model test**: loads a bus, a map, a mod, or any file or folder the way the game does and
  lists every warning. See [Troubleshooting](TROUBLESHOOTING.md#checking-content).

## Profile

The driver whose personnel file the game writes: hours, kilometres, punctuality, tickets,
experience and level, and the recent runs. Create, choose or delete a driver here. The figures
come from OMSI's `.odr` personnel files and the session summaries the game writes to
`~/.openomsi/sessions`.

## Settings

**Settings** holds every setting of `~/.openomsi/settings.cfg`. Every change is saved at once;
the game reads them when it starts (many also change at once in a running game, through its
**Options**). Keys the page does not manage are kept as they are.

- The search box (`Ctrl+F`) searches all settings, their help and their file keys.
- Tabs: Graphics, Driving, Camera, Sound, Gameplay, General, AI Lists (not on phones). The
  *Troubleshooting* tab opens the Model test.
- Rows that do not apply are hidden: Enhanced-only rows in Vanilla, Windows-only rows elsewhere.
- **Heavy** marks a choice that can cost many frames per second.

The tables below give each setting's name in the file in brackets. Phones and tablets start with
lower graphics defaults (2x MSAA, 4x anisotropic, no ambient occlusion, 1024 shadows, 128 mirrors,
60 fps, 900 m objects, 70 % passengers).

## Graphics settings

**Settings → Graphics**, first column. What each mode and option looks like is explained in
[Graphics](GRAPHICS.md).

| Setting | What it does | Default |
| --- | --- | --- |
| Graphics (`graphics`) | Vanilla (as OMSI 2), Vanilla+, Enhanced | Vanilla+ |
| Real-Time Ray Tracing (`ray_tracing`) | Enhanced: traced shadows, occlusion, reflections; next start | off |
| Ray-traced global illumination (`rt_gi`) | experimental, with Real-Time Ray Tracing: light bounced between surfaces | off |
| Quality preset | Low, Medium, High, Extra, Max, Ultra, or Custom | Medium |
| Optimise for my PC | measures your next drive and adjusts the graphics | |
| Anti-aliasing (`msaa`) | Off, 2x, 4x, 8x MSAA | 4x |
| Upscaling (`upscaler`) | Standard (Catmull-Rom), AMD FSR 1, and DLSS / FSR 3 / MetalFX where available | Standard |
| NVIDIA DLSS runtime | Windows with RTX: installs NVIDIA's DLLs after you accept their terms | |
| Upscaling quality (`upscale_quality`) | DLSS / FSR 3 / MetalFX: Native to Ultra performance | Quality |
| Frame generation (`frame_generation`) | MetalFX: a generated frame between two drawn ones | Off |
| FSR quality (`fsr_quality`) | FSR 1: Ultra Quality (77 %) to Performance (50 %), Automatic, or the render scale | Quality (67 %) |
| FSR sharpness (`fsr_sharpness`) | FSR 1's sharpening | 80 % |
| Render scale (`render_scale`) | draw smaller and upscale: Auto, 100 % to 50 % | Auto |
| Anisotropic (`anisotropy`) | texture filtering, Off to 16x | 8x |
| Shadow map (`shadow_size`) | 256 to 4096 | 2048 |
| Ambient occlusion (`ssao`) | not in Vanilla | on |
| Sun shadows (`shadows`) | not in Vanilla | on |
| Shadows cast by (`shadow_casters`) | every solid mesh, or only `[shadow]` meshes as OMSI | every solid mesh |
| Detail texturing up close (`detail_textures`) | fine grain on ground and roads | on |
| Night brightness (`night_brightness`) | Enhanced: lifts the night, Off to +3 | Off |
| LED glow (`led_glow`) | Enhanced: LED destination dots' glow, Off to 15 | 6 |
| LED mip strength (`led_mips`) | Enhanced: LED matrix sharpness vs. shimmer, 0 to 4 | 1.3 |
| OMSI's shadow meshes (`shadow_blobs`) | the flat shadow under a vehicle | on |
| Reflective objects reflect (`reflections`) | strength of the reflection maps, Off to 100 % | 100 % |
| Road glare (`road_glare`) | Enhanced: highlights on roads, 0–200 % | 100 % |
| Traffic headlight brightness (`traffic_headlights`) | other vehicles' headlamps, 0–200 % | 100 % |
| Clouds (`clouds`) | | on |
| Cloud quality (`cloud_quality`) | Enhanced: High or Low | High |
| Street light (`street_light`) | Enhanced: shade and bounced light of a whole street; Auto, On, Off | Off |
| Light has shadows (`light_shadows`) | Enhanced: Off, Street lamps and vehicles, Every light | Off |
| Cloud shadows and light shafts (`cloud_shadows`) | Enhanced | off |
| Glow of lights (`light_glow`) | Enhanced: glare and halos round lights | off |
| Volumetric fog (`volumetric_fog`) | Enhanced: fog you can see light in | on |
| Rain quality (`rain_quality`) | High, Medium, Low | High |
| Vehicle smoke (`vehicle_smoke`) | All vehicles, Player only, Off | All vehicles |
| Puddles / Real-Time Reflections (`puddles`) | wet-road reflections: High, Low, Off | High |
| Windy trees (`windy_trees`) | foliage sways in the wind | on |
| Detailed trees (`detailed_trees`) | no streaky leaves close up, shaded crowns | on |
| Built-up snow on roads and roofs (`road_snow`) | snow builds up, with ruts and tracks | off |

### Display

| Setting | What it does | Default |
| --- | --- | --- |
| Fullscreen (`fullscreen`) | | off |
| Window size (`resolution`) | Automatic, or 1280 x 720 to 3840 x 2160 | Automatic |
| V-sync (`vsync`) | | on |
| Simulation on its own thread (`sim_thread`) | | off |
| Frame limit (`max_fps`) | screen refresh rate, 30 to 144 fps, Unlimited | screen refresh rate |
| Graphics API (`graphics_api`) | Windows: Automatic, Vulkan, DirectX 12, DirectX 11 (ANGLE), OpenGL, ANGLE; Linux/Android: Automatic, Vulkan, OpenGL; not on a Mac | Automatic |
| mX Graphics (Experimental) (`mx_graphics`) | runs the game on mX instead of wgpu; next start | off |

### World and memory

| Setting | What it does | Default |
| --- | --- | --- |
| View distance (`view_distance`) | how far tiles stay loaded, 300 to 2500 m | 1200 m |
| Object distance (`max_obj_dist`) | Automatic, 300 to 3000 m | Automatic |
| Small objects (`min_obj_size`) | All, Normal, Fewer, Few | Normal |
| Mirrors (`mirror_size`) | Off, 128, 256, 512, 1024 | 256 |
| Bus mirror refresh (`mirror_refresh`) | None (frozen), Economical, Full, Every frame | Full |
| Mirror quality (`mirror_quality`) | Enhanced: Standard, or As the picture | Standard |
| Mirror panels every frame (`mirror_hud_every_frame`) | the panels' mirrors drawn every frame | off |
| Shadows in the mirrors (`mirror_shadows`) | | off |
| Smoke and spray in the mirrors (`mirror_particles`) | | off |
| Video memory (`video_memory`) | Automatic (nine tenths of the card), or 1 to 16 GB | Automatic |
| Compress textures on loading (`texture_compression`) | | on |
| DXT/BC textures stay compressed on the GPU (`gpu_texture_compression`) | off: for a driver that mishandles them, at 4–8 times the memory | on |

### Graphics profiles

Save the graphics settings under a name (**Save current graphics as profile**), then **Load** or
**Delete** them. **Import** and **Export** share a profile as a file. A profile holds only the
graphics settings, not the full screen choice or the graphics API.

## Driving settings

**Settings → Driving**, column *Keyboard & mouse*. On phones *Touch controls* come first: **Door
buttons** (Standard / Direct OMSI triggers, default Standard), **Pedal length** (80–160 %,
default 100 %) and **Pedal rest zone** (0–40 %, default 15 %).

| Setting | What it does | Default |
| --- | --- | --- |
| Driving keys (`drive_keys`) | W A S D + arrows, W A S D only, Arrow keys only, Custom controls | W A S D + arrows |
| Steering linearity (`steering_linear`) | keys turn at OMSI's steady pace | off |
| Old Steering (`old_steering`) | the wheel stays where you leave it | off |
| Dynamic steering (`red_steer_spd`) | slower keys at speed | off |
| Mouse steering sensitivity (`mouse_sens`) | 10–300 %, 100 % = OMSI | 100 % |
| Mouse pedal strength (`mouse_pedal_strength`) | cursor travel for full pedal, 50–200 % | 100 % |
| Smooth mouse steering (`mouse_smooth`) | off: the wheel follows the cursor at once | on |
| The mouse works the pedals (`mouse_pedals`) | off: the mouse only steers | on |
| Hold the cursor while the mouse steers (`mouse_hold`) | off: system crosshair, window edges are the lock | on |
| A right click ends the mouse steering (`mouse_right_off`) | as in OMSI | off |
| Indicators cancel themselves (`blinker_cancel`) | as the bus's script does | on |
| The keyboard brake stays on until the throttle (`brake_hold`) | as in OMSI | on |
| Pedal keys speed (`key_pedal_speed`) | 25–200 %, 100 % = OMSI | 100 % |
| Automatic clutch (`auto_clutch`) | manual gearboxes | on |
| Automated manual gearbox (`auto_shift`) | shifts a manual gearbox by the engine speed | off |

**Change the keys** opens the Controls page.

### Wheel, pedals and gamepad

Column *Game controllers*. **Set up a wheel or pedals** opens Controls → Game controllers.

| Setting | What it does | Default |
| --- | --- | --- |
| Wheel rotation (`wheel_range`) | your wheel's rotation lock to lock, 180–1800° | 900° |
| Full lock at (`wheel_lock`) | wheel angle for the bus's full lock; OMSI = the whole wheel | OMSI |
| Cab wheel rotation (`cab_wheel_rotation`) | how far the cab's wheel model turns; Automatic = the bus's own | Automatic |
| Throttle pedal strength (`pedal_throttle`) | 50–200 % | Normal |
| Brake pedal strength (`pedal_brake`) | 50–200 % | Normal |
| H-pattern shifter: return to neutral (`momentary_gears`) | for a shifter with no neutral button | off |
| Arrow keys switch the cameras with a wheel too (`arrows_switch_cams`) | off: with a wheel the arrows glance | on |
| Keep driving in the background (`ctrl_background`) | controllers work while the window is not in front | off |
| Gamepad type (`pad_type`) | Automatic, Xbox, PlayStation 4, PlayStation 5 | Automatic |
| Default gamepad buttons (`pad_buttons`) | indicators, doors, views, gears, menu, pause | on |
| Stick steering speed (`pad_steer_speed`) | 0.8–5 s | 2.0 s |
| Stick steering smoothing (`pad_steer_smooth`) | Off to 300 ms | 120 ms |
| Stick dead zone (`pad_deadzone`) | 0–40 % | 8 % |
| Stick steers like a wheel (`pad_steer_linear`) | for a wheel seen as a gamepad | off |

### Force feedback

| Setting | What it does | Default |
| --- | --- | --- |
| Force feedback and vibration (`ff_enabled`) | off: the game sends the wheel no forces | on |
| Invert force feedback by default (`ff_invert`) | wheels with a saved direction keep their own | off |
| Road texture vibration (`ff_road_vib`) | Off to 400 % | Normal |
| Engine vibration (`ff_engine_vib`) | Off to 400 % | Normal |
| Vibration fade-out (`ff_fade`) | how long a jolt eases away, Off to 1.5 s | 280 ms |

**Reset wheel settings** puts these and the wheel rows back.

## Camera settings

**Settings → Camera**, column *Driver's view*.

| Setting | What it does | Default |
| --- | --- | --- |
| Seat forward / back, up / down, right / left (`seat_y`, `seat_z`, `seat_x`) | moves the driver's eye, ±60 cm | 0 |
| Head pitch (`seat_pitch_deg`) | the neutral view angle, ±45° | 0° |
| Field of view (`fov`; `triple_fov_deg` with three screens) | 20–120°, or Default (the bus's cameras) | Default |
| Mouse look sensitivity (`look_sens`) | 10–200 % | 100 % |
| Right stick turns the view (`right_stick_look`) | off: the gamepad's right stick leaves the camera alone | on |
| Smooth the mouse look (`look_smoothing_ms`) | Off to 200 ms | Off |
| Head sway at a standstill (`head_idle`) | a head at rest breathes and shifts, Off to 100 % | Off |
| Sway pace (`head_idle_pace`) | 50–200 % | 100 % |
| Driver's view turns with the steering (`steer_look`) | the view turns into the curve | off |
| Steering view angle (`steer_look_angle`) | at full lock, 0–60° | 30° |
| Steering view response (`steer_look_response`) | smoothing, 50–1000 ms | 250 ms |
| Head moves with the bus (`head_movement`) | | on |
| Camera glides between viewpoints (`driverview_smooth`) | | on |
| Driver's hands in the cab view (`hands_in_cab`) | | off |
| Driver sits as in OMSI 2 (`driver_like_omsi`) | | off |
| Right mouse button turns the view (`alt_view`) | off: right zooms as in OMSI, the wheel button turns | on |
| Mirror panels on the screen (`mirror_hud`) | first panels for a bus without a layout: Off, Left, Right, Left and right, Left right and front | Off |
| Precision mouse zoom (`precision_zoom`) | FOV curve instead of linear | off |
| Zoom on a key (`zoom_hold_amount`) | how far *Zoom while held* zooms | 2x |
| Zoom key stays (`zoom_hold_latch`) | one press in, the next out | off |
| Flashlight follows the cursor (`flashlight_cursor`) | with the cursor free | off |

**Reset the seat position** puts the seat and pitch back. The seat set in the game menu is kept
per bus in `~/.openomsi/seats.cfg`; a bus never fitted uses the seat of these settings. The
automatic steering turn is suppressed while VR or head tracking controls the view.

### Outside views, head tracking, triple screen, VR

| Setting | What it does | Default |
| --- | --- | --- |
| Camera collisions (`camera_collision`) | outside view | on |
| Driver at the wheel (`driver`) | outside views | on |
| Head tracking (`head_tracking`) | TrackIR / opentrack, natively | off |
| Head tracking with (`head_tracking_source`) | Automatic, Tobii eye tracker, opentrack (UDP 4242) | Automatic |
| Look further at the screen's edges (`tobii_edges`) | Tobii | off |
| Yaw, pitch, roll, X, Y, Z sensitivity | per axis, 0 switches it off | 100 % |
| Invert yaw … Invert Z | per axis | off |
| Three screen projections (`triple_screen`) | | off |
| Span three monitors at startup (`triple_span`) | | on |
| HUD on centre screen (`triple_hud_center`) | | on |
| Visible width of one panel (`triple_width_mm`) | | 600 mm |
| Eye to centre screen (`triple_distance_mm`) | | 650 mm |
| Both frames at each join (`triple_bezel_mm`) | | 0 mm |
| Left / right screen inward angle | | 45° |
| Eye above screen centre (`triple_eye_height_mm`) | | 0 mm |
| Use OpenXR headset (`vr`) | Windows | off |
| Eye resolution (`vr_scale`) | 50–100 % | 65 % |
| Head tracking smoothing (`vr_head_smoothing_ms`) | Off to 30 ms | Off |
| Bus mirror refresh (`vr_mirror_rate`) | Off, 8/s to 360/s, Every frame | 16/s |
| Show headset picture on monitor (`vr_desktop_mirror`) | | on |

**Reset head-tracking axes** sets all six to 15 % and not inverted. Triple screens:
[Triple screen](triple-screen.md). VR: [Virtual reality](VR.md).

## Sound settings

**Settings → Sound**.

| Setting | What it does | Default |
| --- | --- | --- |
| Volume (`volume`) | master volume | 60 % |
| Traffic (`vol_ai`) | AI vehicles | 100 % |
| Surroundings (`vol_scenery`) | scenery sounds | 100 % |
| Doppler effect (`doppler`) | | on |
| Ambience (`ambient`) | places, weather, nature, town | on |
| Ambience volume (`vol_ambient`) | | 80 % |
| Passenger voices (`pax_voices`) | Greetings and tickets, Only the ticket asked for, Silent | Greetings and tickets |
| Radio: small speaker / FM hiss / signal dropouts | see [Radio](RADIO.md) | 0 |
| Voice chat (`voice_enabled`) | multiplayer voice | off |
| Microphone (`voice_mode`) | Push to talk, or Open microphone (nearby) | Push to talk |
| Microphone gain (`voice_mic_gain`) | 0–400 % | 100 % |
| Open microphone: speech threshold (`voice_threshold_db`) | −80 to −10 dB | −45 dB |
| Voices: volume / nearby / radio | 0–200 % | 100 % |
| Nearby voice range when you host (`voice_range`) | 10–300 m | 75 m |
| Show who is talking (`voice_talker_list`) | | on |
| Everybody else's voice chat off (`voice_all_off`) | | off |

The second column, **Radio stations**, edits your stations (see [Radio](RADIO.md)). Voice chat:
[Voice chat](VOICE.md).

## Gameplay settings

**Settings → Gameplay**, column *Passengers*.

| Setting | What it does | Default |
| --- | --- | --- |
| Boarding (`boarding`) | Pay and take the ticket, The driver sells the ticket, Just walk in | Pay and take the ticket |
| Standing passengers (`standing_chance`) | Never, 5 % to Always | 5 % |
| Passenger animations (`passenger_animation`) | Original OMSI, or Natural (procedural, with IK) | Original OMSI |
| Character models (`character_models`) | Original, or High quality (Rocketbox avatars) | Original |
| Passengers pay the exact fare (`exact_fare`) | off: they overpay and need change | on |
| Passengers react to bad driving (`pax_bad_driving`) | complaints, getting off early | on |
| Passenger connections between lines (`passenger_connections`) | onward trips, single player only | off |
| Connection chance (`passenger_transfer_percent`) | 0–100 % | 20 % |
| Passengers signal approaching buses (`passenger_hailing`) | with natural animations | off |
| Passengers react to full buses (`passenger_full_bus_reaction`) | | off |
| Short rides stand by a door (`pax_short_rides_stand`) | | off |
| Arrival announcements at the stop (`auto_announce`) | depot files with two recordings per stop | off |
| How many passengers (`pax_density`) | 0–200 % | 100 % |
| Ability to get up (`get_up`) | `Ctrl+Shift+G` gets up and walks | off |
| Start with a full tank (`full_tank`) | | on |
| Trip summary (`trip_summary`) | | on |

**High quality** character models draw the people with 91
[Microsoft Rocketbox avatars](https://github.com/microsoft/Microsoft-Rocketbox) with living faces:
they blink, look about, smile when they pay and move their lips. It is drawn on your computer
only; where people go, sit and pay does not change, and in multiplayer the others see their own
people. The avatars are downloaded once (about 310 MB, into `~/.openomsi/avatars`); until then
the people stay the content's own.

### Traffic

| Setting | What it does | Default |
| --- | --- | --- |
| Random traffic (`ai_unsched_factor`) | 25–200 % | 100 % |
| AI reaction time (`ai_reaction`) | 25–200 % | 100 % |
| Timetable vehicles (`ai_max_scheduled`) | All, or at most 10, 25, 50 | All |
| Parked cars (`ai_max_parked`) | None, Every space, at most 35, 100, 250 | Every space |
| People at once (`ai_max_humans`) | 50 to 1000 | 200 |
| Timetable buses ahead of time wait only at timed stops (`ai_wait_timed_stops_only`) | off: an early bus waits at every stop, as in OMSI | off |

With this setting on, an early timetable bus waits only at its first and last stop, railway
stations and stops with a time of their own.

### Simulation

OMSI's own options.

| Setting | What it does | Default |
| --- | --- | --- |
| Maintenance (`maintenance`) | Infinite (no wear), Very bad, Bad, Normal, Good | Infinite |
| Collisions with vehicles (`collision_vehicles`) | impacts deform the struck AI vehicle where its model allows | on |
| Collisions with objects (`collision_objects`) | walls, poles, bridges; off is OMSI's `no_collision` | on |
| Collisions with people (`collision_pedestrians`) | | on |
| Realistic road physics (`realistic_road_physics`) | | off |
| Start at the real time (`use_real_time`) | | off |
| Start on today's date (`use_real_date`) | | off |
| Keep today's year (`use_real_year`) | even outside the map's years | off |
| Sync the clock with the real time (`time_sync`) | locks the time; the host's in multiplayer | off |
| Sync the weather with METAR (`metar_sync`) | locks the weather; see [Weather and seasons](WEATHER.md) | off |
| Time speed (`time_speed`) | Real time, x2 to x30; not in multiplayer | Real time |

`collision_objects` is taken from OMSI's own options when openOMSI starts the first time.

## General settings

**Settings → General**, column *Interface & online*.

| Setting | What it does | Default |
| --- | --- | --- |
| Launcher theme | Dark, Light, System; the game follows it | Dark |
| Launcher layout | Big Map, Control Room, Classic+, Classic | Big Map |
| Launcher size | Smallest to Largest | Normal |
| Language (`language`) | the interface's language | English |
| Translate the remaining texts automatically (`machine_translation`) | offline, downloads 620 MB once | off |
| The launcher rests while a game runs (`launcher_rest`) | gives the graphics card to the game | on |
| Discord Rich Presence (`discord_status`) | your status in Discord (Discord must run on this computer) | off |
| Discord: the map / bus / line / next stop / multiplayer / buttons | what Discord shows | on |
| Disable plugins (`disable_plugins`) | next game without Lua or OMSI plugins | on (new installs) |
| Game interface size (`ui_scale`) | 50–200 % | 100 % |
| Interface grows with the window (`ui_scale_window`) | above 1080 lines, up to twice | on |
| Interface opacity (`ui_opacity`) | backgrounds only, texts stay solid; 20–100 % | 85 % |
| Name of the button under the mouse (`tooltips`) | | on |
| Frame rate in the corner (`show_fps`) | | off |
| Performance details (`perf_overlay`) | Off, Compact, Detailed | Off |
| Notes in the top-left corner (`notes`) | and each kind of note | on |
| Chat in online games (`chat`) | | on |
| Chat size (`chat_size`) | 50–300 %; `Ctrl+wheel` over the chat too | 100 % |
| Other players' names above their buses (`name_tags`) | | on |
| Photo mode (`photo_mode`) | Simple, or Advanced (for photographers) | Simple |

OMSI's own texts (such as cockpit switch names) exist in English, German and French only; other
languages show them in English.

**Disable plugins** is on by default for new installations only. Normal bus scripts still run;
plugin features of a vehicle may be missing. An existing settings file keeps its plugins. To use
a trusted plugin, switch it off and start a new drive. `--no-plugins` does the same for one game
started from the command line.

### Navigator

| Setting | What it does | Default |
| --- | --- | --- |
| Navigator (`navigator`) | `Shift+N`: map, schedule, off | on |
| Dynamic route arrows (`nav_dynamic_arrows`) | arrows over the road | off |
| Map-placed route arrows (`nav_map_arrows`) | the map's own arrows | off |
| AI vehicles on the map (`nav_ai`) | | on |
| Cars on the map too (`nav_ai_cars`) | | on |
| Map surface under the roads (`nav_surface`) | water, land, rail from a map package | on |
| Classic 2D navigator (`nav_classic_2d`) | lighter top-down map | off |
| 3D buildings on the map (`nav_3d`) | | on |
| Navigator as a widget (`nav_widget`) | drag it anywhere, size it at its corner | on |
| Navigator size (`nav_size`) | 60–200 % | 100 % |
| Corner (`navigator_corner`) | top left, top right, bottom left, bottom right | bottom left |

### Updates, privacy and support

| Setting | What it does | Default |
| --- | --- | --- |
| Look for updates when the launcher starts (`update_check`) | | on |
| Update channel (`update_channel`) | Stable, or Beta (for testers) | Stable |
| Install updates without asking (`update_auto`) | | off |
| Tell me about a new version during a session (`update_notify`) | | on |
| Count me in the website's "playing now" (`presence`) | anonymous | on |
| Send servers an identifier for bans (`hardware_id`) | different for each server owner | on |

- The version line, **Check now**, and **Choose a version…** (see [Updates](#updates)).
- **Reset all settings…**: graphics, sound, controllers and game settings go back to how they
  came. The language, the drivers, the key bindings and the game folder stay.
- **Support package**: logs, settings and a description of the machine in one zip, for a bug
  report (see [Troubleshooting](TROUBLESHOOTING.md#reporting-a-bug)).

**Playing now.** While you drive, the game tells the project's counter every ten minutes that it
is being played, and says goodbye at the end. It sends a random id made for each session, the
version and the kind of system, nothing else; no addresses are kept. A dedicated server is never
counted.

## AI Lists

**Settings → AI Lists** (not on phones) chooses which AI cars, lorries and buses a map's traffic
and depots are made of, and which buses run on which line.

- **Traffic**: the map's groups of AI vehicles. Add, replace or remove vehicles, and set how
  often each appears.
- **Depot buses**: the buses of each depot; the timetable draws its buses from here in
  proportion. Fleet numbers and plates can be set.
- **Lines**: per line, *As the map has it*, *Only these types* or *Mostly these types*, and the
  tours that get them. The line keeps its tours and times; only the bus changes.

Every choice comes from the installed vehicles. Changes are saved beside the map in the content
folder as the files OMSI reads; the map's own files stay. **Reset the map** deletes your changes.

## Settings only in the file

These keys of `settings.cfg` have no row on the Settings page. Some are switched in the game's
menus.

| Key | What it does | Default |
| --- | --- | --- |
| `post_aa` | Enhanced's edge smoothing: `fxaa` or `off` | `fxaa` |
| `lamp_shadows` | Enhanced: street lamp and headlamp shadow maps | on |
| `rain_lights` | in a downpour some house windows light up by day | on |
| `units` | units of the game's texts: `metric`, `uk`, `us` | `metric` |
| `info_bar` | OMSI's information bar (`Shift+Y`), as last left | off |
| `mouse_steering` | mouse steering as last left (`O`) | off |
| `milezero` | the MileZero radio card over the navigator | off |
| `pax_no_ticket` | the fare is paid without a ticket | off |
| `pax_late_tolerance_min` | minutes late before boarders complain; 0 off | 5 |
| `pax_free_drive` | stops fill in free drive: `auto`, `on`, `off` | `auto` |
| `pax_board_when_stopped` | people walk to the bus only once it stands with a door open | off |
| `pax_stop_numbers_exact` | a stop's passenger numbers as the map writes them | off |
| `parked_buses_drawn` | a server's parked buses drawn at most; −1 automatic | −1 |
| `ctrl_deadzone` | dead zone of set-up controller axes (0–0.3) | 0 |
| `ctrl_off` | controllers switched off, by name, separated by `\|` | empty |
| `head_tracking_port` | opentrack's UDP port | 4242 |
| `head_tracking_smoothing`, `head_tracking_dead_zone` | Tobii tracker smoothing (0–1) and dead zone (degrees) | 0.5, 0.1 |
| `metar_station` | ICAO code for the METAR sync; empty: nearest | empty |
| `pause_quick_actions` | the quick menu's actions | `default` |
| `ui_theme_pack` | interface theme, see [Interface themes](THEMES.md) | `openomsi` |
| `remote_cockpit`, `remote_cockpit_port` | the remote cockpit on a phone or tablet | off, 8642 |
| `version` | the file's version | |

## Updates

When the launcher starts, it asks GitHub for the latest release of your channel. When there is a
newer one, a dialog offers it.

- **Update now** downloads it, checks it against the SHA-256 GitHub lists, puts it in place and
  restarts the launcher. Mods, content and settings stay.
- A broken download resumes where it stopped (up to four times). A failed request is retried
  after 3 and 10 s. When the GitHub API does not answer, the version is read from github.com.
- While open, the launcher looks again every 30 minutes, and shortly after a game ends.
- During a drive the game looks a minute in and every hour. A newer version is downloaded in the
  background and a card over the navigator says so. The launcher installs it when the session
  ends (by itself with *Install updates without asking*, else it asks), never while a game runs.
- A folder openOMSI cannot write to (Program Files, an app opened straight from Downloads on
  macOS) is reported with what to do.
- On Android the system's installer asks; **Update** replaces openOMSI and starts it again.

**Choose a version…** (Settings → General) lists the releases. Installing an older one turns the
update check off so that it stays. Versions that did not start on this computer, or are known to
be broken, are marked. Any release can also be downloaded by hand from
[GitHub](https://github.com/openOMSI-org/openOMSI/releases/latest).

What happens when a new version does not start: [Troubleshooting](TROUBLESHOOTING.md#a-version-does-not-start).
