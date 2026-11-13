use std::path::PathBuf;
use std::sync::Arc;

fn decode(path: &std::path::Path) -> Result<omsi_sim::image_decode::DecodedImage, String> {
    omsi_texture::decode_file(path).map(|i| omsi_sim::image_decode::DecodedImage { width: i.width, height: i.height, rgba: i.rgba, has_alpha: i.has_alpha }).map_err(|e| e.to_string())
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (root, bus, out) = (PathBuf::from(&a[1]), PathBuf::from(&a[2]), PathBuf::from(&a[3]));
    omsi_sim::image_decode::install(decode);
    for extra in a.iter().skip(4).filter(|s| !s.starts_with("--")) {
        omsi_cfg::add_content_root(PathBuf::from(extra));
    }
    omsi_cfg::add_content_root(root.clone());
    let ty = Arc::new(omsi_sim::VehicleType::load(&root, &bus).expect("vehicle"));
    println!("doorways {:?}", omsi_remote::doorways(&ty));
    let mut host = omsi_sim::VehicleHost::new(Default::default());
    let fonts = Arc::new(parking_lot::Mutex::new(omsi_sim::texttex::FontLibrary::new(&root)));
    host.font_lib = Some(fonts.clone());
    let dir = bus.parent().unwrap();
    let mut hofs: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("hof"))).collect();
    hofs.sort();
    if let Ok(h) = std::env::var("HOF") {
        hofs.insert(0, PathBuf::from(h));
    }
    let hof = hofs.first().and_then(|p| omsi_vehicle::Hof::load(p).ok()).map(Arc::new);
    host.hof = hof.clone();
    let mut v = omsi_sim::VehicleInstance::new(ty.clone(), host);
    v.init_text_textures(&mut fonts.lock(), &|p| omsi_texture::decode_file(p).ok().map(|i| (i.width, i.height, i.rgba)));
    for _ in 0..30 {
        v.update(1.0 / 30.0);
    }
    let mut s = omsi_sim::startup::StartUp::new(&v, &[]);
    for _ in 0..900 {
        let running = s.tick(&mut v, &[], 1.0 / 30.0);
        v.update(1.0 / 30.0);
        if !running {
            break;
        }
    }
    if let Some(h) = &hof {
        let line = h.info_trips.first().map(|t| t.line.clone()).unwrap_or_default();
        println!("hof {} line {line:?} termini {}", h.name, h.termini.len());
        omsi_sim::timetable_run::set_player_destination_at(&mut v, h, &line, 1.min(h.termini.len().saturating_sub(1)), &[]);
    }
    for _ in 0..120 {
        v.update(1.0 / 30.0);
        v.update_text_textures();
    }
    println!("script textures: {}", v.host.script_textures.len());
    for (i, t) in v.host.script_textures.iter().enumerate() {
        let lit = t.rgba.chunks(4).filter(|p| p[3] > 0).count();
        println!("  st {i}: {}x{} alpha>0 {lit}", t.width, t.height);
    }
    let mut sign = omsi_remote::sign::Sign::find(&ty, &root).expect("sign");
    sign.describe().iter().for_each(|l| println!("{l}"));
    for (i, m) in ty.meshes.iter().enumerate() {
        if std::env::var("MESH").is_ok_and(|w| w.split(',').any(|n| n == i.to_string())) {
            println!("mesh {i}: {}", ty.model.meshes[m.def_index].file);
        }
    }
    let started = std::time::Instant::now();
    let picture = loop {
        if let Some(p) = sign.render(&v) {
            break p;
        }
        if started.elapsed().as_secs() > 10 {
            break None;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    match picture {
        Some(png) => std::fs::write(&out, png).unwrap(),
        None => println!("no picture"),
    }
}
