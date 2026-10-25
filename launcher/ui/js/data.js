// Loading the content and keeping the duty: what the pages share.

import { call, on, onPhone, inTauri } from './api.js';
import { S, set, toast, remembered } from './store.js';
import { setLanguage, t, hhmm } from './i18n.js';

/** The duty as the launcher remembers it (`launcher-duty.json`, shared with the game's own
 * launcher window: the same fields). */
export const CHOICE_DEFAULTS = {
  bus: '', paint: '', plate: '', number: '', hof: '', hof_manual: false,
  map: '', entry: -1, line: null, tour: null, free: false,
  time: 9 * 60, start_trip: null, date: '1989-05-30',
  season: 'auto', phase: 'mid', own_date: null,
  weather: '', traffic: 30, passengers: true, schedule: true, autostart: false, comfortable_cabin_on_start: false, on_foot: false,
  lan_mode: 'off', lan_addr: '', lan_ws_fallback: null, version: 2,
};

export async function boot() {
  const b = await call('bootstrap');
  await setLanguage(b.language);
  const choice = { ...CHOICE_DEFAULTS, ...(b.choice || {}) };
  // (a server joined in an earlier session is not joined again by itself)
  if (choice.lan_mode === 'join') Object.assign(choice, { lan_mode: 'off', lan_ws_fallback: null });
  if (choice.version < 2) Object.assign(choice, { entry: -1, version: 2 });
  Object.assign(choice, clockPatch(choice, b.settings));
  set({ boot: b, settings: b.settings, config: b.config, choice, favourites: b.favourites || [], ready: true, page: b.root ? 'drive' : 'setup' });
  if (b.crash) set({ crash: b.crash });
  applyTheme();
  if (b.root) {
    loadContent();
    readAhead();
    loadDepot();
    // (the bus preview's helper and renderer get ready while the page is read)
    call('preview_warm').catch(() => {});
  }
  startPolling();
  on('vehicles-progress', (p) => set({ vehiclesProgress: p }));
  if (b.updated) toast(t('openOMSI {version} is installed', { version: b.updated }), 'ok', 8000);
}

export const LAUNCHER_SIZES = [['0.7', 'Smallest'], ['0.8', 'Very small'], ['0.9', 'Smaller'], ['1', 'Normal'], ['1.15', 'Larger'], ['1.3', 'Largest']];

export const launcherSize = () => remembered('launcher_size', '1');

export const LAUNCHER_LAYOUTS = [
  ['bigmap', 'Big Map: the map fills the page'],
  ['control', 'Control Room: everything side by side'],
  ['classicplus', 'Classic+: two tabs, time and weather on top'],
  ['classic', 'Classic: in steps, as before'],
];

export const launcherLayout = () => {
  const l = remembered('launcher_layout', 'bigmap');
  return LAUNCHER_LAYOUTS.some(([id]) => id === l) ? l : 'bigmap';
};

export function fitWindow() {
  if (inTauri) call('fit_zoom', { size: Number(launcherSize()) || 1 }).catch(() => {});
}

