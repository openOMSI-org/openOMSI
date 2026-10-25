# Scripts of map objects

A scenery object (`.sco`) can name Lua or compiled (`.oop`) scripts that animate it and run its
logic. This is an openOMSI extension; OMSI 2 skips the keywords, so an object that uses them
still loads there (and simply does not move). The existing `[script]` (`.osc`) and `[newanim]`
animations keep working next to them.

```text
[lua_script]
gate.lua        # path relative to the .sco; repeat the block for more scripts
[lua_range]
150             # how close the camera must be (m); 0 or absent: 250 m, at most 800 m
[lua_rate]
30              # frames per second of the script; 0 or absent: every game frame
```

One script instance serves all objects of the type that name the file; it learns about the
objects by events and tells them apart by `id` (the object's id in the map). Example:
[examples/object-script](examples/object-script).

## Events

| Event | Arguments | When |
| --- | --- | --- |
| `object_init` | `id, type, x, y, z, heading` | an object of the script comes into range for the first time |
| `object_frame` | `id, dt` | every frame (or `[lua_rate]`) of an object in range; `dt` is the time since its last frame |
| `object_click` | `id, event` | the player clicks the object |
| `object_gone` | `id` | the object leaves the range or is unloaded |

## Functions

The `omsi.object.*` functions are listed in [PLUGINS.md](PLUGINS.md#scenery-objects-for-scripts-a-sco-names).
A script can use them only for the objects it drives; for any other id they raise an error or
return `nil`. Besides them it has the audio functions (`omsi.audio.*`); it cannot reach the
vehicle, the UI or files.

* Variables: `object.var`/`set_var` read and write the object's script variables (those of its
  `[varnamelist]` and the builtin ones); its `[newanim]` animations, `[visible]` conditions,
  lamps and sounds react to them, so a gate can be animated by the `.sco` itself and only
  decided by the script.
* Meshes: `object.set_mesh(id, mesh, {x, y, z, rx, ry, rz, sx, sy, sz, px, py, pz})` lays a
  transform over the mesh. The axes are those of the model (x right, y forward, z up), angles
  in degrees, the turn happens about `(px, py, pz)` of the mesh, the order is scale, turn
  (x, then y, then z), move.
* `object.set_visible(id, mesh, visible)` overrides the `[visible]` conditions; call it without
  `visible` to hand the decision back.
* An object with a Lua script is redrawn every frame while in range; a far-away one costs nothing.
