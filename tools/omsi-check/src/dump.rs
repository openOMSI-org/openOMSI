//! `--dump <out>`: a fingerprint of what every content file parses to, one line per file
//! (`path <tab> hash <tab> size of the description`), for before/after comparisons of a
//! loader change: two dumps made by two builds differ exactly in the files whose parsed
//! structure changed. `--dump-detail <list>` then prints the whole structure of the files
//! named in `list` (one path per line, as the dump writes them) to compare them in full.
//!
//! Folders and mounted archives (`--content x.zip`) are walked through the VFS alike.
//! Vehicles and scenery objects include their compiled scripts, models their resolved
//! mesh files.

use omsi_cfg::vfs;
use rayon::prelude::*;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Every file below `dir`, through the VFS (folders and mounted archives).
pub fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Some(entries) = vfs::list_dir(dir) else {
        return;
    };
    for (name, is_dir) in entries {
        let p = dir.join(&name);
        if is_dir {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Built-in variables (road vehicle, road vehicle strings, scenery object), set by `main`.
pub static BUILTINS: std::sync::OnceLock<(Vec<String>, Vec<String>, Vec<String>)> =
    std::sync::OnceLock::new();

/// The compiled scripts of an object (varlists, stringvarlists, scripts, constfiles), in a
/// stable form (the program's own maps are hash maps).
fn scripts(lists: [&[PathBuf]; 4], vehicle: bool) -> String {
    if lists[2].is_empty() {
        return String::new();
    }
    let (rv, rvs, so) = BUILTINS.get().cloned().unwrap_or_default();
    let mut inp = omsi_script::CompileInput {
        varlists: lists[0].to_vec(),
        stringvarlists: lists[1].to_vec(),
        scripts: lists[2].to_vec(),
        constfiles: lists[3].to_vec(),
        ..Default::default()
    };
    if vehicle {
        inp.builtin_vars = rv;
        inp.builtin_str_vars = rvs;
        for a in 0..8 {
            for side in ["L", "R"] {
                for pre in ["Wheel_Rotation_", "Axle_Steering_", "Axle_Suspension_"] {
                    inp.builtin_vars.push(format!("{pre}{a}_{side}"));
                }
            }
        }
    } else {
        inp.builtin_vars = so;
    }
    let p = omsi_script::compile(&inp);
    let mut macros: Vec<String> = p.macros.keys().cloned().collect();
    macros.sort();
    let mut triggers: Vec<String> = p.triggers.keys().cloned().collect();
    triggers.sort();
    let blocks: Vec<String> = p
        .blocks
        .iter()
        .map(|b| format!("{} {:?}", b.name, b.ops))
        .collect();
    let mut errors: Vec<String> = p.errors.iter().map(|e| e.to_string()).collect();
    errors.sort();
    format!(
        "scripts: blocks {blocks:#?}\nmacros {macros:?}\ntriggers {triggers:?}\nerrors {errors:#?}"
    )
}

/// Whether every mesh file of a model resolves to an existing file (and where).
fn meshes(dir: &Path, model: &omsi_model::Model) -> String {
    let mut s = String::from("meshes:\n");
    for m in &model.meshes {
        let p = omsi_cfg::resolve_path(dir, &m.file);
        s.push_str(&format!(
            "  {} -> {} {}\n",
            m.file,
            p.display(),
            vfs::is_file(&p)
        ));
    }
    s
}

/// What `p` parses to, or None for a file no loader reads.
pub fn describe(p: &Path) -> Option<String> {
    let name = p.file_name()?.to_string_lossy().to_ascii_lowercase();
    let ext = p
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let lower = p.to_string_lossy().to_ascii_lowercase().replace('\\', "/");
    let dir = p.parent().unwrap_or(Path::new(""));
    let err = |e: omsi_cfg::CfgError| format!("error: {e}");
    let s = match ext.as_str() {
        "bus" | "ovh" => match omsi_vehicle::Vehicle::load(p) {
            Ok(v) => {
                let s = &v.scripts;
                format!(
                    "{v:#?}\n{}",
                    scripts(
                        [&s.varlists, &s.stringvarlists, &s.scripts, &s.constfiles],
                        true
                    )
                )
            }
            Err(e) => err(e),
        },
        "sco" => match omsi_scenery::SceneryObject::load(p) {
            Ok(o) => {
                let s = &o.scripts;
                format!(
                    "{o:#?}\n{}\n{}",
                    meshes(dir, &o.model),
                    scripts(
                        [&s.varlists, &s.stringvarlists, &s.scripts, &s.constfiles],
                        false
                    )
                )
            }
            Err(e) => err(e),
        },
        "sli" => match omsi_scenery::Spline::load(p) {
            Ok(s) => format!("{s:#?}"),
            Err(e) => err(e),
        },
        "hum" => match omsi_content::Human::load(p) {
            Ok(h) => format!("{h:#?}"),
            Err(e) => err(e),
        },
        "hof" => match omsi_vehicle::Hof::load(p) {
            Ok(h) => format!("{h:#?}"),
            Err(e) => err(e),
        },
        "owt" => match omsi_content::Weather::load(p) {
            Ok(w) => format!("{w:#?}"),
            Err(e) => err(e),
        },
        "oft" => match omsi_content::Font::load_all(p) {
            Ok(f) => format!("{f:#?}"),
            Err(e) => err(e),
        },
        "cti" => match omsi_content::tickets::TicketItems::load(p) {
            Ok(t) => format!("{t:#?}"),
            Err(e) => err(e),
        },
        "otp" => match omsi_content::TicketPack::load(p) {
            Ok(t) => format!("{t:#?}"),
            Err(e) => err(e),
        },
        "odr" => match omsi_content::Driver::load(p) {
            Ok(d) => format!("{d:#?}"),
            Err(e) => err(e),
        },
        "osn" => match omsi_content::Situation::load(p) {
            Ok(s) => format!("{s:#?}"),
            Err(e) => err(e),
        },
        "ttp" => format!("{:#?}", omsi_timetable::Trip::load(p).map_err(err)),
        "ttr" => format!("{:#?}", omsi_timetable::Track::load(p).map_err(err)),
        "ttl" => format!("{:#?}", omsi_timetable::Line::load(p).map_err(err)),
        "ocu" => format!("{:#?}", omsi_timetable::CarUse::load(p).map_err(err)),
        "map" if name.starts_with("tile_") || lower.contains("/chrono/") => {
            format!("{:#?}", omsi_map::Tile::load(p).map_err(err))
        }
        "cfg" => {
            if name == "global.cfg" {
                format!("{:#?}", omsi_map::GlobalCfg::load(p).map_err(err))
            } else if name.starts_with("ailists") {
                format!("{:#?}", omsi_map::AiLists::load(p).map_err(err))
            } else if name == "envir.cfg" {
                format!("{:#?}", omsi_content::Envir::load(p).map_err(err))
            } else if name.contains("passengercabin") {
                format!("{:#?}", omsi_vehicle::PassengerCabin::load(p).map_err(err))
            } else if name.starts_with("paths") {
                format!("{:#?}", omsi_vehicle::VehiclePaths::load(p).map_err(err))
            } else if name.contains("sound") {
                format!("{:#?}", omsi_vehicle::SoundCfg::load(p).map_err(err))
            } else if lower.contains("/model/")
                || lower.contains("/model_")
                || name.starts_with("model")
            {
                match omsi_model::Model::load(p) {
                    Ok(m) => format!("{m:#?}\n{}", meshes(dir, &m)),
                    Err(e) => err(e),
                }
            } else {
                return None;
            }
        }
        _ => return None,
    };
    Some(s)
}

/// The file's name in a dump: relative to the content root it lies under.
fn dump_name(p: &Path, roots: &[PathBuf]) -> String {
    for r in roots {
        if let Ok(rel) = p.strip_prefix(r) {
            return format!(
                "{}/{}",
                r.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                rel.display()
            );
        }
    }
    p.display().to_string()
}

pub fn dump(roots: &[PathBuf], out: &Path) -> std::io::Result<()> {
    let mut files = Vec::new();
    for r in roots {
        walk(r, &mut files);
    }
    let t0 = std::time::Instant::now();
    let mut rows: Vec<String> = files
        .par_iter()
        .filter_map(|p| {
            let d = describe(p)?;
            Some(format!(
                "{}\t{:016x}\t{}",
                dump_name(p, roots),
                fnv(&d),
                d.len()
            ))
        })
        .collect();
    rows.sort();
    let mut f = std::io::BufWriter::new(std::fs::File::create(out)?);
    for r in &rows {
        writeln!(f, "{r}")?;
    }
    println!(
        "[dump] {} of {} files described in {:.0} s -> {}",
        rows.len(),
        files.len(),
        t0.elapsed().as_secs_f64(),
        out.display()
    );
    Ok(())
}

pub fn dump_detail(roots: &[PathBuf], list: &Path, out: &Path) -> std::io::Result<()> {
    let wanted: Vec<String> = std::fs::read_to_string(list)?
        .lines()
        .map(|l| l.split('\t').next().unwrap_or("").to_string())
        .filter(|l| !l.is_empty())
        .collect();
    let mut files = Vec::new();
    for r in roots {
        walk(r, &mut files);
    }
    let mut f = std::io::BufWriter::new(std::fs::File::create(out)?);
    for w in &wanted {
        let Some(p) = files.iter().find(|p| &dump_name(p, roots) == w) else {
            writeln!(f, "== {w}\n(not found)")?;
            continue;
        };
        writeln!(f, "== {w}\n{}", describe(p).unwrap_or_default())?;
    }
    Ok(())
}
