//! Every triangle of one material with its corners and UVs - `uvtris <file> <material>`.
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let m = omsi_o3d::load_mesh(std::path::Path::new(&a[0])).unwrap();
    let k: usize = a[1].parse().unwrap();
    for t in m.triangles.iter().filter(|t| t.material as usize == k) {
        let s: Vec<String> = t
            .indices
            .iter()
            .map(|&i| {
                let v = &m.vertices[i as usize];
                format!(
                    "({:.3},{:.3},{:.3})uv({:.3},{:.3})",
                    v.position.x, v.position.y, v.position.z, v.uv.x, v.uv.y
                )
            })
            .collect();
        println!("{}", s.join(" "));
    }
}
