use std::{borrow::Cow, collections::HashMap};

const NOTICE_KEYS: &[&str] = &[
    "connected to {host}'s session ({players} player(s))",
    "In this session: {players}",
    "{name} joined with {vehicle}",
    "{name} joined",
    "{name} is back",
    "{name} left",
    "{name} lost the connection",
    "{name} was sent away by the host",
    "the host turned us away: {reason}",
    "the host sent you away: {reason}",
    "reconnecting ...",
    "only a joined game can reconnect",
    "asked the server for its administration",
    "the host ended the session",
    "the host has gone quiet; trying to reach it again",
    "the host has not answered for {:.0} s - the session is over (playing on alone; still trying, or type /reconnect)",
];

#[derive(Default)]
pub(super) struct TranslationCache {
    language: String,
    lines: HashMap<String, String>,
}

impl TranslationCache {
    pub(super) fn line<'a>(
        &'a mut self,
        line: &'a str,
        language: &str,
        lookup: impl Fn(&str) -> Option<String>,
    ) -> Cow<'a, str> {
        if !line.starts_with("* ") {
            return match line.strip_prefix("Admin (private): ").and_then(|text| Some((text, lookup("Admin (private)")?))) {
                Some((text, admin)) => Cow::Owned(format!("{admin}: {text}")),
                None => Cow::Borrowed(line),
            };
        }
        if self.language != language {
            self.language = language.to_owned();
            self.lines.clear();
        }
        if !self.lines.contains_key(line) {
            if self.lines.len() >= omsi_lan::game::CHAT_KEEP {
                self.lines.clear();
            }
            self.lines
                .insert(line.to_owned(), translated_chat_line(line, lookup));
        }
        Cow::Borrowed(
            self.lines
                .get(line)
                .expect("chat translation cache entry was just inserted"),
        )
    }
}

