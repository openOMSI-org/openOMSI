//! The status page's world: what an older server told and what a newer one tells.

use super::ServerInfo;

/// The status tells the world's date with its time (the launcher shows both); anything that
/// is not a date reads as none, and an older server's status has none.
#[test]
fn the_status_tells_the_date() {
    let i = ServerInfo { name: "S".into(), date: "2026-11-03".into(), time: "08:00".into(), ..Default::default() };
    let b = ServerInfo::from_json(&i.to_json()).unwrap();
    assert_eq!((b.date.as_str(), b.time.as_str()), ("2026-11-03", "08:00"));
    let bad = i.to_json().replace("2026-11-03", "tomorrow");
    assert_eq!(ServerInfo::from_json(&bad).unwrap().date, "");
    let old = r#"{"name":"Old Server","map":"maps/Test/global.cfg","time":"08:00"}"#;
    assert_eq!(ServerInfo::from_json(old).unwrap().date, "");
}
