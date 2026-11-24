pub fn absolute_like(s: &str) -> bool {
    let t = s.trim_start();
    let b = t.as_bytes();
    t.starts_with(['/', '\\'])
        || (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':')
        || t.contains(":\\")
        || t.contains(":/")
}

pub fn freetex_ok(s: &str) -> bool {
    let t = s.trim();
    t.is_empty() || !(t.contains(':') || t.starts_with(['/', '\\']))
}

pub fn remote_string_ok(s: &str, picture: bool) -> bool {
    if picture {
        freetex_ok(s)
    } else {
        !absolute_like(s)
    }
}

pub(crate) fn only_relative(pictures: Vec<String>) -> Vec<String> {
    pictures
        .into_iter()
        .map(|p| if freetex_ok(&p) { p } else { String::new() })
        .collect()
}

/// Is `s` a vehicle's fingerprint (`v2-`, or the older `v1-`, and 64 lower-case hex digits)?
/// (`v2` hashes the bus file's path too; `v1` is still recognised on the wire but no longer
/// produced - see the game's `lan_mods::identity`.)
pub fn is_fingerprint(s: &str) -> bool {
    s.strip_prefix("v2-")
        .or_else(|| s.strip_prefix("v1-"))
        .is_some_and(|h| {
            h.len() == 64
                && h.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_from_the_network_stays_relative() {
        for good in [
            "",
            r"..\..\Anzeigen\Rollband_FC\Paris\17.tga",
            "Texture/blind.bmp",
            "17.tga",
        ] {
            assert!(freetex_ok(good), "{good}");
        }
        for bad in [
            r"\\attacker.example\s\x.bmp",
            "//attacker.example/s/x.bmp",
            r"C:\Windows\x.bmp",
            "c:x.bmp",
            "/dev/zero",
            r"\x.bmp",
            r"\\?\UNC\host\s\x.bmp",
            r"..\..\C:\x.bmp",
        ] {
            assert!(!freetex_ok(bad), "{bad}");
        }
        assert_eq!(
            only_relative(vec!["a.tga".into(), r"\\h\s\x.bmp".into(), "b.tga".into()]),
            vec!["a.tga".to_string(), String::new(), "b.tga".to_string()]
        );
    }

    #[test]
    fn a_text_may_hold_a_colon_but_not_a_path_off_the_machine() {
        assert!(remote_string_ok("12:30 Hauptbahnhof", false));
        assert!(remote_string_ok("Linie 5", false));
        assert!(!remote_string_ok(r"\\attacker\s\x.bmp", false));
        assert!(!remote_string_ok("D:/x.bmp", false));
        assert!(!remote_string_ok("12:30 Hauptbahnhof", true));
    }
}
