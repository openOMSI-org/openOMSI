//! The interface's language. Every text the painter draws or measures goes through [`tr`]:
//! the English text is the key, and the application hands over a lookup (its rust-i18n
//! tables) and the language to look it up in. English, or a text the tables do not have
//! (a bus's name, a number), is drawn as it is.

use std::borrow::Cow;
use std::sync::RwLock;

pub use omsi_cfg::template::{templated, templates, Piece, Template};
use omsi_cfg::template::Index;

/// A lookup: (language, English text) → the text in that language.
pub type Lookup = fn(&str, &str) -> Option<String>;

static STATE: RwLock<(Option<Lookup>, String)> = RwLock::new((None, String::new()));

/// What is asked when the tables have no translation (the application's machine
/// translation: a text it has translated already, None while it is still at it).
static FALLBACK: RwLock<Option<Lookup>> = RwLock::new(None);

/// The lookup for texts the tables do not have.
pub fn set_fallback(f: Option<Lookup>) {
    if let Ok(mut s) = FALLBACK.write() {
        *s = f;
    }
}

/// The lookup to translate with.
pub fn set_lookup(f: Lookup) {
    if let Ok(mut s) = STATE.write() {
        s.0 = Some(f);
    }
}

/// The language to show (`ru`, `de`, `fr`; empty or `en` for English).
pub fn set_language(code: &str) {
    if let Ok(mut s) = STATE.write() {
        s.1 = if code.eq_ignore_ascii_case("en") { String::new() } else { code.to_ascii_lowercase() };
    }
}

/// The language shown now (empty for English).
pub fn language() -> String {
    STATE.read().map(|s| s.1.clone()).unwrap_or_default()
}

static TEMPLATES: RwLock<Option<Index>> = RwLock::new(None);

/// The keys of the tables that are format strings: a text drawn with its numbers or names
/// filled in ("Mirror panel added (3 in all)") is translated by the key it was made from.
pub fn set_templates(keys: impl IntoIterator<Item = String>) {
    if let Ok(mut s) = TEMPLATES.write() {
        *s = Some(Index::new(keys));
    }
}

/// `text` by the templates of [`set_templates`], among those whose leading text starts as it does.
fn by_template(text: &str, lookup: impl Fn(&str) -> Option<String>) -> Option<String> {
    TEMPLATES.read().ok()?.as_ref()?.templated(text, lookup)
}

/// `text` in the interface's language.
pub fn tr(text: &str) -> Cow<'_, str> {
    let Ok(s) = STATE.read() else { return Cow::Borrowed(text) };
    match (&s.0, s.1.is_empty()) {
        (Some(f), false) if !text.is_empty() => match f(&s.1, text).or_else(|| by_template(text, |k| f(&s.1, k))) {
            Some(t) => Cow::Owned(t),
            None => match FALLBACK.read().ok().and_then(|g| *g).and_then(|g| g(&s.1, text)) {
                Some(t) => Cow::Owned(t),
                None => Cow::Borrowed(text),
            },
        },
        _ => Cow::Borrowed(text),
    }
}

static SAID: RwLock<Vec<(String, String)>> = RwLock::new(Vec::new());

pub fn said(shown: String, english: String) -> String {
    if shown != english {
        if let Ok(mut said) = SAID.write() {
            said.retain(|(s, _)| *s != shown);
            if said.len() >= 32 {
                said.remove(0);
            }
            said.push((shown.clone(), english));
        }
    }
    shown
}

fn fill_in(key: &str, holes: &[(&str, &dyn std::fmt::Display)], value: impl Fn(&str) -> String) -> String {
    let english = holes.iter().fold(key.to_string(), |s, (h, v)| s.replace(h, &v.to_string()));
    let shown = holes.iter().fold(tr(key).into_owned(), |s, (h, v)| s.replace(h, &value(&v.to_string())));
    said(shown, english)
}

pub fn tr_fill(key: &str, holes: &[(&str, &dyn std::fmt::Display)]) -> String {
    fill_in(key, holes, str::to_string)
}

pub fn tr_fill_words(key: &str, holes: &[(&str, &dyn std::fmt::Display)]) -> String {
    fill_in(key, holes, |v| tr(v).into_owned())
}

