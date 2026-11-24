//! A source scan for tests that hold what code declares against what it does: which fields
//! of a type (`App`) a function reaches, through the type's methods and the free functions
//! the value is handed to.
//!
//! Not a compiler: it reads the source with comments, strings and character literals blanked
//! and follows `self.f`, `self.m(..)`, `Self::m(..)`, `who.f` / `who.m(..)` in a function
//! whose parameter of the type is `who`, and `f(.., self, ..)` (the value handed on). Methods
//! are found by name, so a name defined twice (cfg variants) counts as both bodies. That is
//! how openOMSI's app code is written; code that hides a field behind a macro or a trait
//! method of another type is not followed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The `.rs` files under `dir`, in path order, with their text.
pub fn sources(dir: &Path) -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<(String, String)>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|e| e == "rs") {
                if let Ok(text) = std::fs::read_to_string(&p) {
                    out.push((p.display().to_string(), text));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, &mut out);
    out
}

/// The source with comments, strings and character literals blanked (same length, the line
/// breaks kept), so that braces and names in them do not count.
pub fn code_only(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = b.to_vec();
    let blank = |out: &mut Vec<u8>, from: usize, to: usize| {
        for c in &mut out[from..to.min(b.len())] {
            if *c != b'\n' {
                *c = b' ';
            }
        }
    };
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                let end = s[i..].find('\n').map_or(b.len(), |k| i + k);
                blank(&mut out, i, end);
                i = end;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let end = s[i + 2..].find("*/").map_or(b.len(), |k| i + 2 + k + 2);
                blank(&mut out, i, end);
                i = end;
            }
            b'r' if matches!(b.get(i + 1), Some(b'"' | b'#'))
                && (i == 0 || !is_ident_char(b[i - 1])) =>
            {
                let hashes = b[i + 1..].iter().take_while(|c| **c == b'#').count();
                if b.get(i + 1 + hashes) != Some(&b'"') {
                    i += 1;
                    continue;
                }
                let close = format!("\"{}", "#".repeat(hashes));
                let start = i + 2 + hashes;
                let end = s[start..]
                    .find(&close)
                    .map_or(b.len(), |k| start + k + close.len());
                blank(&mut out, i, end);
                i = end;
            }
            b'"' => {
                let mut j = i + 1;
                while j < b.len() && b[j] != b'"' {
                    j += if b[j] == b'\\' { 2 } else { 1 };
                }
                blank(&mut out, i, j + 1);
                i = j + 1;
            }
            b'\'' => {
                // a character literal ('x', '\n', '\u{..}', 'ä'), not a lifetime ('a)
                let end = if b.get(i + 1) == Some(&b'\\') {
                    s[i + 2..].find('\'').map(|k| i + 2 + k + 1)
                } else {
                    s[i + 1..]
                        .chars()
                        .next()
                        .map(|c| i + 1 + c.len_utf8())
                        .filter(|&e| b.get(e) == Some(&b'\''))
                        .map(|e| e + 1)
                };
                match end {
                    Some(e) => {
                        blank(&mut out, i, e);
                        i = e;
                    }
                    None => i += 1,
                }
            }
            _ => i += 1,
        }
    }
    String::from_utf8(out).unwrap_or_default()
}

/// The end of the block opening at `open` (a `{`), past its `}`.
pub fn block_end(s: &str, open: usize) -> usize {
    let mut depth = 0;
    for (k, c) in s[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return open + k + 1;
                }
            }
            _ => {}
        }
    }
    s.len()
}

/// The identifier starting at `at` (empty when none does).
pub fn ident_at(s: &str, at: usize) -> &str {
    let end = s[at..]
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .map_or(s.len(), |k| at + k);
    &s[at..end]
}

