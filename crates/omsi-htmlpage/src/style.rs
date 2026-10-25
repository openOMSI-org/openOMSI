//! Computed style: lengths, colours and the property table a page can set.

use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Len {
    Px(f32),
    Pct(f32),
    /// `calc(10px + 50%)`: pixels plus a percentage.
    Mix(f32, f32),
}

impl Len {
    pub(crate) fn px(self, base: f32) -> f32 {
        match self {
            Len::Px(v) => v,
            Len::Pct(p) => base * p / 100.0,
            Len::Mix(v, p) => v + base * p / 100.0,
        }
    }
}

/// A stop of a `linear-gradient`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GStop {
    pub(crate) pos: Option<Len>,
    pub(crate) color: [u8; 4],
}

/// `background: linear-gradient(...)`: the angle (degrees, CSS convention: 0 up, 90 right) and
/// the stops.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Gradient {
    pub(crate) angle: f32,
    pub(crate) stops: Vec<GStop>,
}

/// `box-shadow`: offsets, blur, spread and colour (the first shadow only; `inset` is ignored).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Shadow {
    pub(crate) dx: f32,
    pub(crate) dy: f32,
    pub(crate) blur: f32,
    pub(crate) spread: f32,
    pub(crate) color: [u8; 4],
}

/// `background-size`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum BgSize {
    Auto,
    Cover,
    Contain,
    /// Width and height; `None` is `auto` (keeps the picture's proportions).
    Dims(Option<Len>, Option<Len>),
}

#[derive(Debug, Clone)]
pub(crate) struct Style {
    pub(crate) color: [u8; 4],
    pub(crate) bg: [u8; 4],
    pub(crate) font_px: f32,
    pub(crate) bold: bool,
    pub(crate) align: u8,
    pub(crate) line_h: f32,
    pub(crate) hidden: bool,
    pub(crate) margin: [f32; 4],
    pub(crate) margin_auto: bool,
    pub(crate) padding: [f32; 4],
    pub(crate) width: Option<Len>,
    pub(crate) height: Option<Len>,
    pub(crate) none: bool,
    pub(crate) inline: bool,
    /// `display:inline-block`: a box that sits in a row with its neighbours.
    pub(crate) inline_block: bool,
    pub(crate) radius: f32,
    /// The radius was given in percent (stored as a share of 10000 px): `50%` rounds to a circle.
    pub(crate) radius_pct: bool,
    /// `background-image: url(...)`: the path as written.
    pub(crate) bg_img: Option<Arc<str>>,
    pub(crate) bg_size: BgSize,
    /// `background-repeat`: (horizontally, vertically).
    pub(crate) bg_repeat: (bool, bool),
    /// `background-position`: (x, y); a percentage places the picture's own point at the
    /// same point of the box.
    pub(crate) bg_pos: [Len; 2],
    /// Height / width of an `<img>` whose width is a percentage (0: not used).
    pub(crate) aspect: f32,
    /// The element this style was computed for (what a text run belongs to when it is hit).
    pub(crate) node: usize,
    /// Border widths and colours (top, right, bottom, left); a side whose style is `none` has
    /// width 0.
    pub(crate) bw: [f32; 4],
    pub(crate) bcolor: [[u8; 4]; 4],
    /// `box-sizing: border-box`: `width` and `height` count padding and border.
    pub(crate) border_box: bool,
    /// 0 static, 1 relative, 2 absolute, 3 fixed.
    pub(crate) position: u8,
    /// `top`, `right`, `bottom`, `left`.
    pub(crate) inset: [Option<Len>; 4],
    pub(crate) z: i32,
    pub(crate) overflow_hidden: bool,
    pub(crate) opacity: f32,
    pub(crate) min_w: Option<Len>,
    pub(crate) max_w: Option<Len>,
    pub(crate) min_h: Option<Len>,
    pub(crate) max_h: Option<Len>,
    /// Which sides of the margin are `auto` (top, right, bottom, left).
    pub(crate) m_auto: [bool; 4],
    pub(crate) flex: bool,
    pub(crate) inline_flex: bool,
    /// `display: grid`: laid out as a column of its children (no grid tracks).
    pub(crate) grid: bool,
    /// 0 row, 1 row-reverse, 2 column, 3 column-reverse.
    pub(crate) dir: u8,
    /// 0 nowrap, 1 wrap, 2 wrap-reverse.
    pub(crate) wrap: u8,
    /// 0 start, 1 end, 2 center, 3 space-between, 4 space-around, 5 space-evenly.
    pub(crate) justify: u8,
    /// 0 stretch, 1 start, 2 end, 3 center, 4 baseline.
    pub(crate) align_items: u8,
    pub(crate) align_self: Option<u8>,
    /// `row-gap`, `column-gap`.
    pub(crate) gap: [f32; 2],
    pub(crate) grow: f32,
    pub(crate) shrink: f32,
    pub(crate) basis: Option<Len>,
    pub(crate) order: i32,
    /// `white-space: nowrap` / `pre`.
    pub(crate) nowrap: bool,
    /// 0 none, 1 uppercase, 2 lowercase, 3 capitalize.
    pub(crate) transform: u8,
    pub(crate) letter_px: f32,
    pub(crate) underline: bool,
    pub(crate) strike: bool,
    pub(crate) ellipsis: bool,
    pub(crate) gradient: Option<Gradient>,
    pub(crate) shadow: Option<Shadow>,
    /// `border-radius` per corner is not kept; `outline` neither.
    /// The custom properties (`--name`) in force, inherited.
    pub(crate) vars: Arc<std::collections::HashMap<String, String>>,
    /// `vertical-align: middle` on an inline block / image: 0 baseline, 1 middle.
    pub(crate) valign: u8,
}