pub fn english(text: &str) -> Cow<'_, str> {
    match SAID.read().ok().and_then(|s| s.iter().find(|(t, _)| t == text).map(|(_, e)| e.clone())) {
        Some(e) => Cow::Owned(e),
        None => Cow::Borrowed(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fr(key: &str) -> Option<String> {
        match key {
            "Mirror panel added ({} in all)" => Some("Panneau ajouté ({} au total)".into()),
            "Line {line}, tour {}" => Some("Ligne {line}, service {}".into()),
            "Line {name} is there already" => Some("La ligne {name} existe déjà".into()),
            "Panel shows mirror {} of {}" => Some("Le panneau montre le rétroviseur {} sur {}".into()),
            "Object {id} {name}: moved {:+.2} m" => Some("Objet {name} ({id}) : déplacé de {:+.2} m".into()),
            "Air pressure is low ({tank}spring brake {:.1} bar)" => Some("Pression d'air basse ({tank}frein à ressort {:.1} bar)".into()),
            "Line {} {} 3" => Some("Ligne {} {} 3".into()),
            "Driving keys: {name} (Settings)" => Some("Touches de conduite : {name} (Paramètres)".into()),
            "Arrow keys only" => Some("Flèches seulement".into()),
            _ => None,
        }
    }

    fn all() -> Vec<Template> {
        let keys = ["Mirror panel added ({} in all)", "Line {name} is there already", "Line {line}, tour {}", "Line {line} · tour {t}", "Line {} {} 3", "Panel shows mirror {} of {}", "Object {id} {name}: moved {:+.2} m", "Air pressure is low ({tank}spring brake {:.1} bar)", "Driving keys: {name} (Settings)"];
        templates(keys.iter().map(|k| k.to_string()))
    }

    #[test]
    fn a_formatted_text_is_translated_by_its_template() {
        let t = all();
        assert_eq!(templated(&t, "Mirror panel added (3 in all)", fr).as_deref(), Some("Panneau ajouté (3 au total)"));
        assert_eq!(templated(&t, "Panel shows mirror 2 of 5", fr).as_deref(), Some("Le panneau montre le rétroviseur 2 sur 5"));
        assert_eq!(templated(&t, "Object 12 Haus.sco: moved +1.50 m", fr).as_deref(), Some("Objet Haus.sco (12) : déplacé de +1.50 m"));
        assert_eq!(templated(&t, "Air pressure is low (spring brake 4.2 bar)", fr).as_deref(), Some("Pression d'air basse (frein à ressort 4.2 bar)"));
        assert_eq!(templated(&t, "Line M41, tour 3", fr).as_deref(), Some("Ligne M41, service 3"));
        assert_eq!(templated(&t, "Line M41 is there already", fr).as_deref(), Some("La ligne M41 existe déjà"));
    }

    #[test]
    fn the_index_by_leading_text_finds_what_the_whole_list_finds() {
        let keys = ["Mirror panel added ({} in all)", "Line {name} is there already", "Line {line}, tour {}", "Line {line} · tour {t}", "Line {} {} 3", "Panel shows mirror {} of {}", "Object {id} {name}: moved {:+.2} m", "Air pressure is low ({tank}spring brake {:.1} bar)", "Driving keys: {name} (Settings)"];
        set_templates(keys.iter().map(|k| k.to_string()));
        let t = all();
        for text in ["Mirror panel added (3 in all)", "Line M41, tour 3", "Line 5 · tour 3", "Line 5 tour 3", "Object 12 Haus.sco: moved +1.50 m", "Driving keys: Arrow keys only (Settings)", "Li", "L", "", "Grundorf", "Panneau ajouté (3 au total)"] {
            assert_eq!(by_template(text, fr), templated(&t, text, fr), "{text}");
        }
    }

    #[test]
    fn a_text_no_template_fits_is_left_alone() {
        let t = all();
        assert_eq!(templated(&t, "Mirror panel added", fr), None);
        assert_eq!(templated(&t, "Mirror panel added (3 in all) now", fr), None);
        assert_eq!(templated(&t, "Line and tour", fr), None);
        assert_eq!(templated(&t, "Panneau ajouté (3 au total)", fr), None);
    }

    #[test]
    fn the_first_template_that_fits_answers_even_without_a_translation() {
        let t = all();
        assert_eq!(templated(&t, "Line 5 · tour 3", fr), None);
        assert_eq!(templated(&t, "Line 5 tour 3", fr).as_deref(), Some("Ligne 5 tour 3"));
    }

    #[test]
    fn a_value_that_is_a_text_of_the_tables_is_translated_too() {
        let t = all();
        assert_eq!(templated(&t, "Driving keys: Arrow keys only (Settings)", fr).as_deref(), Some("Touches de conduite : Flèches seulement (Paramètres)"));
        assert_eq!(templated(&t, "Driving keys: Q W E (Settings)", fr).as_deref(), Some("Touches de conduite : Q W E (Paramètres)"));
    }

    #[test]
    fn only_worded_templates_with_apart_placeholders_count() {
        let keys = ["+{} more", "{} more", "Plain text", "Parts missing: needs %{packs}", "Braces {{}} here {} too", "Weather: {name}", "Clear {}", "Hat {} {}", "Radio {}: {}", "Clock: {:02}:{:02}", "Line {line}{} · {} more", "Joined {name}{how} - choose"];
        let t: Vec<String> = templates(keys.iter().map(|k| k.to_string())).into_iter().map(|(k, _)| k).collect();
        assert!(t.is_empty(), "{t:?}");
        let t: Vec<String> = templates(["Head tracking with opentrack (UDP port {})", "Panel shows mirror {} of {}"].map(String::from)).into_iter().map(|(k, _)| k).collect();
        assert_eq!(t.len(), 2, "{t:?}");
        let t = templates(["Gear {} (H-pattern)".to_string()]);
        assert_eq!(t[0].1, vec![Piece::Text("Gear ".into()), Piece::Hole(String::new()), Piece::Text(" (H-pattern)".into())]);
    }

    #[test]
    fn a_shown_text_remembers_its_english() {
        let shown = said("Gang 3".into(), "Gear 3".into());
        assert_eq!(english(&shown), "Gear 3");
        assert_eq!(english("Gear 4"), "Gear 4");
    }
}