fn is_ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// The fields of `struct <ty>` in a source (`pub(crate) name: ..` lines).
pub fn struct_fields(source: &str, ty: &str) -> Vec<String> {
    let s = code_only(source);
    let Some(at) = s.find(&format!("struct {ty} {{")) else {
        return Vec::new();
    };
    let open = at + s[at..].find('{').unwrap_or(0);
    let body = &s[open + 1..block_end(&s, open) - 1];
    body.lines()
        .map(str::trim)
        .map(|l| {
            l.strip_prefix("pub(crate) ")
                .or_else(|| l.strip_prefix("pub "))
                .unwrap_or(l)
        })
        .filter(|l| l.contains(':') && !l.starts_with('#'))
        .map(|l| ident_at(l, 0).to_string())
        .filter(|f| !f.is_empty())
        .collect()
}

/// The arms of `match <scrutinee> {` in `fn <name>` of a source, by their pattern (`Variant`),
/// each with its lines; an arm starts at the match's own indentation.
pub fn match_arms(source: &str, fn_name: &str, scrutinee: &str) -> BTreeMap<String, String> {
    let s = code_only(source);
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    let Some(start) = s.find(&format!("fn {fn_name}")) else {
        return out;
    };
    let Some(m) = s[start..].find(&format!("match {scrutinee} {{")) else {
        return out;
    };
    let open = start + m + "match ".len() + scrutinee.len() + 1;
    let arms = &s[open + 1..block_end(&s, open) - 1];
    let indent = arms
        .lines()
        .find(|l| !l.trim().is_empty())
        .map_or(0, |l| l.len() - l.trim_start().len());
    let mut current = String::new();
    for line in arms.lines() {
        let t = line.trim_start();
        if !t.is_empty() && line.len() - t.len() == indent {
            if let Some(k) = t.find("=>") {
                current = t[..k].trim().to_string();
            }
        }
        if !current.is_empty() {
            let arm = out.entry(current.clone()).or_default();
            arm.push_str(line);
            arm.push('\n');
        }
    }
    out
}

/// The contexts `stage_ctx!` declares in a source: name -> the parts it reads and writes.
pub fn contexts(source: &str) -> BTreeMap<String, Vec<String>> {
    let s = code_only(source);
    let mut out = BTreeMap::new();
    let mut from = 0;
    while let Some(k) = s[from..].find("stage_ctx!(") {
        let at = from + k + "stage_ctx!(".len();
        from = at;
        let Some(open) = s[at..].find('{').map(|o| at + o) else {
            break;
        };
        let name = s[at..open]
            .split_whitespace()
            .last()
            .unwrap_or("")
            .to_string();
        let body = &s[open + 1..block_end(&s, open) - 1];
        let parts = body
            .split(|c| c == '[' || c == ']' || c == ',')
            .map(str::trim)
            .filter(|w| {
                !w.is_empty()
                    && w.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                    && *w != "reads"
                    && *w != "writes"
            })
            .map(str::to_string)
            .collect();
        if !name.is_empty() {
            out.insert(name, parts);
        }
    }
    out
}

/// The functions that can reach a value of one type: the type's own methods (by `self`), and
/// the free functions (and other types' methods) with a parameter of it (by that name).
#[derive(Default)]
pub struct Code {
    /// name -> (what the body calls the value, the body)
    fns: BTreeMap<String, Vec<(String, String)>>,
    methods: BTreeSet<String>,
}

impl Code {
    /// The functions of the sources that reach a `ty` (`App`).
    pub fn read(sources: &[(String, String)], ty: &str) -> Code {
        let mut code = Code::default();
        let heads = [
            format!("impl {ty} {{"),
            format!("impl crate::{ty} {{"),
            format!("impl super::{ty} {{"),
        ];
        for (_, text) in sources {
            let s = code_only(text);
            for head in &heads {
                let mut from = 0;
                while let Some(k) = s[from..].find(head.as_str()) {
                    let open = from + k + head.len() - 1;
                    let end = block_end(&s, open);
                    code.add_fns(&s[open + 1..end - 1], |_| Some("self".into()), true);
                    from = end;
                }
            }
            code.add_fns(&s, |sig| param_of(sig, ty), false);
        }
        code
    }

