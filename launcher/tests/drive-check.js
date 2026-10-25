import { html, render } from '../ui/vendor/preact-htm.js';
import { Tours } from '../ui/js/pages/drive-duty.js';
import { S, set, remember } from '../ui/js/store.js';
import { applyTheme, choose, curTour, duty, resolveTour, transportRefusal, LAUNCHER_LAYOUTS } from '../ui/js/data.js';

const OPTIONS = {
  Map: () => one('.page-drive .map-row'),
  'Map note': () => labelled(/^Add a note$/) || one('.page-drive textarea'),
  Bus: () => labelled(/^Change bus$/) || one('.page-drive .bus-list'),
  Livery: () => labelled(/^Preview next livery$/) || one('.page-drive .livery .select'),
  'Fleet number': () => field(/^Fleet number$/),
  'Custom fleet number': () => one('.page-drive input[placeholder="Custom"]'),
  'Number plate': () => field(/^Number plate$/),
  'Depot file': () => field(/^Depot file$/),
  'Free drive': () => labelled(/^Free drive$/),
  Line: () => one('.page-drive .line-row'),
  Tour: () => one('.page-drive .tour-row'),
  'Trip to start with': () => one('.page-drive .roadbook .trip-row'),
  'Next departures': () => one('.page-drive .next-tours summary'),
  'Buses of the tour': () => text(/^Buses the timetable gives this tour:$/),
  'Start at': () => field(/^Start at$/),
  'Route map': () => one('.page-drive .routemap canvas'),
  IBIS: () => one('.page-drive .ibis-box'),
  'Start time': () => field(/^Start time$/),
  'Current time': () => labelled(/^Current time$/),
  Date: () => field(/^Date$/),
  Calendar: () => labelled(/^Choose a date$/),
  'Current date': () => labelled(/^Current date$/),
  Season: () => field(/^Season$/),
  'Natural weather': () => text(/^Natural weather$/),
  'Custom weather': () => text(/^Custom weather$/),
  'Weather cycle': () => text(/^Weather cycle$/),
  'Tomorrow.io': () => text(/^Tomorrow\.io$/),
  'Cars around': () => text(/^Cars around$/),
  Passengers: () => text(/^Passengers$/),
  'Timetable buses': () => text(/^Timetable buses$/),
  'Bus ready to go': () => text(/^Bus ready to go$/),
  'Start on foot': () => text(/^Start on foot$/),
  Start: () => labelled(/^(Start the duty|Start driving)$/),
  'Continue a save': () => labelled(/^Continue$/),
  'Command line': () => text(/^Show the command line$/),
};

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const frame = () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
const settle = async (ms = 250) => {
  await frame();
  await sleep(ms);
  await frame();
};

function shown(el) {
  if (!el) return false;
  if (el.checkVisibility && !el.checkVisibility({ visibilityProperty: true, contentVisibilityAuto: true })) return false;
  if (el.closest('details:not([open])') && !el.closest('summary')) return false;
  const r = el.getBoundingClientRect();
  return r.width > 2 && r.height > 2;
}

const inView = (el) => {
  const r = el.getBoundingClientRect();
  return shown(el) && r.bottom > 0 && r.right > 0 && r.top < innerHeight && r.left < innerWidth;
};

const one = (sel) => [...document.querySelectorAll(sel)].find(shown);
const label = (el) => (el.getAttribute('title') || el.getAttribute('aria-label') || el.textContent || el.className || el.tagName).trim().replace(/\s+/g, ' ').slice(0, 40);
const labelled = (re) => [...document.querySelectorAll('button, [role=button], [role=tab], a, summary')].find((b) => shown(b) && re.test(label(b)));
const field = (re) => [...document.querySelectorAll('.field > label, .dc-field > span:first-child, .label')].find((l) => shown(l) && re.test(l.textContent.trim()));