export function applyTheme() {
  const pick = localStorage.getItem('theme') || 'dark';
  const light = pick === 'light' || (pick === 'system' && matchMedia('(prefers-color-scheme: light)').matches);
  document.documentElement.dataset.theme = light ? 'light' : 'dark';
  document.documentElement.dataset.layout = launcherLayout();
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

export const weatherOfItsOwn = (w) => /^(cycle$|tomorrow$|metar:|custom:)/i.test(w || '');

export const weatherName = (w, list) =>
  /^metar:/i.test(w || '') ? `${t('Current weather')}${w.length > 6 ? ` (${w.slice(6)})` : ''}`
  : /^tomorrow$/i.test(w || '') ? 'Tomorrow.io'
  : /^cycle$/i.test(w || '') ? t('Weather cycle')
  : /^custom:/i.test(w || '') ? t('Custom weather')
  : list?.find((x) => x.file === w)?.name || t('Natural weather');

/** A remembered map or bus that is not installed any more gives way to the first one. */
export function fixChoice() {
  const c = { ...S.choice };
  let mapChanged = false;
  if (S.maps?.length && !S.maps.some((m) => m.file === c.map)) {
    c.map = (S.maps.find((m) => /grundorf/i.test(m.file)) || S.maps[0]).file;
    c.entry = -1;
    mapChanged = true;
  }
  if (S.vehicles?.length && !S.vehicles.some((v) => v.file === c.bus)) {
    c.bus = S.vehicles[0].file;
    c.paint = '';
    c.number = randomFleetNumber(S.vehicles[0]);
  }
  if (S.weather && c.weather && !weatherOfItsOwn(c.weather) && !S.weather.some((w) => w.file === c.weather)) c.weather = '';
  // (a date the player picked stays even outside the map's [years]; only a map that had
  // to give way to another one takes a date the new map allows)
  if (mapChanged) c.date = fitDateToMap(c.date, c.map);
  Object.assign(c, allowedBus(c));
  set({ choice: c });
}

const norm = (f) => (f || '').replace(/\\/g, '/').toLowerCase();

export const busFile = (f) => {
  const n = norm(f);
  const at = n.lastIndexOf('vehicles/');
  return at > 0 ? n.slice(at) : n;
};

export function serverBuses(c = S.choice) {
  return c?.lan_mode === 'join' && c.lan_server?.vehicles?.length ? new Set(c.lan_server.vehicles.map(busFile)) : null;
}

export function allowedBus(c = S.choice) {
  const allowed = serverBuses(c);
  if (!allowed || allowed.has(busFile(c.bus))) return {};
  const own = S.vehicles?.find((v) => allowed.has(busFile(v.file)));
  return own ? { bus: own.file, paint: '', number: '', hof_manual: false } : {};
}

export const noServerBus = () => {
  const allowed = serverBuses();
  return !!allowed && !!S.vehicles && !S.vehicles.some((v) => allowed.has(busFile(v.file)));
};

export const serverJoined = (answer) => ({
  lan_server: { asked: !!answer, name: answer?.name || '', time: answer?.time || '', weather: answer?.weather || '', vehicles: answer?.vehicles || [] },
});

export async function askJoined() {
  const c = S.choice;
  if (c?.lan_mode !== 'join' || c.lan_server?.asked) return;
  const r = await call('server_query', { address: c.lan_addr.trim() }).catch(() => null);
  if (!r || S.choice.lan_mode !== 'join' || S.choice.lan_addr !== c.lan_addr) return;
  choose({ ...serverJoined({ ...r, name: c.lan_server?.name || r.name }), lan_map: c.lan_map || r.map || '' });
  choose(allowedBus());
}

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

/** Transport modes (bus, tram, railway, metro, trolleybus). A map that names none takes every
 * vehicle; otherwise the vehicle must share a mode with it. Mirrors `transport_refusal` of the
 * launcher core, so the pickers can say why before the game does. */
export const MODES = ['bus', 'tram', 'railway', 'metro', 'trolleybus'];
export const modeLabel = (m) => t({ bus: 'Bus', tram: 'Tram', railway: 'Railway', metro: 'Metro', trolleybus: 'Trolleybus' }[m] || m);
export function transportRefusal(map, vehicle) {
  const a = map?.transport || [];
  const b = vehicle?.transport || [];
  if (!a.length || a.some((m) => b.includes(m))) return null;
  return t('This map is made for {maps} only; the vehicle counts as {vehicle}.', { maps: a.map(modeLabel).join(', '), vehicle: b.length ? b.map(modeLabel).join(', ') : t('nothing known') });
}
/** Modes worth showing as a tag: everything but a plain bus, whose tag would be noise on the
 * thousands of stock buses. */
export const shownModes = (x) => (x?.transport || []).filter((m) => m !== 'bus' || (x.transport.length > 1));

export const curMap = () => S.maps?.find((m) => m.file === S.choice?.map);
export const curBus = () => S.vehicles?.find((v) => v.file === S.choice?.bus);
export const curLine = () => S.lines.list?.find((l) => l.name === S.choice?.line);
// Old saved choices use the displayed number; new choices identify the variant.
export const tourId = (tour) => tour?.id ?? tour?.number;
export const resolveTour = (line, selection) => line?.tours.find((x) => tourId(x) === selection) || line?.tours.find((x) => x.number === selection && x.runs) || line?.tours.find((x) => x.number === selection);
export const curTour = () => resolveTour(curLine(), S.choice?.tour);
export const realTime = () => !!S.settings?.use_real_time;
export const realDate = () => !!S.settings?.use_real_date;

/** `date` (YYYY-MM-DD) fitted to the year span of the map `file` (its `[years]`), as OMSI 2
 * starts a map on a day it allows: the same day in the nearest allowed year, 29 February the
 * 28th where that year has none. Inside the span, or not a date: kept. The launcher core
 * fits the game's date the same way (`fit_date_to_years`). */
export function fitDateToMap(date, file) {
  const span = S.maps?.find((m) => m.file === file)?.year_span;
  const parts = /^(\d+)-(\d+)-(\d+)$/.exec((date || '').trim());
  if (!span || !parts) return date;
  const [, y, mm, dd] = parts;
  const was = Number(y);
  const year = was < span[0] ? span[0] : was > span[1] ? span[1] : was;
  if (year === was) return date;
  const leap = (year % 4 === 0 && year % 100 !== 0) || year % 400 === 0;
  const day = mm === '02' && dd === '29' && !leap ? '28' : dd;
  return `${String(year).padStart(4, '0')}-${mm}-${day}`;
}

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
    // (the computer's date is fitted to the map's [years]; a date the player chose is kept,
    // see `choose`)
    const d = fitDateToMap(`${String(year).padStart(4, '0')}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`, c.map);
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
function randomFleetNumber(vehicle) {
  const numbers = vehicle?.numbers || [];
  return numbers.length ? numbers[Math.floor(Math.random() * numbers.length)][0] : '';
}

/** Change the duty (and keep it on disk a moment later). */
export function choose(patch) {
  const before = S.choice;
  const c = { ...before, ...patch };
  // (a map's [years] only move the date when another map is chosen: a date the player
  // picked or typed stays as it is, even outside them)
  if (c.map !== before.map) c.date = fitDateToMap(c.date, c.map);
  if (c.bus !== before.bus && !patch.number) {
    c.number = randomFleetNumber(S.vehicles?.find((v) => v.file === c.bus));
  }
  // (another bus does not carry on with the depot bus's plate)
  if (c.bus !== before.bus && !('plate' in patch) && depotBusOf(before)) c.plate = '';
  set({ choice: c });
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => call('choice_save', { choice: S.choice }).catch(() => {}), 400);
  if (c.map !== before.map || c.date !== before.date) {
    loadLines();
  }
  if (c.map !== before.map) loadSituations();
  if (c.map !== before.map || c.bus !== before.bus || c.date !== before.date || c.line !== before.line || c.free !== before.free) updateHof();
  else if (c.hof !== before.hof || c.hof_manual !== before.hof_manual) loadIbis();
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
    const tour = resolveTour(line, S.choice.tour) || line?.tours.find((x) => x.runs) || line?.tours[0] || null;
    if (line?.name !== S.choice.line || tourId(tour) !== S.choice.tour) choose({ line: line?.name ?? null, tour: tourId(tour) ?? null });
  } catch (e) {
    if (S.lines.key === key) set({ lines: { key, list: [], loading: false, error: e.message } });
  }
}

