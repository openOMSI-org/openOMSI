//! `openomsi-xconv`: DirectX `.x` meshes to OMSI `.o3d`, many at once, from the command
//! line - the same switches as OMSI's converter, at its defaults.
//!
//! ```text
//! openomsi-xconv [switches] file.x [more.x ...] [folder ...]
//!   --no-zup           X-File: z up off (the file is already Y up)
//!   --separate         Separate Files: one .o3d a mesh
//!   --details          Show Details of X
//!   --keep-specular    Remove Specular off
//!   --no-opt-materials Opt. Materials off
//!   --no-opt-vertices  Opt. Vertices off
//!   --normals-up       All Normals Up
//!   --local            Use Submesh Local Pos. as Global Pos.
//!   --bones            skin weights written as bones
//!   --pivot            the frame matrix kept as the pivot (origin_from_mesh)
//!   --out DIR          write into DIR instead of beside each file
//! ```
//! A folder converts every `.x` in it (not below it). The exit code is the number of files
//! that failed.

use omsi_xconv::{convert_file, Options};
use std::path::PathBuf;

fn main() {
    let mut o = Options::default();
    let mut out_dir: Option<PathBuf> = None;
    let mut files: Vec<PathBuf> = Vec::new();
    let mut args = std::env::args_os().skip(1);
    while let Some(a) = args.next() {
        let s = a.to_string_lossy();
        match s.as_ref() {
            "--no-zup" => o.z_up = false,
            "--separate" => o.separate_files = true,
            "--details" => o.show_details = true,
            "--keep-specular" => o.remove_specular = false,
            "--no-opt-materials" => o.opt_materials = false,
            "--no-opt-vertices" => o.opt_vertices = false,
            "--normals-up" => o.all_normals_up = true,
            "--local" => o.local_as_global = true,
            "--bones" => o.bones = true,
            "--pivot" => o.pivot = true,
            "--out" => out_dir = args.next().map(PathBuf::from),
            "-h" | "--help" => {
                print!("{}", HELP);
                return;
            }
            _ if s.starts_with("--") => {
                eprintln!("unknown switch {s}\n{HELP}");
                std::process::exit(2);
            }
            _ => {
                let p = PathBuf::from(&a);
                if p.is_dir() {
                    let mut found: Vec<PathBuf> = std::fs::read_dir(&p)
                        .map(|rd| rd.flatten().map(|e| e.path()).filter(|q| q.extension().is_some_and(|e| e.eq_ignore_ascii_case("x"))).collect())
                        .unwrap_or_default();
                    found.sort();
                    files.extend(found);
                } else {
                    files.push(p);
                }
            }
        }
    }
    if files.is_empty() {
        eprint!("{HELP}");
        std::process::exit(2);
    }
    if let Some(d) = &out_dir {
        if let Err(e) = std::fs::create_dir_all(d) {
            eprintln!("cannot make {}: {e}", d.display());
            std::process::exit(1);
        }
    }
    let mut failed = 0;
    for f in &files {
        match convert_file(f, out_dir.as_deref(), &o) {
            Ok(written) => {
                for (path, log) in written {
                    println!("{} -> {}", f.display(), path.display());
                    for l in log {
                        println!("    {l}");
                    }
                }
            }
            Err(e) => {
                failed += 1;
                eprintln!("{}: {e}", f.display());
            }
        }
    }
    std::process::exit(failed.min(255));
}

const HELP: &str = "openomsi-xconv [switches] file.x [more.x ...] [folder ...]
  --no-zup           X-File: z up off (the file is already Y up)
  --separate         Separate Files: one .o3d a mesh
  --details          Show Details of X
  --keep-specular    Remove Specular off
  --no-opt-materials Opt. Materials off
  --no-opt-vertices  Opt. Vertices off
  --normals-up       All Normals Up
  --local            Use Submesh Local Pos. as Global Pos.
  --bones            skin weights written as bones
  --pivot            the frame matrix kept as the pivot (origin_from_mesh)
  --out DIR          write into DIR instead of beside each file
";
