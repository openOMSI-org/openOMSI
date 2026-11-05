use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(root), Some(bus), Some(out)) = (args.next(), args.next(), args.next()) else {
        eprintln!("skins <OMSI root> <.bus file> <output folder>");
        std::process::exit(2);
    };
    let (root, out) = (PathBuf::from(root), PathBuf::from(out));
    let ty = omsi_sim::VehicleType::load(&root, &PathBuf::from(bus)).expect("vehicle");
    let started = std::time::Instant::now();
    let skins = omsi_remote::extract_skins(&ty, &root, None);
    println!("{} pictures in {:?}", skins.len(), started.elapsed());
    std::fs::create_dir_all(&out).unwrap();
    for (name, png) in skins {
        std::fs::write(out.join(format!("{name}.png")), png).unwrap();
    }
}
