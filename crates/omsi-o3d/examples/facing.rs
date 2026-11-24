//! For each o3d: the determinant of its matrix and how many faces turn their backs on
//! their own normals - `facing <files>`.
fn main() {
    for p in std::env::args().skip(1) {
        let Ok(m) = omsi_o3d::load_mesh(std::path::Path::new(&p)) else {
            continue;
        };
        let (mut against, mut counted) = (0usize, 0usize);
        for t in &m.triangles {
            let v = t.indices.map(|i| &m.vertices[i as usize]);
            let g = (v[1].position - v[0].position).cross(v[2].position - v[0].position);
            let n = v[0].normal + v[1].normal + v[2].normal;
            if g.length_squared() > 1e-12 && n.length_squared() > 1e-12 {
                counted += 1;
                if g.dot(n) < 0.0 {
                    against += 1;
                }
            }
        }
        let det = m.transform.determinant();
        if counted > 0 && against * 10 >= counted * 9 || det < 0.0 {
            println!("{:>8.3} {:5}/{:<5} {}", det, against, counted, p);
        }
    }
}
