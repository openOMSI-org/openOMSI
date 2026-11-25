//! What INFO says of a player's bus: its content fingerprint and its height.

use super::*;

#[test]
fn info_carries_what_the_bus_is_and_its_height_and_an_older_info_has_neither() {
    let fp = format!("v1-{}", "0123456789abcdef".repeat(4));
    let mut p = pose(1.5);
    p.content = fp.clone();
    p.height = 3.1;
    p.freetex = vec!["17.tga".into()];
    let text = p.encode_info();
    let q = Pose::decode_info(&text.split('|').collect::<Vec<_>>()).unwrap();
    assert_eq!(q.content, fp);
    assert!((q.height - 3.1).abs() < 0.01);
    assert_eq!(q.freetex, p.freetex);
    // an older game's INFO ends with the pictures
    let older: Vec<&str> = text.split('|').take(15).collect();
    let q = Pose::decode_info(&older).unwrap();
    assert!(q.content.is_empty() && q.height == 0.0);
    assert_eq!(q.freetex, p.freetex);
    // what is no fingerprint is not taken for one
    for bad in ["v1-xyz", "v2-0123", &fp.to_uppercase(), "Vehicles/x.bus"] {
        let t = text.replace(&fp, bad);
        assert!(
            Pose::decode_info(&t.split('|').collect::<Vec<_>>())
                .unwrap()
                .content
                .is_empty(),
            "{bad}"
        );
    }
    // with everything else at its longest the fingerprint still goes whole
    p.freetex = (0..MAX_FREETEX)
        .map(|k| format!("{k}{}", "é".repeat(200)))
        .collect();
    p.bus = format!("Vehicles/{}/{}.bus", "Ü".repeat(60), "b".repeat(120));
    p.texts = (0..MAX_TEXTS)
        .map(|k| format!("{k}ß{}", "ñ".repeat(40)))
        .collect();
    p.name = "Ж".repeat(40);
    p.paint = "Ö".repeat(70);
    p.line = "Щ".repeat(20);
    p.destination = "Weiden (Oberpfalz) Bahnhof/ZOB – über Stockerhut ".repeat(3);
    p.tour = "ä".repeat(70);
    p.figure = format!("Humans/{}/{}.hum", "é".repeat(60), "f".repeat(120));
    let text = p.encode_info();
    assert!(text.len() <= MAX_DATAGRAM, "{} bytes", text.len());
    assert_eq!(
        Pose::decode_info(&text.split('|').collect::<Vec<_>>())
            .unwrap()
            .content,
        fp
    );
    // the state keeps it, as it keeps the rest of the info
    let mut kept = Pose::decode_info(&text.split('|').collect::<Vec<_>>()).unwrap();
    kept.set_state(pose(2.0));
    assert_eq!(kept.content, fp);
}