export async function updateHof() {
  const c = S.choice;
  const hof = await call('default_hof', { map: curMap() || null, bus: curBus() || null, date: c.date, line: c.free ? null : c.line }).catch(() => '');
  set({ hof });
  loadIbis();
}

export const dutyHof = () => (S.choice.hof_manual ? S.choice.hof : S.hof) || '';

let ibisAsked = '';
let ibisTrip = null;
export async function loadIbis(trip = ibisTrip) {
  if (onPhone) return;
  ibisTrip = trip;
  const c = S.choice;
  const hof = /\.(bus|ovh)/i.test(dutyHof()) ? '' : dutyHof();
  const base = c.free || !c.line || !c.bus ? '' : `${c.bus}|${hof}|${c.line}`;
  const stops = trip?.stops.map((x) => x.name) || [];
  const key = base && `${base}|${trip?.terminus || ''}|${stops.join('|')}`;
  if (key === ibisAsked) return;
  if (!key || !ibisAsked.startsWith(`${base}|`)) set({ ibis: null });
  ibisAsked = key;
  if (!key) return;
  const ibis = await call('ibis', { bus: c.bus, hof, line: c.line, terminus: trip?.terminus || '', stops }).then((info) => ({ info }), (e) => ({ error: e.message || String(e) }));
  if (ibisAsked !== key) return;
  if ('error' in ibis) ibisAsked = '';
  set({ ibis });
}

