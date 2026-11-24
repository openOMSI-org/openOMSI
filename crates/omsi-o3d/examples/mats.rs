fn main() {
    let p = std::env::args().nth(1).unwrap();
    let m = omsi_o3d::load_mesh(std::path::Path::new(&p)).unwrap();
    for (mi, mat) in m.materials.iter().enumerate() {
        let mut lo = glam::Vec3::splat(f32::MAX);
        let mut hi = glam::Vec3::splat(f32::MIN);
        let mut n = 0;
        let mut samples = Vec::new();
        for t in m.triangles.iter().filter(|t| t.material as usize == mi) {
            for &i in &t.indices {
                let v = &m.vertices[i as usize];
                lo = lo.min(v.position);
                hi = hi.max(v.position);
                n += 1;
                if samples.len() < 6 {
                    samples.push((v.position, v.uv));
                }
            }
        }
        println!(
            "mat {mi} {:?}: {n} verts, bounds {:?}..{:?}",
            mat.texture, lo, hi
        );
        for (p, uv) in samples {
            println!("   pos {:?} uv {:?}", p, uv);
        }
    }
}
