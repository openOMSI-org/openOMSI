//! `cargo run -p omsi-transit --example transit_check -- <map folder> [bus|tram|railway|metro|trolleybus ...]`
//! reads the map's `transport` folder and prints every problem found; exit status 1 on errors.

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(dir) = args.next() else {
        eprintln!("usage: transit_check <map folder> [modes the map is made for]");
        std::process::exit(2);
    };
    let modes: omsi_cfg::transport::ModeSet = args.filter_map(|a| omsi_cfg::transport::TransportMode::from_name(&a)).collect();
    let mut issues = Vec::new();
    let t = omsi_transit::TransitData::load(std::path::Path::new(&dir), &mut issues);
    t.check(modes, &mut issues);
    if t.is_empty() {
        println!("no transport folder files found");
    } else {
        println!("{}: {} stations, {} routes, {} switches, {} depots, {} signals", t.files.join(", "), t.stations.list.len(), t.routes.list.len(), t.switches.list.len(), t.depots.list.len(), t.signals.signals.len());
    }
    for i in &issues {
        println!("{i}");
    }
    if issues.iter().any(|i| i.error) {
        std::process::exit(1);
    }
}
