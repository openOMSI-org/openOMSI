// openOMSI website: a small router that shows the overview, the download page and the
// Markdown files of docs/ (each also written as its own page by scripts/build-site.mjs).
const REPO = "openOMSI-org/openOMSI";
// the "playing now" counter (services/presence/): the games running right now
const PRESENCE = "https://openomsi.savvabestbrother.workers.dev";
const MAPPER_REPO = "openOMSI-org/openMapper";
let markedLoad = null;
function loadMarked() {
  if (window.marked) return Promise.resolve();
  return markedLoad ||= new Promise((done, fail) => {
    const s = document.createElement("script");
    s.src = "/vendor/marked.min.js";
    s.integrity = "sha384-/TQbtLCAerC3jgaim+N78RZSDYV7ryeoBCVqTuzRrFec2akfBkHS7ACQ3PQhvMVi";
    s.onload = done;
    s.onerror = () => { markedLoad = null; fail(new Error("The page could not be loaded. Try again later.")); };
    document.head.appendChild(s);
  });
}
// The "New" markers of openMapper (menu badge, announcement on the overview) end by themselves
const MAPPER_NEW_UNTIL = Date.parse("2026-11-23T00:00:00Z");
const mapperIsNew = () => Date.now() < MAPPER_NEW_UNTIL;
const MAPPER_DOCS = [
  { file: "TUTORIAL", title: "Your first map", icon: "school", group: "openMapper" },
  { file: "FEATURES", title: "Features and controls", icon: "list", group: "openMapper" },
  { file: "ROADMAP", title: "Roadmap", icon: "flag", group: "openMapper" },
];
const DOCS = [
  { file: "OPENMAPPER", title: "openMapper", icon: "edit_road", group: "openMapper" },
  { file: "USER_GUIDE", title: "User guide", icon: "sports_esports", group: "Play" },
  { file: "ANDROID", title: "Android & mobile", icon: "smartphone", group: "Play" },
  { file: "VR", title: "Virtual reality", icon: "view_in_ar", group: "Play" },
  { file: "SERVER", title: "Dedicated server", icon: "dns", group: "Play" },
  { file: "MODDING", title: "Modding beyond OMSI 2", icon: "handyman", group: "Create" },
  { file: "PBR", title: "PBR materials", icon: "texture", group: "Create" },
  { file: "MAP_PACKAGE", title: "Map packages", icon: "map", group: "Create" },
  { file: "ROUTES", title: "Routes", icon: "alt_route", group: "Create" },
  { file: "PLUGINS", title: "Plugins", icon: "extension", group: "Create" },
  { file: "FORMATS", title: "Content formats", icon: "description", group: "Create" },
  { file: "BUILDING", title: "Building", icon: "build", group: "Under the hood" },
  { file: "ARCHITECTURE", title: "Architecture", icon: "account_tree", group: "Under the hood" },
  { file: "DEBUG_FLAGS", title: "Debug switches", icon: "bug_report", group: "Under the hood" },
  { file: "VERSIONING", title: "Versioning & releases", icon: "new_releases", group: "Under the hood" },
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

const docSlug = file => file.toLowerCase().replace(/_/g, "-");
const docHref = (set, file) => `/${set === "mapper" ? "mapper/docs" : "docs"}/${docSlug(file)}/`;
function newPath(old) {
  const [p, a] = old.split("#");
  const m = p.match(/^\/(mapper\/)?docs\/([A-Za-z0-9_]+)$/);
  const path = m ? docHref(m[1] ? "mapper" : "openomsi", m[2]) : p === "/" || !p ? "/" : p.replace(/\/?$/, "/");
  return path + (a ? "#" + a : "");
}

const view = document.getElementById("view");
const drawer = document.getElementById("drawer");
const scrim = document.getElementById("scrim");

document.getElementById("doc-links").innerHTML = ["Play", "Create", "Under the hood"].map(g =>
  `<div class="drawer-subheader">${g}</div><div class="drawer-section">${DOCS.filter(d => d.group === g).map(d =>
    `<a href="${docHref("openomsi", d.file)}" data-route="/docs/${docSlug(d.file)}"><span class="material-icons">${d.icon}</span>${d.title}</a>`).join("")}</div>`).join("");

document.getElementById("mapper-links").innerHTML = `<a href="/docs/openmapper/" data-route="/docs/openmapper"><span class="material-icons">info</span>Map editor</a>` + MAPPER_DOCS.map(d =>
  `<a href="${docHref("mapper", d.file)}" data-route="/mapper/docs/${docSlug(d.file)}"><span class="material-icons">${d.icon}</span>${d.title}</a>`).join("");
function toggleDrawer(open) {
  drawer.classList.toggle("open", open);
  scrim.classList.toggle("open", open);
}
document.getElementById("menu-btn").onclick = () => toggleDrawer(!drawer.classList.contains("open"));
scrim.onclick = () => toggleDrawer(false);

let release = null;
let releaseAt = 0;
function latestRelease() {
  if (release && Date.now() - releaseAt < 120000) return release;
  releaseAt = Date.now();
  release = gh("releases/latest").catch(e => e.status === 404 ? { none: true } : { failed: true });
  return release;
}

function newerVersion(a, b) {
  const x = a.replace(/^v/, "").split(".").map(Number);
  const y = b.replace(/^v/, "").split(".").map(Number);
  for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] > y[i];
  return false;
}