    fn add_fns(&mut self, s: &str, who: impl Fn(&str) -> Option<String>, methods: bool) {
        let mut f = 0;
        while let Some(k) = s[f..].find("fn ") {
            let at = f + k;
            f = at + 3;
            if at > 0 && is_ident_char(s.as_bytes()[at - 1]) {
                continue;
            }
            let name = ident_at(s, at + 3).to_string();
            let Some(open) = s[at..].find('{').map(|o| at + o) else {
                break;
            };
            // (`;` before the body's brace: a declaration)
            if name.is_empty() || s[at..open].contains(';') {
                continue;
            }
            let Some(who) = who(&s[at..open]) else {
                continue;
            };
            let end = block_end(s, open);
            if methods {
                self.methods.insert(name.clone());
                // (a method's body is not scanned again for nested fns of its own)
                f = end;
            }
            self.fns
                .entry(name)
                .or_default()
                .push((who, s[open..end].to_string()));
        }
    }

    /// Whether a function of this name reaches the value.
    pub fn has(&self, name: &str) -> bool {
        self.fns.contains_key(name)
    }

    /// The fields the functions `entries` reach, directly and through what they call.
    pub fn reach(&self, entries: &BTreeSet<String>) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut todo: Vec<String> = entries.iter().cloned().collect();
        let mut fields = BTreeSet::new();
        while let Some(n) = todo.pop() {
            if !seen.insert(n.clone()) {
                continue;
            }
            for (who, body) in self.fns.get(&n).into_iter().flatten() {
                let (f, c) = self.scan(who, body);
                fields.extend(f);
                todo.extend(c);
            }
        }
        fields
    }

    /// In a body where the value is called `who`: the fields it reads or writes (a field of
    /// a field `sub` of the value as `sub.field`, for the `nested` ones) and the functions it
    /// is handed to or called on.
    pub fn scan(&self, who: &str, body: &str) -> (BTreeSet<String>, BTreeSet<String>) {
        let mut fields = BTreeSet::new();
        let mut calls = BTreeSet::new();
        let b = body.as_bytes();
        let pat = format!("{who}.");
        let mut from = 0;
        while let Some(k) = body[from..].find(&pat) {
            let at = from + k;
            from = at + pat.len();
            if at > 0 && (is_ident_char(b[at - 1]) || b[at - 1] == b'.') {
                continue;
            }
            let name = ident_at(body, from);
            if name.is_empty() {
                continue;
            }
            let after = from + name.len();
            if b.get(after) == Some(&b'(') || body[after..].starts_with("::<") {
                if self.has(name) {
                    calls.insert(name.to_string());
                }
                continue;
            }
            // `who.session.traffic`: the session's own field
            if b.get(after) == Some(&b'.') {
                let sub = ident_at(body, after + 1);
                if !sub.is_empty() && b.get(after + 1 + sub.len()) != Some(&b'(') {
                    fields.insert(format!("{name}.{sub}"));
                }
            }
            fields.insert(name.to_string());
        }
        // a context taken from the value (`XCx::of(&mut self.parts())`): `XCx::of`, for the
        // caller to look up what that context holds
        let mut from = 0;
        while let Some(k) = body[from..].find("::of(") {
            let at = from + k;
            from = at + 5;
            let head = &body[..at];
            let start = head
                .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .map_or(0, |k| k + 1);
            if start < at
                && body[from..]
                    .chars()
                    .take(40)
                    .collect::<String>()
                    .contains(who)
            {
                fields.insert(format!("{}::of", &head[start..]));
            }
        }
        if who == "self" {
            let mut from = 0;
            while let Some(k) = body[from..].find("Self::") {
                let at = from + k + 6;
                from = at;
                let name = ident_at(body, at);
                if self.methods.contains(name) {
                    calls.insert(name.to_string());
                }
            }
        }
        // the value handed on: `f(self`, `f(a, self)`, `x.f(&mut *self, ..)`
        let mut from = 0;
        while let Some(k) = body[from..].find(who) {
            let at = from + k;
            from = at + who.len();
            if (at > 0 && is_ident_char(b[at - 1]))
                || b.get(from).is_some_and(|c| is_ident_char(*c) || *c == b'.')
            {
                continue;
            }
            // (`&mut *self`, `&self`: what is before the borrow)
            let before = body[..at].trim_end_matches(['&', '*', ' ', '\n']);
            let before = before
                .strip_suffix("mut")
                .unwrap_or(before)
                .trim_end_matches(['&', '*', ' ', '\n']);
            if !(before.ends_with('(') || before.ends_with(',')) {
                continue;
            }
            // the call this argument is in: back to its unmatched `(`
            let mut depth = 0;
            let mut open = None;
            for (i, c) in body[..at].char_indices().rev() {
                match c {
                    ')' | ']' | '}' => depth += 1,
                    '(' | '[' | '{' if depth > 0 => depth -= 1,
                    '(' => {
                        open = Some(i);
                        break;
                    }
                    '[' | '{' => break,
                    _ => {}
                }
            }
            let Some(open) = open else { continue };
            let head = body[..open].trim_end();
            let start = head
                .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .map_or(0, |k| k + 1);
            if self.has(&head[start..]) {
                calls.insert(head[start..].to_string());
            }
        }
        (fields, calls)
    }
}

