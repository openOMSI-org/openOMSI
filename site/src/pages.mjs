// The pages themselves, as plain template strings.

import { DISCORD, DONATE, PLATFORMS, REPO, SERVERS } from "./config.mjs";
import { esc } from "./md.mjs";
import { pageShell, u } from "./layout.mjs";
import { downloadLine, downloadRows, homeDownload, issueList, latestBeta, latestStable, releaseList } from "./releases.mjs";

const NEEDS_OMSI = `<b>openOMSI needs an installed copy of the original OMSI&nbsp;2.</b> It contains no game content: it plays on the
maps, vehicles and files of your OMSI&nbsp;2 and does not start without one. Your OMSI&nbsp;2 folder is never written to.`;

// ------------------------------------------------------------------------------------------------
export function homePage(ctx) {
  const { groups } = ctx;
  const dl = homeDownload(ctx.gh);
  const docLinks = groups.filter((g) => g.id !== "notes").map((g) => `<div><h3>${esc(g.title)}</h3><ul>${g.items.slice(0, 4).map((d) => `<li><a href="${u(d.url)}">${esc(d.title)}</a></li>`).join("")}</ul></div>`).join("");
  const body = `<main id="main" class="wrap home-main" data-latest="${esc(ctx.gh?.latest || "")}">
<div class="home-text">
  <header class="intro">
    <h1 class="wordmark"><img class="on-dark" src="${u("assets/logos/openomsi-wordmark-light.svg")}" alt="openOMSI" height="52"><img class="on-light" src="${u("assets/logos/openomsi-wordmark-dark.svg")}" alt="" height="52"></h1>
    <p class="lead">A from-scratch recreation of the bus simulator OMSI&nbsp;2, written in Rust. It loads your OMSI&nbsp;2 maps, buses and mods as they are, on a 64-bit, multithreaded engine for Windows, macOS, Linux and Android.</p>
    <p class="status"><span id="home-version">${dl.version}</span><span id="home-players" hidden></span><span class="warn">Early development: expect bugs</span></p>
    <aside class="notice" role="note"><p>${NEEDS_OMSI}</p></aside>
  </header>

  <section class="about" aria-labelledby="h-about">
    <h2 id="h-about">What it does</h2>
    <ul class="about-list">
      <li><b>Runs your content as it is.</b> Maps, splines, scenery, buses, scripts, timetables, HOF files, weather, tickets and plugins load the way OMSI&nbsp;2.2.032 loads them. Existing mods need no conversion, and the OMSI&nbsp;2 folder is never written to. <a href="${u("docs/formats/")}">File formats</a></li>
      <li><b>A new engine.</b> 64-bit with no 2&nbsp;GB limit, tiles and textures loaded on worker threads, Metal, Vulkan and DirectX&nbsp;12. Four graphics modes, from Vanilla, which looks like OMSI&nbsp;2, to Enhanced+ with physically based light, ray-traced shadows and reflections. <a href="${u("docs/architecture/")}">Architecture</a></li>
      <li><b>A real launcher.</b> Map, line, tour and bus on one screen with a route map and a live bus preview. Mods from a folder or an archive, wheels and pedals with force feedback, updates on a Stable or Beta channel. <a href="${u("docs/launcher/")}">The launcher</a></li>
      <li><b>Driving as in OMSI&nbsp;2.</b> Timetables, tickets, passengers and AI traffic as the original runs them, a navigator in the cab, a city map, a photo mode and a remote cockpit on a phone or tablet. <a href="${u("docs/driving/")}">Driving</a></li>
      <li><b>Trams, metros, trains and trolleybuses.</b> Rail vehicles on their own tracks and a real overhead wire. <a href="${u("docs/rail/")}">Rail</a>, <a href="${u("docs/trolleybus/")}">trolleybuses</a></li>
      <li><b>Multiplayer.</b> LAN sessions, and a dedicated server that runs the whole map without a window. <a href="${u("docs/server/")}">Dedicated server</a></li>
      <li><b>Android and VR.</b> The full game on a phone with on-screen wheel or tilt steering and a cab panel; OpenXR headsets on Windows. <a href="${u("docs/android/")}">Android</a>, <a href="${u("docs/vr/")}">VR</a></li>
      <li><b>More for modders.</b> PBR materials, textures up to 16K, any number of interior lights, HTML textures, and plugins as Lua, compiled <code>.oop</code> (Lua or WebAssembly) or OMSI's own DLLs. <a href="${u("docs/modding/")}">Modding</a>, <a href="${u("docs/plugins/")}">plugins</a></li>
      <li><b>openMapper.</b> A map editor: draw roads, place buildings and trees, shape the ground and drive the map in openOMSI on one key. <a href="${u("docs/mapper/")}">openMapper</a></li>
    </ul>
  </section>

  <section class="community" aria-labelledby="h-comm">
    <h2 id="h-comm">Community and support</h2>
    <p>Questions, screenshots and mods are on <a href="${DISCORD}">Discord</a>. Bugs and wishes go to <a href="${u("issues/")}">Issues</a>. The project is free and open source under the GNU AGPL v3; if you want to support its development: ${DONATE.map((d) => `<a href="${d.url}" rel="noopener">${esc(d.name)}</a>`).join(" or ")}.</p>
    <p>Also from the family: the <a href="https://github.com/openOMSI-org/openOMSI-Plugins">plugin catalog</a> of reviewed open source plugins and the <a href="https://github.com/openOMSI-org/openOMSI-Development-Tools">Development Tools</a>, a workbench for plugins.</p>
  </section>
</div>

<div class="home-rail">
  <aside class="panel get" aria-labelledby="h-get">
    <h2 id="h-get">Download</h2>
    <div class="hero-dls" id="hero-download">${dl.button}</div>
    <p class="get-version muted" id="get-version">${dl.info}</p>
    <ul class="get-others" id="get-others">${dl.others}</ul>
    <p class="get-links"><a href="${u("download/")}">All downloads and installing</a> · <a href="${u("download/beta/")}">Beta</a> · <a href="https://github.com/${REPO}">Source code</a></p>
  </aside>

  <section class="panel" aria-labelledby="h-rel">
    <h2 id="h-rel">Latest release</h2>
    <div class="gh rel-notes" id="home-notes">${dl.notes}</div>
    <p class="get-links"><a href="${u("releases/")}">All release notes</a></p>
  </section>

  <section class="panel" aria-labelledby="h-docs">
    <h2 id="h-docs">Documentation</h2>
    <div class="doc-cols">${docLinks}</div>
    <p class="get-links"><a href="${u("docs/")}">All documents</a> · press <kbd>/</kbd> to search</p>
  </section>
</div>
</main>`;
  return pageShell({
    title: "",
    path: "/",
    body,
    bodyClass: "home",
    scripts: ["data.js", "dynamic.js"],
    head: `<script src="${u("assets/js/legacy.js")}"></script>\n`,
    assetVersion: ctx.v,
  });
}

