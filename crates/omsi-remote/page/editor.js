"use strict";

const THEMES = [["night", "Night"], ["cockpit", "Cockpit"], ["day", "Day"], ["amber", "Amber LED"], ["contrast", "High contrast"]];
const STYLES = [["vdv", "Dashboard"], ["default", "Standard"], ["round", "Round"], ["rocker", "Rocker"], ["flat", "Flat"], ["key", "Key"]];
const COLORS = ["", "#f5a623", "#e03131", "#2f9e44", "#1c7ed6", "#7048e8", "#f08c00", "#ffffff", "#495057"];
const ACTION_TYPES = [["act", "Bus key"], ["game", "Game"], ["indicator", "Indicator"], ["startup", "Start the bus"], ["saloon", "Saloon lights"], ["sign", "Destination sign"], ["clearline", "Clear line"], ["page", "Page"], ["none", "Nothing"]];
const GAME_ACTIONS = ["view_set_driver", "view_set_passenger", "view_set_outside", "view_set_map", "view_toggle_viewpoint", "view_reset_direction", "view_reset_all_directions", "view_interiorcam_minus", "view_interiorcam_plus", "view_set_schedule", "view_set_ticketselling", "sim_pause", "open_menu", "screenshot", "photo_mode", "quicksave", "voice_radio"];

const clone = (x) => JSON.parse(JSON.stringify(x));

