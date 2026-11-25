//! Load every vehicle of every content root the way the game does and say what goes wrong:
//! script errors, meshes that cannot be read, packs that are not installed, and whether the
//! Shift+U start-up gets the electrics and the engine going.
//!
//! usage: bus_audit <content folder> [name filter] [--quiet]
//! (the OMSI 2 installation is taken from $OMSI_ROOT or ~/.openomsi-root)
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct Collect;
impl log::Log for Collect {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Warn
    }
    fn log(&self, r: &log::Record) {
        // OMSI_DEBUG_IBIS: the typist's reasoning straight to stderr
        if omsi_cfg::flags::OMSI_DEBUG_IBIS.live_os().is_some()
            && r.level() <= log::Level::Info
            && r.target().contains("ibis")
        {
            eprintln!("{}", r.args());
        }
        if self.enabled(r.metadata()) {
            LOG.lock().unwrap().push(format!("{}", r.args()));
        }
    }
    fn flush(&self) {}
}

fn original_root() -> Option<PathBuf> {
    if let Ok(r) = omsi_cfg::flags::OMSI_ROOT.live_var() {
        return Some(PathBuf::from(r));
    }
    let home = std::env::var("HOME").ok()?;
    let s = std::fs::read_to_string(Path::new(&home).join(".openomsi-root")).ok()?;
    Some(PathBuf::from(s.trim()))
}

