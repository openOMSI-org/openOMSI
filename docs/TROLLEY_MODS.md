# Real OMSI 2 trolleybus content: how it is made, and how openOMSI runs it

What this records was read off three real add-ons, with their files only read, never run:

| Add-on | What it is | Control |
|---|---|---|
| ЗиУ-682Г 016.02 (Trolza), remake by Wlad209, 136 MB | The old Russian trolleybus | rheostat-contactor ("реостатно-контакторная") |
| Trolza-5275.03 / .05 "Optima", 96 MB | The new low-floor trolleybus | thyristor-pulse ("тиристорно-импульсная"), the IGBT kind |
| Bronitsyn ("БРОНИЦЫН"), by liaz677, 305 MB | A fictional trolleybus town, route 1 | - |

None of them carries a plugin, a DLL or a script that touches the overhead wire. (There is no
`.dll` in any of the three archives.) That is the central finding: **in these add-ons the
trolleybus is a bus with a decoration**, and everything that makes it electric in openOMSI
has to come from the host, read off what the files do say.

## The buses

**Poles.** Each pole is two meshes in the model.cfg (the Russian names say "folded" and
"raised": `Штанга левая слож.o3d`, `Штанга левая подн.o3d`, and the same for the right), each with a
`[visible]` on a cockpit variable and a `[mouseevent]` on the same name:

```
[mesh]  ...Штанга левая слож.o3d     [mouseevent] cp_SHTANGALEV   [visible] cp_SHTANGALEV 0
[mesh]  ...Штанга левая подн.o3d     [mouseevent] cp_SHTANGALEV   [visible] cp_SHTANGALEV 1
(and cp_SHTANGAPRAV for the right pole)
```

`cockpit.osc` has `{trigger:cp_SHTANGALEV}` / `{trigger:cp_SHTANGAPRAV}` toggling the variable
(`(L.L.cp_SHTANGALEV) ! (S.L.cp_SHTANGALEV)`), and **nothing reads it**: not the drive, not the
electrics (`elec_busbar_main` comes from the battery switch alone). The raised mesh stands at a
fixed pose (about 19 degrees: hinge on the roof 0.26 m behind the middle, 3.3 m up, the shoe
6.9 m back and 5.5 m up, 0.2 to 0.3 m either side of the centre line). The Optima does the same,
only with the meshes in `Kuzov/` and the hinge a little further out. There are no pole angles
as variables, no contact variable, no voltage.

**Drive.** Both are ordinary bus scripts: the ZiU runs `Praga_2m70.osc` and a diesel engine
script (an engine file for a Raba diesel), the Optima `antrieb.osc` (ZF automatic gearbox,
retarder) with a 100+ kW diesel curve. "Controller", "contactors" and "thyristors" are
meshes, textures and sounds (`Дугогасящие контакторов`, `Индикатор сети`, the thyristor sound
that a key adds - see the readme of the Optima), driven by script variables of the bus,
not by electrics. The mesh `Индикатор сети` (the mains lamp) is lit by `elec_busbar_main`.

**What differs between the two drive types, as the files have it.**
The *only* machine-readable difference is the description in the `.bus` (and the textures and
sounds): "реостатно-контакторная" against "тиристорно-импульсная". The poles, their
variables, the scripts' structure, the cockpit triggers are the same. A host that wants to
treat them differently must read the description (we do: see below) or take the
`[trolley_rheostat]` / `[trolley_async]` blocks of this host's own format. A pulse-controlled
bus draws no starting current beyond what its power asks and does not click; a rheostat bus
draws a starting current while its controller holds the motors back and clicks its contactors.

## The map

Bronitsyn draws its wire with **splines**, not with an object per span:

* `Splines/Jonpol/troll_ks.sli` ("ks" - контактная сеть, the contact network): two profiles, each
  a thin strip (2 cm wide, 2 cm high) at height 5.04 to 5.06 m over the spline, at lateral
  positions -0.26 and +0.26. The strips are the two wires; the spline's centre line is where
  the pair runs. `tram_ks.sli` is the same with one strip (a tram's one wire). 262
  `[spline]` records in the map's 23 tiles use `troll_ks.sli`.
* The hangers and bends are plain scenery objects (`Sceneryobjects/ks/podves*.sco`,
  `kriva*.sco`: mesh and `[nocollision]`, group "Контактная сеть троллейбуса"), 98 of them
  placed singly. Poles of the street lamps double as masts.
* A **frog** is three of these splines meeting end to end (one toe, a straight and a branch); the
  map has one (at 1252, 1287, a junction by a playground). Nothing operates it: the map has no
  switch object, no script and no variable for it.
* There are **no dead sections**, insulators or substations: the wire is a picture.
* The map ships a `Vehicles/Ziu` folder with two `.bus` files that borrow the models of the
  ZiU add-on (they name `model\model_682b.cfg`, which is not in the map), and the map's
  `.hof` files.

## What a host must provide to make this content work

This is the interface the real content leaves to its host (openOMSI provides all of it):

1. **Read the wire off the map**: any spline whose profile is two thin, high strips is a pair
   of wires (centre = the middle of the strips, spacing = their distance, height = their height);
   three of them ending at one point are a frog.
2. **Make the poles from the model**: a bus that has no `[trolley_pole]` blocks but two
   meshes visible on a variable called `*SHTANG*` / `*pole*` with value 1 has two poles, whose
   hinges and lengths are those meshes' (hinge: the low end of the box furthest forward; shoe:
   the high end furthest back). The variable is the cab's switch.
   A bus whose poles are two plain meshes always shown, raised (the ZiU-682B that ships with
   Bronitsyn: `animation\palkalev.o3d` / `palkaprav.o3d`, no `[visible]`, no animation), has two
   poles too, told by the meshes' shape: under 0.6 m wide, on the roof (2 m up or more), at
   least 3 m long and rising 1 m or more towards the back. It has no cab switch.
   The same shape test takes a pole mesh that its model animates or shows on another variable
   when its file is named like a pole (`shtang`, `stange`, `palk`, `pole`...): the Ikarus 280T
   (GVM) has `GVM\Shtanga_podn_L/R.o3d` on its rear section, shown by the battery switch
   `knopka_massa` and turned by `articulation_0_alpha/beta` to look as if they followed a wire.
   Those conditions and animations are dropped, and the rope meshes beside them (`Verevka_*`)
   are hidden (the game draws the ropes).
