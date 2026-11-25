# Trolleybuses

openOMSI has its own trolleybus simulation: a real two-wire overhead line on the map, and real
poles and a real electric drive on the vehicle. This page is the reference for map makers and
vehicle modders. It describes two small text formats:

* the **wire** of a map: `trolley.txt` next to its `global.cfg`;
* the **gear** of a vehicle: `[trolley_pole]` blocks and a drive in its `.bus`.

OMSI 2 has no overhead wire of its own. Its trolleybus add-ons carry a plugin DLL that reads the
bus's position from Omsi.exe's memory and moves fake poles over wires that are only meshes. That
cannot run in openOMSI. Existing add-ons still work: a map's wire splines are read as the wire
when there is no `trolley.txt`, and a bus with `cp_SHTANGALEV` / `cp_SHTANGAPRAV` pole meshes gets
poles without a `[trolley_pole]` block. How that works is in [Real trolleybus add-ons](TROLLEY_MODS.md).

For driving a trolleybus as a player, see [Driving](DRIVING.md).

## Quick start

### A wire for a map

Save this as `trolley.txt` next to the map's `global.cfg`. It is one straight pair of wires,
150 m long, 6 m up, fed at 600 V.

```text
[trolley_net]
voltage 600
spacing 0.6

[trolley_span]
p 0 0 6.0
p 150 0 6.0
```

### Poles and a drive for a bus

Add these blocks to the bus's `.bus` file. Every key you leave out keeps its default.

```text
[trolley_pole]
side left
pos -0.3 -1.0 3.2
length 6.6
motor 1

[trolley_pole]
side right
pos 0.3 -1.0 3.2
length 6.6
motor 1

[trolley_rheostat]
```

The bus now has two poles that its motor can raise from the cab, and a ZiU-style rheostat drive.
With **Ctrl+R** in the cab the poles go up on the wire.

## How it works

### The wires

* Two wires run a fixed distance apart, one of each polarity.
* The bus has power only while one pole is on the positive wire, the other on the negative wire,
  and both lie on wire a substation feeds.
* A section can be dead. Between sections there is an insulator: a dead stretch, 0.8 m by
  default.
* The voltage at the bus is the substation's, less what the feeder drops with the section's
  current. Other trolleybuses on the same section count.

### The poles

Each pole is a boom hinged on the roof. It has an elevation and a swing, a spring that presses
the shoe up against the wire (120 N at the working angle by default), its own mass and inertia,
and a shoe whose groove holds the wire.

On the wire, the spring must accelerate the boom along every rise of the wire. Where the wire
rises faster than the boom can follow (a hanger, a kink, unevenness at speed), the shoe leaves it
for a moment. The contact opens, and an arc strikes if current flows.

### Dewiring

A pole **comes off** (dewires) when:

* the wire lifts out of the shoe's groove (a bounce);
* the boom stands too far off the wire's direction seen from above: 40° by default, about 4 m off
  the line for a 6.5 m boom. Leaving the wire's line or a wrong switch position does this;
* a bend of the wire is taken too fast. The shoe cannot turn quicker than its sideways speed
  allows (`kink_speed`, 3 m/s): a 25° bend at 11 m/s is too much, at 3 m/s it is not;
* the wire is out of the boom's reach, or ends under the shoe.

A pole that has come off springs up to its stop and swings with the bus until its rope holds it.
A pole with a **rope retriever** pulls itself down to its hook half a second later.

### Power

Without a circuit the bus does not move:

* A **modelled drive** (`[trolley_rheostat]` or `[trolley_async]`) loses its force at once. The
  bus rolls on, slows under its rolling resistance, and stops.
