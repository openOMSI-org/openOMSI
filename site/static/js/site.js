// Everything the static pages gain from a script: theme buttons, the docs menu on small screens,
// search (/ or Ctrl+K), copy buttons, filters for long tables, the "on this page" marker.
(() => {
  const root = document.documentElement;
  const base = document.body.dataset.base || "/";
  const $ = (s, el = document) => el.querySelector(s);
  const $$ = (s, el = document) => [...el.querySelectorAll(s)];
  const esc = (t) => String(t).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

  // ---- theme: system -> light -> dark
  const setTheme = (t) => {
    if (t) root.dataset.theme = t; else delete root.dataset.theme;
    try { if (t) localStorage.setItem("theme", t); else localStorage.removeItem("theme"); } catch {}
    $$("[data-theme-set]").forEach((b) => b.setAttribute("aria-pressed", String((b.dataset.themeSet || "") === (t || ""))));
    const names = { "": "system", light: "light", dark: "dark" };
    $$("[data-theme-cycle]").forEach((b) => { b.title = `Colour theme: ${names[t || ""]}`; b.setAttribute("aria-label", b.title); });
  };
  $$("[data-theme-set]").forEach((b) => b.addEventListener("click", () => setTheme(b.dataset.themeSet)));
  $$("[data-theme-cycle]").forEach((b) => b.addEventListener("click", () => setTheme({ "": "light", light: "dark", dark: "" }[root.dataset.theme || ""])));
  setTheme(root.dataset.theme || "");

  // ---- docs menu on a narrow screen
  const toggle = $("[data-nav-toggle]");
  if (toggle) {
    toggle.hidden = false;
    toggle.addEventListener("click", () => {
      const open = document.body.classList.toggle("nav-open");
      toggle.setAttribute("aria-expanded", String(open));
    });
  }

  // ---- copy buttons on code
  $$("figure.code").forEach((fig) => {
    const code = $("pre", fig);
    const b = document.createElement("button");
    b.type = "button"; b.className = "copy"; b.textContent = "Copy"; b.setAttribute("aria-label", "Copy the code");
    b.addEventListener("click", async () => {
      try { await navigator.clipboard.writeText(code.innerText.replace(/\n$/, "")); b.textContent = "Copied"; b.classList.add("done"); }
      catch {
        b.textContent = "Press Ctrl+C";
        const r = document.createRange(); r.selectNodeContents(code);
        const s = getSelection(); s.removeAllRanges(); s.addRange(r);
      }
      setTimeout(() => { b.textContent = "Copy"; b.classList.remove("done"); }, 1600);
    });
    fig.appendChild(b);
  });

  // ---- filter for long tables
  $$(".table-wrap[data-filter]").forEach((wrap) => {
    const rows = $$("tbody tr", wrap);
    const bar = document.createElement("div");
    bar.className = "table-filter";
    bar.innerHTML = `<input type="search" placeholder="Filter these ${rows.length} rows" aria-label="Filter the table"><span aria-live="polite"></span>`;
    wrap.before(bar);
    const input = $("input", bar), count = $("span", bar);
    input.addEventListener("input", () => {
      const q = input.value.trim().toLowerCase();
      let n = 0;
      for (const r of rows) { const hit = !q || r.textContent.toLowerCase().includes(q); r.classList.toggle("filtered", !hit); if (hit) n++; }
      count.textContent = q ? `${n} of ${rows.length}` : "";
    });
  });

  // ---- "on this page": mark the section in view
  const toc = $$(".docs-toc a");
  if (toc.length && "IntersectionObserver" in window) {
    const byId = new Map(toc.map((a) => [decodeURIComponent(a.hash.slice(1)), a]));
    const heads = [...byId.keys()].map((id) => document.getElementById(id)).filter(Boolean);
    let current = null;
    const mark = (a) => {
      if (!a || a === current) return;
      if (current) current.classList.remove("on");
      current = a; a.classList.add("on");
      a.scrollIntoView({ block: "nearest" });
    };
    const io = new IntersectionObserver((entries) => {
      const seen = entries.filter((e) => e.isIntersecting).sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)[0];
      if (seen) mark(byId.get(seen.target.id));
    }, { rootMargin: "-72px 0px -70% 0px" });
    heads.forEach((h) => io.observe(h));
  }

  // ---- search
  const dlg = $("#search");
  if (!dlg || typeof dlg.showModal !== "function") return;
  const input = $("#search-input"), box = $("#search-results");
  let index = null, loading = null, sel = 0, shown = [];
  $$("[data-search-open]").forEach((b) => { b.hidden = false; b.addEventListener("click", open); });
  $(".search-close", dlg).addEventListener("click", (e) => { e.preventDefault(); dlg.close(); });
  dlg.addEventListener("click", (e) => { if (e.target === dlg) dlg.close(); });
  document.addEventListener("keydown", (e) => {
    const typing = /^(INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName) || e.target.isContentEditable;
    if ((e.key === "/" && !typing && !e.metaKey && !e.ctrlKey) || ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k")) { e.preventDefault(); open(); }
  });
  function load() {
    if (!loading) {
      loading = fetch(`${base}assets/search-index.json`).then((r) => r.json()).then((rows) => {
        index = rows.map(([url, doc, page, head, text]) => ({ url, doc, page, head, text, lt: text.toLowerCase(), lh: `${head} ${page} ${doc}`.toLowerCase(), lhead: head.toLowerCase() }));
      });
    }
    return loading;
  }
  function open() { dlg.showModal(); input.select(); draw([]); load().then(run); }
  function run() {
    const terms = input.value.toLowerCase().split(/\s+/).filter(Boolean);
    if (!index || !terms.length) { shown = []; draw(shown, terms); return; }
    const out = [];
    for (const r of index) {
      let score = 0, ok = true;
      for (const t of terms) {
        let s = 0;
        const h = r.lh.indexOf(t);
        if (h >= 0) s += (h === 0 || /[^a-z0-9]/.test(r.lh[h - 1])) ? 14 : 8;
        if (r.lhead.includes(t)) s += 6;
        if (r.lt.includes(t)) s += 2;
        if (!s) { ok = false; break; }
        score += s;
      }
      if (ok) out.push([score, r]);
    }
    out.sort((a, b) => b[0] - a[0]);
    // not more than three hits from one page
    const per = new Map(); shown = [];
    for (const [, r] of out) {
      const k = r.url.split("#")[0], n = per.get(k) || 0;
      if (n < 3) { shown.push(r); per.set(k, n + 1); }
      if (shown.length >= 30) break;
    }
    sel = 0; draw(shown, terms);
  }
  const hl = (text, terms) => {
    let s = esc(text);
    for (const t of terms) s = s.replace(new RegExp(esc(t).replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "gi"), (m) => `<mark>${m}</mark>`);
    return s;
  };
  function snippet(r, terms) {
    const i = terms.map((t) => r.lt.indexOf(t)).filter((n) => n >= 0).sort((a, b) => a - b)[0];
    if (i === undefined) return r.text.slice(0, 130);
    const from = Math.max(0, i - 50);
    return (from ? "…" : "") + r.text.slice(from, from + 150) + "…";
  }
  function draw(list, terms = []) {
    if (!input.value.trim()) { box.innerHTML = `<div class="sr-empty">${index ? `Search ${index.length} sections of the documentation.` : "Loading the index…"}</div>`; return; }
    if (!list.length) { box.innerHTML = `<div class="sr-empty">${index ? "Nothing found." : "Loading the index…"}</div>`; return; }
    box.innerHTML = list.map((r, i) => `<a class="sr" role="option" href="${base}${r.url.replace(/^\//, "")}" aria-selected="${i === sel}"><b>${hl(r.head || r.page || r.doc, terms)}</b><small>${esc([r.doc, r.page && r.head ? r.page : ""].filter(Boolean).join(" › "))}</small><p>${hl(snippet(r, terms), terms)}</p></a>`).join("");
  }
  function move(d) {
    if (!shown.length) return;
    sel = (sel + d + shown.length) % shown.length;
    $$(".sr", box).forEach((a, i) => { a.setAttribute("aria-selected", String(i === sel)); if (i === sel) a.scrollIntoView({ block: "nearest" }); });
  }
  input.addEventListener("input", run);
  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") { e.preventDefault(); move(1); }
    else if (e.key === "ArrowUp") { e.preventDefault(); move(-1); }
    else if (e.key === "Enter" && shown[sel]) { e.preventDefault(); location.href = `${base}${shown[sel].url.replace(/^\//, "")}`; }
  });
})();