function text(re) {
  const walk = document.createTreeWalker(document.querySelector('#app'), NodeFilter.SHOW_TEXT);
  for (let n = walk.nextNode(); n; n = walk.nextNode()) if (re.test(n.data.trim()) && shown(n.parentElement)) return n.parentElement;
  return null;
}

function onTop(el) {
  const r = el.getBoundingClientRect();
  const x = Math.min(Math.max(r.left + r.width / 2, 1), innerWidth - 1);
  const y = Math.min(Math.max(r.top + r.height / 2, 1), innerHeight - 1);
  const hit = document.elementFromPoint(x, y);
  if (hit?.closest('.toasts')) return true;
  return !!hit && (hit === el || el.contains(hit) || hit.contains(el));
}

function clipped(el) {
  const b = el.getBoundingClientRect();
  for (let p = el.parentElement; p && p !== document.body; p = p.parentElement) {
    const o = getComputedStyle(p).overflowY;
    if (o !== 'auto' && o !== 'scroll' && o !== 'hidden') continue;
    const a = p.getBoundingClientRect();
    if (b.top < a.top - 1 || b.bottom > a.bottom + 1) return true;
  }
  return false;
}

function scan(where) {
  const out = [];
  const page = document.querySelector('.page-drive');
  if (!page) return [`${where}: no Drive page`];
  if (page.scrollWidth > page.clientWidth + 1) out.push(`${where}: the page scrolls sideways (${page.scrollWidth} > ${page.clientWidth})`);
  for (const el of page.querySelectorAll('button, a, input, .select, [role=button], summary, textarea')) {
    if (!inView(el) || el.closest('.routemap') || clipped(el)) continue;
    if (!onTop(el)) {
      const r = el.getBoundingClientRect();
      const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
      out.push(`${where}: "${label(el)}" is covered by ${hit?.className || hit?.tagName}`);
    }
    if (el.tagName === 'INPUT' && el.type !== 'checkbox' && el.type !== 'range' && el.scrollWidth > el.clientWidth + 1) out.push(`${where}: the field "${el.value}" is cut off`);
  }
  for (const col of page.querySelectorAll('.duty-col, .drive-panel-body > *')) {
    const n = col.nextElementSibling;
    if (n && shown(n) && shown(col) && col.getBoundingClientRect().bottom > n.getBoundingClientRect().top + 1) out.push(`${where}: ${col.className} runs under ${n.className}`);
  }
  return out;
}

function dialogOk(where, what) {
  const d = [...document.querySelectorAll('.dialog')].pop();
  if (!d) return [`${where}: ${what} did not open`];
  const r = d.getBoundingClientRect();
  const out = [];
  if (r.width < 260 || r.height < 120) out.push(`${where}: ${what} is too small (${Math.round(r.width)}x${Math.round(r.height)})`);
  if (r.left < -1 || r.top < -1 || r.right > innerWidth + 1 || r.bottom > innerHeight + 1) out.push(`${where}: ${what} is off the screen`);
  if (!onTop(d.querySelector('.dialog-head') || d)) out.push(`${where}: ${what} is under something else`);
  return out;
}

async function closeDialog(what) {
  const n = document.querySelectorAll('.dialog').length;
  const d = [...document.querySelectorAll('.dialog')].pop();
  const x = d && [...d.querySelectorAll('button')].find((b) => /^(Cancel|Close)$/.test(label(b)));
  if (x) x.click();
  else window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
  await settle();
  return document.querySelectorAll('.dialog').length >= n ? [`${what} would not close`] : [];
}

function reach(re, root = document.querySelector('.page-drive')) {
  const b = [...root.querySelectorAll('button, [role=button], .map-row')].find((x) => re.test(label(x)) && shown(x));
  b?.scrollIntoView({ block: 'nearest' });
  return b && inView(b) ? b : null;
}