async function latestBeta() {
  const [list, stable] = await Promise.all([gh("releases?per_page=30&page=1"), latestRelease()]);
  // Older pre-releases stay in the history, but are never offered as an update to stable.
  if (stable.failed) throw new Error("The stable release could not be checked. Try again later.");
  return list.filter(r => !r.draft && r.prerelease && /^v\d+\.\d+\.\d+$/.test(r.tag_name) && (!stable.tag_name || newerVersion(r.tag_name, stable.tag_name)))
    .sort((a, b) => newerVersion(a.tag_name, b.tag_name) ? -1 : newerVersion(b.tag_name, a.tag_name) ? 1 : 0)[0] || null;
}

function slug(text) {
  return text.toLowerCase().replace(/[^\w\- ]+/g, "").replace(/ /g, "-");
}

function showTemplate(id) {
  view.innerHTML = "";
  view.appendChild(document.getElementById(id).content.cloneNode(true));
}

function scanResult(data, sha) {
  const d = data?.[sha];
  if (!d) return null;
  return { m: Number(d.m) || 0, s: Number(d.s) || 0, n: Number(d.n) || 0, fp: d.fp === 1 || d.fp === true, why: typeof d.why === "string" ? d.why : "" };
}

function scanVerdict(r) {
  if (!r || !r.n) return { kind: "wait" };
  const flagged = r.m + r.s;
  if (!flagged) return { kind: "clean", label: "Clean", note: `no engine out of ${r.n} flags this file` };
  if (flagged >= 3) return { kind: "flagged", label: "Flagged", note: `${flagged} of ${r.n} engines flag this file` };
  if (r.fp) return { kind: "checked", label: "Checked", note: `${flagged} of ${r.n} engines flag this file; the maintainers reviewed it as a false positive${r.why ? ` (${r.why})` : ""}` };
  return { kind: "checked", label: "Checked", note: `${flagged} of ${r.n} engines guess from how this file looks, usually a false positive: see the report` };
}

async function home() {
  showTemplate("home");
  document.body.dataset.page = "home";
  scanStrip();
  crew();
  showcase();
  const wall = document.getElementById("home-wall");
  [...document.getElementById("gallery").content.querySelectorAll("figure")].slice(0, 8).forEach(f => wall.appendChild(f.cloneNode(true)));
  if (mapperIsNew()) document.getElementById("mapper-news")?.removeAttribute("hidden");
  playingNow();
  const os = { windows: "Windows", "windows-arm": "Windows", mac: "macOS", linux: "Linux", "linux-arm": "Linux", android: "Android" }[visitorOs()];
  if (os) document.querySelector("#hero-download span:last-child").textContent = `Download for ${os}`;
  const rel = await latestRelease();
  const chip = document.querySelector("#version-chip > span:last-child");
  if (!chip) return;
  chip.classList.remove("sign-wait");
  if (rel.tag_name) chip.textContent = `Stable ${rel.tag_name.replace(/^v/, "")}`;
  else chip.innerHTML = `<a href="https://github.com/${REPO}/releases/latest" target="_blank" rel="noopener">Latest release</a>`;
}

let showWatch = null;
function showcase() {
  const box = document.getElementById("showcase");
  if (!box) return;
  const tabs = [...box.querySelectorAll("[role=tab]")];
  const compare = document.getElementById("compare");
  const range = compare.querySelector("input");
  const open = t => {
    tabs.forEach(o => { o.setAttribute("aria-selected", String(o === t)); o.tabIndex = o === t ? 0 : -1; });
    box.querySelectorAll(".show-pane").forEach(p => { p.hidden = p.dataset.show !== t.dataset.show; });
    compare.classList.toggle("sweep", t.dataset.show === "graphics" && !compare.style.getPropertyValue("--pos"));
  };
  tabs.forEach((t, i) => {
    t.onclick = () => open(t);
    t.onkeydown = e => {
      const step = { ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1 }[e.key];
      if (!step) return;
      e.preventDefault();
      const next = tabs[(i + step + tabs.length) % tabs.length];
      open(next);
      next.focus();
    };
    t.addEventListener("animationend", () => open(tabs[(i + 1) % tabs.length]));
  });
  range.oninput = () => {
    compare.classList.remove("sweep");
    compare.style.setProperty("--pos", `${range.value}%`);
  };
  const pause = document.getElementById("show-pause");
  if (matchMedia("(prefers-reduced-motion: reduce)").matches) { pause.hidden = true; return; }
  box.classList.add("auto");
  pause.onclick = () => {
    const stop = !box.classList.toggle("stopped");
    box.classList.toggle("auto", stop);
    pause.setAttribute("aria-pressed", String(!stop));
    pause.title = pause.ariaLabel = stop ? "Pause the slideshow" : "Play the slideshow";
    pause.firstElementChild.textContent = stop ? "pause" : "play_arrow";
  };
  showWatch?.disconnect();
  showWatch = new IntersectionObserver(([e]) => box.classList.toggle("paused", !e.isIntersecting), { threshold: .3 });
  showWatch.observe(box);
}

