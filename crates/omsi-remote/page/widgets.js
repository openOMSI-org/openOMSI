"use strict";

const R = {
  texts: {},
  T(s) { return Object.prototype.hasOwnProperty.call(R.texts, s) ? R.texts[s] : s; },
  get(o, path) {
    if (!path) return undefined;
    let v = o;
    for (const k of String(path).split(".")) {
      if (v == null) return undefined;
      v = v[k];
    }
    return v;
  },
  truthy(v) { return v === true || (typeof v === "number" && v > 0.5) || (typeof v === "string" && v.trim() !== ""); },
  el(tag, cls, html) {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    if (html != null) e.textContent = html;
    return e;
  },
  icon(name) {
    const i = R.el("span", "ico");
    if (name) i.style.setProperty("--icon", `url("/icons/${encodeURIComponent(name)}.svg")`);
    return i;
  },
  uid() { return Math.random().toString(36).slice(2, 9); },
  doorways(s) {
    const leaves = R.get(s, "vehicle.doors.list") || [];
    const exits = R.get(s, "vehicle.passengers.exits") || [];
    const open = (n) => Number(leaves[n] && leaves[n].open) || 0;
    const ways = new Set(((App.vehicle && App.vehicle.aliases) || []).map(([a]) => a).filter((a) => /^door_\d$/.test(a))).size;
    let groups = ((App.vehicle && App.vehicle.doors) || []).filter((g) => Array.isArray(g) && g.length).slice(0, 6);
    if (!groups.length) {
      const n = Math.max(1, Math.min(6, ways || exits.length || Math.ceil(leaves.length / 2) || R.get(s, "vehicle.doors.count") || 2));
      const per = Math.min(2, Math.max(1, Math.round(leaves.length / n)));
      groups = Array.from({ length: n }, (_, i) => Array.from({ length: per }, (_, k) => i * per + k));
    }
    return groups.map((g, i) => {
      const o = Math.max(0, Math.min(1, Math.max(0, ...g.map(open))));
      return { o, open: o > 0.05, moving: o > 0.05 && o < 0.95, request: R.truthy(exits[i] && exits[i].requested) };
    });
  },
  skin(w) {
    const v = App.vehicle;
    if (!v || !v.skins || !v.skins.length || !(App.layout && App.layout.skins === true) || w.skin === false) return null;
    const d = w.do || {};
    const name = d.type === "act" ? d.name : d.type === "indicator" && d.value === 3 ? "blinker_warn_toggle" : null;
    if (!name) return null;
    const have = new Set(v.skins);
    const wanted = [name, ...(v.aliases || []).filter(([a]) => a === name).map(([, b]) => b)];
    const found = wanted.find((n) => have.has(n));
    if (!found) return null;
    const url = (n) => `/vt/${encodeURIComponent(n)}.png?k=${encodeURIComponent(App.token || "")}`;
    return { day: url(found), lit: have.has(found + ".lit") ? url(found + ".lit") : null };
  },
  buzz() { if (navigator.vibrate) navigator.vibrate(12); },
};

const SHOWS = {
  speed: { cap: "Speed", path: "vehicle.motion.speedKmh", unit: "km/h", dp: 0 },
  time: { cap: "Time", path: "env.time.asString", fmt: (v) => String(v || "").slice(0, 5) || "–" },
  line: { cap: "Line", path: "vehicle.route.line" },
  destination: { cap: "Destination", path: "vehicle.route.destination", text: true },
  nextStop: { cap: "Next stop", path: "vehicle.route.current.name", text: true },
  delay: { cap: "Delay", path: "vehicle.route.delaySec", fmt: (v) => v == null ? "–" : `${v > 0 ? "+" : v < 0 ? "−" : ""}${Math.floor(Math.abs(v) / 60)}:${String(Math.round(Math.abs(v) % 60)).padStart(2, "0")}` },
  rpm: { cap: "RPM", path: "vehicle.engine.rpm", dp: 0 },
  gear: { cap: "Gear", path: "vehicle.engine.gear", dp: 0 },
  tank: { cap: "Fuel", path: "vehicle.engine.tankContent", unit: "l", dp: 0 },
  passengers: { cap: "Passengers", path: "vehicle.passengers.onboard", dp: 0 },
  temperature: { cap: "Cabin", path: "vehicle.cabin.temperature", unit: "°C", dp: 1 },
  odometer: { cap: "Odometer", path: "vehicle.motion.odometerKm", unit: "km", dp: 1 },
  outside: { cap: "Outside", path: "env.weather.temperature", unit: "°C", dp: 0 },
  date: { cap: "Date", path: "env.date.asString" },
  custom: { cap: "", path: "" },
};

const LAMPS = [
  ["", "None"],
  ["vehicle.doors.anyOpen", "A door is open"],
  ["doorways.0.open", "Door 1 open"],
  ["doorways.1.open", "Door 2 open"],
  ["doorways.2.open", "Door 3 open"],
  ["doorways.3.open", "Door 4 open"],
  ["doorways.4.open", "Door 5 open"],
  ["doorways.5.open", "Door 6 open"],
  ["vehicle.passengers.exits.0.requested", "Stop request"],
  ["vehicle.brakes.parking", "Parking brake"],
  ["vehicle.brakes.stop", "Stop brake"],
  ["vehicle.brakes.kneeling", "Kneeling"],
  ["vehicle.engine.running", "Engine running"],
  ["vehicle.electrics.on", "Electrics on"],
  ["vehicle.battery.on", "Battery"],
  ["vehicle.lights.headlights", "Headlights"],
  ["vehicle.lights.fog", "Fog light"],
  ["vehicle.lights.hazard", "Hazard lights"],
  ["vehicle.lights.indicatorLeft", "Left indicator"],
  ["vehicle.lights.indicatorRight", "Right indicator"],
  ["vehicle.lights.interior", "Saloon lights"],
  ["vehicle.wipers.running", "Wipers"],
];

