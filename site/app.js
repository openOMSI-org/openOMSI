// openOMSI website: a small hash router that shows the overview, the download page and the
// Markdown files of docs/ (copied next to this page by .github/workflows/pages.yml).
const REPO = "openOMSI-org/openOMSI";
// the "playing now" counter (services/presence/): the games running right now
const PRESENCE = "https://openomsi.savvabestbrother.workers.dev";
const MAPPER_REPO = "openOMSI-org/openMapper";
// The "New" markers of openMapper (menu badge, announcement on the overview) end by themselves
const MAPPER_NEW_UNTIL = Date.parse("2026-11-23T00:00:00Z");
const mapperIsNew = () => Date.now() < MAPPER_NEW_UNTIL;
const MAPPER_DOCS = [
  { file: "TUTORIAL", title: "Your first map", icon: "school" },
  { file: "FEATURES", title: "Features and controls", icon: "list" },
  { file: "ROADMAP", title: "Roadmap", icon: "flag" },
];
const DOCS = [
  { file: "OPENMAPPER", title: "openMapper", icon: "edit_road" },
  { file: "USER_GUIDE", title: "User guide", icon: "sports_esports" },
  { file: "ANDROID", title: "Android & mobile", icon: "smartphone" },
  { file: "VR", title: "Virtual reality", icon: "view_in_ar" },
  { file: "SERVER", title: "Dedicated server", icon: "dns" },
  { file: "MODDING", title: "Modding beyond OMSI 2", icon: "handyman" },
  { file: "PBR", title: "PBR materials", icon: "texture" },
  { file: "MAP_PACKAGE", title: "Map packages", icon: "map" },
  { file: "PLUGINS", title: "Plugins", icon: "extension" },
  { file: "BUILDING", title: "Building", icon: "build" },
  { file: "FORMATS", title: "Content formats", icon: "description" },
  { file: "ARCHITECTURE", title: "Architecture", icon: "account_tree" },
  { file: "ROUTES", title: "Routes", icon: "alt_route" },
  { file: "DEBUG_FLAGS", title: "Debug switches", icon: "bug_report" },
  { file: "VERSIONING", title: "Versioning & releases", icon: "new_releases" },
];
const PLATFORMS = [
  { key: "windows-x64", setup: "-setup.exe", name: "Windows", icon: "desktop_windows", note: "64-bit (x64), Windows 10 or newer", os: "windows" },
  { key: "windows-arm64", setup: "-setup.exe", name: "Windows on ARM", icon: "desktop_windows", note: "ARM64 (Snapdragon laptops), Windows 11", os: "windows-arm" },
  { key: "macos-arm64", setup: ".dmg", name: "macOS (Apple silicon)", icon: "laptop_mac", note: "M1 or newer, macOS 11 or newer", os: "mac" },
  { key: "macos-x64", setup: ".dmg", name: "macOS (Intel)", icon: "laptop_mac", note: "Intel Macs, macOS 11 or newer", os: "mac-intel" },
  { key: "linux-x64", name: "Linux", icon: "computer", note: "x86-64, Vulkan or OpenGL, WebKitGTK 4.1", os: "linux" },
  { key: "linux-arm64", name: "Linux on ARM", icon: "computer", note: "ARM64, Vulkan or OpenGL, WebKitGTK 4.1", os: "linux-arm" },
  { key: "android-arm64", ext: "apk", name: "Android", icon: "smartphone", note: "arm64 phones and tablets, Android 8.0+", os: "android" },
];
// The dedicated server: no window, for hosting a session (see the Dedicated server page).
const SERVERS = [
  { key: "server-windows-x64", name: "Server for Windows", icon: "dns", note: "x64, no window" },
  { key: "server-windows-arm64", name: "Server for Windows on ARM", icon: "dns", note: "ARM64, no window" },
  { key: "server-linux-x64", name: "Server for Linux", icon: "dns", note: "x86-64, no window" },
  { key: "server-linux-arm64", name: "Server for Linux on ARM", icon: "dns", note: "ARM64 (Raspberry Pi 4/5, ARM VPS), no window" },
];