async function crew() {
  const box = document.getElementById("crew");
  if (!box) return;
  let all;
  try { all = await gh("contributors?per_page=100"); } catch { return; }
  const people = all.filter(c => c.type === "User");
  if (!view.contains(box) || !people.length) return;
  const shown = people.slice(0, 12);
  box.querySelector(".crew-faces").innerHTML = shown.map(c =>
    `<a href="${esc(c.html_url)}" target="_blank" rel="noopener" title="${esc(c.login)}"><img src="${esc(c.avatar_url)}&s=96" alt="${esc(c.login)}" width="38" height="38" loading="lazy"></a>`).join("");
  box.querySelector(".crew-count").textContent = `${all.length === 100 ? "100+" : people.length} contributors`;
  box.hidden = false;
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
  document.title = "Download openOMSI for Windows, macOS, Linux and Android";
  const grid = document.getElementById("dl-grid");
  const ver = document.getElementById("dl-version");
  if (beta) {
    document.title = "Beta downloads · openOMSI";
    view.querySelector("h1").textContent = "Beta downloads";
    document.getElementById("dl-kicker").textContent = "Beta channel";
    document.getElementById("dl-channel").innerHTML = `Betas are for testing new changes. For everyday use, choose the <a href="/download/">stable release</a>.`;
    document.getElementById("dl-warning-title").textContent = "Beta: for testing";
    document.getElementById("dl-warning-text").innerHTML = `This build may have regressions. Report problems with the version number on the <a href="/issues/">Issues</a> page. You can return to Stable in the launcher's update settings.`;
  }
  let rel;
  try { rel = beta ? await latestBeta() : await latestRelease(); }
  catch (e) { if (view.contains(ver)) { ver.textContent = e.message; document.querySelector(".dl-hero-wait")?.remove(); } return; }
  if (!view.contains(ver)) return;
  document.querySelector(".dl-hero-wait")?.remove();
  if (rel?.failed) {
    ver.innerHTML = `GitHub is not answering right now. <a href="https://github.com/${REPO}/releases/latest">Download from GitHub</a> or try again in a few minutes.`;
    return;
  }
  if (!rel?.tag_name) {
    ver.innerHTML = beta
      ? `There is no beta newer than the stable release. <a href="/download/">Download stable</a> or browse the <a href="/releases/beta/">beta history</a>.`
      : "No stable release has been published yet.";
    return;
  }
  const v = rel.tag_name.replace(/^v/, "");
  if (rel.html_url) document.getElementById("dl-github").href = rel.html_url;
  playingNow();
  ver.innerHTML = `<span class="sign"><span class="sign-line"><span class="material-icons">${beta ? "science" : "directions_bus"}</span></span><span>${beta ? "Beta" : "Stable"} ${esc(v)}</span></span>
    <span>Released ${new Date(rel.published_at).toLocaleDateString("en-GB", { day: "numeric", month: "long", year: "numeric" })}</span>
    <a href="/releases/${beta ? "beta/" : ""}">Release notes</a>`;
  // Each file by its exact name: matched by its ending alone, "-windows-x64.zip" was the
  // dedicated server's zip as well, and the Windows button downloaded the server.
  const find = name => (rel.assets || []).find(a => a.name === name);
  const files = p => {
    const zip = find(`openOMSI-${v}-${p.key}.${p.ext || "zip"}`);
    const setup = p.setup && find(`openOMSI-${v}-${p.key}${p.setup}`);
    return { main: setup || zip, portable: setup && zip };
  };
  const mb = a => `${(a.size / 1048576).toFixed(0)} MB`;
  let scans = {};
  try { scans = JSON.parse((rel.body || "").match(/<!-- virustotal-data:(\{.*?\}) -->/)?.[1] || "{}"); } catch {}
  const scanning = (rel.body || "").includes("<!-- virustotal:start -->");
  const scan = a => {
    const sha = (a?.digest || "").replace(/^sha256:/, "");
    const v = scanVerdict(scanResult(scans, sha));
    if (v.kind === "wait") return scanning && sha ? `<span class="vt vt-wait" title="VirusTotal is scanning this file"><span class="material-icons">shield</span>Scanning…</span>` : "";
    const [cls, icon] = { clean: ["", "verified_user"], checked: ["", "verified_user"], flagged: [" vt-bad", "gpp_bad"] }[v.kind];
    return `<a class="vt${cls}" href="https://www.virustotal.com/gui/file/${esc(sha)}" target="_blank" rel="noopener" title="VirusTotal: ${esc(v.note)}"><span class="material-icons">${icon}</span>${esc(v.label)}</a>`;
  };
  const portable = (f, cls) => f.portable ? `<a class="btn btn-outlined ${cls}" href="${esc(f.portable.browser_download_url)}" download>Portable .zip</a>` : "";
  const row = p => {
    const f = files(p);
    return `<div class="dl-row${p.os && p.os === visitorOs() ? " dl-row-mine" : ""}">
      <span class="material-icons dl-row-icon">${p.icon}</span>
      <span class="dl-row-main"><b>${p.name}</b><span>${p.note}</span></span>
      <span class="dl-size">${f.main ? mb(f.main) : ""}${scan(f.main)}</span>
      <span class="dl-row-actions">${portable(f, "btn-sm")}${f.main
        ? `<a class="btn btn-contained btn-sm" href="${esc(f.main.browser_download_url)}" download><span class="material-icons">download</span>Download</a>`
        : `<span class="muted">Not in this release</span>`}</span></div>`;
  };
  const mine = PLATFORMS.find(p => p.os === visitorOs());
  const mf = mine && files(mine);
  if (mf?.main) document.getElementById("dl-featured").innerHTML = `<div class="dl-hero">
    <span class="material-icons dl-hero-icon">${mine.icon}</span>
    <div class="dl-hero-text"><p class="kicker">Recommended for your system</p><h2>${mine.name}</h2>
      <p>${mine.note} · ${mb(mf.main)}${mine.os === "mac" ? `. On an Intel Mac, take <i>macOS (Intel)</i> below.` : ""}</p>${scan(mf.main) ? `<p class="dl-hero-scan">${scan(mf.main)} <span>${scanResult(scans, (mf.main.digest || "").replace(/^sha256:/, ""))?.n ? "Checked by VirusTotal" : "VirusTotal scan in progress"}</span></p>` : ""}</div>
    <div class="dl-hero-actions"><a class="btn btn-contained btn-lg" href="${esc(mf.main.browser_download_url)}" download><span class="material-icons">download</span>Download ${esc(v)}</a>${portable(mf, "btn-lg")}</div>
  </div>`;
  const group = (title, list) => `<h2 class="section-title">${title}</h2><div class="dl-list">${list.map(row).join("")}</div>`;
  grid.innerHTML = group("All downloads", PLATFORMS) + group("Dedicated server", SERVERS);
}

