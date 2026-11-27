use std::path::Path;

/// The prefix of the name of a plugin that belongs to a vehicle.
pub const VEHICLE_PREFIX: &str = "vehicle:";
/// The prefix of the name of a plugin that drives scenery objects (`[lua_script]` of a `.sco`).
pub const OBJECT_PREFIX: &str = "object:";

/// The plugin name an object script runs under: its file name plus a short hash of the whole
/// path, so `Objects/A/gate.lua` and `Objects/B/gate.lua` are two scripts. The game loads a
/// script and routes its events by this name alone.
pub fn object_script_name(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "script".into());
    // FNV-1a over the path as written, with one kind of separator and no letter case
    let mut h: u32 = 0x811c_9dc5;
    for b in path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
        .bytes()
    {
        h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    format!("{OBJECT_PREFIX}{stem}-{h:08x}")
}