* A **scripted drive** (the bus's own script, no drive block) has its driving torque switched
  off. A torque that brakes still works. On the wire, the torque is scaled by the line voltage
  over the nominal voltage.

So a mod's scripted drive works on a wire and stops without one.

### Rheostat drive

`[trolley_rheostat]` is a series-wound DC motor with a resistor controller (ZiU style):

* The controller steps the resistors out (14 steps), then the field in (two weakening steps),
  while the current stays under what the pedal asks for.
* The resistors turn what they take into heat, and the contactors click.
* Dynamic braking excites the motors from the line and drives a resistor. Without the line, only
  the mechanical brakes work.

### Asynchronous drive

`[trolley_async]` is an inverter drive:

* A force limit up to its corner speed, a power limit above it.
* The force fades as the voltage sags. A deep sag trips the inverter.
* Regenerative braking gives power back to the section, as far as the feeder takes it
  (`regen_kw` of the section plus what other buses draw). A brake resistor on the roof burns the
  rest.
* An optional **battery** (`[trolley_battery]`) moves the bus slowly off the wire (15 km/h,
  30 kW by default) and charges from the line.

### Arcs

Where current is broken, an arc burns `L I² / 2` of the motor circuit's inductance (12 mH). The
game draws a flash with sparks, lights the wire and the roof, and plays a crack. That happens
when:

* a shoe leaves the wire, or a pole comes off, under load;
* an insulator or a frog is crossed under power.

Coasting through an insulator strikes none. Power on through it, a loud one. A frog or insulator
crossed with more than 3 A arcs every time by default (a coasting bus's auxiliaries often draw
more). A map can make those arcs rarer, weaker or need more current with the `frog_arc_*` and
`insulator_arc_*` keys of `[trolley_net]`. The chance is drawn per frog crossing and per
insulator entry and exit, from the vehicle's seed, so it is repeatable. Arcs of a shoe leaving
the wire or a pole coming off are not affected by these keys.

## Driving a trolleybus

### Keys

| Key | Where | Does |
| --- | --- | --- |
| **R** (hold) | Outside, near the back of the bus | Take the rope of the nearest pole. It rises to just under the wire you look at. Let go and its spring puts it on |
| **T** | The same | Pull the nearest pole down: the shoe off the wire, the boom on its hook |
| **Shift+R** / **Shift+T** | The same | Both poles |
| **Ctrl+R** | Anywhere, the cab too | Both poles on the wire by themselves, or down if one is on. Not when the keyboard file or the bus uses Ctrl+R |

"Outside" means on foot, in the outside camera or in the free camera. Get out on foot with
`Ctrl+Shift+G` (the `get_up` setting). The outside camera is **F3**.

In the cab, R and T belong to the gearbox. There the poles answer the cab's own switch and
Ctrl+R. In the cab only poles with a motor (`motor 1`) answer, and they find the wire by
themselves.

The game's auto-start (**Shift+U**) also puts the poles on the wire, when each has a wire within
reach.

### Putting a pole on by hand

1. Walk to the back of the bus, or use the outside or free camera.
2. Hold **R**. The rope of the nearest pole is in your hand, and the boom follows where you look
   (the wire it meets within 1.4 m of the cross-hairs).
3. Let go of **R**. The spring lifts the shoe into the wire, if you aimed true. The shoe's horns
   catch a wire within 12 cm by default (`funnel`).

A hand is not steady: a boom aimed from the ground wanders a few centimetres.

### With the mouse

**In the cab**, a click on the cab's pole switch (`cp_SHTANGALEV`, `cp_SHTANGAPRAV`, or what the
model calls it) has the motor raise that pole to the wire nearest its tip. The hand lets go when
it is up. One pole at a time. **T** still pulls a pole down by its rope.

**Outside**, a click on a pole takes its rope in hand where it is:

* moving the mouse up lets it rise, down pulls it down, sideways swings it;
* the next click lets go, and the spring lifts the shoe into the wire just above it, or loose
  past it;
* pushed up through a wire, the shoe takes it and the hand lets go;
* let go a little over its hook, the pole catches on the hook;
* a pole on the wire is taken off the same way, with an arc under current.

A plain trolleybus draws no boom of its own. Its model's raised pole mesh is turned about its
hinge by `Trolley_Pole<k>_Elev` and `_Azim`. The cab's switch reads 0 with the pole on its hook
(the folded mesh) and 1 otherwise.

### A model that holds the ropes itself

A model can handle its ropes with its own `[mouseevent]`s on the boom and the rope, as OMSI 2
trolleybuses do. Its script declares and writes, for each pole k:

| Variable | Meaning |
| --- | --- |
| `Trolley_Pole<k>_Rope` | 1 the rope is held (its mouse event pressed), -1 pulled down (a retriever), 0 let go |
| `Trolley_Pole<k>_Rope_X`, `_Rope_Y` | The mouse movement while held, in pixels: `(L.S.mouse_x)` / `_y` of its `_drag`, added up. The game reads them each step and sets them to 0 |

A pole with these variables takes no click of the game's own. The hand works as above, held
while the mouse button is.

## The wire: trolley.txt

### Where the file goes

* `trolley.txt` in the folder of the map's `global.cfg`.
* Another content root (a mod) may hold `maps/<name>/trolley.txt` for a map that has none.
* Without a file, the map's wire splines are read as the wire (see
  [Real trolleybus add-ons](TROLLEY_MODS.md)).

### Format

* Lines are `key value`, grouped in `[block]`s.
* `//` starts a comment.
* Coordinates are the map's: x east, y north, z up, in metres.

A full example:

```text
[trolley_net]
voltage 600
spacing 0.6
roughness 0.0012
frog_arc_chance 1
frog_arc_scale 1
frog_arc_min_amps 3
insulator_arc_chance 1
insulator_arc_scale 1
insulator_arc_min_amps 3

[trolley_section]
id 1
name Substation A
live 1
regen_kw 0
feeder_ohm 0.05

[trolley_span]          // the centre line of the pair, as many points as it has bends
section 1
p 395.0 -185.4 6.10
p 392.0 -185.4 6.05
p 262.0 -185.4 6.10

[trolley_switch]
name Seeburger junction
pos 262 -185.4 6.1
mode radio
initial straight
route 158 branch
channel 2
range 150

[trolley_insulator]
pos 300 -185.4 6.1
length 1.0

[trolley_sounds]
frog sounds\frog.wav
switch sounds\switch.wav
```