async function weatherEditor(where, root) {
  const tile = [...root.querySelectorAll('.weather-tile')].find((x) => shown(x) && /Custom weather/.test(x.textContent));
  if (!tile) return [];
  const n = document.querySelectorAll('.dialog').length;
  tile.click();
  await settle(500);
  if (document.querySelectorAll('.dialog').length <= n) return [`${where}: the weather editor did not open`];
  return [...dialogOk(where, 'the weather editor'), ...(await closeDialog('the weather editor'))];
}

async function exercise(where) {
  const out = [];
  const before = { ...S.choice };
  let b = reach(/^Change bus$/);
  if (b) {
    b.click();
    await settle(500);
    out.push(...dialogOk(where, 'the bus picker'), ...(await closeDialog('the bus picker')));
  }
  b = reach(/^Vehicle settings & details$/);
  if (b && !b.classList.contains('disclose')) {
    b.click();
    await settle(400);
    out.push(...dialogOk(where, 'the vehicle settings'), ...(await closeDialog('the vehicle settings')));
  }
  b = document.querySelector('.page-drive .map-row.click');
  b?.scrollIntoView({ block: 'nearest' });
  if (b && inView(b)) {
    b.click();
    await settle(400);
    out.push(...dialogOk(where, 'the map picker'), ...(await closeDialog('the map picker')));
  }
  b = reach(/^Day & weather$/);
  if (b) {
    b.click();
    await settle(400);
    out.push(...dialogOk(where, 'the day dialog'));
    const top = () => [...document.querySelectorAll('.dialog')].pop();
    const cal = reach(/^Choose a date$/, top());
    if (cal) {
      cal.click();
      await settle(300);
      out.push(...dialogOk(where, 'the calendar'), ...(await closeDialog('the calendar')));
      cal.click();
      await settle(300);
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      await settle();
      if (document.querySelectorAll('.dialog').length !== 1) out.push(`${where}: Escape in the calendar closed the day dialog too`);
    } else out.push(`${where}: the day dialog has no calendar`);
    out.push(...(await weatherEditor(where, top())), ...(await closeDialog('the day dialog')));
  }
  if (!document.querySelector('.dialog')) {
    const cal = reach(/^Choose a date$/);
    if (cal) {
      cal.click();
      await settle(300);
      out.push(...dialogOk(where, 'the calendar'), ...(await closeDialog('the calendar')));
    }
    out.push(...(await weatherEditor(where, document.querySelector('.page-drive'))));
  }
  const next = reach(/^Preview next livery$/);
  if (next && !next.disabled) {
    const p = S.choice.paint;
    next.click();
    await settle();
    if (S.choice.paint === p) out.push(`${where}: the livery arrow did nothing`);
  }
  const lines = [...document.querySelectorAll('.page-drive .line-row')].filter(shown);
  if (lines[1]) {
    lines[1].click();
    await settle(400);
    if (S.choice.line === before.line) out.push(`${where}: choosing a line did nothing`);
    lines[0].click();
    await settle(400);
  }
  const tours = [...document.querySelectorAll('.page-drive .tour-row')].filter(shown);
  if (tours[1]) {
    tours[1].click();
    await settle(300);
    if (S.choice.tour === before.tour) out.push(`${where}: choosing a tour did nothing`);
  }
  const trips = [...document.querySelectorAll('.page-drive .roadbook .trip-row')].filter((x) => inView(x) && onTop(x) && !clipped(x));
  if (trips[1]) {
    trips[1].click();
    await settle(300);
    if (!S.choice.start_trip) out.push(`${where}: choosing a trip did nothing`);
  }
  const nt = document.querySelector('.page-drive .next-tours');
  if (nt && shown(nt)) {
    nt.querySelector('summary').click();
    await settle();
    out.push(...scan(`${where} (next departures open)`));
    nt.open = false;
    await settle();
  }
  const cont = reach(/^Continue$/);
  if (cont) {
    cont.click();
    await settle();
    const m = document.querySelector('.menu.up');
    if (!m || !shown(m)) out.push(`${where}: the saves menu did not open`);
    else {
      if (m.getBoundingClientRect().top < 0) out.push(`${where}: the saves menu goes off the screen`);
      const del = m.querySelector('button[title="Delete"]');
      if (del) {
        del.click();
        await settle();
        out.push(...dialogOk(where, 'the delete question'), ...(await closeDialog('the delete question')));
      } else out.push(`${where}: a save cannot be deleted`);
    }
    document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    await settle();
  }
  const pip = document.querySelector('.cp-pip');
  if (pip) {
    pip.click();
    await settle(500);
    out.push(...scan(`${where} (picture swapped)`));
    document.querySelector('.cp-pip')?.click();
    await settle(400);
  }
  const free = reach(/^Free drive$/);
  if (free) {
    free.click();
    await settle(400);
    out.push(...scan(`${where} (free drive)`));
    reach(/^Timetable$/)?.click();
    await settle(400);
  }
  set({ choice: { ...S.choice, line: before.line, tour: before.tour, start_trip: before.start_trip, paint: before.paint, free: before.free } });
  await settle(300);
  return out;
}

