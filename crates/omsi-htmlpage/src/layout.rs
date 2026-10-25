//! Block flow and wrapped inline text: turns the styled page into boxes, and finds the
//! box under a point.

use super::*;

pub(crate) struct LItem {
    pub(crate) text: String,
    pub(crate) px: f32,
    pub(crate) bold: bool,
    pub(crate) color: [u8; 4],
    pub(crate) dx: f32,
    pub(crate) w: f32,
    pub(crate) node: usize,
    pub(crate) letter: f32,
    pub(crate) underline: bool,
    pub(crate) strike: bool,
}

/// An absolutely positioned element waiting for its containing block: its node, the style of
/// its parent (what it inherits), and where it would have stood in the flow.
pub(crate) struct Pend {
    pub(crate) node: usize,
    pub(crate) parent: Style,
    pub(crate) sx: f32,
    pub(crate) sy: f32,
    pub(crate) fixed: bool,
}

pub(crate) struct LLine {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) h: f32,
    pub(crate) items: Vec<LItem>,
}

pub(crate) enum Item {
    Block(LBox),
    Line(LLine),
}

pub(crate) struct LBox {
    /// The border box.
    pub(crate) rect: [f32; 4],
    pub(crate) mb: f32,
    pub(crate) st: Style,
    pub(crate) items: Vec<Item>,
    pub(crate) node: usize,
    /// Absolutely positioned descendants whose containing block is further up.
    pub(crate) pending: Vec<Pend>,
}

/// Move a laid-out box (and everything in it).
pub(crate) fn shift(b: &mut LBox, dx: f32, dy: f32) {
    b.rect[0] += dx;
    b.rect[1] += dy;
    for p in &mut b.pending {
        p.sx += dx;
        p.sy += dy;
    }
    for it in &mut b.items {
        match it {
            Item::Block(c) => shift(c, dx, dy),
            Item::Line(l) => {
                l.x += dx;
                l.y += dy;
            }
        }
    }
}

/// How wide the content of a box wants to be, counted from `cx` (its content's left edge).
pub(crate) fn natural_width(b: &LBox, cx: f32) -> f32 {
    let mut w = 0.0f32;
    for it in &b.items {
        match it {
            Item::Line(l) => {
                if let Some(li) = l.items.last() {
                    w = w.max(li.dx + li.w);
                }
            }
            Item::Block(c) => w = w.max(c.rect[0] - cx + c.rect[2] + c.st.margin[1]),
        }
    }
    w
}

/// The element under a point: the text run or box on top, `None` outside every box.
pub(crate) fn hit(b: &LBox, x: f32, y: f32) -> Option<usize> {
    if b.st.hidden {
        return None;
    }
    for it in b.items.iter().rev() {
        match it {
            Item::Block(c) => {
                if let Some(n) = hit(c, x, y) {
                    return Some(n);
                }
            }
            Item::Line(l) => {
                if y >= l.y && y < l.y + l.h {
                    for li in &l.items {
                        let x0 = l.x + li.dx;
                        if x >= x0 && x < x0 + li.w {
                            return Some(li.node);
                        }
                    }
                }
            }
        }
    }
    let [rx, ry, rw, rh] = b.rect;
    (x >= rx && x < rx + rw && y >= ry && y < ry + rh).then_some(b.node)
}

pub(crate) struct Layouter<'a> {
    pub(crate) dom: &'a Dom,
    pub(crate) reg: &'a FontRef<'static>,
    pub(crate) bold: &'a FontRef<'static>,
    pub(crate) vw: f32,
    pub(crate) vh: f32,
    pub(crate) imgs: &'a ImageStore,
}

impl<'a> Layouter<'a> {
    pub(crate) fn tw(&self, s: &str, px: f32, bold: bool) -> f32 {
        let font = if bold { self.bold } else { self.reg };
        let sc = PxScale::from(px);
        let mut w = 0.0;
        let mut prev = None;
        for c in s.chars() {
            face_for(font, bold, c, |face, id, fi| {
                let sf = face.as_scaled(sc);
                if let Some((p, pf)) = prev {
                    if pf == fi {
                        w += sf.kern(p, id);
                    }
                }
                w += sf.h_advance(id);
                prev = Some((id, fi));
            });
        }
        w
    }