pub(super) fn translated_chat_line(line: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let Some(notice) = line.strip_prefix("* ") else {
        return line.into();
    };
    if let Some(players) = notice.strip_prefix("In this session: ") {
        let Some(translation) = lookup("In this session: {players}") else {
            return line.into();
        };
        // Only the first entry carries the host marker; names and vehicle identifiers are data.
        let players = if let Some((host, rest)) = players.split_once(" (host") {
            if rest == ")" || rest.starts_with("), ") || rest.starts_with(", ") {
                if let Some((vehicle, tail)) =
                    rest.strip_prefix(", ").and_then(|r| r.split_once(')'))
                {
                    lookup("{name} (host, {vehicle})")
                        .map(|t| t.replace("{vehicle}", vehicle).replace("{name}", host) + tail)
                } else {
                    rest.strip_prefix(')').and_then(|tail| {
                        lookup("{name} (host)").map(|t| t.replace("{name}", host) + tail)
                    })
                }
                .unwrap_or_else(|| players.into())
            } else {
                players.into()
            }
        } else {
            players.into()
        };
        return format!("* {}", translation.replace("{players}", &players));
    }
    for key in ["the host turned us away: {reason}", "the host sent you away: {reason}"] {
        if let Some(reason) = notice.strip_prefix(key.trim_end_matches("{reason}")) {
            return lookup(key).map_or_else(|| line.into(), |t| format!("* {}", t.replace("{reason}", &omsi_ui::tr(reason))));
        }
    }
    static TEMPLATES: std::sync::OnceLock<Vec<omsi_ui::i18n::Template>> =
        std::sync::OnceLock::new();
    let templates = TEMPLATES.get_or_init(|| {
        // These are known system notices, so leading placeholders are safe here.
        let mut templates = Vec::new();
        for key in NOTICE_KEYS {
            let mut pieces = vec![omsi_ui::i18n::Piece::Text("* ".into())];
            let mut rest = *key;
            while let Some(open) = rest.find('{') {
                if open > 0 {
                    pieces.push(omsi_ui::i18n::Piece::Text(rest[..open].into()));
                }
                let close = rest.find('}').unwrap();
                pieces.push(omsi_ui::i18n::Piece::Hole(rest[open + 1..close].split(':').next().unwrap_or("").into()));
                rest = &rest[close + 1..];
            }
            if !rest.is_empty() {
                pieces.push(omsi_ui::i18n::Piece::Text(rest.into()));
            }
            templates.push((key.to_string(), pieces));
        }
        templates.sort_by_key(|(_, pieces)| {
            std::cmp::Reverse(
                pieces
                    .iter()
                    .map(|piece| match piece {
                        omsi_ui::i18n::Piece::Text(text) => text.len(),
                        _ => 0,
                    })
                    .sum::<usize>(),
            )
        });
        templates
    });
    let translated = lookup(notice).or_else(|| {
        omsi_ui::i18n::templated(templates, line, |key| {
            NOTICE_KEYS.contains(&key).then(|| lookup(key)).flatten()
        })
    });
    translated
        .map(|text| format!("* {text}"))
        .unwrap_or_else(|| line.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_chat_notices_translate_before_wrapping_and_keep_player_content() {
        let translate = |line| {
            translated_chat_line(line, |key| {
                crate::_rust_i18n_try_translate("zh-tw", key).map(|text| text.into_owned())
            })
        };
        assert_eq!(
            translate("* connected to OMSI-Fan's session (2 player(s))"),
            "* 已連線至 OMSI-Fan 的遊戲（2 位玩家）"
        );
        assert_eq!(
            translate("* In this session: OMSI-Fan (host, MAN_EN92_main)"),
            "* 本次連線的玩家：OMSI-Fan（房主，MAN_EN92_main）"
        );
        assert_eq!(
            translate("* In this session: OMSI-Fan (host), Alice (MAN_NL202)"),
            "* 本次連線的玩家：OMSI-Fan（房主）, Alice (MAN_NL202)"
        );
        assert_eq!(
            translate("* Alice {name} joined with MAN_EN92_main"),
            "* Alice {name} 已加入，駕駛 MAN_EN92_main"
        );
        assert_eq!(translate("* Alice left"), "* Alice 已離開");
        assert_eq!(translate("* reconnecting ..."), "* 正在重新連線...");
        let away = crate::_rust_i18n_try_translate("zh-tw", "the host sent you away: {reason}").unwrap().replace("{reason}", "banned");
        assert_eq!(translate("* the host sent you away: banned"), format!("* {away}"));
        for text in [
            "Alice: connected to OMSI-Fan's session (2 player(s))",
            "Alice: In this session: OMSI-Fan (host)",
            "* A custom server announcement",
        ] {
            assert_eq!(translate(text), text);
        }
        let original = "* connected to OMSI-Fan's session (2 player(s))";
        assert_eq!(translated_chat_line(original, |_| None), original);
    }

    #[test]
    fn translation_cache_reuses_lines_and_reloads_after_language_changes() {
        use std::cell::Cell;

        let mut cache = TranslationCache::default();
        let lookups = Cell::new(0);
        let lookup_language = Cell::new("zh-tw");
        let lookup = |key: &str| {
            lookups.set(lookups.get() + 1);
            crate::_rust_i18n_try_translate(lookup_language.get(), key)
                .map(|text| text.into_owned())
        };
        let line = "* Alice left";
        assert_eq!(cache.line(line, "zh-tw", lookup), "* Alice 已離開");
        let first_lookup_count = lookups.get();
        assert_eq!(cache.line(line, "zh-tw", lookup), "* Alice 已離開");
        assert_eq!(lookups.get(), first_lookup_count);
        lookup_language.set("en");
        assert_eq!(cache.line(line, "en", lookup), line);
        assert!(lookups.get() > first_lookup_count);
    }

    #[test]
    fn ordinary_chat_lines_borrow_the_input_without_caching_or_lookup() {
        use std::cell::Cell;

        let mut cache = TranslationCache::default();
        let lookups = Cell::new(0);
        let line = "Alice: hello";
        let borrowed_input = matches!(
            cache.line(line, "zh-tw", |_| {
                lookups.set(lookups.get() + 1);
                None
            }),
            Cow::Borrowed(value) if std::ptr::eq(value, line)
        );

        assert!(borrowed_input);
        assert!(cache.lines.is_empty());
        assert_eq!(lookups.get(), 0);
    }

    #[test]
    fn translation_cache_discards_old_entries_at_the_chat_limit() {
        let mut cache = TranslationCache::default();
        for index in 0..omsi_lan::game::CHAT_KEEP {
            let line = format!("* system notice {index}");
            assert_eq!(cache.line(&line, "en", |_| None), line);
        }
        assert_eq!(cache.lines.len(), omsi_lan::game::CHAT_KEEP);

        let latest = "* latest system notice";
        assert_eq!(cache.line(latest, "en", |_| None), latest);

        assert_eq!(cache.lines.len(), 1);
        assert!(cache.lines.contains_key(latest));
    }
}
