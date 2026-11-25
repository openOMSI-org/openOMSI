# Object scripts

A scenery object (`.sco`) can run a Lua or compiled (`.oop`) script that animates it and runs its
logic: a barrier that lifts, a sign that changes, a door that opens on a click. This page is for
modders who make map objects.

This is an openOMSI extension. OMSI 2 skips the keywords, so the object still loads there but does
not move. OMSI's own `[script]` (`.osc`) and `[newanim]` animations keep working next to it.

## Example: a barrier

The complete example is in `docs/examples/object-script/` of the openOMSI repository.

`Gate.sco`:

```text
[mesh]
post.o3d

[mesh]
arm.o3d

[varnamelist]
1
gate_open

[lua_script]
gate.lua

[lua_range]
150

[lua_rate]
30
```

`gate.lua`:

```lua
-- A barrier arm that turns up to 90 degrees about the point (0, 0, 1.2).
local state = {}

omsi.on("object_init", function(id, kind, x, y, z, heading)
  state[id] = { angle = 0 }
end)

omsi.on("object_frame", function(id, dt)
  local s = state[id]
  if not s then return end
  local wanted = (omsi.object.var(id, "gate_open") or 0) * 90
  local step = 60 * dt
  if s.angle < wanted then s.angle = math.min(s.angle + step, wanted)
  elseif s.angle > wanted then s.angle = math.max(s.angle - step, wanted) end
  omsi.object.set_mesh(id, "arm", { ry = -s.angle, pz = 1.2 })
end)

omsi.on("object_click", function(id, event)
  local open = omsi.object.var(id, "gate_open") or 0
  omsi.object.set_var(id, "gate_open", 1 - open)
end)

omsi.on("object_gone", function(id) state[id] = nil end)
```

## Keywords

| Keyword | Value | Default |
|---|---|---|
| `[lua_script]` | Path of a `.lua` or `.oop` file, relative to the `.sco`. Repeat the block for more scripts. | None |
| `[lua_range]` | How close the camera must be for the script to run, in m. At most 800. | 250 (also for 0) |
| `[lua_rate]` | How many times per second the script's frame runs | Every game frame (also for 0) |

## How it runs

- One script instance serves every object of the type that names the file.
- The script tells objects apart by `id`, the object's id in the map.
- It can control only the objects of its own type. For any other id, the functions raise an
  error or return `nil`.
- An object with a script is redrawn every frame while in range. An object out of range costs
  nothing.

## Events

| Event | Arguments | When |
|---|---|---|
| `object_init` | `id, type, x, y, z, heading` | An object comes into range for the first time |
| `object_frame` | `id, dt` | Every frame (or at `[lua_rate]`) while the object is in range. `dt` is the game time since its last frame. |
| `object_click` | `id, event` | The player clicks a mesh of the object that has a `[mouseevent]` |
| `object_gone` | `id` | The object leaves the range or is unloaded |

`heading` is in compass degrees: north 0, east 90.

## Functions

| Function | Returns | What it does |
|---|---|---|
| `omsi.object.list()` | List of ids | The loaded objects of this script |
| `omsi.object.info(id)` | Table | `{id, type, x, y, z, heading, active, distance}`: type is the `.sco` name, `active` is in range, `distance` from the listener in m |
| `omsi.object.var(id, name)` | Number or `nil` | A script variable: from its `[varnamelist]`, or a builtin (`Switch`, `InUse`, `NightlightA` …) |
| `omsi.object.set_var(id, name, value)` | Boolean | Sets a script variable |
| `omsi.object.set_vars(id, values)` | Count | Sets several variables from a table `name → number` |
| `omsi.object.str(id, name)` | String or `nil` | A string variable |
| `omsi.object.set_str(id, name, value)` | Boolean | Sets a string variable |
| `omsi.object.trigger(id, name)` | Boolean | Runs a `{trigger:name}` of the object's `.osc` script; false when it has none |
| `omsi.object.meshes(id)` | List of strings | The object's mesh files, numbered from 1 |
| `omsi.object.set_mesh(id, mesh, transform)` | Boolean | Moves, turns and scales a mesh (below) |
| `omsi.object.set_visible(id, mesh, visible)` | Boolean | Shows or hides a mesh |

The script also has the audio functions (`omsi.audio.*`). It cannot reach the vehicle, the user
interface or files. The full API is in the [plugin API reference](PLUGIN_API.md).

### Variables

`set_var` writes the object's script variables. Its `[newanim]` animations, `[visible]`
conditions, lamps and sounds react to them. So a gate can be animated by the `.sco` itself, and
the script only decides when.

### Moving meshes

`omsi.object.set_mesh(id, mesh, {x, y, z, rx, ry, rz, sx, sy, sz, px, py, pz})` lays a transform
over what the mesh's animations do.

- `mesh` is the mesh's number from 1, or its file name.
- `x, y, z` move the mesh in metres. Axes are the model's: x right, y forward, z up.
- `rx, ry, rz` turn it about the x, y and z axes, in degrees. Positive `rz` is
  counter-clockwise seen from above.
- `sx, sy, sz` scale it.
- `px, py, pz` is the pivot point of the turn and scale.
- The order is scale, turn (x, then y, then z), move. Every key is optional.
- Without a table, the mesh returns to what the model says.

### Showing and hiding meshes

`omsi.object.set_visible(id, mesh, visible)` overrides the mesh's `[visible]` conditions. Call it
without `visible` to hand the decision back to the conditions.
