# Suspension types

A vehicle can give each axle a different kind of suspension: leaf springs, air bellows or a
progressive coil. This page is for vehicle modders.

OMSI 2 has one kind: a coil spring with a damper, read from each axle's `achse_feder`,
`achse_daempfer` and `achse_maxforce`. openOMSI keeps exactly that for every axle without a
`[suspension]` block. OMSI 2 skips the block, so the axle stays a coil strut there.

## Example

A bus with air suspension at the front and leaf springs at the rear. Add to the `.bus`:

```text
[suspension]
axle 0
type air

[suspension]
axle 1
type leaf
friction 3000
```

## Keys

Each line of the block is `key value`. Text after `//` is a comment.

| Key | Meaning | Default |
|---|---|---|
| `axle` | Axle index in the order of the `[newachse]` blocks, 0 = first | All axles |
| `type` | `coil`, `progressive`, `leaf` or `air` | `coil` |
| `friction` | Dry friction of the leaves, in N (leaf only) | 1500 |
| `progression` | Extra stiffness at 10 cm of compression, as a fraction of the spring rate | Depends on the type |
| `level_time` | Time constant of the levelling valve, in s (air only, 0.2-120) | 4 |

- The last block that names an axle wins.
- A block without `axle` is the default for every axle no other block names.
- The spring rate (`achse_feder`), the damper and the maximum force stay those of the axle. The
  type only changes how they act.
- `kind` is accepted instead of `type`. An unknown type is a coil.

## Types

| Type | Also accepted | Behaviour | Default `progression` |
|---|---|---|---|
| `coil` | `spring`, `steel` | OMSI 2's linear strut, unchanged | 0 |
| `progressive` | | The spring stiffens with compression | 0.5 |
| `leaf` | `leaf_spring`, `ressort` | Stiffens, and the leaves add dry friction to the damper | 0.8 |
| `air` | `airbag`, `bellows`, `pneumatic` | Air bellows with a levelling valve | 0.35 |

### Leaf springs

The dry friction of the leaves damps even small movements. A leaf-sprung axle does not rock back
and forth on a smooth road.

### Air suspension

The levelling valve holds the empty vehicle's ride height whatever the load. It fills or bleeds
the bellows with the time constant `level_time`, so bumps pass by too fast for it while loading
is compensated. The bellows stiffen with compression.

Scripts still scale the spring rate with `Axle_Springfactor_*`, for kneeling and raising.
