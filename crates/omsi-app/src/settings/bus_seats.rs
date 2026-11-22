use std::path::PathBuf;

fn path() -> Option<PathBuf> {
    crate::lan::data_dir().map(|d| d.join("seats.cfg"))
}

fn key(bus: &str) -> String {
    bus.trim().replace('\\', "/").to_ascii_lowercase()
}

fn read() -> Vec<(String, [f32; 3], f32)> {
    let Some(t) = path().and_then(|p| std::fs::read_to_string(p).ok()) else {
        return Vec::new();
    };
    t.lines().filter_map(parse).collect()
}

fn parse(line: &str) -> Option<(String, [f32; 3], f32)> {
    let mut f = line.rsplitn(5, '|');
    let pitch = f.next()?.trim().parse().ok()?;
    let z = f.next()?.trim().parse().ok()?;
    let y = f.next()?.trim().parse().ok()?;
    let x = f.next()?.trim().parse().ok()?;
    let bus = f.next()?.to_string();
    (!bus.is_empty()).then_some((bus, [x, y, z], pitch))
}

/// The seat kept for `bus` (its `.bus` path), if one is.
pub fn of(bus: &str) -> Option<([f32; 3], f32)> {
    let k = key(bus);
    read().into_iter().find(|e| e.0 == k).map(|e| (e.1, e.2))
}

/// Keep `seat` and `pitch` for `bus`.
pub fn remember(bus: &str, seat: [f32; 3], pitch: f32) {
    let Some(p) = path() else { return };
    let k = key(bus);
    let mut all = read();
    all.retain(|e| e.0 != k);
    all.push((k, seat, pitch));
    let text: String = all
        .iter()
        .map(|(b, s, pt)| format!("{b}|{}|{}|{}|{pt}\n", s[0], s[1], s[2]))
        .collect();
    let _ = std::fs::write(p, text);
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_line_is_read_back() {
        assert_eq!(
            super::parse("vehicles/man_nl_ng/man_en92_main.bus|0.1|-0.2|0.05|3"),
            Some((
                "vehicles/man_nl_ng/man_en92_main.bus".to_string(),
                [0.1, -0.2, 0.05],
                3.0
            ))
        );
        // (a | in the path stays in it: the numbers are the last four fields)
        assert_eq!(
            super::parse("a|b.bus|0|0|0|0").map(|e| e.0),
            Some("a|b.bus".to_string())
        );
        assert_eq!(super::parse("broken"), None);
    }
}
