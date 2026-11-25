// The live parts of the home, download, release and issue pages.
//
// These pages are written complete when the site is built (src/releases.mjs): the latest version,
// the download buttons, the release notes and the issue list are in the HTML, and the visitor's
// system is picked by the style sheet (theme.js sets html[data-os]). Nothing here blanks or waits:
// it only asks GitHub in the background whether something is newer than the build, and redraws the
// part that changed when it is. GitHub limits visitors to 60 requests an hour and sometimes answers
// with an error page; then the page simply stays as it was built.
(() => {
  const REPO = "openOMSI-org/openOMSI";
  const PRESENCE = "https://openomsi.savvabestbrother.workers.dev";
  const base = document.body.dataset.base || "/";
  const { PLATFORMS, SERVERS } = window.OMSI;
  const $ = (s, el = document) => el.querySelector(s);
  const $$ = (s, el = document) => [...el.querySelectorAll(s)];
  const esc = (t) => String(t ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
  const mb = (n) => `${(n / 1048576).toFixed(0)} MB`;
  const date = (s) => new Date(s).toLocaleDateString("en-US", { year: "numeric", month: "short", day: "numeric", timeZone: "UTC" });
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

  // ---- our own copy of GitHub's data (only for what the page does not hold)
  const own = async (path) => {
    const r = await fetch(`${base}assets/data/${path}`);
    if (!r.ok) throw new Error(String(r.status));
    return r.json();
  };
  const built = () => document.querySelector("main")?.dataset.latest || "";

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
  // the latest stable release when GitHub knows a newer one than the page was built with, else null
  async function fresher() {
    try {
      const rel = await live("releases/latest");
      if (!rel.tag_name || !built() || !newer(rel.tag_name, built())) return null;
      return { ...rel, assets: (rel.assets || []).map((a) => ({ name: a.name, size: a.size, browser_download_url: a.browser_download_url })) };
    } catch { return null; }
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
    const rel = await fresher();
    if (!rel) return;
    const v = rel.tag_name.replace(/^v/, "");
    $("#home-version").textContent = `Latest release ${v}`;
    $("#get-version").textContent = `Version ${v}, ${date(rel.published_at)}`;
    const buttons = [`<a class="btn primary wide hero-dl" data-for="any" href="${base}download/">Download ${esc(v)}</a>`];
    const others = [];
    for (const p of PLATFORMS) {
      const a = asset(rel, p).main;
      if (!a) continue;
      buttons.push(`<a class="btn primary wide hero-dl" data-for="${esc(p.os)}" href="${esc(a.browser_download_url)}" title="${esc(a.name)}">Download for ${esc(p.name)} (${esc(p.arch)})</a>`);
      others.push(`<li data-os="${esc(p.os)}"><a href="${esc(a.browser_download_url)}">${esc(p.name)} ${esc(p.arch)}</a><span class="num">${mb(a.size)}</span></li>`);
    }
    $("#hero-download").innerHTML = buttons.join("");
    $("#get-others").innerHTML = others.join("");
    if (rel.body_html) { const n = $("#home-notes"); n.innerHTML = rel.body_html; for (const a of $$("a[href]", n)) a.rel = "noopener"; }
  }
  // the players counter: the last number seen shows at once, the server's answer replaces it
  async function playing() {
    const el = $("#home-players");
    if (!el) return;
    const show = (p) => {
      el.textContent = `${p.players} playing now`;
      el.title = Object.entries(p.systems || {}).map(([os, n]) => `${os}: ${n}`).join(", ");
      el.hidden = false;
    };
    try { const c = JSON.parse(localStorage.getItem("players") || "null"); if (c && Date.now() - c.t < 86400000) show(c.p); } catch {}
    try {
      const r = await fetch(`${PRESENCE}/players`);
      const p = r.ok ? await r.json() : null;
      if (p && typeof p.players === "number") {
        show(p);
        try { localStorage.setItem("players", JSON.stringify({ t: Date.now(), p })); } catch {}
      }
    } catch { /* the counter is out of reach: the last number (or nothing) stays */ }
  }

  // ---- download: only a stable release newer than the build changes anything
  async function download(beta) {
    if (beta) return;
    const rel = await fresher();
    if (!rel) return;
    const v = rel.tag_name.replace(/^v/, "");
    $("#dl-version").innerHTML = `Stable <b>${esc(v)}</b> · ${date(rel.published_at)} · <a href="${base}releases/">release notes</a>`;
    if (rel.html_url) $("#dl-github").href = rel.html_url;
    const rows = (list) => list.map((p) => {
      const a = asset(rel, p);
      const ext = a.main ? a.main.name.match(/\.[a-z0-9]+$/)[0] : "";
      return `<div class="dl-row" data-os="${esc(p.os || "")}"><div class="dl-name">${esc(p.name)}<small>${esc(p.arch)}<span class="yours"> · your system</span></small></div>
        <div class="dl-note">${esc(p.note)}${a.main ? `<span class="num muted">${mb(a.main.size)}</span>` : ""}</div>
        <div class="dl-actions">${a.main ? `<a class="btn dl-main" href="${esc(a.main.browser_download_url)}" download>Download ${esc(ext)}</a>` : `<span class="muted">Not in this release</span>`}${a.zip ? `<a class="btn" href="${esc(a.zip.browser_download_url)}" download>Portable .zip</a>` : ""}</div></div>`;
    }).join("");
    $("#dl-games").innerHTML = rows(PLATFORMS);
    $("#dl-servers").innerHTML = rows(SERVERS);
  }

  // ---- releases: the newest are in the page; older ones are shown on request, their notes read
  // from our own small file when one is opened
  function releases() {
    const box = $("#rel-list");
    const more = $("#rel-more");
    if (more) more.onclick = () => { box.classList.add("all"); more.remove(); };
    for (const d of $$("details.rel", box)) {
      const fill = async () => {
        if (d.dataset.done) return;
        d.dataset.done = "1";
        try {
          const r = await own(`releases/${d.dataset.tag}.json`);
          $("[data-body]", d).innerHTML = r.body_html || `<p class="muted">No notes.</p>`;
          $("[data-assets]", d).innerHTML = (r.assets || []).map((a) => `<a class="asset" href="${esc(a.browser_download_url)}">${esc(a.name)} <small>${mb(a.size)}</small></a>`).join("");
          for (const a of $$(".gh a[href]", d)) a.rel = "noopener";
        } catch { $("[data-body]", d).innerHTML = `<p class="muted">The notes could not be loaded. <a href="https://github.com/${REPO}/releases/tag/${esc(d.dataset.tag)}">Open them on GitHub</a>.</p>`; }
      };
      if (d.open) fill();
      d.addEventListener("toggle", () => d.open && fill());
    }
  }

  // ---- issues: every row is in the page; the tabs and the filter hide rows
  const paint = (scope) => $$(".label[data-c]", scope).forEach((el) => el.style.setProperty("--lc", `#${el.dataset.c}`));
  const labels = (ls) => (ls || []).map((l) => `<span class="label"${/^[0-9a-f]{6}$/i.test(l.color) ? ` data-c="${esc(l.color)}"` : ""}>${esc(l.name)}</span>`).join(" ");
  const issueRow = (i) => {
    const q = [i.title, `#${i.number}`, ...(i.labels || []).map((l) => l.name)].join(" ").toLowerCase();
    return `<a class="issue-row" data-state="${esc(i.state)}" data-q="${esc(q)}" data-updated="${esc(i.updated_at)}" href="${base}issues/?n=${i.number}"><span class="st ${esc(i.state)}" aria-label="${esc(i.state)}"></span>
      <span><span class="issue-title">${esc(i.title)}</span>${labels(i.labels)}<span class="issue-meta">#${i.number} · ${esc(i.user?.login)} · updated ${date(i.updated_at)}</span></span>
      <span class="issue-count">${i.comments ? `${i.comments} comment${i.comments > 1 ? "s" : ""}` : ""}</span></a>`;
  };
  function issues() {
    const root = $("#issues-root");
    const n = new URLSearchParams(location.search).get("n");
    if (n && /^\d+$/.test(n)) return issue(root, n);
    const box = $("#issue-list"), empty = $("#issue-empty"), input = $("#iq"), tabs = $(".tabs", root);
    let state = "open";
    const apply = () => {
      const q = input.value.trim().toLowerCase().replace(/^#/, "");
      let shown = 0;
      for (const row of $$(".issue-row", box)) {
        const ok = (state === "all" || row.dataset.state === state) && (!q || row.dataset.q.includes(q));
        row.hidden = !ok;
        if (ok) shown++;
      }
      empty.hidden = shown > 0;
    };
    tabs.hidden = false; input.hidden = false;
    for (const b of $$("button", tabs)) b.onclick = () => { state = b.dataset.s; $$("button", tabs).forEach((x) => x.classList.toggle("on", x === b)); apply(); };
    input.oninput = apply;
    paint(box);
    apply();
    // GitHub's current list, quietly: rows that changed are replaced, new ones added
    (async () => {
      try {
        const fresh = [];
        for (const s of ["open", "closed"]) fresh.push(...(await live(`issues?state=${s}&per_page=100&sort=updated`, false)).filter((i) => !i.pull_request));
        const list = $(".issue-list", box);
        if (!list) return;
        const known = new Map($$(".issue-row", list).map((r) => [r.getAttribute("href"), r]));
        let changed = false;
        for (const i of fresh) {
          const href = `${base}issues/?n=${i.number}`, row = known.get(href);
          if (row && row.dataset.updated === i.updated_at && row.dataset.state === i.state) continue;
          changed = true;
          known.set(href, null);
          row?.remove();
          list.insertAdjacentHTML("afterbegin", issueRow(i));
        }
        if (changed) {
          [...$$(".issue-row", list)].sort((a, b) => b.dataset.updated.localeCompare(a.dataset.updated)).forEach((r) => list.appendChild(r));
          paint(list);
          apply();
        }
      } catch { /* GitHub did not answer: the built list stays */ }
    })();
  }
  async function issue(root, n) {
    // the row from the built list shows at once; the discussion comes from GitHub
    const row = $(`.issue-row[href$="?n=${n}"]`, root);
    const title = row ? $(".issue-title", row).textContent : "";
    const labelHtml = row ? $$(".label", row).map((l) => l.outerHTML).join(" ") : "";
    root.innerHTML = `<p><a class="back" href="${base}issues/">All issues</a></p><div id="issue-box">${title ? `<h2 class="issue-h">${esc(title)} <span class="muted">#${esc(n)}</span></h2><p>${labelHtml}</p>` : ""}<p class="muted" id="issue-wait">Loading the discussion…</p></div>`;
    const box = $("#issue-box");
    paint(box);
    let i, comments;
    try { [i, comments] = await Promise.all([live(`issues/${n}`), live(`issues/${n}/comments?per_page=100`)]); }
    catch {
      $("#issue-wait").outerHTML = `<p class="muted">GitHub did not answer, so the discussion cannot be shown now.</p><p><a class="btn" href="https://github.com/${REPO}/issues/${esc(n)}">Open it on GitHub</a></p>`;
      return;
    }
    document.title = `#${i.number} ${i.title} · openOMSI`;
    const post = (who, when, html) => `<div class="comment"><div class="comment-head"><img src="${esc(who?.avatar_url)}&s=48" alt="" width="24" height="24"><b>${esc(who?.login)}</b><span class="muted">${ago(when)}</span></div><div class="gh">${html || "<p class='muted'>No description.</p>"}</div></div>`;
    box.innerHTML = `<h2 class="issue-h">${esc(i.title)} <span class="muted">#${i.number}</span></h2>
      <p><span class="label ${i.state === "open" ? "ok" : ""}">${i.state === "open" ? "Open" : "Closed"}</span> ${labels(i.labels)}</p>
      ${post(i.user, i.created_at, i.body_html)}${comments.map((c) => post(c.user, c.created_at, c.body_html)).join("")}
      <p><a class="btn" href="${esc(i.html_url)}">Comment on GitHub</a></p>`;
    paint(box);
    for (const a of $$(".gh a[href]", box)) a.rel = "noopener";
  }

  const main = document.querySelector("main");
  const page = main?.dataset.page;
  const beta = main?.dataset.channel === "beta";
  if (document.body.classList.contains("home")) home();
  else if (page === "download") download(beta);
  else if (page === "releases") releases();
  else if (page === "issues") issues();
})();
