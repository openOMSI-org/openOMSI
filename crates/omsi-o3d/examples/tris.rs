// Every triangle of a mesh in the object frame (x right, y forward, z up) with its material.
fn main() {
    let p = std::env::args().nth(1).unwrap();
    let m = omsi_o3d::load_mesh(std::path::Path::new(&p)).unwrap();
    for t in &m.triangles {
        let v: Vec<String> = t
            .indices
            .iter()
            .map(|&i| {
                let q = m.vertices[i as usize].position;
                format!("({:.2},{:.2},{:.2})", q.x, q.z, q.y)
            })
            .collect();
        println!("{} {}", t.material, v.join(" "));
    }
}