const TELLTALES = [
  { id: "parking", picto: "parking", color: "#ff3b30", label: "Parking brake", on: (s) => R.truthy(R.get(s, "vehicle.brakes.parking")) },
  { id: "stop", picto: "stopBrake", color: "#ffb300", label: "Stop brake", on: (s) => R.truthy(R.get(s, "vehicle.brakes.stop")) },
  { id: "doors", picto: "door", color: "#ffb300", label: "A door is open", on: (s) => R.truthy(R.get(s, "vehicle.doors.anyOpen")) },
  { id: "request", picto: "bell", color: "#ff3b30", label: "Stop request", on: (s) => (s.doorways || []).some((d) => d.request) },
  { id: "kneeling", picto: "kneel", color: "#ffb300", label: "Kneeling", on: (s) => R.truthy(R.get(s, "vehicle.brakes.kneeling")) },
  { id: "left", picto: "turnLeft", color: "#2fb344", label: "Left indicator", on: (s) => R.truthy(R.get(s, "vehicle.lights.indicatorLeft")) },
  { id: "hazard", picto: "hazard", color: "#ff3b30", label: "Hazard lights", on: (s) => R.truthy(R.get(s, "vehicle.lights.hazard")) },
  { id: "right", picto: "turnRight", color: "#2fb344", label: "Right indicator", on: (s) => R.truthy(R.get(s, "vehicle.lights.indicatorRight")) },
  { id: "lights", picto: "lowBeam", color: "#2fb344", label: "Headlights", on: (s) => R.truthy(R.get(s, "vehicle.lights.headlights")) },
  { id: "fog", picto: "rearFog", color: "#ffb300", label: "Fog light", on: (s) => R.truthy(R.get(s, "vehicle.lights.fog")) },
  { id: "charge", picto: "battery", color: "#ff3b30", label: "Engine off", on: (s) => R.truthy(R.get(s, "vehicle.electrics.on")) && !R.truthy(R.get(s, "vehicle.engine.running")) },
];

const MESSAGES = [
  { picto: "parking", text: "Parking brake applied", level: 2, on: (s) => R.truthy(R.get(s, "vehicle.brakes.parking")) && Math.abs(Number(R.get(s, "vehicle.motion.speedKmh")) || 0) > 2 },
  { picto: "stopBrake", text: "Stop brake applied", level: 1, on: (s) => R.truthy(R.get(s, "vehicle.brakes.stop")) },
  { picto: "door", text: "Doors open", level: 1, on: (s) => (s.doorways || []).some((d) => d.open) },
  { picto: "bell", text: "Stop requested", level: 0, on: (s) => (s.doorways || []).some((d) => d.request) },
  { picto: "kneel", text: "Bus lowered", level: 0, on: (s) => R.truthy(R.get(s, "vehicle.brakes.kneeling")) },
  { picto: "battery", text: "Engine off", level: 1, on: (s) => R.truthy(R.get(s, "vehicle.electrics.on")) && !R.truthy(R.get(s, "vehicle.engine.running")) },
  { picto: "fuel", text: "Fuel low", level: 2, on: (s) => { const t = R.get(s, "vehicle.engine.tankContent"); return t != null && t < 25; } },
  { picto: "hazard", text: "Hazard lights on", level: 1, on: (s) => R.truthy(R.get(s, "vehicle.lights.hazard")) },
];

