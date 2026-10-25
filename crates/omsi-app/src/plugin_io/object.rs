//! The scenery objects Lua scripts drive (`[lua_script]` of a `.sco`): which objects are in
//! range and told of it (`object_init`, `object_frame`, `object_gone`), and the game's side of
//! the `object.*` functions.

use crate::App;
use glam::{Mat4, Vec3};
use hashbrown::{HashMap, HashSet};
use omsi_plugin::api::Value;
use omsi_plugin::{MeshTransform, ObjectInfo, Plugins};
use std::path::PathBuf;

/// What the driver remembers of an object in range.
struct Known {
    /// Game seconds since the object's scripts last had a frame.
    since: f32,
    plugins: Vec<String>,
}

/// The objects whose scripts run, and the scripts that would not start.
#[derive(Default)]
pub(crate) struct ObjectScripts {
    known: HashMap<i64, Known>,
    failed: HashSet<PathBuf>,
}

/// The plugin name a script file runs under.
fn plugin_name(path: &std::path::Path) -> String {
    omsi_plugin::object_script_name(path)
}

/// Tell the scripts of the objects near the camera what happens to them. Called before the
/// plugins' frame, so that they hear it in the same frame.
pub(crate) fn drive(app: &mut App, plugins: &mut Plugins, dt: f32) {
    // the objects that have scripts, in range or not
    let Some(center) = app.camera.as_ref().map(|c| c.position).or_else(|| app.player.as_ref().map(|p| p.vehicle.position)) else { return };
    struct Seen {
        id: i64,
        kind: String,
        pos: [f64; 3],
        heading: f32,
        files: Vec<PathBuf>,
        gap: f32,
    }
    let mut near: Vec<Seen> = Vec::new();
    for o in app.session.scenery.objects() {
        let Some(l) = o.lua.as_ref() else { continue };
        if o.pos.distance(center) <= l.range {
            near.push(Seen { id: o.map_id, kind: l.type_name.clone(), pos: [o.pos.x, o.pos.y, o.pos.z], heading: o.heading, files: l.files.clone(), gap: if l.rate > 0.0 { 1.0 / l.rate } else { 0.0 } });
        }
    }
    let state = &mut app.integrations.object_scripts;
    let now: HashSet<i64> = near.iter().map(|s| s.id).collect();
    // gone: out of range, or unloaded with their tile
    let gone: Vec<i64> = state.known.keys().copied().filter(|id| !now.contains(id)).collect();
    for id in gone {
        if let Some(k) = state.known.remove(&id) {
            for name in &k.plugins {
                plugins.post(name, "object_gone", vec![Value::from(id)]);
            }
        }
    }
    for s in near {
        let k = match state.known.get_mut(&s.id) {
            Some(k) => k,
            None => {
                let mut names = Vec::new();
                for f in &s.files {
                    if state.failed.contains(f) {
                        continue;
                    }
                    match plugins.ensure_object_script(f) {
                        Some(n) => names.push(n),
                        None => {
                            state.failed.insert(f.clone());
                        }
                    }
                }
                for n in &names {
                    plugins.post(n, "object_init", vec![Value::from(s.id), Value::from(s.kind.as_str()), Value::from(s.pos[0]), Value::from(s.pos[1]), Value::from(s.pos[2]), Value::from(s.heading)]);
                }
                state.known.entry(s.id).or_insert(Known { since: f32::INFINITY, plugins: names })
            }
        };
        k.since = if k.since.is_infinite() { 0.0 } else { k.since + dt };
        if k.since >= s.gap {
            let acc = k.since;
            k.since = 0.0;
            for n in &k.plugins {
                plugins.post(n, "object_frame", vec![Value::from(s.id), Value::from(acc)]);
            }
        }
    }
}

/// The player clicked a `[mouseevent]` of an object: its scripts hear it.
pub(crate) fn clicked(app: &mut App, map_id: i64, event: &str) {
    let Some(plugins) = app.integrations.plugins.as_ref() else { return };
    let Some(o) = app.session.scenery.find(map_id).map(|i| app.session.scenery.get(i)) else { return };
    let Some(l) = o.lua.as_ref() else { return };
    for f in &l.files {
        plugins.post(&plugin_name(f), "object_click", vec![Value::from(map_id), Value::from(event)]);
    }
}

fn find(app: &mut App, id: i64) -> Option<&mut omsi_sim::scenery::SceneryObject> {
    app.session.scenery.find_mut(id)
}

pub(crate) fn owned_by(app: &App, id: i64, plugin: &str) -> bool {
    let s = &app.session.scenery;
    s.find(id).map(|i| s.get(i)).and_then(|o| o.lua.as_ref()).is_some_and(|l| l.files.iter().any(|f| plugin_name(f).eq_ignore_ascii_case(plugin)))
}

