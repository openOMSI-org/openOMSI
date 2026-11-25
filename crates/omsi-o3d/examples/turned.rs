//! Meshes whose faces nearly all turn their backs on their normals, with the file's matrix:
//! whether the matrix is a mirror (determinant) and whether the normals turned by the
//! matrix agree with the winding again (then the exporter only left the normals in the
//! object's own frame, and the faces are wound the right way round).
//! usage: turned <file.o3d>...
fn main() {
    for p in std::env::args().skip(1) {
        let Ok(m) = omsi_o3d::load_mesh(std::path::Path::new(&p)) else {
            continue;
        };
        let rot = glam::Mat3::from_mat4(m.transform_row_major());
        let (mut against, mut against_turned, mut total) = (0usize, 0usize, 0usize);
        for t in &m.triangles {
            let v: Vec<_> = t.indices.iter().map(|&i| &m.vertices[i as usize]).collect();
            let g = (v[1].position - v[0].position).cross(v[2].position - v[0].position);
            let n = v[0].normal + v[1].normal + v[2].normal;
            if g.length_squared() < 1e-12 || n.length_squared() < 1e-12 {
                continue;
            }
            total += 1;
            if g.dot(n) < 0.0 {
                against += 1;
            }
            if g.dot(rot * n) < 0.0 {
                against_turned += 1;
            }
        }
        if total > 0 && against * 10 >= total * 9 {
            println!(
                "{against:5}/{total:5} against, {against_turned:5} with the matrix, det {:5.2}  {p}",
                rot.determinant()
            );
        }
    }
}
