//! Opt-in CPU/heap diagnostic; no GPU, game assets or frame-time claims.
//! Run with `cargo run --release -p omsi-navigator --example ground_rebuild_bench -- grid`.
//! Modes: grid, large, stray, cache (NAVIGATOR_TEST_LAYERS selects a local map cache).
//! Peaks are requested live Rust heap bytes above the prepared input, not process RSS.
use glam::{DVec2, Vec2, Vec3};
use omsi_map::layers::{self, Layers};
use omsi_mapdraw::{GroundLook, MapData, style};
use omsi_ui::{Color, Painter};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};

struct Counted;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
fn add(n: usize) {
    let now = LIVE.fetch_add(n, Ordering::Relaxed) + n;
    PEAK.fetch_max(now, Ordering::Relaxed);
}
// SAFETY: every operation forwards the original pointer/layout to System; counters allocate nothing.
unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            add(l.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, l, n) };
        if !q.is_null() {
            LIVE.fetch_sub(l.size(), Ordering::Relaxed);
            add(n);
        }
        q
    }
}
#[global_allocator]
static ALLOC: Counted = Counted;
fn measure(name: &str, mut run: impl FnMut() -> usize) {
    std::hint::black_box(run());
    for i in 0..5 {
        let base = LIVE.load(Ordering::Relaxed);
        PEAK.store(base, Ordering::Relaxed);
        let start = Instant::now();
        let vertices = std::hint::black_box(run());
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        let peak = PEAK.load(Ordering::Relaxed).saturating_sub(base);
        println!("{name},run={i},ms={ms:.3},peak_heap_bytes={peak},vertices={vertices}");
    }
}
fn rebuild(md: &MapData, reach: f64) -> usize {
    let pal = style::palette(false);
    let look = GroundLook {
        surfaces: true,
        trees: false,
        dashes: false,
        tol: 0.0,
    };
    let mut roads = Painter::with_ground_budget(1_999_994, reach as f32, 0.3);
    md.build_paved(&mut roads, DVec2::ZERO, DVec2::ZERO, reach, pal, look);
    let roads = roads.finish_ground();
    let mut land = Painter::with_ground_budget(2_000_000 - roads.len(), reach as f32, 0.3);
    md.build_land(&mut land, DVec2::ZERO, DVec2::ZERO, reach, pal, look);
    land.finish_ground().len() + roads.len()
}
fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "grid".into());
    if mode == "grid" {
        measure("overflow-grid", || {
            let mut p = Painter::with_ground_budget(2_000_000, 500.0, 0.1);
            for i in 0..360_000 {
                let x = (i % 600) as f32 * 1.6 - 480.0;
                let y = (i / 600) as f32 * 1.6 - 480.0;
                let color = if i % 2 == 0 {
                    Color::WHITE
                } else {
                    Color::BLACK
                };
                p.world_poly(
                    &[
                        Vec3::new(x, y, 0.0),
                        Vec3::new(x + 1.5, y, 0.0),
                        Vec3::new(x + 1.5, y + 1.5, 0.0),
                        Vec3::new(x, y + 1.5, 0.0),
                    ],
                    color,
                );
            }
            assert!(p.ground_reduced());
            let v = p.finish_ground();
            assert!(v.len() <= 2_000_000);
            v.len()
        });
    } else if mode == "large" || mode == "stray" {
        let mut l = Layers::empty(DVec2::ZERO);
        for y in -100..100 {
            for x in -100..100 {
                for i in 0..10 {
                    let a = Vec2::new(x as f32 * 200.0 + i as f32 * 2.0, y as f32 * 200.0);
                    l.land[layers::LAND_GRASS].extend([a, a + Vec2::X, a + Vec2::Y]);
                }
            }
        }
        if mode == "stray" {
            l.land[layers::LAND_GRASS].extend([
                Vec2::new(50_000.0, 50_000.0),
                Vec2::new(150_000.0, 50_000.0),
                Vec2::new(50_000.0, 150_000.0),
            ]);
        }
        let md = MapData::new(Arc::new(l));
        measure(&mode, || rebuild(&md, 250.0));
    } else if mode == "cache" {
        use std::io::Read;
        let path = std::env::var("NAVIGATOR_TEST_LAYERS").expect("local compatible cache path");
        let mut header = [0; 16];
        std::fs::File::open(&path)
            .unwrap()
            .read_exact(&mut header)
            .unwrap();
        let key = u64::from_le_bytes(header[8..16].try_into().unwrap());
        let l = Layers::open(std::path::Path::new(&path), key).unwrap();
        let md = MapData::new(Arc::new(l));
        measure("cache-dense", || rebuild(&md, 1282.0));
    } else {
        panic!("mode must be grid, large, stray or cache");
    }
}