async function options(where) {
  const found = new Set();
  const look = () => {
    for (const [name, find] of Object.entries(OPTIONS)) if (!found.has(name) && find()) found.add(name);
  };
  const steps = [...document.querySelectorAll('.page-drive .drive-step')];
  for (const step of steps.length ? steps : [null]) {
    step?.click();
    await settle(500);
    look();
    const nt = document.querySelector('.page-drive .next-tours');
    if (nt) {
      nt.open = true;
      await settle();
      look();
      nt.open = false;
    }
    const more = [...document.querySelectorAll('.page-drive .disclose')].filter(shown);
    for (const d of more) {
      d.click();
      await settle();
      look();
      d.click();
      await settle();
    }
    for (const re of [/^Day & weather$/, /^Vehicle settings & details$/]) {
      const b = [...document.querySelectorAll('.page-drive button')].find((x) => shown(x) && re.test(label(x)) && !x.classList.contains('disclose'));
      if (!b) continue;
      b.click();
      await settle(400);
      const dialog = [...document.querySelectorAll('.dialog')].pop();
      for (const d of dialog ? [...dialog.querySelectorAll('.disclose')] : []) {
        d.click();
        await settle();
        look();
        d.click();
        await settle();
      }
      look();
      await closeDialog('');
    }
  }
  return Object.keys(OPTIONS).filter((k) => !found.has(k)).map((k) => `${where}: "${k}" cannot be found`);
}

/** A bus does not run on a tram map; a map without modes takes everything. */
function transportModes() {
  const out = [];
  const bus = { transport: ['bus'] };
  const tram = { transport: ['tram'] };
  if (transportRefusal({}, bus) || transportRefusal({ transport: [] }, tram)) out.push('transport: a map without modes must take every vehicle');
  if (!transportRefusal({ transport: ['tram'] }, bus)) out.push('transport: a bus must not run on a tram map');
  if (transportRefusal({ transport: ['tram', 'bus'] }, bus) || transportRefusal({ transport: ['tram'] }, tram)) out.push('transport: a vehicle of the map\'s mode must run');
  return out;
}

/** A date the player typed stays when a duty is chosen, even outside the map's `[years]`. */
async function dateOutsideTheMapsYears() {
  const out = [];
  const maps = S.maps;
  const choice = { ...S.choice };
  const lines = S.lines;
  const map = maps?.find((m) => m.file === choice.map) || maps?.[0];
  if (!map) return ['date: no map to choose a date on'];
  set({ maps: maps.map((m) => (m.file === map.file ? { ...m, year_span: [1986, 2000] } : m)) });
  set({ choice: { ...choice, map: map.file, date: '1989-05-30', line: null, tour: null, start_trip: null } });
  choose({ date: '2025-06-01' });
  await settle(150);
  if (S.choice.date !== '2025-06-01') out.push(`date: typing 2025-06-01 gave ${S.choice.date}`);
  choose({ line: S.choice.line, tour: S.choice.tour, start_trip: null });
  await settle(150);
  if (S.choice.date !== '2025-06-01') out.push(`date: choosing a duty changed it to ${S.choice.date}`);
  set({ maps, choice, lines });
  return out;
}

