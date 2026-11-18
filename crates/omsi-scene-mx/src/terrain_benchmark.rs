//! Opt-in measurement against installed map content; never runs in ordinary CI.
use super::*;

#[test]
#[ignore = "requires TERRAIN_BENCH_ROOT, TERRAIN_BENCH_GLOBAL, TERRAIN_BENCH_TILE=x,y"]
fn tile_preparation_benchmark() {
    struct Warnings;
    impl log::Log for Warnings {
        fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
            metadata.level() <= log::Level::Warn
        }
        fn log(&self, record: &log::Record<'_>) {
            if self.enabled(record.metadata()) {
                eprintln!("{}: {}", record.level(), record.args());
            }
        }
        fn flush(&self) {}
    }
    static WARNINGS: Warnings = Warnings;
    log::set_logger(&WARNINGS).expect("standalone benchmark logger");
    log::set_max_level(log::LevelFilter::Warn);
    let root = PathBuf::from(std::env::var_os("TERRAIN_BENCH_ROOT").expect("TERRAIN_BENCH_ROOT"));
    let global = PathBuf::from(std::env::var_os("TERRAIN_BENCH_GLOBAL").expect("TERRAIN_BENCH_GLOBAL"));
    let tile = std::env::var("TERRAIN_BENCH_TILE").expect("TERRAIN_BENCH_TILE=x,y");
    let date = std::env::var("TERRAIN_BENCH_DATE")
        .unwrap_or_else(|_| "20261001".into())
        .parse()
        .unwrap();
    let (x, y) = if tile == "auto" {
        let probe = World::open(&root, &global, date).unwrap();
        let (count, x, y) = probe
            .global
            .tiles
            .iter()
            .map(|t| {
                let bytes = std::fs::read(global.parent().unwrap().join(&t.file)).unwrap();
                let count = String::from_utf8_lossy(&bytes)
                    .lines()
                    .filter(|line| line.trim().starts_with("[spline_terrain_align"))
                    .count();
                (count, t.x, t.y)
            })
            .max()
            .expect("map has tiles");
        assert!(count > 0, "map must contain aligned splines");
        println!("Selected tile {x},{y}: {count} alignment records before chrono patches");
        (x, y)
    } else {
        let (x, y) = tile.split_once(',').expect("tile x,y or auto");
        (x.parse().unwrap(), y.parse().unwrap())
    };
    // One warm-up, then seven fresh Worlds: reuse OS caches, never the scene tile cache.
    for run in 0..8 {
        let world = World::open(&root, &global, date).unwrap();
        let tile = world
            .global
            .tiles
            .iter()
            .find(|t| t.x == x && t.y == y)
            .expect("tile in global.cfg");
        let path = global.parent().unwrap().join(&tile.file);
        assert!(path.is_file(), "{}", path.display());
        let start = std::time::Instant::now();
        let (prepared, stats) = world.prepare_tiles(&[(x, y, path)]);
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(
            stats.failed_objects, 0,
            "missing object content invalidates this measurement"
        );
        assert_eq!(prepared.len(), 1);
        let mesh = prepared[0].terrain.as_ref().expect("terrain mesh");
        if run > 0 {
            println!(
                "\nTERRAIN_BENCH,{x},{y},{run},{ms:.3},{},{}",
                mesh.positions.len(),
                mesh.indices.len() / 3
            );
        }
    }
}
