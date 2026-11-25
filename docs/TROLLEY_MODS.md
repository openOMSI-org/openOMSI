# OMSI 2 trolleybus add-ons

How openOMSI runs trolleybus add-ons made for OMSI 2, which carry no wire logic of their own.
This page is for modders who want to know what the game reads from such content, and how to make
an existing trolleybus or map work well. The native formats are in
[Trolleybuses](TROLLEYBUS.md).

## What OMSI 2 trolleybus content contains

These findings come from reading three real add-ons:

| Add-on | What it is | Control |
| --- | --- | --- |
| ZiU-682G 016.02 (Trolza), remake by Wlad209 | The classic Russian trolleybus | Rheostat-contactor |
| Trolza-5275.03 / .05 "Optima" | A low-floor trolleybus | Thyristor-pulse (IGBT kind) |
| Bronitsyn, by liaz677 | A fictional trolleybus town, route 1 | - |

None of them has a plugin, a DLL or a script that touches the overhead wire. In these add-ons
**the trolleybus is a bus with decoration**. Everything that makes it electric in openOMSI comes
from the game, worked out from what the files do say.

### The buses

**Poles.** Each pole is two meshes in `model.cfg`, a folded one and a raised one (for example
`Штанга левая слож.o3d` and `Штанга левая подн.o3d`, the same for the right). Each has a
`[visible]` on a cockpit variable and a `[mouseevent]` with the same name:

```text
[mesh]  ...Штанга левая слож.o3d     [mouseevent] cp_SHTANGALEV   [visible] cp_SHTANGALEV 0
[mesh]  ...Штанга левая подн.o3d     [mouseevent] cp_SHTANGALEV   [visible] cp_SHTANGALEV 1
(and cp_SHTANGAPRAV for the right pole)
```

* `cockpit.osc` toggles the variable in `{trigger:cp_SHTANGALEV}` / `{trigger:cp_SHTANGAPRAV}`:
  `(L.L.cp_SHTANGALEV) ! (S.L.cp_SHTANGALEV)`.
* Nothing reads the variable: not the drive, not the electrics. The main busbar
  (`elec_busbar_main`) comes from the battery switch alone.
* The raised mesh stands at a fixed pose, about 19°. On the ZiU the hinge is on the roof 0.26 m
  behind the middle, 3.3 m up; the shoe is 6.9 m back and 5.5 m up, 0.2 to 0.3 m either side of
  the centre line. The Optima is the same, with the meshes in `Kuzov/` and the hinges a little
  further out.
* There are no pole angle variables, no contact variable and no voltage.

**Drive.** Both buses use ordinary bus scripts. The ZiU runs `Praga_2m70.osc` with a diesel
engine script. The Optima runs `antrieb.osc` (ZF automatic gearbox, retarder) with a diesel curve
over 100 kW. The "controller", "contactors" and "thyristors" are meshes, textures and sounds
driven by bus script variables. The mains lamp mesh (`Индикатор сети`) is lit by
`elec_busbar_main`.

**Rheostat or chopper.** The only machine-readable difference between the two control types is
the description in the `.bus`: "реостатно-контакторная" against "тиристорно-импульсная". Poles,
variables, script structure and cockpit triggers are the same. A chopper bus draws no starting
current beyond what its power asks and does not click. A rheostat bus draws a starting current
while its controller holds the motors back, and clicks its contactors.

### The map

Bronitsyn draws its wire with **splines**, not with an object per span:

* `Splines/Jonpol/troll_ks.sli` ("ks" stands for contact network) has two profiles. Each is a thin
  strip, 2 cm wide and 2 cm high, 5.04 to 5.06 m above the spline, at -0.26 m and +0.26 m to the
  side. The strips are the two wires; the spline's centre line is where the pair runs.
  `tram_ks.sli` is the same with one strip. 262 splines in the map's 23 tiles use `troll_ks.sli`.
* Hangers and bends are plain scenery objects (`Sceneryobjects/ks/podves*.sco`, `kriva*.sco`),
  98 of them placed one by one. Street lamp poles double as masts.
* A **frog** is three such splines meeting end to end. The map has one, and nothing operates it:
  no switch object, script or variable.
* There are **no dead sections**, insulators or substations. The wire is a picture.
* The map ships a `Vehicles/Ziu` folder with two `.bus` files that borrow the ZiU add-on's models,
  and the map's `.hof` files.

## What openOMSI does with it

### The wire from the map's splines

When the player drives a trolleybus on a map without `trolley.txt`, the game reads the wire from
the map's splines:

* A spline whose profile is two thin, high strips is a pair of wires. The centre is the middle of
  the strips, the spacing their distance, the height their height.
* Three of them ending at one point are a frog. It works in `power` mode: power on at the plate
  before the frog takes the branch, coasting goes straight.
* The whole map is one live section.
* The game draws no wire of its own over the map's. Switches get a generated body, tongue and
  lamp.

