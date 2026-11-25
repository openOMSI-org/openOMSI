// Builds the website into site/dist:  node build.mjs   (or: npm run build)
//
// Inputs: ../docs/*.md (the documentation), openMapper's own docs (fetched; cached in .cache/),
//         static/ (CSS, JS, fonts), ../assets/ (logos, icon), privacy/index.html (the policy text).
// Output: plain HTML files with real addresses (/docs/user-guide/), a search index and a sitemap.
// No bundler, no framework: the pages work without JavaScript, which only adds search, filters and
// the live parts (downloads, releases, issues, players).

import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { fileURLToPath } from "node:url";
import { MAPPER_DOCS, MAPPER_REPO, MOVED_DOCS, REPO, MOVED_PAGES, PLATFORMS, SERVERS, SITE_URL, LEGACY_ROUTES } from "./src/config.mjs";
import { loadDocs } from "./src/docs.mjs";
import { cleanHtml, loadGithub } from "./src/github.mjs";
import { createMarkdown } from "./src/md.mjs";
import { u } from "./src/layout.mjs";
import {
  docPage, docsIndexPage, downloadPage, homePage, issuesPage, notFoundPage, privacyPage, releasesPage,
} from "./src/pages.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "..");
const out = path.join(here, "dist");
const cacheDir = path.join(here, ".cache");
const check = process.argv.includes("--check");

const write = (rel, data) => {
  const p = path.join(out, rel);
  fs.mkdirSync(path.dirname(p), { recursive: true });
  fs.writeFileSync(p, data);
};
const copyDir = (from, to) => fs.cpSync(from, path.join(out, to), { recursive: true });

fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(out, { recursive: true });

// ---- static assets (their hash is the cache-busting version)
copyDir(path.join(here, "static"), "assets");
copyDir(path.join(root, "assets/logos"), "assets/logos");
fs.copyFileSync(path.join(root, "assets/icons/app/openomsi-256.png"), path.join(out, "assets/icon.png"));
fs.copyFileSync(path.join(root, "assets/icons/app/openomsi.svg"), path.join(out, "assets/icon.svg"));
const hash = crypto.createHash("sha1");
for (const f of ["css/site.css", "js/site.js", "js/dynamic.js", "js/theme.js", "js/nav.js"]) hash.update(fs.readFileSync(path.join(here, "static", f)));
const v = hash.digest("hex").slice(0, 8);

// ---- the builds, for the scripts of the download list
write("assets/js/data.js", `// (generated from site/src/config.mjs)\nwindow.OMSI = ${JSON.stringify({ PLATFORMS, SERVERS })};\n`);

// ---- openMapper's documents come from its own repository
async function mapperDocs() {
  const result = {};
  fs.mkdirSync(cacheDir, { recursive: true });
  for (const d of MAPPER_DOCS) {
    const cached = path.join(cacheDir, `mapper-${d.file}.md`);
    try {
      const r = await fetch(`https://raw.githubusercontent.com/${MAPPER_REPO}/main/docs/${d.file}.md`, { signal: AbortSignal.timeout(15000) });
      if (!r.ok) throw new Error(String(r.status));
      result[d.file] = await r.text();
      fs.writeFileSync(cached, result[d.file]);
    } catch (e) {
      if (fs.existsSync(cached)) { result[d.file] = fs.readFileSync(cached, "utf8"); console.warn(`  ! openMapper ${d.file}.md: ${e.message}, using the cached copy`); }
      else console.warn(`  ! openMapper ${d.file}.md: ${e.message}, skipped`);
    }
  }
  return result;
}

console.log("Reading the documentation…");
const mapperText = await mapperDocs();
const { docs, groups, order } = loadDocs({ docsDir: path.join(root, "docs"), mapperText });

const ctx = { v, groups };

