//! UV and position range of each material of a mesh - `uvbox <file>`.
fn main() {
    for p in std::env::args().skip(1) {
        let m = omsi_o3d::load_mesh(std::path::Path::new(&p)).unwrap();
        println!("{p}");
        for (k, mat) in m.materials.iter().enumerate() {
            let mut uv = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
            let mut pos = [f32::MAX, f32::MAX, f32::MAX, f32::MIN, f32::MIN, f32::MIN];
            let mut n = 0;
            for t in m.triangles.iter().filter(|t| t.material as usize == k) {
                n += 1;
                for i in t.indices {
                    let v = &m.vertices[i as usize];
                    uv = [
                        uv[0].min(v.uv.x),
                        uv[1].min(v.uv.y),
                        uv[2].max(v.uv.x),
                        uv[3].max(v.uv.y),
                    ];
                    let q = v.position.to_array();
                    for a in 0..3 {
                        pos[a] = pos[a].min(q[a]);
                        pos[a + 3] = pos[a + 3].max(q[a]);
                    }
                }
            }
            println!(
                "  {k} {:?}: {n} tris, u {:.3}..{:.3} v {:.3}..{:.3}, pos {:.3?}",
                mat.texture, uv[0], uv[2], uv[1], uv[3], pos
            );
        }
    }
}