### [trolley_net]

| Key | Meaning | Default |
| --- | --- | --- |
| `voltage` | Nominal line voltage (V, 100 to 3000) | 600 |
| `spacing` | Distance between the two wires (m, 0.2 to 2) | 0.6 |
| `roughness` | Unevenness of the contact wire (m). Worn or icy wire: 0.003 and more | 0.0012 |
| `frog_arc_chance` | Chance (0 to 1) that a frog crossed under current arcs. 0: never | 1 |
| `frog_arc_scale` | Multiplier of a frog arc's energy: its flash and crack (0 to 10) | 1 |
| `frog_arc_min_amps` | Least current (A) a frog crossing arcs with | 3 |
| `insulator_arc_chance`, `_scale`, `_min_amps` | The same, for each entry into and exit from an insulator | 1, 1, 3 |

### [trolley_section]

A stretch fed by one substation. Without any section, the whole wire is one live section.

| Key | Meaning | Default |
| --- | --- | --- |
| `id` | Number spans refer to | 1, 2, 3... in file order |
| `name` | Display name | - |
| `live` | 0: no substation feeds it | 1 |
| `regen_kw` | Power its feeder takes back from a braking bus (kW) | 0 |
| `feeder_ohm` | Feeder resistance (Ω, 0 to 5) | 0.05 |

### [trolley_span]

The centre line of the pair of wires.

| Key | Meaning |
| --- | --- |
| `section` | The section id |
| `spacing` | This span's wire spacing, overriding the net's |
| `p x y z` | A point. Give as many as the wire has bends and hangers |

Write the points in the direction that has the positive wire on the right. The sag between
hangers is in the points.

### [trolley_insulator]

| Key | Meaning | Default |
| --- | --- | --- |
| `pos` | `x y z` of a joint of two spans (within 2 m) | - |
| `length` | Length of the dead stretch (m) | 0.8 |

### [trolley_sounds]

The overhead line's own sounds. See [Sounds](#sounds).

| Key | Value | Default |
| --- | --- | --- |
| `frog` | `on`, `off`, or a sound file next to `trolley.txt` | `on` |
| `switch` | The same | `off` |

### How spans join

* Span ends within 1.2 m of each other are joined into one point.
* Where the sections of two joined spans differ, there is an insulator (0.8 m) unless a
  `[trolley_insulator]` gives its length.
* A switch needs a joint of exactly three spans. Which span is the toe (the single side), and
  which leaves straight or to the side, is worked out from their directions.
* A pole follows the wire through joints and switches, from span to span.
* Two wires that cross without sharing a joint are not connected.

### What the game draws

The masts and hangers are the map's own objects. The game draws the two wires (8.5 mm thick) and
nothing else, whatever vehicle is driven: a bus or a car sees the wire too.

With `trolley.txt`, switches get no frog drawn over them. A map without `trolley.txt`, whose wire
is read from its splines, draws that wire itself, and its switches get a generated body, tongue
and lamp.

## Wire switches

A `[trolley_switch]` is a frog where three spans meet. Its `pos` must be within 3 m of such a
joint. Otherwise the log warns that it "has no joint of three spans".

| Key | Meaning | Default |
| --- | --- | --- |
| `name` | Name, for displays and triggers | - |
| `pos` | `x y z` of the joint | - |
| `mode` | `power`, `indicator` (or `blinker`), `radio`, `fixed` (or `manual`) | `power` |
| `trigger` | Power/indicator: contact plate distance before the frog (m, 0.5 to 60) | 9 |
| `sense` | The current that counts as power on (A) | 12 |
| `invert` | 1: power on goes straight, coasting takes the branch | 0 |
| `initial` | `straight`, `branch` or `random` | `straight` |
| `route` | Radio: `<code> <way>`, way is `straight`, `branch`, `left` or `right`. Repeatable | - |
| `channel` | Radio: only vehicles sending on this channel set it | any |
| `range` | Radio: reception radius (m, 1 to 1000) | 150 |

### Power and indicator modes

A contact plate lies `trigger` metres before the frog. When a shoe passes it:

* **power**: the current the bus draws decides. Power on takes the branch, coasting goes straight
  (`invert 1` reverses it);
* **indicator**: the bus's indicator on the branch's side takes the branch.

After a change the switch is locked for 2.5 s.

Taking a switch the wrong way needs no rule. The shoe follows the branch while the bus goes
straight on, and the pole comes off when the boom stands too far off the wire. A shoe entering a
frog from either branch always continues on the single wire.

