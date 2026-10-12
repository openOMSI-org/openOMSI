//! RosterFlow's prints: the bus graph and the duty graph as a PDF file (Optibus's "Bus Graph":
//! a row per bus or duty on an hour axis, its trips as bars in the line's colour with the
//! stops' first letters and the minutes over their ends, the depot runs black, the duties
//! under the bus as a grey band with their numbers). A small PDF writer of its own: pages of
//! rectangles, polygons, lines and words in the Helvetica every PDF reader has.

/// A PDF being written: its pages' drawing, in points from the top left.
pub struct Pdf {
    w: f32,
    h: f32,
    pages: Vec<String>,
}

fn rgb(c: [u8; 3]) -> String {
    format!("{:.3} {:.3} {:.3}", c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0)
}

/// A text as a PDF string in WinAnsi: Latin-1's letters as they are, others a "?".
fn pdf_string(s: &str) -> Vec<u8> {
    let mut out = vec![b'('];
    for ch in s.chars() {
        let b = match ch {
            '\u{20AC}' => 0x80,
            '\u{2013}' => 0x96,
            '\u{2014}' => 0x97,
            '\u{2192}' => b'>',
            c if (c as u32) < 0x20 => b' ',
            c if (c as u32) < 0x100 => c as u32 as u8,
            _ => b'?',
        };
        if matches!(b, b'(' | b')' | b'\\') {
            out.push(b'\\');
        }
        out.push(b);
    }
    out.push(b')');
    out
}

impl Pdf {
    /// A PDF of pages `w` by `h` points (A4 across: 842 by 595), its first page begun.
    pub fn new(w: f32, h: f32) -> Pdf {
        Pdf { w, h, pages: vec![String::new()] }
    }

    pub fn new_page(&mut self) {
        self.pages.push(String::new());
    }

    pub fn pages(&self) -> usize {
        self.pages.len()
    }

    fn out(&mut self) -> &mut String {
        self.pages.last_mut().unwrap()
    }

    fn y(&self, y: f32) -> f32 {
        self.h - y
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, fill: [u8; 3]) {
        let yy = self.y(y + h);
        let s = format!("{} rg {x:.2} {yy:.2} {w:.2} {h:.2} re f\n", rgb(fill));
        self.out().push_str(&s);
    }

    pub fn frame(&mut self, x: f32, y: f32, w: f32, h: f32, width: f32, c: [u8; 3]) {
        let yy = self.y(y + h);
        let s = format!("{} RG {width:.2} w {x:.2} {yy:.2} {w:.2} {h:.2} re S\n", rgb(c));
        self.out().push_str(&s);
    }