pub(crate) fn list(app: &App, plugin: &str) -> Vec<i64> {
    app.session.scenery.objects().iter().filter(|o| o.lua.as_ref().is_some_and(|l| l.files.iter().any(|f| plugin_name(f).eq_ignore_ascii_case(plugin)))).map(|o| o.map_id).collect()
}

pub(crate) fn info(app: &App, id: i64) -> Option<ObjectInfo> {
    let s = &app.session.scenery;
    let o = s.get(s.find(id)?);
    let l = o.lua.as_ref()?;
    let center = app.camera.as_ref().map(|c| c.position).or_else(|| app.player.as_ref().map(|p| p.vehicle.position))?;
    let distance = o.pos.distance(center);
    Some(ObjectInfo { id, kind: l.type_name.clone(), pos: [o.pos.x, o.pos.y, o.pos.z], heading: o.heading, active: distance <= l.range, distance })
}

pub(crate) fn var(app: &mut App, id: i64, name: &str) -> Option<f32> {
    find(app, id)?.inst.var(name)
}

pub(crate) fn set_var(app: &mut App, id: i64, name: &str, v: f32) -> bool {
    find(app, id).is_some_and(|o| o.inst.set_var(name, v))
}

pub(crate) fn str_var(app: &mut App, id: i64, name: &str) -> Option<String> {
    let o = find(app, id)?;
    o.inst.program.str_var(name).map(|_| o.inst.str_var(name).to_string())
}

pub(crate) fn set_str(app: &mut App, id: i64, name: &str, v: &str) -> bool {
    let Some(o) = find(app, id) else { return false };
    match o.inst.program.str_var(name) {
        Some(i) => {
            if let Some(slot) = o.inst.state.str_vars.get_mut(i as usize) {
                *slot = v.to_string();
            }
            true
        }
        None => false,
    }
}

pub(crate) fn trigger(app: &mut App, id: i64, name: &str) -> bool {
    find(app, id).is_some_and(|o| o.inst.trigger(name))
}

pub(crate) fn meshes(app: &mut App, id: i64) -> Vec<String> {
    match find(app, id) {
        Some(o) => (0..o.inst.mesh_count()).filter_map(|i| o.inst.mesh_file(i).map(str::to_string)).collect(),
        None => Vec::new(),
    }
}

/// A script's transform as the matrix the animations compose with.
pub(crate) fn matrix(t: &MeshTransform) -> Mat4 {
    let pivot = Vec3::from(t.pivot);
    Mat4::from_translation(pivot + Vec3::from(t.pos))
        * Mat4::from_rotation_z(t.rot[2].to_radians())
        * Mat4::from_rotation_y(t.rot[1].to_radians())
        * Mat4::from_rotation_x(t.rot[0].to_radians())
        * Mat4::from_scale(Vec3::from(t.scale))
        * Mat4::from_translation(-pivot)
}

pub(crate) fn set_mesh(app: &mut App, id: i64, mesh: &str, t: Option<MeshTransform>) -> bool {
    let Some(o) = find(app, id) else { return false };
    match o.inst.mesh_index(mesh) {
        Some(i) => o.inst.set_mesh_transform(i, t.as_ref().map(matrix)),
        None => false,
    }
}

pub(crate) fn set_visible(app: &mut App, id: i64, mesh: &str, v: Option<bool>) -> bool {
    let Some(o) = find(app, id) else { return false };
    match o.inst.mesh_index(mesh) {
        Some(i) => o.inst.set_mesh_visible(i, v),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transform_turns_about_its_pivot() {
        let t = MeshTransform { pos: [0.0; 3], rot: [0.0, 0.0, 90.0], scale: [1.0; 3], pivot: [1.0, 0.0, 0.0] };
        let m = matrix(&t);
        // the pivot stays; a point 1 m beyond it (x = 2) goes to y = 1 (quarter turn counter-clockwise)
        assert!((m.transform_point3(Vec3::new(1.0, 0.0, 0.0)) - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-5);
        assert!((m.transform_point3(Vec3::new(2.0, 0.0, 0.0)) - Vec3::new(1.0, 1.0, 0.0)).length() < 1e-5);
        let moved = matrix(&MeshTransform { pos: [0.0, 0.0, 3.0], rot: [0.0; 3], scale: [2.0, 2.0, 2.0], pivot: [0.0; 3] });
        assert!((moved.transform_point3(Vec3::new(1.0, 1.0, 1.0)) - Vec3::new(2.0, 2.0, 5.0)).length() < 1e-5);
    }

    #[test]
    fn plugin_names_follow_the_whole_path() {
        let a = plugin_name(std::path::Path::new("Objects/A/gate.lua"));
        let b = plugin_name(std::path::Path::new("Objects/B/gate.lua"));
        assert!(a.starts_with("object:gate-") && b.starts_with("object:gate-"), "{a} {b}");
        assert_ne!(a, b, "the same file name in two folders is two scripts");
        assert_eq!(a, plugin_name(std::path::Path::new("Objects/A/gate.lua")), "the same file is the same script");
    }
}
