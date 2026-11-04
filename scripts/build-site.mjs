import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";

const out = path.resolve(process.argv[2] || "_site");
const SITE = "https://openomsi.org";
if (out === process.cwd() || !path.relative(out, process.cwd()).startsWith("..") && path.relative(out, process.cwd()) !== "" || ["site", "docs", "assets"].some(d => out === path.resolve(d))) {
  console.error(`refusing to write the site into ${out}`);
  process.exit(1);
}

fs.rmSync(out, { recursive: true, force: true });
fs.cpSync("site", out, { recursive: true });
fs.mkdirSync(path.join(out, "docs"), { recursive: true });
for (const f of fs.readdirSync("docs")) if (f.endsWith(".md")) fs.copyFileSync(path.join("docs", f), path.join(out, "docs", f));
fs.cpSync("assets/logos", path.join(out, "assets/logos"), { recursive: true });
fs.copyFileSync("assets/icons/app/openomsi-256.png", path.join(out, "assets/icon.png"));
fs.copyFileSync("assets/icons/app/openomsi.svg", path.join(out, "assets/icon.svg"));

const sandbox = {};
vm.runInNewContext(fs.readFileSync("site/vendor/marked.min.js", "utf8"), sandbox);
const { marked } = sandbox;

const app = fs.readFileSync("site/app.js", "utf8");
const list = name => vm.runInNewContext(app.match(new RegExp(`const ${name} = (\\[[\\s\\S]*?\\n\\]);`))[1]);
const DOCS = list("DOCS");
const MAPPER_DOCS = list("MAPPER_DOCS");
const docSlug = file => file.toLowerCase().replace(/_/g, "-");
const docHref = (set, file) => `/${set === "mapper" ? "mapper/docs" : "docs"}/${docSlug(file)}/`;
const newPath = old => {
  const [p, a] = old.split("#");
  const m = p.match(/^\/(mapper\/)?docs\/([A-Za-z0-9_]+)$/);
  return (m ? docHref(m[1] ? "mapper" : "openomsi", m[2]) : p === "/" || !p ? "/" : p.replace(/\/?$/, "/")) + (a ? "#" + a : "");
};
const slug = t => t.toLowerCase().replace(/[^\w\- ]+/g, "").trim().replace(/ /g, "-");

const shell = fs.readFileSync("site/index.html", "utf8");
const template = id => shell.match(new RegExp(`<template id="${id}">([\\s\\S]*?)</template>`))[1];
const attr = t => t.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const text = html => {
  let t = html, before;
  do { before = t; t = t.replace(/<[^<>]*>/g, ""); } while (t !== before);
  return t.replace(/[<>]/g, "").replace(/&nbsp;/g, " ").replace(/&lt;/g, "‹").replace(/&gt;/g, "›").replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&amp;/g, "&").replace(/\s+/g, " ").trim();
};
const summary = html => {
  const first = (html.match(/<p>([\s\S]*?)<\/p>/) || [])[1] || "";
  const t = text(first);
  return t.length > 158 ? t.slice(0, 155).replace(/\s+\S*$/, "") + "…" : t;
};

const docLinks = ["Play", "Create", "Under the hood"].map(g =>
  `<div class="drawer-subheader">${g}</div><div class="drawer-section">${DOCS.filter(d => d.group === g).map(d =>
    `<a href="${docHref("openomsi", d.file)}" data-route="/docs/${docSlug(d.file)}"><span class="material-icons">${d.icon}</span>${d.title}</a>`).join("")}</div>`).join("");
const mapperLinks = MAPPER_DOCS.map(d =>
  `<a href="${docHref("mapper", d.file)}" data-route="/mapper/docs/${docSlug(d.file)}"><span class="material-icons">${d.icon}</span>${d.title}</a>`).join("");

function page({ route, title, description, view, home = false, section = "", noindex = false }) {
  const url = SITE + route;
  let html = shell
    .replace(/<title>[^<]*<\/title>/, () => `<title>${attr(title)}</title>`)
    .replace(/<meta name="description" content="[^"]*">/, () => `<meta name="description" content="${attr(description)}">`)
    .replace(/<link rel="canonical" href="[^"]*">/, () => noindex ? `<meta name="robots" content="noindex">` : `<link rel="canonical" href="${url}">`)
    .replace(/<meta property="og:url" content="[^"]*">/, () => `<meta property="og:url" content="${url}">`)
    .replace(/<meta property="og:title" content="[^"]*">/, () => `<meta property="og:title" content="${attr(title)}">`)
    .replace(/<meta property="og:description" content="[^"]*">/, () => `<meta property="og:description" content="${attr(description)}">`)
    .replace(/<meta name="twitter:title" content="[^"]*">/, () => `<meta name="twitter:title" content="${attr(title)}">`)
    .replace(/<meta name="twitter:description" content="[^"]*">/, () => `<meta name="twitter:description" content="${attr(description)}">`)
    .replace('<body data-page="home">', () => `<body data-page="${home ? "home" : "page"}">`)
    .replace('<div id="doc-links"></div>', () => `<div id="doc-links">${docLinks}</div>`)
    .replace(/(<div class="drawer-section" id="mapper-links">[\s\S]*?Map editor<\/a>)/, (m, start) => start + mapperLinks)
    .replace('<div id="view"></div>', () => `<div id="view" data-pre="${route}">${view}</div>`);
  if (!home) html = html.replace(/<script type="application\/ld\+json">[\s\S]*?<\/script>\n/g, "");
  if (section) {
    html = html.replace(`data-section="${section}">`, () => `data-section="${section}" class="active">`);
    html = html.replace(new RegExp(`<details class="drawer-group" data-sections="([^"]*\\b${section}\\b[^"]*)">`), (m, list) => `<details class="drawer-group" data-sections="${list}" open>`);
  }
  const file = route === "/404" ? path.join(out, "404.html") : path.join(out, route, "index.html");
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, html);
}

