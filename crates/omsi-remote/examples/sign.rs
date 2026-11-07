use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(root), Some(bus)) = (args.next(), args.next()) else {
        eprintln!("sign <OMSI root> <.bus file>");
        std::process::exit(2);
    };
    let root = PathBuf::from(root);
    let ty = omsi_sim::VehicleType::load(&root, &PathBuf::from(bus)).expect("vehicle");
    match omsi_remote::sign::Sign::find(&ty, &root) {
        Some(s) => s.describe().iter().for_each(|l| println!("{l}")),
        None => println!("no destination sign found"),
    }
}
