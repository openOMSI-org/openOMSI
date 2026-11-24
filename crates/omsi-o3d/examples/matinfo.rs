//! The materials an o3d stores - `matinfo <files>`.
fn main() {
    for p in std::env::args().skip(1) {
        let Ok(m) = omsi_o3d::load_mesh(std::path::Path::new(&p)) else {
            continue;
        };
        println!("{p}");
        for x in &m.materials {
            println!("  {x:?}");
        }
    }
}
