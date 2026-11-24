//! Sanity metrics for decoded meshes: how well stored normals agree with face normals and
//! how the uv range looks (helps verify the vertex unscrambling).
fn main() {
    for p in std::env::args().skip(1) {
        let m = match omsi_o3d::load_mesh(std::path::Path::new(&p)) {
            Ok(m) => m,
            Err(e) => {
                println!("{p}: {e}");
                continue;
            }
        };
        let mut agree = 0usize;
        let mut total = 0usize;
        for t in &m.triangles {
            let a = m.vertices[t.indices[0] as usize].position;
            let b = m.vertices[t.indices[1] as usize].position;
            let c = m.vertices[t.indices[2] as usize].position;
            let n = (b - a).cross(c - a);
            if n.length() < 1e-9 {
                continue;
            }
            let n = n.normalize();
            for &i in &t.indices {
                let vn = m.vertices[i as usize].normal;
                if vn.length() > 0.5 {
                    total += 1;
                    if vn.normalize().dot(n) > 0.3 {
                        agree += 1;
                    }
                }
            }
        }
        let frac_uv = m
            .vertices
            .iter()
            .filter(|v| v.uv.x >= -0.001 && v.uv.x <= 1.001 && v.uv.y >= -1.001 && v.uv.y <= 1.001)
            .count();
        println!(
            "{}: v{} verts {} tris {} normal agreement {:.3} uv-in-range {:.3}",
            p,
            m.version,
            m.vertices.len(),
            m.triangles.len(),
            agree as f64 / total.max(1) as f64,
            frac_uv as f64 / m.vertices.len().max(1) as f64
        );
    }
}