// Which build fits the visitor, roughly: the one to put first and mark.
function visitorOs() {
  const ua = navigator.userAgent || "";
  if (/Android/i.test(ua)) return "android";
  if (/Windows/i.test(ua)) return /ARM|aarch64/i.test(ua) ? "windows-arm" : "windows";
  if (/Mac OS X|Macintosh/i.test(ua)) return "mac";
  if (/Linux/i.test(ua)) return /aarch64|arm64/i.test(ua) ? "linux-arm" : "linux";
  return "";
}

const view = document.getElementById("view");
const drawer = document.getElementById("drawer");
const scrim = document.getElementById("scrim");

document.getElementById("doc-links").innerHTML = DOCS.filter(d => d.file !== "OPENMAPPER").map(d =>
  `<a href="#/docs/${d.file}" data-route="/docs/${d.file}"><span class="material-icons">${d.icon}</span>${d.title}</a>`).join("");

document.getElementById("mapper-links").insertAdjacentHTML("beforeend", MAPPER_DOCS.map(d =>
  `<a href="#/mapper/docs/${d.file}" data-route="/mapper/docs/${d.file}"><span class="material-icons">${d.icon}</span>${d.title}</a>`).join(""));
function toggleDrawer(open) {
  drawer.classList.toggle("open", open);
  scrim.classList.toggle("open", open);
}
document.getElementById("menu-btn").onclick = () => toggleDrawer(!drawer.classList.contains("open"));
scrim.onclick = () => toggleDrawer(false);

