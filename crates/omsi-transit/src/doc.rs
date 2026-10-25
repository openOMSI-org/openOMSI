//! The text format of the transport files: `[block]` lines followed by `key value` lines.
//!
//! ```text
//! // a comment (also # at the start of a line)
//! [station]
//! id Sokol
//! name Sokol
//! modes metro tram
//! ```
//!
//! A block runs to the next `[block]` line. A key may repeat. Blank lines and comments are
//! skipped; a value is the rest of the line, trimmed.

use std::fmt;

/// A problem found in a file: where, how bad, what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub file: String,
    pub line: usize,
    pub error: bool,
    pub message: String,
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = if self.error { "error" } else { "warning" };
        if self.line > 0 {
            write!(f, "{}:{}: {kind}: {}", self.file, self.line, self.message)
        } else {
            write!(f, "{}: {kind}: {}", self.file, self.message)
        }
    }
}

/// Collects the issues of one file.
pub struct Issues {
    pub file: String,
    pub list: Vec<Issue>,
}

impl Issues {
    pub fn new(file: &str) -> Issues {
        Issues { file: file.to_string(), list: Vec::new() }
    }

    pub fn error(&mut self, line: usize, message: impl Into<String>) {
        self.list.push(Issue { file: self.file.clone(), line, error: true, message: message.into() });
    }

    pub fn warn(&mut self, line: usize, message: impl Into<String>) {
        self.list.push(Issue { file: self.file.clone(), line, error: false, message: message.into() });
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Entry {
    pub key: String,
    pub value: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Block {
    /// The block's keyword, lower-cased, without brackets.
    pub name: String,
    pub line: usize,
    pub entries: Vec<Entry>,
}

impl Block {
    /// The first value of a key (keys compare without regard to case).
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.iter().find(|e| e.key.eq_ignore_ascii_case(key)).map(|e| e.value.as_str())
    }

    pub fn line_of(&self, key: &str) -> usize {
        self.entries.iter().find(|e| e.key.eq_ignore_ascii_case(key)).map(|e| e.line).unwrap_or(self.line)
    }

    /// Every value of a (repeatable) key.
    pub fn all<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a Entry> + 'a {
        self.entries.iter().filter(move |e| e.key.eq_ignore_ascii_case(key))
    }

    pub fn str(&self, key: &str) -> String {
        self.get(key).unwrap_or("").to_string()
    }

    pub fn f32(&self, key: &str) -> Option<f32> {
        self.get(key).and_then(|v| num(v.split_whitespace().next()?))
    }

    pub fn f32_or(&self, key: &str, default: f32) -> f32 {
        self.f32(key).unwrap_or(default)
    }

    pub fn i32(&self, key: &str) -> Option<i32> {
        self.f32(key).map(|v| v as i32)
    }

    pub fn flag(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(|v| flag(v))
    }

    pub fn flag_or(&self, key: &str, default: bool) -> bool {
        self.flag(key).unwrap_or(default)
    }

    /// A list value: words separated by commas, semicolons or spaces where `by_comma` is false;
    /// by commas and semicolons only (names with spaces) where it is true.
    pub fn list(&self, key: &str, by_comma: bool) -> Vec<String> {
        let mut out = Vec::new();
        for e in self.all(key) {
            out.extend(split_list(&e.value, by_comma));
        }
        out
    }

    /// Three numbers `x y z`.
    pub fn vec3(&self, key: &str) -> Option<[f64; 3]> {
        let v = self.get(key)?;
        let n: Vec<f64> = v.split_whitespace().filter_map(|w| w.parse().ok()).collect();
        (n.len() >= 3).then(|| [n[0], n[1], n[2]])
    }

    /// A length in metres: `150`, `150 m`, `0.15 km`.
    pub fn metres(&self, key: &str) -> Option<f32> {
        parse_length(self.get(key)?)
    }

    /// A duration in seconds: `150`, `150 s`, `2.5 min`, `1 h`.
    pub fn seconds(&self, key: &str) -> Option<f32> {
        parse_duration(self.get(key)?)
    }

    /// A speed in m/s: `80 km/h` (also plain km/h numbers, `20 m/s`, `30 mph`).
    pub fn speed(&self, key: &str) -> Option<f32> {
        parse_speed(self.get(key)?)
    }
}

pub fn split_list(v: &str, by_comma: bool) -> Vec<String> {
    let sep = |c: char| c == ',' || c == ';' || (!by_comma && c.is_whitespace());
    v.split(sep).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

fn num(s: &str) -> Option<f32> {
    s.replace(',', ".").parse::<f32>().ok().filter(|v| v.is_finite())
}

pub fn flag(v: &str) -> Option<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "1" | "yes" | "true" | "on" => Some(true),
        "0" | "no" | "false" | "off" => Some(false),
        _ => None,
    }
}

