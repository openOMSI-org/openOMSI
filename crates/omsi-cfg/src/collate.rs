//! The order Omsi.exe sorts names in: Delphi's `AnsiCompareText`, which is Windows'
//! `CompareString` ignoring case with its word sort. Letters compare without their accents
//! and case (`Äpfel` beside `Apfel`, `ß` as `ss`), hyphens and apostrophes count for nothing,
//! and spaces and punctuation come before digits, digits before letters. Only where two names
//! are the same that way do the accents, then the case, then the hyphens decide.

use std::cmp::Ordering;

/// Windows' order of the spaces and punctuation among themselves (all before the digits).
const PUNCTUATION: &str = " \t!\"#$%&()*,./:;?@[\\]^_`{|}~¡¦¨¯´¸¿+<=>±«»×÷¢£¤¥§©¬®°µ¶·";

/// One character's place in the first comparison: (class, key). Class 0 is spaces and
/// punctuation, 1 digits, 2 letters; anything else sorts after them by its code point.
fn primary(c: char, out: &mut Vec<(u8, u32)>) {
    if c == '-' || c == '\'' || c == '\u{ad}' {
        return;
    }
    if let Some(i) = PUNCTUATION.chars().position(|p| p == c) {
        out.push((0, i as u32));
    } else if c.is_ascii_digit() {
        out.push((1, c as u32));
    } else if c.is_alphabetic() {
        for b in base_letters(c).chars() {
            out.push((2, b as u32));
        }
    } else if c.is_whitespace() {
        out.push((0, 0));
    } else {
        out.push((3, c as u32));
    }
}

/// A letter without its accent and case (`Ä` -> `a`, `ß` -> `ss`, `Œ` -> `oe`).
fn base_letters(c: char) -> String {
    let lower: String = c.to_lowercase().collect();
    let mut out = String::new();
    for l in lower.chars() {
        let base = match l {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => "a",
            'æ' => "ae",
            'ç' | 'ć' | 'č' => "c",
            'ď' | 'đ' => "d",
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => "e",
            'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' => "i",
            'ł' | 'ľ' | 'ĺ' => "l",
            'ñ' | 'ń' | 'ň' => "n",
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => "o",
            'œ' => "oe",
            'ŕ' | 'ř' => "r",
            'ś' | 'š' | 'ş' => "s",
            'ß' => "ss",
            'ť' | 'ţ' => "t",
            'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' | 'ų' => "u",
            'ý' | 'ÿ' => "y",
            'ź' | 'ż' | 'ž' => "z",
            _ => {
                out.push(l);
                continue;
            }
        };
        out.push_str(base);
    }
    out
}

fn keys(s: &str) -> Vec<(u8, u32)> {
    let mut out = Vec::with_capacity(s.len());
    s.chars().for_each(|c| primary(c, &mut out));
    out
}

/// Compare two names as Omsi.exe's lists order them (see the module).
pub fn windows_cmp(a: &str, b: &str) -> Ordering {
    keys(a)
        .cmp(&keys(b))
        // accents: the plain letter first
        .then_with(|| {
            let accents = |s: &str| -> Vec<bool> {
                s.chars().filter(|c| c.is_alphabetic()).map(|c| c.to_lowercase().collect::<String>() != base_letters(c)).collect()
            };
            accents(a).cmp(&accents(b))
        })
        // case: the small letter first, as Windows has it
        .then_with(|| {
            let upper = |s: &str| -> Vec<bool> { s.chars().filter(|c| c.is_alphabetic()).map(char::is_uppercase).collect() };
            upper(a).cmp(&upper(b))
        })
        // the hyphens and apostrophes left out above, then the text itself
        .then_with(|| a.chars().filter(|c| matches!(c, '-' | '\'')).count().cmp(&b.chars().filter(|c| matches!(c, '-' | '\'')).count()))
        .then_with(|| a.cmp(b))
}

/// How Windows orders the entries of a folder on NTFS - the order Omsi.exe's FindFirst /
/// FindNext loops meet files in. Unlike [`windows_cmp`] this is no word sort: the names are
/// compared code unit by code unit (UTF-16) after the volume's upper-casing, so punctuation
/// counts with its code point (a space and `-` before the digits, `_` after the capitals) and
/// a letter beyond ASCII after `Z`. A letter whose capital is two letters keeps its own unit.
pub fn cmp_as_listed(a: &str, b: &str) -> Ordering {
    fn units(s: &str) -> impl Iterator<Item = u16> + '_ {
        s.chars().flat_map(|c| {
            let mut upper = c.to_uppercase();
            let one = match (upper.next(), upper.next()) {
                (Some(u), None) => u,
                _ => c,
            };
            let mut buf = [0u16; 2];
            let n = one.encode_utf16(&mut buf).len();
            buf.into_iter().take(n)
        })
    }
    units(a).cmp(units(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(names: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = names.iter().map(|s| s.to_string()).collect();
        v.sort_by(|a, b| windows_cmp(a, b));
        v
    }

    /// The order OMSI 2's start point list shows synthetic names in (a Grundorf copy).
    #[test]
    fn names_sort_as_omsi_lists_them() {
        let got = sorted(&["beta", "Äpfel", "Apfelz", "Alpha", "Gamma", "Az", "a-c", "ab", "Nordspitze Bauernhof"]);
        assert_eq!(got, ["ab", "a-c", "Alpha", "Äpfel", "Apfelz", "Az", "beta", "Gamma", "Nordspitze Bauernhof"]);
    }

    /// Punctuation before digits before letters, digit by digit: Gerolstein's depot places.
    #[test]
    fn punctuation_digits_letters() {
        let got = sorted(&["(519) Halle", "(518, 519, 563) Hof", "(518) Bahnhof", "5 Markt", "Markt", " Kirche"]);
        assert_eq!(got, [" Kirche", "(518) Bahnhof", "(518, 519, 563) Hof", "(519) Halle", "5 Markt", "Markt"]);
    }

    #[test]
    fn accents_and_case_only_break_ties() {
        assert_eq!(windows_cmp("Apfel", "Äpfel"), Ordering::Less);
        assert_eq!(windows_cmp("apfel", "Apfel"), Ordering::Less);
        assert_eq!(windows_cmp("Straße", "Strasse"), Ordering::Greater);
        assert_eq!(windows_cmp("Strasse", "Straße"), Ordering::Less);
        assert_eq!(windows_cmp("Straße A", "Strasse B"), Ordering::Less);
        assert_eq!(windows_cmp("ab", "a-b"), Ordering::Less);
        assert_eq!(windows_cmp("same", "same"), Ordering::Equal);
    }

    #[test]
    fn folder_listings_compare_capitals_by_code_unit() {
        let mut v = vec!["Linie_2.oft", "Zug.oft", "LINIE 2.oft", "linie2.oft", "Ära.oft", "linie-2.oft"];
        v.sort_by(|a, b| cmp_as_listed(a, b));
        assert_eq!(v, ["LINIE 2.oft", "linie-2.oft", "linie2.oft", "Linie_2.oft", "Zug.oft", "Ära.oft"]);
        assert_eq!(cmp_as_listed("abc.OFT", "ABC.oft"), Ordering::Equal);
    }
}