// ------------------------------------------------------------------------------------------------
export function downloadPage(beta, ctx) {
  const rel = beta ? latestBeta(ctx.gh) : latestStable(ctx.gh);
  const line = rel ? downloadLine(rel, beta)
    : beta ? `There is no beta newer than the stable release. <a href="${u("download/")}">Download stable</a> or see the <a href="${u("releases/beta/")}">beta history</a>.`
    : `The release list could not be read. Every file is on <a href="https://github.com/${REPO}/releases/latest">GitHub</a>.`;
  const tabs = `<nav class="tabs" aria-label="Release channel"><a href="${u("download/")}"${beta ? "" : ' aria-current="page"'}>Stable</a><a href="${u("download/beta/")}"${beta ? ' aria-current="page"' : ""}>Beta</a></nav>`;
  const body = `<main id="main" class="wrap page" data-page="download" data-channel="${beta ? "beta" : "stable"}" data-latest="${esc(ctx.gh?.latest || "")}">
  <header class="page-head">
    <h1>${beta ? "Beta downloads" : "Download openOMSI"}</h1>
    <p class="lead" id="dl-lead">${beta
      ? `Betas are for testing new changes and may have regressions. For everyday use take the <a href="${u("download/")}">stable release</a>.`
      : `The stable release is for everyday use. To try upcoming changes, take a <a href="${u("download/beta/")}">beta</a>.`}</p>
    ${tabs}
  </header>

  <aside class="notice" role="note"><p>${NEEDS_OMSI} Install OMSI&nbsp;2 first.</p></aside>
  <aside class="notice notice-warn" role="note"><p><b>${beta ? "Beta: for testing." : "Early development: expect bugs."}</b>
  ${beta ? `This build may have regressions. Report problems with the version number on <a href="${u("issues/")}">Issues</a>. The launcher's update settings take you back to Stable.`
         : `Things may be missing or broken, and may change between versions. Report problems on <a href="${u("issues/")}">Issues</a>.`}</p></aside>

  <p class="release-line" id="dl-version" aria-live="polite">${line}</p>

  <section aria-labelledby="h-game">
    <h2 id="h-game">Game and launcher</h2>
    <div class="dl-list" id="dl-games">${rel ? downloadRows(rel, PLATFORMS) : ""}</div>
  </section>

  <section aria-labelledby="h-server">
    <h2 id="h-server">Dedicated server</h2>
    <p class="muted">Only for hosting a multiplayer session without playing on that machine. To play, take the game above. See <a href="${u("docs/server/")}">Dedicated server</a>.</p>
    <div class="dl-list" id="dl-servers">${rel ? downloadRows(rel, SERVERS) : ""}</div>
  </section>

  <section aria-labelledby="install" class="install">
    <h2 id="install">Installing</h2>
    <div class="install-grid">
      <div id="os-windows"><h3>Windows</h3>
        <p>Run the setup: it installs openOMSI for you, with no administrator rights. The <i>Portable .zip</i> goes into a folder of your own (not <code>Program Files</code>); run <code>openomsi.exe</code> there.</p>
        <p>SmartScreen may warn about an unknown app: <i>More info</i>, then <i>Run anyway</i>. Almost every PC is <b>x64</b>; <b>ARM64</b> is only for Snapdragon laptops.</p></div>
      <div id="os-macos"><h3>macOS</h3>
        <p>Open the <code>.dmg</code>, drag <code>openOMSI.app</code> onto Applications and open it. <b>Apple silicon</b> is for M1 and newer, <b>Intel</b> for older Macs.</p>
        <p>If macOS refuses an app from the internet: right-click, <i>Open</i>, <i>Open</i>, or run once:</p>
        <figure class="code bare"><pre><code>xattr -dr com.apple.quarantine openOMSI.app</code></pre></figure></div>
      <div id="os-linux"><h3>Linux</h3>
        <p>Unpack and run <code>./openomsi</code> (<code>chmod +x openomsi</code> if it does not start). It needs Vulkan or OpenGL drivers and <b>WebKitGTK 4.1</b> for the launcher window:</p>
        <table class="plain"><tbody>
          <tr><td>Debian, Ubuntu</td><td><code>libwebkit2gtk-4.1-0</code></td></tr>
          <tr><td>Fedora</td><td><code>webkit2gtk4.1</code></td></tr>
          <tr><td>Arch</td><td><code>webkit2gtk-4.1</code></td></tr></tbody></table></div>
      <div id="os-android"><h3>Android</h3>
        <p>Install the <code>.apk</code>, allow access to all files and copy the whole OMSI&nbsp;2 folder to <code>openOMSI/OMSI 2</code> on the phone. See <a href="${u("docs/android/")}">Android</a>.</p></div>
    </div>
    <h3>Mods and updates</h3>
    <ul class="tight">
      <li>Install mods on the launcher's <b>Mods</b> page (a folder, <code>.zip</code>, <code>.7z</code> or <code>.rar</code>) or put them into the <code>Mods</code> folder next to the game. The original OMSI&nbsp;2 folder is never changed.</li>
      <li>The launcher offers new versions at the start. <b>Settings, General, Updates</b> chooses Stable or Beta. If it offers nothing, download the file here or from <a id="dl-github" href="${rel?.html_url || `https://github.com/${REPO}/releases/latest`}">GitHub</a>.</li>
      <li>Each release lists the checksums of all its files in <code>SHA256SUMS</code>.</li>
      <li>Versions are numbered <code>MAJOR.MINOR.PATCH</code>: see <a href="${u("docs/versioning/")}">Versioning</a>. All versions: <a href="${u("releases/")}">Releases</a>.</li>
    </ul>
  </section>
