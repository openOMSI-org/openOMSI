# Trams, metros and trains

OMSI 2 has one kind of rail (`[path]` type 2) and one flag for everything that runs on it. The
editor cannot be changed, so openOMSI reads what a map maker says in three places and works the
rest out. Nothing here changes how a stock map or a mod loads: a map without an
`openomsi-rail.cfg` is read as before, with its tracks classed by their names and their
surroundings.

## Vehicle classes

`RailClass` (`omsi_sim::rail`) is `tram`, `metro` or `train`. A vehicle is a rail vehicle when
its file says so as OMSI 2 does (`[type] 2`, `[boogies]`, `[contact_shoe]` or `[rail_body_osc]`);
which class it is comes from, in this order:

1. `[rail_class]` in the `.ovh` / `.bus` (an openOMSI keyword, followed by `tram`, `metro` or
   `train`);
2. a `[contact_shoe]`: a third rail, so a metro;
3. a car over 45 t, or a vehicle over 60 t with four driven axles: a train (length says nothing:
   a 42 m low-floor tram is lighter than a 20 m coach);
4. otherwise a tram.

The game never takes a bus for a tram or a tram for a bus: a road vehicle has no class, a rail
vehicle always has one.

## Track

Every rail lane gets a `TrackInfo`: street track (rails in or beside a road) or mainline track,
whether a third rail runs beside it, whether it is in a tunnel, and a speed limit.

A map maker tells the game what a track is by, in this order:

1. **A rule** in `<map>/openomsi-rail.cfg`:

   ```
   # a rule applies to every rail lane it matches; later rules win over earlier ones
   [track]
   file       = Splines/MyMap/tram_*.sli   # the spline profile or the object file (a glob; * and ?)
   tile       = 12 5                       # optional: only on this tile (its grid position)
   id         = 4711                       # optional: only this spline or object
   path       = 1                          # optional: only this [path] of it
   class      = tram                       # tram | metro | train: who the track is for
   street     = yes                        # yes: in the street; no: railway
   third_rail = no
   tunnel     = no
   speed      = 40                         # km/h
   ```

   A rule can also just give a turnout's diverging branch its limit (`tile`, `id`, `path`,
   `speed`).
2. **The name of the spline profile or object**, so that the vanilla editor is enough: a name
   with `tram`, `strassenbahn` or `tramway` is tram track in the street; `metro`, `ubahn`,
   `u-bahn`, `subway` metro track with a third rail; `train`, `sbahn` or `railway` railway.
   A generic name such as `rail_typ001` or `gleis` says nothing, the surroundings (3) decide. Pick or copy a profile with the right name in the editor, and the game
   knows.
3. **Where it lies**: beside a street lane it is tram track, a bend tighter than 60 m is too
   (no railway bends so), anything else is railway.

Each rail lane's speed limit is its own (`[rule] speedlimit`, or the file's `speed`), brought
down to what its tightest curve allows: the speed at which the sideways acceleration is
0.8 m/s² in a street and 1.4 m/s² on a railway (whose outer rail is raised). A turnout's
diverging branch is a curve, so it limits itself.

A vehicle on a track that is not for it (a train in a street, a metro without a third rail) is
reported in the log; it is not forbidden, because mod maps lay their tracks as they like.

## Switches

A switch is an object of the map (its `.sco`) with two or more rail paths leaving one. A
driver's vehicle takes, at a fork, the branch the indicator points to (left, right), else the
one the object's switch is set to, else the straightest. It throws the switches it runs over, so
they move with it; a switch a vehicle runs into from the branch side (trailing) takes it along
whatever it is set to. The AI trains take the branch their route names. Their signals ask
for them from 600 m (`RAIL_ASK_DISTANCE`) unless the light's own program says otherwise.

## Pantographs

A rail vehicle with a pantograph moves only with it raised: no current, no drive in either
direction (the brakes still work). A pantograph the file declares always counts; one the game
found from the model counts only when it also found the switch that raises it (else a mistaken
find could keep a vehicle from ever moving). The game knows which variable moves the pantograph:

1. the vehicle's own declaration in its `.bus` / `.ovh`:

   ```
   [pantograph_var]
   Panto_anim_oben            # 0 down, 1 raised
   [pantograph_trigger]
   cp_Panto_toggle            # the mouse-event trigger that raises and lowers it
   ```
