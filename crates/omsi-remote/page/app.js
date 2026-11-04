"use strict";

const $ = (id) => document.getElementById(id);

const store = {
  get(k) { try { return localStorage.getItem(k); } catch (_) { return null; } },
  set(k, v) { try { v == null ? localStorage.removeItem(k) : localStorage.setItem(k, v); } catch (_) {} },
};

const App = {
  ws: null,
  token: store.get("openomsi.token"),
  vehicle: null,
  layout: null,
  scope: null,
  state: {},
  page: Math.max(0, Number(store.get("openomsi.page")) || 0),
  nodes: [],
  pressed: new Map(),
  retry: 0,
  frame: 0,
  qrPin: null,
  pairing: false,
  pending: null,
  saving: null,
  conn: ["wait", "Connecting…"],
  sign: null,
  rtt: null,

  orient() { return innerWidth >= innerHeight ? "l" : "p"; },

  send(msg) {
    if (!this.ws || this.ws.readyState !== 1) return false;
    this.ws.send(JSON.stringify(msg));
    return true;
  },

  press(d, down, w) {
    switch (d.type) {
      case "act":
        if (w && w.mode === "pulse") {
          if (down) { this.send({ t: "act", name: d.name, down: true }); this.send({ t: "act", name: d.name, down: false }); }
        } else this.send({ t: "act", name: d.name, down });
        break;
      case "game": if (down) this.send({ t: "game", name: d.name }); break;
      case "indicator": if (down) this.send({ t: "indicator", value: d.value }); break;
      case "startup": if (down) this.send({ t: "startup" }); break;
      case "saloon": if (down) this.send({ t: "saloon" }); break;
      case "clearline": if (down) this.send({ t: "clearline" }); break;
      case "sign": if (!down) this.routeSheet(); break;
      case "page": if (!down) this.showPage(Number(d.value) || 0); break;
    }
  },

  releaseAll() {
    const held = [...this.pressed];
    this.pressed.clear();
    for (const [node, [d, w]] of held) {
      node.classList.remove("down");
      if (node.release) node.release();
      if (d.type === "act") this.press(d, false, w);
    }
  },

  dropDetached() {
    for (const [node, [d, w]] of this.pressed) {
      if (node.isConnected) continue;
      this.pressed.delete(node);
      if (d.type === "act") this.press(d, false, w);
    }
  },

  status(state, key) {
    this.conn = [state, key];
    document.body.classList.toggle("offline", state !== "on");
    $("conn").dataset.state = state;
    const rtt = state === "on" && this.rtt != null ? ` · ${this.rtt} ms` : "";
    $("conn-text").textContent = R.T(key) + rtt;
  },

  connect() {
    if (this.ws && this.ws.readyState < 2) return;
    clearTimeout(this.retryTimer);
    this.status("wait", "Connecting…");
    const ws = new WebSocket(`ws://${location.host}/ws`);
    this.ws = ws;
    ws.onopen = () => {
      const pin = (location.hash.match(/pin=(\d{6})/) || [])[1];
      if (pin) { this.qrPin = pin; history.replaceState(null, "", location.pathname); }
      if (this.token) ws.send(JSON.stringify({ t: "hello", token: this.token }));
      else if (this.qrPin) this.pairWith(this.qrPin);
      else if (this.pairing && $("pin").value.replace(/\D/g, "").length === 6) this.pair();
      else this.showPair();
    };
    ws.onmessage = (e) => {
      this.heard = performance.now();
      let m;
      try { m = JSON.parse(e.data); } catch (_) { return; }
      this.receive(m);
    };
    ws.onclose = () => this.dropped(ws);
  },

  dropped(ws) {
    if (this.ws !== ws) return;
    this.releaseAll();
    this.pressed.clear();
    Drive.letGo();
    if (this.conn[1] !== "Too many devices are connected") this.status("off", "Disconnected");
    this.ws = null;
    const wait = Math.min(8000, 500 * 2 ** this.retry++);
    this.retryTimer = setTimeout(() => this.connect(), wait);
  },

  ping() {
    const ws = this.ws;
    const now = performance.now();
    if (ws && ws.readyState === 1 && this.conn[0] === "on" && this.pingAt && this.heard < this.pingAt && now - this.pingAt > 6000) {
      this.dropped(ws);
      try { ws.close(); } catch (_) {}
      return;
    }
    if (!this.pingAt || this.heard >= this.pingAt) this.pingAt = now;
    this.send({ t: "ping", id: Math.round(now) });
  },

  receive(m) {
    switch (m.t) {
      case "pong":
        if (typeof m.id === "number") this.rtt = Math.max(1, Math.round(performance.now() - m.id));
        if (this.conn[0] === "on") this.status("on", "Connected");
        break;
      case "paired":
        this.token = m.token;
        this.qrPin = null;
        this.pairing = false;
        store.set("openomsi.token", m.token);
        $("pair").hidden = true;
        break;
      case "denied":
        if (m.reason === "token") {
          this.token = null;
          store.set("openomsi.token", null);
          if (this.qrPin) this.pairWith(this.qrPin);
          else this.showPair();
        } else if (m.reason === "full") {
          this.retry = 5;
          this.status("off", "Too many devices are connected");
          $("pin-error").textContent = R.T("Too many devices are connected");
        } else {
          this.qrPin = null;
          this.pairing = false;
          const msg = { pin: "Wrong code", locked: "Too many tries, wait two minutes", closed: "Open Settings → Remote cockpit in the game to pair this device" }[m.reason];
          if ($("pair").hidden) this.showPair();
          $("pin-error").textContent = R.T(msg || "Wrong code");
          $("pin-go").disabled = false;
        }
        break;
      case "texts":
        R.texts = m.texts || {};
        document.documentElement.lang = m.lang || "en";
        this.translateStatic();
        if (!Editor.active) this.render();
        break;
      case "vehicle":
        this.retry = 0;
        $("pair").hidden = true;
        this.status("on", "Connected");
        if (Editor.active) {
          if (this.vehicle && m.key === this.vehicle.key) { this.pending = m; break; }
          Editor.cancel();
        }
        this.takeVehicle(m);
        break;
      case "roads":
      case "nav":
        MapView.take(m);
        this.schedule();
        break;
      case "sign":
        this.sign = m.has ? `/vt/~sign.png?k=${encodeURIComponent(this.token || "")}&v=${m.v}` : null;
        for (const [node, w, kind] of this.nodes) if (w.kind === "sign") kind.update(node, w, this.state);
        break;
      case "layout.done":
        Editor.saved(!!m.ok);
        break;
      case "state":
        if (m.full) this.state = { vehicle: m.vehicle, env: m.env };
        else {
          if (m.vehicle) this.state.vehicle = Object.assign({}, this.state.vehicle, m.vehicle);
          if (m.env) this.state.env = m.env;
        }
        this.state.doorways = R.doorways(this.state);
        if ("paused" in m) $("paused").hidden = !m.paused;
        if (this.vehicle && !this.layout && this.state.vehicle && !Editor.active) { this.layout = defaultLayout(this.state); this.scope = "default"; this.render(); }
        this.schedule();
        break;
    }
  },

  takeVehicle(m) {
    const same = JSON.stringify(m);
    if (same === this.lastVehicle && !this.pending) { this.status("on", "Connected"); return; }
    this.lastVehicle = same;
    this.pending = null;
    this.status("on", "Connected");
    this.vehicle = m.key ? m : null;
    this.scope = m.layout ? m.layout.scope : null;
    this.layout = m.layout ? normalize(m.layout.data) : null;
    if (this.layout && !this.layout.pages.length) this.layout = null;
    if (!this.layout && this.vehicle && this.state.vehicle) { this.layout = defaultLayout(this.state); this.scope = "default"; }
    $("vehicle-name").textContent = m.name || "";
    $("empty").hidden = !!m.key;
    this.render();
  },

  showPair() {
    if (!$("pair").hidden) return;
    $("pair").hidden = false;
    $("pin").value = "";
    $("pin-error").textContent = "";
    $("pin-go").disabled = false;
    this.status("wait", "Not paired");
    setTimeout(() => $("pin").focus(), 50);
  },

  pairWith(pin) {
    $("pair").hidden = false;
    $("pin").value = pin;
    this.pair();
  },

  pair() {
    if ($("pin-go").disabled) return;
    const pin = $("pin").value.replace(/\D/g, "");
    if (pin.length !== 6) { $("pin-error").textContent = R.T("The code has six digits"); return; }
    $("pin-go").disabled = true;
    $("pin-error").textContent = "";
    this.pairing = true;
    const name = /iPad/.test(navigator.userAgent) ? "iPad" : /iPhone/.test(navigator.userAgent) ? "iPhone" : /Android/.test(navigator.userAgent) ? "Android" : "Browser";
    if (!this.send({ t: "pair", pin, name })) {
      $("pin-go").disabled = false;
      this.connect();
    }
  },

  translateStatic() {
    for (const e of document.querySelectorAll("[data-t]")) e.textContent = R.T(e.dataset.t);
    $("btn-edit").title = R.T("Edit the layout");
    $("btn-full").title = R.T("Full screen");
    this.status(...this.conn);
  },

  currentPage() {
    const pages = (this.layout && this.layout.pages) || [];
    if (this.page >= pages.length || this.page < 0) this.page = 0;
    return pages[this.page];
  },

  showPage(i) {
    this.page = i;
    store.set("openomsi.page", String(i));
    this.render();
  },

  applyTheme() {
    const l = this.layout || {};
    document.documentElement.dataset.theme = l.theme || "night";
    document.documentElement.style.setProperty("--accent", l.accent || "#f5a623");
    const bg = getComputedStyle(document.documentElement).getPropertyValue("--bg").trim();
    document.querySelector('meta[name="theme-color"]').content = bg || "#0f1115";
  },

  renderTabs() {
    const tabs = $("tabs");
    tabs.textContent = "";
    const pages = (this.layout && this.layout.pages) || [];
    pages.forEach((p, i) => {
      const b = R.el("button", null, R.T(p.name || ""));
      b.setAttribute("aria-selected", String(i === this.page));
      b.onclick = () => this.showPage(i);
      tabs.append(b);
    });
    tabs.hidden = pages.length < 2 && !Editor.active;
  },

  render() {
    this.releaseAll();
    Drive.letGo();
    this.applyTheme();
    this.renderTabs();
    const board = $("board");
    board.textContent = "";
    this.nodes = [];
    const page = this.currentPage();
    if (!page) return;
    const o = this.orient();
    if (!page.grid[o]) page.grid[o] = o === "l" ? [8, 5] : [4, 8];
    const [cols, minRows] = page.grid[o];
    const rows = Math.max(minRows, pack(page.widgets, cols, o, minRows));
    page.grid[o][1] = rows;
    board.style.gridTemplateColumns = `repeat(${cols}, minmax(0, 1fr))`;
    board.style.gridTemplateRows = `repeat(${rows}, minmax(0, 1fr))`;
    for (const w of page.widgets) {
      const kind = KINDS[w.kind];
      if (!kind || !w.pos[o]) continue;
      const [x, y, ww, hh] = w.pos[o];
      const cell = R.el("div", `w style-${w.style || "default"}`);
      cell.dataset.id = w.id;
      cell.style.gridColumn = `${x + 1} / span ${Math.min(ww, cols - x)}`;
      cell.style.gridRow = `${y + 1} / span ${hh}`;
      try {
        cell.append(kind.make(w));
      } catch (e) {
        continue;
      }
      board.append(cell);
      this.nodes.push([cell.firstChild, w, kind]);
    }
    this.update();
    if (Editor.active) Editor.decorate();
  },

  schedule() {
    if (this.frame) return;
    this.frame = requestAnimationFrame(() => { this.frame = 0; this.update(); });
  },

  update() {
    const s = this.state;
    for (const [node, w, kind] of this.nodes) {
      if (!kind.update) continue;
      try { kind.update(node, w, s); } catch (_) {}
    }
  },

  sheet(title, body) {
    $("sheet-title").textContent = title;
    const b = $("sheet-body");
    b.textContent = "";
    b.append(body);
    $("sheet").hidden = false;
  },

  closeSheet() { $("sheet").hidden = true; },

  listButton(main, sub, onclick, on) {
    const b = R.el("button", on ? "on" : null);
    b.append(R.el("span", null, main));
    if (sub) b.append(R.el("span", "sub", sub));
    b.onclick = onclick;
    return b;
  },

  filtered(list, render) {
    const wrap = R.el("div");
    const search = R.el("input", "search");
    search.placeholder = R.T("Search");
    search.type = "search";
    const box = R.el("div", "list");
    const fill = () => {
      const q = search.value.trim().toLowerCase();
      box.textContent = "";
      for (const item of list) {
        const node = render(item);
        if (!q || node.textContent.toLowerCase().includes(q)) box.append(node);
      }
    };
    search.oninput = fill;
    fill();
    wrap.append(search, box);
    return wrap;
  },

  routeSheet() {
    const depot = this.vehicle && this.vehicle.depot;
    const body = R.el("div");
    const seg = R.el("div", "seg");
    const pane = R.el("div");
    pane.style.marginTop = "10px";
    const tabs = [
      ["Routes", () => {
        const routes = (depot && depot.routes) || [];
        if (!routes.length) return R.el("p", null, R.T("The bus has no depot file"));
        return this.filtered(routes, (r) => this.listButton(`${r.line} · ${r.destination || r.name}`, r.name, () => { this.send({ t: "route", index: r.index }); this.closeSheet(); }));
      }],
      ["Destinations", () => {
        const list = (depot && depot.destinations) || [];
        if (!list.length) return R.el("p", null, R.T("The bus has no depot file"));
        return this.filtered(list, (d) => this.listButton(d.name, String(d.code), () => { this.send({ t: "destination", index: d.index }); this.closeSheet(); }));
      }],
      ["Line", () => {
        const f = R.el("div");
        const label = R.el("label", "field");
        label.append(R.el("span", null, R.T("Line number")));
        const input = R.el("input");
        input.inputMode = "numeric";
        input.maxLength = 16;
        label.append(input);
        const row = R.el("div", "actions-row");
        const ok = R.el("button", "primary", R.T("Set the line"));
        ok.onclick = () => { if (input.value.trim()) { this.send({ t: "line", line: input.value.trim() }); this.closeSheet(); } };
        const clear = R.el("button", "danger", R.T("Clear line"));
        clear.onclick = () => { this.send({ t: "clearline" }); this.closeSheet(); };
        row.append(ok, clear);
        f.append(label, row);
        setTimeout(() => input.focus(), 50);
        return f;
      }],
    ];
    const show = (i) => {
      [...seg.children].forEach((b, k) => b.classList.toggle("on", k === i));
      pane.textContent = "";
      pane.append(tabs[i][1]());
    };
    tabs.forEach(([name], i) => {
      const b = R.el("button", null, R.T(name));
      b.onclick = () => show(i);
      seg.append(b);
    });
    body.append(seg, pane);
    this.sheet(R.T("Destination sign"), body);
    show(0);
  },

  confirmNextStop(st) {
    const body = R.el("div");
    body.append(R.el("p", null, st.name));
    const row = R.el("div", "actions-row");
    const ok = R.el("button", "primary", R.T("Continue from this stop"));
    ok.onclick = () => { this.send({ t: "nextstop", index: st.index }); this.closeSheet(); };
    row.append(ok);
    body.append(row);
    this.sheet(R.T("Next stop"), body);
  },

  toast(text) {
    const t = $("toast");
    t.textContent = text;
    t.hidden = false;
    clearTimeout(this.toastTimer);
    this.toastTimer = setTimeout(() => (t.hidden = true), 2200);
  },

  init() {
    for (const b of document.querySelectorAll("[data-icon]")) b.append(R.icon(b.dataset.icon));
    this.translateStatic();
    $("pin-go").onclick = () => this.pair();
    $("pin").addEventListener("keydown", (e) => { if (e.key === "Enter") this.pair(); });
    $("pin").addEventListener("input", () => { if ($("pin").value.replace(/\D/g, "").length === 6) this.pair(); });
    $("sheet-close").onclick = () => this.closeSheet();
    $("sheet").addEventListener("click", (e) => { if (e.target.id === "sheet") this.closeSheet(); });
    $("btn-edit").onclick = () => (Editor.active ? Editor.cancel() : Editor.start());
    const full = document.documentElement.requestFullscreen || document.documentElement.webkitRequestFullscreen;
    if (!full) $("btn-full").hidden = true;
    $("btn-full").onclick = () => {
      if (document.fullscreenElement || document.webkitFullscreenElement) (document.exitFullscreen || document.webkitExitFullscreen).call(document);
      else full.call(document.documentElement);
    };
    let last = this.orient();
    addEventListener("resize", () => {
      const o = this.orient();
      if (o !== last) { last = o; this.render(); }
    });
    document.addEventListener("visibilitychange", () => {
      this.pingAt = 0;
      if (document.visibilityState === "visible") this.connect();
      else this.releaseAll();
    });
    addEventListener("pagehide", () => this.releaseAll());
    document.addEventListener("gesturestart", (e) => e.preventDefault());
    for (const type of ["lostpointercapture", "pointercancel"]) document.addEventListener(type, () => this.dropDetached(), true);
    setInterval(() => this.ping(), 2000);
    this.connect();
  },
};

App.init();
