use std::fmt::Write;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let assets = manifest.join("../../assets/icons");
    let mut names: Vec<(String, PathBuf)> = Vec::new();
    for dir in ["material", "custom"] {
        let d = assets.join(dir);
        println!("cargo:rerun-if-changed={}", d.display());
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("svg") {
                let name = p.file_stem().unwrap().to_string_lossy().to_string();
                names.push((name, p));
            }
        }
    }
    names.sort();
    names.dedup_by(|a, b| a.0 == b.0);
    let mut out = String::from("pub(crate) static ICONS: &[(&str, &str)] = &[\n");
    for (n, p) in &names {
        writeln!(out, "    ({n:?}, include_str!({:?})),", p.to_string_lossy().replace('\\', "/")).unwrap();
    }
    out.push_str("];\n");
    let dest = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("icons.rs");
    std::fs::write(dest, out).unwrap();
}
