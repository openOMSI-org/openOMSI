# Trolleybuses

OMSI 2 itself has no overhead wire. Its trolleybus add-ons carry a plugin DLL that reads the
bus's place out of Omsi.exe's memory and moves fake poles over wires that are only meshes on a
map; none of that can run in openOMSI. openOMSI therefore has its own trolleybus simulation,
built into the game and described by two small text formats, so that a map can carry a real
overhead wire and a vehicle real poles and a real drive:

* the **wire** of a map: `trolley.txt` beside its `global.cfg` (section "The wire" below);
* the **gear** of a vehicle: `[trolley_pole]` and a drive in its `.bus` (section "The gear").

Everything is worked out from mechanics and electrics, with the numbers a real trolleybus has;
the constants are in the vehicle's file, not in the game.

> Real trolleybus add-ons for OMSI 2 carry no wire logic of their own; how they are read and run
> here is in [TROLLEY_MODS.md](TROLLEY_MODS.md). A map's wire splines are read as the wire when no
> `trolley.txt` is given, and a bus with `cp_SHTANGALEV`/`cp_SHTANGAPRAV` pole meshes gets poles
> without a `[trolley_pole]` block.

## What it does

* **Two wires** a fixed distance apart, one of each polarity. A bus is powered only while one
  pole is on the positive and the other on the negative wire, and both lie on wire a
  substation feeds. A section can be dead; between sections there is an insulator (a dead
  stretch around the joint, 0.8 m unless the file says otherwise).
* **Poles.** Each is a boom hinged on the roof with an elevation and a swing, a spring pressing
  the shoe up against the wire (120 N at the working angle by default), its own mass and
  inertia, and a shoe whose groove holds the wire. On the wire the shoe lies on it a boom's
  length from the hinge; the spring's force has to accelerate the boom along every rise of the
  wire. Where the wire rises faster than the boom can follow (a hanger, a kink, the wire's
  unevenness at speed) the shoe leaves it for a moment: the contact is open and an arc strikes
  if current flows. If the wire lifts out of the groove the pole has **come off** (bounce); it
  also comes off when
  * the boom stands too far off the wire's direction seen from above (40 degrees by default:
    about 4 m off the line for a 6.5 m boom), which is what a bus leaving the wire's line, or a
    wrong switch position, does;
  * a bend of the wire is taken too fast: the shoe cannot turn quicker than its sideways speed
    allows (`kink_speed`, 3 m/s: a 25 degree bend at 11 m/s is too much, at 3 m/s it is not);
  * the wire is out of the boom's reach or ends under the shoe.
  A pole that has come off springs up to its stop (80 degrees) and swings about under the bus's
  own motion until a rope holds it. A pole with a **rope retriever** pulls itself down to its
  hook half a second later.
* **By hand.** Out of the cab, near the back of the bus (walking, the outside camera or the free
  camera) hold **R**: the rope of the pole nearest to you is in your hand and the boom rises and
  follows where you look (the cross-hairs of the view, the wire it meets within 1.4 m); let go
  of R and the spring lifts the shoe into the wire, if you aimed it true (the shoe's horns catch
  a wire within 7 cm). A hand is not steady: a boom aimed at a wire from the ground wanders a
  few centimetres. **T** pulls the pole down on to its hook (on the wire it pulls the shoe off,
  with an arc if current flows). **Shift** with either takes both poles. In the cab only poles
  with a motor (`motor 1`) answer R and T, and they find the wire by themselves.
* **Power.** With no circuit the bus does not move: a vehicle with a modelled drive loses its
  force at once (the bus rolls on, slowing under its rolling resistance, and stops), and the
  torque of a scripted drive is switched off (a torque that brakes still works). The voltage a
  bus finds is the substation's less what the feeder drops with the section's current,
  including other trolleybuses on the same section.
