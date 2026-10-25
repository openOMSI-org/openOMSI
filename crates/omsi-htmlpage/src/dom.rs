//! The page: HTML parsing into a flat node list, entities, `<style>` collection and the
//! CSS parser that turns style sheets into rules.

#[derive(Debug, Clone, Default)]
pub(crate) struct Node {
    pub(crate) tag: String,
    pub(crate) id: String,
    pub(crate) classes: Vec<String>,
    pub(crate) inline: Vec<(String, String)>,
    pub(crate) text: Option<String>,
    pub(crate) kids: Vec<usize>,
    pub(crate) parent: Option<usize>,
    /// Inline event attributes: `onclick="..."` is stored as `("click", "...")`.
    pub(crate) on: Vec<(String, String)>,
    /// `src` of an `<img>`.
    pub(crate) src: String,
    /// `width` / `height` attributes of an `<img>` (`"64"`, `"50%"`), empty when absent.
    pub(crate) attr_w: String,
    pub(crate) attr_h: String,
    /// Every attribute as written (lower-case names), for attribute selectors and
    /// `getAttribute`.
    pub(crate) attrs: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum AttrOp {
    Exists,
    Eq,
    Prefix,
    Suffix,
    Contains,
    Word,
    Dash,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AttrSel {
    pub(crate) name: String,
    pub(crate) op: AttrOp,
    pub(crate) value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Pseudo {
    /// `:nth-child(an+b)` and its relatives (`:first-child` is 0n+1, `:last-child` counts from
    /// the end); `of_type` counts only siblings of the same tag.
    Nth { a: i32, b: i32, from_last: bool, of_type: bool },
    OnlyChild,
    Empty,
    Root,
    Not(Box<Simple>),
    /// `:hover`, `:focus`, `::before` ...: states and parts a texture never has.
    Never,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Simple {
    pub(crate) tag: Option<String>,
    pub(crate) id: Option<String>,
    pub(crate) classes: Vec<String>,
    pub(crate) attrs: Vec<AttrSel>,
    pub(crate) pseudos: Vec<Pseudo>,
}

/// How a compound selector is joined to the one before it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) enum Comb {
    /// A space.
    #[default]
    Descendant,
    /// `>`
    Child,
    /// `+`
    Adjacent,
    /// `~`
    Sibling,
}

/// A complex selector, left to right; the first entry's combinator is not used.
pub(crate) type Chain = Vec<(Comb, Simple)>;

#[derive(Debug, Clone)]
pub(crate) struct Rule {
    pub(crate) sels: Vec<Chain>,
    pub(crate) decls: Vec<(String, String)>,
    pub(crate) order: usize,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ScriptMeta {
    pub(crate) run: bool,
    pub(crate) module: bool,
    /// The file the text came from (`data-omsi-src`, put there where an external script was
    /// inlined), what a module's imports are relative to.
    pub(crate) path: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Dom {
    pub(crate) nodes: Vec<Node>,
    pub(crate) rules: Vec<Rule>,
    pub(crate) scripts: Vec<String>,
    /// Per script: whether to run it, whether it is a module, and the file it came from.
    pub(crate) script_meta: Vec<ScriptMeta>,
    pub(crate) body: usize,
    /// Counts every visible change of the page; layout and frame caches key on it.
    pub(crate) generation: u64,
}

pub(crate) const VOID: &[&str] = &["br", "img", "hr", "meta", "link", "input", "area", "base", "col", "source", "wbr"];

pub(crate) fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p..];
        if let Some(e) = rest.find(';').filter(|e| *e <= 8) {
            let name = &rest[1..e];
            let rep = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some('\u{a0}'),
                _ => {
                    if let Some(h) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                        u32::from_str_radix(h, 16).ok().and_then(char::from_u32)
                    } else if let Some(d) = name.strip_prefix('#') {
                        d.parse::<u32>().ok().and_then(char::from_u32)
                    } else {
                        None
                    }
                }
            };
            if let Some(c) = rep {
                out.push(c);
                rest = &rest[e + 1..];
                continue;
            }
        }
        out.push('&');
        rest = &rest[1..];
    }
    out.push_str(rest);
    out
}

pub(crate) fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = false;
    for c in s.chars() {
        if c.is_whitespace() && c != '\u{a0}' {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(c);
            last_space = false;
        }
    }
    out
}