fn main() {
    omsi_sim::image_decode::install(decode_picture);
    log::set_logger(&Collect).unwrap();
    log::set_max_level(if omsi_cfg::flags::OMSI_DEBUG_IBIS.live_os().is_some() {
        log::LevelFilter::Info
    } else {
        log::LevelFilter::Warn
    });
    let a: Vec<String> = std::env::args().collect();
    let content = PathBuf::from(&a[1]);
    let filter = a
        .get(2)
        .filter(|s| !s.starts_with("--"))
        .map(|s| s.to_ascii_lowercase());
    let quiet = a.iter().any(|s| s == "--quiet");
    omsi_cfg::add_content_root(content.clone());
    let archives = content.join("Archives");
    if archives.is_dir() {
        for z in omsi_cfg::vfs::mount_dir_zips(&archives) {
            omsi_cfg::add_content_root(z);
        }
    }
    let orig = original_root();
    if let Some(o) = &orig {
        omsi_cfg::add_content_root(o.clone());
    }
    let root = orig.clone().unwrap_or(content.clone());
    // every .bus / .ovh of every root's Vehicles folder, the first root's copy of a name
    let mut buses: Vec<PathBuf> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for vdir in omsi_cfg::content_dirs("Vehicles") {
        for pack in omsi_cfg::vfs::read_dir_paths(&vdir) {
            if !omsi_cfg::vfs::is_dir(&pack) {
                continue;
            }
            for f in omsi_cfg::vfs::read_dir_paths(&pack) {
                let name = f
                    .file_name()
                    .map(|n| n.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                if !(name.ends_with(".bus") || name.ends_with(".ovh")) {
                    continue;
                }
                let pack_name = pack
                    .file_name()
                    .map(|n| n.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                if !seen.insert(format!("{pack_name}/{name}")) {
                    continue;
                }
                if let Some(flt) = &filter {
                    if !format!("{pack_name}/{name}").contains(flt.as_str()) {
                        continue;
                    }
                }
                buses.push(f);
            }
        }
    }
    buses.sort();
    let (mut ok, mut failed) = (0, 0);
    for bus in &buses {
        LOG.lock().unwrap().clear();
        let short = bus
            .to_string_lossy()
            .rsplit("Vehicles/")
            .next()
            .unwrap_or("")
            .to_string();
        let def = match omsi_vehicle::Vehicle::load(bus) {
            Ok(d) => d,
            Err(e) => {
                println!("{short}: cannot read: {e}");
                failed += 1;
                continue;
            }
        };
        // rear sections, AI-only and non-bus vehicles are not started
        let is_bus = bus
            .extension()
            .map(|e| e.eq_ignore_ascii_case("bus"))
            .unwrap_or(false);
        let vt = match omsi_sim::VehicleType::load(&root, bus) {
            Ok(v) => std::sync::Arc::new(v),
            Err(e) => {
                println!("{short}: cannot load: {e:#}");
                failed += 1;
                continue;
            }
        };
        let errors = vt.program.errors.len();
        let mut v = omsi_sim::VehicleInstance::new(
            vt.clone(),
            omsi_sim::VehicleHost::new(Default::default()),
        );
        for _ in 0..30 {
            v.update(1.0 / 30.0);
        }
        let mut start = String::new();
        if is_bus && !def.is_rear_section() && !short.to_ascii_lowercase().contains("_ai") {
            let mut s = omsi_sim::startup::StartUp::new(&v, &[]);
            let mut t = 0.0;
            while t < 30.0 {
                let running = s.tick(&mut v, &[], 1.0 / 30.0);
                v.update(1.0 / 30.0);
                t += 1.0 / 30.0;
                if !running {
                    break;
                }
            }
            for _ in 0..90 {
                v.update(1.0 / 30.0);
            }
            // `--long`: two minutes on with the engine running, then one with it off: the
            // batteries must not go flat and nothing may switch itself off
            if std::env::args().any(|a| a == "--long") {
                let bat_vars: Vec<String> =
                    v.ty.program
                        .var_names()
                        .into_iter()
                        .filter(|n| {
                            let n = n.to_ascii_lowercase();
                            (n.contains("bat") || n.contains("akku"))
                                && (n.contains("volt")
                                    || n.contains("spannung")
                                    || n.contains("charge")
                                    || n.contains("ladung")
                                    || n.ends_with("_u"))
                        })
                        .collect();
                let snap = |v: &omsi_sim::VehicleInstance| {
                    bat_vars
                        .iter()
                        .map(|n| (n.clone(), v.var(n).unwrap_or(f32::NAN)))
                        .collect::<Vec<_>>()
                };
                let b0 = snap(&v);
                let mut died = None;
                for k in 0..(120 * 30) {
                    v.update(1.0 / 30.0);
                    if died.is_none()
                        && (!omsi_sim::startup::power_on(&v)
                            || !omsi_sim::startup::engine_running(&v))
                    {
                        died = Some(k as f32 / 30.0);
                    }
                }
                let b1 = snap(&v);
                let flat: Vec<String> = b0
                    .iter()
                    .zip(&b1)
                    .filter(|((_, a), (_, b))| {
                        a.is_finite() && b.is_finite() && *a > 1.0 && *b < *a * 0.5
                    })
                    .map(|((n, a), (_, b))| format!("{n} {a:.1}->{b:.1}"))
                    .collect();
                if died.is_some() || !flat.is_empty() {
                    println!(
                        "  LONG {short}: {} {}",
                        died.map(|t| format!("switched off after {t:.0} s;"))
                            .unwrap_or_default(),
                        flat.join(", ")
                    );
                }
            }
            // `--ibis`: type a line and destination of the bus's own depot file on its IBIS
            if std::env::args().any(|a| a == "--ibis") {
                println!("  IBIS {short}: {}", ibis_probe(&mut v, bus));
            }
            // variables that are no numbers any more: a model of the scripts gone wrong
            let bad_vars: Vec<String> =
                v.ty.program
                    .var_names()
                    .into_iter()
                    .filter(|n| v.var(n).is_some_and(|x| !x.is_finite()))
                    .collect();
            if !bad_vars.is_empty() {
                let mut b = bad_vars.clone();
                b.sort();
                println!(
                    "  NAN {short}: {} variables not finite: {}",
                    b.len(),
                    b.iter().take(8).cloned().collect::<Vec<_>>().join(", ")
                );
            }
            let power = omsi_sim::startup::power_on(&v);
            let engine = omsi_sim::startup::engine_running(&v);
            start = format!(
                "start-up: electrics {} engine {} ({})",
                if power { "on" } else { "OFF" },
                if engine { "runs" } else { "OFF" },
                s.report.join("; ")
            );
            if power && engine {
                ok += 1;
            } else {
                failed += 1;
            }
        }
        let logs = LOG.lock().unwrap().clone();
        let missing_meshes = logs
            .iter()
            .filter(|l| l.contains(".o3d") || l.contains(".x:") || l.contains(".X:"))
            .count();
        let bad = !start.is_empty() && start.contains("OFF");
        if !quiet || bad || errors > 0 || !vt.missing_packs.is_empty() {
            println!(
                "{short}: {} meshes, {errors} script errors, {missing_meshes} mesh warnings, missing packs {:?}; {start}",
                vt.meshes.len(),
                vt.missing_packs
            );
            if errors > 0 && !quiet {
                for e in vt.program.errors.iter().take(5) {
                    println!("    {e}");
                }
            }
        }
    }
    println!(
        "{} vehicles; start-up ok {ok}, failed {failed}",
        buses.len()
    );
}

/// Type the first line of the bus's first depot file and its second destination on the
/// IBIS, as the game's typist does for a duty: what came of it.
fn ibis_probe(v: &mut omsi_sim::VehicleInstance, bus: &std::path::Path) -> String {
    let Some(dir) = bus.parent() else {
        return "no folder".into();
    };
    let mut hofs: Vec<PathBuf> = omsi_cfg::vfs::read_dir_paths(dir)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("hof")))
        .collect();
    hofs.sort();
    let Some(hof) = hofs.first().and_then(|p| omsi_vehicle::Hof::load(p).ok()) else {
        return "no depot file".into();
    };
    // the first IBIS trip of the depot file, as a duty types it (`schedule::ibis_target`):
    // line and route from its code, the terminus its route leads to
    let code = hof
        .info_trips
        .first()
        .map(|t| omsi_cfg::parse_f32(&t.code) as u32);
    // OMSI_AUDIT_LINE=5E types that line instead (number and letter code as a duty does)
    let own_line = omsi_cfg::flags::OMSI_AUDIT_LINE.live_var().ok();
    let line = own_line
        .as_deref()
        .and_then(|l| {
            l.trim_end_matches(|c: char| c.is_ascii_alphabetic())
                .parse()
                .ok()
        })
        .unwrap_or(code.map(|c| c / 100).unwrap_or(0));
    let suffix = match own_line
        .as_deref()
        .and_then(|l| l.chars().last())
        .map(|c| c.to_ascii_uppercase())
    {
        Some('E') => 10,
        Some('N') => 4,
        Some('U') => 31,
        Some('S') => 23,
        Some('M') => 32,
        _ => 0,
    };
    let route = code.map(|c| c % 100).filter(|r| *r > 0);
    let Some((ti, term)) = hof
        .termini
        .iter()
        .enumerate()
        .find(|(i, t)| *i > 0 && t.code > 0)
    else {
        return format!("{}: no destination with a code", hof.name);
    };
    v.host.hof = Some(std::sync::Arc::new(hof.clone()));
    // (an IBIS trip's `route` field is the code of the terminus it runs to)
    let trip_term = hof
        .info_trips
        .first()
        .map(|t| omsi_cfg::parse_f32(&t.route) as i32)
        .and_then(|c| hof.termini.iter().position(|t| t.code == c));
    let (ti, term) = match (route, trip_term) {
        (Some(_), Some(i)) => (i, &hof.termini[i]),
        _ => (ti, term),
    };
    let target = omsi_sim::ibis::Target {
        line,
        suffix,
        route,
        terminus_code: if route.is_some() {
            None
        } else {
            Some(term.code as u32)
        },
        route_index: route.map(|_| 0),
        terminus_index: ti as i32,
        stop: 0,
    };
    let mut typist = omsi_sim::ibis::Typist::new(v, target, &|_| true, false);
    let mut t = 0.0;
    while t < 120.0 && typist.tick(v, 1.0 / 30.0) {
        v.update(1.0 / 30.0);
        t += 1.0 / 30.0;
    }
    if own_line.is_some() {
        // the line alone, as if typed (the depot file need not know it)
        v.set_var("IBIS_Linie_Complex", (line * 100 + suffix) as f32);
        v.set_var("IBIS_Linie_Suffix", suffix as f32);
        v.set_var("IBIS_LinieKurs", line as f32);
    }
    for _ in 0..300 {
        v.update(1.0 / 30.0);
    }
    // what the line displays show
    let shown: Vec<String> = ["Matrix_Nr", "IBIS_Complex_Line", "SetLineTo"]
        .iter()
        .filter_map(|n| {
            v.ty.program
                .str_var(n)
                .map(|i| format!("{n}={:?}", v.state.str_vars[i as usize]))
        })
        .collect();
    if !shown.is_empty() {
        println!("  displays: {}", shown.join(" "));
    }
    match typist.outcome() {
        Some(Ok(s)) => format!("ok: line {line} destination {} ({s})", term.code),
        Some(Err(e)) => format!("FAILED line {line} destination {}: {}", term.code, e),
        None => "unfinished".into(),
    }
}

/// The game's picture decoder for the simulation (as omsi-app registers it at start).
fn decode_picture(path: &std::path::Path) -> Result<omsi_sim::image_decode::DecodedImage, String> {
    omsi_texture::decode_file(path)
        .map(|i| omsi_sim::image_decode::DecodedImage {
            width: i.width,
            height: i.height,
            rgba: i.rgba,
            has_alpha: i.has_alpha,
        })
        .map_err(|e| e.to_string())
}
