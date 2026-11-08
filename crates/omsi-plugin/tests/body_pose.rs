//! `vehicle.body_frame` / `mesh_point` and `other_body_frame` / `other_mesh_point`: a plugin
//! asks where a body and a point of a mesh are, for the bus and for a vehicle round it
//! (a trolleybus plugin putting a pole's shoe where the model draws it).
use omsi_plugin::{HostConfig, PluginIo, Plugins};
use std::collections::HashMap;
use std::path::PathBuf;

/// The bus: at (100, 200, 3), turned a quarter left (its right is north, its forward west).
const BODY: [f64; 12] = [100.0, 200.0, 3.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
/// A trolleybus round it (id 7): at (10, 20, 0), not turned.
const OTHER: [f64; 12] = [10.0, 20.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
/// The bus's rear section (an articulated bus: section 1), 9 m behind its front part.
const REAR: [f64; 12] = [109.0, 200.0, 3.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

#[derive(Default)]
struct Game {
    vars: HashMap<String, f32>,
}

/// A point of a mesh in a body's frame, where the game would put it (only `shoe.o3d` exists).
fn point(frame: &[f64; 12], mesh: &str, p: [f32; 3]) -> Option<[f64; 3]> {
    // (an empty name, like a suffix of every path, matches here: the API must not ask)
    if mesh != "shoe.o3d" && !mesh.is_empty() {
        return None;
    }
    let (x, y, z) = (p[0] as f64, p[1] as f64, p[2] as f64);
    Some([
        frame[0] + frame[3] * x + frame[6] * y + frame[9] * z,
        frame[1] + frame[4] * x + frame[7] * y + frame[10] * z,
        frame[2] + frame[5] * x + frame[8] * y + frame[11] * z,
    ])
}

impl PluginIo for Game {
    fn system(&mut self, _: &str) -> Option<f32> {
        None
    }
    fn set_system(&mut self, _: &str, _: f32) {}
    fn has_vehicle(&self) -> bool {
        true
    }
    fn var(&mut self, name: &str) -> Option<f32> {
        self.vars.get(name).copied()
    }
    fn set_var(&mut self, name: &str, v: f32) {
        self.vars.insert(name.into(), v);
    }
    fn string(&mut self, _: &str) -> Option<String> {
        None
    }
    fn set_string(&mut self, _: &str, _: &str) {}
    fn fire(&mut self, _: &str, _: bool) {}
    fn position(&self) -> Option<[f64; 4]> {
        Some([BODY[0], BODY[1], BODY[2], 0.0])
    }
    fn body_frame(&self, section: usize) -> Option<[f64; 12]> {
        [BODY, REAR].get(section).copied()
    }
    fn mesh_point(&self, mesh: &str, p: [f32; 3], section: usize) -> Option<[f64; 3]> {
        point([BODY, REAR].get(section)?, mesh, p)
    }
    fn other_body_frame(&mut self, id: u64, section: usize) -> Option<[f64; 12]> {
        (id == 7 && section == 0).then_some(OTHER)
    }
    fn other_mesh_point(&mut self, id: u64, mesh: &str, p: [f32; 3], section: usize) -> Option<[f64; 3]> {
        if id == 7 && section == 0 { point(&OTHER, mesh, p) } else { None }
    }
}

fn dir() -> PathBuf {
    let d = std::env::temp_dir().join(format!("omsi-lua-test-body-pose-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("Pose")).unwrap();
    d
}

#[test]
fn a_plugin_sees_where_bodies_and_mesh_points_are() {
    let d = dir();
    std::fs::write(
        d.join("Pose/main.lua"),
        r#"
        function on_frame(dt)
          local x, y, z, rx, ry, rz, fx, fy, fz, ux, uy, uz = omsi.body_frame()
          omsi.set_var("body_x", x) omsi.set_var("body_ry", ry) omsi.set_var("body_fx", fx) omsi.set_var("body_uz", uz)
          assert(omsi.body_frame(2) == nil, "no third section")
          -- the same point on the rear section's shoe: that section's frame
          local qx, qy, qz = omsi.mesh_point("shoe.o3d", 1, -6, 5, 1)
          omsi.set_var("rear_x", qx) omsi.set_var("rear_y", qy) omsi.set_var("rear_z", qz)
          assert(omsi.mesh_point("shoe.o3d", 1, -6, 5, 2) == nil, "no third section")
          -- 1 m right, 6 m back, 5 m up: right is north, back is east
          local sx, sy, sz = omsi.mesh_point("shoe.o3d", 1, -6, 5)
          omsi.set_var("shoe_x", sx) omsi.set_var("shoe_y", sy) omsi.set_var("shoe_z", sz)
          assert(omsi.mesh_point("no_such.o3d", 0, 0, 0) == nil)
          assert(omsi.mesh_point("", 0, 0, 0) == nil, "an empty name is no mesh")
          assert(omsi.other_mesh_point(7, "", 0, 0, 0) == nil, "an empty name is no mesh")
          local ox, oy = omsi.other_body_frame(7)
          omsi.set_var("other_x", ox) omsi.set_var("other_y", oy)
          local px, py, pz = omsi.other_mesh_point(7, "shoe.o3d", 1, -6, 5)
          omsi.set_var("other_shoe_x", px) omsi.set_var("other_shoe_y", py) omsi.set_var("other_shoe_z", pz)
          assert(omsi.other_body_frame(8) == nil, "no such vehicle")
          assert(omsi.other_mesh_point(7, "no_such.o3d", 0, 0, 0) == nil)
          assert(omsi.other_mesh_point(7, "shoe.o3d", 0, 0, 0, 1) == nil, "it has no second section")
        end
        "#,
    )
    .unwrap();
    let mut plugins = Plugins::load(&[d.clone()], &HostConfig::default());
    let mut g = Game::default();
    // (the bus's own variables: a plugin sets only those the bus has)
    for k in ["body_x", "body_ry", "body_fx", "body_uz", "rear_x", "rear_y", "rear_z", "shoe_x", "shoe_y", "shoe_z", "other_x", "other_y", "other_shoe_x", "other_shoe_y", "other_shoe_z"] {
        g.vars.insert(k.into(), f32::NAN);
    }
    plugins.frame(&mut g);
    let v = |k: &str| g.vars.get(k).copied().unwrap_or(f32::NAN);
    assert_eq!((v("body_x"), v("body_ry"), v("body_fx"), v("body_uz")), (100.0, 1.0, -1.0, 1.0));
    assert_eq!((v("shoe_x"), v("shoe_y"), v("shoe_z")), (106.0, 201.0, 8.0));
    assert_eq!((v("rear_x"), v("rear_y"), v("rear_z")), (115.0, 201.0, 8.0));
    assert_eq!((v("other_x"), v("other_y")), (10.0, 20.0));
    assert_eq!((v("other_shoe_x"), v("other_shoe_y"), v("other_shoe_z")), (11.0, 14.0, 5.0));
    plugins.finalize();
}