impl Default for Style {
    fn default() -> Style {
        Style {
            color: [0, 0, 0, 255],
            bg: [0, 0, 0, 0],
            font_px: 16.0,
            bold: false,
            align: 0,
            line_h: 1.2,
            hidden: false,
            margin: [0.0; 4],
            margin_auto: false,
            padding: [0.0; 4],
            width: None,
            height: None,
            none: false,
            inline: false,
            inline_block: false,
            radius: 0.0,
            radius_pct: false,
            bg_img: None,
            bg_size: BgSize::Auto,
            bg_repeat: (true, true),
            bg_pos: [Len::Pct(0.0), Len::Pct(0.0)],
            aspect: 0.0,
            node: 0,
            bw: [0.0; 4],
            bcolor: [[0, 0, 0, 255]; 4],
            border_box: false,
            position: 0,
            inset: [None; 4],
            z: 0,
            overflow_hidden: false,
            opacity: 1.0,
            min_w: None,
            max_w: None,
            min_h: None,
            max_h: None,
            m_auto: [false; 4],
            flex: false,
            inline_flex: false,
            grid: false,
            dir: 0,
            wrap: 0,
            justify: 0,
            align_items: 0,
            align_self: None,
            gap: [0.0; 2],
            grow: 0.0,
            shrink: 1.0,
            basis: None,
            order: 0,
            nowrap: false,
            transform: 0,
            letter_px: 0.0,
            underline: false,
            strike: false,
            ellipsis: false,
            gradient: None,
            shadow: None,
            vars: Arc::default(),
            valign: 0,
        }
    }
}

impl Style {
    pub(crate) fn inherit(&self) -> Style {
        Style {
            color: self.color,
            font_px: self.font_px,
            bold: self.bold,
            align: self.align,
            line_h: self.line_h,
            hidden: self.hidden,
            nowrap: self.nowrap,
            transform: self.transform,
            letter_px: self.letter_px,
            underline: self.underline,
            strike: self.strike,
            vars: self.vars.clone(),
            ..Style::default()
        }
    }
}

