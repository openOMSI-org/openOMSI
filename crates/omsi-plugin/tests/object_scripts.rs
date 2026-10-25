//! Scripts of scenery objects (`[lua_script]` of a `.sco`): started by name, told of their
//! objects by events, touching only the objects they drive.
use omsi_plugin::api::Value;
use omsi_plugin::{object_script_name, MeshTransform, ObjectInfo, PluginIo, Plugins};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Default)]
struct Game {
    /// id -> (owner plugin, variables)
    objects: HashMap<i64, (String, HashMap<String, f32>)>,
    meshes: Vec<(i64, String, Option<MeshTransform>)>,
    visible: Vec<(i64, String, Option<bool>)>,
}

impl PluginIo for Game {
    fn system(&mut self, _: &str) -> Option<f32> {
        None
    }
    fn set_system(&mut self, _: &str, _: f32) {}
    fn has_vehicle(&self) -> bool {
        false
    }
    fn var(&mut self, _: &str) -> Option<f32> {
        None
    }
    fn set_var(&mut self, _: &str, _: f32) {}
    fn string(&mut self, _: &str) -> Option<String> {
        None
    }
    fn set_string(&mut self, _: &str, _: &str) {}
    fn fire(&mut self, _: &str, _: bool) {}
    fn dt(&self) -> f32 {
        0.5
    }
    fn object_owned_by(&self, id: i64, plugin: &str) -> bool {
        self.objects.get(&id).is_some_and(|o| o.0 == plugin)
    }
    fn object_list(&self, plugin: &str) -> Vec<i64> {
        let mut v: Vec<i64> = self.objects.iter().filter(|(_, o)| o.0 == plugin).map(|(i, _)| *i).collect();
        v.sort();
        v
    }
    fn object_info(&self, id: i64) -> Option<ObjectInfo> {
        self.objects.get(&id).map(|_| ObjectInfo { id, kind: "Gate".into(), pos: [1.0, 2.0, 3.0], heading: 90.0, active: true, distance: 12.5 })
    }
    fn object_var(&mut self, id: i64, name: &str) -> Option<f32> {
        self.objects.get(&id)?.1.get(name).copied()
    }
    fn object_set_var(&mut self, id: i64, name: &str, v: f32) -> bool {
        match self.objects.get_mut(&id).and_then(|o| o.1.get_mut(name)) {
            Some(slot) => {
                *slot = v;
                true
            }
            None => false,
        }
    }
    fn object_meshes(&self, _: i64) -> Vec<String> {
        vec!["arm.o3d".into(), "post.o3d".into()]
    }
    fn object_set_mesh(&mut self, id: i64, mesh: &str, t: Option<MeshTransform>) -> bool {
        self.meshes.push((id, mesh.to_string(), t));
        true
    }
    fn object_set_visible(&mut self, id: i64, mesh: &str, v: Option<bool>) -> bool {
        self.visible.push((id, mesh.to_string(), v));
        true
    }
}

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("omsi-object-script-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn game_with(plugin: &str) -> Game {
    let mut g = Game::default();
    g.objects.insert(7, (plugin.to_string(), HashMap::from([("open".to_string(), 0.0)])));
    g.objects.insert(8, ("object:other".to_string(), HashMap::from([("open".to_string(), 0.0)])));
    g
}

