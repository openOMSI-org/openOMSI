//! `mapgen_dump <map folder> <out.png> [x y size]`: the navigator's layers of a map
//! (`omsi_scene::mapgen`) drawn from above, flat, to look at what the generator made. Without
//! a place the whole map is drawn; with one, `size` metres square around (x, y) (world).
extern crate omsi_scene_mx as omsi_scene;

use glam::Vec2;
use omsi_map::layers;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: mapgen_dump <map folder> <out.png> [x y size]");
        std::process::exit(2);
    }
    let _ = env_logger_init();
    let map_dir = std::path::PathBuf::from(&args[1]);
    let root = map_dir.parent().and_then(|p| p.parent()).unwrap_or(&map_dir).to_path_buf();
    let global = omsi_map::global::GlobalCfg::load(&map_dir.join("global.cfg")).expect("global.cfg");
    omsi_map::configure_grid(&global);
    let tiles: Vec<(i32, i32, std::path::PathBuf)> = global.tiles.iter().map(|t| (t.x, t.y, omsi_cfg::resolve_path(&map_dir, &t.file))).filter(|t| t.2.is_file()).collect();
    let l = omsi_scene::mapgen::cached(&root, &map_dir, &global.ground_textures, &tiles, &[]);
    let (lo, hi) = match args.get(3..6) {
        Some([x, y, s]) => {
            let (x, y, s): (f64, f64, f64) = (x.parse().unwrap(), y.parse().unwrap(), s.parse().unwrap());
            let c = Vec2::new((x - l.origin.x) as f32, (y - l.origin.y) as f32);
            (c - Vec2::splat(s as f32 * 0.5), c + Vec2::splat(s as f32 * 0.5))
        }
        _ => l.bounds().expect("tiles"),
    };
    let size = 2048u32;
    let k = size as f32 / (hi - lo).max_element();
    let mut img = image::RgbaImage::from_pixel(size, size, image::Rgba([214, 230, 196, 255]));
    let px = |p: Vec2| Vec2::new((p.x - lo.x) * k, size as f32 - (p.y - lo.y) * k);
    let tri = |img: &mut image::RgbaImage, a: Vec2, b: Vec2, c: Vec2, col: [u8; 4]| {
        let (a, b, c) = (px(a), px(b), px(c));
        let mn = a.min(b).min(c).max(Vec2::ZERO);
        let mx = a.max(b).max(c).min(Vec2::splat(size as f32 - 1.0));
        let area = (b - a).perp_dot(c - a);
        if area.abs() < 1e-6 || mn.x > mx.x || mn.y > mx.y {
            return;
        }
        for y in mn.y as u32..=mx.y as u32 {
            for x in mn.x as u32..=mx.x as u32 {
                let q = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let w0 = (b - a).perp_dot(q - a) / area;
                let w1 = (c - b).perp_dot(q - b) / area;
                let w2 = (a - c).perp_dot(q - c) / area;
                if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                    let p = img.get_pixel_mut(x, y);
                    let t = col[3] as f32 / 255.0;
                    for i in 0..3 {
                        p.0[i] = (p.0[i] as f32 * (1.0 - t) + col[i] as f32 * t) as u8;
                    }
                }
            }
        }
    };
    let land = [[200, 226, 178, 255], [160, 205, 140, 255], [236, 232, 196, 255], [236, 234, 230, 255], [240, 230, 200, 255], [150, 200, 240, 255]];
    for class in [layers::LAND_GRASS, layers::LAND_FIELD, layers::LAND_SAND, layers::LAND_PAVED, layers::LAND_FOREST, layers::LAND_WATER] {
        for t in l.land_of(class).chunks_exact(3) {
            tri(&mut img, t[0], t[1], t[2], land[class]);
        }
    }
    // (with OMSI_DUMP_CLASSES the road is beige, the footway pink and the yard blue)
    let classes = std::env::var_os("OMSI_DUMP_CLASSES").is_some();
    let paved = if classes { [[255, 235, 205, 255], [255, 205, 215, 255], [170, 200, 240, 255]] } else { [[255, 255, 255, 255], [246, 244, 240, 255], [226, 226, 230, 255]] };
    for class in [layers::PAVE_FOOT, layers::PAVE_AREA, layers::PAVE_ROAD] {
        for t in l.paved_of(class).chunks_exact(3) {
            tri(&mut img, t[0], t[1], t[2], paved[class]);
        }
    }
    let mark = [[196, 200, 208, 255], [236, 190, 70, 255]];
    for class in [layers::PAINT_WHITE, layers::PAINT_AMBER] {
        for t in l.paint_of(class).chunks_exact(3) {
            tri(&mut img, t[0], t[1], t[2], mark[class]);
        }
    }
    for t in &l.trees {
        let c = Vec2::new(t[0], t[1]);
        let r = t[2];
        for k in 0..8 {
            let a0 = k as f32 / 8.0 * std::f32::consts::TAU;
            let a1 = (k + 1) as f32 / 8.0 * std::f32::consts::TAU;
            tri(&mut img, c, c + Vec2::new(a0.cos(), a0.sin()) * r, c + Vec2::new(a1.cos(), a1.sin()) * r, [120, 180, 100, 200]);
        }
    }
    for line in &l.lines {
        let col = if line.class == layers::LINE_RAIL { [140, 140, 150, 255] } else { [170, 150, 190, 255] };
        for ab in line.points.windows(2) {
            let d = (ab[1] - ab[0]).normalize_or_zero().perp() * 0.9;
            tri(&mut img, ab[0] + d, ab[1] + d, ab[1] - d, col);
            tri(&mut img, ab[0] + d, ab[1] - d, ab[0] - d, col);
        }
    }
    for b in &l.buildings {
        let shade = (230.0 - (b.height.min(40.0) * 2.0)) as u8;
        for t in &b.roof {
            tri(&mut img, b.outline[t[0] as usize], b.outline[t[1] as usize], b.outline[t[2] as usize], [shade, shade.saturating_sub(4), shade.saturating_sub(10), 255]);
        }
    }
    img.save(&args[2]).expect("png");
    println!("{} houses, {} trees, {:?}..{:?}", l.buildings.len(), l.trees.len(), lo, hi);
}

fn env_logger_init() -> Option<()> {
    struct L;
    impl log::Log for L {
        fn enabled(&self, m: &log::Metadata) -> bool {
            m.level() <= log::Level::Info
        }
        fn log(&self, r: &log::Record) {
            if self.enabled(r.metadata()) && r.target().starts_with("omsi_scene") {
                eprintln!("{}", r.args());
            }
        }
        fn flush(&self) {}
    }
    log::set_boxed_logger(Box::new(L)).ok()?;
    log::set_max_level(log::LevelFilter::Info);
    Some(())
}
