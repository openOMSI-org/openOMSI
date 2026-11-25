//! The interface's languages and the spellings of them a settings file may use.

/// The interface's languages: the settings' code (OMSI's three-letter style), the name in
/// the language itself, the interface tables' code, other spellings a file may use, and the
/// machine translation's (NLLB) name of it (empty: none).
/// OMSI's own texts (key names, `.dsc` descriptions, tutorials) exist in English, German
/// and French: every other language shows those in English.
pub const LANGUAGES: &[(&str, &str, &str, &[&str], &str)] = &[
    ("ENG", "English", "en", &["en", "english"], ""),
    (
        "DEU",
        "Deutsch",
        "de",
        &["de", "ger", "german", "deutsch"],
        "deu_Latn",
    ),
    (
        "FRA",
        "Français",
        "fr",
        &["fr", "fre", "french", "francais", "français"],
        "fra_Latn",
    ),
    (
        "RUS",
        "Русский",
        "ru",
        &["ru", "russian", "русский"],
        "rus_Cyrl",
    ),
    (
        "SRP",
        "Srpski (latinica)",
        "sr",
        &[
            "sr",
            "srp",
            "sr-latn",
            "sr_latn",
            "serbian",
            "serbian latin",
            "srpski",
        ],
        "",
    ),
    (
        "UKR",
        "Українська",
        "uk",
        &["uk", "ua", "ukrainian", "українська"],
        "ukr_Cyrl",
    ),
    (
        "BEL",
        "Беларуская",
        "be",
        &["be", "by", "belarusian", "беларуская"],
        "bel_Cyrl",
    ),
    (
        "KAZ",
        "Қазақша",
        "kk",
        &["kk", "kz", "kazakh", "қазақша"],
        "kaz_Cyrl",
    ),
    (
        "POL",
        "Polski",
        "pl",
        &["pl", "polish", "polski"],
        "pol_Latn",
    ),
    (
        "CZE",
        "Čeština",
        "cs",
        &["cs", "cz", "czech", "čeština", "ces"],
        "ces_Latn",
    ),
    (
        "HUN",
        "Magyar",
        "hu",
        &["hu", "hungarian", "magyar"],
        "hun_Latn",
    ),
    (
        "ESP",
        "Español",
        "es",
        &["es", "spa", "spanish", "español"],
        "spa_Latn",
    ),
    (
        "CAT",
        "Català",
        "ca",
        &["ca", "cat", "ca-es", "ca-ad", "catalan", "català", "catala"],
        "cat_Latn",
    ),
    (
        "PTB",
        "Português (Brasil)",
        "pt",
        &["pt", "br", "pt-br", "por", "portuguese", "português"],
        "por_Latn",
    ),
    (
        "PTP",
        "Português (Portugal)",
        "pt-pt",
        &[
            "pt-pt",
            "pt_pt",
            "pt-portugal",
            "portuguese-portugal",
            "português (portugal)",
            "português de portugal",
        ],
        "por_Latn",
    ),
    (
        "ITA",
        "Italiano",
        "it",
        &["it", "italian", "italiano"],
        "ita_Latn",
    ),
    (
        "NLD",
        "Nederlands",
        "nl",
        &["nl", "dutch", "nederlands"],
        "nld_Latn",
    ),
    (
        "SWE",
        "Svenska",
        "sv",
        &["sv", "se", "sv-se", "sv-fi", "swedish", "svenska"],
        "swe_Latn",
    ),
    (
        "TUR",
        "Türkçe",
        "tr",
        &["tr", "turkish", "türkçe"],
        "tur_Latn",
    ),
    (
        "JPN",
        "日本語",
        "ja",
        &["ja", "jp", "japanese", "日本語"],
        "jpn_Jpan",
    ),
    (
        "ZHT",
        "繁體中文",
        "zh-tw",
        &[
            "zh-tw",
            "zh-hant",
            "zh-hk",
            "zh-mo",
            "cht",
            "traditional chinese",
            "繁體中文",
            "繁体中文",
        ],
        "zho_Hant",
    ),
    (
        "KOR",
        "한국어",
        "ko",
        &["ko", "kr", "korean", "한국어"],
        "kor_Hang",
    ),
    ("THA", "ไทย", "th", &["th", "thai", "ไทย"], "tha_Thai"),
    (
        "VIE",
        "Tiếng Việt",
        "vi",
        &["vi", "vietnamese", "tiếng việt"],
        "vie_Latn",
    ),
    (
        "IND",
        "Bahasa Indonesia",
        "id",
        &["id", "indonesian", "bahasa indonesia"],
        "ind_Latn",
    ),
    (
        "MSA",
        "Bahasa Melayu",
        "ms",
        &["ms", "malay", "bahasa melayu"],
        "zsm_Latn",
    ),
    (
        "TGL",
        "Filipino",
        "tl",
        &["tl", "fil", "filipino", "tagalog"],
        "tgl_Latn",
    ),
    (
        "CHS",
        "中文 (简体)",
        "zh",
        &[
            "zh",
            "zh-cn",
            "zh-sg",
            "zh-hans",
            "zhs",
            "chs",
            "cn",
            "chinese",
            "simplified chinese",
            "简体中文",
            "簡體中文",
            "中文",
        ],
        "zho_Hans",
    ),
    ("HIN", "हिन्दी", "hi", &["hi", "hindi", "हिन्दी"], "hin_Deva"),
    (
        "KAT",
        "ქართული",
        "ka",
        &["ka", "kat", "geo", "ka-ge", "georgian", "ქართული"],
        "kat_Geor",
    ),
];

/// The settings' language code from any spelling the game accepts (English when unknown).
pub fn language_code(s: &str) -> &'static str {
    let s = s.trim().to_lowercase();
    LANGUAGES
        .iter()
        .find(|(code, _, _, aliases, _)| {
            code.eq_ignore_ascii_case(&s) || aliases.iter().any(|a| *a == s)
        })
        .map(|l| l.0)
        .unwrap_or("ENG")
}

/// The interface tables' code of a language (`ru`, `ja` ...; empty for English).
pub fn language_iso(code: &str) -> &'static str {
    let c = language_code(code);
    LANGUAGES
        .iter()
        .find(|l| l.0 == c)
        .map(|l| if l.2 == "en" { "" } else { l.2 })
        .unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::{language_code, language_iso};

    #[test]
    fn serbian_latin_aliases_resolve_to_the_sr_locale() {
        for alias in [
            "SRP",
            "sr",
            "srp",
            "sr-Latn",
            "sr_latn",
            "serbian",
            "serbian latin",
            "srpski",
        ] {
            assert_eq!(language_code(alias), "SRP", "{alias}");
            assert_eq!(language_iso(alias), "sr", "{alias}");
        }
    }
}
