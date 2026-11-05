"use strict";

const MapView = {
  roads: null,
  nav: null,
  views: new Set(),
  running: false,
  scale: 1.6,
  zoom: 0,

  zoomBy(f) {
    const z = Math.max(0.25, Math.min(3, this.zoom * f));
    this.scale *= z / this.zoom;
    this.zoom = z;
    store.set("openomsi.mapZoom", String(Math.round(z * 1000) / 1000));
    this.wake();
  },

  attach(view) {
    if (!this.zoom) this.zoom = Math.max(0.25, Math.min(3, Number(store.get("openomsi.mapZoom")) || 1));
    view.querySelector(".zoom .in").addEventListener("click", () => this.zoomBy(1.5));
    view.querySelector(".zoom .out").addEventListener("click", () => this.zoomBy(1 / 1.5));
    const canvas = view.querySelector("canvas");
    const touches = new Map();
    let spread = 0;
    const apart = () => {
      const [a, b] = [...touches.values()];
      return Math.hypot(a.x - b.x, a.y - b.y);
    };
    canvas.addEventListener("pointerdown", (e) => {
      touches.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (touches.size === 2) spread = apart();
    });
    canvas.addEventListener("pointermove", (e) => {
      if (!touches.has(e.pointerId)) return;
      touches.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (touches.size !== 2 || !spread) return;
      const d = apart();
      if (d > 0) this.zoomBy(d / spread);
      spread = d;
    });
    for (const type of ["pointerup", "pointercancel", "pointerleave"]) canvas.addEventListener(type, (e) => { touches.delete(e.pointerId); spread = 0; });
    canvas.addEventListener("wheel", (e) => { e.preventDefault(); this.zoomBy(Math.exp(-e.deltaY / 400)); }, { passive: false });
  },

  take(m) {
    if (m.t === "roads") this.roads = m;
    if (m.t === "nav") this.nav = m;
    this.version = (this.version || 0) + 1;
    this.wake();
  },

  wake() {
    if (this.running || !this.views.size) return;
    this.running = true;
    const tick = () => {
      for (const v of this.views) if (!v.isConnected) this.views.delete(v);
      if (!this.views.size) { this.running = false; return; }
      for (const v of this.views) this.draw(v);
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  },

  draw(view) {
    const canvas = view.querySelector("canvas");
    const s = App.state;
    const box = { width: canvas.clientWidth, height: canvas.clientHeight };
    const key = [R.get(s, "vehicle.motion.x"), R.get(s, "vehicle.motion.y"), R.get(s, "vehicle.motion.heading"), this.version, box.width, box.height, Math.round(this.scale * 100)].join("|");
    if (view.drawn === key) return;
    view.drawn = key;
    const dpr = Math.min(3, devicePixelRatio || 1);
    const W = Math.max(1, Math.round(box.width * dpr)), H = Math.max(1, Math.round(box.height * dpr));
    if (canvas.width !== W || canvas.height !== H) { canvas.width = W; canvas.height = H; }
    const g = canvas.getContext("2d");
    const css = view.css || (view.css = getComputedStyle(view));
    g.fillStyle = css.getPropertyValue("--map-bg").trim() || "#16191d";
    g.fillRect(0, 0, W, H);
    const x = Number(R.get(s, "vehicle.motion.x")), y = Number(R.get(s, "vehicle.motion.y"));
    const empty = !this.roads || !Number.isFinite(x) || !Number.isFinite(y);
    view.classList.toggle("empty", empty);
    if (empty) return;
    const speed = Math.abs(Number(R.get(s, "vehicle.motion.speedKmh")) || 0);
    const want = Math.max(1.1, 4.2 - speed * 0.045) * dpr * this.zoom;
    this.scale += (want - this.scale) * 0.04;
    const k = this.scale;
    const h = (Number(R.get(s, "vehicle.motion.heading")) || 0) * Math.PI / 180;
    const [fx, fy, rx, ry] = [Math.sin(h), Math.cos(h), Math.cos(h), -Math.sin(h)];
    const cx = W / 2, cy = H * 0.7;
    const project = (ax, ay, px, py) => {
      const dx = ax + px - x, dy = ay + py - y;
      return [cx + (dx * rx + dy * ry) * k, cy - (dx * fx + dy * fy) * k];
    };
    const path = (anchor, p) => {
      g.beginPath();
      for (let i = 0; i < p.length; i += 2) {
        const [sx, sy] = project(anchor[0], anchor[1], p[i], p[i + 1]);
        if (i) g.lineTo(sx, sy); else g.moveTo(sx, sy);
      }
    };
    g.lineCap = "round";
    g.lineJoin = "round";
    const a = this.roads.anchor;
    for (const pass of [0, 1]) {
      g.strokeStyle = pass ? (css.getPropertyValue("--map-road").trim() || "#3b4049") : (css.getPropertyValue("--map-casing").trim() || "#0c0e11");
      for (const r of this.roads.roads) {
        g.lineWidth = Math.max(2 * dpr, (r.w || 6) * k) + (pass ? 0 : 3 * dpr);
        path(a, r.p);
        g.stroke();
      }
    }
    const n = this.nav;
    if (n && n.route && n.route.length > 3) {
      g.strokeStyle = "#0a4fb0";
      g.lineWidth = Math.max(9 * dpr, 7 * k);
      path(n.anchor, n.route);
      g.stroke();
      g.strokeStyle = "#2f8cff";
      g.lineWidth = Math.max(6 * dpr, 5 * k);
      path(n.anchor, n.route);
      g.stroke();
    }
    const stops = (n && n.stops) || [];
    for (let i = stops.length - 2; i >= 0; i -= 2) {
      const [sx, sy] = project(n.anchor[0], n.anchor[1], stops[i], stops[i + 1]);
      const r = (i ? 10 : 13) * dpr;
      if (sx < -r || sy < -r || sx > W + r || sy > H + r) continue;
      g.save();
      g.translate(sx, sy);
      g.fillStyle = "#ffffff";
      g.strokeStyle = "#0a4fb0";
      g.lineWidth = 3 * dpr;
      g.beginPath(); g.arc(0, 0, r, 0, Math.PI * 2); g.fill(); g.stroke();
      const u = r / 13;
      g.fillStyle = "#0a4fb0";
      g.fillRect(-7 * u, -6 * u, 14 * u, 10 * u);
      g.fillStyle = "#ffffff";
      g.fillRect(-5 * u, -4 * u, 10 * u, 3.5 * u);
      g.fillStyle = "#0a4fb0";
      g.beginPath(); g.arc(-4 * u, 5 * u, 2 * u, 0, Math.PI * 2); g.arc(4 * u, 5 * u, 2 * u, 0, Math.PI * 2); g.fill();
      g.restore();
    }
    g.save();
    g.translate(cx, cy);
    g.fillStyle = "rgba(47, 140, 255, .25)";
    g.beginPath(); g.arc(0, 0, 22 * dpr, 0, Math.PI * 2); g.fill();
    g.fillStyle = "#ffffff";
    g.beginPath(); g.arc(0, 0, 13 * dpr, 0, Math.PI * 2); g.fill();
    g.fillStyle = "#2f8cff";
    g.beginPath(); g.moveTo(0, -9 * dpr); g.lineTo(7 * dpr, 7 * dpr); g.lineTo(0, 3 * dpr); g.lineTo(-7 * dpr, 7 * dpr); g.closePath(); g.fill();
    g.restore();
  },

  overlay(view) {
    const n = this.nav, s = App.state;
    const turn = view.querySelector(".turn"), stop = view.querySelector(".stopbar"), spd = view.querySelector(".spd");
    const t = n && n.turn;
    turn.hidden = !t;
    if (t) {
      const icon = t.dir === 2 ? "uturn" : t.dir < 0 ? "turnLeft" : "turnRight";
      if (turn.dataset.icon !== icon) { turn.dataset.icon = icon; turn.firstChild.replaceChildren(picto(icon)); }
      turn.querySelector(".dist").textContent = t.m >= 1000 ? `${(t.m / 1000).toFixed(1)} km` : `${Math.max(10, Math.round(t.m / 10) * 10)} m`;
      turn.querySelector(".street").textContent = t.street || "";
    }
    const st = n && n.stop;
    stop.hidden = !st;
    if (st) {
      stop.querySelector(".name").textContent = st.name;
      stop.querySelector(".dist").textContent = st.m >= 1000 ? `${(st.m / 1000).toFixed(1)} km` : `${Math.round(st.m)} m`;
      stop.querySelector(".time").textContent = String(R.get(s, "env.time.asString") || "").slice(0, 5);
    }
    spd.querySelector(".v").textContent = Math.round(Math.abs(Number(R.get(s, "vehicle.motion.speedKmh")) || 0));
    const lim = n && n.limit;
    spd.querySelector(".lim").hidden = !lim;
    if (lim) spd.querySelector(".lim").textContent = Math.round(lim);
    const street = view.querySelector(".here");
    street.textContent = (n && n.street) || "";
    street.hidden = !(n && n.street);
  },
};