pub(crate) fn parse_style_attr(s: &str) -> Vec<(String, String)> {
    s.split(';')
        .filter_map(|d| {
            let (k, v) = d.split_once(':')?;
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim().to_string());
            if k.is_empty() {
                None
            } else {
                Some((k, v))
            }
        })
        .collect()
}

impl Dom {
    pub(crate) fn parse(html: &str) -> Dom {
        let mut dom = Dom::default();
        dom.nodes.push(Node { tag: "#root".into(), ..Node::default() });
        let mut stack: Vec<usize> = vec![0];
        let mut css = String::new();
        let b = html.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'<' {
                let rest = &html[i..];
                if rest.starts_with("<!--") {
                    i += rest.find("-->").map(|e| e + 3).unwrap_or(rest.len());
                    continue;
                }
                if rest.starts_with("<!") || rest.starts_with("<?") {
                    i += rest.find('>').map(|e| e + 1).unwrap_or(rest.len());
                    continue;
                }
                if let Some(close) = rest.strip_prefix("</") {
                    let end = close.find('>').unwrap_or(close.len());
                    let name = close[..end].trim().to_ascii_lowercase();
                    if let Some(pos) = stack.iter().rposition(|&n| dom.nodes[n].tag == name) {
                        if pos > 0 {
                            stack.truncate(pos);
                        }
                    }
                    i += 2 + end + 1;
                    continue;
                }
                // an opening tag: find its end, honouring quotes
                let mut j = i + 1;
                let mut quote = 0u8;
                while j < b.len() {
                    let c = b[j];
                    if quote != 0 {
                        if c == quote {
                            quote = 0;
                        }
                    } else if c == b'"' || c == b'\'' {
                        quote = c;
                    } else if c == b'>' {
                        break;
                    }
                    j += 1;
                }
                let inner = &html[i + 1..j.min(html.len())];
                i = (j + 1).min(html.len());
                let self_closing = inner.ends_with('/');
                let inner = inner.trim_end_matches('/');
                let (name, attrs) = match inner.find(|c: char| c.is_whitespace()) {
                    Some(p) => (&inner[..p], &inner[p..]),
                    None => (inner, ""),
                };
                let name = name.to_ascii_lowercase();
                if name.is_empty() || !name.chars().next().map_or(false, |c| c.is_ascii_alphabetic()) {
                    continue;
                }
                let mut node = Node { tag: name.clone(), parent: stack.last().copied(), ..Node::default() };
                for (k, v) in parse_attrs(attrs) {
                    node.attrs.push((k.clone(), v.clone()));
                    match k.as_str() {
                        "id" => node.id = v,
                        "class" => node.classes = v.split_whitespace().map(str::to_string).collect(),
                        "style" => node.inline = parse_style_attr(&v),
                        "src" => node.src = v,
                        "width" => node.attr_w = v,
                        "height" => node.attr_h = v,
                        e if e.len() > 2 && e.starts_with("on") => node.on.push((e[2..].to_string(), v)),
                        _ => {}
                    }
                }
                let idx = dom.nodes.len();
                let parent = *stack.last().unwrap();
                dom.nodes.push(node);
                dom.nodes[parent].kids.push(idx);
                if name == "style" || name == "script" {
                    let end_tag = format!("</{}", name);
                    let lower = html[i..].to_ascii_lowercase();
                    let end = lower.find(&end_tag).unwrap_or(lower.len());
                    let body = html[i..i + end].to_string();
                    if name == "style" {
                        css.push_str(&body);
                        css.push('\n');
                    } else {
                        let n = &dom.nodes[idx];
                        let attr = |k: &str| n.attrs.iter().find(|(a, _)| a == k).map(|(_, v)| v.to_ascii_lowercase());
                        let ty = attr("type").unwrap_or_default();
                        let module = ty == "module";
                        let run = ty.is_empty() || module || ty.contains("javascript") || ty.contains("ecmascript");
                        let path = n.attrs.iter().find(|(a, _)| a == "data-omsi-src").map(|(_, v)| v.clone());
                        dom.script_meta.push(ScriptMeta { run, module, path });
                        dom.scripts.push(body);
                    }
                    i += end;
                    i += html[i..].find('>').map(|e| e + 1).unwrap_or(html.len() - i);
                    continue;
                }
                if !self_closing && !VOID.contains(&name.as_str()) {
                    stack.push(idx);
                }
            } else {
                let end = html[i..].find('<').map(|e| i + e).unwrap_or(html.len());
                let raw = decode_entities(&html[i..end]);
                i = end;
                let text = collapse_ws(&raw);
                if text.trim().is_empty() {
                    continue;
                }
                let parent = *stack.last().unwrap();
                let idx = dom.nodes.len();
                dom.nodes.push(Node { tag: "#text".into(), text: Some(text), parent: Some(parent), ..Node::default() });
                dom.nodes[parent].kids.push(idx);
            }
        }
        dom.rules = parse_css(&css);
        dom.body = dom.nodes.iter().position(|n| n.tag == "body").unwrap_or(0);
        dom
    }

    pub(crate) fn text_of(&self, idx: usize) -> String {
        let n = &self.nodes[idx];
        if let Some(t) = &n.text {
            return t.clone();
        }
        n.kids.iter().map(|&k| self.text_of(k)).collect::<Vec<_>>().join("")
    }

    pub(crate) fn set_text(&mut self, idx: usize, s: String) {
        if let [k] = self.nodes[idx].kids[..] {
            if self.nodes[k].tag == "#text" {
                if self.nodes[k].text.as_deref() != Some(s.as_str()) {
                    self.nodes[k].text = Some(s);
                    self.generation += 1;
                }
                return;
            }
        }
        let t = self.nodes.len();
        self.nodes.push(Node { tag: "#text".into(), text: Some(s), parent: Some(idx), ..Node::default() });
        self.nodes[idx].kids = vec![t];
        self.generation += 1;
    }

    /// A new element that hangs nowhere yet (`document.createElement`).
    pub(crate) fn create(&mut self, tag: &str) -> usize {
        self.nodes.push(Node { tag: tag.to_string(), ..Node::default() });
        self.nodes.len() - 1
    }

    pub(crate) fn detach(&mut self, idx: usize) {
        if let Some(p) = self.nodes[idx].parent.take() {
            self.nodes[p].kids.retain(|&k| k != idx);
            self.generation += 1;
        }
    }

    /// Is the node part of the page (under the root), not a detached piece?
    pub(crate) fn is_attached(&self, idx: usize) -> bool {
        self.is_inside(idx, 0)
    }

    pub(crate) fn is_inside(&self, mut idx: usize, ancestor: usize) -> bool {
        loop {
            if idx == ancestor {
                return true;
            }
            match self.nodes[idx].parent {
                Some(p) => idx = p,
                None => return false,
            }
        }
    }

    /// `parent.appendChild(child)`; moving an element into itself or its own subtree is ignored.
    pub(crate) fn append(&mut self, parent: usize, child: usize) {
        if child == 0 || self.is_inside(parent, child) {
            return;
        }
        self.detach(child);
        self.nodes[child].parent = Some(parent);
        self.nodes[parent].kids.push(child);
        self.generation += 1;
    }

    pub(crate) fn copy_from(&mut self, src: &Dom, s: usize, parent: usize) {
        let mut n = src.nodes[s].clone();
        n.parent = Some(parent);
        n.kids.clear();
        let idx = self.nodes.len();
        self.nodes.push(n);
        self.nodes[parent].kids.push(idx);
        for &k in &src.nodes[s].kids {
            self.copy_from(src, k, idx);
        }
    }

    /// `element.innerHTML = markup`: the children are replaced by the parsed markup.
    /// (`<style>` and `<script>` in it are not run.)
    pub(crate) fn graft(&mut self, parent: usize, html: &str) {
        let frag = Dom::parse(&format!("<body>{html}</body>"));
        self.nodes[parent].kids.clear();
        for &k in &frag.nodes[frag.body].kids {
            self.copy_from(&frag, k, parent);
        }
        self.generation += 1;
    }

    pub(crate) fn by_id(&self, id: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.text.is_none() && !n.id.is_empty() && n.id == id)
    }

    pub(crate) fn matches(&self, idx: usize, chain: &Chain) -> bool {
        let Some(((_, last), _)) = chain.split_last() else { return false };
        self.matches_from(idx, chain, chain.len() - 1, last)
    }

    /// Does `idx` match `chain[at]` (`simple`) and, to its left, the rest of the chain?
    fn matches_from(&self, idx: usize, chain: &Chain, at: usize, simple: &Simple) -> bool {
        if !self.matches_simple(idx, simple) {
            return false;
        }
        if at == 0 {
            return true;
        }
        let comb = chain[at].0;
        let (_, left) = &chain[at - 1];
        match comb {
            Comb::Descendant => {
                let mut cur = self.nodes[idx].parent;
                while let Some(p) = cur {
                    if self.matches_from(p, chain, at - 1, left) {
                        return true;
                    }
                    cur = self.nodes[p].parent;
                }
                false
            }
            Comb::Child => self.nodes[idx].parent.is_some_and(|p| self.matches_from(p, chain, at - 1, left)),
            Comb::Adjacent => self.prev_element(idx).is_some_and(|p| self.matches_from(p, chain, at - 1, left)),
            Comb::Sibling => {
                let mut cur = self.prev_element(idx);
                while let Some(p) = cur {
                    if self.matches_from(p, chain, at - 1, left) {
                        return true;
                    }
                    cur = self.prev_element(p);
                }
                false
            }
        }
    }

    /// The element before `idx` among its parent's children.
    pub(crate) fn prev_element(&self, idx: usize) -> Option<usize> {
        let p = self.nodes[idx].parent?;
        let kids = &self.nodes[p].kids;
        let at = kids.iter().position(|&k| k == idx)?;
        kids[..at].iter().rev().copied().find(|&k| self.nodes[k].text.is_none())
    }

    /// The element children of `idx`.
    pub(crate) fn element_kids(&self, idx: usize) -> impl Iterator<Item = usize> + '_ {
        self.nodes[idx].kids.iter().copied().filter(|&k| self.nodes[k].text.is_none())
    }

    pub(crate) fn matches_simple(&self, idx: usize, s: &Simple) -> bool {
        let n = &self.nodes[idx];
        if n.text.is_some() {
            return false;
        }
        s.tag.as_ref().map_or(true, |t| *t == n.tag)
            && s.id.as_ref().map_or(true, |i| *i == n.id)
            && s.classes.iter().all(|c| n.classes.contains(c))
            && s.attrs.iter().all(|a| self.matches_attr(idx, a))
            && s.pseudos.iter().all(|p| self.matches_pseudo(idx, p))
    }

    /// The value of attribute `name` of an element (lower-case name); `None` when it has none.
    /// The ones the engine keeps in fields of their own are read from there, so a script's
    /// changes show.
    pub(crate) fn attr(&self, idx: usize, name: &str) -> Option<String> {
        let n = &self.nodes[idx];
        match name {
            "id" => (!n.id.is_empty()).then(|| n.id.clone()),
            "class" => (!n.classes.is_empty()).then(|| n.classes.join(" ")),
            "style" => (!n.inline.is_empty()).then(|| style_text(&n.inline)),
            "src" => (!n.src.is_empty()).then(|| n.src.clone()),
            "width" => (!n.attr_w.is_empty()).then(|| n.attr_w.clone()),
            "height" => (!n.attr_h.is_empty()).then(|| n.attr_h.clone()),
            on if on.len() > 2 && on.starts_with("on") => n.on.iter().find(|(k, _)| k == &on[2..]).map(|(_, v)| v.clone()),
            _ => n.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()),
        }
    }

    /// Set attribute `name` (lower case) of an element.
    pub(crate) fn set_attr(&mut self, idx: usize, name: &str, val: &str) {
        if self.attr(idx, name).as_deref() == Some(val) {
            return;
        }
        let n = &mut self.nodes[idx];
        match name {
            "id" => n.id = val.to_string(),
            "class" => n.classes = val.split_whitespace().map(str::to_string).collect(),
            "style" => n.inline = parse_style_attr(val),
            "src" => n.src = val.to_string(),
            "width" => n.attr_w = val.to_string(),
            "height" => n.attr_h = val.to_string(),
            on if on.len() > 2 && on.starts_with("on") => {
                let ty = on[2..].to_string();
                n.on.retain(|(k, _)| *k != ty);
                n.on.push((ty, val.to_string()));
            }
            _ => {}
        }
        match n.attrs.iter_mut().find(|(k, _)| k == name) {
            Some(slot) => slot.1 = val.to_string(),
            None => n.attrs.push((name.to_string(), val.to_string())),
        }
        self.generation += 1;
    }

    pub(crate) fn remove_attr(&mut self, idx: usize, name: &str) {
        let n = &mut self.nodes[idx];
        match name {
            "id" => n.id.clear(),
            "class" => n.classes.clear(),
            "style" => n.inline.clear(),
            "src" => n.src.clear(),
            "width" => n.attr_w.clear(),
            "height" => n.attr_h.clear(),
            on if on.len() > 2 && on.starts_with("on") => n.on.retain(|(k, _)| *k != on[2..]),
            _ => {}
        }
        n.attrs.retain(|(k, _)| k != name);
        self.generation += 1;
    }

    /// The names of an element's attributes.
    pub(crate) fn attr_names(&self, idx: usize) -> Vec<String> {
        let n = &self.nodes[idx];
        let mut out: Vec<String> = n.attrs.iter().map(|(k, _)| k.clone()).collect();
        for (k, present) in [("id", !n.id.is_empty()), ("class", !n.classes.is_empty()), ("style", !n.inline.is_empty()), ("src", !n.src.is_empty())] {
            if present && !out.iter().any(|o| o == k) {
                out.push(k.to_string());
            }
        }
        out.retain(|k| self.attr(idx, k).is_some());
        out
    }

    pub(crate) fn create_text(&mut self, text: &str) -> usize {
        self.nodes.push(Node { tag: "#text".into(), text: Some(text.to_string()), ..Node::default() });
        self.nodes.len() - 1
    }

    /// `parent.insertBefore(child, before)`; `before` None appends. A node cannot go into itself.
    pub(crate) fn insert_before(&mut self, parent: usize, child: usize, before: Option<usize>) {
        if child == 0 || self.is_inside(parent, child) {
            return;
        }
        self.detach(child);
        self.nodes[child].parent = Some(parent);
        let at = before.and_then(|b| self.nodes[parent].kids.iter().position(|&k| k == b));
        match at {
            Some(p) => self.nodes[parent].kids.insert(p, child),
            None => self.nodes[parent].kids.push(child),
        }
        self.generation += 1;
    }

    /// The markup of an element's children.
    pub(crate) fn inner_html(&self, idx: usize) -> String {
        let mut out = String::new();
        for &k in &self.nodes[idx].kids {
            self.write_html(k, &mut out);
        }
        out
    }

    pub(crate) fn outer_html(&self, idx: usize) -> String {
        let mut out = String::new();
        self.write_html(idx, &mut out);
        out
    }

    fn write_html(&self, idx: usize, out: &mut String) {
        let n = &self.nodes[idx];
        if let Some(t) = &n.text {
            out.push_str(&t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;"));
            return;
        }
        out.push('<');
        out.push_str(&n.tag);
        for name in self.attr_names(idx) {
            if let Some(v) = self.attr(idx, &name) {
                out.push_str(&format!(" {name}=\"{}\"", v.replace('&', "&amp;").replace('"', "&quot;")));
            }
        }
        out.push('>');
        if VOID.contains(&n.tag.as_str()) {
            return;
        }
        for &k in &n.kids {
            self.write_html(k, out);
        }
        out.push_str(&format!("</{}>", n.tag));
    }

    /// A copy of `idx` (and its subtree when `deep`), hanging nowhere.
    pub(crate) fn clone_node(&mut self, idx: usize, deep: bool) -> usize {
        let mut n = self.nodes[idx].clone();
        n.parent = None;
        n.kids.clear();
        let copy = self.nodes.len();
        self.nodes.push(n);
        if deep {
            for k in self.nodes[idx].kids.clone() {
                let kc = self.clone_node(k, true);
                self.nodes[kc].parent = Some(copy);
                self.nodes[copy].kids.push(kc);
            }
        }
        copy
    }

    /// Every element below `idx` (not `idx` itself), in document order.
    pub(crate) fn descendants(&self, idx: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut stack: Vec<usize> = self.nodes[idx].kids.iter().rev().copied().collect();
        while let Some(k) = stack.pop() {
            if self.nodes[k].text.is_none() {
                out.push(k);
            }
            stack.extend(self.nodes[k].kids.iter().rev().copied());
        }
        out
    }

    fn matches_attr(&self, idx: usize, a: &AttrSel) -> bool {
        let Some(v) = self.attr(idx, &a.name) else { return false };
        match a.op {
            AttrOp::Exists => true,
            AttrOp::Eq => v == a.value,
            AttrOp::Prefix => !a.value.is_empty() && v.starts_with(&a.value),
            AttrOp::Suffix => !a.value.is_empty() && v.ends_with(&a.value),
            AttrOp::Contains => !a.value.is_empty() && v.contains(&a.value),
            AttrOp::Word => v.split_whitespace().any(|w| w == a.value),
            AttrOp::Dash => v == a.value || v.starts_with(&format!("{}-", a.value)),
        }
    }

    fn matches_pseudo(&self, idx: usize, p: &Pseudo) -> bool {
        let n = &self.nodes[idx];
        match p {
            Pseudo::Never => false,
            Pseudo::Root => n.parent == Some(0) || n.parent.is_none(),
            Pseudo::Empty => n.kids.iter().all(|&k| self.nodes[k].text.as_deref().is_some_and(|t| t.is_empty())),
            Pseudo::Not(inner) => !self.matches_simple(idx, inner),
            Pseudo::OnlyChild => n.parent.is_some_and(|par| self.element_kids(par).count() == 1),
            Pseudo::Nth { a, b, from_last, of_type } => {
                let Some(par) = n.parent else { return false };
                let sibs: Vec<usize> = self.element_kids(par).filter(|&k| !of_type || self.nodes[k].tag == n.tag).collect();
                let Some(pos) = sibs.iter().position(|&k| k == idx) else { return false };
                let i = if *from_last { sibs.len() - pos } else { pos + 1 } as i32;
                // i = a*k + b for some k >= 0
                if *a == 0 {
                    i == *b
                } else {
                    (i - b) % a == 0 && (i - b) / a >= 0
                }
            }
        }
    }

    pub(crate) fn query(&self, sel: &str) -> Option<usize> {
        let chains = parse_selector_list(sel);
        (0..self.nodes.len()).find(|&i| chains.iter().any(|c| self.matches(i, c)))
    }

    pub(crate) fn query_all(&self, sel: &str) -> Vec<usize> {
        let chains = parse_selector_list(sel);
        (0..self.nodes.len()).filter(|&i| chains.iter().any(|c| self.matches(i, c))).collect()
    }
}