    pub(crate) fn style_of(&self, idx: usize, ps: &Style) -> Style {
        let n = &self.dom.nodes[idx];
        let mut st = ps.inherit();
        let pf = ps.font_px;
        match n.tag.as_str() {
            "b" | "strong" => {
                st.inline = true;
                st.bold = true;
            }
            "span" | "i" | "em" | "a" | "small" | "u" | "label" | "code" | "s" | "sub" | "sup" | "abbr" | "time" | "mark" | "kbd" => {
                st.inline = true;
                match n.tag.as_str() {
                    "u" => st.underline = true,
                    "s" => st.strike = true,
                    _ => {}
                }
            }
            "h1" => {
                st.font_px = pf * 2.0;
                st.bold = true;
                st.margin[0] = st.font_px * 0.67;
                st.margin[2] = st.font_px * 0.67;
            }
            "h2" => {
                st.font_px = pf * 1.5;
                st.bold = true;
                st.margin[0] = st.font_px * 0.83;
                st.margin[2] = st.font_px * 0.83;
            }
            "h3" => {
                st.font_px = pf * 1.17;
                st.bold = true;
                st.margin[0] = st.font_px;
                st.margin[2] = st.font_px;
            }
            "p" => {
                st.margin[0] = pf;
                st.margin[2] = pf;
            }
            "body" => st.margin = [8.0; 4],
            "button" => {
                st.inline_block = true;
                st.bg = [0xe0, 0xe0, 0xe0, 255];
                st.color = [0, 0, 0, 255];
                st.padding = [6.0, 12.0, 6.0, 12.0];
                st.align = 1;
                st.radius = 4.0;
            }
            "img" => st.inline_block = true,
            "head" | "style" | "script" | "title" | "meta" | "link" | "template" | "noscript" => st.none = true,
            "tr" => {
                st.flex = true;
            }
            "td" | "th" => {
                st.grow = 1.0;
                st.basis = Some(Len::Px(0.0));
                st.padding = [1.0; 4];
                if n.tag == "th" {
                    st.bold = true;
                    st.align = 1;
                }
            }
            _ => {}
        }
        if n.tag == "small" {
            st.font_px = pf * 0.83;
        }
        let mut matched: Vec<(u32, usize, &Rule)> = self
            .dom
            .rules
            .iter()
            .filter_map(|r| {
                r.sels.iter().filter(|c| self.dom.matches(idx, c)).map(specificity).max().map(|s| (s, r.order, r))
            })
            .collect();
        matched.sort_by_key(|(s, o, _)| (*s, *o));
        // the declarations in cascade order, those marked !important after the others
        let mut decls: Vec<(&str, &str, bool)> = Vec::new();
        for (_, _, r) in &matched {
            for (k, v) in &r.decls {
                decls.push((k.as_str(), v.as_str(), v.trim_end().to_ascii_lowercase().ends_with("!important")));
            }
        }
        for (k, v) in &n.inline {
            decls.push((k.as_str(), v.as_str(), v.trim_end().to_ascii_lowercase().ends_with("!important")));
        }
        // custom properties first: the declarations may name them
        if decls.iter().any(|(k, ..)| k.starts_with("--")) {
            let vars = Arc::make_mut(&mut st.vars);
            for important in [false, true] {
                for (k, v, imp) in &decls {
                    if k.starts_with("--") && *imp == important {
                        let v = v.trim().trim_end_matches("!important").trim();
                        vars.insert(k.to_string(), v.to_string());
                    }
                }
            }
        }
        for important in [false, true] {
            for (k, v, imp) in &decls {
                if *imp != important || k.starts_with("--") {
                    continue;
                }
                let mut val = std::borrow::Cow::Borrowed(*v);
                if val.contains("var(") {
                    val = std::borrow::Cow::Owned(resolve_vars(&val, &st.vars));
                }
                if val.to_ascii_lowercase().contains("currentcolor") {
                    let c = st.color;
                    val = std::borrow::Cow::Owned(val.to_ascii_lowercase().replace("currentcolor", &format!("rgba({},{},{},{})", c[0], c[1], c[2], c[3] as f32 / 255.0)));
                }
                st.apply(k, &val, pf, self.vw, self.vh);
            }
        }
        if n.tag == "img" {
            self.size_img(idx, &mut st);
        }
        st.node = idx;
        st
    }

    /// The size of an `<img>`: CSS first, then its `width`/`height` attributes, then the
    /// picture's own size; a missing side keeps the picture's proportions.
    fn size_img(&self, idx: usize, st: &mut Style) {
        let n = &self.dom.nodes[idx];
        let u = Units { font: st.font_px, vw: self.vw, vh: self.vh };
        if st.width.is_none() && !n.attr_w.is_empty() {
            st.width = parse_len(&n.attr_w, &u);
        }
        if st.height.is_none() && !n.attr_h.is_empty() {
            st.height = parse_len(&n.attr_h, &u);
        }
        let (nw, nh) = match self.imgs.dims(&n.src) {
            Some((w, h)) if !n.src.is_empty() => (w as f32, h as f32),
            _ => (0.0, 0.0),
        };
        match (st.width, st.height) {
            (None, None) => {
                st.width = Some(Len::Px(nw));
                st.height = Some(Len::Px(nh));
            }
            (Some(Len::Px(w)), None) => st.height = Some(Len::Px(if nw > 0.0 { w * nh / nw } else { 0.0 })),
            (None, Some(Len::Px(h))) => st.width = Some(Len::Px(if nh > 0.0 { h * nw / nh } else { 0.0 })),
            (Some(Len::Pct(_)), None) if nw > 0.0 => st.aspect = nh / nw,
            _ => {}
        }
    }

    pub(crate) fn inline_runs(&self, idx: usize, st: &Style, out: &mut Vec<(String, Style)>) {
        for &k in &self.dom.nodes[idx].kids {
            let kn = &self.dom.nodes[k];
            if let Some(t) = &kn.text {
                out.push((transform_text(t, st), st.clone()));
                continue;
            }
            let cs = self.style_of(k, st);
            if cs.none || cs.position >= 2 {
                continue;
            }
            if kn.tag == "br" {
                out.push(("\n".into(), cs));
            } else if cs.inline {
                self.inline_runs(k, &cs, out);
            }
        }
    }

    pub(crate) fn finish_line(&self, items: &mut Vec<LItem>, lw: &mut f32, lh: &mut f32, out: &mut Vec<LLine>, cy: &mut f32, x: f32, w: f32, align: u8) {
        if let Some(l) = items.last_mut() {
            let t = l.text.trim_end().to_string();
            let nw = self.tw(&t, l.px, l.bold) + l.letter * t.chars().count() as f32;
            *lw -= l.w - nw;
            l.w = nw;
            l.text = t;
        }
        let off = match align {
            1 => (w - *lw) / 2.0,
            2 => w - *lw,
            _ => 0.0,
        }
            .max(0.0);
        let h = *lh;
        out.push(LLine { x: x + off, y: *cy, h, items: std::mem::take(items) });
        *cy += h;
        *lw = 0.0;
        *lh = 0.0;
    }

    pub(crate) fn lines(&self, runs: &[(String, Style)], x: f32, y: f32, w: f32, align: u8) -> (Vec<LLine>, f32) {
        self.lines_e(runs, x, y, w, align, false)
    }

