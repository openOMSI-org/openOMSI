// The documentation as a tree of pages: every docs/*.md is read once, its headings get their
// GitHub ids, a long document is cut into one page per chapter, and every link between documents is
// resolved to the page that now holds its target.

import fs from "node:fs";
import path from "node:path";
import {
  DOC_GROUPS, MAPPER_DOCS, MAPPER_GROUP, MAPPER_REPO, REPO, SPLIT_BYTES, SPLIT_CHAPTER_BYTES,
} from "./config.mjs";
import { createMarkdown, firstParagraph, makeSlugger, plainText } from "./md.mjs";

export const slugOf = (file) => file.toLowerCase().replace(/_/g, "-");

const headingText = (tokens, i) => tokens[i + 1].content.replace(/[`*]/g, "").trim();

// the address of a chapter: its heading id, tidied and kept short
function pageSlug(doc, id) {
  let s = id.replace(/-{2,}/g, "-").replace(/^-|-$/g, "");
  if (s.length > 56) s = s.slice(0, 56).replace(/-[^-]*$/, "");
  const used = (doc.slugs ??= new Set());
  let out = s || "section", n = 2;
  while (used.has(out)) out = `${s}-${n++}`;
  used.add(out);
  return out;
}

function lineBytes(lines, from, to) {
  let n = 0;
  for (let i = from; i < to; i++) n += lines[i].length + 1;
  return n;
}

// Cut one parsed document into pages.
function buildPages(doc, tokens, lines) {
  const top = [];
  tokens.forEach((t, i) => {
    if (t.type === "heading_open" && t.level === 0) top.push({ i, level: Number(t.tag.slice(1)), id: t.attrGet("id"), title: headingText(tokens, i), line: t.map[0] });
  });
  const h1 = top.find((h) => h.level === 1);
  const afterH1 = h1 ? h1.i + 3 : 0;
  const h2s = top.filter((h) => h.level === 2);
  const split = doc.bytes > SPLIT_BYTES && h2s.length >= 3;
  const mk = (p) => Object.assign({ doc, children: [], parent: null }, p);

  if (!split) {
    // one page: everything after the H1
    doc.pages = [mk({ kind: "single", level: 1, slug: "", id: h1?.id, title: doc.title, tokens: tokens.slice(afterH1), shift: 0 })];
    return;
  }
  const firstH2 = h2s[0].i;
  const overview = mk({ kind: "overview", level: 1, slug: "", id: h1?.id, title: doc.title, tokens: tokens.slice(afterH1, firstH2), shift: 0 });
  doc.pages = [overview];
  h2s.forEach((h2, n) => {
    const next = h2s[n + 1];
    const end = next ? next.i : tokens.length;
    const bytes = lineBytes(lines, h2.line, next ? next.line : lines.length);
    const h3s = top.filter((h) => h.level === 3 && h.i > h2.i && h.i < end);
    const body = tokens.slice(h2.i + 3, end);
    const chapter = mk({ kind: "chapter", level: 2, slug: pageSlug(doc, h2.id), id: h2.id, title: h2.title, tokens: body, shift: 1, parent: overview });
    overview.children.push(chapter);
    doc.pages.push(chapter);
    if (bytes > SPLIT_CHAPTER_BYTES && h3s.length >= 3) {
      chapter.tokens = tokens.slice(h2.i + 3, h3s[0].i);
      h3s.forEach((h3, m) => {
        const nx = h3s[m + 1];
        const sec = mk({ kind: "section", level: 3, slug: pageSlug(doc, h3.id), id: h3.id, title: h3.title, tokens: tokens.slice(h3.i + 3, nx ? nx.i : end), shift: 2, parent: chapter });
        chapter.children.push(sec);
        doc.pages.push(sec);
      });
    }
  });
}

export function loadDocs({ docsDir, mapperText }) {
  const docs = [];
  const byFile = new Map();
  const groups = [];

  const addDoc = (g, meta, src, remote) => {
    const doc = {
      file: meta.file, slug: meta.slug || slugOf(meta.file), group: g, remote,
      title: meta.title, summary: meta.summary, bytes: src.length, src,
      url: `/docs/${meta.slug || slugOf(meta.file)}/`, pages: [], anchors: new Map(), headings: [],
    };
    docs.push(doc); byFile.set(meta.file, doc); g.items.push(doc);
    return doc;
  };

  for (const g of [...DOC_GROUPS, MAPPER_GROUP]) {
    const group = { id: g.id, title: g.title, blurb: g.blurb, items: [] };
    groups.push(group);
    if (g.id === "mapper") {
      for (const m of MAPPER_DOCS) {
        const text = mapperText?.[m.file];
        if (text == null) continue;
        addDoc(group, m, text, "mapper");
      }
      continue;
    }
    for (const m of g.docs) {
      const p = path.join(docsDir, `${m.file}.md`);
      if (!fs.existsSync(p)) { console.warn(`  ! docs/${m.file}.md is listed but missing`); continue; }
      addDoc(group, m, fs.readFileSync(p, "utf8"));
    }
  }
  // OPENMAPPER lives in the content group; the editor's own documents follow it in the menu
  const mapperIdx = groups.findIndex((g) => g.id === "mapper");
  if (groups[mapperIdx].items.length === 0) groups.splice(mapperIdx, 1);

  // a docs/*.md nobody listed is still published (under "Notes"), so no document is lost
  const known = new Set(docs.filter((d) => !d.remote).map((d) => d.file));
  const strays = fs.readdirSync(docsDir).filter((f) => f.endsWith(".md")).map((f) => f.slice(0, -3)).filter((f) => !known.has(f));
  if (strays.length) {
    const notes = groups.find((g) => g.id === "notes");
    for (const f of strays) {
      console.warn(`  ! docs/${f}.md is not in site/src/config.mjs: published under "Notes and licensing"`);
      addDoc(notes, { file: f, title: f.replace(/[_-]/g, " ") }, fs.readFileSync(path.join(docsDir, `${f}.md`), "utf8"));
    }
  }

  // ---- pass 1: parse, ids, pages
  const resolveLink = makeLinkResolver({ docsDir, byFile });
  const md = createMarkdown({ resolveLink });
  for (const doc of docs) {
    const env = { doc };
    const tokens = md.parse(doc.src, env);
    const slug = makeSlugger();
    for (let i = 0; i < tokens.length; i++) {
      if (tokens[i].type !== "heading_open") continue;
      const id = slug(headingText(tokens, i));
      tokens[i].attrSet("id", id);
    }
    const h1 = tokens.find((t) => t.type === "heading_open" && t.tag === "h1" && t.level === 0);
    const h1Text = h1 ? headingText(tokens, tokens.indexOf(h1)) : doc.file;
    doc.h1 = h1Text;
    doc.title = doc.title || h1Text;
    const lines = doc.src.split("\n");
    buildPages(doc, tokens, lines);
    // the lead of a document: its first paragraph, unless the config gives a better line
    const lead = firstParagraph(doc.pages[0].tokens);
    doc.lead = lead;
    doc.summary = doc.summary || lead;
    doc.words = doc.src.split(/\s+/).length;
    // urls of pages, and the page every heading ended up on
    for (const p of doc.pages) {
      p.url = p.slug ? `${doc.url}${p.slug}/` : doc.url;
      p.doc = doc;
    }
    for (const p of doc.pages) {
      // headings inside this page (id -> this page)
      for (const t of p.tokens) if (t.type === "heading_open") doc.anchors.set(t.attrGet("id"), p);
      if (p.id) doc.anchors.set(p.id, p);
      p.headings = p.tokens
        .map((t, i) => (t.type === "heading_open" && t.level === 0 ? { level: Number(t.tag.slice(1)), id: t.attrGet("id"), text: headingText(p.tokens, i) } : null))
        .filter(Boolean);
    }
    doc.anchors.set(doc.pages[0].id ?? "", doc.pages[0]);
  }

  // ---- pass 2: HTML
  for (const doc of docs) {
    for (const p of doc.pages) {
      const env = { doc, shift: p.shift };
      p.html = md.renderer.render(p.tokens, md.options, env);
      p.text = plainText(p.tokens);
      p.sections = sectionsOf(p);
    }
  }

  // reading order across the whole documentation
  const order = [];
  for (const g of groups) for (const d of g.items) for (const p of d.pages) order.push(p);
  order.forEach((p, i) => { p.prev = order[i - 1] || null; p.next = order[i + 1] || null; });
  return { docs, groups, order, byFile };
}

// A page's text, cut at its headings, for the search index. A table row is a record of its own:
// the flags and the API functions are looked up by name.
function sectionsOf(page) {
  const out = [];
  let head = page.title, buf = [], id = page.id;
  const flush = () => {
    const rest = [], rows = [];
    let inTable = false, cells = null;
    for (const t of buf) {
      if (t.type === "table_open") inTable = true;
      if (t.type === "table_close") { inTable = false; continue; }
      if (!inTable) { rest.push(t); continue; }
      if (t.type === "tr_open") cells = [];
      else if (t.type === "tr_close" && cells?.length) { rows.push(cells); cells = null; }
      else if (t.type === "inline" && cells) cells.push(plainText([t]));
    }
    const text = plainText(rest);
    if (text || head) out.push({ id, head, text });
    // (the first row of a table is its header)
    for (const [name, ...others] of rows.slice(1)) out.push({ id, head: name, text: others.join(" · "), row: true });
    buf = [];
  };
  for (let i = 0; i < page.tokens.length; i++) {
    const t = page.tokens[i];
    if (t.type === "heading_open" && t.level === 0) {
      flush();
      head = headingText(page.tokens, i);
      id = t.attrGet("id");
      i += 2;
    } else buf.push(t);
  }
  flush();
  return out;
}

function makeLinkResolver({ docsDir, byFile }) {
  const repoRoot = path.resolve(docsDir, "..");
  return (href, env) => {
    const doc = env.doc;
    if (/^(https?:|mailto:|tel:)/i.test(href)) return { href, external: true };
    // images of openMapper's documents live in its repository
    if (doc?.remote === "mapper" && !href.startsWith("#")) {
      const m = href.match(/^(?:\.\/)?([A-Za-z0-9_\-]+)\.md(?:#(.*))?$/);
      if (m) {
        const target = byFile.get(m[1]);
        if (target?.remote === "mapper") return { href: target.url + (m[2] ? "#" + m[2] : "") };
      }
      return { href: `https://github.com/${MAPPER_REPO}/blob/main/docs/${href}`, external: true };
    }
    const [file, anchor] = href.split("#");
    const pageFor = (d, a) => {
      if (!a) return d.url;
      const p = d.anchors.get(a);
      return p ? `${p.url}#${a}` : d.url;
    };
    if (!file) return { href: doc ? pageFor(doc, anchor) : href };
    const m = file.match(/^(?:\.\/)?([A-Za-z0-9_\-]+)\.md$/);
    if (m && byFile.has(m[1])) return { href: pageFor(byFile.get(m[1]), anchor) };
    const abs = path.resolve(docsDir, file);
    const rel = path.relative(repoRoot, abs).split(path.sep).join("/");
    if (rel === "docs/plugin-api.json") return { href: "/docs/plugin-api.json" };
    const exists = fs.existsSync(abs);
    const kind = exists && fs.statSync(abs).isDirectory() ? "tree" : "blob";
    return { href: `https://github.com/${REPO}/${kind}/main/${rel}${anchor ? "#" + anchor : ""}`, external: true };
  };
}