/// Splits `150 m` into the number and its unit word (lower-cased, empty when none).
fn number_and_unit(v: &str) -> Option<(f32, String)> {
    let v = v.trim();
    let split = v.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ',' || c == '-' || c == '+')).unwrap_or(v.len());
    let n = num(&v[..split])?;
    Some((n, v[split..].trim().to_ascii_lowercase()))
}

pub fn parse_length(v: &str) -> Option<f32> {
    let (n, unit) = number_and_unit(v)?;
    match unit.as_str() {
        "" | "m" => Some(n),
        "km" => Some(n * 1000.0),
        "cm" => Some(n / 100.0),
        "mm" => Some(n / 1000.0),
        _ => None,
    }
}

pub fn parse_duration(v: &str) -> Option<f32> {
    let (n, unit) = number_and_unit(v)?;
    match unit.as_str() {
        "" | "s" | "sec" => Some(n),
        "min" | "m" => Some(n * 60.0),
        "h" => Some(n * 3600.0),
        _ => None,
    }
}

pub fn parse_speed(v: &str) -> Option<f32> {
    let (n, unit) = number_and_unit(v)?;
    match unit.as_str() {
        "" | "km/h" | "kmh" | "kph" => Some(n / 3.6),
        "m/s" => Some(n),
        "mph" => Some(n * 0.447_04),
        _ => None,
    }
}

/// A time of day `HH:MM` or `HH:MM:SS` in seconds; hours past 24 are allowed (a tour running
/// past midnight).
pub fn parse_clock(v: &str) -> Option<f32> {
    let parts: Vec<&str> = v.trim().split(':').collect();
    if !(2..=3).contains(&parts.len()) {
        return None;
    }
    let h: f32 = parts[0].trim().parse().ok()?;
    let m: f32 = parts[1].trim().parse().ok()?;
    let s: f32 = parts.get(2).map(|s| s.trim().parse().ok()).unwrap_or(Some(0.0))?;
    (h >= 0.0 && (0.0..60.0).contains(&m) && (0.0..60.0).contains(&s)).then(|| h * 3600.0 + m * 60.0 + s)
}

/// Reads a file's text into blocks. Lines before the first block are ignored with a warning.
pub fn parse(text: &str, issues: &mut Issues) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        if line.starts_with('[') {
            match line.find(']') {
                Some(end) => {
                    blocks.push(Block { name: line[1..end].trim().to_ascii_lowercase(), line: n, entries: Vec::new() });
                    let rest = line[end + 1..].trim();
                    if !rest.is_empty() && !rest.starts_with("//") && !rest.starts_with('#') {
                        issues.warn(n, format!("text after the block name is ignored: {rest}"));
                    }
                }
                None => issues.error(n, "a block name is missing its ]"),
            }
            continue;
        }
        let Some(block) = blocks.last_mut() else {
            issues.warn(n, "text before the first [block] is ignored");
            continue;
        };
        // (a trailing `// comment` is cut off)
        let body = match line.find(" //") {
            Some(p) => line[..p].trim_end(),
            None => line,
        };
        let (key, value) = match body.split_once(char::is_whitespace) {
            Some((k, v)) => (k.trim(), v.trim()),
            None => (body, ""),
        };
        block.entries.push(Entry { key: key.to_ascii_lowercase(), value: value.to_string(), line: n });
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_keys_and_comments() {
        let mut is = Issues::new("t.cfg");
        let b = parse("// head\n[Station]\nid A\nname  Main Square  // the hub\nmodes tram, metro\n\n[station]\nid B\n", &mut is);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].name, "station");
        assert_eq!(b[0].get("NAME"), Some("Main Square"));
        assert_eq!(b[0].list("modes", false), vec!["tram", "metro"]);
        assert!(is.list.is_empty());
    }

    #[test]
    fn units() {
        assert_eq!(parse_length("0.15 km"), Some(150.0));
        assert_eq!(parse_length("150"), Some(150.0));
        assert_eq!(parse_duration("2,5 min"), Some(150.0));
        assert_eq!(parse_duration("1 h"), Some(3600.0));
        assert!((parse_speed("36 km/h").unwrap() - 10.0).abs() < 1e-4);
        assert_eq!(parse_speed("20 m/s"), Some(20.0));
        assert_eq!(parse_clock("05:30"), Some(19800.0));
        assert_eq!(parse_clock("25:00"), Some(90000.0));
        assert_eq!(parse_clock("5"), None);
        assert_eq!(parse_length("far"), None);
    }

    #[test]
    fn stray_text_is_reported() {
        let mut is = Issues::new("t.cfg");
        let b = parse("x y\n[a\nk v\n[b] junk\n", &mut is);
        assert_eq!(is.list.len(), 4, "{:?}", is.list);
        assert_eq!(b.len(), 1);
    }
}
