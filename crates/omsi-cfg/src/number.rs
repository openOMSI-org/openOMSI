//! Delphi-compatible lenient number parsing (`StrToFloat`/`StrToInt` with default 0).

fn clean(s: &str) -> &str {
    s.trim()
}

/// Parse a float the way OMSI does: decimal point, optional exponent, garbage → 0.
/// A trailing comment or unit after whitespace is ignored (`"5 (metres)"` → 5).
pub fn parse_f64(s: &str) -> f64 {
    let s = clean(s);
    if s.is_empty() {
        return 0.0;
    }
    let first = s.split_whitespace().next().unwrap_or("");
    // Delphi accepts a leading '+', and "1." / ".5".
    let mut t = first.replace(',', ".");
    if t.ends_with('.') {
        t.push('0');
    }
    if t.starts_with('.') || t.starts_with("-.") {
        t = t.replace('.', "0.");
    }
    match t.parse::<f64>() {
        // Rust reads "nan", "inf", "infinity" and 1e999 as numbers; a value that is not
        // finite is garbage like any other (it reached sorts and lane lookups as NaN)
        Ok(v) if !v.is_finite() => 0.0,
        Ok(v) => v,
        Err(_) => {
            // Fall back to the longest numeric prefix.
            let mut end = 0;
            for (i, c) in t.char_indices() {
                if c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E' {
                    end = i + c.len_utf8();
                } else {
                    break;
                }
            }
            t[..end]
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .unwrap_or(0.0)
        }
    }
}

pub fn parse_f32(s: &str) -> f32 {
    // (a double beyond f32's range would become infinite)
    let v = parse_f64(s) as f32;
    if v.is_finite() { v } else { 0.0 }
}

pub fn parse_i64(s: &str) -> i64 {
    let s = clean(s);
    if let Some(first) = s.split_whitespace().next() {
        if let Ok(v) = first.parse::<i64>() {
            return v;
        }
        // Delphi StrToInt fails on "1.0"; OMSI usually uses StrToFloat then truncates.
        return parse_f64(first).trunc() as i64;
    }
    0
}

pub fn parse_i32(s: &str) -> i32 {
    parse_i64(s).clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn not_finite_is_garbage() {
        for s in ["nan", "NaN", "inf", "-inf", "Infinity", "1e999", "-1e999"] {
            assert_eq!(parse_f64(s), 0.0, "{s}");
        }
        assert_eq!(parse_f32("1e39"), 0.0);
        assert_eq!(parse_f32("2.5"), 2.5);
    }

    #[test]
    fn floats() {
        assert_eq!(parse_f64("6.21874087223886E-7"), 6.21874087223886E-7);
        assert_eq!(parse_f64(" -13.4285734512165 "), -13.4285734512165);
        assert_eq!(parse_f64("junk"), 0.0);
        assert_eq!(parse_f64(""), 0.0);
        assert_eq!(parse_f64("1."), 1.0);
        assert_eq!(parse_i64("4192"), 4192);
        assert_eq!(parse_i64("3.0"), 3);
    }
}