const PRESETS = [
  { id: "door1", label: "Door 1", icon: "door_front", do: { type: "act", name: "door_1" }, lamp: "doorways.0.open", picto: "door", style: "vdv", lampColor: "#3ddc5c" },
  { id: "door2", label: "Door 2", icon: "door_sliding", do: { type: "act", name: "door_2" }, lamp: "doorways.1.open", picto: "door", style: "vdv", lampColor: "#3ddc5c" },
  { id: "door3", label: "Door 3", icon: "door_sliding", do: { type: "act", name: "door_3" }, lamp: "doorways.2.open", picto: "door", style: "vdv", lampColor: "#3ddc5c" },
  { id: "door4", label: "Door 4", icon: "door_sliding", do: { type: "act", name: "door_4" }, lamp: "doorways.3.open", picto: "door", style: "vdv", lampColor: "#3ddc5c" },
  { id: "door5", label: "Door 5", icon: "door_sliding", do: { type: "act", name: "door_5" }, lamp: "doorways.4.open", picto: "door", style: "vdv", lampColor: "#3ddc5c" },
  { id: "door6", label: "Door 6", icon: "door_sliding", do: { type: "act", name: "door_6" }, lamp: "doorways.5.open", picto: "door", style: "vdv", lampColor: "#3ddc5c" },
  { id: "doorsAll", label: "All doors", icon: "unfold_more", do: { type: "act", name: "doors_all" }, lamp: "vehicle.doors.anyOpen", picto: "doors", style: "vdv", lampColor: "#3ddc5c" },
  { id: "stopBrake", label: "Stop brake", icon: "back_hand", do: { type: "act", name: "bus_dooraft" }, lamp: "vehicle.brakes.stop", picto: "stopBrake", style: "vdv", lampColor: "#ffc400" },
  { id: "parkingBrake", label: "Parking brake", icon: "local_parking", do: { type: "act", name: "parking_brake_toggle" }, lamp: "vehicle.brakes.parking", picto: "parking", style: "vdv", lampColor: "#ff3b30" },
  { id: "hazard", label: "Hazard lights", icon: "warning", do: { type: "indicator", value: 3 }, lamp: "vehicle.lights.hazard", picto: "hazard", style: "vdv", lampColor: "#ff3b30", face: "red" },
  { id: "horn", label: "Horn", icon: "campaign", do: { type: "act", name: "horn" }, picto: "horn", style: "vdv" },
  { id: "headlights", label: "Headlights", icon: "light_mode", do: { type: "act", name: "kw_scheinwerfer_toggle" }, lamp: "vehicle.lights.headlights", picto: "lowBeam", style: "vdv", lampColor: "#3ddc5c" },
  { id: "highBeam", label: "High beam", icon: "highlight", do: { type: "act", name: "kw_fernlicht_toggle" }, picto: "highBeam", style: "vdv", lampColor: "#3d8bff" },
  { id: "fog", label: "Fog light", icon: "foggy", do: { type: "act", name: "taster_nebelschluss" }, lamp: "vehicle.lights.fog", picto: "rearFog", style: "vdv", lampColor: "#ffb000" },
  { id: "saloon", label: "Saloon lights", icon: "light", do: { type: "saloon" }, lamp: "vehicle.lights.interior", picto: "saloon", style: "vdv", lampColor: "#3ddc5c" },
  { id: "driverLight", label: "Driver light", icon: "flashlight_on", do: { type: "act", name: "cp_fahrerlicht_toggle" }, picto: "driverLight", style: "vdv", lampColor: "#3ddc5c" },
  { id: "wipers", label: "Wipers", icon: "water_drop", do: { type: "act", name: "kw_wipermode_up" }, lamp: "vehicle.wipers.running", picto: "wiper", style: "vdv", lampColor: "#3ddc5c" },
  { id: "battery", label: "Battery", icon: "bolt", do: { type: "act", name: "cp_batterietrennschalter_toggle" }, lamp: "vehicle.electrics.on", picto: "battery", style: "vdv", lampColor: "#3ddc5c" },
  { id: "engine", label: "Engine start", icon: "power_settings_new", do: { type: "act", name: "kw_m_enginestart" }, lamp: "vehicle.engine.running", picto: "engine", style: "vdv", lampColor: "#3ddc5c" },
  { id: "startup", label: "Start the bus", icon: "key", do: { type: "startup" }, lamp: "vehicle.engine.running", picto: "key", style: "vdv", lampColor: "#3ddc5c" },
  { id: "ticket", label: "Ticket", icon: "confirmation_number", do: { type: "act", name: "ticket_give" }, picto: "ticket", style: "vdv" },
  { id: "pram", label: "Pram button", icon: "airline_seat_recline_normal", do: { type: "act", name: "cp_schalter_kinderwagen" }, picto: "pram", style: "vdv", lampColor: "#3d8bff" },
  { id: "microphone", label: "Microphone", icon: "volume_up", do: { type: "act", name: "cp_microphone" }, picto: "microphone", style: "vdv", lampColor: "#ffb000" },
  { id: "gearD", label: "D", icon: "", do: { type: "act", name: "automatic_D" }, picto: "D", style: "vdv", lampColor: "#3ddc5c" },
  { id: "gearN", label: "N", icon: "", do: { type: "act", name: "automatic_N" }, picto: "N", style: "vdv", lampColor: "#ff3b30", pictoColor: "#ff3b30" },
  { id: "gearR", label: "R", icon: "", do: { type: "act", name: "automatic_R" }, picto: "R", style: "vdv", lampColor: "#3ddc5c" },
  { id: "route", label: "Route", icon: "route", do: { type: "sign" } },
  { id: "clearLine", label: "Clear line", icon: "close", do: { type: "clearline" } },
  { id: "viewDriver", label: "Driver's view", icon: "person", do: { type: "game", name: "view_set_driver" } },
  { id: "viewPax", label: "Passenger's view", icon: "groups", do: { type: "game", name: "view_set_passenger" } },
  { id: "viewOutside", label: "Outside view", icon: "videocam", do: { type: "game", name: "view_set_outside" } },
  { id: "viewMap", label: "Map", icon: "map", do: { type: "game", name: "view_set_map" } },
  { id: "viewReset", label: "Reset view", icon: "restart_alt", do: { type: "game", name: "view_reset_direction" } },
  { id: "camPrev", label: "Camera −", icon: "chevron_left", do: { type: "game", name: "view_interiorcam_minus" } },
  { id: "camNext", label: "Camera +", icon: "chevron_right", do: { type: "game", name: "view_interiorcam_plus" } },
  { id: "pause", label: "Pause", icon: "pause", do: { type: "game", name: "sim_pause" } },
  { id: "screenshot", label: "Screenshot", icon: "photo_camera", do: { type: "game", name: "screenshot" } },
];