</main>`;
  return pageShell({
    title: beta ? "Beta downloads" : "Download",
    description: beta ? "Beta builds of openOMSI for testing." : "Download openOMSI for Windows, macOS, Linux and Android.",
    path: beta ? "/download/beta/" : "/download/",
    body,
    scripts: ["data.js", "dynamic.js"],
    assetVersion: ctx.v,
  });
}

// ------------------------------------------------------------------------------------------------
export function releasesPage(beta, ctx) {
  const body = `<main id="main" class="wrap page" data-page="releases" data-channel="${beta ? "beta" : "stable"}">
  <header class="page-head">
    <h1>${beta ? "Beta releases" : "Stable releases"}</h1>
    <p class="lead">${beta ? `Versions for testing, including older betas. <a href="${u("download/beta/")}">Download the latest beta</a>.`
      : `Stable versions, newest first. <a href="${u("download/")}">Download the latest stable release</a>.`}</p>
    <nav class="tabs" aria-label="Release channel"><a href="${u("releases/")}"${beta ? "" : ' aria-current="page"'}>Stable</a><a href="${u("releases/beta/")}"${beta ? ' aria-current="page"' : ""}>Beta</a></nav>
  </header>
  <div id="rel-list" class="rel-list">${releaseList(ctx.gh, beta)}</div>
