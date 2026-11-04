"use strict";

const Drive = {
  steer: 0,
  out: 0,
  throttle: 0,
  brake: 0,
  wheels: new Set(),
  fingers: new Map(),
  lock: 360,
  timer: null,
  quiet: 0,
  last: 0,
  sent: null,
  sentAt: 0,
  centring: false,

  holding() {
    for (const f of this.fingers.values()) if (f.kind === "wheel") return true;
    return false;
  },

  wake() {
    if (this.timer) return;
    this.last = performance.now();
    this.quiet = 0;
    this.timer = setInterval(() => this.tick(), 16);
  },

  tick() {
    const now = performance.now();
    const dt = Math.min(0.1, (now - this.last) / 1000);
    this.last = now;
    if (this.centring && !this.holding()) {
      const back = 1.1 * dt;
      this.steer = Math.abs(this.steer) <= back ? 0 : this.steer - Math.sign(this.steer) * back;
      if (this.steer === 0) this.centring = false;
    } else if (!this.holding() && this.steer !== 0) {
      const kmh = Math.abs(Number(R.get(App.state, "vehicle.motion.speedKmh")) || 0);
      const back = Math.min(1, kmh / 25) * 1.6 * dt;
      this.steer = Math.abs(this.steer) <= back ? 0 : this.steer - Math.sign(this.steer) * back;
    }
    const a = Math.abs(this.steer);
    const target = Math.sign(this.steer) * (0.35 * a + 0.65 * a * a);
    const eased = this.out + (target - this.out) * (1 - Math.exp(-dt / 0.09));
    this.out += Math.max(-dt / 0.8, Math.min(dt / 0.8, eased - this.out));
    if (Math.abs(this.out - target) < 0.0005) this.out = target;
    const msg = { t: "drive", s: Math.round(this.out * 1000) / 1000, g: Math.round(this.throttle * 1000) / 1000, b: Math.round(this.brake * 1000) / 1000 };
    const moved = !this.sent || msg.s !== this.sent.s;
    const stale = this.out !== 0 && now - this.sentAt > 100;
    const busy = this.holding() || this.throttle > 0 || this.brake > 0 || moved;
    this.quiet = busy ? 0 : this.quiet + 1;
    if (App.ws && App.ws.readyState === 1 && (this.quiet < 6 || stale)) {
      App.send(msg);
      this.sent = msg;
      this.sentAt = now;
    }
    for (const w of this.wheels) {
      if (w.isConnected) this.paint(w);
      else this.wheels.delete(w);
    }
    if (this.quiet >= 6 && this.out === 0) {
      clearInterval(this.timer);
      this.timer = null;
    }
  },

  paint(node) {
    const deg = this.steer * (Number(node.dataset.lock) || this.lock);
    node.querySelector(".rim").style.transform = `rotate(${deg}deg)`;
    node.querySelector(".deg").textContent = `${Math.round(deg)}°`;
    node.classList.toggle("centring", this.centring);
  },

  release(id) {
    const f = this.fingers.get(id);
    if (!f) return;
    this.fingers.delete(id);
    if (f.kind === "combined") {
      this.throttle = 0;
      this.brake = 0;
      f.node.style.setProperty("--up", 0);
      f.node.style.setProperty("--down", 0);
    } else if (f.kind !== "wheel") {
      this[f.kind] = 0;
      if (f.node) f.node.style.setProperty("--p", 0);
    }
    this.wake();
  },

  letGo() {
    this.fingers.clear();
    this.steer = 0;
    this.out = 0;
    this.centring = false;
    this.throttle = 0;
    this.brake = 0;
    for (const p of document.querySelectorAll(".pedal")) for (const v of ["--p", "--up", "--down"]) p.style.setProperty(v, 0);
    this.sent = null;
    this.wake();
  },

  grab(e, node, kind) {
    if (document.body.classList.contains("editing")) return false;
    for (const f of this.fingers.values()) if (f.node === node) return false;
    e.preventDefault();
    this.fingers.set(e.pointerId, { kind, node, angle: 0 });
    try { node.setPointerCapture(e.pointerId); } catch (_) {}
    return true;
  },

  wheel(w) {
    const d = R.el("div", "wheel");
    d.dataset.lock = String(w.max || this.lock);
    d.innerHTML = '<svg class="rim" viewBox="0 0 200 200"><circle cx="100" cy="100" r="88" fill="none" stroke-width="16"/><circle class="top" cx="100" cy="12" r="5"/><path class="spoke" d="M24 112 L76 104 L100 128 L124 104 L176 112" fill="none" stroke-width="14" stroke-linejoin="round"/><circle class="hub" cx="100" cy="104" r="24"/><path class="centre" d="M84 104l7-6v12z M116 104l-7-6v12z M99 96h2v16h-2z"/></svg><span class="deg">0°</span>';
    const at = (e) => {
      const r = d.getBoundingClientRect();
      return Math.atan2(e.clientY - (r.top + r.height / 2), e.clientX - (r.left + r.width / 2));
    };
    d.addEventListener("pointerdown", (e) => {
      const r = d.getBoundingClientRect();
      const reach = Math.hypot(e.clientX - (r.left + r.width / 2), e.clientY - (r.top + r.height / 2)) / (Math.min(r.width, r.height) / 2);
      if (reach < 0.3 && !document.body.classList.contains("editing")) {
        e.preventDefault();
        R.buzz();
        this.centring = true;
        this.wake();
        return;
      }
      if (!this.grab(e, d, "wheel")) return;
      this.centring = false;
      this.fingers.get(e.pointerId).angle = at(e);
      this.wake();
    });
    d.addEventListener("pointermove", (e) => {
      const f = this.fingers.get(e.pointerId);
      if (!f || f.node !== d) return;
      const a = at(e);
      let da = a - f.angle;
      if (da > Math.PI) da -= 2 * Math.PI;
      if (da < -Math.PI) da += 2 * Math.PI;
      f.angle = a;
      const lock = ((Number(d.dataset.lock) || this.lock) * Math.PI) / 180;
      this.steer = Math.max(-1, Math.min(1, this.steer + da / lock));
      this.wake();
    });
    d.addEventListener("contextmenu", (e) => e.preventDefault());
    this.wheels.add(d);
    this.paint(d);
    return d;
  },

  pedal(sink) {
    const d = R.el("div", "pedals one");
    const p = R.el("div", sink ? "pedal combined sink" : "pedal combined");
    p.append(R.el("div", "fill up"), R.el("div", "fill down"), R.el("span", "txt top", R.T(sink ? "Brake" : "Throttle")), R.el("span", "txt bottom", R.T(sink ? "Throttle" : "Brake")), R.el("div", "mid"));
    const set = (e) => {
      const r = p.getBoundingClientRect();
      const y = ((r.top + r.height / 2 - e.clientY) / Math.max(1, r.height / 2)) * (sink ? -1.1 : 1.1);
      const v = Math.abs(y) < 0.08 ? 0 : Math.max(-1, Math.min(1, y - Math.sign(y) * 0.08));
      this.throttle = Math.max(0, v);
      this.brake = Math.max(0, -v);
      p.style.setProperty("--up", this.throttle);
      p.style.setProperty("--down", this.brake);
      this.wake();
    };
    p.addEventListener("pointerdown", (e) => {
      if (!this.grab(e, p, "combined")) return;
      R.buzz();
      set(e);
    });
    p.addEventListener("pointermove", (e) => {
      const f = this.fingers.get(e.pointerId);
      if (f && f.node === p) set(e);
    });
    p.addEventListener("contextmenu", (e) => e.preventDefault());
    d.append(p);
    return d;
  },

  pedals(w) {
    if (w && (w.look === "one" || w.look === "oneDown")) return this.pedal(w.look === "oneDown");
    const rise = !!(w && w.push === "up");
    const d = R.el("div", rise ? "pedals rise" : "pedals");
    for (const [k, label] of [["brake", "Brake"], ["throttle", "Throttle"]]) {
      const p = R.el("div", "pedal " + k);
      p.append(R.el("div", "fill"), R.el("span", "txt", R.T(label)));
      const set = (e) => {
        const r = p.getBoundingClientRect();
        const at = (e.clientY - r.top) / Math.max(1, r.height);
        const v = Math.max(0, Math.min(1, (rise ? 1 - at : at) * 1.15 - 0.05));
        this[k] = v;
        p.style.setProperty("--p", v);
        this.wake();
      };
      p.addEventListener("pointerdown", (e) => {
        if (!this.grab(e, p, k)) return;
        R.buzz();
        set(e);
      });
      p.addEventListener("pointermove", (e) => {
        const f = this.fingers.get(e.pointerId);
        if (f && f.node === p) set(e);
      });
      p.addEventListener("contextmenu", (e) => e.preventDefault());
      d.append(p);
    }
    return d;
  },
};

for (const type of ["pointerup", "pointercancel", "lostpointercapture"]) document.addEventListener(type, (e) => Drive.release(e.pointerId), true);
document.addEventListener("visibilitychange", () => { if (document.hidden) Drive.letGo(); });
