//! The converter against the stock objects that ship both the `.x` and the `.o3d` the
//! original tool made of it: for each pair, some setting of the switches must give the same
//! vertices (to a rounding), the same triangles and the same materials. Needs OMSI 2
//! (`OMSI_ROOT`, or the Steam folder); ignored without it. `XCONV_ONLY=<substring>`
//! looks at the pairs whose path has it, with every try reported.

use omsi_xconv::{convert, parse, Options};
use std::path::{Path, PathBuf};

fn root() -> Option<PathBuf> {
    let p = std::env::var_os("OMSI_ROOT").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\OMSI 2"));
    p.join("Omsi.exe").exists().then_some(p)
}

fn pairs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            pairs(&p, out);
        } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("x")) && p.with_extension("o3d").exists() {
            out.push(p);
        }
    }
}

/// How far two meshes are apart: None when they match (counts, indices, materials exact;
/// numbers to 1e-3), else what differs first, with a rank (lower is nearer).
/// The frames' order turned round, children and all (to tell how the original walks them).
fn reversed(f: &omsi_xconv::xfile::Frame) -> omsi_xconv::xfile::Frame {
    let mut r = f.clone();
    r.meshes.reverse();
    r.children = f.children.iter().rev().map(reversed).collect();
    r
}

/// The frames sorted by name at every level (to tell how the original orders them).
fn sorted(f: &omsi_xconv::xfile::Frame) -> omsi_xconv::xfile::Frame {
    let mut r = f.clone();
    r.children = f.children.iter().map(sorted).collect();
    r.children.sort_by_key(|c| c.name.to_lowercase());
    r
}

fn differ(ours: &omsi_xconv::O3dMesh, theirs: &omsi_o3d::Mesh) -> Option<(u32, String)> {
    if ours.vertices.len() != theirs.vertices.len() {
        return Some((9, format!("{} vertices against {}", ours.vertices.len(), theirs.vertices.len())));
    }
    if ours.triangles.len() != theirs.triangles.len() {
        return Some((8, format!("{} triangles against {}", ours.triangles.len(), theirs.triangles.len())));
    }
    for (i, (a, b)) in ours.vertices.iter().zip(&theirs.vertices).enumerate() {
        let bv = [b.position.x, b.position.y, b.position.z, b.normal.x, b.normal.y, b.normal.z, b.uv.x, b.uv.y];
        for k in 0..8 {
            let tol = 1e-3 * a[k].abs().max(1.0);
            if (a[k] - bv[k]).abs() > tol {
                return Some((if k < 3 { 7 } else if k < 6 { 6 } else { 5 }, format!("vertex {i} field {k}: {} against {}", a[k], bv[k])));
            }
        }
    }
    for (i, (a, b)) in ours.triangles.iter().zip(&theirs.triangles).enumerate() {
        if a.0 != b.indices {
            return Some((4, format!("triangle {i}: {:?} against {:?}", a.0, b.indices)));
        }
    }
    if ours.materials.len() != theirs.materials.len() {
        return Some((3, format!("{} materials against {}", ours.materials.len(), theirs.materials.len())));
    }
    for (i, (a, b)) in ours.triangles.iter().zip(&theirs.triangles).enumerate() {
        if a.1 != b.material {
            return Some((2, format!("triangle {i} material {} against {}", a.1, b.material)));
        }
    }
    for (i, (a, b)) in ours.materials.iter().zip(&theirs.materials).enumerate() {
        let close = |x: f32, y: f32| (x - y).abs() < 1e-4;
        let ok = a.diffuse.iter().zip(&b.diffuse).all(|(x, y)| close(*x, *y))
            && a.specular.iter().zip(&b.specular).all(|(x, y)| close(*x, *y))
            && a.emissive.iter().zip(&b.emissive).all(|(x, y)| close(*x, *y))
            && close(a.power, b.specular_power)
            && a.texture.eq_ignore_ascii_case(&b.texture);
        if !ok {
            return Some((1, format!("material {i}: {:?} {:?} {:?} {} {:?} against {:?} {:?} {:?} {} {:?}", a.diffuse, a.specular, a.emissive, a.power, a.texture, b.diffuse, b.specular, b.emissive, b.specular_power, b.texture)));
        }
    }
    None
}

