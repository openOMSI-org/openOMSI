// Loading the content and keeping the duty: what the pages share.

import { call, on } from './api.js';
import { S, set, toast } from './store.js';
import { setLanguage, t, hhmm } from './i18n.js';

/** The duty as the launcher remembers it (`launcher-duty.json`, shared with the game's own
 * launcher window: the same fields). */
export const CHOICE_DEFAULTS = {
  bus: '', paint: '', plate: '', number: '', hof: '', hof_manual: false,
  map: '', entry: -1, line: null, tour: null, free: false,
  time: 9 * 60, start_trip: null, date: '1989-05-30',
  season: 'auto', phase: 'mid', own_date: null,
  weather: '', traffic: 30, passengers: true, schedule: true, autostart: false, on_foot: false,
  lan_mode: 'off', lan_addr: '', version: 2,
};

export async function boot() {
  const b = await call('bootstrap');
  await setLanguage(b.language);
  const choice = { ...CHOICE_DEFAULTS, ...(b.choice || {}) };
  // (a server joined in an earlier session is not joined again by itself)
  if (choice.lan_mode === 'join') choice.lan_mode = 'off';
  if (choice.version < 2) Object.assign(choice, { entry: -1, version: 2 });
  Object.assign(choice, clockPatch(choice, b.settings));
  set({ boot: b, settings: b.settings, config: b.config, choice, favourites: b.favourites || [], ready: true, page: b.root ? 'drive' : 'setup' });
  applyTheme();
  if (b.root) {
    loadContent();
    readAhead();
    // (the bus preview's helper and renderer get ready while the page is read)
    call('preview_warm').catch(() => {});
  }
  startPolling();
  on('vehicles-progress', (p) => set({ vehiclesProgress: p }));
}

export function applyTheme() {
  const pick = localStorage.getItem('theme') || 'dark';
  const light = pick === 'light' || (pick === 'system' && matchMedia('(prefers-color-scheme: light)').matches);
  document.documentElement.dataset.theme = light ? 'light' : 'dark';
}

export async function loadContent() {
  const errors = { ...S.errors };
  const [maps, weather] = await Promise.all([
    call('maps').catch((e) => ((errors.maps = e.message), [])),
    call('weather').catch((e) => ((errors.weather = e.message), [])),
  ]);
  // (a weather's name comes in English where OMSI has none in the interface's language: the
  // interface's tables translate it further)
  set({ maps, weather: weather.map((w) => ({ ...w, name: t(w.name) })), errors });
  fixChoice();
  loadLines();
  loadSituations();
  call('vehicles')
    .then((vehicles) => {
      set({ vehicles, vehiclesProgress: null });
      fixChoice();
      updateHof();
    })
    .catch((e) => set({ vehicles: [], errors: { ...S.errors, vehicles: e.message } }));
  loadProfiles();
}

export const weatherOfItsOwn = (w) => /^(cycle$|metar:|custom:)/i.test(w || '');

export const weatherName = (w, list) =>
  /^metar:/i.test(w || '') ? `${t('Current weather')}${w.length > 6 ? ` (${w.slice(6)})` : ''}`
  : /^cycle$/i.test(w || '') ? t('Weather cycle')
  : /^custom:/i.test(w || '') ? t('Custom weather')
  : list?.find((x) => x.file === w)?.name || t('Natural weather');

/** A remembered map or bus that is not installed any more gives way to the first one. */
function fixChoice() {
  const c = { ...S.choice };
  if (S.maps?.length && !S.maps.some((m) => m.file === c.map)) {
    c.map = (S.maps.find((m) => /grundorf/i.test(m.file)) || S.maps[0]).file;
    c.entry = -1;
  }
  if (S.vehicles?.length && !S.vehicles.some((v) => v.file === c.bus)) {
    c.bus = S.vehicles[0].file;
    c.paint = '';
    c.number = '';
  }
  if (S.weather && c.weather && !weatherOfItsOwn(c.weather) && !S.weather.some((w) => w.file === c.weather)) c.weather = '';
  set({ choice: c });
}

const norm = (f) => (f || '').replace(/\\/g, '/').toLowerCase();