// openMapper, the map editor: its own repository's latest release, the same way as the game's
const MAPPER_RAW = `https://raw.githubusercontent.com/${MAPPER_REPO}/main`;
// openMapper's own docs are read from its repository, so they are always the current ones
const DOC_SETS = {
  openomsi: { list: DOCS, repo: REPO, url: n => `/docs/${n}.md` },
  mapper: { list: MAPPER_DOCS, repo: MAPPER_REPO, url: n => `${MAPPER_RAW}/docs/${n}.md` },
};

const DOC_NAV = [
  ...DOCS.map(d => ({ ...d, href: docHref("openomsi", d.file) })),
  ...MAPPER_DOCS.map(d => ({ ...d, href: docHref("mapper", d.file) })),
];

function pageToc() {
  const h2s = [...view.querySelectorAll(".doc h2")];
  if (h2s.length > 1) view.querySelector(".toc").innerHTML = `<p>On this page</p>` + h2s.map(h =>
    `<a href="#${h.id}">${esc(h.textContent.replace(/#$/, ""))}</a>`).join("");
  tocSpy();
}

function gallery() {
  showTemplate("gallery");
  document.title = "Screenshots from the community · openOMSI";
}

const shotBox = document.getElementById("shot-box");
let shotAt = null;
function showShot(fig) {
  shotAt = fig;
  const img = fig.querySelector("img");
  shotBox.querySelector("img").src = fig.querySelector("button").dataset.full;
  shotBox.querySelector("img").alt = img.alt;
  shotBox.querySelector(".shot-box-cap").textContent = `${img.alt}. Screenshot by ${fig.querySelector("figcaption").lastChild.textContent}.`;
  if (!shotBox.open) shotBox.showModal();
}
document.addEventListener("click", e => {
  const b = e.target.closest(".wall figure button");
  if (b) return showShot(b.closest("figure"));
  const nav = e.target.closest("[data-shot]");
  if (e.target === shotBox || nav?.dataset.shot === "close") return shotBox.close();
  if (nav) step(Number(nav.dataset.shot));
});
function step(d) {
  const all = [...shotAt.parentElement.children];
  showShot(all[(all.indexOf(shotAt) + d + all.length) % all.length]);
}
shotBox.addEventListener("keydown", e => {
  if (e.key === "ArrowRight" || e.key === "ArrowLeft") { e.preventDefault(); step(e.key === "ArrowRight" ? 1 : -1); }
});
shotBox.addEventListener("close", () => { if (shotAt?.isConnected) shotAt.querySelector("button").focus(); });

function security(anchor) {
  showTemplate("security");
  document.title = "Is openOMSI safe? Security and VirusTotal checks · openOMSI";
  view.querySelectorAll(".doc h2").forEach(h => { h.id = slug(h.textContent); });
  pageToc();
  scanStats();
  toAnchor(anchor);
}