const KINDS = {
  button: {
    name: "Button",
    size: [1, 1],
    make(w) {
      const b = R.el("button", "btn");
      const skin = R.skin(w);
      const pic = !skin && w.picto ? picto(w.picto) : null;
      if (w.face) b.classList.add("face-" + w.face);
      if (pic) {
        if (w.pictoColor) pic.style.color = w.pictoColor;
        b.append(pic);
      }
      else if (skin) {
        b.classList.add("skinned");
        const img = R.el("img", "face");
        img.src = skin.day;
        img.alt = "";
        img.draggable = false;
        if (skin.lit) { img.dataset.day = skin.day; img.dataset.lit = skin.lit; }
        b.append(img);
      } else if (w.icon) b.append(R.icon(w.icon));
      const label = R.T(w.label || "");
      if (label && !(pic && GLYPHS[w.picto] === label)) b.append(R.el("span", "txt", label));
      if (w.color) { b.classList.add("custom"); b.style.setProperty("--wc", w.color); }
      if (w.lampColor) b.style.setProperty("--wl", w.lampColor);
      if (w.lamp) b.classList.add("has-lamp");
      bindPress(b, w);
      return b;
    },
    update(node, w, s) {
      if (!w.lamp) return;
      const on = R.truthy(R.get(s, w.lamp));
      node.classList.toggle("lit", on);
      const img = node.querySelector("img.face[data-lit]");
      if (img) {
        const want = on ? img.dataset.lit : img.dataset.day;
        if (img.getAttribute("src") !== want) img.src = want;
      }
    },
  },
  display: {
    name: "Display",
    size: [2, 1],
    make(w) {
      const show = (Object.prototype.hasOwnProperty.call(SHOWS, w.show) && SHOWS[w.show]) || SHOWS.custom;
      const d = R.el("div", "disp" + (show.text ? " text" : ""));
      d.append(R.el("div", "cap", w.label ? R.T(w.label) : R.T(show.cap)), R.el("div", "val", "–"));
      return d;
    },
    update(node, w, s) {
      const show = (Object.prototype.hasOwnProperty.call(SHOWS, w.show) && SHOWS[w.show]) || SHOWS.custom;
      const v = R.get(s, w.show === "custom" || !SHOWS[w.show] ? w.path : show.path);
      let text;
      if (show.fmt) text = show.fmt(v);
      else if (v == null || v === "") text = "–";
      else if (typeof v === "number") text = v.toFixed(w.dp ?? show.dp ?? 1);
      else if (typeof v === "boolean") text = v ? "●" : "○";
      else text = String(v);
      const val = node.lastChild;
      const unit = w.unit ?? show.unit;
      if (val.dataset.v === text + unit) return;
      val.dataset.v = text + unit;
      val.style.setProperty("--n", Math.max(4, text.length + (unit ? unit.length * 0.45 : 0)));
      val.textContent = text;
      if (unit && text !== "–") val.append(R.el("small", null, unit));
    },
  },
  sign: {
    name: "Destination sign",
    size: [4, 1],
    make(w) {
      const d = R.el("div", "sign");
      if (w.color) d.style.setProperty("--wc", w.color);
      const img = R.el("img", "real");
      img.alt = "";
      img.hidden = true;
      img.addEventListener("error", () => {
        img.dataset.bad = img.getAttribute("src");
        d.classList.remove("has-real");
        img.hidden = true;
      });
      d.append(R.el("span", "line"), R.el("span", "dest"), img);
      d.addEventListener("click", () => App.routeSheet());
      return d;
    },
    update(node, w, s) {
      const img = node.querySelector("img.real");
      const real = w.real !== false && App.sign && img.dataset.bad !== App.sign;
      node.classList.toggle("has-real", !!real);
      img.hidden = !real;
      if (real && img.getAttribute("src") !== App.sign) img.src = App.sign;
      const line = R.get(s, "vehicle.route.line") || "";
      const dest = R.get(s, "vehicle.route.destination") || "";
      node.querySelector(".line").textContent = line;
      const d = node.querySelector(".dest");
      d.textContent = dest || R.T("No destination");
      d.classList.toggle("empty", !dest);
    },
  },
  gauge: {
    name: "Speedometer",
    size: [2, 2],
    make(w) {
      const d = R.el("div", "gauge");
      const max = w.max || 80;
      d.innerHTML = `<svg viewBox="0 0 120 100"><path class="arc-bg" d="M15 85 A50 50 0 1 1 105 85" fill="none" stroke-width="10" stroke-linecap="round"/><path class="arc" d="M15 85 A50 50 0 1 1 105 85" fill="none" stroke-width="10" stroke-linecap="round" pathLength="100" stroke-dasharray="0 200" visibility="hidden"/><text class="num" x="60" y="66" text-anchor="middle" font-size="30">0</text><text class="unit" x="60" y="88" text-anchor="middle" font-size="11"></text><text class="unit cap" x="60" y="38" text-anchor="middle" font-size="8"></text></svg>`;
      d.querySelector(".unit").textContent = w.unit ?? "km/h";
      d.querySelector(".cap").textContent = w.label ? R.T(w.label) : "";
      d.dataset.max = max;
      if (w.color) d.style.setProperty("--wc", w.color);
      return d;
    },
    update(node, w, s) {
      const v = Math.abs(Number(R.get(s, w.path || "vehicle.motion.speedKmh")) || 0);
      const max = Number(node.dataset.max) || 80;
      const arc = node.querySelector(".arc");
      const f = Math.min(1, v / max) * 100;
      arc.setAttribute("stroke-dasharray", `${f} 200`);
      arc.setAttribute("visibility", f > 0.5 ? "visible" : "hidden");
      node.querySelector(".num").textContent = Math.round(v);
    },
  },
  blinkers: {
    name: "Indicators",
    size: [3, 1],
    make(w) {
      const d = R.el("div", "blinkers");
      for (const [icon, value, label] of [["turn_left", 1, "Left"], ["warning", 3, "Hazard"], ["turn_right", 2, "Right"]]) {
        const b = R.el("button", "btn has-lamp");
        const skin = value === 3 && R.skin({ do: { type: "indicator", value } });
        b.classList.add("vdv");
        if (value === 3) b.classList.add("face-red");
        if (!skin) b.append(picto(["", "turnLeft", "turnRight", "hazard"][value]));
        else {
          const img = R.el("img", "face");
          img.src = skin.day;
          img.alt = "";
          b.classList.add("skinned");
          b.append(img);
        }
        b.dataset.v = value;
        if (value === 3) b.style.setProperty("--wl", "#ff3b30");
        bindPress(b, { do: { type: "indicator", value } }, R.T(label));
        d.append(b);
      }
      return d;
    },
    update(node, w, s) {
      const [l, h, r] = node.children;
      const hazard = R.truthy(R.get(s, "vehicle.lights.hazard"));
      l.classList.toggle("lit-blink", !hazard && R.truthy(R.get(s, "vehicle.lights.indicatorLeft")));
      r.classList.toggle("lit-blink", !hazard && R.truthy(R.get(s, "vehicle.lights.indicatorRight")));
      h.classList.toggle("lit", hazard);
    },
  },
  doors: {
    name: "Doors",
    size: [4, 1],
    make() { return R.el("div", "doors"); },
    update(node, w, s) { citaroUpdate(node, w, s, false); },
  },
  vdo: {
    name: "Bus display",
    size: [3, 2],
    make() { return R.el("div", "doors full"); },
    update(node, w, s) { citaroUpdate(node, w, s, true); },
  },
  stops: {
    name: "Stops",
    size: [3, 3],
    make() { return R.el("div", "stops"); },
    update(node, w, s) {
      const stops = R.get(s, "vehicle.route.stops") || [];
      const key = JSON.stringify(stops.map((x) => [x.name, x.departure, x.served, x.current]));
      if (node.dataset.k === key) return;
      node.dataset.k = key;
      node.textContent = "";
      if (!stops.length) node.append(R.el("div", "row", R.T("No timetable")));
      for (const st of stops) {
        const row = R.el("div", "row" + (st.served ? " served" : "") + (st.current ? " cur" : ""));
        row.append(R.el("span", "t", st.departure || st.arrival || ""), R.el("span", "n", st.name || ""));
        row.addEventListener("click", () => {
          if (!document.body.classList.contains("editing")) App.confirmNextStop(st);
        });
        node.append(row);
      }
      const cur = node.querySelector(".cur");
      if (cur) node.scrollTop = cur.offsetTop - node.clientHeight / 2 + cur.offsetHeight / 2;
    },
  },
  telltales: {
    name: "Warning lamps",
    size: [8, 1],
    make(w) {
      const d = R.el("div", "telltales");
      for (const t of TELLTALES) {
        if (w.lamps && !w.lamps.includes(t.id)) continue;
        const i = R.el("span", "tt");
        i.append(picto(t.picto));
        i.title = R.T(t.label);
        i.dataset.id = t.id;
        i.style.setProperty("--tc", t.color);
        d.append(i);
      }
      return d;
    },
    update(node, w, s) {
      for (const i of node.children) {
        const t = TELLTALES.find((x) => x.id === i.dataset.id);
        i.classList.toggle("on", !!t && t.on(s));
      }
    },
  },
  wheel: {
    name: "Steering wheel",
    size: [4, 4],
    make(w) { return Drive.wheel(w); },
    update() {},
  },
  pedals: {
    name: "Pedals",
    size: [2, 3],
    make(w) { return Drive.pedals(w); },
    update() {},
  },
  map: {
    name: "Map",
    size: [8, 5],
    make() {
      const v = R.el("div", "mapview");
      v.innerHTML = '<canvas></canvas><div class="turn" hidden><div class="ic"></div><div><div class="dist"></div><div class="street"></div></div></div><div class="here" hidden></div><div class="spd"><span class="v">0</span><span class="u">km/h</span><span class="lim" hidden></span></div><div class="stopbar" hidden><span class="pin"></span><span class="name"></span><span class="dist"></span><span class="time"></span></div><div class="zoom"><button class="in">+</button><button class="out">−</button></div><div class="nomap"></div>';
      v.querySelector(".zoom .in").setAttribute("aria-label", R.T("Zoom in"));
      v.querySelector(".zoom .out").setAttribute("aria-label", R.T("Zoom out"));
      MapView.attach(v);
      v.querySelector(".nomap").textContent = R.T("The map appears once the game has read the map's roads");
      v.querySelector(".pin").append(picto("bell"));
      MapView.views.add(v);
      MapView.wake();
      return v;
    },
    update(node) { MapView.overlay(node); },
  },
  messages: {
    name: "Messages",
    size: [4, 1],
    make() {
      const d = R.el("div", "messages");
      d.append(R.el("div", "msg-icon"), R.el("div", "msg-text"), R.el("div", "msg-count"));
      d.dataset.i = "0";
      return d;
    },
    update(node, w, s) {
      const active = MESSAGES.filter((m) => m.on(s)).sort((a, b) => b.level - a.level);
      const now = Math.floor(Date.now() / 3000);
      const m = active.length ? active[now % active.length] : null;
      const key = m ? m.text + active.length : "";
      if (node.dataset.k === key) return;
      node.dataset.k = key;
      const [icon, text, count] = node.children;
      icon.textContent = "";
      node.dataset.level = m ? String(m.level) : "";
      if (m) icon.append(picto(m.picto));
      else icon.append(picto("info"));
      text.textContent = m ? R.T(m.text) : R.T("All good");
      count.textContent = active.length > 1 ? `${(now % active.length) + 1}/${active.length}` : "";
    },
  },
  label: {
    name: "Label",
    size: [2, 1],
    make(w) { return R.el("div", "w-label", R.T(w.label || "")); },
  },
};