function docView(set, file, md) {
  const repo = set === "mapper" ? "openOMSI-org/openMapper" : "openOMSI-org/openOMSI";
  const raw = marked.parse(md)
    .replace(/<(h[1-4])>([\s\S]*?)<\/\1>/g, (all, tag, inner) => `<${tag} id="${slug(text(inner))}">${inner}</${tag}>`)
    .replace(/href="([^"]*)"/g, (all, href) => {
      if (href.startsWith("#/")) return `href="${newPath(href.slice(1))}"`;
      if (href.startsWith("#") || href.startsWith("/") || href.startsWith("mailto:")) return all;
      const site = href.match(/^https:\/\/openomsi\.org(\/.*)$/);
      if (site) return `href="${site[1].startsWith("/#/") ? newPath(site[1].slice(2)) : site[1]}"`;
      if (/^https?:/.test(href)) return all;
      const m = href.match(/^(?:\.\/)?([A-Z_]+)\.md(#.*)?$/);
      if (m && (set === "mapper" ? MAPPER_DOCS : DOCS).some(d => d.file === m[1])) return `href="${docHref(set, m[1])}${m[2] || ""}"`;
      return `href="https://github.com/${repo}/blob/main/docs/${href}"`;
    });
  return { raw, view: `<div class="content content-doc"><article class="doc" data-raw>${raw}</article><aside class="toc" aria-label="On this page"></aside></div>` };
}

const pages = [];
const add = p => { page(p); if (!p.noindex) pages.push(p.route); };

const loading = title => `<div class="content"><h1>${title}</h1><div class="loading">Loading…</div></div>`;
add({ route: "/", home: true, title: "openOMSI: OMSI 2 rebuilt from scratch, free and open source",
  description: "openOMSI recreates the bus simulator OMSI 2 in Rust: your OMSI 2 maps, buses and mods on a new 64-bit engine. Free and open source, for Windows, macOS, Linux and Android.",
  view: template("home") });
add({ route: "/download/", section: "download", title: "Download openOMSI for Windows, macOS, Linux and Android",
  description: "Download openOMSI, the free OMSI 2 remake: setup for Windows, dmg for macOS, Linux and Android builds and the dedicated server. Every download is checked on VirusTotal.",
  view: template("download") });
add({ route: "/beta/", section: "download", title: "Beta downloads · openOMSI",
  description: "Test the next version of openOMSI: the newest beta for Windows, macOS, Linux and Android.",
  view: template("download").replace("<h1>Download openOMSI</h1>", () => "<h1>Beta downloads</h1>").replace('id="dl-kicker">Download<', () => 'id="dl-kicker">Beta channel<') });
add({ route: "/security/", section: "security", title: "Is openOMSI safe? Security and VirusTotal checks · openOMSI",
  description: "How openOMSI keeps you safe: built in public, checked on every change, every download scanned on VirusTotal, why Windows may warn and how to check a download yourself.",
  view: template("security") });
add({ route: "/releases/", section: "releases", title: "Stable releases · openOMSI", description: "Every stable version of openOMSI with its release notes and downloads.", view: loading("Stable releases") });
add({ route: "/releases/beta/", section: "releases", title: "Beta releases · openOMSI", description: "Every beta of openOMSI with its release notes and downloads.", view: loading("Beta releases") });
add({ route: "/issues/", section: "issues", title: "Issues · openOMSI", description: "Bugs and wishes reported for openOMSI.", view: loading("Issues") });

for (const d of DOCS) {
  const { raw, view } = docView("openomsi", d.file, fs.readFileSync(`docs/${d.file}.md`, "utf8"));
  const section = d.file === "OPENMAPPER" ? "mapper" : "docs";
  add({ route: docHref("openomsi", d.file), section, title: `${d.title} · openOMSI docs`, description: summary(raw) || `${d.title}: openOMSI documentation.`, view });
}
for (const d of MAPPER_DOCS) {
  const r = await fetch(`https://raw.githubusercontent.com/openOMSI-org/openMapper/main/docs/${d.file}.md`).catch(() => null);
  if (!r?.ok) { console.warn(`openMapper's ${d.file}.md could not be read: its page is left to the browser`); continue; }
  const { raw, view } = docView("mapper", d.file, await r.text());
  add({ route: docHref("mapper", d.file), section: "mapper", title: `${d.title} · openMapper`, description: summary(raw) || `${d.title}: openMapper documentation.`, view });
}
add({ route: "/404", noindex: true, title: "Not found · openOMSI", description: "This page does not exist.", view: template("notfound") });

pages.push("/privacy/", "/legal/");
fs.writeFileSync(path.join(out, "sitemap.xml"), `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${pages.map(p => `  <url><loc>${SITE}${p}</loc></url>`).join("\n")}\n</urlset>\n`);
console.log(`built ${pages.length} pages into ${out}`);