const Editor = {
  active: false,
  backup: null,
  sel: null,
  undo: [],
  drag: null,

  start() {
    if (!App.layout) return;
    this.drag = null;
    this.backup = clone(App.layout);
    this.undo = [];
    this.sel = null;
    this.active = true;
    document.body.classList.add("editing");
    Drive.letGo();
    $("btn-edit").setAttribute("aria-pressed", "true");
    this.bar();
    App.render();
  },

  stop() {
    this.active = false;
    this.drag = null;
    this.sel = null;
    document.body.classList.remove("editing");
    $("btn-edit").setAttribute("aria-pressed", "false");
    const bar = $("edit-bar");
    if (bar) bar.remove();
    if (App.pending) App.takeVehicle(App.pending);
    else App.render();
  },

  cancel() {
    if (this.backup) App.layout = this.backup;
    this.stop();
  },

  snapshot() {
    this.undo.push({ layout: clone(App.layout), page: App.page });
    if (this.undo.length > 60) this.undo.shift();
    this.bar();
  },

  page() { return App.currentPage(); },
  selected() { const p = this.page(); return p && p.widgets.find((w) => w.id === this.sel); },

  button(icon, label, onclick, cls) {
    const b = R.el("button", cls || null);
    if (icon) b.append(R.icon(icon));
    if (label) b.append(R.el("span", null, R.T(label)));
    b.onclick = onclick;
    return b;
  },

  bar() {
    let bar = $("edit-bar");
    if (!bar) { bar = R.el("div"); bar.id = "edit-bar"; document.body.append(bar); }
    bar.textContent = "";
    const has = !!this.selected();
    bar.append(this.button("add", "Add", () => this.addSheet()));
    if (has) {
      bar.append(this.button("tune", "Properties", () => this.propsSheet()));
      bar.append(this.button("content_copy", "Duplicate", () => this.duplicate()));
      bar.append(this.button("delete", "Delete", () => this.remove()));
    }
    bar.append(this.button("view_list", "Pages", () => this.pagesSheet()));
    bar.append(this.button("palette", "Look", () => this.lookSheet()));
    const u = this.button("history", "", () => this.restore());
    u.disabled = !this.undo.length;
    u.title = R.T("Undo");
    bar.append(u);
    bar.append(this.button("close", "Cancel", () => this.cancel()));
    bar.append(this.button("save", "Save", () => this.saveSheet(), "primary"));
  },

  restore() {
    if (!this.undo.length) return;
    const last = this.undo.pop();
    App.layout = last.layout;
    App.page = last.page;
    this.bar();
    App.render();
  },

  select(id) {
    this.sel = id;
    this.bar();
    this.decorate();
  },

  decorate() {
    for (const cell of document.querySelectorAll("#board .w")) {
      cell.classList.toggle("sel", cell.dataset.id === this.sel);
      const old = cell.querySelector(".handle");
      if (old) old.remove();
      if (cell.dataset.id === this.sel) {
        const h = R.el("div", "handle");
        h.addEventListener("pointerdown", (e) => this.begin(e, cell, "size"));
        cell.append(h);
      }
      if (!cell.dataset.bound) {
        cell.dataset.bound = "1";
        cell.addEventListener("pointerdown", (e) => { if (!e.target.classList.contains("handle")) this.begin(e, cell, "move"); });
      }
    }
    const board = $("board");
    if (!board.dataset.bound) {
      board.dataset.bound = "1";
      board.addEventListener("pointerdown", (e) => { if (this.active && e.target === board) this.select(null); });
      board.addEventListener("pointermove", (e) => this.move(e));
      board.addEventListener("pointerup", (e) => this.end(e));
      board.addEventListener("pointercancel", (e) => this.end(e));
    }
    const p = this.page();
    if (p) {
      const [cols, rows] = p.grid[App.orient()];
      const r = board.getBoundingClientRect();
      board.style.setProperty("--cell-w", `${(r.width - this.gap()) / cols}px`);
      board.style.setProperty("--cell-h", `${(r.height - this.gap()) / rows}px`);
    }
  },

  gap() { return parseFloat(getComputedStyle($("board")).paddingLeft) || 8; },

  begin(e, cell, kind) {
    if (!this.active || this.drag) return;
    e.preventDefault();
    e.stopPropagation();
    const w = this.page().widgets.find((x) => x.id === cell.dataset.id);
    if (!w) return;
    if (this.sel !== w.id) this.select(w.id);
    const p = this.page();
    const o = App.orient();
    const [cols, rows] = p.grid[o];
    const r = $("board").getBoundingClientRect();
    const g = this.gap();
    this.drag = { kind, w, cell, x0: e.clientX, y0: e.clientY, pos: [...w.pos[o]], cw: (r.width - g) / cols, ch: (r.height - g) / rows, cols, rows, moved: false, finger: e.pointerId };
    try { $("board").setPointerCapture(e.pointerId); } catch (_) {}
  },

  candidate(e, d) {
    const dx = Math.round((e.clientX - d.x0) / d.cw);
    const dy = Math.round((e.clientY - d.y0) / d.ch);
    let [x, y, w, h] = d.pos;
    if (d.kind === "move") {
      x = Math.max(0, Math.min(d.cols - w, x + dx));
      y = Math.max(0, Math.min(d.rows - h, y + dy));
    } else {
      w = Math.max(1, Math.min(d.cols - x, w + dx));
      h = Math.max(1, Math.min(d.rows - y, h + dy));
    }
    return [x, y, w, h];
  },

  move(e) {
    const d = this.drag;
    if (!d || e.pointerId !== d.finger) return;
    if (Math.abs(e.clientX - d.x0) + Math.abs(e.clientY - d.y0) > 6) d.moved = true;
    const [x, y, w, h] = this.candidate(e, d);
    d.cell.style.gridColumn = `${x + 1} / span ${w}`;
    d.cell.style.gridRow = `${y + 1} / span ${h}`;
    d.cell.classList.toggle("ghost", this.collides(d.w, [x, y, w, h]));
  },

  collides(w, [x, y, ww, hh]) {
    const o = App.orient();
    return this.page().widgets.some((v) => {
      if (v === w || !v.pos[o]) return false;
      const [a, b, c, dd] = v.pos[o];
      return x < a + c && a < x + ww && y < b + dd && b < y + hh;
    });
  },

  end(e) {
    const d = this.drag;
    if (!d || e.pointerId !== d.finger) return;
    this.drag = null;
    if (e.type === "pointercancel") { App.render(); return; }
    d.cell.classList.remove("ghost");
    if (!d.moved) { App.render(); return; }
    const pos = this.candidate(e, d);
    if (!this.collides(d.w, pos)) {
      this.snapshot();
      d.w.pos[App.orient()] = pos;
    }
    App.render();
  },

  addWidget(w) {
    this.snapshot();
    const p = this.page();
    const o = App.orient();
    p.widgets.push(w);
    pack(p.widgets, p.grid[o][0], o, p.grid[o][1]);
    const other = o === "l" ? "p" : "l";
    if (p.grid[other]) pack(p.widgets, p.grid[other][0], other, p.grid[other][1]);
    this.sel = w.id;
    App.closeSheet();
    App.render();
    this.bar();
  },

  duplicate() {
    const w = this.selected();
    if (!w) return;
    const c = clone(w);
    const [, , ww, hh] = w.pos[App.orient()];
    Object.assign(c, { id: R.uid(), pos: {}, sz: { l: [ww, hh], p: [ww, hh] } });
    this.addWidget(c);
    delete c.sz;
  },

  remove() {
    const p = this.page();
    if (!p || !this.sel) return;
    this.snapshot();
    p.widgets = p.widgets.filter((w) => w.id !== this.sel);
    this.sel = null;
    this.bar();
    App.render();
  },

  addSheet() {
    const body = R.el("div");
    body.append(R.el("h3", null, R.T("Widgets")));
    const kinds = R.el("div", "list");
    for (const [kind, def] of Object.entries(KINDS)) {
      kinds.append(App.listButton(R.T(def.name), "", () => {
        const w = widget(kind, kind === "button" ? { label: "Button", icon: "touch_app", do: { type: "none" } } : kind === "display" ? { show: "speed" } : kind === "label" ? { label: "Label" } : {});
        this.addWidget(w);
      }));
    }
    body.append(kinds);
    body.append(R.el("h3", null, R.T("Ready-made buttons")));
    body.append(App.filtered(PRESETS, (p) => {
      const b = App.listButton(R.T(p.label), "", () => this.addWidget(presetWidget(p.id)));
      if (p.icon) b.prepend(R.icon(p.icon));
      return b;
    }));
    App.sheet(R.T("Add"), body);
  },

  field(label, input) {
    const f = R.el(input.tagName === "INPUT" || input.tagName === "SELECT" ? "label" : "div", "field");
    f.append(R.el("span", null, R.T(label)), input);
    return f;
  },

  input(value, onchange, type) {
    const i = R.el("input");
    i.type = type || "text";
    i.value = value ?? "";
    i.onchange = () => onchange(i.value);
    return i;
  },

  dropdown(options, value, onchange) {
    const s = R.el("select");
    for (const [v, label] of options) {
      const o = R.el("option", null, R.T(label));
      o.value = v;
      if (v === value) o.selected = true;
      s.append(o);
    }
    s.onchange = () => onchange(s.value);
    return s;
  },

  seg(options, value, onchange) {
    const s = R.el("div", "seg");
    for (const [v, label] of options) {
      const b = R.el("button", v === value ? "on" : null, R.T(label));
      b.onclick = (e) => { e.preventDefault(); onchange(v); };
      s.append(b);
    }
    return s;
  },

  swatches(value, onchange) {
    const s = R.el("div", "swatches");
    for (const c of COLORS) {
      const b = R.el("button", c === (value || "") ? "on" : null);
      b.style.background = c || "var(--btn)";
      b.title = c ? c : R.T("Theme");
      b.onclick = (e) => { e.preventDefault(); onchange(c); };
      s.append(b);
    }
    const pick = R.el("input");
    pick.type = "color";
    pick.value = value || "#f5a623";
    pick.onchange = () => onchange(pick.value);
    s.append(pick);
    return s;
  },

  change(w, f) {
    this.snapshot();
    f(w);
    App.render();
    this.propsSheet();
  },

  propsSheet() {
    const w = this.selected();
    if (!w) return;
    const body = R.el("div");
    const set = (k) => (v) => this.change(w, (x) => { if (v === "" || v == null) delete x[k]; else x[k] = v; });
    body.append(this.field("Label", this.input(w.label ? R.T(w.label) : "", set("label"))));
    if (w.kind === "button") {
      body.append(this.field("Look", this.seg(STYLES, w.style || "default", (v) => set("style")(v === "default" ? "" : v))));
      const iconRow = R.el("div", "actions-row");
      const ib = this.button(w.picto ? "" : w.icon || "add", w.icon || w.picto ? "" : "Icon", () => this.iconSheet(w));
      if (w.picto && picto(w.picto)) ib.prepend(picto(w.picto));
      iconRow.append(ib);
      if (w.icon || w.picto) iconRow.append(this.button("close", "No icon", () => this.change(w, (x) => { delete x.icon; delete x.picto; })));
      body.append(this.field("Icon", iconRow));
      body.append(this.field("Colour", this.swatches(w.color, set("color"))));
      const d = w.do || { type: "none" };
      body.append(this.field("Does", this.dropdown(ACTION_TYPES, d.type, (v) => this.change(w, (x) => { x.do = { type: v }; if (v === "indicator") x.do.value = 3; if (v === "page") x.do.value = 0; }))));
      if (d.type === "act" || d.type === "game") {
        const row = R.el("div", "row2");
        row.append(this.input(d.name, (v) => this.change(w, (x) => { x.do.name = v.trim(); })));
        row.append(this.button("search", "Choose", () => this.actionSheet(w)));
        body.append(this.field(d.type === "act" ? "Key or trigger of the bus" : "Game action", row));
      }
      if (d.type === "act") body.append(this.field("Press", this.seg([["key", "While held"], ["pulse", "Short press"]], w.mode || "key", (v) => set("mode")(v === "key" ? "" : v))));
      if (d.type === "indicator") body.append(this.field("Indicator", this.seg([["1", "Left"], ["2", "Right"], ["3", "Hazard"], ["0", "Off"]], String(d.value ?? 3), (v) => this.change(w, (x) => { x.do.value = Number(v); }))));
      if (d.type === "page") body.append(this.field("Page", this.dropdown((App.layout.pages || []).map((p, i) => [String(i), p.name || String(i + 1)]), String(d.value ?? 0), (v) => this.change(w, (x) => { x.do.value = Number(v); }))));
      const known = LAMPS.some(([p]) => p === (w.lamp || ""));
      body.append(this.field("Lamp", this.dropdown([...LAMPS, ["custom", "Other value…"]], known ? (w.lamp || "") : "custom", (v) => set("lamp")(v === "custom" ? "vehicle." : v))));
      if (!known) body.append(this.field("Value", this.input(w.lamp, set("lamp"))));
      if (w.lamp) body.append(this.field("Lamp colour", this.swatches(w.lampColor, set("lampColor"))));
    }
    if (w.kind === "display") {
      body.append(this.field("Shows", this.dropdown(Object.entries(SHOWS).map(([k, v]) => [k, v.cap || "Other value…"]), w.show || "custom", set("show"))));
      if (w.show === "custom" || !SHOWS[w.show]) body.append(this.field("Value", this.input(w.path, set("path"))));
      body.append(this.field("Unit", this.input(w.unit, (v) => this.change(w, (x) => { x.unit = v; }))));
    }
    if (w.kind === "gauge") {
      body.append(this.field("Highest value", this.input(w.max || 80, (v) => set("max")(Number(v) || 80), "number")));
      body.append(this.field("Unit", this.input(w.unit, (v) => this.change(w, (x) => { x.unit = v; }))));
      body.append(this.field("Value", this.input(w.path || "vehicle.motion.speedKmh", set("path"))));
    }
    if (w.kind === "pedals") {
      const look = w.look === "one" || w.look === "oneDown" ? w.look : "two";
      body.append(this.field("Pedals", this.seg([["two", "Two pedals"], ["one", "One pedal: up to accelerate, down to brake"], ["oneDown", "One pedal: down to accelerate, up to brake"]], look, set("look"))));
      if (look === "two") body.append(this.field("Push the pedals", this.seg([["down", "Downwards"], ["up", "Upwards"]], w.push === "up" ? "up" : "down", set("push"))));
    }
    if (w.kind === "wheel") body.append(this.field("Steering lock", this.input(w.max || 360, (v) => set("max")(Math.max(45, Math.min(1440, Number(v) || 360))), "number")));
    if (w.kind === "doors" || w.kind === "vdo") body.append(this.field("Look", this.seg([["line", "Line drawing"], ["lcd", "Green display"]], w.look || "line", set("look"))));
    if (w.kind === "telltales") {
      const list = R.el("div", "list");
      for (const t of TELLTALES) {
        const on = !w.lamps || w.lamps.includes(t.id);
        const b = App.listButton(R.T(t.label), on ? "●" : "○", () => this.change(w, (x) => {
          const now = x.lamps || TELLTALES.map((y) => y.id);
          x.lamps = on ? now.filter((id) => id !== t.id) : [...now, t.id];
        }), on);
        b.prepend(picto(t.picto));
        list.append(b);
      }
      body.append(this.field("Lamps shown", list));
    }
    if (w.kind === "sign") body.append(this.field("Shows", this.seg([["real", "The bus's own display"], ["text", "Plain text"]], w.real === false ? "text" : "real", (v) => set("real")(v === "text" ? false : ""))));
    if (w.kind === "sign" || w.kind === "gauge") body.append(this.field("Colour", this.swatches(w.color, set("color"))));
    App.sheet(R.T("Properties"), body);
  },

  iconSheet(w) {
    const body = R.el("div");
    const search = R.el("input", "search");
    search.type = "search";
    search.placeholder = R.T("Search");
    const grid = R.el("div", "icons");
    const symbols = R.el("div", "icons");
    for (const n of [...Object.keys(PICTOS), ...Object.keys(GLYPHS)]) {
      const b = R.el("button", n === w.picto ? "on" : null);
      b.title = n;
      b.append(picto(n));
      b.onclick = () => this.change(w, (x) => { x.picto = n; delete x.icon; });
      symbols.append(b);
    }
    body.append(R.el("h3", null, R.T("Bus symbols")), symbols, R.el("h3", null, R.T("Other icons")), search, grid);
    fetch("/icons.json").then((r) => r.json()).then((names) => {
      const fill = () => {
        const q = search.value.trim().toLowerCase();
        grid.textContent = "";
        for (const n of names) {
          if (q && !n.includes(q)) continue;
          const b = R.el("button", n === w.icon ? "on" : null);
          b.title = n;
          b.append(R.icon(n));
          b.onclick = () => this.change(w, (x) => { x.icon = n; delete x.picto; });
          grid.append(b);
        }
      };
      search.oninput = fill;
      fill();
    });
    App.sheet(R.T("Icon"), body);
  },

  actionSheet(w) {
    const game = w.do.type === "game";
    const list = game ? GAME_ACTIONS : [...new Set([...(App.vehicle.actions || []), ...PRESETS.filter((p) => p.do.type === "act").map((p) => p.do.name)])].sort();
    App.sheet(R.T(game ? "Game action" : "Key or trigger of the bus"), App.filtered(list, (n) => App.listButton(n, "", () => this.change(w, (x) => { x.do.name = n; }), n === w.do.name)));
  },

  pagesSheet() {
    const body = R.el("div");
    const pages = App.layout.pages;
    const p = this.page();
    const o = App.orient();
    body.append(this.field("Name", this.input(R.T(p.name || ""), (v) => { this.snapshot(); p.name = v; App.render(); this.pagesSheet(); })));
    const grid = R.el("div", "row2");
    const num = (i, min, max) => this.input(p.grid[o][i], (v) => { this.snapshot(); p.grid[o][i] = Math.max(min, Math.min(max, Math.round(Number(v)) || min)); App.render(); this.pagesSheet(); }, "number");
    grid.append(this.field("Columns", num(0, 1, 16)), this.field("Rows", num(1, 1, 16)));
    body.append(grid);
    body.append(R.el("p", null, R.T(o === "l" ? "The grid of the landscape layout: turn the device to arrange the portrait one." : "The grid of the portrait layout: turn the device to arrange the landscape one.")));
    const row = R.el("div", "actions-row");
    row.append(this.button("add", "New page", () => {
      this.snapshot();
      pages.push({ name: R.T("Page") + " " + (pages.length + 1), widgets: [], grid: { l: [8, 5], p: [4, 8] } });
      App.page = pages.length - 1;
      App.render();
      this.pagesSheet();
    }));
    row.append(this.button("chevron_left", "", () => this.shift(-1)));
    row.append(this.button("chevron_right", "", () => this.shift(1)));
    if (pages.length > 1) row.append(this.button("delete", "Delete the page", () => {
      this.snapshot();
      pages.splice(App.page, 1);
      App.page = Math.max(0, App.page - 1);
      App.render();
      this.pagesSheet();
    }, "danger"));
    body.append(row);
    App.sheet(R.T("Pages"), body);
  },

  shift(by) {
    const pages = App.layout.pages;
    const to = App.page + by;
    if (to < 0 || to >= pages.length) return;
    this.snapshot();
    [pages[App.page], pages[to]] = [pages[to], pages[App.page]];
    App.page = to;
    App.render();
  },

  lookSheet() {
    const l = App.layout;
    const body = R.el("div");
    body.append(this.field("Theme", this.seg(THEMES, l.theme || "night", (v) => { this.snapshot(); l.theme = v; App.render(); this.lookSheet(); })));
    body.append(this.field("Accent colour", this.swatches(l.accent, (v) => { this.snapshot(); l.accent = v || "#f5a623"; App.render(); this.lookSheet(); })));
    body.append(this.field("Buttons", this.seg([["drawn", "Drawn buttons"], ["bus", "The bus's own buttons"]], l.skins === true ? "bus" : "drawn", (v) => { this.snapshot(); l.skins = v === "bus"; App.render(); this.lookSheet(); })));
    App.sheet(R.T("Look"), body);
  },

  saveSheet() {
    const body = R.el("div");
    const name = (App.vehicle && App.vehicle.name) || "";
    body.append(R.el("p", null, R.T("Saved on the computer that runs the game, for every device.")));
    const list = R.el("div", "list");
    list.append(App.listButton(R.T("For this vehicle"), name, () => this.save("vehicle")));
    list.append(App.listButton(R.T("For every vehicle"), R.T("Vehicles with a layout of their own keep it"), () => this.save("all")));
    body.append(list);
    const row = R.el("div", "actions-row");
    const saved = (App.vehicle && App.vehicle.saved) || {};
    if (saved.vehicle) row.append(this.button("restart_alt", "Forget this vehicle's layout", () => this.reset("vehicle"), "danger"));
    if (saved.all) row.append(this.button("restart_alt", "Forget the layout for every vehicle", () => this.reset("all"), "danger"));
    row.append(this.button("refresh", "Start again from the standard layout", () => {
      this.snapshot();
      App.layout = defaultLayout(App.state);
      App.closeSheet();
      App.render();
    }));
    row.append(this.button("download", "Export", () => this.exportSheet()));
    row.append(this.button("upload", "Import", () => this.importSheet()));
    body.append(row);
    App.sheet(R.T("Save"), body);
  },

  save(scope) {
    if (!App.send({ t: "layout.save", scope, layout: App.layout })) {
      App.toast(R.T("Not saved: the game cannot be reached"));
      return;
    }
    App.saving = scope;
    App.closeSheet();
  },

  saved(ok) {
    if (App.saving === null) return;
    App.saving = null;
    if (!ok) {
      App.toast(R.T("The game refused this layout"));
      return;
    }
    this.backup = null;
    if (this.active) this.stop();
    App.toast(R.T("Layout saved"));
  },

  reset(scope) {
    App.send({ t: "layout.reset", scope });
    App.layout = null;
    App.closeSheet();
    this.stop();
    App.toast(R.T("Layout forgotten"));
  },

  exportSheet() {
    const t = R.el("textarea");
    t.readOnly = true;
    t.value = JSON.stringify(App.layout, null, 1);
    const body = R.el("div");
    body.append(R.el("p", null, R.T("Copy this text to keep the layout or share it.")), t);
    App.sheet(R.T("Export"), body);
    setTimeout(() => { t.focus(); t.select(); }, 50);
  },

  importSheet() {
    const t = R.el("textarea");
    const body = R.el("div");
    const err = R.el("p", "error");
    const row = R.el("div", "actions-row");
    row.append(this.button("check", "Import", () => {
      try {
        const l = normalize(JSON.parse(t.value));
        if (!l || !l.pages.length) throw new Error();
        this.snapshot();
        App.layout = l;
        App.page = 0;
        App.closeSheet();
        App.render();
      } catch (_) {
        err.textContent = R.T("This is not a layout");
      }
    }, "primary"));
    body.append(R.el("p", null, R.T("Paste a layout exported from another device.")), t, err, row);
    App.sheet(R.T("Import"), body);
  },
};