async function scanFigures() {
  let rels;
  try { rels = await Promise.all([latestRelease(), latestBeta().catch(() => null)]); } catch { rels = [{ failed: true }]; }
  const failed = !!rels[0]?.failed;
  const rows = rels.filter(r => r?.tag_name).map(rel => {
    const scanned = (rel.body || "").includes("<!-- virustotal:start -->");
    let data = {};
    try { data = JSON.parse((rel.body || "").match(/<!-- virustotal-data:(\{.*?\}) -->/)?.[1] || "{}"); } catch {}
    const files = [...new Map((rel.assets || []).filter(a => (a.digest || "").startsWith("sha256:") && a.name !== "SHA256SUMS").map(a => [a.digest, a])).values()];
    const done = files.map(a => scanResult(data, a.digest.slice(7))).filter(r => r?.n);
    const kinds = done.map(r => scanVerdict(r).kind);
    return { rel, scanned, files: files.length, done: done.length, checks: done.reduce((t, r) => t + r.n, 0),
      flags: kinds.filter(k => k === "flagged").length, reviewed: kinds.filter(k => k === "checked").length };
  }).filter(r => r.files);
  const sum = k => rows.reduce((t, r) => t + r[k], 0);
  return { failed, rows, done: sum("done"), files: sum("files"), checks: sum("checks"), flags: sum("flags"), reviewed: sum("reviewed"),
    versions: rows.map(r => r.rel.tag_name.replace(/^v/, "")).join(" and ") };
}

async function scanStats() {
  const box = document.getElementById("vt-stats");
  const f = await scanFigures();
  if (!view.contains(box)) return;
  if (f.failed) { box.innerHTML = `<p class="muted">GitHub is not answering right now, so the figures could not be loaded. The results are in the notes of each <a href="/releases/">release</a>.</p>`; return; }
  if (!f.done) { box.innerHTML = `<p class="muted">The current versions have not been checked yet. Each <a href="/releases/">release</a> gets its VirusTotal results in its notes once they are in.</p>`; return; }
  box.innerHTML = `<div class="vt-figures">
      <div><b>${f.done}${f.done < f.files ? ` <small>/ ${f.files}</small>` : ""}</b><span>downloads checked</span></div>
      <div><b>${f.checks.toLocaleString("en-GB")}</b><span>antivirus checks</span></div>
      <div class="${f.flags ? "vt-figure-warn" : "vt-figure-ok"}"><b>${f.flags}</b><span>${f.flags === 1 ? "file flagged" : "files flagged"}</span></div>
    </div>
    <ul class="vt-releases">${f.rows.map(r => {
      const state = r.flags ? ["gpp_maybe", "vt-state-warn"] : r.done ? ["verified_user", "vt-state-ok"] : ["shield", "vt-state-wait"];
      const text = !r.scanned ? "not checked yet" : !r.done ? "scan queued" : `${r.done < r.files ? `${r.done} of ${r.files} downloads checked so far` : `all ${r.files} downloads checked`}, ${r.flags ? `${r.flags} flagged` : r.reviewed ? "no confirmed detection" : "no detection"}`;
      return `<li class="${state[1]}"><span class="material-icons">${state[0]}</span><b>${r.rel.prerelease ? "Beta" : "Stable"} ${esc(r.rel.tag_name.replace(/^v/, ""))}</b><span>${text}</span><a href="${esc(r.rel.html_url)}" target="_blank" rel="noopener">Details</a></li>`;
    }).join("")}</ul><p class="vt-note">Like most new free programs, a download can get a guess from one or two small engines that judge files by how they look, without any known threat. Such a file is shown as Checked: these guesses are false positives, and the engine's maker is told. Every engine's answer is in the release notes.</p>`;
}

async function scanStrip() {
  const box = document.getElementById("vt-strip");
  if (!box) return;
  const f = await scanFigures();
  if (!view.contains(box) || !f.done || f.rows.some(r => !r.scanned)) return;
  const verdict = `<b>${f.checks.toLocaleString("en-GB")}</b> antivirus checks, <b>${f.flags ? `${f.flags} flagged` : f.reviewed ? "no confirmed detection" : "no detection"}</b>`;
  box.querySelector("span:last-child").innerHTML = f.done < f.files
    ? `VirusTotal is checking every download of openOMSI ${esc(f.versions)}: <b>${f.done} of ${f.files}</b> done so far, ${verdict}`
    : `<b>All ${f.files} downloads</b> of openOMSI ${esc(f.versions)} checked by VirusTotal: ${verdict}`;
  box.classList.toggle("vt-strip-warn", f.flags > 0);
  box.hidden = false;
}

let spyQueued = false;
function tocSpy() {
  spyQueued = false;
  const links = [...view.querySelectorAll(".toc a")];
  if (!links.length) return;
  const heads = [...view.querySelectorAll(".doc h2")];
  const atEnd = innerHeight + scrollY >= document.documentElement.scrollHeight - 2;
  const current = atEnd ? heads.length - 1 : Math.max(0, heads.filter(h => h.getBoundingClientRect().top < 120).length - 1);
  links.forEach((a, i) => a.classList.toggle("active", i === current));
}
addEventListener("scroll", () => { if (!spyQueued) { spyQueued = true; requestAnimationFrame(tocSpy); } }, { passive: true });

