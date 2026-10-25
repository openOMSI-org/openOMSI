//! `object.*`: the scenery objects the plugin drives. A map object type names the script in
//! its `.sco` (`[lua_script]`); the game loads it as a plugin called `object:<file>` and tells
//! it of each placed object of that type (`object_init`, `object_frame`, `object_gone`,
//! `object_click`). The script tells objects apart by their `id`, the map's id of the placed
//! object, and may touch only the objects its own type has.

use super::{def, o, p, rec};
use crate::api::{ApiError, ApiFn, Ctx, Value};
use crate::io::MeshTransform;

const NEW: &str = crate::api::VERSION;

/// The id of an object this plugin drives, or the error a script sees.
fn own(c: &mut Ctx<'_>, id: i64) -> Result<(), ApiError> {
    let name = c.state().name.clone();
    if c.io().object_owned_by(id, &name) {
        Ok(())
    } else {
        Err(ApiError(format!("omsi.object: object {id} is not one this script drives")))
    }
}

/// A table `{x, y, z, rx, ry, rz, sx, sy, sz, px, py, pz}` (every key optional) as a transform.
fn transform(v: &Value) -> MeshTransform {
    let get = |k: &str, d: f64| match v {
        Value::Map(m) => m.iter().find(|(n, _)| n == k).and_then(|(_, x)| x.as_f64()).unwrap_or(d) as f32,
        _ => d as f32,
    };
    MeshTransform {
        pos: [get("x", 0.0), get("y", 0.0), get("z", 0.0)],
        rot: [get("rx", 0.0), get("ry", 0.0), get("rz", 0.0)],
        scale: [get("sx", 1.0), get("sy", 1.0), get("sz", 1.0)],
        pivot: [get("px", 0.0), get("py", 0.0), get("pz", 0.0)],
    }
}

pub static FNS: &[ApiFn] = &[
    def!("object.list", "object", [], "list of numbers", "The ids of the objects this script drives that are loaded now (those within its range are active).", NEW, None, false, |c, a| {
        let name = c.state().name.clone();
        Value::List(c.io().object_list(&name).into_iter().map(|i| Value::from(i)).collect())
    }),
    def!("object.info", "object", [p("id", "integer")], "table", "`{id, type, x, y, z, heading, active, distance}` of an object: its type (the `.sco` file's name), where it stands (map coordinates), the way it faces (compass degrees), whether it is in the script's range and how far the listener is from it (m).", NEW, None, false, |c, a| {
        let id = a.int(0)?;
        own(c, id)?;
        Ok::<_, ApiError>(c.io().object_info(id).map(|i| rec(vec![("id", i.id.into()), ("type", i.kind.into()), ("x", i.pos[0].into()), ("y", i.pos[1].into()), ("z", i.pos[2].into()), ("heading", Value::from(i.heading)), ("active", i.active.into()), ("distance", i.distance.into())])))
    }),
    def!("object.var", "object", [p("id", "integer"), p("name", "string")], "number or nil", "A script variable of an object (those its `[varnamelist]` declares, and the builtin ones: `Switch`, `InUse`, `NightlightA` ...); `nil` when it has none of that name.", NEW, None, false, |c, a| {
        let (id, name) = (a.int(0)?, a.str(1)?);
        own(c, id)?;
        Ok::<_, ApiError>(c.io().object_var(id, &name).map(|v| v as f64))
    }),
    def!("object.set_var", "object", [p("id", "integer"), p("name", "string"), p("value", "number")], "boolean", "Sets a script variable of an object, which its `[newanim]` animations, `[visible]` conditions, materials, lamps and sounds read; false when it has none of that name.", NEW, None, false, |c, a| {
        let (id, name, v) = (a.int(0)?, a.str(1)?, a.num(2)?);
        own(c, id)?;
        Ok::<_, ApiError>(v.is_finite() && c.io().object_set_var(id, &name, v as f32))
    }),
    def!("object.set_vars", "object", [p("id", "integer"), p("values", "table")], "integer", "Sets many variables of an object at once (a table name -> number); how many it has.", NEW, None, false, |c, a| {
        let id = a.int(0)?;
        own(c, id)?;
        let mut n = 0;
        if let Value::Map(m) = a.table(1)? {
            for (k, v) in m {
                if let Some(x) = v.as_f64().filter(|x| x.is_finite()) {
                    n += c.io().object_set_var(id, k, x as f32) as i64;
                }
            }
        }
        Ok::<_, ApiError>(n)
    }),
    def!("object.str", "object", [p("id", "integer"), p("name", "string")], "string or nil", "A string variable of an object.", NEW, None, false, |c, a| {
        let (id, name) = (a.int(0)?, a.str(1)?);
        own(c, id)?;
        Ok::<_, ApiError>(c.io().object_str(id, &name))
    }),
    def!("object.set_str", "object", [p("id", "integer"), p("name", "string"), p("value", "string")], "boolean", "Sets a string variable of an object (a display's text, a texture's name).", NEW, None, false, |c, a| {
        let (id, name, v) = (a.int(0)?, a.str(1)?, a.str(2)?);
        own(c, id)?;
        Ok::<_, ApiError>(c.io().object_set_str(id, &name, &v))
    }),
    def!("object.trigger", "object", [p("id", "integer"), p("name", "string")], "boolean", "Runs a `{trigger:name}` of the object's own `.osc` script; false when it has none.", NEW, None, false, |c, a| {
        let (id, name) = (a.int(0)?, a.str(1)?);
        own(c, id)?;
        Ok::<_, ApiError>(c.io().object_trigger(id, &name))
    }),
    def!("object.meshes", "object", [p("id", "integer")], "list of strings", "The mesh files of an object, in the order the scripts number them from 1.", NEW, None, false, |c, a| {
        let id = a.int(0)?;
        own(c, id)?;
        Value::List(c.io().object_meshes(id).into_iter().map(Value::from).collect())
    }),
    def!("object.set_mesh", "object", [p("id", "integer"), p("mesh", "string"), o("transform", "table")], "boolean", "Moves, turns and scales one mesh of an object over what its animations do. `mesh` is its number from 1 or its file name; `transform` is `{x, y, z, rx, ry, rz, sx, sy, sz, px, py, pz}`: a move in metres, a turn about the x, y and z axes in degrees (z up, positive counter-clockwise seen from above), a scale, all about the point (px, py, pz) of the mesh, every key optional. Without a table the mesh is as the model has it. Objects with a script are redrawn every frame; moving a mesh costs nothing more.", NEW, None, false, |c, a| {
        let (id, mesh) = (a.int(0)?, a.str(1)?);
        own(c, id)?;
        let t = a.opt_table(2)?.map(transform);
        Ok::<_, ApiError>(c.io().object_set_mesh(id, &mesh, t))
    }),
    def!("object.set_visible", "object", [p("id", "integer"), p("mesh", "string"), o("visible", "bool")], "boolean", "Shows or hides one mesh of an object whatever its `[visible]` conditions say; without `visible` the conditions decide again.", NEW, None, false, |c, a| {
        let (id, mesh) = (a.int(0)?, a.str(1)?);
        own(c, id)?;
        let v = match a.get(2) {
            Value::Nil => None,
            v => Some(v.truthy()),
        };
        Ok::<_, ApiError>(c.io().object_set_visible(id, &mesh, v))
    }),
];
