//! What a page costs the game on each engine: `compare <classic|modern> <pages> <frames> <page.html>`.
//!
//! Makes `pages` renderers of the file (the same page on several displays), feeds each every frame
//! like the game does (changed variables in, a picture out) and prints the time the game thread
//! spends per frame. Run it under `/usr/bin/time -l` for the memory and the CPU of the whole process
//! (the modern engine's page threads included).

use omsi_sim::htmltex::{HtmlRenderer, PageApi};
use omsi_sim::vehicle_api::ApiValue;
use std::time::Instant;

fn make(engine: &str, html: &str) -> Box<dyn HtmlRenderer> {
    match engine {
        "classic" => Box::new(omsi_htmlengine::EngineRenderer::with_api(800, 480, html, PageApi::Vehicle)),
        "modern" => Box::new(omsi_htmlpage::EngineRenderer::with_api(800, 480, html, PageApi::Vehicle)),
        other => panic!("engine is classic or modern, not {other}"),
    }
}

fn map(v: Vec<(&str, ApiValue)>) -> ApiValue {
    ApiValue::Map(v.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// What `window.omsi.vehicle` holds in the game (the fields demo.html shows), the speed moving.
fn vehicle(speed: f32, open: bool) -> ApiValue {
    let n = ApiValue::Num;
    let b = ApiValue::Bool;
    map(vec![
        ("info", map(vec![("number", ApiValue::Str("42".into())), ("ident", ApiValue::Str("BENCH".into())), ("route", ApiValue::Str("Line 7".into())), ("nextStop", ApiValue::Str("Central Station".into()))])),
        ("motion", map(vec![("speedKmh", n(speed as f64)), ("heading", n(90.0)), ("pitch", n(0.4)), ("bank", n(-0.2)), ("steeringDeg", n(3.0)), ("x", n(120.5)), ("y", n(33.2)), ("z", n(0.1)), ("odometerKm", n(1234.5))])),
        ("engine", map(vec![("running", b(true)), ("rpm", n(800.0 + speed as f64 * 25.0)), ("gear", n(3.0)), ("tankContent", n(180.0)), ("throttle", n(0.4)), ("brake", n(0.0)), ("clutch", n(0.0))])),
        ("electrics", map(vec![("on", b(true)), ("failure", b(false))])),
        ("battery", map(vec![("on", b(true))])),
        ("brakes", map(vec![("parking", b(false)), ("stop", b(open)), ("kneeling", b(false))])),
        ("wipers", map(vec![("running", b(false))])),
        ("train", map(vec![("trailers", n(0.0))])),
        ("lights", map(vec![("headlights", n(2.0)), ("brake", b(false)), ("reverse", b(false)), ("fog", b(false)), ("indicatorLeft", b(open)), ("indicatorRight", b(false)), ("hazard", b(false)), ("interior", n(0.6))])),
        ("doors", map(vec![("count", n(2.0)), ("anyOpen", b(open)), ("list", ApiValue::List(vec![map(vec![("number", n(1.0)), ("open", n(if open { 1.0 } else { 0.0 })), ("isOpen", b(open))]), map(vec![("number", n(2.0)), ("open", n(0.0)), ("isOpen", b(false))])]))])),
        ("passengers", map(vec![("onboard", n(23.0)), ("entries", ApiValue::List(vec![map(vec![("number", n(1.0)), ("open", b(open)), ("requested", b(false))])])), ("exits", ApiValue::List(vec![map(vec![("number", n(2.0)), ("open", b(false)), ("requested", b(open))])]))])),
        ("cabin", map(vec![("temperature", n(21.5))])),
        ("condition", map(vec![("dirt", n(0.12)), ("crashes", n(0.0)), ("lastImpactKJ", n(0.0)), ("streetCondition", n(1.0))])),
    ])
}

struct Stderr;
impl log::Log for Stderr {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Warn
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            eprintln!("{}: {}", r.level(), r.args());
        }
    }
    fn flush(&self) {}
}

fn main() {
    let _ = log::set_logger(&Stderr).map(|()| log::set_max_level(log::LevelFilter::Warn));
    let args: Vec<String> = std::env::args().collect();
    let (engine, pages, frames, file) = (&args[1], args[2].parse::<usize>().unwrap(), args[3].parse::<usize>().unwrap(), &args[4]);
    let html = std::fs::read_to_string(file).unwrap();
    let dir = std::path::Path::new(file).parent().unwrap().to_path_buf();
    let t = Instant::now();
    let mut rs: Vec<Box<dyn HtmlRenderer>> = (0..pages).map(|_| make(engine, &html)).collect();
    for r in &mut rs {
        r.set_asset_dirs(vec![dir.clone()]);
        let _ = r.poll_frame();
    }
    let created = t.elapsed().as_secs_f64() * 1000.0;
    let mut per_frame = Vec::with_capacity(frames);
    let mut drawn = 0usize;
    let mut last: Option<Vec<u8>> = None;
    for f in 0..frames {
        let t = Instant::now();
        for (i, r) in rs.iter_mut().enumerate() {
            let speed = ((f + i * 7) % 90) as f32;
            r.set_vehicle(&vehicle(speed, (f / 30) % 2 == 0));
            r.set_vars(&[("Velocity".to_string(), speed), ("Engine_N".to_string(), 700.0 + speed * 20.0), ("clock".to_string(), (f / 60) as f32)], &[("destination".to_string(), format!("Stop {}", f / 120))]);
            if let Some(fr) = r.poll_frame() {
                drawn += 1;
                if i == 0 {
                    last = Some(fr);
                }
            }
            let _ = r.take_events();
        }
        per_frame.push(t.elapsed().as_secs_f64() * 1000.0);
        // (a game frame at 60 per second: the modern engine draws on its own thread meanwhile)
        std::thread::sleep(std::time::Duration::from_micros(16_667).saturating_sub(t.elapsed()));
    }
    if let Some(dump) = std::env::var_os("COMPARE_DUMP") {
        // the last picture of the first page, to look at
        if let Some(f) = last {
            std::fs::write(dump, f).unwrap();
        }
    }
    per_frame.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = per_frame.iter().sum::<f64>() / per_frame.len() as f64;
    let p = |q: f64| per_frame[((per_frame.len() - 1) as f64 * q) as usize];
    println!("{engine} pages={pages} frames={frames} created={created:.1}ms frames-delivered={drawn} game-thread ms/frame: mean={mean:.3} p50={:.3} p95={:.3} p99={:.3} max={:.3}", p(0.5), p(0.95), p(0.99), per_frame[per_frame.len() - 1]);
}
