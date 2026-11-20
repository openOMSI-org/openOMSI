use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=defaults");
    let dir = Path::new(&env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR")).join("defaults");
    let mut names = Vec::new();
    for entry in fs::read_dir(&dir).unwrap_or_else(|e| panic!("wheel defaults: {e}")) {
        let entry = entry.unwrap_or_else(|e| panic!("wheel defaults: {e}"));
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_else(|| panic!("wheel defaults: {}", path.display())).to_string();
        println!("cargo:rerun-if-changed=defaults/{name}");
        names.push(name);
    }
    names.sort();
    let mut out = String::from("pub static FILES: &[&str] = &[\n");
    for name in &names {
        out.push_str("    include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/defaults/");
        out.push_str(name);
        out.push_str("\")),\n");
    }
    out.push_str("];\n");
    let dest = Path::new(&env::var("OUT_DIR").expect("OUT_DIR")).join("wheel_defaults_files.rs");
    fs::write(&dest, out).unwrap_or_else(|e| panic!("wheel defaults: {e}"));
}