/// The name of a signature's parameter of type `&ty` / `&mut ty` (`app: &mut App`).
fn param_of(sig: &str, ty: &str) -> Option<String> {
    for prefix in ["&mut ", "&"] {
        for path in ["", "crate::", "super::"] {
            let pat = format!("{prefix}{path}{ty}");
            let mut from = 0;
            while let Some(k) = sig[from..].find(&pat) {
                let at = from + k;
                from = at + pat.len();
                if sig.as_bytes().get(from).is_some_and(|c| is_ident_char(*c)) {
                    continue;
                }
                let Some(before) = sig[..at].trim_end().strip_suffix(':') else {
                    continue;
                };
                let before = before.trim_end();
                let start = before
                    .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .map_or(0, |k| k + 1);
                let name = &before[start..];
                if !name.is_empty() && name != "self" {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"
struct App {
    pub(crate) a: u32,
    /// the session
    pub(crate) b: Session,
    c: u8,
}
impl App {
    fn one(&mut self) { self.a += 1; self.two(); helper(self, 3); let s = "self.c {"; }
    fn two(&self) -> u8 { Self::three(self) }
    fn three(&self) -> u8 { self.b.x }
}
fn helper(app: &mut App, k: u8) { app.c = k; // app.nothing
}
fn other(x: &mut Other) { x.c = 1; }
"#;

    #[test]
    fn follows_methods_free_functions_and_the_value_handed_on() {
        let code = Code::read(&[("a.rs".into(), SRC.into())], "App");
        let reach = code.reach(&["one".to_string()].into_iter().collect());
        assert_eq!(
            reach.into_iter().collect::<Vec<_>>(),
            ["a", "b", "b.x", "c"]
        );
        assert!(!code.has("other"));
        assert_eq!(struct_fields(SRC, "App"), ["a", "b", "c"]);
    }

    #[test]
    fn strings_comments_and_characters_do_not_count() {
        let s = code_only("let a = '{'; let b = \"}\"; // {\nlet c: &'a str = r#\"{\"#; /* } */ x");
        assert!(!s.contains('{') && !s.contains('}'));
        assert!(s.contains("&'a str") && s.ends_with('x'));
    }

    #[test]
    fn contexts_and_their_use() {
        let src = "stage_ctx!(\n    /// doc\n    ACx { reads: [world, clock], writes: [player] }\n);\nimpl App { fn f(&mut self) { g(ACx::of(&mut self.parts())); } }\n";
        assert_eq!(contexts(src)["ACx"], ["world", "clock", "player"]);
        let code = Code::read(&[("a.rs".into(), src.into())], "App");
        assert!(
            code.reach(&["f".to_string()].into_iter().collect())
                .contains("ACx::of")
        );
    }

    #[test]
    fn arms_by_their_pattern() {
        let src = "fn run(s: S) {\n    match s {\n        A => app.one(),\n        B => {\n            app.two();\n        }\n    }\n}\n";
        let arms = match_arms(src, "run", "s");
        assert_eq!(arms.keys().collect::<Vec<_>>(), ["A", "B"]);
        assert!(arms["B"].contains("app.two()"));
    }
}
