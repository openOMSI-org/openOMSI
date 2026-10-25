# Suspension types

OMSI 2 has one strut: a coil spring with a damper, read from each axle's `achse_feder`,
`achse_daempfer` and `achse_maxforce`. openOMSI keeps that exactly for every axle without a
`[suspension]` block, and lets a vehicle give any axle another kind. OMSI 2 skips the block, and
the axle there is the coil strut.

```
[suspension]
axle 1               // index in the order of the [newachse] blocks (0 = the first); left out: all axles
type leaf            // coil | leaf | air | progressive
friction 2500        // N, dry friction of the leaves (leaf; 1500 when left out)
progression 0.8      // extra stiffness at 10 cm of compression, as a fraction of the rate
level_time 4         // s, the levelling valve's time constant (air)
```

The last block that names an axle wins; a block with no `axle` is the default for the axles no
other block names. The spring rate (`achse_feder`), the damper and the maximum force stay those of
the axle; the kind changes how they act.

| Type | What it does |
| --- | --- |
| `coil` | OMSI 2's strut, linear, no change |
| `progressive` | the spring stiffens with compression (`progression`, default 0.5) |
| `leaf` | stiffens (`progression` 0.8) and adds the dry friction of the leaves to the damper: it damps even small movements and does not creak back and forth on a smooth road |
| `air` | air bellows with a levelling valve: the ride height of the empty vehicle is held whatever the load (the valve fills or bleeds the bellows with time constant `level_time`, so bumps pass it by, loading does not); the bellows stiffen with compression (`progression`, default 0.35). Scripts still scale the rate with `Axle_Springfactor_*`, for kneeling and raising |

Example, a bus with air at the front and leaf springs behind:

```
[suspension]
axle 0
type air

[suspension]
axle 1
type leaf
friction 3000
```
