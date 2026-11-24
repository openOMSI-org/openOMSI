//! On Windows, the application icon goes into `openomsi-launcher.exe`.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    println!("cargo:rerun-if-changed=../../assets/icons/app/openomsi.ico");
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/icons/app/openomsi.ico")
        .set("ProductName", "openOMSI")
        .set("FileDescription", "openOMSI launcher tools");
    if let Err(e) = res.compile() {
        println!("cargo:warning=no icon in the executable: {e}");
    }
}