</main>`;
  return pageShell({
    title: beta ? "Beta releases" : "Releases",
    description: "Every openOMSI version with its release notes and files.",
    path: beta ? "/releases/beta/" : "/releases/",
    body, scripts: ["data.js", "dynamic.js"], assetVersion: ctx.v,
  });
}

export function issuesPage(ctx) {
  const body = `<main id="main" class="wrap page" data-page="issues">
  <header class="page-head">
    <h1>Issues</h1>
    <p class="lead">Bugs and wishes, read from GitHub. To report one you need a GitHub account.</p>
    <p><a class="btn primary" href="https://github.com/${REPO}/issues/new">New issue</a></p>
  </header>
  <div id="issues-root"><div class="issue-bar"><div class="tabs" hidden><button type="button" data-s="open" class="on">Open</button><button type="button" data-s="closed">Closed</button><button type="button" data-s="all">All</button></div>
    <input id="iq" type="search" placeholder="Filter by title, label or number" aria-label="Filter issues" hidden></div>
    <div id="issue-list" data-state="open">${issueList(ctx.gh)}</div><p class="muted" id="issue-empty" hidden>Nothing here.</p></div>
</main>`;
  return pageShell({
    title: "Issues",
    description: "Open and closed bugs and requests of openOMSI.",
    path: "/issues/",
    body, scripts: ["data.js", "dynamic.js"], assetVersion: ctx.v,
  });
}

// ------------------------------------------------------------------------------------------------
export function notFoundPage(ctx) {
  const body = `<main id="main" class="wrap page">
  <header class="page-head">
    <h1>There is nothing at this address</h1>
    <p class="lead">The page may have moved when the site was rebuilt. Search the documentation, or start from the list.</p>
    <p class="actions"><button class="btn primary" type="button" data-search-open hidden>Search the docs</button>
    <a class="btn" href="${u("docs/")}">All documentation</a><a class="btn quiet" href="${u("")}">Home</a></p>
  </header>