2. else the meshes whose file names say pantograph (`PANT_1.o3d`, `Pantograf`,
   `Stromabnehmer`, `Токоприемник`) and the variable their animations turn on. A mod made from
   a bus's scripts (the Tatra T6B5 of Gladbeck) animates its pantograph with `door_0`, the
   door the script already knows how to open and close; the game finds that, takes `door_0`
   out of the doors (no door key, no passengers boarding, no "door open while driving" for the
   career) and fires the door's own `bus_door_0` / `bus_door_0_close` as the pantograph's
   switch. The trigger is found from the script: the one that sets the variable, or sets the
   door leaf of that name.

**Shift+P** raises or lowers the pantograph of the vehicle you drive (a bus script's door
needs its stop brake on, as the door did). `OMSI_DEBUG_RAIL=1` logs the pantograph's value
with the drive twice a second.

With the setting "Realistic road physics" on, a raised pantograph rises until its head meets the contact wire and stops there: the head's
travel is measured on the model, the animation is shown the travel at which the head touches, and
current is brought only while the head lies on a live wire (a lowered one, or a wire out of reach,
gives none). The wire's height is the class's standard one kept within the pantograph's reach, so
that a map without a drawn wire never strands a tram. Where the wire is uneven at speed (rougher in
wet and frost, harder at forks) the head leaves it; a break under current strikes an arc.
`OMSI_DEBUG_RAIL=1` logs the head, the wire, the contact and the current.

## Body on its springs

With the same setting, `omsi_sim::rail::body`: heave, pitch and roll on the secondary suspension, pushed by the bend's
lateral acceleration, the traction and the brake, and the rails' unevenness at each bogie.

## Movement

The track says where a vehicle is; the rail dynamics (`omsi_sim::rail::dynamics`) say how fast
it goes. The forces are those of railway engineering:

| Force | Model |
| --- | --- |
| Running resistance | `m g (a + b v)` by class: tram 3.5 N/kN, metro 2.5, train 1.5, rising with speed; 1.8 times that for a train that stands |
| Air | `½ ρ (Cd A + 0.4 m² per car) v²`, doubled in a tunnel; `ρ` from the temperature |
| Curve | Röckl: `650 / (R - 55)` N/kN above 300 m, `500 / (R - 30)` below |
| Grade | the weight times the slope under the vehicle, in the direction it faces |
| Adhesion | Curtius and Kniffler `0.161 + 7.5 / (v + 44)` (km/h); 0.7 on wet rails, 0.4 on leaves or snow, 0.2 on ice |
| Drive | the scripts' `M_Wheel` over the driven wheels' radius, no more than adhesion times the weight on driven axles; the driven wheels spin beyond it |
| Brakes | the scripts' `Axle_Brakeforce_*` of all cars, no more than 0.9 of adhesion times the weight; beyond it the wheels slide and hold 0.75 of it |
| Track brake | a tram's emergency stop adds a magnetic track brake that adhesion does not limit |
| Mass | the whole consist, with 6 % added for the rotating parts |

A vehicle whose scripts give no drive (the stock trains are scripted for the AI only) is moved
by the pedals with its class's typical performance: a tram 14 kW/t, 1.2 m/s² at the start,
1.3 m/s² of service brake and 2.8 of emergency brake; a metro 14 kW/t, 1.1 and 1.2/1.8 m/s²; a
train 9 kW/t, 0.9 and 0.9/1.3 m/s².

## Road vehicles

The road vehicles' own physics (`omsi_sim::rigid`) are OMSI 2's unless the setting **Realistic
road physics** (Esc menu, Settings; `realistic_road_physics=1` in `settings.cfg`;
`OMSI_REALISTIC_ROAD=1` for one run) is on, which is off by default. On, all of it from the
vehicle's own file and the map:

* The weather's wind on the bus (a crosswind leans and pushes it, a headwind slows it) and an
  air that is not 1.2 kg/m3 - on top of the drag the vehicle's own scripts already put on its
  brakes (every OMSI 2 bus: `bremse.osc`, 9.5 m2 x cw 0.8 x 0.6 v2 for the whole bus): nothing is
  counted twice, in still air of the scripts' density the body adds nothing.