#[test]
fn an_object_script_hears_its_objects_and_moves_them() {
    let d = dir("run");
    std::fs::write(d.join("gate.lua"), r#"
        local seen = {}
        omsi.on("object_init", function(id, kind, x, y, z, heading) seen[id] = kind .. x .. heading end)
        omsi.on("object_frame", function(id, dt)
          omsi.object.set_var(id, "open", omsi.object.var(id, "open") + dt)
          omsi.object.set_mesh(id, "arm", { rz = 30 * omsi.object.var(id, "open"), pz = 1 })
          if omsi.object.var(id, "open") > 1 then omsi.object.set_visible(id, 2, false) end
        end)
        omsi.on("object_gone", function(id) seen[id] = nil end)
        function on_frame() omsi.object.set_var(7, "seen", #omsi.object.list()) end
    "#).unwrap();
    let mut plugins = Plugins::default();
    let mut game = game_with(&object_script_name(&d.join("gate.lua")));
    let name = plugins.ensure_object_script(&d.join("gate.lua")).expect("the script starts");
    assert_eq!(name, object_script_name(&d.join("gate.lua")));
    assert_eq!(plugins.ensure_object_script(&d.join("gate.lua")).as_deref(), Some(name.as_str()), "a second object does not start it again");
    plugins.post(&name, "object_init", vec![Value::from(7i64), Value::from("Gate"), Value::from(1.0), Value::from(2.0), Value::from(3.0), Value::from(90.0)]);
    for _ in 0..3 {
        plugins.post(&name, "object_frame", vec![Value::from(7i64), Value::from(0.5)]);
        plugins.frame(&mut game);
    }
    let vars = &game.objects[&7].1;
    assert!((vars["open"] - 1.5).abs() < 1e-5, "three frames of half a second: {vars:?}");
    assert_eq!(game.meshes.len(), 3);
    let (id, mesh, t) = &game.meshes[2];
    assert_eq!((*id, mesh.as_str()), (7, "arm"));
    let t = t.unwrap();
    assert!((t.rot[2] - 45.0).abs() < 1e-3 && t.pivot[2] == 1.0 && t.scale == [1.0; 3], "{t:?}");
    assert_eq!(game.visible.last(), Some(&(7, "2".to_string(), Some(false))), "a number names a mesh too");
}

#[test]
fn a_script_cannot_touch_objects_it_does_not_drive() {
    let d = dir("own");
    std::fs::write(d.join("gate.lua"), r#"
        omsi.on("object_frame", function(id, dt)
          local ok, err = pcall(function() omsi.object.set_var(8, "open", 5) end)
          omsi.object.set_var(7, "denied", ok and 0 or 1)
          omsi.object.set_var(7, "open", 1)
        end)
    "#).unwrap();
    let mut plugins = Plugins::default();
    let mut game = game_with(&object_script_name(&d.join("gate.lua")));
    game.objects.get_mut(&7).unwrap().1.insert("denied".into(), 0.0);
    let name = plugins.ensure_object_script(&d.join("gate.lua")).unwrap();
    plugins.post(&name, "object_frame", vec![Value::from(7i64), Value::from(0.1)]);
    plugins.frame(&mut game);
    assert_eq!(game.objects[&7].1["denied"], 1.0, "object 8 belongs to another script");
    assert_eq!(game.objects[&8].1["open"], 0.0);
    assert_eq!(game.objects[&7].1["open"], 1.0);
}

#[test]
fn a_plain_plugin_drives_no_objects() {
    let d = dir("plain");
    std::fs::write(d.join("nosy.lua"), "function on_frame() local ok = pcall(function() omsi.object.set_var(7, 'open', 9) end); omsi.set_var('nosy', ok and 1 or 0) end").unwrap();
    let mut plugins = Plugins::load(&[d.clone()], &omsi_plugin::HostConfig::default());
    let mut game = game_with("object:gate");
    plugins.frame(&mut game);
    assert_eq!(game.objects[&7].1["open"], 0.0);
}

#[test]
fn object_scripts_may_play_sounds_and_nothing_else_of_the_game() {
    let allowed = Plugins::vehicle_script_permissions();
    assert!(!allowed.is_empty());
    let d = dir("perm");
    std::fs::write(d.join("pushy.lua"), "omsi.on('object_frame', function(id) pcall(function() omsi.command('shot') end); pcall(function() omsi.set_var('x', 1) end); omsi.object.set_var(id, 'ran', 1) end)").unwrap();
    let mut plugins = Plugins::default();
    let mut game = game_with(&object_script_name(&d.join("pushy.lua")));
    game.objects.get_mut(&7).unwrap().1.insert("ran".into(), 0.0);
    let name = plugins.ensure_object_script(&d.join("pushy.lua")).unwrap();
    plugins.post(&name, "object_frame", vec![Value::from(7i64), Value::from(0.1)]);
    plugins.frame(&mut game);
    assert_eq!(game.objects[&7].1["ran"], 1.0);
}

#[test]
fn two_folders_with_the_same_file_name_are_two_scripts() {
    let d = dir("twins");
    for (folder, mark) in [("A", 1), ("B", 2)] {
        std::fs::create_dir_all(d.join(folder)).unwrap();
        std::fs::write(d.join(folder).join("gate.lua"), format!("omsi.on('object_frame', function(id) omsi.object.set_var(id, 'who', {mark}) end)")).unwrap();
    }
    let mut plugins = Plugins::default();
    let a = plugins.ensure_object_script(&d.join("A").join("gate.lua")).expect("A starts");
    let b = plugins.ensure_object_script(&d.join("B").join("gate.lua")).expect("B starts");
    assert_ne!(a, b, "the same basename in two folders must not share a script");
    assert_eq!(a, object_script_name(&d.join("A").join("gate.lua")));
    let mut game = game_with(&b);
    game.objects.get_mut(&7).unwrap().1.insert("who".into(), 0.0);
    plugins.post(&a, "object_frame", vec![Value::from(7i64), Value::from(0.1)]);
    plugins.post(&b, "object_frame", vec![Value::from(7i64), Value::from(0.1)]);
    plugins.frame(&mut game);
    assert_eq!(game.objects[&7].1["who"], 2.0, "only the script that owns the object may drive it (A is refused, B runs)");
}
