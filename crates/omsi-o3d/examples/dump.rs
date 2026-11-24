fn main() {
    let p = std::env::args().nth(1).unwrap();
    let m = omsi_o3d::load_mesh(std::path::Path::new(&p)).unwrap();
    println!(
        "verts {} tris {} mats {:?} bounds {:?} matrix {:?}",
        m.vertices.len(),
        m.triangles.len(),
        m.materials
            .iter()
            .map(|x| x.texture.clone())
            .collect::<Vec<_>>(),
        m.bounds(),
        m.transform_row_major()
    );
    for (i, v) in m.vertices.iter().enumerate().take(40) {
        println!("{i:3} pos {:?} n {:?} uv {:?}", v.position, v.normal, v.uv);
    }
    for t in m.triangles.iter().take(20) {
        println!("tri {:?} mat {}", t.indices, t.material);
    }
}