* **Rheostat drive** (`[trolley_rheostat]`): a series-wound DC motor solved against a stack of
  resistors; a controller steps the resistors out (14 steps) and then the field in (two
  weakening steps) while the current stays under what the pedal asks for, the resistors turn
  what they take into heat, and the contactors click; dynamic braking excites the motors from
  the line and drives a resistor (without the line only the mechanical brakes work).
* **Asynchronous drive** (`[trolley_async]`): an inverter with a force limit up to its corner
  speed and a power limit above it, faded out as the voltage sags and tripped in a deep sag;
  regenerative braking gives power back to the section as far as its feeder takes it
  (`regen_kw` of the section plus what other buses draw), a brake resistor on the roof burns the
  rest; an optional **battery** (`[trolley_battery]`) moves the bus slowly (15 km/h, 30 kW by
  default) when it is not on the wire and charges from the line.
* **Switches.** With `mode power` or `mode indicator`, a frog in the wire (`[trolley_switch]`)
  is set by a contact plate a few metres before it, when a shoe passes. In power mode the current the bus draws then says which
  way (power on takes the branch, coasting goes straight; `invert 1` the other way), with
  `mode indicator` the bus's indicator on the branch's side takes the branch, with `mode radio`
  the cab's left or right request (`Trolley_Switch_Request`) takes that way, and without a
  request the vehicle's route code (`Trolley_Switch_Code`) is looked up in the switch's `route`
  lines; a radio switch with a `channel` hears only vehicles sending on it
  (`Trolley_Switch_Channel`), and with nothing it hears it keeps its position. Radio mode
  always receives requests inside its horizontal `range` (150 m by default), including
  while the poles are down. It never reads a contact plate and does not wait for the
  power/indicator modes' 2.5 s lock: a new matching request changes the switch and its display
  state on the next simulation step. Taking it the wrong way needs no rule: the shoe
  follows the branch while the bus goes straight on, and the pole comes off when the boom
  stands too far off the wire. A shoe entering a frog from either branch always goes on along
  the single wire.
  Radio range is a circle around the frog measured from the vehicle's trolley reference
  position, not along the wire. There is no heading or approach filter: every radio switch
  in range on a matching channel can respond, even on a neighbouring road. Use channels to
  separate nearby receivers. After its initial step, a vehicle transmits only on a change
  of direction request or route code, not continuously while holding the same selection.
  Releasing a request to `0` transmits nothing, so a toggled or chosen switch is not handed
  back to the route code.
  A route code other than `0` is also sent to each receiver as the vehicle enters its range,
  so the switches on a route set themselves as the bus arrives. Direction requests are never
  sent on entry. Loading a vehicle and changing the channel alone never transmit, and a
  direction selected outside reception range is not queued for later delivery.
  The last matching transmission sets the switch; another bus's stale selection does not
  immediately undo it. An unchanged position does not reset the display's change timer. Leaving the circle,
  sending an unknown route or selecting another channel leaves the frog as it was.
  A momentary button can instead send `Trolley_Switch_Request = 2` while pressed and `0`
  when released. Each press toggles the actual retained position of every matching receiver,
  not a remembered cab selection. Holding it does not repeat; entering range, changing the
  channel or changing the route while held does not toggle. Release and press again to send
  another toggle. A request already held when the vehicle is first stepped is not a press.
  Absolute left/right requests use the selection-change behavior above.
  For manual-only radio control, keep the route code at `0` and use a momentary toggle
  button. The driver reads the actual position from the roadside switch display. Range is
  configurable reception coverage, not a model of antenna propagation or a safety interlock.