let release = null;
let releaseAt = 0;
async function latestRelease() {
  if (release && Date.now() - releaseAt < 120000) return release;
  releaseAt = Date.now();
  try {
    const r = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`);
    release = r.ok ? await r.json() : { none: true };
  } catch { release = { none: true }; }
  return release;
}

let releasesRequest = null;
async function gameReleases() {
  if (!releasesRequest) releasesRequest = (async () => {
    const list = [];
    for (let page = 1; ; page++) {
      const batch = await gh(`releases?per_page=100&page=${page}`);
      list.push(...batch.filter(r => !r.draft && /^v\d+\.\d+\.\d+$/.test(r.tag_name)));
      if (batch.length < 100) return list;
    }
  })().catch(e => { releasesRequest = null; throw e; });
  return releasesRequest;
}

function newerVersion(a, b) {
  const x = a.replace(/^v/, "").split(".").map(Number);
  const y = b.replace(/^v/, "").split(".").map(Number);
  for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] > y[i];
  return false;
}

async function latestBeta() {
  const [list, stable] = await Promise.all([gameReleases(), latestRelease()]);
  // Older pre-releases stay in the history, but are never offered as an update to stable.
  if (!stable.tag_name) throw new Error("The stable release could not be checked. Try again later.");
  return list.filter(r => r.prerelease && newerVersion(r.tag_name, stable.tag_name))
    .sort((a, b) => newerVersion(a.tag_name, b.tag_name) ? -1 : newerVersion(b.tag_name, a.tag_name) ? 1 : 0)[0] || null;
}

function slug(text) {
  return text.toLowerCase().replace(/<[^>]+>/g, "").replace(/[^\w\- ]+/g, "").trim().replace(/\s+/g, "-");
}

function showTemplate(id) {
  view.innerHTML = "";
  view.appendChild(document.getElementById(id).content.cloneNode(true));
}

async function home() {
  showTemplate("home");
  if (mapperIsNew()) document.getElementById("mapper-news")?.removeAttribute("hidden");
  playingNow();
  const rel = await latestRelease();
  const chip = document.querySelector("#version-chip span:last-child");
  if (chip) chip.textContent = rel.tag_name ? `Latest: ${rel.tag_name.replace(/^v/, "")}` : "No release yet";
}

// How many play openOMSI right now: the counter's count of these ten minutes (the README's
// badge shows the same one), asked again as soon as the next ten minutes have begun while the
// page shows it (the counter runs on Cloudflare's free plan: every request counts against its
// daily limit, and the answer is cached until then anyway).
let playingTimer = null;
async function playingNow() {
  clearTimeout(playingTimer);
  const chip = document.getElementById("players-chip");
  if (!chip) return;
  let wait = 600000;
  try {
    const r = await fetch(`${PRESENCE}/players`);
    const p = r.ok ? await r.json() : null;
    if (p && typeof p.players === "number") {
      chip.lastElementChild.textContent = `${p.players} playing now`;
      chip.title = Object.entries(p.systems || {}).map(([os, n]) => `${os}: ${n}`).join(", ");
      chip.hidden = false;
      // (a little after the next count, the cache's clock and ours never quite agree)
      const next = Date.parse(p.next);
      if (!Number.isNaN(next)) wait = Math.min(600000, Math.max(20000, next - Date.now() + 20000));
    }
  } catch { /* (the counter is out of reach: the chip stays hidden) */ }
  playingTimer = setTimeout(playingNow, wait);
}

async function download(beta = false) {
  showTemplate("download");
  const grid = document.getElementById("dl-grid");
  const ver = document.getElementById("dl-version");
  if (beta) {
    document.title = "Beta downloads · openOMSI";
    view.querySelector("h1").textContent = "Beta downloads";
    document.getElementById("dl-channel").innerHTML = `Betas are for testing new changes. For everyday use, choose the <a href="#/download">stable release</a>.`;
    document.getElementById("dl-warning-title").textContent = "Beta: for testing";
    document.getElementById("dl-warning-text").innerHTML = `This build may have regressions. Report problems with the version number on the <a href="#/issues">Issues</a> page. You can return to Stable in the launcher's update settings.`;
  }
  let rel;
  try { rel = beta ? await latestBeta() : await latestRelease(); }
  catch (e) { if (view.contains(ver)) ver.textContent = e.message; return; }
  if (!view.contains(ver)) return;
  if (!rel?.tag_name) {
    ver.innerHTML = beta
      ? `There is no beta newer than the stable release. <a href="#/download">Download stable</a> or browse the <a href="#/releases/beta">beta history</a>.`
      : "No stable release has been published yet.";
    return;
  }
  const v = rel.tag_name.replace(/^v/, "");
  if (rel.html_url) document.getElementById("dl-github").href = rel.html_url;
  ver.innerHTML = `${beta ? "Latest beta" : "Stable version"}: <b>${esc(v)}</b> · ${new Date(rel.published_at).toLocaleDateString()}`;
  // Each file by its exact name: matched by its ending alone, "-windows-x64.zip" was the
  // dedicated server's zip as well, and the Windows button downloaded the server.
  const card = p => {
    const find = name => (rel.assets || []).find(a => a.name === name);
    const zip = find(`openOMSI-${v}-${p.key}.${p.ext || "zip"}`);
    const setup = p.setup && find(`openOMSI-${v}-${p.key}${p.setup}`);
    const a = setup || zip;
    const size = a ? ` · ${(a.size / 1048576).toFixed(0)} MB` : "";
    const mine = p.os && p.os === visitorOs();
    return `<div class="card elevation-${mine ? 3 : 1} dl-card${mine ? " dl-mine" : ""}"><div class="dl-head"><span class="material-icons card-icon">${p.icon}</span>${mine ? `<span class="dl-badge">Your system</span>` : ""}</div>
      <h3>${p.name}</h3><p>${p.note}${size}</p>
      ${a ? `<a class="btn btn-contained" href="${a.browser_download_url}" download><span class="material-icons">download</span>Download</a>`
          : `<p>Not in this release.</p>`}
      ${setup && zip ? `<a class="btn btn-outlined" style="margin-top:0" href="${zip.browser_download_url}" download>Portable .zip</a>` : ""}</div>`;
  };
  const games = [...PLATFORMS].sort((x, y) => (y.os === visitorOs()) - (x.os === visitorOs()));
  grid.innerHTML = games.map(card).join("")
    + `<h2 class="section-title" style="grid-column:1/-1">Dedicated server</h2>`
    + SERVERS.map(card).join("");
}

// openMapper, the map editor: its own repository's latest release, the same way as the game's
const MAPPER_RAW = `https://raw.githubusercontent.com/${MAPPER_REPO}/main`;
// openMapper's own docs are read from its repository, so they are always the current ones
const DOC_SETS = {
  openomsi: { list: DOCS, repo: REPO, route: "docs", url: n => `docs/${n}.md`, blob: "" },
  mapper: { list: MAPPER_DOCS, repo: MAPPER_REPO, route: "mapper/docs", url: n => `${MAPPER_RAW}/docs/${n}.md`, blob: "" },
};