### Radio mode

A radio switch listens to vehicles within its `range`:

* The cab's left or right request (`Trolley_Switch_Request`) takes that way.
* Without a request, the vehicle's route code (`Trolley_Switch_Code`) is looked up in the
  switch's `route` lines.
* With a `channel`, the switch hears only vehicles sending on it (`Trolley_Switch_Channel`).
* With nothing it hears, it keeps its position.

Radio switches never read a contact plate and have no 2.5 s lock. A new matching request changes
the switch, and its display state, on the next simulation step. They also receive with the poles
down.

**Range.** The range is a circle around the frog, measured from the vehicle's trolley reference
position, not along the wire. There is no heading or approach filter: every radio switch in
range on a matching channel can respond, even on a neighbouring road. Use channels to separate
nearby receivers. Range is reception coverage, not a model of radio propagation or a safety
interlock.

**When a vehicle sends:**

* After its first step, a vehicle sends only when its direction request or route code changes,
  not continuously.
* Releasing a request to `0` sends nothing. A chosen switch is not handed back to the route code.
* A route code other than `0` is also sent to each receiver as the vehicle enters its range, so
  the switches on a route set themselves as the bus arrives. Direction requests are never sent
  on entry.
* Loading a vehicle, or changing only the channel, sends nothing.
* A direction chosen outside range is not queued for later.

**Which transmission wins.** The last matching transmission sets the switch. Another bus's stale
selection does not undo it. An unchanged position does not reset the display's change timer.
Leaving the circle, sending an unknown route, or choosing another channel leaves the frog as it
was.

**Momentary toggle button.** A button can send `Trolley_Switch_Request = 2` while pressed and `0`
when released:

* Each press toggles the actual position of every matching receiver, not a remembered cab
  selection.
* Holding does not repeat. Entering range, changing the channel or changing the route while held
  does not toggle. Release and press again to toggle again.
* A request already held when the vehicle is first stepped is not a press.

For manual-only radio control, keep the route code at `0` and use a momentary toggle button. The
driver reads the actual position from the roadside switch display.

### initial random

A switch with `initial random` takes a way at random when the game first reads the map's wire.

* The game keeps that network until it quits. Placing a vehicle again, or reloading the same map,
  keeps the positions the switches have by then. A new draw needs a restart.
* In an online game (serving, hosting or joining) switch positions are not synchronized, so
  `initial random` switches start straight there, the same for everyone.
* A room joined later on a map already driven in the same run keeps that run's network and its
  positions.

## Switch displays

A map can use a scripted scenery object as a switch display. Declare these variables in the
object's `[varnamelist]`. The engine sets them before its script runs.

| Variable | Value |
| --- | --- |
| `Trolley_Frog_State` | `0` no matching switch, `1` set to the left leg, `2` set to the right leg (seen from the toe) |
| `Trolley_Frog_Straight` | `1` if the selected leg is the straight way, else `0` |
| `Trolley_Frog_Changed` | Simulation seconds since the switch last changed. `1000000` at first, or without a switch |

* The object must declare `Trolley_Frog_State` to take part.
* A placed object's string that matches a `[trolley_switch]` name selects that switch (no case,
  spaces around ignored).
* Otherwise it shows the nearest switch within 40 m horizontally. Name displays explicitly at
  closely spaced junctions.

The values show the actual switch position, not the cab's request. They work for radio,
indicator, power and fixed switches, and can drive `[visible]`, `[matl_change]` or a script.
A left-bar mesh:

```text
[visible]
Trolley_Frog_State
1
```

The right-bar mesh uses `2`. The map supplies the housing, symbols and any blinking. The engine
generates no display model.

* The timer advances once per scenery step, however many vehicles there are.
* Repeating a request, or a request rejected during the lock, does not reset it.
* The network is available once a vehicle has loaded the map's wire, and stays after that
  vehicle is removed.
* Existing add-on signs that predict a request keep their own behaviour until their scripts use
  these variables.

## Frog and crossing triggers

A bus script can react to its poles passing frogs and crossings. The triggers run before the
step's `{frame}`, so what they write is read in the same frame.

### Frogs

When a shoe goes through a frog, the game fires:

| Trigger | When |
| --- | --- |
| `Trolley_Pole<k>_Frog` | That pole crossed a frog |
| `Trolley_Frog` | Once per frog, whichever pole crossed it |
| `Trolley_Frog_<name>` | Once for that named frog (spaces in the name become `_`) |

The poles are apart, so they cross a frog at different moments. `Trolley_Frog` and
`Trolley_Frog_<name>` still fire once per frog.

```text
{trigger:Trolley_Frog_Junction}
(L.L.crossed) 1 + (S.L.crossed)
{end}
```

### Crossings

Where two wires cross without joining (an insulated overhead crossing), the shoe goes over:

| Trigger | When |
| --- | --- |
| `Trolley_Pole<k>_Joint` | That pole crossed a conductor |
| `Trolley_Joint` | A conductor was crossed, whichever pole |
| `Trolley_Crossing` | Once per physical crossing |

Each conductor intersection fires `Trolley_Joint` and the pole's own trigger once. At a
perpendicular crossing each of the two wires on one span crosses both on the other: four
intersections. So `Trolley_Joint` fires **four times** and each pole's trigger **twice** for one
physical crossing. Use `Trolley_Crossing` to react once.

* This includes the inner conductor crossing between a frog's straight and branch legs.
* The joined ends at a frog fire no joint triggers. `Trolley_Frog` reports the frog.
* Either side triggers independently. Polarity does not matter.
* A wire that passes above another flies over it and fires nothing.

## The gear: vehicle blocks

The gear goes in the vehicle's `.bus`. Inside a block each line is `key value`, `//` starts a
comment, and keys left out keep their defaults. Values outside the allowed range are clamped.

### [trolley_pole]

One block per pole.

```text
[trolley_pole]
side left
pos -0.3 1.0 4.16
length 6.7
rest_angle -1.5
work_angle 16
motor 1
retriever 35
```

| Key | Meaning | Default |
| --- | --- | --- |
| `side` | `left` / `right` (or `-1` / `1`): the wire it runs on, seen from the driver | `right` |
| `pos` (or `base`) | Hinge on the roof: x right, y forward, z up (m) | `0.35 -1.0 3.0` |
| `length` | Boom length, hinge to the shoe's groove (m, 2 to 12) | 6.6 |
| `rest_angle` | Elevation lying on the hook (°, -10 to 30) | 2.5 |
| `work_angle` | Elevation in work, where `tip_force` is rated (°, 5 to 70) | 26 |
| `max_angle` | Highest it rises loose, held by its rope (°, 10 to 80) | 45 |
| `tip_force` | What the shoe presses the wire with at `work_angle` (N, 20 to 600) | 120 |
| `mass` | Boom with rope (kg) | 16 |
| `head_mass` | Head and shoe (kg) | 3 |
| `groove_depth` | How deep the wire sits in the shoe (m) | 0.06 |
| `funnel` | Half the width the horns catch a wire in (m) | 0.12 |
| `max_wire_angle` | How far the boom may stand off the wire, seen from above (°) | 40 |
| `kink_speed` | Sideways shoe speed a bend may ask for (m/s) | 3.0 |
| `retriever` | A rope retriever pulls it down after dewiring at this rate (°/s). `yes` means 35 | 0 (none) |
| `motor` | Raised and lowered from the cab | `0` |
| `boom_bounce` | The boom's bounce on a script's lifting gear (Hz) | 1.1 |
| `boom_damping` | How quickly that bounce dies away (damping ratio) | 0.12 |
| `visible` | The game draws the boom and rope. `0`: the model draws its own | `1` |
| `reach` | How far from the rope's handle it can be handled (m) | 9 |

### [trolley_rheostat]

A series DC drive with a resistor controller. All keys are optional.

| Key | Meaning | Default |
| --- | --- | --- |
| `gear_ratio` | Motor to wheels | 9.6 |
| `gear_efficiency` | | 0.93 |
| `motors` | Motors in parallel (1 to 8) | 1 |
| `armature_ohm` | Armature and series field (Ω) | 0.14 |
| `flux_sat` | Flux linkage at saturation (V·s/rad) | 3.3 |
| `flux_half_amps` | Current at which half of it is built up (A) | 25 |
| `steps` | Resistor steps (2 to 60) | 14 |
| `start_ohm` | The whole external resistance (Ω) | 2.6 |
| `field_steps` | Flux kept in each field-weakening step | `0.72 0.5` |
| `step_up_s` | Seconds per step while the controller climbs | 0.22 |
| `step_down_s` | Seconds per step while it falls | 0.08 |
| `current_min` | The pedal's current at its low end (A per motor) | 120 |
| `current_max` | The pedal's current at its high end (A per motor) | 310 |
| `brake_current` | Dynamic braking: largest generator current (A). 0: none | 220 |
| `brake_excitation` | Its excitation, as a share of saturation flux | 0.75 |
| `brake_force` | Mechanical brakes with the pedal down (N) | 56000 |
| `overspeed` | Motor speed limit (rad/s) | 380 |

### [trolley_async]

An inverter drive.