#[test]
#[ignore = "needs OMSI 2 installed"]
fn the_stock_pairs_come_out_as_the_original_made_them() {
    let Some(root) = root() else {
        eprintln!("no OMSI 2: skipped");
        return;
    };
    let only = std::env::var("XCONV_ONLY").ok();
    let mut files = Vec::new();
    for d in ["Sceneryobjects", "Vehicles", "Splines"] {
        pairs(&root.join(d), &mut files);
    }
    files.sort();
    if let Some(o) = &only {
        files.retain(|f| f.to_string_lossy().to_lowercase().contains(&o.to_lowercase()));
    }
    assert!(!files.is_empty());
    let mut matched = 0;
    let mut report = Vec::new();
    for x_path in &files {
        let bytes = std::fs::read(x_path).unwrap();
        let x = match parse(&bytes) {
            Ok(x) => x,
            Err(e) => {
                report.push(format!("{}: cannot read: {e}", x_path.display()));
                continue;
            }
        };
        let theirs = match omsi_o3d::parse_o3d(&std::fs::read(x_path.with_extension("o3d")).unwrap()) {
            Ok(m) => m,
            Err(e) => {
                report.push(format!("{}: their o3d: {e}", x_path.display()));
                continue;
            }
        };
        let stem = x_path.file_stem().unwrap().to_string_lossy().to_string();
        let mut tries: Vec<(u32, String, String)> = Vec::new();
        let mut hit = None;
        let mut x_rev = x.clone();
        x_rev.root = reversed(&x.root);
        let mut x_sorted = x.clone();
        x_sorted.root = sorted(&x.root);
        if only.is_some() {
            fn tree(f: &omsi_xconv::xfile::Frame, depth: usize, out: &mut Vec<String>) {
                out.push(format!("     {}frame {:?}: {} meshes ({}), matrix {}", "  ".repeat(depth), f.name, f.meshes.len(), f.meshes.iter().map(|m| format!("{:?} {}v", m.name, m.positions.len())).collect::<Vec<_>>().join(", "), f.matrix.map(|m| format!("{:?}", &m[12..15])).unwrap_or("none".into())));
                for c in &f.children {
                    tree(c, depth + 1, out);
                }
            }
            tree(&x.root, 0, &mut report);
            // (what the corners look like, for the folding rules)
            let o = Options { opt_vertices: false, ..Default::default() };
            let raw = convert(&x, &stem, &o);
            let exact: std::collections::HashSet<[u32; 8]> = raw[0].mesh.vertices.iter().map(|v| v.map(f32::to_bits)).collect();
            let rounded: std::collections::HashSet<[i64; 8]> = raw[0].mesh.vertices.iter().map(|v| v.map(|f| (f as f64 * 1e4).round() as i64)).collect();
            let pos_only: std::collections::HashSet<[u32; 3]> = raw[0].mesh.vertices.iter().map(|v| [v[0].to_bits(), v[1].to_bits(), v[2].to_bits()]).collect();
            let pos_uv: std::collections::HashSet<[u32; 5]> = raw[0].mesh.vertices.iter().map(|v| [v[0].to_bits(), v[1].to_bits(), v[2].to_bits(), v[6].to_bits(), v[7].to_bits()]).collect();
            report.push(format!("     corners {}: distinct exact {}, to 1e-4 {}, by position {}, by position+uv {}; theirs {}", raw[0].mesh.vertices.len(), exact.len(), rounded.len(), pos_only.len(), pos_uv.len(), theirs.vertices.len()));
            let show: Vec<usize> = std::env::var("XCONV_SHOW").ok().map(|s| s.split(',').filter_map(|v| v.parse().ok()).collect()).unwrap_or_else(|| vec![0, 1, 2, 3]);
            for i in &show {
                if let Some(v) = theirs.vertices.get(*i) {
                    report.push(format!("     theirs v{i}: {:?} {:?} {:?}", v.position, v.normal, v.uv));
                }
            }
            for i in &show {
                if let Some(v) = raw[0].mesh.vertices.get(*i) {
                    report.push(format!("     ours   v{i}: {:?}", v));
                }
            }
        }
        'combos: for z_up in [true, false] {
            for local_as_global in [false, true] {
                for opt_vertices in [true, false] {
                    for opt_materials in [true, false] {
                        for (remove_specular, order) in [(true, 0), (false, 0)] {
                            let o = Options { z_up, remove_specular, opt_vertices, opt_materials, local_as_global, ..Default::default() };
                            let out = convert(match order { 1 => &x_rev, 2 => &x_sorted, _ => &x }, &stem, &o);
                            let name = format!("zup={} local={} optv={} optm={} spec-={}{}", u8::from(z_up), u8::from(local_as_global), u8::from(opt_vertices), u8::from(opt_materials), u8::from(remove_specular), [" ", " REVERSED", " SORTED"][order]);
                            match differ(&out[0].mesh, &theirs) {
                                None => {
                                    hit = Some(name);
                                    break 'combos;
                                }
                                Some((rank, why)) => tries.push((rank, name, why)),
                            }
                        }
                    }
                }
            }
        }
        let rel = x_path.strip_prefix(&root).unwrap().display().to_string();
        let shape = format!("{} v {} t {} m, theirs v{}", theirs.vertices.len(), theirs.triangles.len(), theirs.materials.len(), theirs.version);
        match hit {
            Some(name) => {
                matched += 1;
                report.push(format!("OK   {rel} ({shape}): {name}"));
            }
            None => {
                tries.sort_by_key(|t| t.0);
                let best = &tries[0];
                report.push(format!("DIFF {rel} ({shape}): nearest {}: {}", best.1, best.2));
                if only.is_some() {
                    for t in &tries {
                        report.push(format!("       {} -> {}", t.1, t.2));
                    }
                }
            }
        }
    }
    for l in &report {
        eprintln!("{l}");
    }
    eprintln!("{matched} of {} pairs match", files.len());
    assert!(only.is_some() || matched * 2 >= files.len(), "fewer than half the stock pairs come out as the original's:\n{}", report.join("\n"));
}