export async function loadDepot() {
  try {
    const d = await call('depot_get');
    set({ depot: d.buses, depotGroups: d.groups, errors: { ...S.errors, depot: null } });
  } catch (e) {
    set({ depot: [], errors: { ...S.errors, depot: e.message } });
  }
}

/** The depot bus the duty drives: its bus, livery, plate and fleet number all as they are. */
export function depotBusOf(c = S.choice) {
  const plate = (c?.plate || '').trim().toUpperCase();
  return c && plate ? S.depot?.find((b) => busFile(b.bus) === busFile(c.bus) && (b.paint || '') === (c.paint || '') && b.plate === plate && (b.number || '') === (c.number || '').trim()) : null;
}

/** Take a depot bus for the next duty. */
export function driveDepotBus(b) {
  const v = S.vehicles?.find((x) => busFile(x.file) === busFile(b.bus));
  if (!v) return;
  choose({ bus: v.file, paint: b.paint, plate: b.plate, number: b.number, hof_manual: false });
  if (S.choice.number !== b.number) choose({ number: b.number });
  set({ page: 'drive' });
  toast(t('{name} is ready under Drive: choose the time and the duty.', { name: b.name }), 'ok');
}

export async function loadSituations() {
  if (!S.choice?.map) return;
  set({ situations: await call('saved_situations', { map: S.choice.map }).catch(() => []) });
}

/** Page data kept between visits: answered at once from what was read last, read again
 * behind it (`fresh` = the command's answer is set into `S.cache[key]`). */