</main>`;
  return pageShell({ title: "Not found", path: "/404.html", body, noindex: true, assetVersion: ctx.v });
}

export function privacyPage(inner, ctx) {
  const body = `<main id="main" class="wrap page prose-page"><article class="doc">${inner}</article></main>`;
  return pageShell({
    title: "Privacy",
    description: "How openOMSI handles information in the game, on openomsi.org and in its community services.",
    path: "/privacy/", body, assetVersion: ctx.v,
  });
}

// ------------------------------------------------------------------------------------------------
// Documentation

export function docsIndexPage(groups, ctx) {
  const cards = (g) =>
    g.items.map((d) => {
      const chapters = d.pages.length > 1 ? `${d.pages.length - 1} chapters` : "";
      return `<li><a class="doc-card" href="${u(d.url)}"><b>${esc(d.title)}</b><span>${esc(d.summary)}</span><small>${[chapters, `${readMin(d)} min read`].filter(Boolean).join(" · ")}</small></a></li>`;
    }).join("");
  const sections = groups.map((g) => `<section aria-labelledby="g-${g.id}"><h2 id="g-${g.id}">${esc(g.title)} <small>${esc(g.blurb)}</small></h2>
    <ul class="doc-cards">${cards(g)}</ul></section>`).join("\n");
  const start = [
    ["Get started", "docs/getting-started/", "Install openOMSI, point it at OMSI 2 and drive your first bus."],
    ["Settings", "docs/launcher/", "The launcher and every setting, page by page."],
    ["Make a map", "docs/mapper/", "openMapper, the map editor: install and five tutorials."],
    ["Make content", "docs/modding/", "What openOMSI adds for vehicle, object and map makers."],
    ["Write a plugin", "docs/plugins/", "Your first Lua plugin, then the whole omsi.* API."],
    ["Look up a format", "docs/formats/", "Every OMSI 2 file format as openOMSI reads it."],
  ].map(([t, h, d]) => `<a class="start-card" href="${u(h)}"><b>${t}</b><span>${d}</span></a>`).join("");
  const body = `<main id="main" class="wrap page">
  <header class="page-head">
    <h1>Documentation</h1>
    <p class="lead">Written next to the code and published from the repository, so it describes the version on <code>main</code>. Press <kbd>/</kbd> to search all of it.</p>
  </header>
  <div class="start-cards">${start}</div>
  ${sections}