    /// `ellipsis`: a line that does not fit is cut and ends in "…" (`text-overflow`).
    pub(crate) fn lines_e(&self, runs: &[(String, Style)], x: f32, y: f32, w: f32, align: u8, ellipsis: bool) -> (Vec<LLine>, f32) {
        let mut out = Vec::new();
        let mut cy = y;
        let mut items: Vec<LItem> = Vec::new();
        let (mut lw, mut lh) = (0.0f32, 0.0f32);
        for (text, st) in runs {
            if text == "\n" {
                if lh == 0.0 {
                    lh = st.font_px * st.line_h;
                }
                self.finish_line(&mut items, &mut lw, &mut lh, &mut out, &mut cy, x, w, align);
                continue;
            }
            for tok in text.split_inclusive(' ') {
                let letters = st.letter_px * tok.chars().count() as f32;
                let full = self.tw(tok, st.font_px, st.bold) + letters;
                let t = tok.trim_end();
                let trimmed = if t.len() == tok.len() { full } else { self.tw(t, st.font_px, st.bold) + st.letter_px * t.chars().count() as f32 };
                if !items.is_empty() && lw + trimmed > w + 0.01 && !st.nowrap {
                    self.finish_line(&mut items, &mut lw, &mut lh, &mut out, &mut cy, x, w, align);
                }
                if items.is_empty() && tok.trim().is_empty() {
                    continue;
                }
                items.push(LItem { text: tok.to_string(), px: st.font_px, bold: st.bold, color: st.color, dx: lw, w: full, node: st.node, letter: st.letter_px, underline: st.underline, strike: st.strike });
                lw += full;
                lh = lh.max(st.font_px * st.line_h);
            }
        }
        if !items.is_empty() {
            if ellipsis && lw > w + 0.01 {
                self.ellipsize(&mut items, &mut lw, w);
            }
            self.finish_line(&mut items, &mut lw, &mut lh, &mut out, &mut cy, x, w, align);
        }
        (out, cy - y)
    }

    /// Cut the end of a line that is too long and put "…" there.
    fn ellipsize(&self, items: &mut Vec<LItem>, lw: &mut f32, w: f32) {
        let dots = items.last().map(|l| self.tw("…", l.px, l.bold)).unwrap_or(0.0);
        while let Some(l) = items.last_mut() {
            let mut chars: Vec<char> = l.text.chars().collect();
            let before = l.dx;
            while !chars.is_empty() && before + self.tw(&chars.iter().collect::<String>(), l.px, l.bold) + dots > w {
                chars.pop();
            }
            if chars.is_empty() && items.len() > 1 {
                items.pop();
                continue;
            }
            let l = items.last_mut().unwrap();
            let mut t: String = chars.into_iter().collect();
            t.push('…');
            l.w = self.tw(&t, l.px, l.bold);
            l.text = t;
            *lw = l.dx + l.w;
            break;
        }
    }

    /// How wide the content of element `idx` wants to be (content box, no wrapping): its own
    /// `width` if it has one, else the widest of its text lines and children.
    pub(crate) fn max_content(&self, idx: usize, ps: &Style) -> f32 {
        let st = self.style_of(idx, ps);
        let extra = st.padding[1] + st.padding[3] + st.bw[1] + st.bw[3];
        if let Some(l) = st.width {
            if let Len::Px(w) = l {
                return if st.border_box { (w - extra).max(0.0) } else { w };
            }
        }
        let n = &self.dom.nodes[idx];
        if n.tag == "img" {
            return match st.width { Some(Len::Px(w)) => w, _ => 0.0 };
        }
        let mut runs: Vec<(String, Style)> = Vec::new();
        let mut widest = 0.0f32;
        let mut row = 0.0f32;
        let row_flex = st.flex && st.dir < 2;
        let mut first = true;
        for &k in &n.kids {
            let kn = &self.dom.nodes[k];
            if let Some(t) = &kn.text {
                runs.push((transform_text(t, &st), st.clone()));
                continue;
            }
            let cs = self.style_of(k, &st);
            if cs.none || cs.position >= 2 {
                continue;
            }
            if kn.tag == "br" {
                runs.push(("\n".into(), cs));
                continue;
            }
            if cs.inline && !cs.flex {
                self.inline_runs(k, &cs, &mut runs);
                continue;
            }
            // a block child: the text before it is a line of its own
            widest = widest.max(self.runs_width(&runs));
            runs.clear();
            let cw = self.max_content(k, &st) + cs.margin[1] + cs.margin[3] + cs.padding[1] + cs.padding[3] + cs.bw[1] + cs.bw[3];
            let cw = if cs.border_box { cw } else { cw };
            if row_flex || cs.inline_block {
                row += cw + if first { 0.0 } else { st.gap[1] };
                first = false;
            } else {
                widest = widest.max(cw);
            }
        }
        widest = widest.max(self.runs_width(&runs)).max(row);
        widest
    }

    fn runs_width(&self, runs: &[(String, Style)]) -> f32 {
        let mut best = 0.0f32;
        let mut cur = 0.0f32;
        for (t, st) in runs {
            if t == "\n" {
                best = best.max(cur);
                cur = 0.0;
            } else {
                cur += self.tw(t, st.font_px, st.bold) + st.letter_px * t.chars().count() as f32;
            }
        }
        best.max(cur)
    }

    /// A text-only flex item: the runs laid out at width `w`.
    fn anon_box(&self, runs: &[(String, Style)], st: &Style, w: f32) -> LBox {
        let (lines, h) = self.lines(runs, 0.0, 0.0, w, st.align);
        let mut s = st.inherit();
        s.node = st.node;
        LBox { rect: [0.0, 0.0, w, h], mb: 0.0, st: s, items: lines.into_iter().map(Item::Line).collect(), node: st.node, pending: Vec::new() }
    }

