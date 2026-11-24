//! The bus's files read again - "Reload this vehicle" - read its meshes again too (#2618).
//! What tells the world a rebuilt mesh from the one it already has on the GPU is the stamp
//! each mesh carries: the size and the time its `.o3d` was last written.

use super::*;

const TRIANGLE: &str = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
const QUAD: &str = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3\nf 1 3 4\n";

/// A vehicle folder whose model has one mesh, `body.obj`, holding `mesh`.
fn vehicle_with(dir: &Path, mesh: &str) {
    std::fs::write(dir.join("test.bus"), "[model]\nmodel.cfg\n[varnamelist]\n1\nvars.txt\n[script]\n1\nmain.osc\n").unwrap();
    std::fs::write(dir.join("model.cfg"), "[mesh]\nbody.obj\n").unwrap();
    std::fs::write(dir.join("vars.txt"), "speed\n").unwrap();
    std::fs::write(dir.join("main.osc"), "{init}\n{end}\n").unwrap();
    std::fs::write(dir.join("body.obj"), mesh).unwrap();
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("omsi_{tag}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_mesh_file_that_changed_has_another_stamp() {
    let dir = temp_dir("stamp");
    let mesh = dir.join("body.obj");
    std::fs::write(&mesh, TRIANGLE).unwrap();
    let first = source_stamp(&mesh);
    assert_ne!(first, 0, "a file that is there has a stamp");
    assert_eq!(source_stamp(&mesh), first, "a file nobody touched keeps it");
    std::fs::write(&mesh, QUAD).unwrap();
    assert_ne!(source_stamp(&mesh), first, "a rebuilt mesh is another mesh");
    assert_eq!(source_stamp(&dir.join("absent.obj")), 0, "a mesh of a pack that is not installed has none");
    std::fs::remove_dir_all(&dir).ok();
}

/// The player rebuilds a mesh, picks "Reload this vehicle": the vehicle made again has the
/// new mesh, and its stamp is not the one the world keyed the old mesh on - which is how the
/// upload tells them apart and gives the bus the new one.
#[test]
fn a_vehicle_read_again_after_its_mesh_was_rebuilt_has_the_new_mesh() {
    let dir = temp_dir("rebuilt");
    vehicle_with(&dir, TRIANGLE);
    let first = VehicleType::load(&dir, &dir.join("test.bus")).unwrap();
    assert_eq!(first.meshes.len(), 1, "{:?}", first.meshes.iter().map(|m| &m.file).collect::<Vec<_>>());
    assert_eq!(first.meshes[0].data.indices.len() / 3, 1, "one triangle");
    let stamp = first.meshes[0].stamp;
    assert_eq!(stamp, source_stamp(&first.meshes[0].file), "the stamp is its file's");

    vehicle_with(&dir, QUAD);
    let second = VehicleType::load(&dir, &dir.join("test.bus")).unwrap();
    assert_eq!(second.meshes[0].data.indices.len() / 3, 2, "the mesh is read again");
    assert_ne!(second.meshes[0].stamp, stamp, "and it is a mesh the GPU has not been given yet");

    // nothing touched: the same stamp, so twenty buses of a type still upload their meshes once
    let third = VehicleType::load(&dir, &dir.join("test.bus")).unwrap();
    assert_eq!(third.meshes[0].stamp, second.meshes[0].stamp);
    std::fs::remove_dir_all(&dir).ok();
}