pub(crate) fn parse_color(s: &str) -> Option<[u8; 4]> {
    let s = s.trim().to_ascii_lowercase();
    if let Some(h) = s.strip_prefix('#') {
        let d: Vec<u8> = h.chars().map(|c| c.to_digit(16).map(|v| v as u8)).collect::<Option<Vec<_>>>()?;
        return match d.len() {
            3 => Some([d[0] * 17, d[1] * 17, d[2] * 17, 255]),
            4 => Some([d[0] * 17, d[1] * 17, d[2] * 17, d[3] * 17]),
            6 => Some([d[0] * 16 + d[1], d[2] * 16 + d[3], d[4] * 16 + d[5], 255]),
            8 => Some([d[0] * 16 + d[1], d[2] * 16 + d[3], d[4] * 16 + d[5], d[6] * 16 + d[7]]),
            _ => None,
        };
    }
    if let Some(a) = s.strip_prefix("rgba(").or_else(|| s.strip_prefix("rgb(")) {
        let inner = a.trim_end_matches(')');
        let p: Vec<f32> = inner
            .split(|c| c == ',' || c == ' ' || c == '/')
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .map(|x| match x.strip_suffix('%') {
                Some(pc) => pc.parse::<f32>().unwrap_or(0.0) * 2.55,
                None => x.parse::<f32>().unwrap_or(0.0),
            })
            .collect();
        if p.len() < 3 {
            return None;
        }
        let alpha = p.get(3).map_or(255.0, |a| if *a <= 1.0 { a * 255.0 } else { *a });
        return Some([p[0].clamp(0.0, 255.0) as u8, p[1].clamp(0.0, 255.0) as u8, p[2].clamp(0.0, 255.0) as u8, alpha.clamp(0.0, 255.0) as u8]);
    }
    if let Some(a) = s.strip_prefix("hsla(").or_else(|| s.strip_prefix("hsl(")) {
        let inner = a.trim_end_matches(')');
        let p: Vec<f32> = inner
            .split(|c| c == ',' || c == ' ' || c == '/')
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .map(|x| x.trim_end_matches("deg").trim_end_matches('%').parse::<f32>().unwrap_or(0.0))
            .collect();
        if p.len() < 3 {
            return None;
        }
        let (h, sat, l) = (p[0].rem_euclid(360.0) / 360.0, (p[1] / 100.0).clamp(0.0, 1.0), (p[2] / 100.0).clamp(0.0, 1.0));
        let q = if l < 0.5 { l * (1.0 + sat) } else { l + sat - l * sat };
        let pp = 2.0 * l - q;
        let ch = |mut t: f32| {
            t = t.rem_euclid(1.0);
            let v = if t < 1.0 / 6.0 {
                pp + (q - pp) * 6.0 * t
            } else if t < 0.5 {
                q
            } else if t < 2.0 / 3.0 {
                pp + (q - pp) * (2.0 / 3.0 - t) * 6.0
            } else {
                pp
            };
            (v * 255.0).round().clamp(0.0, 255.0) as u8
        };
        let alpha = p.get(3).map_or(255.0, |a| if *a <= 1.0 { a * 255.0 } else { *a });
        return Some([ch(h + 1.0 / 3.0), ch(h), ch(h - 1.0 / 3.0), alpha.clamp(0.0, 255.0) as u8]);
    }
    let named = match s.as_str() {
        "black" => [0, 0, 0, 255],
        "navy" => [0, 0, 128, 255],
        "teal" => [0, 128, 128, 255],
        "olive" => [128, 128, 0, 255],
        "maroon" => [128, 0, 0, 255],
        "purple" => [128, 0, 128, 255],
        "pink" => [255, 192, 203, 255],
        "brown" => [165, 42, 42, 255],
        "gold" => [255, 215, 0, 255],
        "darkgray" | "darkgrey" => [169, 169, 169, 255],
        "lightgray" | "lightgrey" => [211, 211, 211, 255],
        "dimgray" | "dimgrey" => [105, 105, 105, 255],
        "gainsboro" => [220, 220, 220, 255],
        "whitesmoke" => [245, 245, 245, 255],
        "darkred" => [139, 0, 0, 255],
        "darkgreen" => [0, 100, 0, 255],
        "darkblue" => [0, 0, 139, 255],
        "lightgreen" => [144, 238, 144, 255],
        "lightblue" => [173, 216, 230, 255],
        "skyblue" => [135, 206, 235, 255],
        "steelblue" => [70, 130, 180, 255],
        "royalblue" => [65, 105, 225, 255],
        "dodgerblue" => [30, 144, 255, 255],
        "orangered" => [255, 69, 0, 255],
        "tomato" => [255, 99, 71, 255],
        "crimson" => [220, 20, 60, 255],
        "indigo" => [75, 0, 130, 255],
        "violet" => [238, 130, 238, 255],
        "khaki" => [240, 230, 140, 255],
        "beige" => [245, 245, 220, 255],
        "ivory" => [255, 255, 240, 255],
        "lavender" => [230, 230, 250, 255],
        "salmon" => [250, 128, 114, 255],
        "coral" => [255, 127, 80, 255],
        "turquoise" => [64, 224, 208, 255],
        "limegreen" => [50, 205, 50, 255],
        "forestgreen" => [34, 139, 34, 255],
        "seagreen" => [46, 139, 87, 255],
        "chocolate" => [210, 105, 30, 255],
        "tan" => [210, 180, 140, 255],
        "slategray" | "slategrey" => [112, 128, 144, 255],
        "midnightblue" => [25, 25, 112, 255],
        "deeppink" => [255, 20, 147, 255],
        "hotpink" => [255, 105, 180, 255],
        "darkorange" => [255, 140, 0, 255],
        "white" => [255, 255, 255, 255],
        "red" => [255, 0, 0, 255],
        "green" => [0, 128, 0, 255],
        "lime" => [0, 255, 0, 255],
        "blue" => [0, 0, 255, 255],
        "yellow" => [255, 255, 0, 255],
        "orange" => [255, 165, 0, 255],
        "gray" | "grey" => [128, 128, 128, 255],
        "silver" => [192, 192, 192, 255],
        "cyan" | "aqua" => [0, 255, 255, 255],
        "magenta" | "fuchsia" => [255, 0, 255, 255],
        "transparent" => [0, 0, 0, 0],
        _ => return None,
    };
    Some(named)
}

pub(crate) struct Units {
    pub(crate) font: f32,
    pub(crate) vw: f32,
    pub(crate) vh: f32,
}

pub(crate) fn parse_len(v: &str, u: &Units) -> Option<Len> {
    let v = v.trim().to_ascii_lowercase();
    if v.starts_with("calc(") || v.starts_with("min(") || v.starts_with("max(") || v.starts_with("clamp(") || v.starts_with('(') {
        let (px, pct) = Calc::new(&v, u).eval()?;
        return Some(if pct == 0.0 { Len::Px(px) } else if px == 0.0 { Len::Pct(pct) } else { Len::Mix(px, pct) });
    }
    plain_len(&v, u)
}

fn plain_len(v: &str, u: &Units) -> Option<Len> {
    let num = |suffix: &str| v.strip_suffix(suffix).and_then(|n| n.trim().parse::<f32>().ok());
    if v == "0" {
        return Some(Len::Px(0.0));
    }
    // (longer suffixes first: `vmin` before `in`, `rem` before `em`)
    for (suffix, k) in [("vmin", u.vw.min(u.vh) / 100.0), ("vmax", u.vw.max(u.vh) / 100.0), ("rem", 16.0), ("vw", u.vw / 100.0), ("vh", u.vh / 100.0), ("em", u.font), ("ex", u.font * 0.5), ("ch", u.font * 0.5), ("pt", 4.0 / 3.0), ("pc", 16.0), ("cm", 96.0 / 2.54), ("mm", 96.0 / 25.4), ("in", 96.0), ("px", 1.0)] {
        if let Some(n) = num(suffix) {
            return Some(Len::Px(n * k));
        }
    }
    if let Some(n) = num("%") {
        return Some(Len::Pct(n));
    }
    v.parse::<f32>().ok().map(Len::Px)
}

/// A value inside `calc()`: pixels plus a percentage, or a plain number.
#[derive(Clone, Copy)]
struct V {
    px: f32,
    pct: f32,
    num: bool,
}

impl V {
    fn scale(self, k: f32) -> V {
        V { px: self.px * k, pct: self.pct * k, num: self.num }
    }
}