| Key | Meaning | Default |
| --- | --- | --- |
| `max_force` | Force at the wheels from standstill (N) | 30000 |
| `max_power_kw` | Power at the wheels above the corner speed | 170 |
| `efficiency` | Motor and inverter together | 0.9 |
| `regen_power_kw` | Most regenerative power | 150 |
| `regen_force` | Largest regenerative braking force (N) | 26000 |
| `chopper` | A brake resistor takes what the line will not | `1` |
| `v_min` | Voltage below which the force fades (V) | 480 |
| `v_trip` | Voltage below which the inverter trips (V) | 380 |
| `v_regen_max` | Regeneration stops above this voltage (V) | 720 |
| `force_rate` | How fast the force follows the pedal (N/s) | 60000 |
| `brake_force` | Mechanical brakes with the pedal down (N) | 56000 |

### [trolley_battery]

Optional, for the asynchronous drive.

| Key | Meaning | Default |
| --- | --- | --- |
| `kwh` | Capacity | 4 |
| `kw` | Most it drives with (kW at the wheels) | 30 |
| `max_kmh` | Speed it works up to | 15 |

### [trolley_aux]

One number on the next line: lights, compressor and heater, taken from the line (kW). Default 0.

```text
[trolley_aux]
6.0
```

### [trolley_vars]

Names of the script variables the gear reads, when the model uses different ones. A name the
vehicle's variable list does not have is left alone.

| Key | Default variable | Meaning |
| --- | --- | --- |
| `direction` | `Trolley_Direction` | > 0.5 forward, < -0.5 reverse, else neutral. Without it: forward |
| `indicator_left` | `Blinker_Left` | For switches in `indicator` mode |
| `indicator_right` | `Blinker_Right` | The same |
| `raise_request` | `Trolley_Poles_Raise` | +1 raise, -1 lower the poles that have a motor |
| `switch_request` | `Trolley_Switch_Request` | Radio: -1 left, +1 right, 2 momentary toggle, 0 none. Without it the indicator is sent |
| `switch_code` | `Trolley_Switch_Code` | Radio: the route code, 0 none |
| `switch_channel` | `Trolley_Switch_Channel` | Radio: the channel sent on |

### [trolley_none]

A vehicle whose own script or plugin moves its poles puts an empty block in its `.bus`:

```text
[trolley_none]
```

The game then gives it no gear, and does not read poles from its model either. The meshes keep
their own `[visible]` and animations.

Without any trolley block, a vehicle is taken as a plain trolleybus when its model looks like one
(see [Real trolleybus add-ons](TROLLEY_MODS.md)).

### Articulated buses

Each section of an articulated vehicle has its own `.bus`, read on its own. Put the blocks in the
`.bus` of the section whose model carries the poles (usually the rear one, as on the Ikarus
280T).

### Drive and the bus script

With a drive block:

* the vehicle's `M_Wheel` is replaced by the drive's force times the driven axle's wheel radius;
* the drive's mechanical brake force is added to `Brakeforce`.

The vehicle's variable list must declare `Brakeforce`, `M_Wheel`, `Throttle` and `Brake`, as
every vehicle's does.

Without a drive block, the script's driving torque is kept, scaled by line voltage over nominal
voltage, and cut when the poles bring no current.

## Script variables

The game writes these every physics step, declared in the model or not. A model may animate or
make sound with them.

| Variable | Meaning |
| --- | --- |
| `Trolley_Pole<k>_Elev`, `_Azim` | Pole k (1, 2...): elevation over the horizontal and swing from straight back (°, + to the right) |
| `Trolley_Pole<k>_State` | 0 down on the hook, 1 being pulled down, 2 in the hand, 3 loose, 4 on the wire with the contact open, 5 on the wire |
| `Trolley_Pole<k>_Contact` | 1 while the shoe touches the wire |
| `Trolley_Pole<k>_Force` | What the shoe presses with (N) |
| `Trolley_Pole<k>_Bounce` | 0 to 1: how hard the boom bounces on its lifting gear |
| `Trolley_Pole<k>_Hit` | 0 to 1: how hard its last hit was (on the wire, back on it, against its rope) |
| `Trolley_Pole<k>_Shoe_Yaw`, `_Shoe_Pitch` | The head's swivel against the boom (°), so the shoe lies along the wire. Off the wire it swings back to 0 |
| `Trolley_Volts`, `Trolley_Amps` | Voltage at the bus and current from the line (negative: given back) |
| `Trolley_Powered` | 1 while both poles are on live wire |
| `Trolley_Notch` | The rheostat controller's position |
| `Trolley_Arc` | 0 to 1: how hard an arc burns now |
| `Trolley_Hiss` | 0 to 1: the shoes' hiss |
| `Trolley_Battery` | Battery charge, 0 to 1 |

### A model with its own poles

Put `visible 0` in each `[trolley_pole]` and rotate the pole meshes by `Trolley_Pole<k>_Elev` and
`_Azim`. The head (the shoe and what swivels with it) turns on top of that by `_Shoe_Yaw`, then
`_Shoe_Pitch`, about the head's pivot. `_Shoe_Yaw` turns about the boom's up axis (+ to the
right), `_Shoe_Pitch` about its side axis (+ far end up). Example: a boom at 15° under a level
wire has `_Shoe_Pitch` -15, so the shoe lies level on the wire.

