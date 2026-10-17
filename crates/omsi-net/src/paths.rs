//! The paths of files another machine names (a player's bus, their figure): relative to a
//! content root, or none.

/// The longest vehicle path taken from the network.
const MAX_VEHICLE_PATH: usize = 260;

/// `Pose::bus` as a path relative to a content root that may be looked up on this machine,
/// or None. A pose comes from another machine, so the path is not trusted: it must be
/// relative (no leading `/`, no drive, no `:`), without empty, `.` or `..` parts (or parts
/// of dots only, which Windows reads as those), and name a `.bus` or `.ovh` file.
pub fn vehicle_path(bus: &str) -> Option<String> {
    relative_path(bus, &["bus", "ovh"])
}

/// A person's figure file (`.hum`) as another game names it: the same rules as
/// [`vehicle_path`] (relative, no `..`, no drive letters), for the driver and walker figure.
pub fn human_path(hum: &str) -> Option<String> {
    relative_path(hum, &["hum"])
}

fn relative_path(path: &str, exts: &[&str]) -> Option<String> {
    let p = path.trim().replace('\\', "/");
    if p.is_empty()
        || p.len() > MAX_VEHICLE_PATH
        || p.starts_with('/')
        || p.chars().any(|c| c == ':' || c == '|' || c.is_control())
    {
        return None;
    }
    if p.split('/')
        .any(|part| part.trim_matches(|c| c == '.' || c == ' ').is_empty())
    {
        return None;
    }
    let file = p.rsplit('/').next()?;
    let ext = file.rsplit_once('.')?.1.to_ascii_lowercase();
    exts.contains(&ext.as_str()).then_some(p)
}