3. **Make the electrics from the poles**: the bus's own script keeps driving it (that is the only
   drive it has); the host takes its driving torque away when the poles do not bring current,
   and says how much current it draws from what the script asks.
4. Everything else a real trolleybus does - the pole's spring, its coming off, arcs, dead
   sections, switches by current or indicator - is the host's own, layered over this.

## What openOMSI does

* `omsi_scene::trolley_derive` reads a map's wire splines as above (for any map, only when a
  trolleybus is the player's vehicle; `trolley.txt` beside the map, in an overlay root or named
  by `OMSI_TROLLEY_FILE` overrides it). Frogs are found where three ends meet, in `power` mode
  (power on at the plate before the frog takes the branch, coasting goes straight; `trolley.txt`
  can say `indicator` or `fixed`). The game draws no wire of its own over the map's (`drawn_by_map`).
  The whole map is one live section; `OMSI_TROLLEY_DEAD_AT=x,y,r;...` makes a dead stretch (with an
  insulator at each end) for a test, and a `trolley.txt` can have the real ones.
* `omsi_sim::vehicle::types::legacy_trolley` makes the poles of a plain bus (see 2 above) and
  reads rheostat or chopper off its description (`реостат`, `тиристор`, `IGBT`, `импульс`,
  `асинхрон`, `инвертор`...). The cab's switches `cp_SHTANGALEV`/`PRAV` stay the way the player
  asks for the poles: a click or the key (1) raises ours (the motor does it, finding the wire),
  and while ours are up the variable reads 2, which hides both of the model's meshes (ours are drawn
  instead); the key (the script turns the 2 into a 0) pulls them down; down, the variable is 0 and
  the model's folded poles show.
* A bus with always-shown pole meshes (no switch) gets its poles from those meshes, for the
  player's bus only (an AI copy keeps them as they are drawn). The model's own meshes are turned
  about their hinges by the pole angles, so they fold on the roof when down and follow the wire
  when up. They are raised with R in the cab (the motor finds the wire), or by hand from outside.
  Poles on the rear section of an articulated bus belong to the front part's gear (its script,
  its traction), and stand on the rear section: the hinge, the hands and the drawing follow it.
* Everything of the first version stays: the poles' mechanics and dewiring, current needed to
  move, arcs, the hands (R/T from outside, the same keys; scripted by `OMSI_TROLLEY_HANDS`),
  sound. With the script's own drive the line current is estimated from the force and speed
  the script asks for, plus the starting current of a rheostat bus.

## Walk-through (offscreen, dev build, `OMSI_SEED=1`)

| Check | Result |
|---|---|
| ZiU on Bronitsyn, `cp_SHTANGALEV`/`PRAV` clicked at 0.5 s | both poles rise, on the wire 2.2 s later, 600 V, circuit true |
| The same, driving north along the wire from (181, 110) | 55 km/h after 36 s; 130 to 260 A (start current high, falling with speed) |
| Dead stretch (181, 330, r 25) | bus loses current at the insulator (y 304), 0 V 0 A across, rolls on at about 49 km/h, power again at y 355 |
| Hand re-wiring (pole 1 pulled, circuit dead, raised again) | works on the real wire, see below |
| Frog at (1252, 1287), coasting | straight: no event, poles stay on |
| Frog, power on | plate at y 1278, shoes take the branch, the bus goes straight on, both poles come off 1.2 s later (out of reach) with arcs |
| Optima (chopper) on the same wire | poles up and on the wire, 7 A (the aux load only) |

Images read: `real/evidence/ev_13.5.png` (ZiU, poles on the map's wire), `h_13.0.png` (re-raising by hand),
`fr_7.6.png` (the dewiring flash at the frog).

## What does not work, and why

* The Optima's own gearbox script did not go into D with the triggers I could fire offscreen
  (`automatic_N/D` need a state of its own scripts), so it was not driven under power: its poles,
  circuit and current were checked standing. This is a test-recipe gap, not a trolley one.
* The map's frog is a graphical junction; our frog body, tongue and lamp are drawn over it.
* No add-on here has real sections or insulators; they are tested with `OMSI_TROLLEY_DEAD_AT`.
* AI trolleybuses (the map's route 1 is run by AI vehicles) are not powered by the wire.
* The engine of these plain buses keeps running with the poles down (their scripts are diesel
  scripts): the bus stops moving, the engine sound goes on.