async function doc(name, anchor, set = "openomsi") {
  const cfg = DOC_SETS[set];
  const meta = cfg.list.find(d => d.file === name);
  view.innerHTML = `<div class="content"><div class="doc"><div class="loading">Loading…</div></div></div>`;
  let md;
  try {
    if (!meta && set !== "openomsi") throw new Error(404);
    const r = await fetch(cfg.url(name));
    if (!r.ok) throw new Error(r.status);
    md = await r.text();
  } catch {
    view.querySelector(".doc").innerHTML = `<h1>Not found</h1><p>There is no document called <code>${esc(name)}</code>.</p>`;
    return;
  }
  const box = view.querySelector(".doc");
  box.innerHTML = `<div class="doc-toolbar"><a href="https://github.com/${cfg.repo}/blob/main/docs/${name}.md">
    <span class="material-icons" style="font-size:18px">edit</span> Edit on GitHub</a></div>` + marked.parse(md);
  document.title = `${meta ? meta.title : name} · ${set === "mapper" ? "openMapper" : "openOMSI"}`;
  // heading anchors
  box.querySelectorAll("h1, h2, h3, h4").forEach(h => {
    h.id = slug(h.textContent);
    if (h.tagName !== "H1") h.insertAdjacentHTML("beforeend", `<a class="anchor" href="#/${cfg.route}/${name}#${h.id}">#</a>`);
  });
  // links between documents stay inside the site; other repository files go to GitHub
  box.querySelectorAll("a[href]").forEach(a => {
    const href = a.getAttribute("href");
    if (/^(https?:|mailto:|#)/.test(href)) return;
    const m = href.match(/^(?:\.\/)?([A-Z_]+)\.md(?:#(.*))?$/);
    if (m && cfg.list.some(d => d.file === m[1])) {
      a.setAttribute("href", `#/${cfg.route}/${m[1]}${m[2] ? "#" + m[2] : ""}`);
    } else {
      a.setAttribute("href", `https://github.com/${cfg.repo}/blob/main/docs/${href}`);
    }
  });
  // pictures of openMapper's docs live in its repository
  if (set === "mapper") box.querySelectorAll("img[src]").forEach(i => {
    const src = i.getAttribute("src");
    if (!/^(https?:|data:)/.test(src)) i.src = new URL(src, `${MAPPER_RAW}/docs/`).href;
  });
  if (anchor) document.getElementById(anchor)?.scrollIntoView();
  else window.scrollTo(0, 0);
}


// --- Releases and issues, read from the GitHub API (60 requests an hour per visitor
// without a token: every list is fetched once per visit and kept)
const cache = {};
async function gh(path) {
  if (cache[path]) return cache[path];
  const r = await fetch(`https://api.github.com/repos/${REPO}/${path}`, { headers: { Accept: "application/vnd.github+json" } });
  if (!r.ok) throw new Error(r.status === 403 ? "GitHub's hourly request limit is reached - try again later." : `GitHub answered ${r.status}.`);
  return (cache[path] = await r.json());
}
function esc(t) {
  return String(t ?? "").replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}
const MD_TAGS = new Set(["a", "abbr", "b", "blockquote", "br", "code", "dd", "del", "details", "div", "dl", "dt", "em", "h1", "h2", "h3", "h4", "h5", "h6", "hr", "i", "img", "input", "ins", "kbd", "li", "mark", "ol", "p", "pre", "q", "s", "samp", "small", "span", "strike", "strong", "sub", "summary", "sup", "table", "tbody", "td", "tfoot", "th", "thead", "tr", "tt", "u", "ul"]);
const MD_DROP = new Set(["script", "style", "iframe", "frame", "frameset", "object", "embed", "applet", "form", "link", "meta", "base", "template", "noscript", "textarea", "select", "button", "svg", "math", "title", "xmp", "noembed", "noframes", "plaintext", "audio", "video", "source", "track", "canvas", "portal", "dialog"]);
const MD_ATTRS = {
  "*": ["title"],
  a: ["href"],
  img: ["src", "alt", "width", "height"],
  p: ["align"],
  div: ["align"],
  td: ["align", "colspan", "rowspan"],
  th: ["align", "colspan", "rowspan"],
  ol: ["start"],
  details: ["open"],
  input: ["type", "checked", "disabled"],
};
function safeUrl(value, image) {
  try {
    const u = new URL(value.trim(), `https://github.com/${REPO}/issues/`);
    if (u.protocol === "https:" || u.protocol === "http:" || (!image && u.protocol === "mailto:")) return u.href;
  } catch {}
  return null;
}
function md(text) {
  const t = document.createElement("template");
  t.innerHTML = marked.parse(text || "");
  for (const e of [...t.content.querySelectorAll("*")]) {
    const tag = e.localName;
    if (e.namespaceURI !== "http://www.w3.org/1999/xhtml" || MD_DROP.has(tag) || (tag === "input" && e.getAttribute("type") !== "checkbox")) {
      e.remove();
      continue;
    }
    if (!MD_TAGS.has(tag)) {
      e.replaceWith(...e.childNodes);
      continue;
    }
    const allowed = [...MD_ATTRS["*"], ...(MD_ATTRS[tag] || [])];
    for (const a of [...e.attributes]) {
      const name = a.name.toLowerCase();
      if (tag === "code" && name === "class" && /^language-[\w+-]+$/.test(a.value)) continue;
      if (!allowed.includes(name)) { e.removeAttribute(a.name); continue; }
      if (name === "href" || name === "src") {
        const url = safeUrl(a.value, name === "src");
        if (url) e.setAttribute(a.name, url); else e.removeAttribute(a.name);
      }
    }
    if (tag === "input") e.setAttribute("disabled", "");
  }
  return t.innerHTML;
}
function ago(date) {
  const s = (Date.now() - new Date(date)) / 1000;
  for (const [n, u] of [[31536000, "year"], [2592000, "month"], [86400, "day"], [3600, "hour"], [60, "minute"]]) {
    if (s >= n) { const k = Math.floor(s / n); return `${k} ${u}${k > 1 ? "s" : ""} ago`; }
  }
  return "just now";
}
function labelChips(labels) {
  return (labels || []).map(l => `<span class="label" style="--lc:#${esc(l.color)}">${esc(l.name)}</span>`).join("");
}

async function releases(beta = false) {
  view.innerHTML = `<div class="content"><h1>${beta ? "Beta releases" : "Stable releases"}</h1>
    <p class="lead">${beta ? 'Versions for testing, including older betas. <a href="#/beta">Download the latest beta</a>.' : 'Stable versions, newest first. <a href="#/download">Download the latest stable release</a>.'}</p>
    <nav class="release-channels" aria-label="Release channel"><a class="btn ${beta ? "btn-outlined" : "btn-contained"}" href="#/releases"${beta ? "" : ' aria-current="page"'}>Stable</a> <a class="btn ${beta ? "btn-contained" : "btn-outlined"}" href="#/releases/beta"${beta ? ' aria-current="page"' : ""}>Betas</a></nav>
    <div id="rel-list"><div class="loading">Loading…</div></div></div>`;
  document.title = `${beta ? "Beta" : "Stable"} releases · openOMSI`;
  const box = document.getElementById("rel-list");
  let all, stable;
  try { [all, stable] = await Promise.all([gameReleases(), latestRelease()]); }
  catch (e) { box.innerHTML = `<p>${esc(e.message)} <a href="https://github.com/${REPO}/releases">Releases on GitHub</a></p>`; return; }
  if (!view.contains(box)) return;
  const list = all.filter(r => r.prerelease === beta)
    .sort((a, b) => newerVersion(a.tag_name, b.tag_name) ? -1 : newerVersion(b.tag_name, a.tag_name) ? 1 : 0);
  if (!list.length) { box.innerHTML = `<p>No ${beta ? "beta" : "stable"} release has been published yet.</p>`; return; }
  const latest = beta
    ? (stable.tag_name ? list.findIndex(r => newerVersion(r.tag_name, stable.tag_name)) : -1)
    : list.findIndex(r => r.tag_name === stable.tag_name);
  // Render notes only as they are requested, rather than parsing the whole history at once.
  let shown = 0;
  const draw = () => {
    shown += 30;
    box.innerHTML = list.slice(0, shown).map((r, i) => {
      const assets = (r.assets || []).map(a => `<a class="asset" href="${esc(a.browser_download_url)}"><span class="material-icons">download</span>${esc(a.name)} <small>${(a.size / 1048576).toFixed(0)} MB</small></a>`).join("");
      return `<details class="card elevation-1 release"${i === latest ? " open" : ""}>
        <summary><span class="rel-tag">${esc(r.tag_name.replace(/^v/, ""))}</span>
          ${i === latest ? `<span class="label" style="--lc:#2da44e">${beta ? "Latest beta" : "Latest"}</span>` : ""}${r.prerelease ? '<span class="label" style="--lc:#bf8700">Pre-release</span>' : ""}
          <span class="rel-name">${esc(r.name && r.name !== r.tag_name ? r.name : "")}</span>
          <span class="spacer"></span><span class="muted">${new Date(r.published_at).toLocaleDateString()}</span></summary>
        <div class="doc rel-body">${md(r.body) || "<p class='muted'>No notes.</p>"}</div>
        ${assets ? `<div class="assets">${assets}</div>` : ""}
      </details>`;
    }).join("") + (shown < list.length ? '<button class="btn btn-outlined" id="rel-more">More releases</button>' : "");
    const more = document.getElementById("rel-more");
    if (more) more.onclick = draw;
  };
  draw();
}

let issueState = "open", issueQuery = "";
async function issues(number) {
  if (number) return issue(number);
  view.innerHTML = `<div class="content"><h1>Issues</h1>
    <p class="lead">Bugs and wishes. To report one, you need a GitHub account.</p>
    <p class="issue-new"><a class="btn btn-contained" href="https://github.com/${REPO}/issues/new"><span class="material-icons">add</span>New issue</a></p>
    <div class="issue-bar">
      <div class="tabs"><button data-s="open">Open</button><button data-s="closed">Closed</button><button data-s="all">All</button></div>
      <input id="iq" type="search" placeholder="Filter by title or label" value="${esc(issueQuery)}">
    </div>
    <div id="issue-list"><div class="loading">Loading…</div></div></div>`;
  document.title = "Issues · openOMSI";
  view.querySelectorAll(".tabs button").forEach(b => {
    b.classList.toggle("on", b.dataset.s === issueState);
    b.onclick = () => { issueState = b.dataset.s; issues(); };
  });
  const box = document.getElementById("issue-list");
  let list;
  try { list = await gh(`issues?state=${issueState}&per_page=100&sort=updated`); } catch (e) { box.innerHTML = `<p>${esc(e.message)} <a href="https://github.com/${REPO}/issues">Issues on GitHub</a></p>`; return; }
  // (the issues list also carries pull requests)
  list = list.filter(i => !i.pull_request);
  const draw = () => {
    const q = issueQuery.trim().toLowerCase();
    const shown = list.filter(i => !q || i.title.toLowerCase().includes(q) || (i.labels || []).some(l => l.name.toLowerCase().includes(q)) || String(i.number) === q.replace("#", ""));
    box.innerHTML = shown.length ? `<div class="card elevation-1 issue-list">${shown.map(i => `
      <a class="issue-row" href="#/issues/${i.number}">
        <span class="material-icons ${i.state === "open" ? "st-open" : "st-closed"}">${i.state === "open" ? "radio_button_unchecked" : "check_circle"}</span>
        <span class="issue-main"><span class="issue-title">${esc(i.title)}</span> ${labelChips(i.labels)}
          <span class="muted">#${i.number} · ${esc(i.user?.login)} · updated ${ago(i.updated_at)}</span></span>
        ${i.comments ? `<span class="muted cm"><span class="material-icons">chat_bubble_outline</span>${i.comments}</span>` : ""}
      </a>`).join("")}</div>` : `<p class="muted">Nothing here.</p>`;
  };
  document.getElementById("iq").oninput = e => { issueQuery = e.target.value; draw(); };
  draw();
}

async function issue(n) {
  view.innerHTML = `<div class="content"><p><a href="#/issues"><span class="material-icons" style="font-size:18px;vertical-align:-3px">arrow_back</span> All issues</a></p><div id="issue-box"><div class="loading">Loading…</div></div></div>`;
  const box = document.getElementById("issue-box");
  let i, comments;
  try { [i, comments] = await Promise.all([gh(`issues/${n}`), gh(`issues/${n}/comments?per_page=100`)]); }
  catch (e) { box.innerHTML = `<p>${esc(e.message)}</p>`; return; }
  document.title = `#${i.number} ${i.title} · openOMSI`;
  const post = (who, when, body) => `<div class="card elevation-1 comment"><div class="comment-head"><img src="${esc(who?.avatar_url)}&s=48" alt=""><b>${esc(who?.login)}</b><span class="muted">${ago(when)}</span></div><div class="doc">${md(body) || "<p class='muted'>No description.</p>"}</div></div>`;
  box.innerHTML = `<h1 class="issue-head">${esc(i.title)} <span class="muted">#${i.number}</span></h1>
    <p><span class="label" style="--lc:${i.state === "open" ? "#2da44e" : "#8250df"}">${i.state === "open" ? "Open" : "Closed"}</span> ${labelChips(i.labels)}</p>
    ${post(i.user, i.created_at, i.body)}${comments.map(c => post(c.user, c.created_at, c.body)).join("")}
    <p><a class="btn btn-outlined" href="${esc(i.html_url)}"><span class="material-icons">reply</span>Comment on GitHub</a></p>`;
  window.scrollTo(0, 0);
}

function route() {
  const hash = location.hash.replace(/^#/, "") || "/";
  const [path, anchor] = hash.split("#");
  document.getElementById("mapper-badge")?.toggleAttribute("hidden", !mapperIsNew());
  document.querySelectorAll(".drawer a[data-route]").forEach(a => a.classList.toggle("active", a.dataset.route === path || (["/issues", "/releases"].includes(a.dataset.route) && path.startsWith(a.dataset.route + "/"))));
  toggleDrawer(false);
  document.title = "openOMSI";
  if (path.startsWith("/docs/")) return doc(path.slice(6), anchor);
  if (path === "/mapper") { location.replace("#/docs/OPENMAPPER"); return; }
  if (path.startsWith("/mapper/docs/")) return doc(path.slice(13), anchor, "mapper");
  if (path === "/download") return download();
  if (path === "/beta") return download(true);
  if (path === "/releases") return releases();
  if (path === "/releases/beta") return releases(true);
  if (path.startsWith("/issues")) return issues(path.split("/")[2]);
  window.scrollTo(0, 0);
  return home();
}
window.addEventListener("hashchange", route);

// Theme: the app bar button goes system -> light -> dark -> system; the choice is kept in
// localStorage (applied before the first paint by the script in <head>)
(() => {
  const btn = document.getElementById("theme-btn");
  if (!btn) return;
  const root = document.documentElement;
  const names = { "": "Theme: system", light: "Theme: light", dark: "Theme: dark" };
  const icons = { "": "brightness_auto", light: "light_mode", dark: "dark_mode" };
  const show = () => {
    const t = root.dataset.theme || "";
    btn.querySelector(".material-icons").textContent = icons[t];
    btn.title = names[t];
    btn.setAttribute("aria-label", names[t]);
  };
  btn.onclick = () => {
    const next = { "": "light", light: "dark", dark: "" }[root.dataset.theme || ""];
    if (next) root.dataset.theme = next; else delete root.dataset.theme;
    try { next ? localStorage.setItem("theme", next) : localStorage.removeItem("theme"); } catch {}
    show();
  };
  show();
})();
route();

// Donate: the button opens its menu upwards; a click elsewhere, Escape or a choice closes it
(() => {
  const button = document.getElementById("donate-button");
  const menu = document.getElementById("donate-menu");
  if (!button || !menu) return;
  const open = on => {
    menu.hidden = !on;
    button.setAttribute("aria-expanded", on ? "true" : "false");
    if (on) menu.querySelector("a").focus();
  };
  button.addEventListener("click", e => { e.stopPropagation(); open(menu.hidden); });
  menu.addEventListener("click", e => { if (e.target.closest("a")) open(false); });
  document.addEventListener("click", e => { if (!menu.hidden && !e.target.closest("#donate")) open(false); });
  document.addEventListener("keydown", e => {
    if (e.key === "Escape" && !menu.hidden) { open(false); button.focus(); }
  });
})();
