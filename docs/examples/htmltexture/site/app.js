// A page written like a small site: a module with classes and template strings.
import { fmtTime } from './util.js';

class Dashboard {
  #last = {};
  constructor() {
    this.el = (id) => document.getElementById(id);
  }
  set(id, value) {
    if (this.#last[id] === value) return;
    this.#last[id] = value;
    this.el(id).textContent = value;
  }
  update({ vehicle, num }) {
    const kmh = Math.round(vehicle?.speed?.kmh ?? num.velocity ?? 0);
    this.set('speed', String(kmh));
    this.set('clock', fmtTime(omsi.time));
    this.set('line', vehicle?.route?.line ?? '-');
    this.set('stop', vehicle?.route?.nextStop ?? '-');
    const doors = (vehicle?.doors?.list ?? []).map((d, i) => `<div class="door${d.isOpen ? ' open' : ''}">${i + 1}</div>`).join('');
    if (this.#last.doors !== doors) {
      this.#last.doors = doors;
      this.el('doors').innerHTML = doors;
    }
  }
}

const dash = new Dashboard();
omsi.update = (data) => dash.update(data);
