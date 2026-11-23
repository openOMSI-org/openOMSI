// Markdown -> HTML for the documentation. Beyond plain Markdown it gives the page real structure:
//  - callouts from `> **Note:** …` and `> [!NOTE]`
//  - tables whose columns are typed by their header (keys, tags, names) and that can be filtered
//  - code with a language label, highlighting (OMSI's [keyword] files included) and ASCII diagrams
//  - GitHub-compatible heading ids, so every old link into a document still lands

import MarkdownIt from "markdown-it";
import hljs from "highlight.js/lib/core";
import bash from "highlight.js/lib/languages/bash";
import powershell from "highlight.js/lib/languages/powershell";
import lua from "highlight.js/lib/languages/lua";
import javascript from "highlight.js/lib/languages/javascript";
import json from "highlight.js/lib/languages/json";
import xml from "highlight.js/lib/languages/xml";
import css from "highlight.js/lib/languages/css";
import ini from "highlight.js/lib/languages/ini";
import rust from "highlight.js/lib/languages/rust";
import delphi from "highlight.js/lib/languages/delphi";
import yaml from "highlight.js/lib/languages/yaml";
import diff from "highlight.js/lib/languages/diff";

hljs.registerLanguage("bash", bash);
hljs.registerLanguage("powershell", powershell);
hljs.registerLanguage("lua", lua);
hljs.registerLanguage("javascript", javascript);
hljs.registerLanguage("json", json);
hljs.registerLanguage("xml", xml);
hljs.registerLanguage("css", css);
hljs.registerLanguage("ini", ini);
hljs.registerLanguage("rust", rust);
hljs.registerLanguage("delphi", delphi);
hljs.registerLanguage("yaml", yaml);
hljs.registerLanguage("diff", diff);
// OMSI 2's own text files: `[keyword]` alone on a line, parameters on the lines after it.
hljs.registerLanguage("omsi", () => ({
  name: "omsi",
  contains: [
    { className: "meta", begin: /^\[[^\]\n]+\]\s*$/m },
    { className: "comment", begin: /^\s*(#|;|\/\/)/m, end: /$/ },
    { className: "number", begin: /\b-?\d+(\.\d+)?\b/ },
    { className: "built_in", begin: /^-<(DISABLED|ENABLED)>-$/m },
  ],
}));

const LANG_ALIAS = {
  sh: "bash", shell: "bash", zsh: "bash", console: "bash", ps1: "powershell", pwsh: "powershell",
  js: "javascript", html: "xml", svg: "xml", pascal: "delphi", toml: "ini", cfg: "omsi", omsi: "omsi",
  rs: "rust", yml: "yaml", jsonc: "json",
};
const LANG_LABEL = {
  bash: "shell", powershell: "PowerShell", lua: "Lua", javascript: "JavaScript", json: "JSON", xml: "HTML",
  css: "CSS", ini: "INI", rust: "Rust", delphi: "Pascal", yaml: "YAML", diff: "diff", omsi: "OMSI file", text: "",
};

export const esc = (t) =>
  String(t ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

// GitHub's heading ids: lower case, punctuation gone, spaces to hyphens, repeats numbered.
export function makeSlugger() {
  const seen = new Map();
  return (text) => {
    let s = text.toLowerCase().replace(/<[^>]+>/g, "").replace(/[^\p{L}\p{N}\- _]/gu, "").trim().replace(/ /g, "-");
    if (!s) s = "section";
    const n = seen.get(s) ?? 0;
    seen.set(s, n + 1);
    return n ? `${s}-${n}` : s;
  };
}

function guessLang(code) {
  if (/[│┌┐└┘├┤┬┴┼─▼▲►◄]|-->|<--|\+--+\+/.test(code) && code.split("\n").length > 2) return "diagram";
  if (/^\[[A-Za-z_][^\]]*\]\s*$/m.test(code) && !/^\s*(\$|#!)/.test(code)) return "omsi";
  if (/^\s*(\$ |openomsi\b|cargo\b|OMSI_[A-Z_]+=|\.\/|git\b|npm\b|chmod\b|xattr\b|sudo\b|export\b|curl\b)/m.test(code)) return "bash";
  return "text";
}

function highlight(code, lang) {
  if (lang === "text" || lang === "diagram" || !hljs.getLanguage(lang)) return esc(code);
  try { return hljs.highlight(code, { language: lang, ignoreIllegals: true }).value; } catch { return esc(code); }
}

const CALLOUTS = {
  note: "Note", important: "Important", warning: "Warning", caution: "Caution", tip: "Tip",
  experimental: "Experimental", beta: "Beta", legacy: "Legacy", deprecated: "Deprecated", breaking: "Breaking change",
  status: "Status", example: "Example", why: "Why", todo: "To do", limits: "Limits", limit: "Limit",
};
const CALLOUT_KIND = {
  note: "note", important: "important", warning: "warning", caution: "warning", tip: "tip", experimental: "warning",
  beta: "warning", legacy: "warning", deprecated: "warning", breaking: "warning", status: "note", example: "note",
  why: "note", todo: "note", limits: "note", limit: "note",
};

const KEY_COLS = /^(key|keys|shortcut|shortcuts|hotkey|hotkeys)$/i;
const TAG_COLS = /^(type|kind|read|since|status|permission|platform|scope)$/i;

export function createMarkdown({ resolveLink } = {}) {
  const md = new MarkdownIt({ html: true, linkify: false, typographer: false });

  // --- code ---------------------------------------------------------------------------------
  md.renderer.rules.fence = (tokens, i) => {
    const t = tokens[i];
    let lang = (t.info || "").trim().split(/\s+/)[0].toLowerCase();
    lang = LANG_ALIAS[lang] || lang;
    if (!lang) lang = guessLang(t.content);
    const code = t.content.replace(/\n$/, "");
    if (lang === "diagram") {
      return `<figure class="diagram"><figcaption>Diagram</figcaption><pre><code>${esc(code)}</code></pre></figure>\n`;
    }
    const label = lang in LANG_LABEL ? LANG_LABEL[lang] : lang;
    const cap = label ? `<figcaption>${esc(label)}</figcaption>` : "";
    const cls = lang === "text" ? "" : ` language-${lang}`;
    return `<figure class="code${label ? "" : " bare"}">${cap}<pre><code class="hljs${cls}">${highlight(code, lang)}</code></pre></figure>\n`;
  };
  md.renderer.rules.code_block = (tokens, i) =>
    `<figure class="code bare"><pre><code>${esc(tokens[i].content.replace(/\n$/, ""))}</code></pre></figure>\n`;
  md.renderer.rules.code_inline = (tokens, i) => {
    const t = tokens[i];
    return `<${t.tag || "code"}>${esc(t.content)}</${t.tag || "code"}>`;
  };

  // --- headings: ids, permalinks, level shifted so a chapter page starts at h1 ---------------
  md.renderer.rules.heading_open = (tokens, i, opts, env) => {
    const t = tokens[i];
    const level = Math.min(6, Math.max(1, Number(t.tag.slice(1)) - (env.shift || 0)));
    const id = t.attrGet("id");
    return `<h${level}${id ? ` id="${esc(id)}"` : ""}>`;
  };
  md.renderer.rules.heading_close = (tokens, i, opts, env) => {
    const t = tokens[i];
    const level = Math.min(6, Math.max(1, Number(t.tag.slice(1)) - (env.shift || 0)));
    const open = tokens[i - 2];
    const id = open?.attrGet?.("id");
    const link = id && level > 1 ? `<a class="anchor" href="#${esc(id)}" aria-label="Link to this section">#</a>` : "";
    return `${link}</h${level}>\n`;
  };

  // --- links ---------------------------------------------------------------------------------
  md.core.ruler.push("openomsi_links", (state) => {
    for (const blk of state.tokens) {
      if (blk.type !== "inline") continue;
      for (const t of blk.children || []) {
        if (t.type !== "link_open") continue;
        const href = t.attrGet("href") || "";
        const out = resolveLink ? resolveLink(href, state.env) : { href };
        t.attrSet("href", out.href);
        if (out.external) { t.attrSet("rel", "noopener"); t.attrJoin("class", "ext"); }
      }
    }
  });

  // --- blockquotes: callouts -----------------------------------------------------------------
  md.core.ruler.push("openomsi_callouts", (state) => {
    const toks = state.tokens;
    for (let i = 0; i < toks.length; i++) {
      if (toks[i].type !== "blockquote_open") continue;
      const p = toks[i + 1], inline = toks[i + 2];
      if (p?.type !== "paragraph_open" || inline?.type !== "inline") continue;
      const kids = inline.children || [];
      while (kids[0]?.type === "text" && kids[0].content === "") kids.shift();
      let label = null;
      // > [!NOTE]
      const m = /^\[!(\w+)\]\s*/.exec(kids[0]?.type === "text" ? kids[0].content : "");
      if (m && CALLOUTS[m[1].toLowerCase()]) {
        label = m[1].toLowerCase();
        kids[0].content = kids[0].content.slice(m[0].length);
        if (!kids[0].content && kids[1]?.type === "softbreak") kids.splice(0, 2);
        else if (!kids[0].content) kids.splice(0, 1);
      } else if (kids[0]?.type === "strong_open" && kids[1]?.type === "text" && kids[2]?.type === "strong_close") {
        // > **Note:** … (the colon may sit inside or outside the bold text)
        const word = kids[1].content.replace(/[:.]\s*$/, "").trim().toLowerCase();
        if (CALLOUTS[word]) {
          label = word;
          let n = 3;
          if (kids[3]?.type === "text" && /^[:.]?\s*$/.test(kids[3].content)) n = 4;
          else if (kids[3]?.type === "text") kids[3].content = kids[3].content.replace(/^[:.]\s*/, "");
          kids.splice(0, n);
        }
      }
      if (label) {
        toks[i].meta = { callout: label };
      }
    }
  });

  // a paragraph of its own that starts "**Note:** ..." is a callout too
  md.core.ruler.push("openomsi_callout_paragraphs", (state) => {
    const toks = state.tokens;
    for (let i = 0; i < toks.length; i++) {
      if (toks[i].type !== "paragraph_open" || toks[i].level !== 0) continue;
      const kids = toks[i + 1]?.children || [];
      while (kids[0]?.type === "text" && kids[0].content === "") kids.shift();
      if (!(kids[0]?.type === "strong_open" && kids[1]?.type === "text" && kids[2]?.type === "strong_close")) continue;
      const raw = kids[1].content.trim();
      const word = raw.replace(/[:.]\s*$/, "").toLowerCase();
      const colonInside = /:\s*$/.test(raw);
      const colonAfter = kids[3]?.type === "text" && /^:/.test(kids[3].content);
      if (!CALLOUTS[word] || !(colonInside || colonAfter)) continue;
      if (colonAfter) kids[3].content = kids[3].content.replace(/^:\s*/, "");
      kids.splice(0, 3);
      toks[i].meta = { callout: word, para: true };
      toks[i + 2].meta = { calloutEnd: true };
    }
  });
  // --- walls of text: a run of `name` (what it does), `name` (what it does) becomes a list of
  //     settings, and any other very long paragraph is cut into paragraphs at sentence ends -------
  md.core.ruler.push("openomsi_long_paragraphs", (state) => {
    const toks = state.tokens;
    const T = state.Token;
    const text = (content) => Object.assign(new T("text", "", 0), { content });
    const render = (kids) => md.renderer.renderInline(kids, md.options, state.env);
    const blank = (t) => t.type === "softbreak" || t.type === "hardbreak" || (t.type === "text" && !t.content.trim());
    const trim = (kids) => {
      let a = 0, b = kids.length;
      while (a < b && blank(kids[a])) a++;
      while (b > a && blank(kids[b - 1])) b--;
      const out = kids.slice(a, b);
      if (out[0]?.type === "text" && out[0].content !== out[0].content.trimStart()) out[0] = text(out[0].content.trimStart());
      const l = out.length - 1;
      if (out[l]?.type === "text" && out[l].content !== out[l].content.trimEnd()) out[l] = text(out[l].content.trimEnd());
      return out;
    };
    // cut a run of tokens at its depth-0 separators (outside parentheses): , ; and plus
    const SEP_ALL = /\s*[,;]\s*(?:(?:and|plus)\s+)?|\s+(?:and|plus)\s+/g;
    const SEP_SEMI = /\s*;\s*/g;
    const segments = (kids, only) => {
      const SEP = only === ";" ? SEP_SEMI : SEP_ALL;
      const segs = [[]];
      let depth = 0, span = 0;
      for (const t of kids) {
        if (t.type === "em_open" || t.type === "strong_open" || t.type === "link_open") span++;
        if (t.type === "em_close" || t.type === "strong_close" || t.type === "link_close") span = Math.max(0, span - 1);
        if (t.type !== "text") { segs[segs.length - 1].push(t); continue; }
        if (span) { for (const c of t.content) { if (c === "(") depth++; else if (c === ")") depth = Math.max(0, depth - 1); } segs[segs.length - 1].push(t); continue; }
        let last = 0, pos = 0;
        SEP.lastIndex = 0;
        let m;
        while ((m = SEP.exec(t.content))) {
          for (; pos < m.index; pos++) { const c = t.content[pos]; if (c === "(") depth++; else if (c === ")") depth = Math.max(0, depth - 1); }
          if (depth === 0 && m[0].length) {
            segs[segs.length - 1].push(text(t.content.slice(last, m.index)));
            segs.push([]);
            last = m.index + m[0].length;
          }
          pos = m.index;
          if (!m[0].length) SEP.lastIndex++;
        }
        for (; pos < t.content.length; pos++) { const c = t.content[pos]; if (c === "(") depth++; else if (c === ")") depth = Math.max(0, depth - 1); }
        segs[segs.length - 1].push(text(t.content.slice(last)));
      }
      return segs.map(trim);
    };
    // the name an item starts with at kids[k]: `code`, or an emphasised word group (*Name*, **Name**)
    const nameLen = (seg, k = 0) => {
      const t = seg[k];
      if (!t) return 0;
      if (t.type === "code_inline") return 1;
      if (t.type === "em_open" || t.type === "strong_open") {
        const close = t.type.replace("_open", "_close");
        for (let n = k + 1; n < seg.length && n < k + 8; n++) {
          if (seg[n].type === close) return n > k + 1 ? n - k + 1 : 0;
          if (seg[n].type !== "text" && seg[n].type !== "softbreak") return 0;
        }
      }
      return 0;
    };
    // is this segment `name` or `name` (description)?  -> { name, desc } or null
    const asItem = (seg) => {
      const n = nameLen(seg, 0);
      if (!n) return null;
      const name = seg.slice(0, n);
      if (seg.length === n) return { name, desc: null };
      const rest = trim(seg.slice(n));
      const first = rest[0];
      if (first?.type === "text" && /^\s*[.!?](\s|$)/.test(first.content)) {
        const left = first.content.replace(/^\s*[.!?]\s*/, "");
        return { name, desc: null, after: trim([...(left ? [text(left)] : []), ...rest.slice(1)]) };
      }
      if (first?.type !== "text" || !/^\(/.test(first.content)) return null;
      // where does the parenthesis that opens the description close?
      let depth = 0, endTok = -1, endPos = -1;
      scan: for (let i = 0; i < rest.length; i++) {
        if (rest[i].type !== "text") continue;
        const c = rest[i].content;
        for (let k = 0; k < c.length; k++) {
          if (c[k] === "(") depth++;
          else if (c[k] === ")" && --depth === 0) { endTok = i; endPos = k; break scan; }
        }
      }
      if (endTok < 0) return null;
      const closing = rest[endTok].content;
      const after = [];
      let tailText = closing.slice(endPos + 1);
      if (tailText.trim() || endTok < rest.length - 1) {
        // text after the description: only a sentence end followed by other prose is allowed
        if (!/^\s*[.!?](\s|$)/.test(tailText) && !(tailText.trim() === "" && false)) return null;
        tailText = tailText.replace(/^\s*[.!?]\s*/, "");
        if (tailText) after.push(text(tailText));
        after.push(...rest.slice(endTok + 1));
      }
      const desc = rest.slice(0, endTok + 1);
      desc[0] = text(desc[0].content.replace(/^\(\s*/, ""));
      desc[endTok] = text(endTok === 0 ? desc[0].content.slice(0, endPos - 1 < 0 ? 0 : endPos - 1) : closing.slice(0, endPos));
      if (endTok === 0) desc[0] = text(first.content.slice(first.content.indexOf("(") + 1, endPos).trimStart());
      return { name, desc: trim(desc), after: trim(after) };
    };
    const CONNECT = /(?:\b(?:holds|has|have|are|is|include|includes|contains|reads|takes|keys|options|settings|accepts|knows|these|following|lines|entries|items)\s*|:\s*)$/i;
    const asItems = (kids) => {
      const segs = segments(kids);
      const items = segs.map(asItem);
      // the first item often follows a lead-in in the same segment: "... holds `msaa` (...)"
      let bestFrom = -1, bestLen = 0;
      for (let i = 0; i < segs.length; i++) {
        let j = i;
        while (j < segs.length && items[j]) { j++; if (items[j - 1].after?.length) break; }
        if (j - i > bestLen) { bestFrom = i; bestLen = j - i; }
      }
      if (bestLen < 3) return null;
      const lead = [];
      if (bestFrom > 0) {
        const before = segs[bestFrom - 1];
        // look for a code token that starts an item inside the segment before the run
        for (let k = 1; k < before.length; k++) {
          let pk = k - 1;
          while (pk > 0 && (before[pk].type === "softbreak" || blank(before[pk]))) pk--;
          if (!nameLen(before, k) || before[pk].type !== "text" || !CONNECT.test(before[pk].content)) continue;
          const it = asItem(before.slice(k));
          if (it) { items[bestFrom - 1] = it; lead.push(...before.slice(0, k)); segs[bestFrom - 1] = before.slice(k); bestFrom--; bestLen++; break; }
        }
      }
      const described = items.slice(bestFrom, bestFrom + bestLen).filter((it) => it.desc).length;
      if (!((bestLen >= 4 && described >= 3) || (bestLen >= 5 && described >= 2))) return null;
      const joinSegs = (list) => { const out = []; list.forEach((sg, n) => { if (n) out.push(text(", ")); out.push(...sg); }); return out; };
      if (!lead.length) lead.push(...joinSegs(segs.slice(0, bestFrom)));
      else if (bestFrom > 0) lead.unshift(...joinSegs(segs.slice(0, bestFrom)), text(", "));
      const run = items.slice(bestFrom, bestFrom + bestLen);
      const lastAfter = run[run.length - 1].after || [];
      const rest = joinSegs(segs.slice(bestFrom + bestLen));
      return { lead: trim(lead), items: run, tail: trim([...lastAfter, ...(lastAfter.length && rest.length ? [text(" ")] : []), ...rest]) };
    };
    const kvHtml = ({ lead, items, tail }) => {
      const para = (kids) => (kids.length ? `<p>${render(kids)}</p>\n` : "");
      let rows = "", names = [];
      const flushNames = () => { if (names.length) rows += `<div class="bare"><dt>${names.join(" ")}</dt></div>\n`; names = []; };
      for (const it of items) {
        if (!it.desc) { names.push(render(it.name)); continue; }
        flushNames();
        rows += `<div><dt>${render(it.name)}</dt><dd>${render(it.desc)}</dd></div>\n`;
      }
      flushNames();
      return `${para(lead)}<dl class="kv">\n${rows}</dl>\n${para(tail)}`;
    };

    // ---- cutting a long run of prose: at sentence ends, or (if there are none) at semicolons
    const ABBR = /(?:\be\.g|\bi\.e|\betc|\bvs|\bcf|\bapprox|\bNo|\bFig|\bSt|\b[A-Z])$/;
    const nextReal = (kids, idx) => { let n = idx + 1; while (kids[n] && (kids[n].type === "softbreak" || blank(kids[n]))) n++; return kids[n]; };
    const pieces = (kids, marks) => {
      const out = [[]];
      let depth = 0;
      kids.forEach((t, idx) => {
        if (t.type !== "text") { out[out.length - 1].push(t); return; }
        const c = t.content;
        let last = 0;
        for (let k = 0; k < c.length; k++) {
          const ch = c[k];
          if (ch === "(") depth++;
          else if (ch === ")") depth = Math.max(0, depth - 1);
          else if (depth === 0 && marks.includes(ch)) {
            const rest = c.slice(k + 1);
            if (rest.length && !/^\s/.test(rest)) continue;
            const after = rest.trimStart();
            let ends;
            if (after.length) ends = ch === ";" ? true : /^[A-Z]/.test(after);
            else { const n = nextReal(kids, idx); ends = !!n && (ch === ";" ? true : (n.type === "strong_open" || n.type === "code_inline" || (n.type === "text" && /^\s*[A-Z]/.test(n.content)))); }
            if (!ends || (ch === "." && ABBR.test(c.slice(Math.max(0, k - 6), k)))) continue;
            out[out.length - 1].push(text(c.slice(last, k + 1)));
            out.push([]);
            last = k + 1;
          }
        }
        out[out.length - 1].push(text(c.slice(last)));
      });
      return out.map(trim).filter((sn) => sn.length);
    };
    const len = (kids) => kids.reduce((n, t) => n + (t.content || "").length, 0);
    const grouped = (parts) => {
      if (parts.length < 2) return null;
      const groups = [[]];
      let size = 0;
      for (const sn of parts) {
        const l = len(sn);
        if (groups[groups.length - 1].length && size + l > 520) { groups.push([]); size = 0; }
        const target = groups[groups.length - 1];
        if (target.length) target.push(text(" "));
        target.push(...sn);
        size += l;
      }
      if (groups.length > 1 && len(groups[groups.length - 1]) < 100) { const l = groups.pop(); groups[groups.length - 1].push(text(" "), ...l); }
      return groups.length > 1 ? groups : null;
    };
    // sentences first; a sentence that is itself long is cut at its semicolons
    const split = (kids) => {
      const out = [];
      for (const sn of pieces(kids, ".!?")) {
        if (len(sn) > 520) out.push(...pieces(sn, ";")); else out.push(sn);
      }
      return out;
    };

    // ---- "**topic** - what it says; **topic** - what it says" becomes a list as well
    const asBold = (seg) => {
      if (seg[0]?.type !== "strong_open") return null;
      const close = seg.findIndex((t) => t.type === "strong_close");
      if (close < 1) return null;
      const rest = trim(seg.slice(close + 1));
      let dashed = false;
      if (rest[0]?.type === "text") { dashed = /^\s*[-–—:]\s/.test(rest[0].content); rest[0] = text(rest[0].content.replace(/^\s*[-–—:]\s*/, "")); }
      return { name: seg.slice(1, close), desc: trim(rest), dashed };
    };
    const asBolds = (kids) => {
      const segs = segments(kids, ";");
      const first = segs.findIndex((sg) => sg.some((t) => t.type === "strong_open"));
      if (first < 0) return null;
      const lead = [];
      const head = segs[first];
      const at = head.findIndex((t) => t.type === "strong_open");
      if (at > 0) { lead.push(...head.slice(0, at)); segs[first] = head.slice(at); }
      lead.push(...[]);
      const items = [];
      for (let k = first; k < segs.length; k++) {
        const it = asBold(segs[k]);
        if (it) items.push(it);
        else if (items.length) { const cur = items[items.length - 1]; cur.desc = trim([...cur.desc, text("; "), ...segs[k]]); }
      }
      const good = items.filter((it) => it.dashed && it.desc.length);
      if (items.length < 3 || good.length < items.length * 0.7) return null;
      return { lead: trim(lead), items, tail: [], bold: true };
    };

    // ---- one paragraph -> html (null when it is fine as it is)
    const para = (kids) => (kids.length ? `<p>${render(kids)}</p>\n` : "");
    const toHtml = (kids, level = 0) => {
      if (!kids.length) return { html: "", changed: false };
      const text0 = len(kids);
      if (level < 6 && text0 >= 400) {
        const kv = asItems(kids) || asBolds(kids);
        if (kv) {
          const l = toHtml(kv.lead, level + 1), t = toHtml(kv.tail, level + 1);
          return { html: l.html + kvHtml({ ...kv, lead: [], tail: [] }) + t.html, changed: true };
        }
      }
      if (text0 > 800) {
        const groups = grouped(split(kids));
        if (groups) return { html: groups.map(para).join(""), changed: true };
      }
      return { html: para(kids), changed: false };
    };

    for (let i = 0; i < toks.length; i++) {
      const open = toks[i];
      if (open.type !== "paragraph_open" || open.meta) continue;
      const inline = toks[i + 1], close = toks[i + 2];
      if (inline?.type !== "inline" || close?.type !== "paragraph_close") continue;
      if (inline.content.length < 520) continue;
      const { html, changed } = toHtml(inline.children || []);
      if (!changed) continue;
      inline.meta = { text: inlineText(inline) };
      inline.children = [];
      open.meta = { html };
      close.meta = { skip: true };
    }
  });

  md.renderer.rules.paragraph_open = (tokens, i) => {
    if (tokens[i].meta?.html != null) return tokens[i].meta.html;
    const c = tokens[i].meta?.callout;
    if (!c) return tokens[i].hidden ? "" : "<p>";
    return `<aside class="callout callout-${CALLOUT_KIND[c]}"><div class="callout-title">${CALLOUTS[c]}</div><div class="callout-body"><p>`;
  };
  md.renderer.rules.paragraph_close = (tokens, i) => {
    if (tokens[i].meta?.skip) return "";
    if (tokens[i].meta?.calloutEnd) return "</p></div></aside>\n";
    return tokens[i].hidden ? "" : "</p>\n";
  };
  md.renderer.rules.blockquote_open = (tokens, i) => {
    const c = tokens[i].meta?.callout;
    if (!c) return `<blockquote class="quote">\n`;
    return `<aside class="callout callout-${CALLOUT_KIND[c]}"><div class="callout-title">${CALLOUTS[c]}</div><div class="callout-body">\n`;
  };
  md.renderer.rules.blockquote_close = (tokens, i) => {
    // find the matching open to know which kind of box to close
    let depth = 0;
    for (let j = i; j >= 0; j--) {
      if (tokens[j].type === "blockquote_close") depth++;
      if (tokens[j].type === "blockquote_open" && --depth === 0) return tokens[j].meta?.callout ? `</div></aside>\n` : `</blockquote>\n`;
    }
    return `</blockquote>\n`;
  };

  // --- tables ---------------------------------------------------------------------------------
  md.core.ruler.push("openomsi_tables", (state) => {
    const toks = state.tokens;
    for (let i = 0; i < toks.length; i++) {
      if (toks[i].type !== "table_open") continue;
      let end = i;
      while (toks[end].type !== "table_close") end++;
      const heads = [];
      let rows = 0, col = -1, inHead = false, inBody = false;
      for (let j = i; j <= end; j++) {
        const t = toks[j];
        if (t.type === "thead_open") inHead = true;
        if (t.type === "thead_close") inHead = false;
        if (t.type === "tbody_open") inBody = true;
        if (inHead && t.type === "inline") heads.push(t.content.replace(/[`*_]/g, "").trim());
        if (inBody && t.type === "tr_open") { rows++; col = -1; }
        if (inBody && t.type === "td_open") {
          col++;
          const h = heads[col] || "";
          const inline = toks[j + 1];
          const kids = inline.children || [];
          const onlyCode = kids.length > 0 && kids.every((k) => k.type === "code_inline");
          if (KEY_COLS.test(h)) {
            t.attrJoin("class", "c-keys");
            for (const k of kids) if (k.type === "code_inline") k.tag = "kbd";
          } else if (TAG_COLS.test(h)) {
            t.attrJoin("class", "c-tag");
            const val = inline.content.replace(/[`*_]/g, "").trim().toLowerCase();
            if (val && val.length <= 24) {
              const cls = /^[a-z0-9.\-]+$/.test(val) ? ` tag-${val}` : "";
              kids.unshift(Object.assign(new state.Token("html_inline", "", 0), { content: `<span class="tag${cls}">` }));
              kids.push(Object.assign(new state.Token("html_inline", "", 0), { content: `</span>` }));
            }
          } else if (col === 0 && onlyCode && inline.content.length <= 34) {
            t.attrJoin("class", "c-name");
          } else if (/^(default|since|returns|permissions?)$/i.test(h)) {
            t.attrJoin("class", "c-mono");
          }
        }
      }
      // a column with long text needs room to wrap in: it gets a minimum width
      const lens = [];
      col = -1; let r = -1, inB = false;
      for (let j = i; j <= end; j++) {
        const t = toks[j];
        if (t.type === "tbody_open") inB = true;
        if (t.type === "tr_open") col = -1;
        if (t.type === "td_open" || t.type === "th_open") { col++; if (inB && t.type === "td_open") lens[col] = Math.max(lens[col] || 0, toks[j + 1].content.length); }
      }
      col = -1;
      const widest = Math.max(0, ...lens.filter(Number.isFinite));
      for (let j = i; j <= end; j++) {
        const t = toks[j];
        if (t.type === "tr_open") col = -1;
        if (t.type === "td_open" || t.type === "th_open") { col++; const L = lens[col] || 0; if (L > 48) t.attrJoin("class", L === widest ? "c-text" : "c-mid"); }
      }
      toks[i].meta = { rows, cols: heads.length };
    }
  });
  md.renderer.rules.table_open = (tokens, i) => {
    const { rows = 0, cols = 0 } = tokens[i].meta || {};
    const filter = rows >= 12 ? ' data-filter="1"' : "";
    return `<div class="table-wrap" data-cols="${cols}"${filter}><table>\n`;
  };
  md.renderer.rules.table_close = () => `</table></div>\n`;

  return md;
}

// ---- plain text (for search and for summaries) -------------------------------------------------
// The words of an inline token: its text and code, without the markup around them.
export function inlineText(inline) {
  if (inline.meta?.text != null) return inline.meta.text;
  let out = "";
  for (const k of inline.children || []) {
    if (k.type === "text" || k.type === "code_inline") out += k.content;
    else if (k.type === "softbreak" || k.type === "hardbreak") out += " ";
    else if (k.type === "image") out += k.content || "";
  }
  return out.replace(/\s+/g, " ").trim();
}

export function plainText(tokens) {
  const out = [];
  for (const t of tokens) {
    if (t.type === "inline") out.push(inlineText(t));
    else if (t.type === "fence" || t.type === "code_block") out.push(t.content);
  }
  return out.join(" ").replace(/\s+/g, " ").trim();
}

// the first paragraph of a run of tokens, as plain text
export function firstParagraph(tokens) {
  for (let i = 0; i < tokens.length; i++) {
    if (tokens[i].type === "paragraph_open" && tokens[i + 1]?.type === "inline") {
      const s = inlineText(tokens[i + 1]);
      if (s) return s;
    }
  }
  return "";
}
