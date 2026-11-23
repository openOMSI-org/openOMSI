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
  md.renderer.rules.paragraph_open = (tokens, i) => {
    const c = tokens[i].meta?.callout;
    if (!c) return tokens[i].hidden ? "" : "<p>";
    return `<aside class="callout callout-${CALLOUT_KIND[c]}"><div class="callout-title">${CALLOUTS[c]}</div><div class="callout-body"><p>`;
  };
  md.renderer.rules.paragraph_close = (tokens, i) => {
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
