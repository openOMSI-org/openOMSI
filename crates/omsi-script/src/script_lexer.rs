#[derive(Debug, Clone, PartialEq)]
pub(super) enum Tok {
    Word(String),
    Str(String),
    Paren(String),
    Brace(String),
}

pub(super) fn has_unclosed_string(line: &str) -> bool {
    let mut in_string = false;
    for c in line.chars() {
        match c {
            '"' => in_string = !in_string,
            '\'' if !in_string => break,
            _ => {}
        }
    }
    in_string
}

pub(super) fn is_script_block_header(line: &str) -> bool {
    let Some(body) = line
        .trim()
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
    else {
        return false;
    };
    // (case-sensitive, as `compile_file` reads headers)
    let body = body.trim();
    matches!(body, "init" | "frame" | "frame_ai")
        || body.starts_with("macro:")
        || body.starts_with("trigger:")
}

/// Tokenize one logical script line. `'` starts a comment (outside a string literal).
pub(super) fn tokenize(line: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '\'' {
            break;
        }
        if c == '"' {
            let start = i + 1;
            let mut j = start;
            while j < chars.len() && chars[j] != '"' {
                j += 1;
            }
            out.push(Tok::Str(chars[start..j.min(chars.len())].iter().collect()));
            i = j + 1;
            continue;
        }
        if c == '(' {
            // The access ends at its matching bracket: mods name variables after fonts like
            // `Font_(8-1)x3`, and `(L.L.Font_(8-1)x3)` is one access, not `(L.L.Font_(8-1)`
            // followed by a stray `x3)`. Without a match the first `)` closes it.
            let start = i + 1;
            let mut depth = 1;
            let mut j = start;
            while j < chars.len() && !chars[j].is_whitespace() {
                match chars[j] {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            if depth != 0 {
                j = start;
                while j < chars.len() && chars[j] != ')' {
                    j += 1;
                }
            }
            out.push(Tok::Paren(
                chars[start..j.min(chars.len())]
                    .iter()
                    .collect::<String>()
                    .trim()
                    .to_string(),
            ));
            i = j + 1;
            continue;
        }
        if c == '{' {
            let start = i + 1;
            let mut j = start;
            while j < chars.len() && chars[j] != '}' {
                j += 1;
            }
            out.push(Tok::Brace(
                chars[start..j.min(chars.len())]
                    .iter()
                    .collect::<String>()
                    .trim()
                    .to_string(),
            ));
            i = j + 1;
            continue;
        }
        let start = i;
        let mut j = i;
        while j < chars.len()
            && !chars[j].is_whitespace()
            && chars[j] != '('
            && chars[j] != '{'
            && chars[j] != '"'
        {
            j += 1;
        }
        out.push(Tok::Word(chars[start..j].iter().collect()));
        i = j.max(start + 1);
    }
    out
}
