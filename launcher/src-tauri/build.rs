fn main() {
    // the interface texts come from the game's translation tables
    println!("cargo:rerun-if-changed=../../crates/omsi-app/locales/app.yml");
    // Rebuild the embedded page when any launcher UI asset changes.
    println!("cargo:rerun-if-changed=../ui");
    tauri_build::build()
}