    pub fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, c: [u8; 3]) {
        let (ay, by) = (self.y(a.1), self.y(b.1));
        let s = format!("{} RG {width:.2} w {:.2} {ay:.2} m {:.2} {by:.2} l S\n", rgb(c), a.0, b.0);
        self.out().push_str(&s);
    }

    /// A filled polygon, outlined in `edge` if given.
    pub fn poly(&mut self, pts: &[(f32, f32)], fill: [u8; 3], edge: Option<[u8; 3]>) {
        if pts.len() < 3 {
            return;
        }
        let mut s = format!("{} rg ", rgb(fill));
        if let Some(e) = edge {
            s.push_str(&format!("{} RG 0.4 w ", rgb(e)));
        }
        for (k, (x, y)) in pts.iter().enumerate() {
            s.push_str(&format!("{x:.2} {:.2} {} ", self.y(*y), if k == 0 { "m" } else { "l" }));
        }
        s.push_str(if edge.is_some() { "h B\n" } else { "h f\n" });
        self.out().push_str(&s);
    }

    /// How wide a text is set (Helvetica's average widths: near enough to centre it).
    pub fn width(s: &str, size: f32, bold: bool) -> f32 {
        let per: f32 = s
            .chars()
            .map(|c| match c {
                'i' | 'l' | 'j' | '.' | ',' | ':' | ';' | '!' | '|' | '\'' | ' ' => 0.28,
                'f' | 't' | 'r' | 'I' | '(' | ')' | '-' => 0.36,
                'm' | 'w' | 'M' | 'W' => 0.83,
                c if c.is_ascii_uppercase() => 0.68,
                c if c.is_ascii_digit() => 0.556,
                _ => 0.54,
            })
            .sum();
        per * size * if bold { 1.06 } else { 1.0 }
    }

    /// Words with their baseline at `y`; `align` 0 left, 1 centred, 2 right of `x`.
    pub fn text(&mut self, x: f32, y: f32, size: f32, bold: bool, s: &str, c: [u8; 3], align: u8) {
        let w = Self::width(s, size, bold);
        let x = match align {
            1 => x - w * 0.5,
            2 => x - w,
            _ => x,
        };
        let yy = self.y(y);
        let mut b = format!("BT /{} {size:.1} Tf {} rg {x:.2} {yy:.2} Td ", if bold { "F2" } else { "F1" }, rgb(c)).into_bytes();
        b.extend(pdf_string(s));
        b.extend(b" Tj ET\n");
        // (the page's drawing is kept as a string: Latin-1 bytes stored as their chars)
        let s: String = b.iter().map(|&x| x as char).collect();
        self.out().push_str(&s);
    }

    /// The file.
    pub fn finish(&self) -> Vec<u8> {
        let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
        let mut offsets: Vec<usize> = Vec::new();
        let n = self.pages.len();
        // objects: 1 catalog, 2 pages, 3 F1, 4 F2, then a page and its contents each
        let obj = |out: &mut Vec<u8>, offsets: &mut Vec<usize>, body: Vec<u8>| {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n", offsets.len()).into_bytes());
            out.extend(body);
            out.extend(b"\nendobj\n");
        };
        obj(&mut out, &mut offsets, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
        let kids: Vec<String> = (0..n).map(|k| format!("{} 0 R", 5 + 2 * k)).collect();
        obj(&mut out, &mut offsets, format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" ")).into_bytes());
        obj(&mut out, &mut offsets, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec());
        obj(&mut out, &mut offsets, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>".to_vec());
        for (k, page) in self.pages.iter().enumerate() {
            obj(&mut out, &mut offsets, format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.0} {:.0}] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {} 0 R >>", self.w, self.h, 6 + 2 * k).into_bytes());
            let bytes: Vec<u8> = page.chars().map(|c| c as u32 as u8).collect();
            let mut body = format!("<< /Length {} >>\nstream\n", bytes.len()).into_bytes();
            body.extend(bytes);
            body.extend(b"\nendstream");
            obj(&mut out, &mut offsets, body);
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).into_bytes());
        for o in &offsets {
            out.extend(format!("{o:010} 00000 n \n").into_bytes());
        }
        out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", offsets.len() + 1).into_bytes());
        out
    }
}

// --- the graphs ----------------------------------------------------------------------------------

/// What a graph's row shows on its axis.
#[derive(Clone, Debug, PartialEq)]
pub enum ItemKind {
    /// A trip with passengers, pointed the way it runs (`inbound`: to the left).
    Trip { inbound: bool },
    /// A depot run (black, "P" over it).
    Pull,
    /// An empty run between places.
    Deadhead,
    /// Signing on or off.
    Sign,
    /// Travel to a relief point (its letter).
    Travel,
    /// Custom work.
    Custom,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphItem {
    pub from: i32,
    pub to: i32,
    pub kind: ItemKind,
    pub label: String,
    pub colour: [u8; 3],
    /// Over its ends: the stop it leaves (two letters) and the minute, the stop it reaches.
    pub start_stop: String,
    pub end_stop: String,
}

/// A row: a bus or a duty, what it runs, and the band under it (a bus's duties, a duty's buses).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct GraphRow {
    pub name: String,
    pub sub: String,
    pub sub2: String,
    pub items: Vec<GraphItem>,
    pub under: Vec<(i32, i32, String)>,
}

const INK: [u8; 3] = [20, 20, 20];
const GREY: [u8; 3] = [120, 120, 120];
const RULE: [u8; 3] = [205, 205, 205];

/// The hour's label on the axis.
fn hour(h: i32) -> String {
    format!("{:02}", h.rem_euclid(24))
}