### A script's lifting gear

A script can run its poles' lifting gear itself, as an OMSI 2 door script runs its doors. It
declares and writes `Trolley_Pole<k>_Gear`, the position of the gear's carriage:

* `0`: on the hook.
* `1`: just under the wire (the wire above it, else where the pole left the wire, else its
  `work_angle`).

The boom rides on the carriage as a spring (`boom_bounce`, `boom_damping`), so it bounces where
the carriage stops or starts. At 1 the gear lets it go, and its spring puts the shoe on the wire.
Going below 1 from the wire takes the shoe off. At 0 the hook catches the boom once it is down.

The script times the run as it likes (a delay, a stop halfway, its speeds) and fires the run's
sound itself, with its own trigger.

* The pole follows the variable from its first change until the run ends (on the wire or the
  hook).
* A variable left unchanged moves nothing, so a hand or the cab can still move the pole.
* A pole taken by hand leaves the script's run.
* Without the variable, `motor 1` works as before.

```text
'raise from the hook: 2 s to halfway, 3 s there, 2 s on
(L.L.gear_t) (L.S.Timegap) + (S.L.gear_t)
(L.L.gear_t) 2 < {if} (L.L.gear_t) 4 / {else} (L.L.gear_t) 5 < {if} 0.5 {else} (L.L.gear_t) 5 - 4 / 0.5 + 1 min {endif} {endif}
(S.L.Trolley_Pole1_Gear) (S.L.Trolley_Pole2_Gear)
```

## Sounds

The gear makes its own sounds from noise and decaying tones, with no files. A vehicle or a map
can replace them.

