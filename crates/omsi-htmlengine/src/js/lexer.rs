//! Source text to tokens.

use super::*;

pub(crate) fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let c: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    const PUNCT3: &[&str] = &["===", "!=="];
    const PUNCT2: &[&str] = &[
        "==", "!=", "<=", ">=", "&&", "||", "+=", "-=", "*=", "/=", "++", "--", "=>",
    ];
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch == '/' && c.get(i + 1) == Some(&'/') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
        } else if ch == '/' && c.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < c.len() && !(c[i] == '*' && c[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else if ch.is_ascii_digit()
            || (ch == '.' && c.get(i + 1).map_or(false, |d| d.is_ascii_digit()))
        {
            let s = i;
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                i += 1;
            }
            if i < c.len() && (c[i] == 'e' || c[i] == 'E') {
                i += 1;
                if i < c.len() && (c[i] == '+' || c[i] == '-') {
                    i += 1;
                }
                while i < c.len() && c[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let t: String = c[s..i].iter().collect();
            out.push(Tok::Num(t.parse().map_err(|_| format!("bad number {t}"))?));
        } else if ch == '"' || ch == '\'' || ch == '`' {
            let q = ch;
            i += 1;
            let mut s = String::new();
            while i < c.len() && c[i] != q {
                if c[i] == '\\' && i + 1 < c.len() {
                    i += 1;
                    s.push(match c[i] {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        o => o,
                    });
                } else {
                    s.push(c[i]);
                }
                i += 1;
            }
            i += 1;
            out.push(Tok::Str(s));
        } else if ch.is_alphabetic() || ch == '_' || ch == '$' {
            let s = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_' || c[i] == '$') {
                i += 1;
            }
            out.push(Tok::Id(c[s..i].iter().collect()));
        } else {
            let rest: String = c[i..(i + 3).min(c.len())].iter().collect();
            if let Some(p) = PUNCT3.iter().find(|p| rest.starts_with(**p)) {
                out.push(Tok::P((*p).to_string()));
                i += 3;
            } else if let Some(p) = PUNCT2.iter().find(|p| rest.starts_with(**p)) {
                out.push(Tok::P((*p).to_string()));
                i += 2;
            } else if "{}()[];,.:?+-*/%<>=!".contains(ch) {
                out.push(Tok::P(ch.to_string()));
                i += 1;
            } else {
                return Err(format!("unexpected character {ch:?}"));
            }
        }
    }
    out.push(Tok::Eof);
    Ok(out)
}

pub(crate) const KEYWORDS: &[&str] = &[
    "function", "return", "if", "else", "var", "let", "const", "for", "while", "typeof", "in",
    "break", "continue",
];