function citaroBus(n, full) {
  const NS = "http://www.w3.org/2000/svg";
  const el = (tag, attrs, parent) => {
    const e = document.createElementNS(NS, tag);
    for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
    if (parent) parent.append(e);
    return e;
  };
  const svg = el("svg", { viewBox: full ? "0 0 501 405" : "106 26 284 120", class: "cdisp", preserveAspectRatio: "xMidYMid meet" });
  const hatch = el("pattern", { id: "h" + R.uid(), width: 6, height: 6, patternUnits: "userSpaceOnUse" }, el("defs", {}, svg));
  el("rect", { class: "hb", width: 6, height: 6 }, hatch);
  el("rect", { class: "hf", width: 3.6, height: 6 }, hatch);
  svg.style.setProperty("--hatch", `url(#${hatch.id})`);
  const body = el("g", { class: "body" }, svg);
  el("path", { class: "shell", d: "M128 34H383V116Q383 126 373 126H132Q119 126 118 114L115 80Q114 58 118 45Q121 34 128 34Z" }, body);
  const step = 180 / Math.max(1, n - 1);
  const dw = Math.min(30, step - 18);
  const spread = Array.from({ length: n - 1 }, (_, i) => [190 + i * step + (step - dw) / 2, dw, 1, 126]);
  const gap = (i) => 190 + (i + 1) * step;
  const wheels = n >= 5 ? [gap(0), gap(n - 3)] : n >= 3 ? [200, 300] : [200, 328];
  const radius = n >= 5 ? 9 : 16;
  const spots = n >= 5 ? [[132, 47, 2, 110], ...spread] : n === 4 ? [[132, 47, 2, 110], [214, 40, 1, 126], [258, 40, 1, 126], [330, 40, 1, 126]] : n === 3 ? [[132, 47, 2, 110], [234, 46, 1, 126], [328, 44, 1, 126]] : n === 2 ? [[132, 47, 2, 110], [248, 48, 1, 126]] : [[132, 47, 2, 110]];
  spots.forEach(([x, w, leaves, bottom], i) => {
    const g = el("g", { class: "door" }, body);
    const lw = w / leaves;
    for (let k = 0; k < leaves; k++) el("rect", { class: "leaf", x: x + k * lw, y: 50, width: lw, height: bottom - 50 }, g);
    const stop = el("text", { class: "stop", x: x + w / 2, y: 46, "text-anchor": "middle" }, g);
    stop.textContent = "STOP";
    el("rect", { x: x - 4, y: 36, width: w + 8, height: bottom - 30, fill: "transparent" }, g);
    bindPress(g, { do: { type: "act", name: `door_${i + 1}` } });
  });
  for (const x of wheels) {
    el("circle", { class: "wheel", cx: x, cy: 126, r: radius }, svg);
    const [k0, k1] = full ? [150, 190] : [62, 100];
    el("path", { class: "kneel", d: `M${x} ${k0}V${k1} M${x - 9} ${k0 + 10}L${x} ${k0 - 1}L${x + 9} ${k0 + 10} M${x - 9} ${k1 - 10}L${x} ${k1 + 1}L${x + 9} ${k1 - 10}` }, svg);
  }
  if (full) {
    const ring = (cls, cx, cy, letter) => {
      const g = el("g", { class: `sym ${cls}` }, svg);
      el("circle", { cx, cy, r: 26 }, g);
      el("path", { d: `M${cx - 33} ${cy - 20}A38 38 0 0 0 ${cx - 33} ${cy + 20} M${cx + 33} ${cy - 20}A38 38 0 0 1 ${cx + 33} ${cy + 20}` }, g);
      const t = el("text", { x: cx, y: cy + 11, "text-anchor": "middle" }, g);
      t.textContent = letter;
    };
    ring("retarder", 59, 59, "R");
    ring("park", 447, 252, "P");
    ring("hsb", 59, 252, "H");
    const n = el("g", { class: "sym gear" }, svg);
    el("rect", { x: 309, y: 213, width: 78, height: 77 }, n);
    const t = el("text", { x: 348, y: 266, "text-anchor": "middle" }, n);
    t.textContent = "N";
    const pram = el("g", { class: "sym pram" }, svg);
    el("path", { d: "M129 252H200Q196 276 166 276Q134 276 129 252Z M129 252Q129 222 158 222V252 M200 252L206 240", "stroke-linejoin": "round" }, pram);
    el("circle", { cx: 145, cy: 284, r: 6 }, pram);
    el("circle", { cx: 186, cy: 284, r: 6 }, pram);
  }
  return svg;
}