/// A graph (Optibus's bus graph or duty graph) as a PDF: `title` and the words after it
/// (`info`), when it was printed, and its rows, as many pages as they need (A4 across).
pub fn graph(title: &str, info: &[String], printed: &str, page_word: &str, rows: &[GraphRow]) -> Vec<u8> {
    let (pw, ph) = (842.0f32, 595.0f32);
    let mut pdf = Pdf::new(pw, ph);
    let t0 = rows.iter().flat_map(|r| r.items.iter().map(|i| i.from)).min().unwrap_or(300).div_euclid(60) * 60;
    let t1 = ((rows.iter().flat_map(|r| r.items.iter().map(|i| i.to)).max().unwrap_or(1380) + 59).div_euclid(60) * 60).max(t0 + 120);
    let (left, right) = (78.0f32, pw - 22.0);
    let x_of = |m: i32| left + (m - t0) as f32 / (t1 - t0) as f32 * (right - left);
    let row_h = 46.0f32;
    let top = 74.0f32;
    let per_page = (((ph - top - 40.0) / row_h).floor() as usize).max(1);
    let pages = rows.len().div_ceil(per_page).max(1);
    for page in 0..pages {
        if page > 0 {
            pdf.new_page();
        }
        pdf.text(22.0, 34.0, 20.0, true, title, INK, 0);
        let mut x = 22.0 + Pdf::width(title, 20.0, true) + 18.0;
        for w in info {
            pdf.text(x, 34.0, 12.0, false, w, INK, 0);
            x += Pdf::width(w, 12.0, false) + 16.0;
        }
        pdf.text(right, 22.0, 9.0, false, &format!("{page_word}: {}/{pages}", page + 1), INK, 2);
        pdf.text(right, 34.0, 9.0, false, printed, INK, 2);
        let shown = &rows[page * per_page..((page + 1) * per_page).min(rows.len())];
        let bottom = top + shown.len() as f32 * row_h;
        // the hours, on top and under the rows, and their lines down
        for h in (t0 / 60)..=(t1 / 60) {
            let xh = x_of(h * 60);
            pdf.text(xh, top - 6.0, 10.0, false, &hour(h), INK, 1);
            pdf.text(xh, bottom + 14.0, 10.0, false, &hour(h), INK, 1);
            pdf.line((xh, top - 2.0), (xh, bottom), 0.3, RULE);
            if h * 60 + 30 < t1 {
                let xm = x_of(h * 60 + 30);
                pdf.line((xm, top), (xm, bottom), 0.2, [232, 232, 232]);
            }
        }
        for (k, r) in shown.iter().enumerate() {
            let y = top + k as f32 * row_h;
            pdf.line((22.0, y + row_h - 6.0), (right, y + row_h - 6.0), 0.3, RULE);
            pdf.text(48.0, y + 18.0, 11.0, false, &r.name, INK, 1);
            pdf.text(48.0, y + 27.0, 6.0, false, &r.sub, INK, 1);
            pdf.text(48.0, y + 34.0, 6.0, false, &r.sub2, INK, 1);
            let bar_y = y + 22.0;
            let bar_h = 6.5;
            // (the last trip's end: where, and at which stop)
            let mut last_end: Option<(f32, String)> = None;
            for it in &r.items {
                let (a, b) = (x_of(it.from), x_of(it.to).max(x_of(it.from) + 1.0));
                match it.kind {
                    ItemKind::Trip { inbound } => {
                        let tip = 3.0f32.min((b - a) * 0.4);
                        let pts = if inbound {
                            vec![(a, bar_y + bar_h * 0.5), (a + tip, bar_y), (b, bar_y), (b, bar_y + bar_h), (a + tip, bar_y + bar_h)]
                        } else {
                            vec![(a, bar_y), (b - tip, bar_y), (b, bar_y + bar_h * 0.5), (b - tip, bar_y + bar_h), (a, bar_y + bar_h)]
                        };
                        pdf.poly(&pts, it.colour, Some([60, 60, 60]));
                        if Pdf::width(&it.label, 5.5, false) < b - a + 8.0 {
                            pdf.text((a + b) * 0.5, bar_y - 1.2, 5.5, false, &it.label, INK, 1);
                        }
                    }
                    ItemKind::Pull => {
                        pdf.rect(a, bar_y, b - a, bar_h, [0, 0, 0]);
                        pdf.text((a + b) * 0.5, bar_y - 1.2, 5.5, false, "P", INK, 1);
                    }
                    ItemKind::Deadhead => pdf.rect(a, bar_y + bar_h * 0.35, b - a, bar_h * 0.3, [90, 90, 90]),
                    ItemKind::Sign => pdf.line((a, bar_y + bar_h * 0.5), (b, bar_y + bar_h * 0.5), 0.8, GREY),
                    ItemKind::Travel | ItemKind::Custom => {
                        pdf.rect(a, bar_y, b - a, bar_h, [250, 250, 250]);
                        pdf.frame(a, bar_y, b - a, bar_h, 0.5, if it.kind == ItemKind::Travel { [214, 150, 20] } else { GREY });
                        pdf.text((a + b) * 0.5, bar_y + bar_h - 1.4, 5.0, true, &it.label, INK, 1);
                    }
                }
                // the stops' letters and the minutes over the trip's ends
                // (as Optibus prints them: "Re 55 | 00" - the stop once where one trip ends and
                // the next begins, the minutes either side of the line)
                if matches!(it.kind, ItemKind::Trip { .. }) {
                    pdf.line((a, bar_y - 7.0), (a, bar_y), 0.3, INK);
                    let joined = last_end.as_ref().is_some_and(|(x, s)| *s == it.start_stop && a - x < 14.0);
                    if !joined {
                        pdf.text(a + 0.8, bar_y - 13.0, 4.8, false, &it.start_stop, INK, 0);
                    }
                    pdf.text(a + 0.8, bar_y - 8.0, 4.8, false, &format!("{:02}", it.from.rem_euclid(60)), INK, 0);
                    pdf.line((b, bar_y - 7.0), (b, bar_y), 0.3, INK);
                    pdf.text(b - 0.8, bar_y - 13.0, 4.8, false, &it.end_stop, INK, 2);
                    pdf.text(b - 0.8, bar_y - 8.0, 4.8, false, &format!("{:02}", it.to.rem_euclid(60)), INK, 2);
                    last_end = Some((b, it.end_stop.clone()));
                }
            }
            // the band under it
            for (k2, (a, b, label)) in r.under.iter().enumerate() {
                let (xa, xb) = (x_of(*a), x_of(*b));
                pdf.rect(xa, bar_y + bar_h + 1.5, (xb - xa).max(1.0), 3.0, if k2 % 2 == 0 { [118, 118, 118] } else { [228, 228, 228] });
                pdf.frame(xa, bar_y + bar_h + 1.5, (xb - xa).max(1.0), 3.0, 0.3, GREY);
                pdf.text(xa + 1.0, bar_y + bar_h + 10.5, 5.0, false, label, GREY, 0);
            }
        }
    }
    pdf.finish()
}

