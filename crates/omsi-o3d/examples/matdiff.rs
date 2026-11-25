fn main() {
    for p in std::env::args().skip(1) {
        let m = omsi_o3d::load_mesh(std::path::Path::new(&p)).unwrap();
        println!("== {p}");
        for mat in &m.materials {
            println!(
                "  {:?} diffuse {:?} spec {:?} emis {:?} power {}",
                mat.texture, mat.diffuse, mat.specular, mat.emissive, mat.specular_power
            );
        }
    }
}