/// An inline style as the `style` attribute writes it.
pub(crate) fn style_text(inline: &[(String, String)]) -> String {
    inline.iter().map(|(k, v)| format!("{k}: {v};")).collect::<Vec<_>>().join(" ")
}

pub(crate) fn parse_attrs(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '=' {
            i += 1;
        }
        if start == i {
            i += 1;
            continue;
        }
        let key: String = chars[start..i].iter().collect::<String>().to_ascii_lowercase();
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        let mut val = String::new();
        if i < chars.len() && chars[i] == '=' {
            i += 1;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            if i < chars.len() && (chars[i] == '"' || chars[i] == '\'') {
                let q = chars[i];
                i += 1;
                while i < chars.len() && chars[i] != q {
                    val.push(chars[i]);
                    i += 1;
                }
                i += 1;
            } else {
                while i < chars.len() && !chars[i].is_whitespace() {
                    val.push(chars[i]);
                    i += 1;
                }
            }
        }
        out.push((key, decode_entities(&val)));
    }
    out
}

/// A comma-separated selector list, with the commas inside brackets and parentheses left alone.
pub(crate) fn parse_selector_list(sel: &str) -> Vec<Chain> {
    let mut out = Vec::new();
    let (mut depth, mut start) = (0i32, 0usize);
    let mut quote: Option<char> = None;
    for (i, c) in sel.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"') | (None, '\'') => quote = Some(c),
            (None, '(') | (None, '[') => depth += 1,
            (None, ')') | (None, ']') => depth -= 1,
            (None, ',') if depth == 0 => {
                out.push(&sel[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&sel[start..]);
    out.into_iter().map(parse_chain).filter(|c| !c.is_empty()).collect()
}

fn parse_nth(arg: &str) -> Option<(i32, i32)> {
    let t: String = arg.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_lowercase();
    match t.as_str() {
        "odd" => return Some((2, 1)),
        "even" => return Some((2, 0)),
        _ => {}
    }
    if let Some(n) = t.find('n') {
        let a = match &t[..n] {
            "" | "+" => 1,
            "-" => -1,
            x => x.parse().ok()?,
        };
        let b = if t[n + 1..].is_empty() { 0 } else { t[n + 1..].trim_start_matches('+').parse().ok()? };
        Some((a, b))
    } else {
        Some((0, t.parse().ok()?))
    }
}

/// One compound selector (`div.a#b[x=y]:first-child`).
fn parse_simple(text: &str) -> Simple {
    let mut s = Simple::default();
    let c: Vec<char> = text.chars().collect();
    let mut i = 0;
    let ident = |i: &mut usize| -> String {
        let st = *i;
        while *i < c.len() && (c[*i].is_alphanumeric() || matches!(c[*i], '-' | '_' | '\\')) {
            *i += 1;
        }
        c[st..*i].iter().collect()
    };
    // a type selector or `*` first
    if i < c.len() && c[i] == '*' {
        i += 1;
    } else if i < c.len() && (c[i].is_alphabetic()) {
        s.tag = Some(ident(&mut i).to_ascii_lowercase());
    }
    while i < c.len() {
        match c[i] {
            '#' => {
                i += 1;
                s.id = Some(ident(&mut i));
            }
            '.' => {
                i += 1;
                s.classes.push(ident(&mut i));
            }
            '[' => {
                let end = c[i..].iter().position(|&x| x == ']').map(|e| i + e).unwrap_or(c.len());
                let inner: String = c[i + 1..end].iter().collect();
                i = (end + 1).min(c.len());
                let ops = [("~=", AttrOp::Word), ("|=", AttrOp::Dash), ("^=", AttrOp::Prefix), ("$=", AttrOp::Suffix), ("*=", AttrOp::Contains), ("=", AttrOp::Eq)];
                let found = ops.iter().filter_map(|(t, op)| inner.find(t).map(|p| (p, *t, *op))).min_by_key(|(p, ..)| *p);
                let sel = match found {
                    Some((p, t, op)) => AttrSel {
                        name: inner[..p].trim().to_ascii_lowercase(),
                        op,
                        value: inner[p + t.len()..].trim().trim_end_matches(" i").trim().trim_matches(|x| x == '"' || x == '\'').to_string(),
                    },
                    None => AttrSel { name: inner.trim().to_ascii_lowercase(), op: AttrOp::Exists, value: String::new() },
                };
                s.attrs.push(sel);
            }
            ':' => {
                i += 1;
                let element = i < c.len() && c[i] == ':';
                if element {
                    i += 1;
                }
                let name = ident(&mut i).to_ascii_lowercase();
                let mut arg = String::new();
                if i < c.len() && c[i] == '(' {
                    let mut depth = 0;
                    let st = i + 1;
                    while i < c.len() {
                        if c[i] == '(' {
                            depth += 1;
                        } else if c[i] == ')' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        i += 1;
                    }
                    arg = c[st..i.min(c.len())].iter().collect();
                    i = (i + 1).min(c.len());
                }
                if element {
                    s.pseudos.push(Pseudo::Never);
                    continue;
                }
                let nth = |of_type, from_last| parse_nth(&arg).map(|(a, b)| Pseudo::Nth { a, b, from_last, of_type }).unwrap_or(Pseudo::Never);
                s.pseudos.push(match name.as_str() {
                    "first-child" => Pseudo::Nth { a: 0, b: 1, from_last: false, of_type: false },
                    "last-child" => Pseudo::Nth { a: 0, b: 1, from_last: true, of_type: false },
                    "first-of-type" => Pseudo::Nth { a: 0, b: 1, from_last: false, of_type: true },
                    "last-of-type" => Pseudo::Nth { a: 0, b: 1, from_last: true, of_type: true },
                    "only-child" => Pseudo::OnlyChild,
                    "nth-child" => nth(false, false),
                    "nth-last-child" => nth(false, true),
                    "nth-of-type" => nth(true, false),
                    "nth-last-of-type" => nth(true, true),
                    "empty" => Pseudo::Empty,
                    "root" => Pseudo::Root,
                    "not" => Pseudo::Not(Box::new(parse_simple(arg.trim()))),
                    "is" | "where" => {
                        // (only the first alternative; enough for the pages seen)
                        let first = parse_selector_list(&arg).into_iter().next().and_then(|mut ch| ch.pop()).map(|(_, s)| s).unwrap_or_default();
                        for cl in first.classes {
                            s.classes.push(cl);
                        }
                        if first.tag.is_some() {
                            s.tag = first.tag;
                        }
                        continue;
                    }
                    _ => Pseudo::Never,
                });
            }
            _ => i += 1,
        }
    }
    s
}

pub(crate) fn parse_chain(sel: &str) -> Chain {
    // split into compounds and combinators; spaces inside brackets and parentheses are kept
    let mut out: Chain = Vec::new();
    let mut cur = String::new();
    let mut comb = Comb::Descendant;
    let mut pending: Option<Comb> = None;
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut flush = |cur: &mut String, comb: &mut Comb, pending: &mut Option<Comb>, out: &mut Chain| {
        if !cur.is_empty() {
            out.push((*comb, parse_simple(cur)));
            cur.clear();
            *comb = Comb::Descendant;
            *pending = None;
        }
    };
    for ch in sel.trim().chars() {
        match (quote, ch) {
            (Some(q), c) if c == q => {
                quote = None;
                cur.push(ch);
            }
            (Some(_), _) => cur.push(ch),
            (None, '"') | (None, '\'') => {
                quote = Some(ch);
                cur.push(ch);
            }
            (None, '(') | (None, '[') => {
                depth += 1;
                cur.push(ch);
            }
            (None, ')') | (None, ']') => {
                depth -= 1;
                cur.push(ch);
            }
            (None, c) if depth == 0 && (c == '>' || c == '+' || c == '~') => {
                flush(&mut cur, &mut comb, &mut pending, &mut out);
                comb = match c {
                    '>' => Comb::Child,
                    '+' => Comb::Adjacent,
                    _ => Comb::Sibling,
                };
            }
            (None, c) if depth == 0 && c.is_whitespace() => flush(&mut cur, &mut comb, &mut pending, &mut out),
            (None, _) => cur.push(ch),
        }
    }
    flush(&mut cur, &mut comb, &mut pending, &mut out);
    if let Some(first) = out.first_mut() {
        first.0 = Comb::Descendant;
    }
    out
}

fn simple_specificity(s: &Simple) -> u32 {
    let mut n = (s.id.is_some() as u32) * 10_000 + (s.classes.len() as u32 + s.attrs.len() as u32) * 100 + s.tag.is_some() as u32;
    for p in &s.pseudos {
        n += match p {
            Pseudo::Not(inner) => simple_specificity(inner),
            Pseudo::Root | Pseudo::Empty | Pseudo::OnlyChild | Pseudo::Nth { .. } | Pseudo::Never => 100,
        };
    }
    n
}

pub(crate) fn specificity(chain: &Chain) -> u32 {
    chain.iter().map(|(_, s)| simple_specificity(s)).sum()
}

pub(crate) fn parse_css(src: &str) -> Vec<Rule> {
    // strip comments
    let mut clean = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(p) = rest.find("/*") {
        clean.push_str(&rest[..p]);
        rest = match rest[p..].find("*/") {
            Some(e) => &rest[p + e + 2..],
            None => "",
        };
    }
    clean.push_str(rest);
    let mut rules = Vec::new();
    let mut s = clean.as_str();
    while let Some(open) = s.find('{') {
        let head = s[..open].trim();
        let Some(close) = s[open..].find('}') else { break };
        let body = &s[open + 1..open + close];
        s = &s[open + close + 1..];
        if head.starts_with('@') {
            // an at-rule with a nested block: skip up to the matching close
            if body.contains('{') {
                if let Some(e) = s.find('}') {
                    s = &s[e + 1..];
                }
            }
            continue;
        }
        let sels: Vec<Chain> = parse_selector_list(head);
        let decls = parse_style_attr(body);
        if !sels.is_empty() {
            let order = rules.len();
            rules.push(Rule { sels, decls, order });
        }
    }
    rules
}