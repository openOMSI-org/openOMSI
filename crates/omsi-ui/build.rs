//! The SVG icons in `assets/icons/{material,custom}` as `(name, svg)` pairs.
use std::io::Write;

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icons");
    let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("icons.rs");
    let mut f = std::fs::File::create(out).unwrap();
    writeln!(f, "pub static ICONS: &[(&str, &str)] = &[").unwrap();
    for folder in ["material", "custom"] {
        let dir = root.join(folder);
        println!("cargo:rerun-if-changed={}", dir.display());
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".svg"))
            .collect();
        names.sort();
        for n in names {
            let path = dir.join(&n).canonicalize().unwrap();
            writeln!(f, "    ({:?}, include_str!({:?})),", n.trim_end_matches(".svg"), path.display().to_string()).unwrap();
        }
    }
    writeln!(f, "];").unwrap();
}
