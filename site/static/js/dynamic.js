// The live parts: the latest version and the players on the home page, the download list, the
// release notes and the issues.
//
// The releases and the issue list are baked into the site when it is built (assets/data/, see
// src/github.mjs), so these pages fill in at once from our own server. GitHub is asked in the
// background for what may be newer; if it does not answer (it limits visitors to 60 requests an
// hour and sometimes gives out error pages) nothing is lost.
(() => {
  const REPO = "openOMSI-org/openOMSI";
  const PRESENCE = "https://openomsi.savvabestbrother.workers.dev";
  const base = document.body.dataset.base || "/";
  const { PLATFORMS, SERVERS } = window.OMSI;
  const $ = (s, el = document) => el.querySelector(s);
  const esc = (t) => String(t ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
  const mb = (n) => `${(n / 1048576).toFixed(0)} MB`;
  const date = (s) => new Date(s).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
  const units = [[31536000, "year"], [2592000, "month"], [86400, "day"], [3600, "hour"], [60, "minute"]];
  const ago = (d) => {
    const s = (Date.now() - new Date(d)) / 1000;
    for (const [n, u] of units) if (s >= n) { const k = Math.floor(s / n); return `${k} ${u}${k > 1 ? "s" : ""} ago`; }
    return "just now";
  };
  const newer = (a, b) => {
    const x = a.replace(/^v/, "").split(".").map(Number), y = b.replace(/^v/, "").split(".").map(Number);
    for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] > y[i];
    return false;
  };
  const byVersion = (a, b) => (newer(a.tag_name, b.tag_name) ? -1 : newer(b.tag_name, a.tag_name) ? 1 : 0);

  // ---- our own copy of GitHub's data
  const own = async (path) => {
    const r = await fetch(`${base}assets/data/${path}`);
    if (!r.ok) throw new Error(String(r.status));
    return r.json();
  };
  let indexRequest;
  const index = () => (indexRequest ||= own("github.json").catch(() => null));
  const fragments = {};
  const fullRelease = async (entry) => {
    if (entry.assets && "body_html" in entry) return entry;
    fragments[entry.tag_name] ||= own(`releases/${entry.tag_name}.json`).then((f) => ({ ...entry, ...f }));
    return fragments[entry.tag_name];
  };

  // ---- GitHub itself, for what is newer than the build (one small request, answered or not)
  async function live(path, html = true) {
    const key = `gh:${html ? "h:" : ""}${path}`;
    try { const c = JSON.parse(sessionStorage.getItem(key) || "null"); if (c && Date.now() - c.t < 300000) return c.v; } catch {}
    const r = await fetch(`https://api.github.com/repos/${REPO}/${path}`, { headers: { Accept: html ? "application/vnd.github.html+json" : "application/vnd.github+json" } });
    if (!r.ok) throw new Error(`GitHub answered ${r.status}.`);
    const v = await r.json();
    try { sessionStorage.setItem(key, JSON.stringify({ t: Date.now(), v })); } catch {}
    return v;
  }
  // true when GitHub knows a newer stable release than the build did (it is then put in the index)
  async function checkFresh() {
    const idx = await index();
    try {
      const rel = await live("releases/latest");
      if (!rel.tag_name) return false;
      if (idx && rel.tag_name === idx.latest) return false;
      const entry = {
        tag_name: rel.tag_name, name: rel.name, prerelease: false, published_at: rel.published_at, html_url: rel.html_url,
        assets: (rel.assets || []).map((a) => ({ name: a.name, size: a.size, browser_download_url: a.browser_download_url })), body_html: rel.body_html,
      };
      if (idx) { idx.latest = rel.tag_name; idx.releases = [entry, ...idx.releases.filter((r) => r.tag_name !== rel.tag_name)]; }
      else indexRequest = Promise.resolve({ latest: rel.tag_name, releases: [entry], issues: null });
      return true;
    } catch { return false; }
  }

  async function latestStable() {
    const idx = await index();
    const entry = idx?.releases.find((r) => r.tag_name === idx.latest);
    return entry ? fullRelease(entry) : null;
  }
  async function latestBeta() {
    const idx = await index();
    if (!idx) return null;
    const entry = idx.releases.filter((r) => r.prerelease && newer(r.tag_name, idx.latest)).sort(byVersion)[0];
    return entry ? fullRelease(entry) : null;
  }

  // ---- which build fits the visitor
  function visitorOs() {
    const ua = navigator.userAgent || "";
    if (/Android/i.test(ua)) return "android";
    if (/Windows/i.test(ua)) return /ARM|aarch64/i.test(ua) ? "windows-arm" : "windows";
    if (/Mac OS X|Macintosh/i.test(ua)) return "mac";
    if (/Linux/i.test(ua)) return /aarch64|arm64/i.test(ua) ? "linux-arm" : "linux";
    return "";
  }
  const asset = (rel, p) => {
    const v = rel.tag_name.replace(/^v/, "");
    const find = (n) => (rel.assets || []).find((a) => a.name === n);
    const zip = find(`openOMSI-${v}-${p.key}.${p.ext || "zip"}`);
    const setup = p.setup && find(`openOMSI-${v}-${p.key}${p.setup}`);
    return { main: setup || zip, zip: setup && zip ? zip : null };
  };

  // ---- home
  async function home() {
    playing();
    const draw = async () => {
      const dl = $("#hero-download"), ver = $("#home-version"), notes = $("#home-notes");
      const rel = await latestStable();
      if (!rel) {
        ver.textContent = "";
        notes.innerHTML = `<p class="muted">The release notes are on <a href="https://github.com/${REPO}/releases">GitHub</a>.</p>`;
        return;
      }
      const v = rel.tag_name.replace(/^v/, "");
      ver.textContent = `Latest release ${v}`;
      $("#get-version").textContent = `Version ${v}, ${date(rel.published_at)}`;
      const mine = PLATFORMS.find((p) => p.os === visitorOs());
      const first = mine && asset(rel, mine).main;
      if (mine && first) { dl.textContent = `Download for ${mine.name}${mine.arch ? ` (${mine.arch})` : ""}`; dl.href = first.browser_download_url; dl.title = first.name; }
      $("#get-others").innerHTML = PLATFORMS.filter((p) => p !== mine).map((p) => {
        const a = asset(rel, p).main;
        return a ? `<li><a href="${esc(a.browser_download_url)}">${esc(p.name)} ${esc(p.arch)}</a><span class="num">${mb(a.size)}</span></li>` : "";
      }).join("");
      notes.innerHTML = rel.body_html || `<p class="muted">No notes.</p>`;
      for (const a of notes.querySelectorAll("a[href]")) a.rel = "noopener";
    };
    await draw();
    if (await checkFresh()) draw();
  }
  async function playing() {
    const el = $("#home-players");
    if (!el) return;
    try {
      const r = await fetch(`${PRESENCE}/players`);
      const p = r.ok ? await r.json() : null;
      if (p && typeof p.players === "number") {
        el.textContent = `${p.players} playing now`;
        el.title = Object.entries(p.systems || {}).map(([os, n]) => `${os}: ${n}`).join(", ");
        el.hidden = false;
      }
    } catch { /* the counter is out of reach: it stays hidden */ }
  }

  // ---- download
  async function download(beta) {
    const ver = $("#dl-version"), games = $("#dl-games"), servers = $("#dl-servers");
    const draw = async () => {
      const rel = beta ? await latestBeta() : await latestStable();
      if (!rel) {
        ver.innerHTML = beta
          ? `There is no beta newer than the stable release. <a href="${base}download/">Download stable</a> or see the <a href="${base}releases/beta/">beta history</a>.`
          : `The release list could not be read. Every file is on <a href="https://github.com/${REPO}/releases/latest">GitHub</a>.`;
        games.innerHTML = servers.innerHTML = "";
        return;
      }
      const v = rel.tag_name.replace(/^v/, "");
      if (rel.html_url) $("#dl-github").href = rel.html_url;
      ver.innerHTML = `${beta ? "Latest beta" : "Stable"} <b>${esc(v)}</b> · ${date(rel.published_at)} · <a href="${base}releases/${beta ? "beta/" : ""}">release notes</a>`;
      const mine = visitorOs();
      const row = (p) => {
        const a = asset(rel, p);
        const hit = p.os && p.os === mine;
        return `<div class="dl-row${hit ? " mine" : ""}"><div class="dl-name">${esc(p.name)}<small>${esc(p.arch)}${hit ? " · your system" : ""}</small></div>
          <div class="dl-note">${esc(p.note)}${a.main ? `<span class="num muted">${mb(a.main.size)}</span>` : ""}</div>
          <div class="dl-actions">${a.main ? `<a class="btn${hit ? " primary" : ""}" href="${esc(a.main.browser_download_url)}" download>Download ${esc(a.main.name.match(/\.[a-z0-9]+$/)[0])}</a>` : `<span class="muted">Not in this release</span>`}${a.zip ? `<a class="btn" href="${esc(a.zip.browser_download_url)}" download>Portable .zip</a>` : ""}</div></div>`;
      };
      games.innerHTML = [...PLATFORMS].sort((x, y) => (y.os === mine) - (x.os === mine)).map(row).join("");
      servers.innerHTML = SERVERS.map(row).join("");
      const os = { windows: "windows", "windows-arm": "windows", mac: "macos", "mac-intel": "macos", linux: "linux", "linux-arm": "linux", android: "android" }[mine];
      if (os) $(`#os-${os}`)?.classList.add("mine");
    };
    await draw();
    if (!beta && (await checkFresh())) draw();
  }

  // ---- releases
  async function releases(beta) {
    const box = $("#rel-list");
    const idx = await index();
    if (!idx) { box.innerHTML = `<p>The release list could not be read. <a href="https://github.com/${REPO}/releases">Releases on GitHub</a></p>`; return; }
    const list = idx.releases.filter((r) => r.prerelease === beta).sort(byVersion);
    if (!list.length) { box.innerHTML = `<p class="muted">No ${beta ? "beta" : "stable"} release has been published yet.</p>`; return; }
    const latest = beta ? list.findIndex((r) => newer(r.tag_name, idx.latest)) : list.findIndex((r) => r.tag_name === idx.latest);
    let shown = 0;
    // a release's notes and files are fetched when it is opened: a small file of our own
    const draw = () => {
      shown += 25;
      box.innerHTML = list.slice(0, shown).map((r, i) => `<details class="rel" data-tag="${esc(r.tag_name)}"${i === latest ? " open" : ""}>
          <summary><span class="rel-tag">${esc(r.tag_name.replace(/^v/, ""))}</span>${i === latest ? `<span class="label ok">${beta ? "Latest beta" : "Latest"}</span>` : ""}${r.prerelease ? `<span class="label warn">Pre-release</span>` : ""}
          <span class="rel-name">${esc(r.name && r.name !== r.tag_name ? r.name : "")}</span><time datetime="${esc(r.published_at)}">${date(r.published_at)}</time></summary>
          <div class="rel-body gh" data-body><p class="muted">Loading…</p></div><div class="assets" data-assets></div></details>`).join("")
        + (shown < list.length ? `<button class="btn" id="rel-more" type="button">Older releases</button>` : "");
      const more = $("#rel-more");
      if (more) more.onclick = draw;
      for (const d of box.querySelectorAll("details.rel")) {
        const fill = async () => {
          if (d.dataset.done) return;
          d.dataset.done = "1";
          const entry = list.find((x) => x.tag_name === d.dataset.tag);
          try {
            const r = await fullRelease(entry);
            d.querySelector("[data-body]").innerHTML = r.body_html || `<p class="muted">No notes.</p>`;
            d.querySelector("[data-assets]").innerHTML = (r.assets || []).map((a) => `<a class="asset" href="${esc(a.browser_download_url)}">${esc(a.name)} <small>${mb(a.size)}</small></a>`).join("");
            for (const a of d.querySelectorAll(".gh a[href]")) a.rel = "noopener";
          } catch { d.querySelector("[data-body]").innerHTML = `<p class="muted">The notes could not be loaded. <a href="${esc(entry.html_url)}">Open them on GitHub</a>.</p>`; }
        };
        if (d.open) fill();
        d.addEventListener("toggle", () => d.open && fill());
      }
    };
    draw();
  }

  // ---- issues: the baked list at once, GitHub's own when it answers
  let state = "open", query = "";
  const labels = (ls) => (ls || []).map((l) => `<span class="label" data-c="${esc(l.color)}">${esc(l.name)}</span>`).join(" ");
  const paint = (scope) => scope.querySelectorAll(".label[data-c]").forEach((el) => { if (/^[0-9a-f]{6}$/i.test(el.dataset.c)) el.style.setProperty("--lc", `#${el.dataset.c}`); });
  async function issues() {
    const root = $("#issues-root");
    const n = new URLSearchParams(location.search).get("n");
    if (n && /^\d+$/.test(n)) return issue(root, n);
    root.innerHTML = `<div class="issue-bar"><div class="tabs"><button data-s="open">Open</button><button data-s="closed">Closed</button><button data-s="all">All</button></div>
      <input id="iq" type="search" placeholder="Filter by title, label or number" value="${esc(query)}" aria-label="Filter issues"></div><div id="issue-list"><p class="muted">Loading…</p></div><p class="muted" id="issue-note"></p>`;
    root.querySelectorAll(".tabs button").forEach((b) => { b.classList.toggle("on", b.dataset.s === state); b.onclick = () => { state = b.dataset.s; issues(); }; });
    const box = $("#issue-list"), note = $("#issue-note");
    const idx = await index();
    let list = idx?.issues ? (state === "all" ? [...idx.issues.open, ...idx.issues.closed].sort((a, b) => b.updated_at.localeCompare(a.updated_at)) : idx.issues[state]) : null;
    const draw = () => {
      if (!list) { box.innerHTML = `<p>The issue list could not be read. <a href="https://github.com/${REPO}/issues">Issues on GitHub</a></p>`; return; }
      const q = query.trim().toLowerCase();
      const hits = list.filter((i) => !q || i.title.toLowerCase().includes(q) || (i.labels || []).some((l) => l.name.toLowerCase().includes(q)) || String(i.number) === q.replace("#", ""));
      box.innerHTML = hits.length ? `<div class="issue-list">${hits.map((i) => `<a class="issue-row" href="${base}issues/?n=${i.number}"><span class="st ${i.state}" aria-label="${i.state}"></span>
        <span><span class="issue-title">${esc(i.title)}</span>${labels(i.labels)}<span class="issue-meta">#${i.number} · ${esc(i.user?.login)} · updated ${ago(i.updated_at)}</span></span>
        <span class="issue-count">${i.comments ? `${i.comments} comment${i.comments > 1 ? "s" : ""}` : ""}</span></a>`).join("")}</div>` : `<p class="muted">Nothing here.</p>`;
      paint(box);
    };
    $("#iq").oninput = (e) => { query = e.target.value; draw(); };
    draw();
    const stamp = idx?.generated ? date(idx.generated) : "";
    if (stamp) note.textContent = `Shown as of ${stamp}; checking GitHub for newer…`;
    try {
      const fresh = (await live(`issues?state=${state}&per_page=100&sort=updated`, false)).filter((i) => !i.pull_request);
      if ($("#issue-list") !== box) return; // (another tab was chosen meanwhile)
      list = fresh; note.textContent = ""; draw();
    } catch { if (stamp && $("#issue-note") === note) note.textContent = `Shown as of ${stamp}. GitHub did not answer, so newer changes may be missing.`; }
  }
  async function issue(root, n) {
    root.innerHTML = `<p><a class="back" href="${base}issues/">All issues</a></p><div id="issue-box"><p class="muted">Loading…</p></div>`;
    const box = $("#issue-box");
    let i, comments;
    try { [i, comments] = await Promise.all([live(`issues/${n}`), live(`issues/${n}/comments?per_page=100`)]); }
    catch {
      const idx = await index();
      const b = idx?.issues && [...idx.issues.open, ...idx.issues.closed].find((x) => String(x.number) === n);
      box.innerHTML = `${b ? `<h2 class="issue-h">${esc(b.title)} <span class="muted">#${b.number}</span></h2><p>${labels(b.labels)}</p>` : ""}<p class="muted">GitHub did not answer, so the discussion cannot be shown now.</p><p><a class="btn" href="https://github.com/${REPO}/issues/${esc(n)}">Open it on GitHub</a></p>`;
      paint(box);
      return;
    }
    document.title = `#${i.number} ${i.title} · openOMSI`;
    const post = (who, when, html) => `<div class="comment"><div class="comment-head"><img src="${esc(who?.avatar_url)}&s=48" alt="" width="24" height="24"><b>${esc(who?.login)}</b><span class="muted">${ago(when)}</span></div><div class="gh">${html || "<p class='muted'>No description.</p>"}</div></div>`;
    box.innerHTML = `<h2 class="issue-h">${esc(i.title)} <span class="muted">#${i.number}</span></h2>
      <p><span class="label ${i.state === "open" ? "ok" : ""}">${i.state === "open" ? "Open" : "Closed"}</span> ${labels(i.labels)}</p>
      ${post(i.user, i.created_at, i.body_html)}${comments.map((c) => post(c.user, c.created_at, c.body_html)).join("")}
      <p><a class="btn" href="${esc(i.html_url)}">Comment on GitHub</a></p>`;
    paint(box);
    for (const a of box.querySelectorAll(".gh a[href]")) a.rel = "noopener";
  }

  const page = document.querySelector("main")?.dataset.page;
  const channel = document.querySelector("main")?.dataset.channel === "beta";
  if (document.body.classList.contains("home")) home();
  else if (page === "download") download(channel);
  else if (page === "releases") releases(channel);
  else if (page === "issues") issues();
})();