    /// The children of a flex container (`display: flex`) laid out: the boxes in order, the height
    /// they use, and the absolute boxes among them. `definite_h` is the container's content height
    /// when it has one.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn flex_layout(&self, idx: usize, st: &Style, cx: f32, top: f32, content_w: f32, definite_h: Option<f32>, pct_h: f32) -> (Vec<Item>, f32, Vec<Pend>) {
        struct F {
            node: Option<usize>,
            st: Style,
            runs: Vec<(String, Style)>,
            /// border-box main size now
            main: f32,
            /// the flex base size (border-box)
            base: f32,
            box_: Option<LBox>,
        }
        let row = st.dir < 2;
        let reverse = st.dir % 2 == 1;
        let mut pend: Vec<Pend> = Vec::new();
        let mut fs: Vec<F> = Vec::new();
        let mut text: Vec<(String, Style)> = Vec::new();
        let flush_text = |text: &mut Vec<(String, Style)>, fs: &mut Vec<F>| {
            if text.iter().any(|(t, _)| !t.trim().is_empty()) {
                let mut cs = st.inherit();
                cs.node = idx;
                fs.push(F { node: None, st: cs, runs: std::mem::take(text), main: 0.0, base: 0.0, box_: None });
            }
            text.clear();
        };
        for &k in &self.dom.nodes[idx].kids {
            let kn = &self.dom.nodes[k];
            if let Some(t) = &kn.text {
                text.push((transform_text(t, st), st.clone()));
                continue;
            }
            let cs = self.style_of(k, st);
            if cs.none {
                continue;
            }
            if cs.position >= 2 {
                pend.push(Pend { node: k, parent: st.clone(), sx: cx, sy: top, fixed: cs.position == 3 });
                continue;
            }
            flush_text(&mut text, &mut fs);
            fs.push(F { node: Some(k), st: cs, runs: Vec::new(), main: 0.0, base: 0.0, box_: None });
        }
        flush_text(&mut text, &mut fs);
        fs.sort_by_key(|f| f.st.order);
        let n = fs.len();
        if n == 0 {
            return (Vec::new(), definite_h.unwrap_or(0.0).min(0.0), pend);
        }
        let (gap_main, gap_cross) = if row { (st.gap[1], st.gap[0]) } else { (st.gap[0], st.gap[1]) };
        let main_avail = if row { Some(content_w) } else { definite_h };
        let margins = |f: &F| f.st.margin;
        let extra = |f: &F| (f.st.padding[1] + f.st.padding[3] + f.st.bw[1] + f.st.bw[3], f.st.padding[0] + f.st.padding[2] + f.st.bw[0] + f.st.bw[2]);
        let stretch_w = |f: &F| -> Option<f32> {
            // a column item's width (border box) when it is stretched across the container
            let [_, mr, _, ml] = f.st.margin;
            let al = f.st.align_self.unwrap_or(st.align_items);
            if f.st.width.is_none() && al == 0 && !f.st.m_auto[1] && !f.st.m_auto[3] { Some((content_w - ml - mr).max(0.0)) } else { None }
        };
        // ---- flex base sizes
        for f in fs.iter_mut() {
            let (ew, eh) = extra(f);
            let [mt, mr, mb, ml] = margins(f);
            let border_box_of = |l: Len, base: f32, e: f32| if f.st.border_box { l.px(base) } else { l.px(base) + e };
            f.base = if row {
                match f.st.basis.or(f.st.width) {
                    Some(l) => border_box_of(l, content_w, ew),
                    None => match f.node {
                        Some(k) => self.max_content(k, st) + ew,
                        None => self.runs_width(&f.runs),
                    },
                }
            } else {
                match f.st.basis.or(f.st.height) {
                    Some(l) => border_box_of(l, definite_h.unwrap_or(0.0), eh),
                    None => {
                        // the height the item has at its width
                        let w = match f.st.width {
                            Some(l) => if f.st.border_box { l.px(content_w) } else { l.px(content_w) + ew },
                            None => stretch_w(f).unwrap_or_else(|| f.node.map_or(self.runs_width(&f.runs), |k| self.max_content(k, st) + ew)),
                        };
                        let cw = (w - ew).max(0.0);
                        match f.node {
                            Some(k) => self.build_x(k, st, 0.0, 0.0, content_w, pct_h, Some(cw), None).rect[3],
                            None => self.anon_box(&f.runs, &f.st, cw).rect[3],
                        }
                    }
                }
            };
            // min/max along the main axis
            let (min, max) = if row { (f.st.min_w, f.st.max_w) } else { (f.st.min_h, f.st.max_h) };
            let basis_for_pct = if row { content_w } else { definite_h.unwrap_or(0.0) };
            let e = if row { ew } else { eh };
            let lo = min.map_or(0.0, |l| if f.st.border_box { l.px(basis_for_pct) } else { l.px(basis_for_pct) + e });
            let hi = max.map_or(f32::INFINITY, |l| if f.st.border_box { l.px(basis_for_pct) } else { l.px(basis_for_pct) + e });
            f.base = f.base.min(hi).max(lo);
            f.main = f.base;
            let _ = (mt, mr, mb, ml);
        }
        // ---- lines
        let outer_main = |f: &F| {
            let m = margins(f);
            f.main + if row { m[1] + m[3] } else { m[0] + m[2] }
        };
        let mut lines: Vec<Vec<usize>> = Vec::new();
        let mut cur: Vec<usize> = Vec::new();
        let mut used = 0.0f32;
        for i in 0..n {
            let size = {
                let m = margins(&fs[i]);
                fs[i].base + if row { m[1] + m[3] } else { m[0] + m[2] }
            };
            if st.wrap != 0 && !cur.is_empty() && main_avail.is_some_and(|a| used + gap_main + size > a + 0.01) {
                lines.push(std::mem::take(&mut cur));
                used = 0.0;
            }
            used += size + if cur.is_empty() { 0.0 } else { gap_main };
            cur.push(i);
        }
        if !cur.is_empty() {
            lines.push(cur);
        }
        // ---- grow and shrink within each line
        for line in &lines {
            let Some(avail) = main_avail else { continue };
            let gaps = gap_main * (line.len().saturating_sub(1)) as f32;
            let total: f32 = line.iter().map(|&i| outer_main(&fs[i])).sum::<f32>() + gaps;
            let free = avail - total;
            if free > 0.0 {
                let grow: f32 = line.iter().map(|&i| fs[i].st.grow).sum();
                if grow > 0.0 {
                    for &i in line {
                        fs[i].main += free * fs[i].st.grow / grow;
                    }
                }
            } else if free < 0.0 {
                let weight: f32 = line.iter().map(|&i| fs[i].st.shrink * fs[i].base).sum();
                if weight > 0.0 {
                    for &i in line {
                        let (ew, eh) = extra(&fs[i]);
                        let cut = -free * fs[i].st.shrink * fs[i].base / weight;
                        // (an item does not shrink below its padding and border)
                        fs[i].main = (fs[i].main - cut).max(if row { ew } else { eh });
                    }
                }
            }
            // the min/max clamp after the distribution
            for &i in line {
                let f = &mut fs[i];
                let (ew, eh) = extra(f);
                let (min, max) = if row { (f.st.min_w, f.st.max_w) } else { (f.st.min_h, f.st.max_h) };
                let e = if row { ew } else { eh };
                let base = if row { content_w } else { definite_h.unwrap_or(0.0) };
                if let Some(l) = max {
                    f.main = f.main.min(if f.st.border_box { l.px(base) } else { l.px(base) + e });
                }
                if let Some(l) = min {
                    f.main = f.main.max(if f.st.border_box { l.px(base) } else { l.px(base) + e });
                }
            }
        }
        // ---- build every item at its main size
        for f in fs.iter_mut() {
            let (ew, eh) = extra(f);
            let m = margins(f);
            let b = if row {
                let cw = (f.main - ew).max(0.0);
                match f.node {
                    Some(k) => self.build_x(k, st, 0.0, 0.0, content_w, pct_h, Some(cw), None),
                    None => self.anon_box(&f.runs, &f.st, cw),
                }
            } else {
                let ch = (f.main - eh).max(0.0);
                let width = match f.st.width {
                    Some(l) => Some(if f.st.border_box { (l.px(content_w) - ew).max(0.0) } else { l.px(content_w) }),
                    None => stretch_w(f).map(|w| (w - ew).max(0.0)),
                };
                match f.node {
                    Some(k) => {
                        let w = width.unwrap_or_else(|| self.max_content(k, st).min(content_w));
                        self.build_x(k, st, 0.0, 0.0, content_w, pct_h, Some(w), Some(ch))
                    }
                    None => self.anon_box(&f.runs, &f.st, width.unwrap_or(content_w)),
                }
            };
            let _ = m;
            f.box_ = Some(b);
        }
        // ---- cross sizes of the lines
        let cross_of = |f: &F| {
            let b = f.box_.as_ref().unwrap();
            let m = margins(f);
            if row { b.rect[3] + m[0] + m[2] } else { b.rect[2] + m[1] + m[3] }
        };
        let mut line_cross: Vec<f32> = lines.iter().map(|l| l.iter().map(|&i| cross_of(&fs[i])).fold(0.0, f32::max)).collect();
        let cross_avail = if row { definite_h } else { Some(content_w) };
        if lines.len() == 1 || st.wrap == 0 {
            if let (Some(c), Some(lc)) = (cross_avail, line_cross.first_mut()) {
                *lc = c;
            }
        }
        // ---- stretch, align and place
        let total_cross: f32 = line_cross.iter().sum::<f32>() + gap_cross * (lines.len().saturating_sub(1)) as f32;
        let mut items: Vec<Item> = Vec::new();
        let mut cross_pos = 0.0f32;
        let order: Vec<usize> = if st.wrap == 2 { (0..lines.len()).rev().collect() } else { (0..lines.len()).collect() };
        for li in order {
            let line = &lines[li];
            let lc = line_cross[li];
            // main-axis free space after the items and the gaps (and auto margins)
            let total: f32 = line.iter().map(|&i| outer_main(&fs[i])).sum::<f32>() + gap_main * (line.len().saturating_sub(1)) as f32;
            let mut free = main_avail.map_or(0.0, |a| a - total).max(0.0);
            let autos: usize = line.iter().map(|&i| {
                let a = fs[i].st.m_auto;
                if row { a[1] as usize + a[3] as usize } else { a[0] as usize + a[2] as usize }
            }).sum();
            let auto_share = if autos > 0 && free > 0.0 { free / autos as f32 } else { 0.0 };
            if autos > 0 {
                free = 0.0;
            }
            // (a reversed axis begins at the other end: flex-start is the far side)
            let justify = if reverse && st.justify < 2 { 1 - st.justify } else { st.justify };
            let (start, between) = match justify {
                1 => (free, 0.0),
                2 => (free / 2.0, 0.0),
                3 if line.len() > 1 => (0.0, free / (line.len() - 1) as f32),
                4 => (free / (2.0 * line.len() as f32), free / line.len() as f32),
                5 => (free / (line.len() + 1) as f32, free / (line.len() + 1) as f32),
                _ => (0.0, 0.0),
            };
            let seq: Vec<usize> = if reverse { line.iter().rev().copied().collect() } else { line.clone() };
            let mut pos = start;
            for &i in &seq {
                let f = &mut fs[i];
                let m = f.st.margin;
                let am = f.st.m_auto;
                let (ew, eh) = extra(f);
                let align = f.st.align_self.unwrap_or(st.align_items);
                let mut b = f.box_.take().unwrap();
                // stretch the cross size of an item whose cross size is auto
                if align == 0 {
                    if row && f.st.height.is_none() {
                        let want = (lc - m[0] - m[2] - eh).max(0.0);
                        if (b.rect[3] - eh - want).abs() > 0.01 {
                            b = match f.node {
                                Some(k) => self.build_x(k, st, 0.0, 0.0, content_w, pct_h, Some((b.rect[2] - ew).max(0.0)), Some(want)),
                                None => b,
                            };
                        }
                    }
                }
                let (ml_auto, mr_auto, mt_auto, mb_auto) = (am[3], am[1], am[0], am[2]);
                let (bw_, bh_) = (b.rect[2], b.rect[3]);
                let cross_size = if row { bh_ + m[0] + m[2] } else { bw_ + m[1] + m[3] };
                let cross_free = (lc - cross_size).max(0.0);
                let cross_auto_lead = if row { mt_auto } else { ml_auto };
                let cross_auto_trail = if row { mb_auto } else { mr_auto };
                let cross_off = if cross_auto_lead && cross_auto_trail {
                    cross_free / 2.0
                } else if cross_auto_lead {
                    cross_free
                } else if cross_auto_trail {
                    0.0
                } else {
                    match align {
                        2 => cross_free,
                        3 => cross_free / 2.0,
                        _ => 0.0,
                    }
                };
                // the margins along the main axis, auto ones taking their share of the free space
                let (lead_m, trail_m) = if row {
                    (m[3] + if ml_auto { auto_share } else { 0.0 }, m[1] + if mr_auto { auto_share } else { 0.0 })
                } else {
                    (m[0] + if mt_auto { auto_share } else { 0.0 }, m[2] + if mb_auto { auto_share } else { 0.0 })
                };
                // `b` was built with its margin edge at the origin: its border box is at (ml, mt)
                let (x, y) = if row {
                    (cx + pos + lead_m, top + cross_pos + cross_off + m[0])
                } else {
                    (cx + cross_pos + cross_off + m[3], top + pos + lead_m)
                };
                let (dx, dy) = (x - b.rect[0], y - b.rect[1]);
                shift(&mut b, dx, dy);
                pos += lead_m + (if row { bw_ } else { bh_ }) + trail_m + gap_main + between;
                items.push(Item::Block(b));
            }
            cross_pos += lc + gap_cross;
        }
        let used_h = if row { definite_h.unwrap_or(total_cross) } else { definite_h.unwrap_or_else(|| fs.iter().map(|f| f.main + f.st.margin[0] + f.st.margin[2]).sum::<f32>() + gap_main * (n.saturating_sub(1)) as f32) };
        let mut out_pend = pend;
        for it in items.iter_mut() {
            if let Item::Block(b) = it {
                out_pend.append(&mut b.pending);
            }
        }
        (items, used_h, out_pend)
    }

    pub(crate) fn build(&self, idx: usize, ps: &Style, x0: f32, y0: f32, avail: f32, pct_h: f32) -> LBox {
        let mut b = self.build_x(idx, ps, x0, y0, avail, pct_h, None, None);
        // the page's root: whatever absolute boxes are left find the viewport
        if idx == self.dom.body {
            let cb = [0.0, 0.0, self.vw, self.vh];
            self.resolve_pending(&mut b, cb, true);
        }
        b
    }

    /// `force` fixes the content width (the row layout of inline blocks uses it).
    pub(crate) fn build_w(&self, idx: usize, ps: &Style, x0: f32, y0: f32, avail: f32, pct_h: f32, force: Option<f32>) -> LBox {
        self.build_x(idx, ps, x0, y0, avail, pct_h, force, None)
    }

    /// Lay out the absolutely positioned boxes waiting in `b` against the containing block `cb`
    /// (x, y, width, height of its padding box) and put them in front of or behind its content
    /// by their `z-index`. `root`: the viewport is the block, so fixed boxes land here too.
    pub(crate) fn resolve_pending(&self, b: &mut LBox, cb: [f32; 4], root: bool) {
        let all = std::mem::take(&mut b.pending);
        let mut keep = Vec::new();
        let mut placed: Vec<(i32, LBox)> = Vec::new();
        for p in all {
            if p.fixed && !root {
                keep.push(p);
                continue;
            }
            let (cbx, cby, cbw, cbh) = if p.fixed { (0.0, 0.0, self.vw, self.vh) } else { (cb[0], cb[1], cb[2], cb[3]) };
            let cs = self.style_of(p.node, &p.parent);
            let [_, mr, _, ml] = cs.margin;
            let [mt, _, mb, _] = cs.margin;
            let extra_w = cs.padding[1] + cs.padding[3] + cs.bw[1] + cs.bw[3];
            let extra_h = cs.padding[0] + cs.padding[2] + cs.bw[0] + cs.bw[2];
            let [top, right, bottom, left] = cs.inset;
            let content = |l: f32, extra: f32| if cs.border_box { (l - extra).max(0.0) } else { l };
            let mut force_w = cs.width.map(|l| content(l.px(cbw), extra_w));
            if force_w.is_none() {
                force_w = match (left, right) {
                    (Some(l), Some(r)) => Some((cbw - l.px(cbw) - r.px(cbw) - ml - mr - extra_w).max(0.0)),
                    _ => Some(self.max_content(p.node, &p.parent).min((cbw - ml - mr - extra_w).max(0.0))),
                };
            }
            let mut force_h = cs.height.map(|l| content(l.px(cbh), extra_h));
            if force_h.is_none() {
                if let (Some(t), Some(bo)) = (top, bottom) {
                    force_h = Some((cbh - t.px(cbh) - bo.px(cbh) - mt - mb - extra_h).max(0.0));
                }
            }
            let mut bx = self.build_x(p.node, &p.parent, 0.0, 0.0, cbw, cbh, force_w, force_h);
            let (ow, oh) = (bx.rect[2], bx.rect[3]);
            let x = match (left, right) {
                (Some(l), _) => cbx + l.px(cbw) + ml,
                (None, Some(r)) => cbx + cbw - r.px(cbw) - mr - ow,
                _ => p.sx + ml,
            };
            let y = match (top, bottom) {
                (Some(t), _) => cby + t.px(cbh) + mt,
                (None, Some(bo)) => cby + cbh - bo.px(cbh) - mb - oh,
                _ => p.sy + mt,
            };
            let (dx, dy) = (x - bx.rect[0], y - bx.rect[1]);
            shift(&mut bx, dx, dy);
            // its own absolute descendants have this box (or none) as their block
            keep.extend(std::mem::take(&mut bx.pending));
            placed.push((cs.z, bx));
        }
        b.pending = keep;
        placed.sort_by_key(|(z, _)| *z);
        let (below, above): (Vec<_>, Vec<_>) = placed.into_iter().partition(|(z, _)| *z < 0);
        let mut items: Vec<Item> = below.into_iter().map(|(_, c)| Item::Block(c)).collect();
        items.append(&mut b.items);
        items.extend(above.into_iter().map(|(_, c)| Item::Block(c)));
        b.items = items;
    }

    /// The box of element `idx`. `force_w` / `force_h` fix the content width / height (the row
    /// layout of inline blocks, flex items and absolute boxes use them).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn build_x(&self, idx: usize, ps: &Style, x0: f32, y0: f32, avail: f32, pct_h: f32, force: Option<f32>, force_h: Option<f32>) -> LBox {
        let mut st = self.style_of(idx, ps);
        let [mt, mr, mb, ml] = st.margin;
        let [pt, pr, pb, pl] = st.padding;
        let [bt, br, bb, bl] = st.bw;
        let extra_w = pl + pr + bl + br;
        let extra_h = pt + pb + bt + bb;
        if force.is_none() && st.inline_block && st.width.is_none() {
            // shrink to fit: lay it out at full width, then narrow it to what the content used
            let auto = (avail - ml - mr - extra_w).max(0.0);
            let probe = self.build_x(idx, ps, x0, y0, avail, pct_h, Some(auto), force_h);
            let nat = natural_width(&probe, x0 + ml + bl + pl);
            let nat = if st.flex { nat.max(self.max_content(idx, ps)).min(auto) } else { nat };
            return self.build_x(idx, ps, x0, y0, avail, pct_h, Some(nat.ceil().min(auto)), force_h);
        }
        // the content width: forced, or the style's, clamped by min/max, or what is left
        let sized = |l: Len, base: f32, extra: f32| {
            let v = l.px(base);
            if st.border_box { (v - extra).max(0.0) } else { v.max(0.0) }
        };
        let clamp_w = |w: f32| {
            let lo = st.min_w.map_or(0.0, |l| sized(l, avail, extra_w));
            let hi = st.max_w.map_or(f32::INFINITY, |l| sized(l, avail, extra_w));
            w.min(hi).max(lo)
        };
        let content_w = match (force, st.width) {
            (Some(cw), _) => cw,
            (None, Some(l)) => clamp_w(sized(l, avail, extra_w)),
            (None, None) => clamp_w(((avail - ml - mr).max(0.0) - extra_w).max(0.0)),
        };
        let outer_w = content_w + extra_w;
        let width_is_set = force.is_some() || st.width.is_some() || st.max_w.is_some();
        let free_w = (avail - outer_w - ml - mr).max(0.0);
        let bx = if width_is_set && (st.m_auto[1] || st.m_auto[3]) {
            match (st.m_auto[3], st.m_auto[1]) {
                (true, true) => x0 + free_w / 2.0,
                (true, false) => x0 + free_w + ml,
                _ => x0 + ml,
            }
        } else {
            x0 + ml
        };
        let by = y0 + mt;
        let cx = bx + bl + pl;
        let top = by + bt + pt;
        let mut cy = top;
        let resolved_h = |l: Len, base: f32| if st.border_box { (l.px(base) - extra_h).max(0.0) } else { l.px(base).max(0.0) };
        let child_pct_h = match st.height {
            Some(l) => resolved_h(l, pct_h),
            None => pct_h,
        };
        let mut items = Vec::new();
        let mut pending: Vec<Pend> = Vec::new();
        let is_container = st.position != 0 || idx == self.dom.body;
        if st.flex {
            let definite = force_h.or(st.height.map(|l| resolved_h(l, pct_h)));
            let (list, used, pend) = self.flex_layout(idx, &st, cx, top, content_w, definite, child_pct_h);
            items = list;
            pending = pend;
            cy = top + used;
        } else {
            let mut runs: Vec<(String, Style)> = Vec::new();
            let ellipsis = st.ellipsis && st.nowrap;
            let flush = |runs: &mut Vec<(String, Style)>, cy: &mut f32, items: &mut Vec<Item>| {
                if runs.iter().any(|(t, _)| !t.trim().is_empty() || t == "\n") {
                    let (lines, h) = self.lines_e(runs, cx, *cy, content_w, st.align, ellipsis);
                    *cy += h;
                    items.extend(lines.into_iter().map(Item::Line));
                }
                runs.clear();
            };
            // consecutive `inline-block` children are laid out in rows, wrapping at the content width
            let mut group: Vec<usize> = Vec::new();
            let mut pend_cell: Vec<Pend> = Vec::new();
            let place = |group: &mut Vec<usize>, cy: &mut f32, items: &mut Vec<Item>, pend: &mut Vec<Pend>| {
                if group.is_empty() {
                    return;
                }
                let mut boxes: Vec<LBox> = group.drain(..).map(|k| self.build_x(k, &st, 0.0, 0.0, content_w, child_pct_h, None, None)).collect();
                let occupied = |b: &LBox| b.st.margin[3] + b.rect[2] + b.st.margin[1];
                let mut i = 0;
                while i < boxes.len() {
                    let (mut j, mut rw) = (i, 0.0f32);
                    while j < boxes.len() {
                        let ow = occupied(&boxes[j]) + if j > i { st.gap[1] } else { 0.0 };
                        if j > i && rw + ow > content_w + 0.01 && !st.nowrap {
                            break;
                        }
                        rw += ow;
                        j += 1;
                    }
                    let off = match st.align {
                        1 => (content_w - rw) / 2.0,
                        2 => content_w - rw,
                        _ => 0.0,
                    }
                        .max(0.0);
                    let rh = boxes[i..j].iter().map(|b| b.st.margin[0] + b.rect[3] + b.st.margin[2]).fold(0.0, f32::max);
                    let mut x = cx + off;
                    for b in &mut boxes[i..j] {
                        let ow = occupied(b);
                        // (vertical-align: middle centres a box in its row)
                        let dy = if b.st.valign == 1 { (rh - (b.st.margin[0] + b.rect[3] + b.st.margin[2])) / 2.0 } else { 0.0 };
                        shift(b, x, *cy + dy);
                        x += ow + st.gap[1];
                    }
                    *cy += rh + if j < boxes.len() { st.gap[0] } else { 0.0 };
                    i = j;
                }
                for mut b in boxes {
                    pend.append(&mut b.pending);
                    items.push(Item::Block(b));
                }
            };
            for &k in &self.dom.nodes[idx].kids {
                let kn = &self.dom.nodes[k];
                if let Some(t) = &kn.text {
                    place(&mut group, &mut cy, &mut items, &mut pend_cell);
                    runs.push((transform_text(t, &st), st.clone()));
                    continue;
                }
                let cs = self.style_of(k, &st);
                if cs.none {
                    continue;
                }
                if cs.position >= 2 {
                    // out of the flow: it waits for its containing block, remembering where it stood
                    pending.push(Pend { node: k, parent: st.clone(), sx: cx, sy: cy, fixed: cs.position == 3 });
                    continue;
                }
                if kn.tag == "br" {
                    place(&mut group, &mut cy, &mut items, &mut pend_cell);
                    runs.push(("\n".into(), cs));
                    continue;
                }
                if cs.inline_block {
                    flush(&mut runs, &mut cy, &mut items);
                    group.push(k);
                    continue;
                }
                if cs.inline {
                    place(&mut group, &mut cy, &mut items, &mut pend_cell);
                    self.inline_runs(k, &cs, &mut runs);
                    continue;
                }
                place(&mut group, &mut cy, &mut items, &mut pend_cell);
                flush(&mut runs, &mut cy, &mut items);
                let mut child = self.build_x(k, &st, cx, cy, content_w, child_pct_h, None, None);
                cy = child.rect[1] + child.rect[3] + child.mb;
                pend_cell.append(&mut child.pending);
                items.push(Item::Block(child));
            }
            place(&mut group, &mut cy, &mut items, &mut pend_cell);
            flush(&mut runs, &mut cy, &mut items);
            pending.append(&mut pend_cell);
        }
        let flow_h = cy - top;
        let mut content_h = match (force_h, st.height) {
            (Some(h), _) => h,
            (None, Some(l)) => resolved_h(l, pct_h),
            (None, None) if st.aspect > 0.0 => content_w * st.aspect,
            (None, None) => flow_h,
        };
        if force_h.is_none() {
            let lo = st.min_h.map_or(0.0, |l| resolved_h(l, pct_h));
            let hi = st.max_h.map_or(f32::INFINITY, |l| resolved_h(l, pct_h));
            content_h = content_h.min(hi).max(lo);
        }
        // a button with a fixed height keeps its label in the middle
        if self.dom.nodes[idx].tag == "button" && (st.height.is_some() || force_h.is_some()) {
            let dy = ((content_h - flow_h) / 2.0).max(0.0);
            if dy > 0.0 {
                for it in &mut items {
                    match it {
                        Item::Block(c) => shift(c, 0.0, dy),
                        Item::Line(l) => l.y += dy,
                    }
                }
            }
        }
        let h = content_h + extra_h;
        if st.radius_pct {
            st.radius = st.radius / 100.0 * outer_w.min(h);
            st.radius_pct = false;
        }
        let mut b = LBox { rect: [bx, by, outer_w, h], mb, st: st.clone(), items, node: idx, pending };
        if is_container {
            let cb = [bx + bl, by + bt, outer_w - bl - br, h - bt - bb];
            self.resolve_pending(&mut b, cb, false);
        }
        // `position: relative` moves the box after it is laid out, its descendants with it
        if st.position == 1 {
            let dx = match (st.inset[3], st.inset[1]) {
                (Some(l), _) => l.px(avail),
                (None, Some(r)) => -r.px(avail),
                _ => 0.0,
            };
            let dy = match (st.inset[0], st.inset[2]) {
                (Some(t), _) => t.px(pct_h),
                (None, Some(bo)) => -bo.px(pct_h),
                _ => 0.0,
            };
            if dx != 0.0 || dy != 0.0 {
                shift(&mut b, dx, dy);
            }
        }
        b
    }
}

/// `text-transform` applied to a run of text.
pub(crate) fn transform_text(t: &str, st: &Style) -> String {
    match st.transform {
        1 => t.to_uppercase(),
        2 => t.to_lowercase(),
        3 => {
            let mut out = String::with_capacity(t.len());
            let mut start = true;
            for c in t.chars() {
                if start && c.is_alphabetic() {
                    out.extend(c.to_uppercase());
                } else {
                    out.push(c);
                }
                start = c.is_whitespace();
            }
            out
        }
        _ => t.to_string(),
    }
}