/** A bus as people call it (its [friendlyname]), from its file: never "MAN_SD77.bus". */
export function busName(file) {
  const v = S.vehicles?.find((x) => norm(x.file) === norm(file));
  if (v) return v.name;
  return (file || '').replace(/\\/g, '/').split('/').pop().replace(/\.(bus|ovh)$/i, '').replace(/_/g, ' ').trim();
}

/** A map as people call it, from its file (`maps/Grundorf/global.cfg`). */
export function mapName(file) {
  const m = S.maps?.find((x) => norm(x.file) === norm(file));
  if (m) return m.friendly || m.name;
  return (file || '').replace(/\\/g, '/').split('/').filter(Boolean).slice(-2, -1)[0] || file;
}

export const curMap = () => S.maps?.find((m) => m.file === S.choice?.map);
export const curBus = () => S.vehicles?.find((v) => v.file === S.choice?.bus);
export const curLine = () => S.lines.list?.find((l) => l.name === S.choice?.line);
export const curTour = () => curLine()?.tours.find((x) => x.number === S.choice?.tour);
export const realTime = () => !!S.settings?.use_real_time;
export const realDate = () => !!S.settings?.use_real_date;

/** The time and date the duty takes from the computer's clock, as `duty_args` gives them to
 * the game (the year stays the duty's unless the real year is on). */
function clockPatch(c, st) {
  const now = new Date();
  const patch = {};
  if (st?.use_real_time) {
    const m = now.getHours() * 60 + now.getMinutes();
    if (c.time !== m) patch.time = m;
  }
  if (st?.use_real_date) {
    const year = st.use_real_year ? now.getFullYear() : parseInt(c.date, 10) || now.getFullYear();
    const d = `${String(year).padStart(4, '0')}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`;
    if (c.date !== d) patch.date = d;
  }
  return patch;
}

/** Keep the duty shown on the computer's clock while the settings ask for it. */
export function followClock() {
  if (!S.choice) return;
  const patch = clockPatch(S.choice, S.settings);
  if (Object.keys(patch).length) choose(patch);
}

let saveTimer = 0;
/** Change the duty (and keep it on disk a moment later). */
export function choose(patch) {
  const before = S.choice;
  const c = { ...before, ...patch };
  set({ choice: c });
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => call('choice_save', { choice: S.choice }).catch(() => {}), 400);
  if (c.map !== before.map || c.date !== before.date) {
    loadLines();
  }
  if (c.map !== before.map) loadSituations();
  if (c.map !== before.map || c.bus !== before.bus || c.date !== before.date || c.line !== before.line || c.free !== before.free) updateHof();
}

export async function loadLines() {
  const c = S.choice;
  if (!c?.map) return;
  const key = `${c.map}|${c.date}`;
  if (S.lines.key === key && (S.lines.list || S.lines.loading)) return;
  set({ lines: { key, list: S.lines.key.split('|')[0] === c.map ? S.lines.list : null, loading: true, error: null } });
  try {
    const list = await call('lines', { map: c.map, date: c.date });
    if (S.lines.key !== key) return;
    set({ lines: { key, list, loading: false, error: null } });
    // a line or tour that is not there on this date gives way to the first one
    const line = list.find((l) => l.name === S.choice.line) || list.find((l) => l.user_allowed && l.tours.length) || null;
    const tour = line?.tours.find((x) => x.number === S.choice.tour) || line?.tours.find((x) => x.runs) || line?.tours[0] || null;
    if (line?.name !== S.choice.line || tour?.number !== S.choice.tour) choose({ line: line?.name ?? null, tour: tour?.number ?? null });
  } catch (e) {
    if (S.lines.key === key) set({ lines: { key, list: [], loading: false, error: e.message } });
  }
}

async function updateHof() {
  const c = S.choice;
  const hof = await call('default_hof', { map: curMap() || null, bus: curBus() || null, date: c.date, line: c.free ? null : c.line }).catch(() => '');
  set({ hof });
}

export async function loadSituations() {
  if (!S.choice?.map) return;
  set({ situations: await call('saved_situations', { map: S.choice.map }).catch(() => []) });
}

/** Page data kept between visits: answered at once from what was read last, read again
 * behind it (`fresh` = the command's answer is set into `S.cache[key]`). */