/// Two letters for a stop (as the bus graph writes them over the trips' ends).
pub fn stop_letters(stop: &str) -> String {
    let s: String = stop.trim().chars().filter(|c| c.is_alphanumeric()).take(2).collect();
    let mut cs = s.chars();
    match (cs.next(), cs.next()) {
        (Some(a), Some(b)) => format!("{}{}", a.to_uppercase(), b.to_lowercase()),
        (Some(a), None) => a.to_uppercase().to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str) -> GraphRow {
        let trip = |from: i32, to: i32, inbound: bool| GraphItem { from, to, kind: ItemKind::Trip { inbound }, label: "150".into(), colour: [245, 128, 40], start_stop: "Jo".into(), end_stop: "Re".into() };
        GraphRow {
            name: name.into(),
            sub: "Any vehicle".into(),
            sub2: "(239.9 km)".into(),
            items: vec![GraphItem { from: 341, to: 360, kind: ItemKind::Pull, label: String::new(), colour: [0, 0, 0], start_stop: String::new(), end_stop: String::new() }, trip(360, 415, false), trip(420, 475, true)],
            under: vec![(341, 420, "1001".into()), (420, 480, "1002".into())],
        }
    }

    #[test]
    fn a_graph_is_a_pdf_with_its_pages_and_a_table_that_points_at_its_objects() {
        let rows: Vec<GraphRow> = (0..30).map(|k| row(&format!("RB{k}"))).collect();
        let pdf = graph("Bus Graph", &["Depot: Grundorf".into(), "Service: Mon - Fri (€)".into()], "Printed: 1989-05-30 12:00", "Page", &rows);
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        if let Ok(p) = std::env::var("ROSTERFLOW_PDF_OUT") {
            std::fs::write(p, &pdf).unwrap();
        }
        // (bytes as Latin-1 chars, every one a byte: its offsets are the file's)
        let text: Vec<u8> = pdf.iter().map(|&b| if b < 0x80 { b } else { b'?' }).collect();
        let text = String::from_utf8(text).unwrap();
        // 30 rows of 46 points on A4 across: three pages
        assert!(text.contains("/Count 3"), "{}", &text[..300]);
        // every offset in the table is where its object begins
        let at = text.rfind("startxref\n").unwrap();
        let xref: usize = text[at + 10..].lines().next().unwrap().parse().unwrap();
        assert!(text[xref..].starts_with("xref"));
        for (k, l) in text[xref..].lines().skip(3).take_while(|l| l.ends_with(" n ")).enumerate() {
            let off: usize = l[..10].parse().unwrap();
            assert!(pdf[off..].starts_with(format!("{} 0 obj", k + 1).as_bytes()), "object {}", k + 1);
        }
        // the euro sign as WinAnsi's, a bracket escaped
        assert!(pdf.windows(3).any(|w| w == [0x80, b'\\', b')']));
        assert_eq!(pdf_string("a(b)"), b"(a\\(b\\))".to_vec());
        assert_eq!(stop_letters("Rennweg"), "Re");
        assert_eq!(stop_letters(" "), "");
    }
}