/** Shared codes must select, highlight and launch the requested weekday variant. */
async function duplicateTourNumbers() {
  const out = [];
  const saved = { choice: S.choice, lines: S.lines, settings: S.settings };
  const base = S.lines.list?.find((l) => l.tours.length > 1);
  if (!base) return ['duplicate tours: fixture needs two tours'];
  const first = { ...base.tours[0], id: 'weekday-a', number: '1', runs: true };
  const second = { ...base.tours[1], id: 'weekday-b', number: '1', runs: false, next_run: '1989-05-31' };
  const line = { ...base, tours: [first, second] };
  const host = document.createElement('div');
  document.body.append(host);
  const originalFetch = window.fetch;
  window.fetch = (url, ...args) => String(url).endsWith('/lines.json')
    ? Promise.resolve(new Response(JSON.stringify([{ ...line, tours: line.tours.map((x) => ({ ...x, runs: S.choice.date === second.next_run ? x.id === second.id : x.id === first.id })) }])))
    : originalFetch(url, ...args);
  try {
    set({ choice: { ...S.choice, line: line.name, tour: first.id, time: 0, date: '1989-05-30', free: false, lan_mode: 'off', season: 'auto' }, lines: { ...S.lines, list: [line] }, settings: { ...S.settings, use_real_date: false } });
    render(html`<${Tours} line=${line} />`, host);
    await settle();
    host.querySelectorAll('.tour-row')[1].click();
    if (S.choice.date !== second.next_run) out.push('duplicate tours: selecting another day did not advance the date');
    await settle();
    if (S.choice.tour !== second.id || curTour()?.id !== second.id) out.push('duplicate tours: second variant cannot be selected');
    if ((await duty()).tour !== '1') out.push('duplicate tours: launch must use the displayed tour code');
    if (resolveTour(line, '1') !== first) out.push('duplicate tours: legacy selection did not prefer the active variant');
    await settle();
    const selected = [...host.querySelectorAll('.tour-row[aria-selected="true"]')];
    if (selected.length !== 1) out.push(`duplicate tours: expected one selected row, got ${selected.length}`);
    host.querySelectorAll('.tour-row')[0].click();
    if (S.choice.tour !== first.id || curTour()?.id !== first.id) out.push('duplicate tours: cannot switch back');
  } finally {
    render(null, host);
    host.remove();
    window.fetch = originalFetch;
    set(saved);
  }
  return out;
}

export async function run({ themes = ['dark', 'light'], layouts = LAUNCHER_LAYOUTS.map(([id]) => id), check = ['options', 'use'] } = {}) {
  const report = {};
  for (const theme of themes) for (const layout of layouts) {
    localStorage.setItem('theme', theme);
    remember('launcher_layout', layout);
    applyTheme();
    set({ page: 'profile' });
    await settle(100);
    set({ page: 'drive' });
    await settle(900);
    const where = `${innerWidth}x${innerHeight} ${theme} ${layout}`;
    const out = [];
    if (check.includes('options') && theme === themes[0]) out.push(...(await options(where)));
    if (check.includes('use')) {
      const steps = [...document.querySelectorAll('.page-drive .drive-step')];
      for (const step of steps.length ? steps : [null]) {
        step?.click();
        await settle(500);
        const w = step ? `${where} (${label(step).replace(/^\d/, '')})` : where;
        out.push(...scan(w), ...(await exercise(w)));
      }
    }
    report[where] = [...new Set(out)];
  }
  report['duplicate tour numbers'] = await duplicateTourNumbers();
  report['duty date'] = await dateOutsideTheMapsYears();
  report['transport modes'] = transportModes();
  return report;
}
