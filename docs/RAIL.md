# Trams, metros and trains

This page is for vehicle modders and map makers who build rail content: how openOMSI tells a
tram from a metro or a train, how it classes track, how switches and pantographs work, and the
vehicle blocks for traction, brakes and couplings. OMSI 2 has one kind of rail (`[path]` type 2)
and one flag for everything that runs on it. openOMSI reads what a map maker or modder says and
works out the rest.

Stock maps and mods load as before. A map without `openomsi-rail.cfg` or a `transport` folder
has its tracks classed by their names and surroundings.

## Vehicle classes

A vehicle is a rail vehicle when its file says so, as in OMSI 2: `[type] 2`, `[boogies]`,
`[contact_shoe]` or `[rail_body_osc]`. Its class is `tram`, `metro` or `train`. The game decides
it in this order:

1. `[rail_class]` in the `.ovh` / `.bus`, with the class on the next line.
2. A `[contact_shoe]` without a declared pantograph: a third rail, so a metro.
3. A car over 45 t, or a vehicle over 60 t with four driven axles: a train.
4. Anything else: a tram.

```text
[rail_class]
metro
```

`[rail_class]` accepts these names:

| Class | Names |
| --- | --- |
| tram | `tram`, `streetcar`, `strassenbahn`, `tramway` |
| metro | `metro`, `subway`, `ubahn`, `u-bahn`, `underground` |
| train | `train`, `railway`, `rail`, `sbahn`, `s-bahn`, `freight` |

Length says nothing: a 42 m low-floor tram is lighter than a 20 m coach. A car with both a shoe
and a pantograph (such as the NF6D) takes its current from the wire and stays a tram.

The game never takes a bus for a tram or a tram for a bus. A road vehicle has no class, a rail
vehicle always has one. A [transport mode](TRANSPORT_MODES.md) keyword such as
`[metro_transport]` decides which maps the vehicle may run on.

## Track

Every rail lane gets track information:

* street track (rails in or beside a road) or mainline track;
* whether a third rail runs beside it;
* whether it is in a tunnel;
* a speed limit.

The game takes this from, in order:

1. a rule in `network.cfg` of the map's `transport` folder (see
   [network.cfg](TRANSPORT_FORMATS.md#networkcfg));
2. a rule in the map's `openomsi-rail.cfg` (below);
3. the name of the spline profile or object;
4. where the track lies.

### openomsi-rail.cfg

Put this file next to the map's `global.cfg`. A minimal rule:

```text
# the tram track profile
[track]
file = rail_ks.sli
class = tram
street = yes
```

A rule with every key:

```text
[track]
file       = Splines/MyMap/tram_*.sli
tile       = 12 5
id         = 4711
path       = 1
class      = tram
street     = yes
third_rail = no
tunnel     = no
speed      = 40
```

| Key | Meaning |
| --- | --- |
| `file` (or `spline`, `object`, `profile`) | A glob on the spline profile or object file name (`*`, `?`, case-insensitive). Without a folder, only the file name is matched |
| `tile` | `x y`: only on this tile |
| `id` | Only this spline or object |
| `path` | Only this `[path]` of the object |
| `class` (or `for`) | `tram`, `metro` or `train`: who the track is for |
| `street` | `yes`: in the street. `no`: railway |
| `third_rail` | A third rail runs beside it |
| `tunnel` | In a tunnel |
| `speed` | Speed limit in km/h (a plain number) |

* Lines are `key = value`. `#` and `;` start a comment.
* A rule applies to every rail lane it matches. Later rules win over earlier ones.
* Unknown keys and blocks are ignored.
* A rule can just limit a turnout's diverging branch: give `tile`, `id`, `path` and `speed`.

A full example ships in the source repository under `docs/examples/openomsi-rail/`.

### Track names

Without a rule, the profile or object name decides. This way the stock editor is enough: pick or
copy a profile with the right name.

| The name contains | Track |
| --- | --- |
| `metro`, `ubahn`, `u-bahn`, `u_bahn`, `subway`, `u-bhf`, `ubhf` | Metro track with a third rail |
| `tram`, `strassenbahn`, `straßenbahn`, `str_bahn`, `tramway` | Tram track in the street |
| `train`, `railway`, `sbahn`, `s-bahn`, `eisenbahn`, `fernbahn`, `mainline` | Railway |

A generic name such as `rail_typ001` or `gleis` says nothing.

### Where the track lies

When neither a rule nor the name decides:

* track beside a street lane is tram track;
* a bend tighter than 60 m is tram track too (no railway bends so);
* anything else is railway.

### Speed limits

Each rail lane's limit is its own (the lane's `[rule] speedlimit`, or a rule's `speed`). It is
brought down to what its tightest curve allows: the speed at which sideways acceleration is
0.8 m/s² in a street and 1.4 m/s² on a railway (whose outer rail is raised). A turnout's
diverging branch is a curve, so it limits itself.

### Vehicles on the wrong track

A vehicle on track that is not for it (a train in a street, a metro without a third rail) is
reported in the log. It is not stopped, because mod maps lay their tracks as they like.

## Switches

A switch is a map object (its `.sco`) with two or more rail paths leaving one point.

**Your vehicle** takes, at a fork:

1. the branch the indicator points to (left or right), else
2. the branch the switch is set to, else
3. the straightest branch.

It throws the switches it runs over, so they move with it. A vehicle entering a switch from the
branch side (trailing) passes whatever it is set to.

**AI trains** take the branch their route names. Their signals ask for the switches from 600 m
ahead, unless the signal's own program says otherwise.

The map's `switches.cfg` can add rest positions, manual or interlocked points and diverging
speed limits (see [switches.cfg](TRANSPORT_FORMATS.md#switchescfg)).

## Pantographs

A rail vehicle with a pantograph moves only with it raised. Without current there is no drive in
either direction. The brakes still work.

**Shift+P** raises or lowers the pantograph of the vehicle you drive. When the pantograph is a
bus script's door (see below), the stop brake must be on, as for the door.

### How the game finds the pantograph

1. The vehicle's own declaration in its `.bus` / `.ovh`:

   ```text
   [pantograph_var]
   Panto_anim_oben

   [pantograph_trigger]
   cp_Panto_toggle
   ```

   | Keyword | Next line |
   | --- | --- |
   | `[pantograph_var]` | The variable that animates it: 0 down, 1 raised |
   | `[pantograph_trigger]` | The mouse-event trigger that raises and lowers it |

2. Else the meshes whose file names say pantograph (`PANT_1.o3d`, `Pantograf`,
   `Stromabnehmer`, `Токоприемник`) and the variable their animations use.

A declared pantograph always counts. One found from the model counts only when the game also
found the switch that raises it. Otherwise a wrong find could keep a vehicle from ever moving.

### A pantograph driven by a door

Some mods built from bus scripts animate the pantograph with a door. The Tatra T6B5 of Gladbeck
uses `door_0`. The game detects this and:

* takes `door_0` out of the doors: no door key, no boarding, no "door open while driving" for
  the career;
* uses the door's own `bus_door_0` / `bus_door_0_close` triggers as the pantograph switch.

The trigger is found in the script: the one that sets the variable, or the door leaf of that
name.

### With Realistic road physics

With **Realistic road physics** on (see [below](#realistic-road-physics)):

* A raised pantograph rises until its head meets the contact wire and stops there. The head's
  travel is measured on the model.
* Current flows only while the head lies on a live wire. A lowered pantograph, or a wire out of
  reach, gives none.
* The wire height is the class's standard height, kept within the pantograph's reach. So a map
  without a drawn wire never strands a tram.
* Where the wire is uneven at speed (rougher in wet and frost, harder at frogs) the head leaves
  it. A break under current strikes an arc.
* The car body moves on its secondary suspension: heave, pitch and roll, pushed by the bend,
  traction, braking and the rails' unevenness at each bogie.

## Current

A rail vehicle can say where its current comes from:

```text
[rail_power]
catenary
```

| Value | Meaning |
| --- | --- |
| `catenary` (`overhead`, `wire`, `pantograph`) | Overhead wire |
| `third_rail` (`thirdrail`, `shoe`) | Third rail |
| `diesel` (`battery`, `onboard`, `none`) | Carries its own power: no line needed |

Without `[rail_power]`: a `[contact_shoe]` means third rail, a declared pantograph means overhead
wire, anything else needs no line.

On a map with a `catenary.cfg` or `thirdrail.cfg` in its `transport` folder, the player's vehicle
moves only where its collector finds a live section:

* a pantograph within 2.5 m of the wire;
* a shoe within 1.2 m of the rail;
* not in a break between sections.

Its drive weakens below 80 % of the nominal voltage and is gone at 50 %. The brakes always work.
On a map without these files, the pantograph's switch alone decides. See
[catenary.cfg and thirdrail.cfg](TRANSPORT_FORMATS.md#catenarycfg-and-thirdrailcfg).

## Movement

The track says where a vehicle is. The rail dynamics say how fast it goes, with the forces of
railway engineering:

| Force | Model |
| --- | --- |
| Running resistance | `m g (a + b v)` by class: tram 3.5 N/kN, metro 2.5, train 1.5, rising with speed. 1.8 times that for a train starting from rest |
| Air | `½ ρ (Cd A + 0.4 m² per car) v²`, doubled in a tunnel. `ρ` follows the temperature |
| Curve | Röckl: `650 / (R - 55)` N/kN above 300 m, `500 / (R - 30)` below |
| Grade | The weight times the slope, in the direction the vehicle faces |
| Adhesion | Curtius and Kniffler `0.161 + 7.5 / (v + 44)` (v in km/h). Factor 0.7 on wet rails, 0.4 on leaves or snow, 0.2 on ice |
| Drive | The scripts' `M_Wheel` over the driven wheel radius, capped at adhesion times the weight on driven axles. Beyond it the wheels spin |
| Brakes | The scripts' `Axle_Brakeforce_*` of all cars, capped at 0.9 of adhesion times the weight. Beyond it the wheels slide and keep 0.75 of it |
| Track brake | A tram's emergency stop adds a magnetic track brake that adhesion does not limit |
| Mass | The whole consist, plus 6 % for the rotating parts |

### Vehicles without a scripted drive

Some vehicles' scripts give no drive (the stock trains are scripted for the AI only). They are
moved by the pedals with their class's typical performance:

| Class | Power | Start | Service brake | Emergency brake |
| --- | --- | --- | --- | --- |
| Tram | 14 kW/t | 1.2 m/s² | 1.3 m/s² | 2.8 m/s² |
| Metro | 14 kW/t | 1.1 m/s² | 1.2 m/s² | 1.8 m/s² |
| Train | 9 kW/t | 0.9 m/s² | 0.9 m/s² | 1.3 m/s² |

## Traction, brakes and couplings

A pedal-driven rail vehicle (one whose scripts give no drive) can describe how it pulls, brakes
and couples in three blocks of its `.bus` / `.ovh`. OMSI 2 skips them. What a file leaves out
keeps the class's typical figure.

Vehicles whose scripts give the drive and brakes (`M_Wheel`, `Axle_Brakeforce_*`) keep those.
For them the blocks only add the coupling check.

A minimal traction block:

```text
[rail_traction]
point 0 80
point 40 80
point 90 36
```

All three blocks:

```text
[rail_traction]
point 0 80          // km/h kN: the most the motors pull at that speed
point 40 80
point 90 36
power_kw 800
efficiency 0.9
regen_point 10 60   // km/h kN: the electric brake
regen_point 80 60

[rail_brake]
type air
service 1.1
emergency 1.6
pipe_wave 250
apply_time 4
release_time 12
blend 1

[rail_coupler]
type scharfenberg
slack 0.03
stiffness 3000
damping 80
max_force 900
```

Each line is `key value`. `//` starts a comment.

### [rail_traction]

| Key | Meaning | Default |
| --- | --- | --- |
| `point` | `km/h kN`: the most the motors pull at that speed. Repeat; sorted by speed | - |
| `power_kw` | Continuous power. The pull never passes `P × efficiency / v` | no limit |
| `efficiency` | Drive line, motors to rail (0.3 to 1) | 0.9 |
| `regen_point` | `km/h kN`: the electric brake. Repeat. None: no electric brake | - |

* Throttle times the curve's pull at the speed, capped by the power.
* Past the last point the pull falls as 1/v.
* The adhesion limit still applies, so a strong motor spins on a leafy rail.

### [rail_brake]

| Key | Meaning | Default |
| --- | --- | --- |
| `type` | `air`, `ep` (electro-pneumatic), `direct` (also `electric`, `hydraulic`) | `air` |
| `service` | Full service deceleration (m/s²) | the class's |
| `emergency` | Emergency deceleration (m/s²) | the class's |
| `pipe_wave` | How fast a change runs down the brake pipe (m/s, 50 to 340) | 250 |
| `apply_time` | Seconds for a cylinder to fill | 4 |
| `release_time` | Seconds for it to empty | 12 |
| `blend` | 1: the electric brake first, the friction brake the rest | 1 |

* Brake pedal 0 to 0.9 is a service application up to full service. Above 0.9 is an emergency.
* On an `air` brake the pipe's drop runs down the train as a wave, and each cylinder fills in
  `apply_time`. The tail brakes later than the head, so a long train stops further on than one
  braked by wire (`ep`) or `direct`.
* Releasing is slower than applying.
* The electric brake takes what it can at the speed and fades out near standstill. The friction
  brake does the rest.

### [rail_coupler]

| Key | Meaning | Default (Scharfenberg) |
| --- | --- | --- |
| `type` | `screw`, `buffer`, `scharfenberg`, `tightlock` | `scharfenberg` |
| `slack` | Free play per coupling (m) | 0.03 |
| `stiffness` | Draft gear (kN/m) | 3000 |
| `damping` | kN·s/m | 80 |
| `max_force` | kN. Beyond it the train breaks in two | 900 |

The type sets the other defaults:

| Type | Slack | Stiffness | Damping | Max force |
| --- | --- | --- | --- | --- |
| `screw` | 0.30 m | 2000 kN/m | 100 kN·s/m | 850 kN |
| `buffer` | 0.12 m | 4000 kN/m | 150 kN·s/m | 1000 kN |
| `scharfenberg` | 0.03 m | 3000 kN/m | 80 kN·s/m | 900 kN |
| `tightlock` | 0.06 m | 3500 kN/m | 100 kN·s/m | 1000 kN |

The cars are masses with free play and a draft gear between them. Starting takes up the slack
car by car and jerks. Braking from the head runs the cars in. A coupling pulled past `max_force`
breaks: the log says so and the brakes come on.

## Realistic road physics

Buses use OMSI 2's own physics unless **Realistic road physics** is on. It is off by default.
Turn it on in the launcher under **Settings → Gameplay → Simulation**, or in the game's options
window. It is stored as `realistic_road_physics=1` in `settings.cfg`.

With it on, everything comes from the vehicle's own file and the map:

* **Wind.** The weather's wind acts on the bus: a crosswind leans and pushes it, a headwind
  slows it. Air density other than 1.2 kg/m³ counts too. Drag the vehicle's scripts already apply
  (every OMSI 2 bus has it in `bremse.osc`) is not counted twice.
* **Tyres by surface.** Each tyre's grip and rolling resistance depend on what it rolls on
  (`Axle_SurfaceID_`: asphalt, concrete, cobblestone, dirt, grass, gravel, snow, deep snow), wet
  or dry. Rolling resistance rises with speed and load.
* **Road roughness** by surface after ISO 8608. Cobbles, gravel, grass and the joints of an old
  concrete-slab road shake the bus. The rear axle drives over what the front axle drove over.
* **Slip-angle tyres.** Side forces come from the slip angle (relaxation length 0.9 tyre radii,
  cornering stiffness 14 per radian of load, friction ellipse with the longitudinal force),
  instead of OMSI's "follows its wheels" rule.

For rail vehicles the same setting turns on the pantograph contact and the body on its springs
(see [Pantographs](#with-realistic-road-physics)).

## Debugging

`OMSI_DEBUG_RAIL=1` logs, twice a second, the drive and brake of the rail vehicle you drive, and
the pantograph: its value, head, wire, contact and current. See [Environment variables](DEBUG_FLAGS.md).

## Status

What a full tram, light rail and train simulation needs, and where openOMSI stands (issue
#3969). Nothing here changes how OMSI 2 content loads.

| Part | Now | Next |
| --- | --- | --- |
| Track position | A rail vehicle runs on a rail lane of the map's paths. Plugins read it with `omsi.bus.track()` | Routes as chains of lanes with kilometre posts, also for AI trains |
| Switches | Set by the driver, by AI routes, and by `switches.cfg` rules | Interlockings that lock routes |
| Signals and blocks | Blocks of `signals.cfg` hold AI trains; their signals show it | Aspects; blocks for the player's train in a train protection system |
| Train protection | - | A plugin API first, then PZB/LZB-like systems as vehicle blocks |
| Catenary | Live and dead sections and breaks; pantographs that touch the wire | Voltage drop with distance and load; AI vehicles on the line |
| Vehicle physics | Traction, brakes and couplings per vehicle; adhesion; body on its springs | - |
| Infrastructure looks | Tracks and wires are the map's meshes | Generated sleepers, ballast and masts for maps that only give the lanes |

Test vehicles from the community (CC-licensed light rail cars were offered in the issue) are
welcome.
