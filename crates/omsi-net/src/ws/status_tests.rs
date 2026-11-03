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

#[test]
fn server_fleet_catalog_is_optional_for_old_servers() {
    let old = r#"{"name":"Old Server","map":"maps/Test/global.cfg","vehicles":"Vehicles/Test/test.bus"}"#;
    let read = ServerInfo::from_json(old).unwrap();
    assert_eq!(read.fleet_version, 0);
    assert!(read.fleet.is_empty());
}

/// A server's owner may give its address without the tunnel (`public_address`): told as
/// `direct`, for a joining game to try first; anything that is no address reads as none.
#[test]
fn the_status_tells_the_direct_address() {
    let i = ServerInfo { name: "S".into(), direct: "203.0.113.7:27025".into(), ..Default::default() };
    assert_eq!(ServerInfo::from_json(&i.to_json()).unwrap().direct, "203.0.113.7:27025");
    let bad = i.to_json().replace("203.0.113.7:27025", "a b/c");
    assert_eq!(ServerInfo::from_json(&bad).unwrap().direct, "");
    assert_eq!(ServerInfo::from_json(r#"{"name":"Old"}"#).unwrap().direct, "");
}