| Sound | When | Replaced by |
| --- | --- | --- |
| Hiss (loop) | Shoes sliding on the wire, louder with current and speed | Vehicle: a loop on `Trolley_Hiss` |
| Arc crack | Current broken (see [Arcs](#arcs)) | Vehicle: `ev_trolleyarc` |
| Frog clatter | A shoe going through a frog | Vehicle: `ev_trolleyfrog`; map: `frog` |
| Contactor click | The rheostat controller stepping | Vehicle: `ev_trolleycontactor` |
| Thud | A shoe landing on the wire or coming off | Vehicle: `ev_trolleyhitup`, `wirehit`, `wireoff`, `dewire`, `hitmax` |
| Hook | A pole's hook letting go or catching it | Vehicle: `ev_trolleyhookopen`, `ev_trolleyhitdown` |
| Switch mechanism | The selected leg actually changes | Map: `switch` |

Which plays: the vehicle's `sound.cfg` entry, else the map's choice, else the game's own.

### Pole sound triggers

The game fires these for what happens to a pole, whoever moves it (a hand, a lifting gear, a
bounce). A pole's own trigger ends in `_<k>`, numbered from 1 as its `Trolley_Pole<k>`
variables. The same trigger without the number plays for either pole (once, when both do it in
the same frame).

| Sound trigger | When | What to record |
| --- | --- | --- |
| `ev_trolleyhookopen_<k>` | The pole is taken off its hook by hand | The hook opening |
| `ev_trolleyhitup_<k>` | Its shoe lands on the wire | The hit |
| `ev_trolleywireoff_<k>` | Its shoe is taken off the wire by the rope | The shoe leaving |
| `ev_trolleyhitdown_<k>` | It comes down on the roof and the hook catches it | The hit and the lock |
| `ev_trolleydewire_<k>` | Its shoe is thrown off the wire: a bounce, a bend, the bus too far aside | The shoe flying off |
| `ev_trolleyhitmax_<k>` | The loose boom is stopped by its rope at `max_angle` | The jolt |
| `ev_trolleywirehit_<k>` | Its shoe falls back on the wire after bouncing | A knock |
| `ev_trolleyfrog_<k>` | Its shoe goes through a frog | The clatter |
| `ev_trolleyinsulator_<k>` | Its shoe goes over an insulator (silent without an entry) | The click |
| `ev_trolleyarc` | An arc's crack | The crack |
| `ev_trolleycontactor` | The rheostat controller steps | The contactor |

### A vehicle's own sounds

A vehicle plays its own sounds the OMSI way, in its `sound.cfg`, as OMSI 2 buses do for their
doors (`ev_doortriggeropen_0`, `ev_doorhitclose_0`):

* An entry with `[trigger]` on one of the triggers above plays each time that happens, and the
  game's own sound is not played.
* An entry whose volume curve or condition reads `Trolley_Hiss` replaces the hiss. A `[sound]`
  without a trigger loops.
* An entry with a silent file or no volume silences a sound.
* One recording for several events is one `[sound]` with several `[trigger]`s.

```text
[sound]
sound\pole_hook.wav
1
[trigger]
ev_trolleyhookopen
[trigger]
ev_trolleyhitdown

[sound]
sound\shoe_hiss.wav
1
[volcurve]
Trolley_Hiss
[pnt]
0
0
[pnt]
1
1
```

A vehicle's entry on a trigger replaces the game's sound even while it is silent. A lifting
gear's recording that already contains the shoe's hit can keep the hit's own entry quiet while
the run goes on, with a `[volcurve]`. A triggered sound's `[conditionSingle]` is not read, as in
OMSI 2.

```text
[sound]
pole_on_wire.wav
1
[trigger]
ev_trolleyhitup_1
[volcurve]
my_gear_running_1
[pnt]
0
1
[pnt]
1
0
```

More variables for sound:

* `Trolley_Pole<k>_Hit` (0 to 1) is how hard the last hit was, for a `[volcurve]`, like OMSI 2's
  `doorSpeed_<n>`.
* A `[volcurve]` on `Trolley_Pole<k>_Elev` or `_State` cuts a run's sound short where a hand stops
  the pole, as `door_<n>` does for a door.
* The boom's bounce has no sound of its own. Loop a rattle or a spring's creak on
  `Trolley_Pole<k>_Bounce`.

### A map's sounds

A map sets the overhead line's sounds (frogs and switch movements) in `[trolley_sounds]` of its
`trolley.txt`. Each key takes `on` (the game's own), `off`, or a sound file next to
`trolley.txt`. A file that cannot be read falls back to the game's own sound.

The other sounds (hiss, arc, contactors, thud, hook) belong to the vehicle. Those keys are ignored
in `trolley.txt`, with a warning.

`switch` is separate from `frog`:

* It plays once, at the frog, whenever the selected leg really changes: by radio, a contact plate
  or a manual setting.
* Repeated selections, rejected requests and the initial state make no sound.
* It defaults to `off`, so existing maps stay silent. `switch on` uses a generated metallic
  clack.
* Several vehicles share the network's events, so one movement plays once, not once per bus.

### How the built-in sounds are heard

* The hiss sits at each touching shoe, not at the bus centre. Its maximum gain is 0.035. Beyond
  1 m it fades steeply, and it is silent at 5 m and beyond. It is also silent in the cab.
* A bus's own hiss in `sound.cfg` keeps its own volume and 3D settings. Do not play an old
  sliding loop over the built-in hiss as well.
* The other built-in sounds are at full volume within a set distance, then fall off as 1/d:
  switch mechanism 15 m, arc 10 m, frog 8 m, thud and hook 6 m, contactors 4 m.
* In the player's cab, sounds on the roof come through the body at half volume and duller with
  doors and windows shut. A switch out on the line comes at a fifth, and duller. Both are clear
  with doors and windows open (`Snd_OutsideVol`). The contactors are inside and not muffled.
* How a sound is heard is fixed when it starts. One started in the cab stays muffled if the view
  changes while it plays.

## Testing

These environment variables help when building a wire or a vehicle. See
[Environment variables](DEBUG_FLAGS.md) for the full list.

| Variable | Does |
| --- | --- |
| `OMSI_TROLLEY_FILE` | Use this `trolley.txt` instead of the map's |
| `OMSI_TROLLEY_UP=1` | Put the poles on the wire by themselves (after 8 s) |
| `OMSI_TROLLEY_HANDS` | Script hands, for example `2-4:raise:01;12-14:pull:0` (seconds, raise or pull, poles) |
| `OMSI_TROLLEY_DEAD=2,3` | Take sections 2 and 3 off the supply |
| `OMSI_TROLLEY_DEAD_AT=x,y,r;...` | Make dead stretches (with an insulator at each end) on a map without real ones |
| `OMSI_DEBUG_TROLLEY=1` | Log the gear every second (`=0.25` for four times as often), every pole event and every arc. Also draws a marker, tongue and lamp at each switch of a `trolley.txt` (green straight, amber to the branch) |

An offscreen test run (see [Command line](COMMAND_LINE.md)):

```bash
OMSI_TROLLEY_FILE=trolley.txt OMSI_TROLLEY_UP=1 OMSI_DEBUG_TROLLEY=1 OMSI_AUTOPILOT=40 \
  openomsi --map maps/<map>/global.cfg --bus Vehicles/<bus>.bus --offscreen out.png \
  --drive 30 --spawn x,y,heading --view outside --snapshots 14.5
```

## Limits

* Only the player's bus has its gear simulated. AI vehicles do not run on the wire.
* Crossings of two wires are found from the geometry so a script can react. Nothing else of them
  is modelled, and a shoe never jumps from one wire to the other.
* The game draws poles as plain booms with a rope, not as any bus's model. A model with its own
  poles sets `visible 0` and follows the variables.
* Wire and gear are not sent over LAN yet. Another player sees a trolleybus as an ordinary bus.
