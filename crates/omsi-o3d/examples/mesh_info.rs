//! What a mesh file holds as openOMSI reads it: `cargo run -p omsi-o3d --example mesh_info
//! <file.o3d|.x|.glb|.gltf|.fbx|.obj>`.

fn main() {
    let Some(p) = std::env::args().nth(1) else {
        eprintln!("usage: mesh_info <mesh file>");
        std::process::exit(2);
    };
    let m = match omsi_o3d::load_mesh(std::path::Path::new(&p)) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{p}: {e}");
            std::process::exit(1);
        }
    };
    let (lo, hi) = m
        .vertices
        .iter()
        .fold((glam::Vec3::MAX, glam::Vec3::MIN), |(a, b), v| {
            (a.min(v.position), b.max(v.position))
        });
    println!(
        "{} vertices, {} triangles, bounds {lo:.3} .. {hi:.3}",
        m.vertices.len(),
        m.triangles.len()
    );
    for (i, mat) in m.materials.iter().enumerate() {
        println!(
            "material {i}: diffuse {:?} texture {:?}",
            mat.diffuse, mat.texture
        );
    }
    println!(
        "{} bones, {} joints, {} blend shapes",
        m.bones.len(),
        m.joints.len(),
        m.morphs.len()
    );
    for j in &m.joints {
        println!(
            "  joint {:?} parent {:?} at {:.3}",
            j.name,
            j.parent.map(|p| &m.joints[p].name),
            j.position
        );
    }
    for mo in &m.morphs {
        println!("  shape {:?}: {} vertices", mo.name, mo.offsets.len());
    }
}
