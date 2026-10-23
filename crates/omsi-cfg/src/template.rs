use std::collections::HashMap;

/// A piece of a template key: literal text, or a `{}` / `{name}` / `{:.1}` placeholder
/// (its name; empty for a positional one).
#[derive(Debug, PartialEq)]
pub enum Piece {
    Text(String),
    Hole(String),
}

/// A key of the tables that is a `format!` string, and its pieces.
pub type Template = (String, Vec<Piece>);

/// The keys that are safe templates, most literal text first. Only keys that cannot half
/// translate an ordinary text count: a leading text of 2+ characters, no two placeholders
/// next to each other, and either a closing text or 2+ worded texts ("Weather: {name}"
/// would take "Weather: METAR request failed" for a name).
pub fn templates(keys: impl IntoIterator<Item = String>) -> Vec<Template> {
    let mut t: Vec<Template> = keys
        .into_iter()
        .filter(|k| k.contains('{') && !k.contains("%{") && !k.contains("{{") && !k.contains("}}"))
        .filter_map(|k| {
            let p = pieces(&k)?;
            let head = matches!(p.first(), Some(Piece::Text(h)) if h.chars().count() >= 2);
            let worded = p.iter().filter(|x| matches!(x, Piece::Text(s) if s.chars().any(char::is_alphanumeric))).count() >= 2;
            let closed = matches!(p.last(), Some(Piece::Text(_)));
            let apart = p.windows(2).all(|w| !matches!(w, [Piece::Hole(_), Piece::Hole(_)]));
            (head && (worded || closed) && apart).then_some((k, p))
        })
        .collect();
    t.sort_by_key(|(_, p)| std::cmp::Reverse(p.iter().map(|x| if let Piece::Text(s) = x { s.len() } else { 0 }).sum::<usize>()));
    t
}

fn pieces(s: &str) -> Option<Vec<Piece>> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(open) = rest.find('{') {
        let close = open + rest[open..].find('}')?;
        let inner = &rest[open + 1..close];
        if inner.contains('{') {
            return None;
        }
        if open > 0 {
            out.push(Piece::Text(rest[..open].to_string()));
        }
        out.push(Piece::Hole(inner.split(':').next().unwrap_or("").to_string()));
        rest = &rest[close + 1..];
    }
    if !rest.is_empty() {
        out.push(Piece::Text(rest.to_string()));
    }
    Some(out)
}

fn captures<'a>(pieces: &[Piece], text: &'a str, out: &mut Vec<&'a str>) -> bool {
    match pieces.split_first() {
        None => text.is_empty(),
        Some((Piece::Text(t), rest)) => text.strip_prefix(t.as_str()).is_some_and(|r| captures(rest, r, out)),
        Some((Piece::Hole(_), rest)) => match rest.first() {
            None => {
                out.push(text);
                true
            }
            Some(Piece::Text(t)) => text.match_indices(t.as_str()).any(|(i, _)| {
                out.push(&text[..i]);
                captures(rest, &text[i..], out) || {
                    out.pop();
                    false
                }
            }),
            Some(Piece::Hole(_)) => false,
        },
    }
}

fn fill(key: &[Piece], got: &[&str], translation: &str) -> Option<String> {
    let holes: Vec<(&str, &str)> = key.iter().filter_map(|p| if let Piece::Hole(n) = p { Some(n.as_str()) } else { None }).zip(got.iter().copied()).collect();
    let mut positional = holes.iter().filter(|(n, _)| n.is_empty()).map(|(_, v)| *v);
    let mut out = String::new();
    for p in pieces(translation)? {
        match p {
            Piece::Text(t) => out.push_str(&t),
            Piece::Hole(n) if n.is_empty() => out.push_str(positional.next()?),
            Piece::Hole(n) => out.push_str(holes.iter().find(|(m, _)| *m == n)?.1),
        }
    }
    Some(out)
}

/// `text` translated by the first of `templates` that fits it (None when none fits, or
/// the one that fits has no translation): its values go into the key's translation, and a
/// value that is itself a text of the tables ("Arrow keys only") is translated too.
pub fn templated(templates: &[Template], text: &str, lookup: impl Fn(&str) -> Option<String>) -> Option<String> {
    templated_among(templates.iter(), text, lookup)
}

fn templated_among<'a>(mut templates: impl Iterator<Item = &'a Template>, text: &str, lookup: impl Fn(&str) -> Option<String>) -> Option<String> {
    let mut got = Vec::new();
    let (key, p) = templates.find(|(_, p)| {
        got.clear();
        matches!(p.first(), Some(Piece::Text(h)) if text.starts_with(h.as_str())) && captures(p, text, &mut got)
    })?;
    let translation = lookup(key)?;
    let values: Vec<String> = got.iter().map(|v| lookup(v).unwrap_or_else(|| v.to_string())).collect();
    fill(p, &values.iter().map(String::as_str).collect::<Vec<_>>(), &translation)
}


/// The templates, most specific first, and where to look among them: by the first two
/// characters of their leading text (every template has 2+), so a text no template can fit
/// - a bus's name, a number, most of what `tr` is asked each frame - costs one hash lookup.
pub struct Index {
    list: Vec<Template>,
    by_head: HashMap<(char, char), Vec<usize>>,
}

fn head2(s: &str) -> Option<(char, char)> {
    let mut c = s.chars();
    Some((c.next()?, c.next()?))
}

impl Index {
    pub fn new(keys: impl IntoIterator<Item = String>) -> Self {
        let list = templates(keys);
        let mut by_head: HashMap<(char, char), Vec<usize>> = HashMap::new();
        for (i, (_, p)) in list.iter().enumerate() {
            if let Some(h) = p.first().and_then(|x| if let Piece::Text(t) = x { head2(t) } else { None }) {
                by_head.entry(h).or_default().push(i);
            }
        }
        Index { list, by_head }
    }

    pub fn templated(&self, text: &str, lookup: impl Fn(&str) -> Option<String>) -> Option<String> {
        let ids = self.by_head.get(&head2(text)?)?;
        templated_among(ids.iter().map(|&i| &self.list[i]), text, lookup)
    }
}