function citaroUpdate(node, w, s, full) {
  const ways = s.doorways || [];
  const n = Math.max(1, Math.min(6, ways.length));
  if (node.dataset.n !== String(n)) {
    node.dataset.n = String(n);
    node.textContent = "";
    node.append(citaroBus(n, full));
  }
  const svg = node.firstChild;
  node.classList.toggle("lcd", w.look === "lcd");
  const flag = (cls, on) => svg.classList.toggle(cls, !!on);
  flag("parking", R.truthy(R.get(s, "vehicle.brakes.parking")));
  flag("stopbrake", R.truthy(R.get(s, "vehicle.brakes.stop")));
  flag("kneeling", R.truthy(R.get(s, "vehicle.brakes.kneeling")));
  flag("neutral", R.truthy(R.get(s, "vehicle.engine.running")) && Number(R.get(s, "vehicle.engine.gear")) === 0);
  svg.querySelectorAll(".door").forEach((g, i) => {
    const d = ways[i] || { open: false, moving: false, request: false };
    g.classList.toggle("open", d.open);
    g.classList.toggle("moving", d.moving);
    g.classList.toggle("req", d.request);
  });
}

function bindPress(node, w, title) {
  if (title) node.title = title;
  const label = title || w.label;
  if (label && !node.getAttribute("aria-label")) node.setAttribute("aria-label", label);
  let finger = null;
  const d = w.do || { type: "none" };
  const start = (e) => {
    if (document.body.classList.contains("editing")) return;
    e.preventDefault();
    if (finger !== null && App.pressed.has(node)) return;
    finger = e.pointerId;
    node.classList.add("down");
    try { node.setPointerCapture(e.pointerId); } catch (_) {}
    R.buzz();
    App.pressed.set(node, [d, w]);
    App.press(d, true, w);
  };
  const end = (e) => {
    if (finger === null || e.pointerId !== finger) return;
    finger = null;
    node.classList.remove("down");
    if (!App.pressed.delete(node)) return;
    App.press(d, false, w);
  };
  node.release = () => { finger = null; };
  node.addEventListener("pointerdown", start);
  node.addEventListener("pointerup", end);
  node.addEventListener("pointercancel", end);
  node.addEventListener("lostpointercapture", end);
  node.addEventListener("contextmenu", (e) => e.preventDefault());
}

