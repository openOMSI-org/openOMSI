use omsi_ui::i18n::{Piece, templated, templates};

#[test]
fn the_template_keys_of_the_tables_are_read_as_the_code_writes_them() {
    let raw = super::_LOCALES
        .lines()
        .filter(|l| l.starts_with('"') && l.contains('{'))
        .count();
    assert_eq!(super::locale_keys(super::_LOCALES).count(), raw);
    let used: Vec<String> = templates(super::locale_keys(super::_LOCALES))
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    for k in [
        "Graphics profile \"{name}\" not found",
        "Mirror panel added ({} in all)",
        "Head tracking with opentrack (UDP port {})",
    ] {
        assert!(used.iter().any(|x| x == k), "{k}");
    }
}

#[test]
fn multiplayer_messages_are_translated_in_traditional_chinese() {
    for key in [
        "Host's mods could not be fetched: {e}",
        "Host's mods: everything the host uses is installed here",
        "LAN: other players join with the code {}  (or by an address: {})",
        "LAN: the host's weather: {}",
        "Sent to the server: {action}",
        "The server does not offer {name}",
        "Online: {n} other players",
        "Online: nobody else yet",
        "Online: 1 other player",
        "Online: connecting ...",
        "Online: not connected: {reason} (chat /reconnect)",
        "Online: in {host}'s game, {others}",
        "Online: {warning}",
        "nobody else yet",
        "1 other player",
        "{n} other players",
    ] {
        let translated =
            super::_rust_i18n_try_translate("zh-tw", key).map(|text| text.into_owned());
        assert!(
            translated
                .as_ref()
                .is_some_and(|text| !text.trim().is_empty() && text != key),
            "zh-tw: {key}"
        );
    }
}

#[test]
fn a_translated_template_is_left_as_it_is_when_translated_again() {
    let t = templates(super::locale_keys(super::_LOCALES));
    for lang in super::_rust_i18n_available_locales() {
        let tr = |key: &str| super::_rust_i18n_try_translate(&lang, key).map(|t| t.into_owned());
        for (key, pieces) in &t {
            // the text the code would draw: a value per placeholder, the same one for a name used twice
            let mut names: Vec<&str> = Vec::new();
            let sample: String = pieces
                .iter()
                .map(|p| match p {
                    Piece::Text(s) => s.clone(),
                    Piece::Hole(n) => {
                        if n.is_empty() || !names.contains(&n.as_str()) {
                            names.push(n);
                        }
                        format!(
                            "V{}",
                            if n.is_empty() {
                                names.len()
                            } else {
                                names.iter().position(|x| x == n).unwrap() + 1
                            }
                        )
                    }
                })
                .collect();
            let Some(once) = templated(&t, &sample, tr) else {
                continue;
            };
            // no value is lost (a named one may be used twice: "{e} electrics ... with {e}")
            for v in sample
                .split('V')
                .skip(1)
                .filter_map(|s| s.split(|c: char| !c.is_ascii_digit()).next())
            {
                assert!(once.contains(&format!("V{v}")), "{lang}: {key}: {once}");
            }
            assert!(
                templated(&t, &once, tr).is_none_or(|twice| twice == once),
                "{lang}: {key}: {once}"
            );
        }
    }
}
