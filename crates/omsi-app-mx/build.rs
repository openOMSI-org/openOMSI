//! `cfg(steam)` for the mX game's Steam code (`src/steam.rs`), on the targets
//! `assets/steam_redist` has the library for. The executable `openomsi` belongs to omsi-app:
//! its `build.rs` gives it the icon, the exported switchable-graphics hints, the rpath and
//! Steam's library beside it, for both games.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(steam)");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    if matches!(
        (os.as_str(), arch.as_str()),
        ("windows", "x86_64") | ("linux", "x86_64") | ("macos", _)
    ) {
        println!("cargo:rustc-cfg=steam");
    }
}