const int = (v, lo, hi) => (Number.isFinite(Number(v)) ? Math.max(lo, Math.min(hi, Math.round(Number(v)))) : null);

function normalize(l) {
  if (!l || l.v !== 1 || !Array.isArray(l.pages)) return null;
  const out = { v: 1, theme: ["night", "cockpit", "day", "amber", "contrast"].includes(l.theme) ? l.theme : "night", accent: /^#[0-9a-f]{3,8}$/i.test(l.accent || "") ? l.accent : "#f5a623", skins: l.skins === true, pages: [] };
  for (const p of l.pages.slice(0, 12)) {
    if (!p || !Array.isArray(p.widgets)) continue;
    const grid = {};
    for (const o of ["l", "p"]) {
      const g = p.grid && p.grid[o];
      const c = Array.isArray(g) && int(g[0], 1, 16), r = Array.isArray(g) && int(g[1], 1, 16);
      if (c && r) grid[o] = [c, r];
    }
    const widgets = [];
    for (const w of p.widgets.slice(0, 96)) {
      if (!w || !Object.prototype.hasOwnProperty.call(KINDS, w.kind)) continue;
      const pos = {};
      for (const o of ["l", "p"]) {
        const q = w.pos && w.pos[o];
        if (!Array.isArray(q)) continue;
        const x = int(q[0], 0, 15), y = int(q[1], 0, 15);
        if (x === null || y === null) continue;
        const ww = int(q[2], 1, 16 - x), hh = int(q[3], 1, 16 - y);
        if (ww && hh) pos[o] = [x, y, ww, hh];
      }
      const c = { ...w, id: typeof w.id === "string" ? w.id : R.uid(), pos };
      delete c.sz;
      if (c.dp != null) c.dp = int(c.dp, 0, 6);
      for (const k of ["color", "lampColor", "pictoColor"]) if (c[k] && !/^#[0-9a-f]{3,8}$/i.test(c[k])) delete c[k];
      widgets.push(c);
    }
    out.pages.push({ name: typeof p.name === "string" ? p.name : "", grid, widgets });
  }
  return out;
}

function presetWidget(id, extra) {
  const p = JSON.parse(JSON.stringify(PRESETS.find((x) => x.id === id)));
  delete p.id;
  return { kind: "button", ...p, ...extra, id: R.uid(), pos: {} };
}

function widget(kind, extra) {
  return { id: R.uid(), kind, pos: {}, ...extra };
}

function pack(widgets, cols, orient, minRows) {
  cols = int(cols, 1, 16) || 8;
  const taken = [];
  const free = (x, y, w, h) => {
    if (x + w > cols) return false;
    for (let j = y; j < y + h; j++) for (let i = x; i < x + w; i++) if (taken[j] && taken[j][i]) return false;
    return true;
  };
  const mark = (x, y, w, h) => {
    for (let j = y; j < y + h; j++) {
      taken[j] = taken[j] || [];
      for (let i = x; i < x + w; i++) taken[j][i] = true;
    }
  };
  for (const w of widgets) {
    const p = w.pos && w.pos[orient];
    if (p) mark(p[0], p[1], Math.min(p[2], cols), p[3]);
  }
  for (const w of widgets) {
    if (w.pos && w.pos[orient]) continue;
    const size = (w.sz && w.sz[orient]) || (KINDS[w.kind] || KINDS.button).size;
    const ww = Math.min(size[0], cols), hh = size[1];
    for (let y = 0; ; y++) {
      let placed = false;
      for (let x = 0; x <= cols - ww; x++) {
        if (free(x, y, ww, hh)) {
          w.pos = w.pos || {};
          w.pos[orient] = [x, y, ww, hh];
          mark(x, y, ww, hh);
          placed = true;
          break;
        }
      }
      if (placed) break;
    }
  }
  let rows = minRows || 1;
  for (const w of widgets) rows = Math.max(rows, w.pos[orient][1] + w.pos[orient][3]);
  return rows;
}

function defaultLayout(state) {
  const doors = Math.max(1, Math.min(6, R.doorways(state).length));
  const at = (w, l, p) => Object.assign(w, { pos: { l, p } });
  const dash = [
    at(widget("map", {}), [0, 0, 6, 6], [0, 3, 4, 5]),
    at(widget("gauge", { label: "Speed" }), [6, 0, 3, 3], [0, 1, 2, 2]),
    at(widget("vdo", {}), [9, 0, 3, 3], [2, 1, 2, 2]),
    at(widget("sign", {}), [6, 3, 6, 1], [0, 0, 4, 1]),
    at(widget("display", { show: "time" }), [6, 4, 2, 1], [0, 8, 2, 1]),
    at(widget("display", { show: "nextStop" }), [8, 4, 4, 1], [2, 8, 2, 1]),
    at(widget("display", { show: "delay" }), [6, 5, 2, 1], [0, 9, 2, 1]),
    at(widget("display", { show: "tank" }), [8, 5, 2, 1], [2, 9, 1, 1]),
    at(widget("display", { show: "temperature" }), [10, 5, 2, 1], [3, 9, 1, 1]),
    at(widget("telltales", {}), [0, 6, 7, 1], [0, 10, 4, 1]),
    at(widget("messages", {}), [7, 6, 5, 1], [0, 11, 4, 1]),
  ];
  const buttons = [];
  for (let k = 1; k <= doors; k++) buttons.push(presetWidget(`door${k}`));
  buttons.push(presetWidget("doorsAll"), presetWidget("stopBrake"), presetWidget("parkingBrake"));
  buttons.push(Object.assign(widget("blinkers", {}), { sz: { l: [3, 1], p: [3, 1] } }));
  for (const id of ["gearD", "gearN", "gearR", "horn", "headlights", "highBeam", "fog", "saloon", "driverLight", "wipers", "battery", "startup", "engine", "ticket", "pram", "microphone", "route", "clearLine", "viewDriver", "viewPax", "viewOutside", "camPrev", "camNext", "pause"]) buttons.push(presetWidget(id));
  const second = doors > 1 ? presetWidget("door2") : presetWidget("headlights");
  const wheel = [
    at(widget("wheel", {}), [0, 0, 6, 6], [0, 1, 4, 4]),
    at(widget("blinkers", {}), [6, 0, 3, 1], [0, 0, 3, 1]),
    at(presetWidget("horn"), [8, 2, 1, 1], [3, 0, 1, 1]),
    at(presetWidget("door1"), [6, 1, 1, 1], [0, 8, 1, 1]),
    at(second, [7, 1, 1, 1], [1, 8, 1, 1]),
    at(presetWidget("doorsAll"), [8, 1, 1, 1], [2, 8, 1, 1]),
    at(presetWidget("stopBrake"), [6, 2, 1, 1], [3, 8, 1, 1]),
    at(presetWidget("parkingBrake"), [7, 2, 1, 1], [3, 9, 1, 1]),
    at(presetWidget("gearD"), [6, 3, 1, 1], [0, 9, 1, 1]),
    at(presetWidget("gearN"), [7, 3, 1, 1], [1, 9, 1, 1]),
    at(presetWidget("gearR"), [8, 3, 1, 1], [2, 9, 1, 1]),
    at(widget("gauge", { label: "Speed" }), [6, 4, 3, 2], [0, 5, 1, 3]),
    at(widget("pedals", {}), [9, 0, 3, 6], [1, 5, 3, 3]),
  ];
  const pages = [
    { name: "Dashboard", widgets: dash, grid: { l: [12, 7], p: [4, 12] } },
    { name: "Buttons", widgets: buttons, grid: { l: [8, 5], p: [4, 9] } },
    { name: "Steering wheel", widgets: wheel, grid: { l: [12, 6], p: [4, 10] } },
  ];
  for (const o of ["l", "p"]) pages[1].grid[o][1] = pack(buttons, pages[1].grid[o][0], o, 1);
  for (const w of buttons) delete w.sz;
  return { v: 1, theme: "night", accent: "#f5a623", pages };
}