* **Arcs and sound.** Where current is broken - the shoe leaving the wire, a pole coming off,
  an insulator or a frog crossed under power - an arc burns `L I^2 / 2` of the motor circuit's
  inductance (12 mH): the game draws a flash with sparks, lights the wire and the roof and
  cracks. Coasting through an insulator strikes none; power on through it a loud one. A frog
  or an insulator crossed with more than 3 A (a coasting bus's auxiliaries often draw more)
  arcs every time by default; a map's `[trolley_net]` can make those arcs rarer, weaker or need
  more current (`frog_arc_*`, `insulator_arc_*` below). The chance is drawn per frog crossing
  and per insulator entry and exit, from the vehicle's seed (repeatable). Arcs of a shoe
  leaving the wire (see `roughness`) and of a pole coming off are not affected. The
  shoes hiss on the wire, louder with the current and the speed (a loop with the rectifier's
  300 Hz ripple); the contactors click, a shoe landing on the wire and a pole on its hook
  thud, a frog clatters. The sounds are made in the game unless the vehicle or the map gives
  its own (see Sounds below).

## Switch displays

A map author can use a scripted scenery object as a switch display. Declare these numeric
variables in the object's `[varnamelist]`; the engine supplies them before its script runs:

| Variable | Value |
| --- | --- |
| `Trolley_Frog_State` | `0`: no matching switch, `1`: set to the left leg, `2`: set to the right leg, seen from the toe |
| `Trolley_Frog_Straight` | `1` if the selected leg is the switch's straight way, otherwise `0` |
| `Trolley_Frog_Changed` | Simulation seconds since the switch last changed; `1000000` initially or without a switch |

The object must declare `Trolley_Frog_State` to opt in. A placed object's string matching a
`[trolley_switch]` name selects that switch (case-insensitive, ignoring surrounding spaces).
Otherwise it shows the nearest switch within 40 m horizontally. Name displays explicitly at
closely spaced junctions so they do not bind to a neighbouring switch.

These values report the actual retained switch position, not the dashboard request. They
work for radio, indicator, power and fixed switches, and can drive `[visible]`,
`[matl_change]` or an OMSI script. For example, a left-bar mesh can use:

```text
[visible]
Trolley_Frog_State
1
```

The right-bar mesh uses value `2`. The map supplies the housing, symbols and any blink
animation; no display model is generated by the engine. The display timer advances once
per scenery simulation step, independently of the number of vehicles. Repeating the same
request or rejecting a request during the switch lock does not reset it. The network becomes
available when a vehicle loads the map's wire and remains available after that vehicle is
removed. Existing add-on signs that predict a request keep their own behavior until their
scripts are adapted to these variables.

## The wire: `trolley.txt`

A map's `trolley.txt` sits in the folder of its `global.cfg`; another content root (a mod) may
hold `maps/<name>/trolley.txt` for a map that has none, and `OMSI_TROLLEY_FILE` names a file for
a test. Lines are `key value`, grouped in `[block]`s, `//` ends a line's meaning, coordinates
are the map's (x east, y north, z up, metres).

```
[trolley_net]
voltage 600            nominal line voltage (V)
spacing 0.6            distance between the two wires (m)
roughness 0.0012       unevenness of the contact wire (m); worn or icy wire: 0.003 and more
frog_arc_chance 1      chance (0..1) that a frog crossed under current arcs; 0: never
frog_arc_scale 1       multiplier of a frog arc's energy (0..10): its flash and crack
frog_arc_min_amps 3    least current (A) a frog crossing arcs with
insulator_arc_chance 1    the same for each entry into and exit from a section insulator
insulator_arc_scale 1
insulator_arc_min_amps 3

[trolley_section]
id 1
name Substation A
live 1                 0: no substation feeds it
regen_kw 0             power its feeder takes back from a braking bus (kW)
feeder_ohm 0.05

[trolley_span]         the centre line of the pair of wires, as many points as the wire has
section 1              bends and hangers, in the direction the positive wire is on the right of
spacing 0.6            (optional)
p 395.0 -185.4 6.10
p 392.0 -185.4 6.05
...

[trolley_switch]       at a point where three spans meet
name Seeburger junction
pos 262 -185.4 6.1
mode radio             power | indicator | radio | fixed
trigger 9              power/indicator: contact plate distance before the frog (m); ignored by radio
sense 12               the current that counts as power on (A)
invert 0
initial straight
route 158 branch       radio: route code 158 takes the branch (straight | branch | left | right)
channel 2              radio: only vehicles sending on channel 2 set it
range 150              radio: reception radius (m, 1..1000, default 150); never a contact plate

[trolley_insulator]    at a joint of two spans
pos 300 -185.4 6.1
length 1.0             dead stretch (m)

[trolley_sounds]       the overhead line's sounds (see Sounds): on | off | a file beside trolley.txt
frog sounds\frog.wav
switch sounds\switch.wav
```

Spans that end within 25 cm of each other are joined. Where the sections of two joined spans
differ there is an insulator (0.8 m) unless a `[trolley_insulator]` gives its length. A switch
needs a joint of exactly three spans; which one is the toe (the single side) and which leaves
straight or to the side is worked out from their directions. A pole can follow the wire through
a joint, a switch and across spans, and a crossing of two wires that do not share a joint is not
a connection.

The wire's masts and hangers are the map's own objects: the game draws the two wires (8.5 mm
thick) and nothing else, whatever vehicle is driven (a bus or a car sees the wire too; a map
without `trolley.txt` whose wire is read off its splines draws that wire itself). The sag
between hangers is in the points. With `trolley.txt`, switches have no frog drawn over them;
`OMSI_DEBUG_TROLLEY` draws a marker at each, a tongue along the way it is set and a lamp
(green straight, amber to the branch), for testing. Spline-derived networks retain these
generated switch bodies, tongues and lamps in normal play, without requiring the debug flag.

`OMSI_TROLLEY_DEAD=2,3` takes the sections of those ids off the supply for a run.

## The gear: blocks of a vehicle's `.bus`

Inside a block every line is `key value ...`; keys left out keep the default.

```
[trolley_pole]              one per pole
side left                   left | right (or -1 | 1)
pos -0.3 1.0 4.16           the hinge on the roof (x right, y forward, z up, m)
length 6.7                  boom length (m)
rest_angle -1.5             elevation lying on the hook (degrees)
work_angle 16               elevation in work (degrees): where the spring's force is rated
max_angle 45                the highest it rises loose, held by its rope (degrees)
tip_force 120               what the shoe presses the wire with at work_angle (N)
mass 16                     boom with rope (kg)
head_mass 3                 head and shoe (kg)
groove_depth 0.06           how far the wire sits in the shoe (m)
funnel 0.07                 half the width the horns catch a wire in (m)
max_wire_angle 40           how far the boom may stand off the wire (degrees)
kink_speed 3.0              sideways speed of the shoe a bend of the wire may ask (m/s)
retriever 35                a rope retriever pulls it down at this rate after dewiring (deg/s)
motor 1                     raised and lowered from the cab
boom_bounce 1.1             the boom's bounce on a script's lifting gear (Hz)
boom_damping 0.12           how quickly that bounce dies away (damping ratio)
visible 1                   the game draws the boom and its rope (0: the model draws its own)
reach 9                     how far from the rope's handle it can be handled (m)
```

```
[trolley_rheostat]          a series DC drive with a resistor controller (all keys optional)
gear_ratio 9.6              motor to wheels
gear_efficiency 0.93
motors 1
armature_ohm 0.14           armature and series field
flux_sat 3.3                flux linkage at saturation (V s/rad)
flux_half_amps 25           the current at which half of it is built up
steps 14                    resistor steps
start_ohm 2.6               the whole external resistance
field_steps 0.72 0.5        flux kept in the field-weakening steps
step_up_s 0.22              seconds a step takes when the controller climbs / falls
step_down_s 0.08
current_min 120             the pedal's current at its ends (A, per motor)
current_max 280
brake_current 220           dynamic braking: the largest generator current (A)
brake_excitation 0.75
brake_force 56000           mechanical brakes with the pedal down (N)
overspeed 380               motor speed limit (rad/s)

[trolley_async]             an inverter drive
max_force 30000             force at the wheels from standstill (N)
max_power_kw 170            power at the wheels above the corner speed
efficiency 0.9
regen_power_kw 150
regen_force 26000
chopper 1                   a brake resistor takes what the line will not
v_min 480                   voltage below which the force fades
v_trip 380                  voltage below which the inverter trips
v_regen_max 720
brake_force 56000

[trolley_battery]           optional, for the asynchronous drive
kwh 4
kw 30
max_kmh 15

[trolley_aux]
6.0                         lights, compressor and heater taken from the line (kW)

[trolley_vars]              names of the script variables the gear reads, if the model's differ
direction Trolley_Direction     > 0.5 forward, < -0.5 reverse, else neutral (default: forward)
indicator_left Blinker_Left     for switches in `indicator` mode
indicator_right Blinker_Right
raise_request Trolley_Poles_Raise   +1 raise, -1 lower the poles that have a motor
switch_request Trolley_Switch_Request   radio: -1 left, +1 right, 2 momentary toggle, 0 none (without it: indicator)
switch_code Trolley_Switch_Code         radio switches: the route code, 0 none
switch_channel Trolley_Switch_Channel   radio switches: the channel sent on
```

With a modelled drive the vehicle's own `M_Wheel` is replaced by the drive's force (times the
wheel radius of the driven axle) and the drive's mechanical brake force is added to
`Brakeforce` (the vehicle's variable list must declare `Brakeforce`, `M_Wheel`, `Throttle`
and `Brake`, as every vehicle's does). Without a drive block the script's driving torque is
kept but scaled by the line voltage over the nominal voltage, and cut when the poles bring no
current: **a mod's scripted drive works on a wire and stops without one**.

### Variables for the model and the sound

The game writes these every physics step (declared in the model or not; a model may animate or
make sound by them):

| Variable | Meaning |
|---|---|
| `Trolley_Pole<k>_Elev`, `_Azim` | pole k (1, 2...): elevation over the horizontal and swing from straight back (degrees, + to the right) |
| `Trolley_Pole<k>_State` | 0 down on the hook, 1 being pulled down, 2 in the hand, 3 loose, 4 on the wire with the contact open, 5 on the wire |
| `Trolley_Pole<k>_Contact` | 1 while the shoe touches the wire |
| `Trolley_Pole<k>_Force` | what the shoe presses with (N) |
| `Trolley_Pole<k>_Bounce` | 0..1 how hard the boom bounces on its script's lifting gear |
| `Trolley_Pole<k>_Hit` | 0..1 how hard its last hit was (on the wire, back on it, against its rope) |
| `Trolley_Volts`, `Trolley_Amps` | the voltage at the bus and the current from the line (negative: given back) |
| `Trolley_Powered` | 1 while both poles are on live wire |
| `Trolley_Notch` | the rheostat controller's position |
| `Trolley_Arc` | 0..1 how hard an arc burns now |
| `Trolley_Hiss` | 0..1 the shoes' hiss |
| `Trolley_Battery` | the battery's charge, 0..1 |

A vehicle whose model carries its own poles puts `visible 0` in its `[trolley_pole]` and rotates
its pole meshes by `Trolley_Pole<k>_Elev` and `_Azim`.

### A script's lifting gear

A vehicle's script can run its poles' lifting gear itself, as an OMSI 2 door script runs its
doors: it declares and writes `Trolley_Pole<k>_Gear`, where the gear's carriage is, 0 on the hook,
1 just under the wire (the wire above it, else where the pole left the wire, else its
`work_angle`). The boom rides on the carriage as a spring (`boom_bounce`, `boom_damping`), so
wherever the carriage stops or starts the boom bounces; at 1 the gear lets it go, and its spring
puts the shoe on the wire. Going below 1 from the wire takes the shoe off it; at 0 the hook
catches the boom once it has come down.

The script times the run as it likes (a delay, a stop halfway, the speeds, from its own constant
file) and fires its run's sound itself, with a trigger of its own, as a door script does. The
pole follows the variable from its first change until the run's end (on the wire or the hook);
a variable left as it is moves nothing, so a hand or the cab still puts a pole up or down. Taken
by hand, a pole leaves the script's run. Without the variable `motor 1` works as before.

```
'raise from the hook: 2 s to halfway, 3 s there, 2 s on
(L.L.gear_t) (L.S.Timegap) + (S.L.gear_t)
(L.L.gear_t) 2 < {if} (L.L.gear_t) 4 / {else} (L.L.gear_t) 5 < {if} 0.5 {else} (L.L.gear_t) 5 - 4 / 0.5 + 1 min {endif} {endif}
(S.L.Trolley_Pole1_Gear) (S.L.Trolley_Pole2_Gear)
```

## Sounds

The gear makes six sounds of its own, from noise and decaying tones (no files):

| Sound | When | Sound trigger | Map key |
|---|---|---|---|
| hiss (a loop) | the shoes sliding on the wire, louder with the current and the speed | - (`Trolley_Hiss`, 0..1) | - |
| arc crack | current broken: a shoe leaving the wire or a pole coming off under load, an insulator or a frog crossed under power | `ev_trolleyarc` | - |
| frog clatter | a shoe going through a frog | `ev_trolleyfrog` (below) | `frog` |
| contactor click | the rheostat controller stepping | `ev_trolleycontactor` | - |
| thud | a shoe landing on the wire or coming off it | `ev_trolleyhitup`, `wirehit`, `wireoff`, `dewire`, `hitmax` (below) | - |
| hook | a pole's hook letting go or catching it | `ev_trolleyhookopen`, `hitdown` (below) | - |
| switch mechanism | the selected leg actually changes, not when a shoe crosses it | - (map-owned) | `switch` |

A vehicle plays its own recordings of the poles as OMSI 2 buses do their doors'
(`ev_doortriggeropen_0` as a door is ordered to move, `ev_doorhitclose_0` as it hits its stop).

A pole's own triggers end in its number `_<k>`, from 1 as its `Trolley_Pole<k>` variables;
the same trigger without the number plays for either pole (fired once when both do it in the
same frame).

The game fires these for what happens to a pole, whoever moves it (a hand, a script's lifting
gear, a bounce):

| Sound trigger | When | What to record |
|---|---|---|
| `ev_trolleyhookopen_<k>` | the pole is taken off its hook by hand | the hook opening |
| `ev_trolleyhitup_<k>` | its shoe lands on the wire | the hit |
| `ev_trolleywireoff_<k>` | its shoe is taken off the wire by the rope | the shoe leaving it |
| `ev_trolleyhitdown_<k>` | it comes down on the roof and its hook catches it | the hit and the lock |
| `ev_trolleydewire_<k>` | its shoe is thrown off the wire: a bounce, a bend, the bus too far aside | the shoe flying off |
| `ev_trolleyhitmax_<k>` | the loose boom is brought up short by its rope at `max_angle` | the jolt |
| `ev_trolleywirehit_<k>` | its shoe falls back on the wire after bouncing off it | a knock |
| `ev_trolleyfrog_<k>` | its shoe goes through a frog | the clatter |
| `ev_trolleyinsulator_<k>` | its shoe goes over a section insulator (silent without it) | the click |
| `ev_trolleyarc` | an arc's crack (see above) | the crack |
| `ev_trolleycontactor` | the rheostat controller steps | the contactor |

One recording for several of these is one `[sound]` with several `[trigger]`s, as OMSI 2's. A
vehicle's entry on a trigger replaces the game's own sound even while it is silent: a lifting
gear's recording that has the shoe's hit in it (the script fires it with the run, as a door's
opening) keeps the hit's own entry quiet while its script's run goes on, with a `[volcurve]` (a
triggered sound's `[conditionSingle]` is not looked at, as in OMSI 2):

```
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

`Trolley_Pole<k>_Hit` (0..1) is how hard pole k's last hit was (on the wire, back on it, against
its rope), for a `[volcurve]` as OMSI 2's `doorSpeed_<n>`. A `[volcurve]` on `Trolley_Pole<k>_Elev`
or `_State` cuts a run's sound short where a hand stops the pole, as `door_<n>` does a door's.
The boom's bounce has no sound of its own; a vehicle can loop a rattle or a spring's creak on
`Trolley_Pole<k>_Bounce` (0..1).

The built-in hiss is located at each touching shoe, not the bus
centre. Its maximum gain is 0.035: beyond 1 m a steep fade is added to the normal distance
falloff, and it is completely silent at 5 m and beyond. It is also silent in the cab.
A bus-authored `sound.cfg` hiss keeps its own volume, viewpoint and 3-D settings; old sliding
loops should not be played over the native hiss as well.

The other built-in sounds are at full volume within a set distance (`[3d]`-style: switch
mechanism 15 m, arc crack 10 m, frog 8 m, thud and hook 6 m, contactors 4 m) and fall off as
1/d beyond. In the player's cab the sounds on the roof come through the body at half volume
and somewhat dull with doors and windows shut, a switch out on the line at a fifth and duller,
both clear as doors and windows open (`Snd_OutsideVol`); the contactors are in the bus and are
not muffled. How a sound is heard is
fixed when it starts: one started in the cab stays muffled if the view changes while it plays.

**A vehicle** plays its own the OMSI way, in its `sound.cfg`: an entry with `[trigger]` on one
of the triggers above plays each time that happens (the game fires the trigger as a script's
`(T.L.ev_trolleyhitup_1)` would), and the game's own sound is not played; an entry whose volume
curve or condition reads `Trolley_Hiss` takes the place of the hiss (a `[sound]` without a
trigger loops; its `[volcurve]` on `Trolley_Hiss` makes it follow the hiss). An entry with a silent file or no volume silences a sound.

```
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

**A map** chooses the overhead line's own sounds (frog crossings and switch movements) in a `[trolley_sounds]` block of
its `trolley.txt`: each key `on` (the game's own), `off`, or a sound file beside `trolley.txt`
(a file that cannot be read leaves the game's own). The vehicle's sounds (hiss, arc crack,
contactors, thud, hook) are the vehicle's: those keys are ignored there with a warning.

Which plays: the vehicle's `sound.cfg` entry, else the map's choice, else the game's own. A
vehicle and a map that say nothing hear the game's own sounds, as before.

`switch` is separate from `frog`: it plays once at the frog's world position whenever the
selected leg really changes, including by radio, a power/indicator plate or a manual setting.
Repeated selections, rejected requests and the initial state make no movement sound. This is
a map sound, not a bus sound trigger. It defaults to `off`, preserving existing maps' silence;
`switch on` uses a generated metallic clack, and `switch sounds\switch.wav` uses the
map's file. An unreadable replacement falls back to the generated clack. Multiple vehicles
share the network's events, so the same movement is not played once per bus.

## Keys

| Key | Where | Does |
|---|---|---|
| R (hold) | outside the bus near its back (on foot, outside or free camera) | take the rope of the nearest pole: it rises to just under the wire you look at (else the one above its working angle); let go to let its spring put it on |
| T | the same | pull the nearest pole down (the shoe off the wire, the boom on its hook) |
| Shift + R / T | the same | both poles |
| Ctrl + R | anywhere, the cab too | both poles on the wire by themselves, or down if one is on, with or without a lifting gear; not where the keyboard file or the bus has Ctrl+R (Ctrl+T is OMSI's change_give) |

In the cab R and T are the gearbox's: the poles answer the cab's own switch and Ctrl+R there.

The game's auto-start (Shift+U) also puts the poles on the wire, when each one has a wire
within its reach.

On foot you get out with Ctrl+Shift+G (the `get_up` setting); the outside camera is F3.

## Checking it offscreen

```
OMSI_TROLLEY_FILE=trolley.txt OMSI_TROLLEY_UP=1 OMSI_DEBUG_TROLLEY=1 OMSI_AUTOPILOT=40 \
  openomsi --map maps/<map>/global.cfg --bus Vehicles/<bus>.bus --offscreen out.png \
  --drive 30 --spawn x,y,heading --view outside --snapshots 14.5
```

`OMSI_TROLLEY_UP` puts the poles on the wire by themselves (8 s), `OMSI_TROLLEY_HANDS` scripts
hands ("2-4:raise:01;12-14:pull:0": seconds, raise or pull, poles), `OMSI_TROLLEY_DEAD` kills
sections, `OMSI_DEBUG_TROLLEY=1` logs the gear every second (set `=0.25` for four times as
often) with every event of the poles (a frog passed, an insulator, a dewiring and why) and every arc.
Unit tests of the wire network, the poles' contact and dewiring, the switches, the drives and
the whole gear are in `crates/omsi-trolley`.

## Limits

* Only the player's bus has gear simulated; AI vehicles do not run on the wire.
* Crossings of two wires (an overhead crossing with insulated sections) are not modelled apart
  from the geometry; a shoe never jumps from one wire to the other.
* The poles are drawn as plain booms with a rope, not as the model of any bus; a model that
  has its own sets `visible 0` and follows the variables.
* Wire and gear are not sent over LAN yet; another player sees a trolleybus as an ordinary bus.


## The mouse's hand on a pole

A click on the cab's pole switch (a plain OMSI 2 trolleybus: `cp_SHTANGALEV`, `cp_SHTANGAPRAV`
or what its model calls it) takes that pole in hand. The tip of the boom follows the mouse: it
goes to the wire the cursor points at (as the R key's gaze does), or to where the cursor points
when no wire is near. The next click lets go, and the spring lifts the shoe into the wire it was
aimed at; a wire missed, the pole flies loose and the motor's retriever (if it has one) pulls it
down. One hand: one pole at a time. T still pulls a pole down by its rope.

From outside the cab (the outside, free or foot view) a click on a pole is the hand on its rope:
the pole is taken where it is, the mouse up lets it rise, down pulls it down, sideways swings it,
with no help to the wire.
The next click lets go, and the spring lifts the shoe from where it was held: into the wire just
above it, or loose past it. Pushed up through a wire the shoe takes it and the hand lets go; let
go a little over its hook, the pole catches on it. A pole on the wire is taken off it the same way
(an arc, as for any dewiring under current).

A model can hold the ropes itself, through its own `[mouseevent]`s on the boom and the rope, as
OMSI 2 trolleybuses do: its script declares and writes, per pole k,

| Variable | Meaning |
|---|---|
| `Trolley_Pole<k>_Rope` | 1 the rope is held (the press of its mouse event), -1 pulled down (a retriever), 0 let go |
| `Trolley_Pole<k>_Rope_X`, `_Rope_Y` | the mouse's movement while held, in pixels (`(L.S.mouse_x)` / `_y` of its `_drag`, added up): the game takes them each step and sets them to 0 |

A pole with these takes no click of the game's own; the hand works as above, held while the
mouse button is.

From the driver's seat the poles are behind the head and the wire is out of the cursor's reach,
so a click on the switch has the motor raise the pole to the wire nearest its tip (the hand lets
go when it is up). A plain trolleybus draws no boom of its own: the model's raised pole mesh is
turned about its hinge by `Trolley_Pole<k>_Elev` and `_Azim` (two `[newanim]`-style animations
made when the model is loaded), and the cab's switch reads 0 with the pole on its hook (the
folded mesh) and 1 otherwise.