A `trolley.txt` next to the map, or in a mod's content root, replaces this. It can set frogs to
`indicator` or `fixed` and add real sections and insulators (see
[The wire: trolley.txt](TROLLEYBUS.md#the-wire-trolleytxt)).

### Poles from the model

A bus without `[trolley_pole]` or `[trolley_none]` blocks gets poles from its model in three
cases.

**1. Poles shown by a cab switch.** Two meshes visible on a variable whose name contains `SHTANG`
(or `штанг`, `pole`) with value 1 are two poles. Each hinge and length comes from its mesh: the
hinge is the low end of the box furthest forward, the shoe the high end furthest back. The
variable is the cab's switch.

**2. Poles always shown.** Two plain meshes always shown, raised (the ZiU-682B that ships with
Bronitsyn: `animation\palkalev.o3d` / `palkaprav.o3d`, no `[visible]`, no animation) are two
poles too. They are recognised by shape:

* under 0.6 m wide;
* on the roof, 2 m up or more;
* at least 3 m long;
* rising 1 m or more towards the back.

Such a bus has no cab switch.

**3. Poles named like poles.** The same shape test takes a pole mesh that the model animates or
shows on another variable, when its file name says pole: `shtang`, `штанг`, `stange`, `palk`,
`палк`, `pole`, `boom`, `szedo`, `szedő`. The Ikarus 280T (GVM) has `GVM\Shtanga_podn_L/R.o3d`
on its rear section, shown by the battery switch `knopka_massa` and turned by
`articulation_0_alpha/beta`. Those conditions and animations are dropped. Rope meshes beside them
(`verevk`, `веревк`, `канат`, `seil` in the name) are hidden, because the game draws the ropes.

### Rheostat or chopper

The game reads the `.bus` text. A bus whose file mentions `тиристор`, `IGBT`, `импульс`,
`асинхрон`, `инвертор`, `транзистор`, `thyristor`, `chopper` or `inverter` is a chopper bus.
Anything else is a rheostat bus. A plain trolleybus takes 3 kW of auxiliary load from the line
(4 kW for a chopper bus).

### The electrics

* The bus's own script keeps driving it. That is the only drive it has.
* The game takes the driving torque away when the poles bring no current.
* It estimates the line current from the force and speed the script asks for, plus the starting
  current of a rheostat bus.

### The cab switches

The cab's switches (`cp_SHTANGALEV` / `cp_SHTANGAPRAV`) still do what the player expects:

* A click, or the key, raises the game's pole: the motor does it and finds the wire.
* While the game's poles are up the variable reads 2. That hides both of the model's meshes, and
  the game's poles are drawn instead.
* The key pulls them down (the script turns the 2 into a 0). Down, the variable is 0 and the
  model's folded poles show.

### Always-shown poles

For a bus whose pole meshes are always shown (no switch):

* The poles are taken from those meshes for the player's bus only. An AI copy keeps them as
  drawn.
* The model's own meshes are turned about their hinges by the pole angles. They fold onto the
  roof when down and follow the wire when up.
* They are raised with **R** in the cab (the motor finds the wire), or by hand from outside.
* Poles on the rear section of an articulated bus belong to the front part's gear (its script,
  its traction) but stand on the rear section. The hinge, the hands and the drawing follow it.

Everything else is the game's own, as for native gear: the poles' mechanics and dewiring, current
needed to move, arcs, the hands, and the sounds (see [Trolleybuses](TROLLEYBUS.md)).

### Keep the game out

A bus whose own script or plugin moves its poles should say `[trolley_none]` in its `.bus`. The
game then reads no poles from its model.

## Tested behaviour

Walk-through on a development build, offscreen, with a fixed seed:

| Check | Result |
| --- | --- |
| ZiU on Bronitsyn, `cp_SHTANGALEV` / `PRAV` clicked at 0.5 s | Both poles rise and are on the wire 2.2 s later. 600 V, circuit closed |
| The same, driving north along the wire | 55 km/h after 36 s. 130 to 260 A: high at the start, falling with speed |
| Dead stretch (test variable) | The bus loses current at the insulator, rolls on at about 49 km/h with 0 V and 0 A, and has power again after it |
| Re-wiring by hand (one pole pulled, raised again) | Works on the map's wire |
| Frog, coasting | Straight on: no event, the poles stay on |
| Frog, power on | The shoes take the branch while the bus goes straight. Both poles come off 1.2 s later, with arcs |
| Optima (chopper) on the same wire | Poles up and on the wire, 7 A (auxiliary load only) |

## Known gaps

* The Optima's own gearbox script did not go into D with the triggers that could be fired
  offscreen, so it was checked standing. This is a gap in the test, not in the trolley code.
* With a `trolley.txt`, the game leaves the map's switch models alone. Only
  `OMSI_DEBUG_TROLLEY` adds a marker, tongue and lamp. Spline-derived wires keep their generated
  switch visuals without it.
* None of these add-ons has real sections or insulators. They were tested with the
  `OMSI_TROLLEY_DEAD_AT` test variable.
* AI trolleybuses (Bronitsyn's route 1 is run by AI) are not powered by the wire.
* The engine of these buses keeps running with the poles down, because their scripts are diesel
  scripts. The bus stops moving, but the engine sound goes on.
