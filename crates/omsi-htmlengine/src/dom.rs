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
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Simple {
    pub(crate) tag: Option<String>,
    pub(crate) id: Option<String>,
    pub(crate) classes: Vec<String>,
}

pub(crate) type Chain = Vec<Simple>;

#[derive(Debug, Clone)]
pub(crate) struct Rule {
    pub(crate) sels: Vec<Chain>,
    pub(crate) decls: Vec<(String, String)>,
    pub(crate) order: usize,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Dom {
    pub(crate) nodes: Vec<Node>,
    pub(crate) rules: Vec<Rule>,
    pub(crate) scripts: Vec<String>,
    pub(crate) body: usize,
    /// Counts every visible change of the page; layout and frame caches key on it.
    pub(crate) generation: u64,
}

pub(crate) const VOID: &[&str] = &[
    "br", "img", "hr", "meta", "link", "input", "area", "base", "col", "source", "wbr",
];

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
            if k.is_empty() { None } else { Some((k, v)) }
        })
        .collect()
}

impl Dom {
    pub(crate) fn parse(html: &str) -> Dom {
        let mut dom = Dom::default();
        dom.nodes.push(Node {
            tag: "#root".into(),
            ..Node::default()
        });
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
                if name.is_empty()
                    || !name
                        .chars()
                        .next()
                        .map_or(false, |c| c.is_ascii_alphabetic())
                {
                    continue;
                }
                let mut node = Node {
                    tag: name.clone(),
                    parent: stack.last().copied(),
                    ..Node::default()
                };
                for (k, v) in parse_attrs(attrs) {
                    match k.as_str() {
                        "id" => node.id = v,
                        "class" => {
                            node.classes = v.split_whitespace().map(str::to_string).collect()
                        }
                        "style" => node.inline = parse_style_attr(&v),
                        "src" => node.src = v,
                        "width" => node.attr_w = v,
                        "height" => node.attr_h = v,
                        e if e.len() > 2 && e.starts_with("on") => {
                            node.on.push((e[2..].to_string(), v))
                        }
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
                dom.nodes.push(Node {
                    tag: "#text".into(),
                    text: Some(text),
                    parent: Some(parent),
                    ..Node::default()
                });
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
        n.kids
            .iter()
            .map(|&k| self.text_of(k))
            .collect::<Vec<_>>()
            .join("")
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
        self.nodes.push(Node {
            tag: "#text".into(),
            text: Some(s),
            parent: Some(idx),
            ..Node::default()
        });
        self.nodes[idx].kids = vec![t];
        self.generation += 1;
    }

    /// A new element that hangs nowhere yet (`document.createElement`).
    pub(crate) fn create(&mut self, tag: &str) -> usize {
        self.nodes.push(Node {
            tag: tag.to_string(),
            ..Node::default()
        });
        self.nodes.len() - 1
    }

    pub(crate) fn detach(&mut self, idx: usize) {
        if let Some(p) = self.nodes[idx].parent.take() {
            self.nodes[p].kids.retain(|&k| k != idx);
            self.generation += 1;
        }
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
        self.nodes
            .iter()
            .position(|n| n.text.is_none() && !n.id.is_empty() && n.id == id)
    }

    pub(crate) fn matches(&self, idx: usize, chain: &Chain) -> bool {
        let Some((last, rest)) = chain.split_last() else {
            return false;
        };
        if !self.matches_simple(idx, last) {
            return false;
        }
        let mut cur = self.nodes[idx].parent;
        for s in rest.iter().rev() {
            loop {
                match cur {
                    Some(p) => {
                        cur = self.nodes[p].parent;
                        if self.matches_simple(p, s) {
                            break;
                        }
                    }
                    None => return false,
                }
            }
        }
        true
    }

    pub(crate) fn matches_simple(&self, idx: usize, s: &Simple) -> bool {
        let n = &self.nodes[idx];
        if n.text.is_some() {
            return false;
        }
        s.tag.as_ref().map_or(true, |t| *t == n.tag)
            && s.id.as_ref().map_or(true, |i| *i == n.id)
            && s.classes.iter().all(|c| n.classes.contains(c))
    }

    pub(crate) fn query(&self, sel: &str) -> Option<usize> {
        let chains: Vec<Chain> = sel.split(',').map(parse_chain).collect();
        (0..self.nodes.len()).find(|&i| chains.iter().any(|c| self.matches(i, c)))
    }
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
        let key: String = chars[start..i]
            .iter()
            .collect::<String>()
            .to_ascii_lowercase();
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

pub(crate) fn parse_chain(sel: &str) -> Chain {
    sel.split_whitespace()
        .filter(|p| *p != ">" && *p != "+" && *p != "~")
        .map(|part| {
            let mut s = Simple::default();
            let mut cur = String::new();
            let mut mode = 't';
            let flush = |mode: char, cur: &mut String, s: &mut Simple| {
                if cur.is_empty() {
                    return;
                }
                let v = std::mem::take(cur);
                match mode {
                    't' if v != "*" => s.tag = Some(v.to_ascii_lowercase()),
                    '#' => s.id = Some(v),
                    '.' => s.classes.push(v),
                    _ => {}
                }
            };
            for c in part.chars() {
                if c == '#' || c == '.' {
                    flush(mode, &mut cur, &mut s);
                    mode = c;
                } else {
                    cur.push(c);
                }
            }
            flush(mode, &mut cur, &mut s);
            s
        })
        .collect()
}

pub(crate) fn specificity(chain: &Chain) -> u32 {
    chain
        .iter()
        .map(|s| {
            (s.id.is_some() as u32) * 10_000
                + (s.classes.len() as u32) * 100
                + s.tag.is_some() as u32
        })
        .sum()
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
        let Some(close) = s[open..].find('}') else {
            break;
        };
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
        let sels: Vec<Chain> = head
            .split(',')
            .map(parse_chain)
            .filter(|c| !c.is_empty())
            .collect();
        let decls = parse_style_attr(body);
        if !sels.is_empty() {
            let order = rules.len();
            rules.push(Rule { sels, decls, order });
        }
    }
    rules
}
