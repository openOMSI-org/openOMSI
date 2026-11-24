//! Triangles whose winding disagrees with their vertex normals, per mesh file: a face the
//! exporter turned round is drawn from the side its normals do not face, so a renderer that
//! culls back faces as Direct3D does leaves a hole there.
//! usage: winding <file.o3d>...
fn main() {
    for p in std::env::args().skip(1) {
        let Ok(m) = omsi_o3d::load_mesh(std::path::Path::new(&p)) else {
            continue;
        };
        let (mut bad, mut total) = (0usize, 0usize);
        for t in &m.triangles {
            let v: Vec<_> = t.indices.iter().map(|&i| &m.vertices[i as usize]).collect();
            // Direct3D's front face is clockwise in its left-handed frame: its normal is
            // then against this cross product
            let g = (v[1].position - v[0].position).cross(v[2].position - v[0].position);
            let n = v[0].normal + v[1].normal + v[2].normal;
            if g.length_squared() < 1e-12 || n.length_squared() < 1e-12 {
                continue;
            }
            total += 1;
            if g.dot(n) < 0.0 {
                bad += 1;
            }
        }
        if total > 0 {
            println!(
                "{:6.1}% {bad:6}/{total:6} {p}",
                100.0 * bad as f64 / total as f64
            );
        }
    }
}