export function cached(key, cmd, args) {
  const before = S.cache[key];
  const p = call(cmd, args).then(
    (v) => (S.cache[key] === before && set({ cache: { ...S.cache, [key]: v } }), v),
    (e) => (S.cache[key] === before && set({ cache: { ...S.cache, [key]: { error: e.message } } }), null),
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

let settingsSaved = Promise.resolve();
let settingsSeq = 0;

export function reloadSettings() {
  settingsSaved = settingsSaved.then(async () => {
    const mine = settingsSeq;
    const now = await call('settings_get').catch(() => null);
    if (!now || mine !== settingsSeq) return;
    const before = S.settings?.language;
    set({ settings: now });
    if (now.language !== before) await applyLanguage();
  });
  return settingsSaved;
}

async function applyLanguage() {
  const lang = S.boot.languages.find((l) => l.code === S.settings.language);
  await setLanguage(lang?.iso || 'en');
  // (the content's names and descriptions are read in the new language)
  loadContent();
}

export async function saveSettings(patch, whole = false) {
  set({ settings: whole ? patch : { ...S.settings, ...patch } });
  const mine = ++settingsSeq;
  let ok = true;
  settingsSaved = settingsSaved.then(async () => {
    try {
      const saved = await call(whole ? 'settings_save' : 'settings_patch', whole ? { settings: patch } : { patch });
      if (mine === settingsSeq) set({ settings: saved });
    } catch (e) {
      ok = false;
      toast(t('The settings could not be saved: {e}', { e: e.message }), 'error');
    }
  });
  await settingsSaved;
  if (!ok) return false;
  if (!whole && 'language' in patch) await applyLanguage();
  return true;
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
  const ask = () => Promise.race([call('server_query', { address: c.lan_addr.trim() }).catch(() => null), new Promise((r) => setTimeout(r, 3000))]);
  const joined = c.lan_mode === 'join' && !extra.tutorial && !extra.situation ? c.lan_map || (await ask())?.map : '';
  const hostMap = (joined || '').trim().replace(/\\/g, '/');
  const installed = S.maps?.find((m) => m.file.toLowerCase() === hostMap.toLowerCase())?.file;
  return {
    map: /maps\//i.test(hostMap) ? installed || hostMap : c.map,
    bus: c.bus,
    paint: c.paint || null,
    plate: c.plate?.trim() || null,
    number: c.number?.trim() || null,
    hof: dutyHof() || null,
    entry: c.entry,
    line: c.free ? null : c.line,
    tour: c.free ? null : curTour()?.number ?? null,
    trip: trip != null ? String(trip) : null,
    whole_tour: trip != null,
    time: hhmm(c.time),
    date: c.date,
    weather: c.weather || null,
    traffic: Math.round(c.traffic),
    passengers: c.passengers,
    schedule: c.schedule,
    autostart: c.autostart,
    comfortable_cabin_on_start: c.comfortable_cabin_on_start,
    on_foot: c.on_foot,
    profile: S.config?.profile || null,
    lan: c.lan_mode === 'host' ? 'host' : c.lan_mode === 'join' ? `join:${c.lan_addr.trim()}` : 'off',
    lan_name: null,
    lan_ws_fallback: c.lan_mode === 'join' ? c.lan_ws_fallback || null : null,
    season: seasonWord || null,
    tutorial: null,
    situation: null,
    depot: depotBusOf(c)?.id || null,
    ...extra,
  };
}

export async function start(extra) {
  try {
    const d = await duty(extra);
    const r = await call('launch', { duty: d });
    set({ gameStarted: true });
    if (d.lan !== 'off' && !d.tutorial) set({ page: 'games' });
    toast(r.others ? t('The game is starting ({n} already running)', { n: r.others }) : t('The game is starting…'), 'ok');
    pollNow();
    return r;
  } catch (e) {
    toast(t(e.message), 'error');
    return null;
  }
}

// --- polling: running games, installs, content changes ----------------------------------

let polling = false;
export const jobRunning = (j) => !!j.state && !['done', 'failed', 'cancelled'].includes(j.state);

export async function installMod(path) {
  const mode = S.modMode || 'auto';
  set({ modPath: path, modInfo: null });
  call('mod_inspect', { path })
    .then((i) => {
      if (S.modPath !== path) return;
      set({ modInfo: i });
      if (!i.fits && i.in_place_ok && (S.modMode || 'auto') === 'extract') set({ modMode: 'inplace' });
    })
    .catch((e) => S.modPath === path && set({ modInfo: { error: e.message } }));
  try {
    await call('mod_install', { path, mode });
    toast(t('Installing {name}…', { name: path.split(/[\\/]/).pop() }));
    pollNow();
  } catch (e) {
    toast(t(e.message), 'error');
  }
}

export async function pollNow() {
  if (polling) return;
  polling = true;
  try {
    const p = await call('poll');
    const changed = S.poll.stamp && p.stamp !== S.poll.stamp;
    const before = S.poll.jobs || [];
    const same = (x) => JSON.stringify([x.stamp, x.jobs, x.instances, x.ended]);
    if (same(p) !== same(S.poll) || p.started?.length) set({ poll: p });
    for (const s of p.started || []) toast(t('Installing {name}…', { name: s }));
    const finished = S.poll.stamp ? (p.jobs || []).filter((j) => !jobRunning(j) && !before.some((b) => b.id === j.id && !jobRunning(b))) : [];
    for (const j of finished) {
      if (j.state === 'done') toast(j.message || t('{name} installed', { name: j.name }), 'ok');
      else if (j.state === 'failed') toast(`${j.name}: ${j.message}`, 'error');
    }
    if (changed || finished.length) {
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
    const busy = S.poll.jobs.some(jobRunning);
    const every = S.poll.instances?.length && !document.hasFocus() ? 20000 : 4000;
    if (busy || Date.now() % every < 1000) pollNow();
  }, 1000);
  on('games-ended', () => {
    set({ gameStarted: false });
    pollNow();
    loadProfiles();
    loadSituations();
    loadDepot();
    reloadSettings();
  });
  window.addEventListener('focus', () => {
    reloadSettings();
    pollNow();
  });
}