/// A `calc()`, `min()`, `max()` or `clamp()` expression: its value as pixels plus a percentage.
struct Calc<'a> {
    s: &'a [u8],
    i: usize,
    u: &'a Units,
}

impl<'a> Calc<'a> {
    fn new(s: &'a str, u: &'a Units) -> Calc<'a> {
        Calc { s: s.as_bytes(), i: 0, u }
    }

    fn eval(mut self) -> Option<(f32, f32)> {
        let v = self.sum()?;
        self.ws();
        (self.i >= self.s.len()).then_some((v.px, v.pct))
    }

    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.s.get(self.i).copied()
    }

    fn sum(&mut self) -> Option<V> {
        let mut a = self.product()?;
        loop {
            match self.peek() {
                Some(b'+') => {
                    self.i += 1;
                    let b = self.product()?;
                    a = V { px: a.px + b.px, pct: a.pct + b.pct, num: a.num && b.num };
                }
                Some(b'-') => {
                    self.i += 1;
                    let b = self.product()?;
                    a = V { px: a.px - b.px, pct: a.pct - b.pct, num: a.num && b.num };
                }
                _ => return Some(a),
            }
        }
    }

    fn product(&mut self) -> Option<V> {
        let mut a = self.atom()?;
        loop {
            match self.peek() {
                Some(b'*') => {
                    self.i += 1;
                    let b = self.atom()?;
                    a = if a.num { b.scale(a.px) } else if b.num { a.scale(b.px) } else { return None };
                }
                Some(b'/') => {
                    self.i += 1;
                    let b = self.atom()?;
                    if !b.num || b.px == 0.0 {
                        return None;
                    }
                    a = a.scale(1.0 / b.px);
                }
                _ => return Some(a),
            }
        }
    }

    fn atom(&mut self) -> Option<V> {
        match self.peek()? {
            b'(' => {
                self.i += 1;
                let v = self.sum()?;
                (self.peek()? == b')').then(|| self.i += 1)?;
                Some(v)
            }
            b'-' => {
                self.i += 1;
                Some(self.atom()?.scale(-1.0))
            }
            b'+' => {
                self.i += 1;
                self.atom()
            }
            c if c.is_ascii_alphabetic() => {
                let st = self.i;
                while self.i < self.s.len() && self.s[self.i].is_ascii_alphabetic() {
                    self.i += 1;
                }
                let name = std::str::from_utf8(&self.s[st..self.i]).ok()?.to_string();
                (self.peek()? == b'(').then(|| self.i += 1)?;
                let mut args = vec![self.sum()?];
                while self.peek()? == b',' {
                    self.i += 1;
                    args.push(self.sum()?);
                }
                (self.peek()? == b')').then(|| self.i += 1)?;
                // (a percentage in a comparison counts as that share of the viewport width)
                let vw = self.u.vw;
                let key = |v: &V| v.px + vw * v.pct / 100.0;
                match name.as_str() {
                    "calc" => args.pop(),
                    "min" => args.into_iter().min_by(|a, b| key(a).total_cmp(&key(b))),
                    "max" => args.into_iter().max_by(|a, b| key(a).total_cmp(&key(b))),
                    "clamp" if args.len() == 3 => {
                        let (lo, mid, hi) = (args[0], args[1], args[2]);
                        Some(if key(&mid) < key(&lo) { lo } else if key(&mid) > key(&hi) { hi } else { mid })
                    }
                    _ => None,
                }
            }
            _ => {
                let st = self.i;
                while self.i < self.s.len() && (self.s[self.i].is_ascii_alphanumeric() || matches!(self.s[self.i], b'.' | b'%')) {
                    self.i += 1;
                }
                let tok = std::str::from_utf8(&self.s[st..self.i]).ok()?;
                // a bare number is a plain number; with a unit it is a length
                if let Ok(n) = tok.parse::<f32>() {
                    return Some(V { px: n, pct: 0.0, num: true });
                }
                match plain_len(tok, self.u)? {
                    Len::Px(v) => Some(V { px: v, pct: 0.0, num: false }),
                    Len::Pct(p) => Some(V { px: 0.0, pct: p, num: false }),
                    Len::Mix(a, b) => Some(V { px: a, pct: b, num: false }),
                }
            }
        }
    }
}