async function doc(name, anchor, set = "openomsi") {
  const cfg = DOC_SETS[set];
  const meta = cfg.list.find(d => d.file === name);
  const here = docHref(set, name);
  const pre = set === "openomsi" && view.dataset.pre === here && view.querySelector(".doc[data-raw]");
  delete view.dataset.pre;
  let html;
  if (pre) html = pre.innerHTML;
  else {
    view.innerHTML = `<div class="content content-doc"><article class="doc"><div class="loading">Loading…</div></article><aside class="toc" aria-label="On this page"></aside></div>`;
    const wait = view.querySelector(".doc");
    try {
      if (!/^[A-Z0-9_]+$/.test(name) || (!meta && set !== "openomsi")) throw new Error(404);
      const r = await fetch(cfg.url(name));
      if (!r.ok) throw new Error(r.status);
      const [md] = await Promise.all([r.text(), loadMarked()]);
      html = marked.parse(md);
    } catch {
      if (view.contains(wait)) notFound();
      return;
    }
    if (!view.contains(wait)) return;
  }
  const box = view.querySelector(".doc");
  box.removeAttribute("data-raw");
  const siblings = meta ? DOC_NAV.filter(d => d.group === meta.group) : [];
  const at = siblings.findIndex(d => d.href === here);
  const step = (d, label, cls) => d ? `<a class="doc-step ${cls}" href="${d.href}"><span class="muted">${label}</span><b>${d.title}</b></a>` : "<span></span>";
  box.innerHTML = (meta ? `<p class="kicker">${meta.group}</p>` : "") + html
    + `<nav class="doc-steps" aria-label="More pages">${at >= 0 ? step(siblings[at - 1], "Previous", "") + step(siblings[at + 1], "Next", "doc-step-next") : ""}</nav>
    <p class="doc-edit"><a href="https://github.com/${cfg.repo}/blob/main/docs/${name}.md"><span class="material-icons">edit</span>Edit this page on GitHub</a></p>`;
  document.title = `${meta ? meta.title : name} · ${set === "mapper" ? "openMapper" : "openOMSI docs"}`;
  // heading anchors
  box.querySelectorAll("h1, h2, h3, h4").forEach(h => {
    h.id = slug(h.textContent);
    if (h.tagName !== "H1") h.insertAdjacentHTML("beforeend", `<a class="anchor" href="#${h.id}" aria-label="Link to this section">#</a>`);
  });
  box.querySelector("#quick-start-from-download-to-driving + ol")?.classList.add("quickstart");
  pageToc();
  // links between documents stay inside the site; other repository files go to GitHub
  box.querySelectorAll("a[href]").forEach(a => {
    const href = a.getAttribute("href");
    if (href.startsWith("#/")) { a.setAttribute("href", newPath(href.slice(1))); return; }
    if (href.startsWith("#") || href.startsWith("/")) return;
    const site = href.match(/^https:\/\/openomsi\.org(\/.*)$/);
    if (site) { a.setAttribute("href", site[1].startsWith("/#/") ? newPath(site[1].slice(2)) : site[1]); return; }
    if (/^(https?:|mailto:)/.test(href)) return;
    const m = href.match(/^(?:\.\/)?([A-Z_]+)\.md(?:#(.*))?$/);
    if (m && cfg.list.some(d => d.file === m[1])) {
      a.setAttribute("href", docHref(set, m[1]) + (m[2] ? "#" + m[2] : ""));
    } else {
      a.setAttribute("href", `https://github.com/${cfg.repo}/blob/main/docs/${href}`);
    }
  });
  // pictures of openMapper's docs live in its repository
  if (set === "mapper") box.querySelectorAll("img[src]").forEach(i => {
    const src = i.getAttribute("src");
    if (!/^(https?:|data:)/.test(src)) i.src = new URL(src, `${MAPPER_RAW}/docs/`).href;
  });
  toAnchor(anchor);
}

let scrollKeep = false;
function toAnchor(anchor, onlyAnchor) {
  const target = anchor && document.getElementById(anchor);
  if (target) target.scrollIntoView();
  else if (!onlyAnchor && !scrollKeep) window.scrollTo(0, 0);
  scrollKeep = false;
}


// --- Releases and issues, read from the GitHub API (60 requests an hour per visitor
// without a token: every list is fetched once per visit and kept)
const cache = {};
let quota = null;
function apiQuota() {
  if (quota && Date.now() < quota.until) return quota.left;
  quota = { until: Date.now() + 60000, left: fetch("https://api.github.com/rate_limit")
    .then(r => r.ok ? r.json() : null).then(d => d?.resources?.core?.remaining ?? -1).catch(() => -1) };
  return quota.left;
}
const RELEASE_KEYS = ["tag_name", "name", "prerelease", "draft", "published_at", "body", "html_url"];
function slim(data) {
  const pick = (o, keys) => Object.fromEntries(keys.filter(k => k in o).map(k => [k, o[k]]));
  const one = o => o && typeof o === "object" && "tag_name" in o
    ? { ...pick(o, RELEASE_KEYS), assets: (o.assets || []).map(a => pick(a, ["name", "size", "digest", "browser_download_url"])) }
    : o && typeof o === "object" && "number" in o && "title" in o
      ? { ...pick(o, ["number", "title", "state", "updated_at", "created_at", "comments", "pull_request", "html_url", "body"]), user: o.user && pick(o.user, ["login", "avatar_url"]), labels: (o.labels || []).map(l => pick(l, ["name", "color"])) }
      : o;
  return Array.isArray(data) ? data.map(one) : one(data);
}
function keep(path, data) {
  const text = JSON.stringify({ at: Date.now(), data: slim(data) });
  if (text.length > 1500000) return;
  try { localStorage.setItem("gh:" + path, text); } catch {
    try {
      Object.keys(localStorage).filter(k => k.startsWith("gh:")).forEach(k => localStorage.removeItem(k));
      localStorage.setItem("gh:" + path, text);
    } catch {}
  }
}
function stored(path) {
  try { return JSON.parse(localStorage.getItem("gh:" + path)); } catch { return null; }
}
function gh(path) {
  return cache[path] ||= (async () => {
    const kept = stored(path);
    if (kept && Date.now() - kept.at < 600000) return kept.data;
    const left = await apiQuota();
    quota.n ??= left;
    if (quota.n < 1) {
      if (kept) return kept.data;
      throw Object.assign(new Error(quota.n < 0 ? "GitHub could not be reached." : "GitHub's hourly request limit is reached - try again later."), { status: 429 });
    }
    quota.n--;
    const r = await fetch(`https://api.github.com/repos/${REPO}/${path}`, { headers: { Accept: "application/vnd.github+json" } }).catch(() => null);
    if (!r?.ok) {
      if (kept) return kept.data;
      throw Object.assign(new Error(r ? `GitHub answered ${r.status}.` : "GitHub could not be reached."), { status: r?.status || 0 });
    }
    const data = await r.json();
    keep(path, data);
    return data;
  })().catch(e => { delete cache[path]; throw e; });
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
    <p class="lead">${beta ? 'Versions for testing, including older betas. <a href="/beta/">Download the latest beta</a>.' : 'Stable versions, newest first. <a href="/download/">Download the latest stable release</a>.'}</p>
    <nav class="release-channels" aria-label="Release channel"><a class="btn ${beta ? "btn-outlined" : "btn-contained"}" href="/releases/"${beta ? "" : ' aria-current="page"'}>Stable</a> <a class="btn ${beta ? "btn-contained" : "btn-outlined"}" href="/releases/beta/"${beta ? ' aria-current="page"' : ""}>Betas</a></nav>
    <div id="rel-list"><div class="loading">Loading…</div></div></div>`;
  document.title = `${beta ? "Beta" : "Stable"} releases · openOMSI`;
  const box = document.getElementById("rel-list");
  const list = [];
  let page = 0, end = false, stable;
  const fill = async want => {
    while (list.length < want && !end) {
      const batch = await gh(`releases?per_page=30&page=${++page}`);
      list.push(...batch.filter(r => !r.draft && r.prerelease === beta && /^v\d+\.\d+\.\d+$/.test(r.tag_name)));
      end = batch.length < 30;
    }
    list.sort((a, b) => newerVersion(a.tag_name, b.tag_name) ? -1 : newerVersion(b.tag_name, a.tag_name) ? 1 : 0);
  };
  try { [, stable] = await Promise.all([fill(5), latestRelease(), loadMarked()]); }
  catch (e) { box.innerHTML = `<p>${esc(e.message)} <a href="https://github.com/${REPO}/releases">Releases on GitHub</a></p>`; return; }
  if (!view.contains(box)) return;
  if (!list.length) { box.innerHTML = `<p>No ${beta ? "beta" : "stable"} release has been published yet.</p>`; return; }
  const latest = beta
    ? (stable.tag_name ? list.findIndex(r => newerVersion(r.tag_name, stable.tag_name)) : -1)
    : list.findIndex(r => r.tag_name === stable.tag_name);
  let shown = 0;
  const draw = () => {
    shown = Math.min(list.length, shown + 10);
    box.innerHTML = list.slice(0, shown).map((r, i) => {
      const assets = (r.assets || []).map(a => `<a class="asset" href="${esc(a.browser_download_url)}"><span class="material-icons">download</span>${esc(a.name)} <small>${(a.size / 1048576).toFixed(0)} MB</small></a>`).join("");
      return `<details class="card elevation-1 release"${i === latest ? " open" : ""}>
        <summary><h2 class="rel-tag">${esc(r.tag_name.replace(/^v/, ""))}</h2>
          ${i === latest ? `<span class="label" style="--lc:#2da44e">${beta ? "Latest beta" : "Latest"}</span>` : ""}${r.prerelease ? '<span class="label" style="--lc:#bf8700">Pre-release</span>' : ""}
          <span class="rel-name">${esc(r.name && r.name !== r.tag_name ? r.name : "")}</span>
          <span class="spacer"></span><span class="muted">${new Date(r.published_at).toLocaleDateString()}</span></summary>
        <div class="doc rel-body">${md(r.body) || "<p class='muted'>No notes.</p>"}</div>
        ${assets ? `<div class="assets">${assets}</div>` : ""}
      </details>`;
    }).join("") + (shown < list.length || !end ? '<button class="btn btn-outlined" id="rel-more">More releases</button>' : "");
    const more = document.getElementById("rel-more");
    if (more) more.onclick = async () => {
      more.disabled = true;
      try { await fill(shown + 10); } catch (e) { more.textContent = e.message; return; }
      if (view.contains(box)) draw();
    };
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
      <a class="issue-row" href="/issues/${i.number}/">
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
  view.innerHTML = `<div class="content"><p><a href="/issues/"><span class="material-icons" style="font-size:18px;vertical-align:-3px">arrow_back</span> All issues</a></p><div id="issue-box"><div class="loading">Loading…</div></div></div>`;
  const box = document.getElementById("issue-box");
  let i, comments;
  try { [i, comments] = await Promise.all([gh(`issues/${n}`), gh(`issues/${n}/comments?per_page=100`), loadMarked()]); }
  catch (e) { box.innerHTML = `<p>${esc(e.message)}</p>`; return; }
  document.title = `#${i.number} ${i.title} · openOMSI`;
  const post = (who, when, body) => `<div class="card elevation-1 comment"><div class="comment-head"><img src="${esc(who?.avatar_url)}&s=48" alt=""><b>${esc(who?.login)}</b><span class="muted">${ago(when)}</span></div><div class="doc">${md(body) || "<p class='muted'>No description.</p>"}</div></div>`;
  box.innerHTML = `<h1 class="issue-head">${esc(i.title)} <span class="muted">#${i.number}</span></h1>
    <p><span class="label" style="--lc:${i.state === "open" ? "#2da44e" : "#8250df"}">${i.state === "open" ? "Open" : "Closed"}</span> ${labelChips(i.labels)}</p>
    ${post(i.user, i.created_at, i.body)}${comments.map(c => post(c.user, c.created_at, c.body)).join("")}
    <p><a class="btn btn-outlined" href="${esc(i.html_url)}"><span class="material-icons">reply</span>Comment on GitHub</a></p>`;
  window.scrollTo(0, 0);
}

function currentPath() {
  const p = location.pathname.replace(/\/index\.html$/, "/").replace(/\/+$/, "");
  return /^\/(mapper\/)?docs\//i.test(p) ? p.toLowerCase().replace(/_/g, "-") : p || "/";
}

let routed = null;
function route() {
  const path = currentPath();
  let anchor = "";
  try { anchor = decodeURIComponent(location.hash.slice(1)); } catch {}
  if (routed === path) return toAnchor(anchor, true);
  if (shotBox.open) shotBox.close();
  const first = routed === null;
  routed = path;
  scrollKeep = first && !anchor;
  if (!first && !anchor) window.scrollTo(0, 0);
  document.getElementById("mapper-badge")?.toggleAttribute("hidden", !mapperIsNew());
  document.querySelectorAll(".drawer a[data-route]").forEach(a => a.classList.toggle("active", a.dataset.route === path || (a.dataset.route === "/issues" && path.startsWith("/issues/"))));
  const section = /^\/(docs\/openmapper|mapper)/.test(path) ? "mapper" : path === "/beta" ? "download" : path.split("/")[1];
  document.querySelectorAll(".topnav a").forEach(a => a.classList.toggle("active", a.dataset.section === section));
  document.querySelectorAll(".drawer-group").forEach(g => { if (g.dataset.sections.split(" ").includes(section)) g.open = true; });
  document.body.dataset.page = "page";
  toggleDrawer(false);
  document.title = "openOMSI: OMSI 2 rebuilt from scratch, free and open source";
  const docPage = path.match(/^\/(mapper\/)?docs\/([a-z0-9-]+)$/);
  if (docPage) return doc(docPage[2].toUpperCase().replace(/-/g, "_"), anchor, docPage[1] ? "mapper" : "openomsi");
  if (path === "/mapper") return go("/docs/openmapper/", true);
  if (path === "/download") return download();
  if (path === "/security") return security(anchor);
  if (path === "/gallery") return gallery();
  if (path === "/beta") return download(true);
  if (path === "/releases") return releases();
  if (path === "/releases/beta") return releases(true);
  const issuePage = path.match(/^\/issues(?:\/(\d+))?$/);
  if (issuePage) return issues(issuePage[1]);
  if (path !== "/") return notFound();
  if (!scrollKeep) window.scrollTo(0, 0);
  scrollKeep = false;
  return home();
}

function go(href, replace) {
  history[replace ? "replaceState" : "pushState"](null, "", href);
  route();
}

function notFound() {
  showTemplate("notfound");
  document.title = "Not found · openOMSI";
  window.scrollTo(0, 0);
}
function oldHash() {
  if (!location.hash.startsWith("#/")) return false;
  history.replaceState(null, "", newPath(location.hash.slice(1)));
  return true;
}
addEventListener("hashchange", () => { if (oldHash()) route(); });
addEventListener("popstate", route);
document.addEventListener("toggle", e => {
  const d = e.target;
  if (d.open && d.name) document.querySelectorAll(`details[name="${d.name}"][open]`).forEach(o => { if (o !== d) o.open = false; });
}, true);
document.addEventListener("click", e => {
  const a = e.target.closest("a[href]");
  if (!a || e.defaultPrevented || e.button || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey || a.target || a.hasAttribute("download")) return;
  const url = new URL(a.href, location.href);
  if (url.origin !== location.origin || /^\/(privacy|legal)\//.test(url.pathname) || /\.[a-z0-9]+$/i.test(url.pathname)) return;
  if (url.pathname === location.pathname && url.hash) return;
  e.preventDefault();
  go(url.pathname + url.hash);
});
document.querySelector(".skip").onclick = e => { e.preventDefault(); document.getElementById("main").focus(); };
latestRelease().then(r => { if (r.tag_name) document.getElementById("drawer-version").textContent = `Stable ${r.tag_name.replace(/^v/, "")}`; });

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
oldHash();
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