</main>`;
  return pageShell({
    title: "Documentation", description: "The openOMSI documentation: user guide, modding, plugins, file formats and internals.",
    path: "/docs/", body, scripts: [], assetVersion: ctx.v,
  });
}

export const readMin = (doc) => Math.max(1, Math.round(doc.words / 220));
const pageMin = (p) => Math.max(1, Math.round(p.text.split(/\s+/).length / 220));

function navTree(groups, current) {
  const curDoc = current.doc;
  return groups.map((g) => {
    const open = g.items.includes(curDoc);
    return `<section class="nav-group"><h2>${esc(g.title)}</h2><ul>${g.items.map((d) => {
      const active = d === curDoc;
      const head = `<a href="${u(d.url)}"${active && current.kind !== "chapter" && current.kind !== "section" ? ' aria-current="page"' : ""}>${esc(d.title)}</a>`;
      let sub = "";
      if (active && d.pages.length > 1) {
        const overview = d.pages[0];
        const chapters = overview.children;
        sub = `<ul class="nav-sub">${chapters.map((c) => {
          const here = c === current || c.children.includes(current);
          const kids = here && c.children.length
            ? `<ul class="nav-sub">${c.children.map((s) => `<li><a href="${u(s.url)}"${s === current ? ' aria-current="page"' : ""}>${esc(s.title)}</a></li>`).join("")}</ul>` : "";
          return `<li><a href="${u(c.url)}"${c === current ? ' aria-current="page"' : ""}>${esc(c.title)}</a>${kids}</li>`;
        }).join("")}</ul>`;
      }
      return `<li${active ? ' class="is-open"' : ""}>${head}${sub}</li>`;
    }).join("")}</ul></section>`;
  }).join("");
}

function tocOf(page) {
  const lo = page.level + 1, hi = page.level + 2;
  const hs = page.headings.filter((h) => h.level >= lo && h.level <= hi);
  if (hs.length < 2) return "";
  return hs.map((h) => `<li class="lv${h.level - lo + 1}"><a href="#${esc(h.id)}">${esc(h.text)}</a></li>`).join("");
}

export function docPage(page, ctx) {
  const doc = page.doc;
  const toc = tocOf(page);
  const crumbs = [`<a href="${u("docs/")}">Docs</a>`];
  if (doc.group.title !== doc.title) crumbs.push(`<span>${esc(doc.group.title)}</span>`);
  if (page.kind === "chapter" || page.kind === "section") crumbs.push(`<a href="${u(doc.url)}">${esc(doc.title)}</a>`);
  if (page.kind === "section") crumbs.push(`<a href="${u(page.parent.url)}">${esc(page.parent.title)}</a>`);
  const isDocRoot = page.kind === "single" || page.kind === "overview";
  const lead = isDocRoot ? (doc.summary || "") : "";
  const edit = doc.remote === "mapper" ? `https://github.com/openOMSI-org/openMapper/edit/main/docs/${doc.file}.md` : `https://github.com/${REPO}/edit/main/docs/${doc.file}.md`;
  const meta = [
    `${isDocRoot ? readMin(doc) : pageMin(page)} min read`,
    `<a href="${edit}" class="ext">Edit this page</a>`,
    doc.remote === "mapper" ? "" : `<code>docs/${doc.file}.md</code>`,
  ].filter(Boolean).map((m) => `<span>${m}</span>`).join("");

  let chapterList = "";
  if (page.kind === "overview") {
    chapterList = `<h2 id="chapters">Chapters</h2><ol class="doc-cards chapter-cards">${page.children.map((c, i) => {
      const first = c.sections[0]?.text?.slice(0, 150) || "";
      return `<li><a class="doc-card" href="${u(c.url)}"><b>${i + 1}. ${esc(c.title)}</b><span>${esc(first)}${first.length >= 150 ? "…" : ""}</span><small>${pageMin(c)} min read</small></a></li>`;
    }).join("")}</ol>`;
  } else if (page.kind === "chapter" && page.children.length) {
    chapterList = `<h2 id="sections">In this chapter</h2><ul class="section-list">${page.children.map((c) =>
      `<li><a href="${u(c.url)}">${esc(c.title)}</a><span class="num">${pageMin(c)} min</span></li>`).join("")}</ul>`;
  }

  const pn = (p, dir) => !p ? "" : `<a class="pn pn-${dir}" href="${u(p.url)}"><span>${dir === "prev" ? "Previous" : "Next"}</span><b>${esc(p.title)}</b>${p.doc !== doc ? `<small>${esc(p.doc.title)}</small>` : ""}</a>`;

  const body = `<div class="docs-layout">
  <nav class="docs-nav" id="docs-nav" aria-label="Documentation"><div class="nav-head"><a href="${u("docs/")}">All documentation</a><button type="button" class="nav-close" data-nav-close aria-label="Close the menu">&times;</button></div>${navTree(ctx.groups, page)}</nav>
  <div class="nav-scrim" data-nav-close aria-hidden="true"></div>
  <script src="${u("assets/js/nav.js")}?v=${ctx.v}"></script>
  <main id="main" class="docs-main">
    <div class="docs-bar"><button class="nav-toggle" type="button" data-nav-toggle aria-expanded="false" aria-controls="docs-nav" hidden><span class="burger" aria-hidden="true"></span>Menu</button><span class="docs-bar-title">${esc(doc.title)}</span></div>
    <article class="doc">
      <header class="doc-head">
        <p class="crumbs">${crumbs.join(" <i>/</i> ")}</p>
        <h1${page.id ? ` id="${esc(page.id)}"` : ""}>${esc(page.title)}</h1>
        ${lead ? `<p class="lead">${esc(lead)}</p>` : ""}
        <p class="meta">${meta}</p>
        ${toc ? `<details class="toc-m"><summary>On this page</summary><ol>${toc}</ol></details>` : ""}
      </header>
      <div class="doc-body">
        ${page.html}
        ${chapterList}
      </div>
      <nav class="prevnext" aria-label="Previous and next page">${pn(page.prev, "prev")}${pn(page.next, "next")}</nav>
      <p class="doc-foot muted">Something wrong or missing? <a href="${edit}" class="ext">Fix it on GitHub</a> or <a href="https://github.com/${REPO}/issues/new" class="ext">open an issue</a>.</p>
    </article>
  </main>
  <aside class="docs-toc" aria-label="On this page">${toc ? `<h2>On this page</h2><ol>${toc}</ol>` : ""}</aside>
</div>`;
  const desc = (isDocRoot ? doc.summary : page.sections[0]?.text?.slice(0, 160)) || doc.summary;
  return pageShell({
    title: page.kind === "single" || page.kind === "overview" ? doc.title : `${page.title} · ${doc.title}`,
    description: desc,
    path: page.url,
    body, bodyClass: "docs", scripts: [], assetVersion: ctx.v,
  });
}