/// Replace every `var(--name, fallback)` of a value by the custom property's value.
pub(crate) fn resolve_vars(val: &str, vars: &std::collections::HashMap<String, String>) -> String {
    let mut out = val.to_string();
    for _ in 0..8 {
        let Some(at) = out.find("var(") else { break };
        // the matching close
        let mut depth = 0;
        let mut end = None;
        for (i, c) in out[at + 3..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(at + 3 + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else { break };
        let inner = &out[at + 4..end];
        let (name, fallback) = match inner.split_once(',') {
            Some((n, f)) => (n.trim(), Some(f.trim())),
            None => (inner.trim(), None),
        };
        let rep = vars.get(name).cloned().or_else(|| fallback.map(str::to_string)).unwrap_or_default();
        out.replace_range(at..=end, &rep);
    }
    out
}

/// Split a value at spaces and at `/`, but not inside parentheses (`rgba(0, 0, 0, .5)`,
/// `url(a b.png)`); a `/` is a token of its own.
pub(crate) fn split_top(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start: Option<usize> = None;
    for (i, c) in s.char_indices() {
        match c {
            '(' => {
                depth += 1;
                start.get_or_insert(i);
            }
            ')' => depth = (depth - 1).max(0),
            c if depth == 0 && (c.is_whitespace() || c == '/') => {
                if let Some(st) = start.take() {
                    out.push(&s[st..i]);
                }
                if c == '/' {
                    out.push("/");
                }
            }
            _ => {
                start.get_or_insert(i);
            }
        }
    }
    if let Some(st) = start {
        out.push(&s[st..]);
    }
    out
}

/// The first `url(...)` of a value: the path (quotes removed) and the byte range of the
/// whole `url(...)` in `val`.
pub(crate) fn find_url(val: &str) -> Option<(String, usize, usize)> {
    let s = val.to_ascii_lowercase().find("url(")?;
    let inner = s + 4;
    let e = val[inner..].find(')')? + inner;
    let path = val[inner..e].trim().trim_matches(|c| c == '"' || c == '\'').trim().to_string();
    Some((path, s, e + 1))
}

pub(crate) fn parse_bg_size(t: &[&str], u: &Units) -> Option<BgSize> {
    let len = |x: &str| if x == "auto" { Some(None) } else { parse_len(x, u).map(Some) };
    match t {
        ["cover"] => Some(BgSize::Cover),
        ["contain"] => Some(BgSize::Contain),
        [a] => Some(BgSize::Dims(len(*a)?, None)),
        [a, b] => Some(BgSize::Dims(len(*a)?, len(*b)?)),
        _ => None,
    }
}

pub(crate) fn parse_bg_pos(t: &[&str], u: &Units) -> Option<[Len; 2]> {
    let val = |x: &str| match x {
        "left" | "top" => Some(Len::Pct(0.0)),
        "center" => Some(Len::Pct(50.0)),
        "right" | "bottom" => Some(Len::Pct(100.0)),
        _ => parse_len(x, u),
    };
    match t {
        [a] => {
            let v = val(*a)?;
            Some(if matches!(*a, "top" | "bottom") { [Len::Pct(50.0), v] } else { [v, Len::Pct(50.0)] })
        }
        [a, b] => {
            let (x, y) = (val(*a)?, val(*b)?);
            Some(if matches!(*a, "top" | "bottom") || matches!(*b, "left" | "right") { [y, x] } else { [x, y] })
        }
        _ => None,
    }
}

pub(crate) fn box_values(v: &str, u: &Units) -> ([f32; 4], bool) {
    let toks: Vec<&str> = v.split_whitespace().collect();
    let mut auto = false;
    let vals: Vec<f32> = toks
        .iter()
        .map(|t| {
            if *t == "auto" {
                auto = true;
                0.0
            } else {
                parse_len(t, u).map_or(0.0, |l| l.px(u.vw))
            }
        })
        .collect();
    let out = match vals.len() {
        1 => [vals[0]; 4],
        2 => [vals[0], vals[1], vals[0], vals[1]],
        3 => [vals[0], vals[1], vals[2], vals[1]],
        4 => [vals[0], vals[1], vals[2], vals[3]],
        _ => [0.0; 4],
    };
    (out, auto)
}

impl Style {
    pub(crate) fn apply(&mut self, prop: &str, val: &str, parent_font: f32, vw: f32, vh: f32) {
        let val = val.trim().trim_end_matches("!important").trim();
        let u = Units { font: parent_font, vw, vh };
        let own = Units { font: self.font_px, vw, vh };
        match prop {
            "color" => {
                if let Some(c) = parse_color(val) {
                    self.color = c;
                }
            }
            "background-color" => {
                if let Some(c) = split_top(val).into_iter().find_map(parse_color) {
                    self.bg = c;
                }
            }
            "background" if val.to_ascii_lowercase().contains("linear-gradient(") => {
                self.bg_img = None;
                self.gradient = parse_gradient(val, &own);
                // (any colour next to it is the fallback under the gradient)
                if let Some(c) = split_top(val).into_iter().find_map(parse_color) {
                    self.bg = c;
                }
            }
            "background-image" if val.to_ascii_lowercase().contains("linear-gradient(") => {
                self.bg_img = None;
                self.gradient = parse_gradient(val, &own);
            }
            "background" => {
                self.gradient = None;
                // the shorthand resets what it does not name (colour excepted, as before)
                self.bg_img = None;
                self.bg_size = BgSize::Auto;
                self.bg_repeat = (true, true);
                self.bg_pos = [Len::Pct(0.0), Len::Pct(0.0)];
                let mut rest = val.to_string();
                if let Some((path, s, e)) = find_url(val) {
                    if !path.is_empty() {
                        self.bg_img = Some(Arc::from(path.as_str()));
                    }
                    rest.replace_range(s..e, " ");
                }
                let (mut pos, mut size, mut slash) = (Vec::new(), Vec::new(), false);
                for t in split_top(&rest) {
                    if t == "/" {
                        slash = true;
                    } else if let Some(c) = parse_color(t) {
                        self.bg = c;
                    } else {
                        match t {
                            "no-repeat" => self.bg_repeat = (false, false),
                            "repeat" => self.bg_repeat = (true, true),
                            "repeat-x" => self.bg_repeat = (true, false),
                            "repeat-y" => self.bg_repeat = (false, true),
                            "none" => self.bg_img = None,
                            _ if slash => size.push(t),
                            _ => pos.push(t),
                        }
                    }
                }
                if let Some(p) = parse_bg_pos(&pos, &own) {
                    self.bg_pos = p;
                }
                if let Some(z) = parse_bg_size(&size, &own) {
                    self.bg_size = z;
                }
            }
            "background-image" => {
                self.bg_img = find_url(val).and_then(|(p, _, _)| if p.is_empty() { None } else { Some(Arc::from(p.as_str())) });
            }
            "background-size" => {
                if let Some(z) = parse_bg_size(&split_top(val), &own) {
                    self.bg_size = z;
                }
            }
            "background-repeat" => match val {
                "no-repeat" => self.bg_repeat = (false, false),
                "repeat" => self.bg_repeat = (true, true),
                "repeat-x" => self.bg_repeat = (true, false),
                "repeat-y" => self.bg_repeat = (false, true),
                _ => {}
            },
            "background-position" => {
                if let Some(p) = parse_bg_pos(&split_top(val), &own) {
                    self.bg_pos = p;
                }
            }
            "font-size" => {
                let named = match val {
                    "small" => Some(13.0),
                    "medium" => Some(16.0),
                    "large" => Some(18.0),
                    "x-large" => Some(24.0),
                    "xx-large" => Some(32.0),
                    _ => None,
                };
                if let Some(px) = named {
                    self.font_px = px;
                } else if let Some(l) = parse_len(val, &u) {
                    self.font_px = l.px(parent_font).max(1.0);
                }
            }
            "font-weight" => {
                self.bold = matches!(val, "bold" | "bolder") || val.parse::<u32>().map_or(false, |w| w >= 600);
            }
            "text-align" => {
                self.align = match val {
                    "center" => 1,
                    "right" | "end" => 2,
                    _ => 0,
                };
            }
            "line-height" => {
                if let Ok(f) = val.parse::<f32>() {
                    self.line_h = f;
                } else if let Some(l) = parse_len(val, &own) {
                    self.line_h = l.px(self.font_px) / self.font_px;
                }
            }
            "margin" => {
                let (v, a) = box_values(val, &own);
                self.margin = v;
                self.margin_auto = a;
                self.m_auto = auto_sides(val);
            }
            "margin-top" => {
                self.m_auto[0] = val == "auto";
                self.margin[0] = parse_len(val, &own).map_or(0.0, |l| l.px(vw))
            }
            "margin-right" => {
                self.m_auto[1] = val == "auto";
                self.margin_auto |= val == "auto";
                self.margin[1] = parse_len(val, &own).map_or(0.0, |l| l.px(vw));
            }
            "margin-bottom" => {
                self.m_auto[2] = val == "auto";
                self.margin[2] = parse_len(val, &own).map_or(0.0, |l| l.px(vw))
            }
            "margin-left" => {
                self.m_auto[3] = val == "auto";
                self.margin_auto |= val == "auto";
                self.margin[3] = parse_len(val, &own).map_or(0.0, |l| l.px(vw));
            }
            "padding" => self.padding = box_values(val, &own).0,
            "padding-top" => self.padding[0] = parse_len(val, &own).map_or(0.0, |l| l.px(vw)),
            "padding-right" => self.padding[1] = parse_len(val, &own).map_or(0.0, |l| l.px(vw)),
            "padding-bottom" => self.padding[2] = parse_len(val, &own).map_or(0.0, |l| l.px(vw)),
            "padding-left" => self.padding[3] = parse_len(val, &own).map_or(0.0, |l| l.px(vw)),
            "width" => self.width = if val == "auto" { None } else { parse_len(val, &own) },
            "height" => self.height = if val == "auto" { None } else { parse_len(val, &own) },
            "display" => {
                self.none = val == "none";
                self.inline = val == "inline";
                self.inline_block = matches!(val, "inline-block" | "inline-flex" | "inline-grid" | "inline-table");
                self.flex = matches!(val, "flex" | "inline-flex");
                self.inline_flex = val == "inline-flex";
                self.grid = matches!(val, "grid" | "inline-grid");
            }
            "visibility" => self.hidden = val == "hidden",
            "border-radius" => {
                // (one radius for all corners: the first value; 50% is a circle or a pill)
                let first = split_top(val).into_iter().next().unwrap_or("0");
                self.radius = match parse_len(first, &own) {
                    Some(Len::Pct(p)) => p,
                    Some(l) => l.px(vw),
                    None => 0.0,
                };
                self.radius_pct = first.trim_end().ends_with('%');
            }
            _ => self.apply_more(prop, val, &own, vw, vh),
        }
    }

    fn apply_more(&mut self, prop: &str, val: &str, own: &Units, vw: f32, vh: f32) {
        let len = |v: &str| parse_len(v, own);
        let side = |name: &str| ["top", "right", "bottom", "left"].iter().position(|s| *s == name);
        let _ = vh;
        match prop {
            // ---- borders
            "border" | "border-top" | "border-right" | "border-bottom" | "border-left" => {
                let sides: Vec<usize> = match prop.strip_prefix("border-").and_then(side) {
                    Some(i) => vec![i],
                    None => vec![0, 1, 2, 3],
                };
                let (mut w, mut style, mut color) = (None, None, None);
                for t in split_top(val) {
                    if let Some(c) = parse_color(t) {
                        color = Some(c);
                    } else if matches!(t, "none" | "hidden" | "solid" | "dashed" | "dotted" | "double" | "groove" | "ridge" | "inset" | "outset") {
                        style = Some(t != "none" && t != "hidden");
                    } else if let Some(l) = match t {
                        "thin" => Some(Len::Px(1.0)),
                        "medium" => Some(Len::Px(3.0)),
                        "thick" => Some(Len::Px(5.0)),
                        _ => len(t),
                    } {
                        w = Some(l.px(vw));
                    }
                }
                for i in sides {
                    // (no style named means solid, which CSS does not do and pages rarely rely on)
                    let on = style.unwrap_or(w.is_some());
                    self.bw[i] = if on { w.unwrap_or(3.0) } else { 0.0 };
                    if let Some(c) = color {
                        self.bcolor[i] = c;
                    }
                }
            }
            "border-width" => {
                let (v, _) = box_values(val, own);
                self.bw = v;
            }
            "border-color" => {
                let cs: Vec<[u8; 4]> = split_top(val).into_iter().filter_map(parse_color).collect();
                self.bcolor = match cs.len() {
                    1 => [cs[0]; 4],
                    2 => [cs[0], cs[1], cs[0], cs[1]],
                    3 => [cs[0], cs[1], cs[2], cs[1]],
                    4 => [cs[0], cs[1], cs[2], cs[3]],
                    _ => self.bcolor,
                };
            }
            "border-style" => {
                if val.split_whitespace().all(|t| t == "none" || t == "hidden") {
                    self.bw = [0.0; 4];
                }
            }
            "border-top-width" | "border-right-width" | "border-bottom-width" | "border-left-width" => {
                if let Some(i) = prop.strip_prefix("border-").and_then(|r| r.strip_suffix("-width")).and_then(side) {
                    self.bw[i] = len(val).map_or(0.0, |l| l.px(vw));
                }
            }
            "border-top-color" | "border-right-color" | "border-bottom-color" | "border-left-color" => {
                if let (Some(i), Some(c)) = (prop.strip_prefix("border-").and_then(|r| r.strip_suffix("-color")).and_then(side), parse_color(val)) {
                    self.bcolor[i] = c;
                }
            }
            "box-sizing" => self.border_box = val == "border-box",
            // ---- size limits
            "min-width" => self.min_w = if val == "auto" || val == "0" { None } else { len(val) },
            "max-width" => self.max_w = if val == "none" { None } else { len(val) },
            "min-height" => self.min_h = if val == "auto" || val == "0" { None } else { len(val) },
            "max-height" => self.max_h = if val == "none" { None } else { len(val) },
            // ---- position
            "position" => {
                self.position = match val {
                    "relative" | "sticky" => 1,
                    "absolute" => 2,
                    "fixed" => 3,
                    _ => 0,
                }
            }
            "top" | "right" | "bottom" | "left" => {
                if let Some(i) = side(prop) {
                    self.inset[i] = if val == "auto" { None } else { len(val) };
                }
            }
            "inset" => {
                let toks: Vec<Option<Len>> = split_top(val).into_iter().map(|t| if t == "auto" { None } else { len(t) }).collect();
                self.inset = match toks.len() {
                    1 => [toks[0]; 4],
                    2 => [toks[0], toks[1], toks[0], toks[1]],
                    3 => [toks[0], toks[1], toks[2], toks[1]],
                    4 => [toks[0], toks[1], toks[2], toks[3]],
                    _ => self.inset,
                };
            }
            "z-index" => self.z = val.parse().unwrap_or(0),
            "overflow" | "overflow-x" | "overflow-y" => self.overflow_hidden = val.split_whitespace().any(|t| matches!(t, "hidden" | "scroll" | "auto" | "clip")),
            "opacity" => self.opacity = val.trim_end_matches('%').parse::<f32>().map_or(1.0, |o| if val.ends_with('%') { o / 100.0 } else { o }).clamp(0.0, 1.0),
            // ---- flexbox
            "flex-direction" => {
                self.dir = match val {
                    "row-reverse" => 1,
                    "column" => 2,
                    "column-reverse" => 3,
                    _ => 0,
                }
            }
            "flex-wrap" => {
                self.wrap = match val {
                    "wrap" => 1,
                    "wrap-reverse" => 2,
                    _ => 0,
                }
            }
            "flex-flow" => {
                for t in val.split_whitespace() {
                    self.apply_more(if t.starts_with("wrap") || t == "nowrap" { "flex-wrap" } else { "flex-direction" }, t, own, vw, vh);
                }
            }
            "justify-content" => {
                self.justify = match val {
                    "flex-end" | "end" | "right" => 1,
                    "center" => 2,
                    "space-between" => 3,
                    "space-around" => 4,
                    "space-evenly" => 5,
                    _ => 0,
                }
            }
            "align-items" => self.align_items = align_code(val).unwrap_or(0),
            "align-self" => self.align_self = if val == "auto" { None } else { align_code(val) },
            "gap" | "grid-gap" => {
                let v: Vec<f32> = split_top(val).into_iter().filter_map(len).map(|l| l.px(vw)).collect();
                match v.as_slice() {
                    [a] => self.gap = [*a, *a],
                    [a, b] => self.gap = [*a, *b],
                    _ => {}
                }
            }
            "row-gap" => self.gap[0] = len(val).map_or(0.0, |l| l.px(vw)),
            "column-gap" => self.gap[1] = len(val).map_or(0.0, |l| l.px(vw)),
            "flex-grow" => self.grow = val.parse().unwrap_or(0.0),
            "flex-shrink" => self.shrink = val.parse().unwrap_or(1.0),
            "flex-basis" => self.basis = if val == "auto" || val == "content" { None } else { len(val) },
            "flex" => {
                match val {
                    "none" => (self.grow, self.shrink, self.basis) = (0.0, 0.0, None),
                    "auto" => (self.grow, self.shrink, self.basis) = (1.0, 1.0, None),
                    "initial" => (self.grow, self.shrink, self.basis) = (0.0, 1.0, None),
                    _ => {
                        let toks = split_top(val);
                        let nums: Vec<f32> = toks.iter().take(2).map_while(|t| t.parse::<f32>().ok()).collect();
                        self.grow = nums.first().copied().unwrap_or(1.0);
                        self.shrink = nums.get(1).copied().unwrap_or(1.0);
                        // `flex: 1` means a basis of 0; a basis may follow
                        self.basis = toks.get(nums.len()).and_then(|t| len(t)).or(Some(Len::Px(0.0)));
                    }
                }
            }
            "order" => self.order = val.parse().unwrap_or(0),
            // ---- text
            "white-space" => self.nowrap = matches!(val, "nowrap" | "pre"),
            "text-transform" => {
                self.transform = match val {
                    "uppercase" => 1,
                    "lowercase" => 2,
                    "capitalize" => 3,
                    _ => 0,
                }
            }
            "letter-spacing" => self.letter_px = if val == "normal" { 0.0 } else { len(val).map_or(0.0, |l| l.px(self.font_px)) },
            "text-decoration" | "text-decoration-line" => {
                self.underline = val.contains("underline");
                self.strike = val.contains("line-through");
            }
            "text-overflow" => self.ellipsis = val == "ellipsis",
            "vertical-align" => self.valign = (val == "middle") as u8,
            "box-shadow" => {
                self.shadow = if val == "none" { None } else { parse_shadow(val, own) };
            }
            _ => {}
        }
    }
}

fn auto_sides(val: &str) -> [bool; 4] {
    let t: Vec<bool> = val.split_whitespace().map(|x| x == "auto").collect();
    match t.len() {
        1 => [t[0]; 4],
        2 => [t[0], t[1], t[0], t[1]],
        3 => [t[0], t[1], t[2], t[1]],
        4 => [t[0], t[1], t[2], t[3]],
        _ => [false; 4],
    }
}

fn align_code(v: &str) -> Option<u8> {
    Some(match v {
        "stretch" | "normal" => 0,
        "flex-start" | "start" | "self-start" => 1,
        "flex-end" | "end" | "self-end" => 2,
        "center" => 3,
        "baseline" => 4,
        _ => return None,
    })
}

fn parse_shadow(val: &str, u: &Units) -> Option<Shadow> {
    let first = {
        // the first shadow of a comma list, commas inside colours kept
        let mut depth = 0;
        let mut end = val.len();
        for (i, c) in val.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => depth -= 1,
                ',' if depth == 0 => {
                    end = i;
                    break;
                }
                _ => {}
            }
        }
        &val[..end]
    };
    if first.split_whitespace().any(|t| t == "inset") {
        return None;
    }
    let mut nums = Vec::new();
    let mut color = [0, 0, 0, 255];
    for t in split_top(first) {
        if let Some(c) = parse_color(t) {
            color = c;
        } else if let Some(l) = parse_len(t, u) {
            nums.push(l.px(u.vw));
        }
    }
    if nums.len() < 2 {
        return None;
    }
    Some(Shadow { dx: nums[0], dy: nums[1], blur: nums.get(2).copied().unwrap_or(0.0).max(0.0), spread: nums.get(3).copied().unwrap_or(0.0), color })
}

