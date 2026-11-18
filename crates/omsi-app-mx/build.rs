//! On Windows, puts the application icon and the version (`OPENOMSI_VERSION`, as the CI
//! passes it) into the executable; Steam's library beside the binary. The version and the
//! commit the game shows are stamped by omsi-platform's `build.rs`.

fn main() {
    println!("cargo:rerun-if-env-changed=OPENOMSI_VERSION");
    windows_icon();
    steam();
    // (the executable exports the two switchable-graphics hints of main.rs, see there)
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        for sym in ["NvOptimusEnablement", "AmdPowerXpressRequestHighPerformance"] {
            println!("cargo:rustc-link-arg-bin=openomsi=/EXPORT:{sym},DATA");
        }
    }
}

/// Steam's rich presence (`src/steam.rs`) on the targets `assets/steam_redist` has the library
/// for: `cfg(steam)`, and the library copied beside the binary, where the game looks for it
/// at its start (`@loader_path` on macOS, `$ORIGIN` on Linux, the exe's folder on Windows) -
/// without it there the game does not start at all. Android, Windows on ARM and Linux on ARM
/// have no Steam library and are built without it.
fn steam() {
    println!("cargo::rustc-check-cfg=cfg(steam)");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let lib = match (os.as_str(), arch.as_str()) {
        ("windows", "x86_64") => "steam_api64.dll",
        ("linux", "x86_64") => "libsteam_api.so",
        ("macos", _) => "libsteam_api.dylib",
        _ => return,
    };
    println!("cargo:rustc-cfg=steam");
    if os == "linux" {
        println!("cargo:rustc-link-arg-bin=openomsi=-Wl,-rpath,$ORIGIN");
    }
    let src = std::path::Path::new("../../assets/steam_redist").join(lib);
    println!("cargo:rerun-if-changed={}", src.display());
    // (OUT_DIR is <target>/<profile>/build/<crate>/out: the binary lies three folders up)
    if let Some(dir) = std::env::var_os("OUT_DIR").map(std::path::PathBuf::from).and_then(|o| o.ancestors().nth(3).map(|d| d.to_path_buf())) {
        let _ = std::fs::copy(&src, dir.join(lib));
    }
}

fn windows_icon() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    println!("cargo:rerun-if-changed=../../assets/icons/app/openomsi.ico");
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/icons/app/openomsi.ico")
        .set("ProductName", "openOMSI")
        .set("FileDescription", "openOMSI")
        .set("ProductVersion", &std::env::var("OPENOMSI_VERSION").unwrap_or_default());
    if let Err(e) = res.compile() {
        println!("cargo:warning=no icon in the executable: {e}");
    }
}
