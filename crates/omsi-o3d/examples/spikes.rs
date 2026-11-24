//! Meshes with a spike: a triangle standing metres tall in an otherwise flat mesh.
//! `spikes <files...>`
fn main() {
    for p in std::env::args().skip(1) {
        let Ok(m) = omsi_o3d::load_mesh(std::path::Path::new(&p)) else {
            continue;
        };
        let up = |i: u32| m.vertices[i as usize].position.y; // (o3d is y-up)
        let mut ext: Vec<f32> = m
            .triangles
            .iter()
            .map(|t| {
                let z = t.indices.map(up);
                z.iter().cloned().fold(f32::MIN, f32::max)
                    - z.iter().cloned().fold(f32::MAX, f32::min)
            })
            .collect();
        if ext.len() < 2 {
            continue;
        }
        let tall = ext.iter().filter(|e| **e > 2.0).count();
        ext.sort_by(|a, b| a.total_cmp(b));
        let med = ext[ext.len() / 2];
        if tall > 0 && med < 0.3 && tall * 10 < ext.len() {
            println!(
                "{p}: v{} {} of {} triangles over 2 m tall (median {:.2})",
                m.version,
                tall,
                ext.len(),
                med
            );
        }
    }
}