export function cached(key, cmd, args) {
  const p = call(cmd, args).then(
    (v) => (set({ cache: { ...S.cache, [key]: v } }), v),
    (e) => (set({ cache: { ...S.cache, [key]: { error: e.message } } }), null),
  );
  return S.cache[key] !== undefined ? Promise.resolve(S.cache[key]) : p;
}

/** What the pages show, read ahead while the launcher starts. */
function readAhead() {
  cached('mods', 'mods_status');
  cached('servers', 'servers_get');
  cached('tutorials', 'tutorials');
  cached('keys', 'keybindings_get');
}

export async function loadProfiles() {
  const profiles = await call('profiles').catch(() => []);
  set({ profiles });
  const name = S.config?.profile || profiles[0];
  if (name) set({ profile: await call('profile', { name }).catch(() => null) });
}

export async function saveSettings(patch) {
  const settings = { ...S.settings, ...patch };
  set({ settings });
  try {
    set({ settings: await call('settings_save', { settings }) });
  } catch (e) {
    toast(t('The settings could not be saved: {e}', { e: e.message }), 'error');
  }
  if ('language' in patch) {
    const lang = S.boot.languages.find((l) => l.code === S.settings.language);
    await setLanguage(lang?.iso || 'en');
    // (the content's names and descriptions are read in the new language)
    loadContent();
  }
}

/** The trip of the tour picked to start with (its place), while the time is still its. */
export function pickedTrip() {
  const c = S.choice;
  const st = c.start_trip;
  if (!st || c.free) return null;
  const [line, tour, index, time] = st;
  return line === c.line && tour === c.tour && (time === c.time || realTime()) ? index : null;
}

/** What the game is started with. */
export async function duty(extra = {}) {
  const c = S.choice;
  const seasonWord = c.season && c.season !== 'auto' ? (await call('season_date', { season: c.season, phase: c.phase, date: c.date, map: c.map, keep: true }).catch(() => null))?.word : null;
  const trip = pickedTrip();
  return {
    map: c.map,
    bus: c.bus,
    paint: c.paint || null,
    plate: c.plate?.trim() || null,
    number: c.number?.trim() || null,
    hof: (c.hof_manual ? c.hof : S.hof) || null,
    entry: c.entry,
    line: c.free ? null : c.line,
    tour: c.free ? null : c.tour,
    trip: trip != null ? String(trip) : null,
    whole_tour: trip != null,
    time: hhmm(c.time),
    date: c.date,
    weather: c.weather || null,
    traffic: Math.round(c.traffic),
    passengers: c.passengers,
    schedule: c.schedule,
    autostart: c.autostart,
    on_foot: c.on_foot,
    profile: S.config?.profile || null,
    lan: c.lan_mode === 'host' ? 'host' : c.lan_mode === 'join' ? `join:${c.lan_addr.trim()}` : 'off',
    lan_name: null,
    season: seasonWord || null,
    tutorial: null,
    situation: null,
    ...extra,
  };
}

export async function start(extra) {
  try {
    const d = await duty(extra);
    const r = await call('launch', { duty: d });
    toast(r.others ? t('The game is starting ({n} already running)', { n: r.others }) : t('The game is starting…'), 'ok');
    pollNow();
    return r;
  } catch (e) {
    toast(e.message, 'error');
    return null;
  }
}

// --- polling: running games, installs, content changes ----------------------------------

let polling = false;
export async function pollNow() {
  if (polling) return;
  polling = true;
  try {
    const p = await call('poll');
    const changed = S.poll.stamp && p.stamp !== S.poll.stamp;
    set({ poll: p });
    for (const s of p.started || []) toast(t('Installing {name}…', { name: s }));
    if (changed) {
      loadContent();
      cached('mods', 'mods_status');
    }
  } catch {
    /* the next look */
  }
  polling = false;
}

function startPolling() {
  pollNow();
  setInterval(() => {
    followClock();
    // (faster while something happens)
    const busy = S.poll.jobs.some((j) => !['done', 'failed', 'cancelled'].includes(j.state));
    if (busy || Date.now() % 4000 < 1000) pollNow();
  }, 1000);
  on('games-ended', () => {
    pollNow();
    loadProfiles();
    loadSituations();
  });
}