// ---- GitHub's releases and issues, baked in (see src/github.mjs)
console.log("Reading releases and issues from GitHub…");
const gh = await loadGithub(cacheDir);
if (gh) {
  // release notes link the repository's files: a document goes to its page here (its anchors are
  // dropped, the document may have been rewritten since), anything else to GitHub
  const byFile = new Map(docs.map((d) => [d.file, d]));
  const notes = createMarkdown({
    resolveLink: (href) => {
      if (/^(https?:|mailto:|#)/.test(href)) return { href, external: /^https?:/.test(href) };
      const m = href.match(/^(?:\.\.\/|\.\/)*docs\/([A-Za-z0-9_-]+)\.md(?:#.*)?$/);
      const doc = m && (byFile.get(m[1]) || docs.find((d) => d.slug === MOVED_DOCS[m[1].toLowerCase().replace(/_/g, "-")]));
      if (doc) return { href: u(doc.url) };
      return { href: `https://github.com/${REPO}/blob/main/${href.replace(/^(\.\.\/|\.\/)+/, "")}`, external: true };
    },
  });
  notes.set({ linkify: true });
  const full = [];
  const releases = gh.releases.map((r) => {
    const body_html = cleanHtml(notes.render(r.body, { doc: null }));
    write(`assets/data/releases/${r.tag_name}.json`, JSON.stringify({ html_url: r.html_url, assets: r.assets, body_html }));
    const { body, assets, ...meta } = r;
    full.push({ ...meta, assets, body_html });
    return meta;
  });
  write("assets/data/github.json", JSON.stringify({ generated: gh.fetched, latest: gh.latest, releases, issues: gh.issues }));
  // the pages are written with all of it in place: nothing is waited for when a page opens
  ctx.gh = { generated: gh.fetched, latest: gh.latest, releases: full, issues: gh.issues };
}

// ---- pages
const pages = [];
const emit = (urlPath, html, { sitemap = true } = {}) => {
  write(path.join(urlPath.replace(/^\//, ""), "index.html"), html);
  if (sitemap) pages.push(urlPath);
};
emit("/", homePage(ctx));
emit("/download/", downloadPage(false, ctx));
emit("/download/beta/", downloadPage(true, ctx), { sitemap: false });
emit("/releases/", releasesPage(false, ctx));
emit("/releases/beta/", releasesPage(true, ctx), { sitemap: false });
emit("/issues/", issuesPage(ctx));
emit("/docs/", docsIndexPage(groups, ctx));
for (const p of order) emit(p.url, docPage(p, ctx));

// documents that were renamed or merged: their old addresses (and the old chapters under them) lead on
const redirect = (to) => `<!doctype html><html lang="en"><head><meta charset="utf-8"><title>Moved</title><meta name="robots" content="noindex"><meta http-equiv="refresh" content="0; url=${to}"><link rel="canonical" href="${SITE_URL}${to}"></head><body><p>This page moved to <a href="${to}">${to}</a>.</p></body></html>\n`;
for (const [from, to] of Object.entries(MOVED_DOCS)) {
  const target = u(`docs/${to}/`);
  write(`docs/${from}/index.html`, redirect(target));
}
for (const [from, to] of Object.entries(MOVED_PAGES)) write(`docs/${from}/index.html`, redirect(u(`docs/${to}/`)));

// the privacy policy: its own text, in the site's frame
const privacySrc = fs.readFileSync(path.join(here, "privacy/index.html"), "utf8");
const inner = privacySrc.match(/<main>([\s\S]*)<\/main>/)[1].replace(/^\s*<p><a href="\.\.\/">openOMSI<\/a><\/p>\s*/, "");
emit("/privacy/", privacyPage(inner, ctx));
write("404.html", notFoundPage(ctx));

// ---- documents served as they are
fs.copyFileSync(path.join(root, "docs/plugin-api.json"), (fs.mkdirSync(path.join(out, "docs"), { recursive: true }), path.join(out, "docs/plugin-api.json")));

// ---- legacy hash addresses: the home page's script reads this table
const legacy = { routes: LEGACY_ROUTES, docs: {}, anchors: {} };
for (const d of docs) {
  legacy.docs[d.remote === "mapper" ? `mapper/${d.file}` : d.file] = d.url;
  if (d.pages.length > 1) for (const [id, p] of d.anchors) if (id && p.url !== d.url) (legacy.anchors[d.remote === "mapper" ? `mapper/${d.file}` : d.file] ??= {})[id] = p.url;
}
for (const [file, slug] of Object.entries({ USER_GUIDE: "user-guide", NVIDIA_DLSS: "nvidia-dlss", WEATHER_REGIONS: "weather-regions", OPENMAPPER: "openmapper", "mapper/TUTORIAL": "mapper-tutorial", "mapper/FEATURES": "mapper-features", "mapper/ROADMAP": "mapper-roadmap" }))
  legacy.docs[file] ??= `/docs/${MOVED_DOCS[slug]}/`;
write("assets/js/legacy.js", `// (generated) #/docs/NAME#anchor and the other old hash addresses lead to their new pages
(function () {
  var h = location.hash;
  if (h.indexOf("#/") !== 0) return;
  var L = ${JSON.stringify(legacy)};
  var base = ${JSON.stringify(u(""))};
  var parts = h.slice(1).split("#"), path = parts[0].replace(/\\/+$/, ""), anchor = parts[1] ? "#" + parts[1] : "";
  var to = null, m;
  if ((m = path.match(/^\\/docs\\/([^\\/]+)$/)) || (m = path.match(/^\\/mapper\\/docs\\/([^\\/]+)$/))) {
    var key = path.indexOf("/mapper/") === 0 ? "mapper/" + m[1] : m[1];
    to = L.docs[key];
    if (to && parts[1] && L.anchors[key] && L.anchors[key][parts[1]]) to = L.anchors[key][parts[1]];
  } else if ((m = path.match(/^\\/issues\\/(\\d+)$/))) {
    to = "/issues/?n=" + m[1]; anchor = "";
  } else if (L.routes[path]) to = L.routes[path];
  if (to) location.replace(base + to.replace(/^\\//, "") + anchor);
})();
`);

// ---- search index: one record per section of every page
const index = [];
for (const p of order) {
  for (const s of p.sections) {
    if (!s.text && !s.head) continue;
    index.push([p.url + (s.id && s.id !== p.id ? `#${s.id}` : ""), p.doc.title, p.title === p.doc.title ? "" : p.title, s.head === p.title && !s.row ? "" : s.head, s.text.slice(0, s.row ? 300 : 1400)]);
  }
}
write("assets/search-index.json", JSON.stringify(index));

// ---- sitemap and robots
write("sitemap.xml", `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${pages.map((p) => `<url><loc>${SITE_URL}${p}</loc></url>`).join("\n")}\n</urlset>\n`);
write("robots.txt", `User-agent: *\nAllow: /\nSitemap: ${SITE_URL}/sitemap.xml\n`);

// ---- link check: every internal href must lead to a file (and an anchor to an id)
function walk(dir) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(path.join(dir, e.name)) : [path.join(dir, e.name)]));
}
const htmlFiles = walk(out).filter((f) => f.endsWith(".html"));
const idsOf = new Map(), bad = [];
for (const f of htmlFiles) idsOf.set(f, new Set([...fs.readFileSync(f, "utf8").matchAll(/\sid="([^"]+)"/g)].map((m) => m[1])));
for (const f of htmlFiles) {
  const html = fs.readFileSync(f, "utf8");
  for (const m of html.matchAll(/<a [^>]*href="([^"]+)"/g)) {
    const href = m[1];
    if (/^(https?:|mailto:|javascript:)/.test(href)) continue;
    const [p0, anchor] = href.replace(/\?[^#]*/, "").split("#");
    let target;
    if (!p0) target = f;
    else {
      const rel = p0.startsWith("/") ? p0.slice(1) : path.posix.join(path.relative(out, path.dirname(f)).split(path.sep).join("/"), p0);
      const abs = path.join(out, rel);
      target = fs.existsSync(abs) && fs.statSync(abs).isDirectory() ? path.join(abs, "index.html") : abs;
    }
    if (!fs.existsSync(target)) { bad.push(`${path.relative(out, f)}: ${href} (no such page)`); continue; }
    if (anchor && target.endsWith(".html") && !idsOf.get(target)?.has(anchor) && !/^\/?(issues|releases)\/?$/.test(p0 || "")) bad.push(`${path.relative(out, f)}: ${href} (no such anchor)`);
  }
}
const uniq = [...new Set(bad)];
console.log(`Wrote ${htmlFiles.length} pages, ${order.length} of them documentation (${docs.length} documents), index of ${index.length} sections.`);
if (uniq.length) {
  console.warn(`\n${uniq.length} broken link(s):`);
  for (const b of uniq.slice(0, 60)) console.warn("  " + b);
  if (check) process.exit(1);
}
