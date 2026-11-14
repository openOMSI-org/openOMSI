//! The language files (`locales/app.<language>.yml`) against their English reference, `locales/app.yml`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn locales() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../omsi-app/locales")
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    // (a key written twice is an error here: YAML would keep the last one silently)
    serde_saphyr::from_str(&text)
        .unwrap_or_else(|e| panic!("{} does not load: {e}", path.display()))
}

fn reference() -> BTreeMap<String, BTreeMap<String, String>> {
    let mut all: BTreeMap<String, serde_json::Value> = read(&locales().join("app.yml"));
    all.remove("_version");
    all.into_iter()
        .map(|(key, langs)| {
            let langs =
                serde_json::from_value(langs).unwrap_or_else(|e| panic!("app.yml: {key:?}: {e}"));
            (key, langs)
        })
        .collect()
}

fn languages() -> BTreeMap<String, BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for entry in std::fs::read_dir(locales()).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        let Some(lang) = name
            .strip_prefix("app.")
            .and_then(|n| n.strip_suffix(".yml"))
        else {
            continue;
        };
        // (a comment-only file loads as nothing)
        let table: Option<BTreeMap<String, String>> = read(&locales().join(&name));
        out.insert(lang.to_string(), table.unwrap_or_default());
    }
    assert!(
        !out.is_empty(),
        "no app.<language>.yml in {}",
        locales().display()
    );
    out
}

fn report(problems: &[String], what: &str) -> String {
    let shown: Vec<&str> = problems.iter().take(40).map(String::as_str).collect();
    let more = problems.len().saturating_sub(shown.len());
    format!(
        "{} {what}:\n  {}{}",
        problems.len(),
        shown.join("\n  "),
        if more > 0 {
            format!("\n  ... and {more} more")
        } else {
            String::new()
        }
    )
}

#[test]
fn app_yml_is_the_english_reference() {
    let wrong: Vec<String> = reference()
        .into_iter()
        .filter(|(key, langs)| langs.len() != 1 || langs.get("en") != Some(key))
        .map(|(key, langs)| format!("{key:?}: {langs:?}"))
        .collect();
    assert!(
        wrong.is_empty(),
        "{}",
        report(
            &wrong,
            "entries of app.yml are not `en: <the key>` alone (a translation goes into app.<language>.yml)"
        )
    );
}

#[test]
fn every_language_file_is_read_by_the_launcher() {
    let files: BTreeSet<String> = languages().into_keys().collect();
    let launcher: BTreeSet<String> = omsi_launcher_lib::i18n::LANGUAGES
        .iter()
        .map(|(lang, _)| lang.to_string())
        .collect();
    assert_eq!(
        files, launcher,
        "the files app.<language>.yml and `languages!` in src/i18n.rs"
    );
}

// The launcher reads the files line by line (`pairs`), the game as YAML: a line in another YAML
// style would reach only the game.
#[test]
fn the_launcher_reads_every_translation_the_game_reads() {
    let mut differ = Vec::new();
    for (lang, yaml) in languages() {
        let text = std::fs::read_to_string(locales().join(format!("app.{lang}.yml"))).unwrap();
        let launcher: BTreeMap<String, String> =
            omsi_launcher_lib::i18n::pairs(&text).into_iter().collect();
        let keys: BTreeSet<&String> = yaml.keys().chain(launcher.keys()).collect();
        for key in keys {
            if yaml.get(key) != launcher.get(key) {
                differ.push(format!(
                    "app.{lang}.yml: {key:?}: YAML {:?}, launcher {:?}",
                    yaml.get(key),
                    launcher.get(key)
                ));
            }
        }
    }
    assert!(
        differ.is_empty(),
        "{}",
        report(
            &differ,
            "translations are read differently by the launcher (write them as `\"English\": \"translation\"` on one line)"
        )
    );
}

#[test]
fn language_files_only_translate_texts_of_app_yml() {
    let english = reference();
    let extra: Vec<String> = languages()
        .into_iter()
        .flat_map(|(lang, table)| {
            table
                .into_keys()
                .filter(|key| !english.contains_key(key))
                .map(move |key| format!("app.{lang}.yml: {key:?}"))
        })
        .collect();
    assert!(
        extra.is_empty(),
        "{}",
        report(
            &extra,
            "translations of texts app.yml does not have (add the text to app.yml, or correct the key)"
        )
    );
}

#[test]
#[ignore = "some languages still lack texts that were missing before the split"]
fn every_text_is_translated_into_every_language() {
    let english = reference();
    let lacking: Vec<String> = languages()
        .into_iter()
        .map(|(lang, table)| {
            (
                lang,
                english
                    .keys()
                    .filter(|key| !table.contains_key(*key))
                    .count(),
            )
        })
        .filter(|(_, missing)| *missing > 0)
        .map(|(lang, missing)| format!("{lang}: {missing} of {} texts missing", english.len()))
        .collect();
    assert!(
        lacking.is_empty(),
        "{}",
        report(&lacking, "languages are not complete")
    );
}