* Each tyre's grip and rolling resistance by what it rolls on (`Axle_SurfaceID_`: asphalt,
  concrete, cobblestone, dirt, grass, gravel, snow, deep snow), wet or dry; rolling resistance
  also rises with speed and the load on the tyre.
* The road's unevenness by surface after ISO 8608 (the spectrum of road profiles), so
  cobbles, gravel, grass and the joints of an old concrete-slab road shake the bus; the rear
  axle drives over what the front axle drove over a wheelbase before.
* The tyres' side forces from the slip angle (a stretched-string model: relaxation length
  0.9 tyre radii, cornering stiffness 14 per radian of load, friction ellipse with the
  longitudinal force) instead of OMSI's "follows its wheels" rule.

`cargo run -p omsi-sim --example handling -- <file.bus>... --kmh 50 [--realistic]` compares the two.

## Traction, brakes and couplings of a vehicle (openOMSI blocks)

A rail vehicle that is driven by the pedals (its scripts give no drive) can describe how it
pulls, brakes and couples in three blocks of its `.bus` / `.ovh`. OMSI 2 skips them; what a file
leaves out stays the class's typical figure. Vehicles whose scripts give the drive and the brakes
(`M_Wheel`, `Axle_Brakeforce_*`) keep those: the blocks add nothing to them except the coupling
check.

```
[rail_traction]
point 0 80          // km/h  kN: the most the motors pull at that speed (repeat, ascending)
point 40 80
point 90 36
power_kw 800        // the continuous power: the pull never passes P / v (efficiency 0.9 default)
regen_point 10 60   // km/h  kN: the electric brake; none, no electric brake
regen_point 80 60

[rail_brake]
type air            // air | ep | direct
service 1.1         // m/s² at full service (default: the class's)
emergency 1.6       // m/s² in an emergency (default: the class's)
pipe_wave 250       // m/s: how fast the change runs down the brake pipe (air)
apply_time 4        // s for a cylinder to fill; release_time 12 to empty
blend 1             // the electric brake first, the friction brake the rest

[rail_coupler]
type scharfenberg   // screw | buffer | scharfenberg | tightlock
slack 0.03          // m free play per coupling
stiffness 3000      // kN/m
damping 80          // kN s/m
max_force 900       // kN: beyond it the train breaks in two
```

* **Traction curve.** Throttle times the curve's pull at the speed, capped by the power. Past the
  last point the pull falls as 1/v. The adhesion limit of the rails (Curtius-Kniffler, wet,
  leaves, ice, sand) still applies, so a strong motor spins on a leafy rail.
* **Brake.** The handle (the brake pedal) 0 to 0.9 is a service application up to the full
  service deceleration, above it an emergency application. On an air brake the drop in the pipe
  runs down the train as a wave and each car's cylinder fills in `apply_time`, so the tail brakes
  later than the head and a long train stops further on than one braked by wire (`ep`) or directly.
  Releasing is slower than applying. The electric brake of the motors takes what it can at the
  speed (it fades under 3 km/h), the friction brake the rest.
* **Couplings.** The cars are masses with a free play and a draft gear between them: starting takes
  up the slack car by car and jerks, braking from the head runs the cars in. The strongest force of
  a coupling this step is `Outcome::coupler_force`; one pulled past `max_force` breaks the train
  (the log says so, and the brakes come on).

`OMSI_DEBUG_RAIL=1` logs the drive and the brake of a vehicle twice a second.

### Current

A rail vehicle can say where it gets its current:

```
[rail_power]
catenary            // catenary | third_rail | diesel (carries its own power)
```

Left out, a `[contact_shoe]` means third rail, a declared pantograph overhead wire, anything else
no line. On a map whose `transport` folder has a `catenary.cfg` or `thirdrail.cfg`
([TRANSPORT_FORMATS.md](TRANSPORT_FORMATS.md)) the player's vehicle moves only where its collector
finds a live section (the pantograph within 2.5 m of the wire, a shoe within 1.2 m of the rail, not in
a break), and its drive weakens below 80 % of the nominal voltage and is gone at 50 %. The brakes
always work. A map without the file is as before: the pantograph's switch alone decides.