/// `linear-gradient([angle | to side,] stop, stop, ...)`.
fn parse_gradient(val: &str, u: &Units) -> Option<Gradient> {
    let low = val.to_ascii_lowercase();
    let at = low.find("linear-gradient(")?;
    let inner_start = at + "linear-gradient(".len();
    let mut depth = 1;
    let mut end = None;
    for (i, c) in val[inner_start..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(inner_start + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let inner = &val[inner_start..end?];
    // split at top-level commas
    let mut parts: Vec<&str> = Vec::new();
    let (mut d, mut st) = (0, 0);
    for (i, c) in inner.char_indices() {
        match c {
            '(' => d += 1,
            ')' => d -= 1,
            ',' if d == 0 => {
                parts.push(inner[st..i].trim());
                st = i + 1;
            }
            _ => {}
        }
    }
    parts.push(inner[st..].trim());
    let mut angle = 180.0;
    let first = parts.first()?.to_ascii_lowercase();
    if let Some(a) = first.strip_suffix("deg").and_then(|n| n.trim().parse::<f32>().ok()) {
        angle = a;
        parts.remove(0);
    } else if let Some(rest) = first.strip_prefix("to ") {
        angle = match rest.trim() {
            "top" => 0.0,
            "right" => 90.0,
            "bottom" => 180.0,
            "left" => 270.0,
            "top right" | "right top" => 45.0,
            "bottom right" | "right bottom" => 135.0,
            "bottom left" | "left bottom" => 225.0,
            "top left" | "left top" => 315.0,
            _ => 180.0,
        };
        parts.remove(0);
    }
    let mut stops = Vec::new();
    for p in parts {
        let toks = split_top(p);
        let color = toks.iter().find_map(|t| parse_color(t))?;
        let pos = toks.iter().find(|t| parse_color(t).is_none()).and_then(|t| parse_len(t, u));
        stops.push(GStop { pos, color });
    }
    (stops.len() >= 2).then_some(Gradient { angle, stops })
}
