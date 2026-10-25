// The timetable editor: the map with every stop of the timetable on it, the lines and tours
// beside it as in Drive. A trip is made by clicking its stops on the map (only the stops the
// map has a way to are ringed, the route it would drive is drawn as they are chosen), a tour is
// the trips under it with their departure times, and a stop clicked says which trips call there.
// Saved as the line's .ttl and the trip's .ttp (in the content folder; OMSI 2's own files
// stay as they are).

import { html, useEffect, useMemo, useState } from '../../vendor/preact-htm.js';
import { S, useStore, set, toast } from '../store.js';
import { t } from '../i18n.js';
import { call } from '../api.js';
import { Icon } from '../icons.js';
import { Button, Input, Notice, Search, Select, Spinner } from '../ui.js';
import { loadLines } from '../data.js';
import { RouteMap, editorMap } from '../routemap.js';
import { depotRun } from './drive.js';

const NOT_IN_NAMES = /[/\\:*?"<>|\u0000-\u001f\u007f]/;

function fileNameOk(name) {
  const stem = name.split('.')[0].trimEnd().toUpperCase();
  return !!name && !NOT_IN_NAMES.test(name) && !/[. ]$/.test(name) && !/^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])$/.test(stem);
}

function fmtTime(min) {
  const s = Math.round(min * 60);
  const h = Math.floor(s / 3600);
  const m = String(Math.floor(s / 60) % 60).padStart(2, '0');
  return s % 60 === 0 ? `${h}:${m}` : `${h}:${m}:${String(s % 60).padStart(2, '0')}`;
}

function parseTime(text) {
  const parts = String(text).trim().split(/[:.]/);
  if (parts.length < 2 || parts.length > 3 || parts.some((p) => !/^\s*\d+\s*$/.test(p))) return null;
  const [h, m, s = 0] = parts.map(Number);
  return h < 48 && m < 60 && s < 60 ? h * 60 + m + s / 60 : null;
}

function nextNumber(tours) {
  const nums = tours.map((x) => String(x.number).trim()).filter((n) => /^-?\d+$/.test(n)).map(Number);
  return String((nums.length ? Math.max(...nums) : 0) + 1);
}

function repeatTour(tours, base, every, until) {
  const first = base.trips[0]?.departure ?? 0;
  let made = 0;
  for (let k = 1; first + every * k <= until + 1e-3 && made < 500; k++) {
    tours.push({ ...base, number: nextNumber(tours), trips: base.trips.map((x) => ({ ...x, departure: x.departure + every * k })) });
    made++;
  }
  return made;
}

const tt = () => S.tt;
/** The trip pointed at in a list is drawn on the map after the pointer has rested on it a moment:
 * sweeping down a long list asks the game for no route at all. */
let previewTimer = 0;
const previewLater = (name) => {
  clearTimeout(previewTimer);
  previewTimer = setTimeout(() => up({ preview: name }), 150);
};
const previewOff = () => {
  clearTimeout(previewTimer);
  if (tt().preview) up({ preview: null });
};
const up = (patch) => set({ tt: { ...S.tt, ...patch } });
const same = (a, b) => a.trim().toLowerCase() === b.trim().toLowerCase();
const byName = (a, b) => a.toLowerCase().localeCompare(b.toLowerCase());

/** What the page looks up in the timetable all the time: stop names, the trips calling at a stop,
 * the depots (the ends of the runs without passengers) and where a stop's links lead. */
function indexOf(data) {
  if (data._ix) return data._ix;
  const names = new Map(data.stops.map((s) => [s.id, s.name]));
  const calls = new Map();
  const depots = new Set();
  const regular = new Map();
  for (const x of data.trips) {
    for (const id of new Set(x.stops)) {
      if (!calls.has(id)) calls.set(id, []);
      calls.get(id).push(x);
    }
    if (x.stops.length > 1 && !depotRun(x)) for (const id of new Set(x.stops)) regular.set(id, (regular.get(id) || 0) + 1);
  }
  // a depot is the end of a run without passengers that no trip with passengers calls at (or
  // calls at least: a run from the depot to the first stop of a line ends where the line begins)
  for (const x of data.trips) {
    if (x.stops.length < 2 || !depotRun(x)) continue;
    const [a, b] = [x.stops[0], x.stops[x.stops.length - 1]];
    const [ra, rb] = [regular.get(a) || 0, regular.get(b) || 0];
    if (ra <= rb) depots.add(a);
    if (rb <= ra) depots.add(b);
  }
  const next = new Map();
  for (const [a, b, m] of data.links) {
    if (!next.has(a)) next.set(a, []);
    next.get(a).push([b, m]);
  }
  const trips = new Map(data.trips.map((x) => [x.name.toLowerCase(), x]));
  return (data._ix = { names, calls, depots, next, trips });
}

const stopName = (ix, id) => ix.names.get(id) || String(id);

/** The first and the last stop of a trip, as words. */
function ends(ix, x) {
  if (!x) return ['', ''];
  const a = x.stops.length ? stopName(ix, x.stops[0]) : '';
  return [a, x.terminus || (x.stops.length ? stopName(ix, x.stops[x.stops.length - 1]) : '')];
}

function suggestDeparture(tour) {
  const n = tour.trips.length;
  if (n === 0) return 360;
  if (n === 1) return tour.trips[0].departure + 30;
  const a = tour.trips[n - 2];
  const b = tour.trips[n - 1];
  return b.departure + Math.max(1, b.departure - a.departure);
}

async function read(map) {
  up({ map, loaded: map, data: null, error: null, line: null, tour: 0, dirty: [], ver: {}, drafts: {}, armed: false, sel: -1, build: null, stop: null, preview: null, picker: false });
  try {
    const data = await call('timetable', { map });
    if (tt().map === map) up({ data, line: data.lines[0]?.name ?? null });
  } catch (e) {
    if (tt().map === map) up({ error: e.message });
  }
}

/** The trips, stops and links as the files have them now, put under the lines being edited. */
async function refreshTrips() {
  const map = tt().map;
  const fresh = await call('timetable', { map });
  const s = tt();
  if (s.map !== map || !s.data) return;
  Object.assign(s.data, { trips: fresh.trips, stops: fresh.stops, links: fresh.links });
  delete s.data._ix;
  up({});
}

async function saveAll() {
  const s = tt();
  if (s.busy) return false;
  if (Object.values(s.drafts).some((d) => parseTime(d) == null)) {
    toast(t('A departure is not a time (h:mm).'), 'error');
    return false;
  }
  const lines = s.data.lines.filter((l) => s.dirty.includes(l.name));
  const sent = Object.fromEntries(lines.map((l) => [l.name, s.ver[l.name]]));
  up({ busy: true });
  try {
    const r = await call('timetable_save', { map: s.map, lines: JSON.parse(JSON.stringify(lines)) });
    const done = (n) => r.saved.includes(n) && tt().ver[n] === sent[n];
    for (const l of lines) if (done(l.name)) for (const x of l.tours) x.trips.sort((a, b) => a.departure - b.departure);
    const untouched = lines.every((l) => tt().ver[l.name] === sent[l.name]);
    up({ dirty: tt().dirty.filter((n) => !done(n)), ...(untouched ? { drafts: {} } : {}) });
    if (r.saved.length) {
      set({ lines: { ...S.lines, key: '' } });
      loadLines();
    }
    if (r.error) toast(t('Not saved: {e}', { e: r.error }), 'error');
    else toast(t('{n} line(s) saved', { n: r.saved.length }), 'ok');
    return !r.error;
  } catch (e) {
    toast(t('Not saved: {e}', { e: e.message }), 'error');
    return false;
  } finally {
    up({ busy: false });
  }
}

async function switchMap(map) {
  if (tt().busy || (tt().dirty.length && !(await saveAll()))) return;
  read(map);
}

async function reset() {
  if (tt().busy) return;
  if (!tt().armed) {
    up({ armed: true });
    toast(t('Press "Reset timetable" again to put the map\'s own timetable back (the changes made here are lost)'));
    return;
  }
  up({ armed: false, busy: true });
  try {
    const r = await call('timetable_reset', { map: tt().map });
    up({ busy: false });
    const map_folder = r.folder;
    if (r.removed) toast(t("The timetable of {map_folder} is the map's own again (the edited copy was removed)", { map_folder }), 'ok');
    else if (r.restored) toast(t('{n} line(s) of {map_folder} put back as they were', { n: r.restored, map_folder }), 'ok');
    else return toast(t('The timetable of {map_folder} has not been changed here', { map_folder }), 'error');
    set({ lines: { ...S.lines, key: '' } });
    loadLines();
    read(tt().map);
  } catch (e) {
    up({ busy: false });
    toast(t(e.message), 'error');
  }
}

function touch(line) {
  const s = tt();
  up({ ver: { ...s.ver, [line.name]: (s.ver[line.name] || 0) + 1 }, dirty: s.dirty.includes(line.name) ? s.dirty : [...s.dirty, line.name] });
}

function addLine() {
  const s = tt();
  const name = s.newLine.trim();
  if (!fileNameOk(name)) return toast(t('Type the new line\'s name (it becomes the file name, so no / \\ : * ? " < > |)'), 'error');
  if (s.data.lines.some((l) => same(l.name, name))) return toast(t('Line {name} is there already', { name }), 'error');
  s.data.lines.push({ name, user_allowed: true, priority: 0, tours: [] });
  s.data.lines.sort((a, b) => byName(a.name, b.name));
  up({ line: name, tour: 0, newLine: '', adding: false, drafts: {}, sel: -1, build: null });
  touch(s.data.lines.find((l) => l.name === name));
  toast(t('Line {name} added: give it tours, then save', { name }), 'ok');
}

// --- trips under a tour -------------------------------------------------------------------

function addTripTo(line, tour, name, at) {
  tour.trips.push({ trip: name, profile: 0, departure: at ?? suggestDeparture(tour) });
  touch(line);
  up({ sel: tour.trips.length - 1, drafts: {}, picker: false, stop: null });
}

function startBuild(line, from) {
  up({ build: { stops: from ? [from] : [], name: '', named: false, line: line?.name || '', to: '', edit: null }, sel: -1, stop: null, picker: false, preview: null });
}

/** The name a made trip gets unless it is typed: "line first - last". */
function autoName(ix, b) {
  if (b.stops.length < 2) return '';
  const raw = `${b.line ? b.line + ' ' : ''}${stopName(ix, b.stops[0])} - ${stopName(ix, b.stops[b.stops.length - 1])}`;
  return raw.replace(/[/\\:*?"<>|\u0000-\u001f\u007f]/g, ' ').replace(/\s+/g, ' ').replace(/[. ]+$/, '').trim();
}

function pickStop(id, line, tour) {
  const s = tt();
  const ix = indexOf(s.data);
  const b = s.build;
  if (!b) return up({ stop: s.stop === id ? null : id, sel: -1 });
  const last = b.stops[b.stops.length - 1];
  if (last === id) return;
  if (last !== undefined && !(ix.next.get(last) || []).some(([to]) => to === id)) {
    return toast(t('The map has no way from {a} to {b}: click a ringed stop.', { a: stopName(ix, last), b: stopName(ix, id) }), 'error');
  }
  up({ build: { ...b, stops: [...b.stops, id] } });
}

function removeBuildStop(k) {
  const b = tt().build;
  const ix = indexOf(tt().data);
  const stops = b.stops.filter((_, i) => i !== k);
  for (let i = 0; i + 1 < stops.length; i++) {
    if (!(ix.next.get(stops[i]) || []).some(([to]) => to === stops[i + 1])) return toast(t('The map has no way from {a} to {b}: take the stops off from the end.', { a: stopName(ix, stops[i]), b: stopName(ix, stops[i + 1]) }), 'error');
  }
  up({ build: { ...b, stops } });
}

async function saveBuild(line, tour, addToTour) {
  const s = tt();
  const b = s.build;
  const ix = indexOf(s.data);
  const name = (b.named ? b.name : autoName(ix, b)).trim();
  if (!fileNameOk(name)) return toast(t('Type the trip\'s name (it becomes the file name, so no / \\ : * ? " < > |)'), 'error');
  if (b.stops.length < 2) return toast(t('A trip needs two stops at least.'), 'error');
  if (ix.trips.has(name.toLowerCase())) return toast(t('A trip named {name} is there already', { name }), 'error');
  up({ busy: true });
  try {
    await call('timetable_trip_save', { map: s.map, trip: { name, line: b.line, terminus: b.to, stops: b.stops } });
    await refreshTrips();
    up({ build: null, busy: false });
    toast(t('Trip {name} saved', { name }), 'ok');
    if (addToTour && line && tour) addTripTo(line, tour, name);
  } catch (e) {
    up({ busy: false });
    toast(t('Not saved: {e}', { e: e.message }), 'error');
  }
}

// --- the page's parts ---------------------------------------------------------------------

function LinesPanel({ s, line }) {
  const lines = s.data?.lines || [];
  const [q, setQ] = useState('');
  const lq = q.trim().toLowerCase();
  const found = lines.filter((l) => !lq || l.name.toLowerCase().includes(lq));
  return html`<div class="tte-block">
    <div class="row duty-head"><span class="section-title grow">${t('Line')}</span><span class="muted small">${lines.length}</span></div>
    ${lines.length > 7 && html`<${Search} value=${q} onInput=${setQ} placeholder="Filter lines..." />`}
    <div class="pick-list tte-lines">
      ${found.length === 0 && html`<p class="muted small" style="padding:8px">${lines.length ? t('No line matches.') : t('This map has no lines yet.')}</p>`}
      ${found.map((l) => html`<button type="button" class=${'line-row' + (l.name === s.line ? ' on' : '')} onClick=${() => up({ line: l.name, tour: 0, drafts: {}, sel: -1, build: null, stop: null })}>
        <span class="line-badge ellipsis">${l.name}</span>
        <span class="grow"></span>
        ${s.dirty.includes(l.name) && html`<span class="dot accent" title=${t('Not saved')}></span>`}
        <span class="muted small">${t('{n} tours', { n: l.tours.length })}</span>
      </button>`)}
    </div>
    ${s.adding
      ? html`<div class="row">
          <div class="grow"><${Input} value=${s.newLine} placeholder="Name of the new line" autofocus onInput=${(v) => up({ newLine: v })} onKeyDown=${(e) => (e.key === 'Enter' ? addLine() : e.key === 'Escape' && up({ adding: false, newLine: '' }))} /></div>
          <${Button} kind="primary" icon="check" title="Add the line" onClick=${addLine} />
          <${Button} icon="x" title="Cancel" onClick=${() => up({ adding: false, newLine: '' })} />
        </div>`
      : html`<${Button} wide icon="plus" label="New line" onClick=${() => up({ adding: true })} />`}
  </div>`;
}

function ToursPanel({ s, line }) {
  const newTour = () => {
    const last = line.tours[line.tours.length - 1];
    line.tours.push({ number: nextNumber(line.tours), ai_group: last?.ai_group ?? 'Busses', extra: last?.extra ?? '', trips: [] });
    up({ tour: line.tours.length - 1, drafts: {}, sel: -1 });
    touch(line);
  };
  return html`<div class="tte-block grow">
    <div class="row duty-head"><span class="section-title grow">${t('Tour')}</span><${Button} small icon="plus" label="New tour" onClick=${newTour} /></div>
    <div class="pick-list tte-tours">
      ${line.tours.length === 0 && html`<p class="muted small" style="padding:8px">${t('No tours yet: make one, then add trips to it.')}</p>`}
      ${line.tours.map((x, i) => {
        const first = x.trips[0];
        const last = x.trips[x.trips.length - 1];
        return html`<button type="button" class=${'tour-row' + (i === s.tour ? ' on' : '')} onClick=${() => up({ tour: i, drafts: {}, sel: -1, build: null, stop: null })}>
          <span class="row"><b class="grow ellipsis">${x.number}</b>${first && html`<span class="num small muted">${fmtTime(first.departure)}${last && last !== first ? ` - ${fmtTime(last.departure)}` : ''}</span>`}</span>
          <span class="sub ellipsis">${t('{n} trips', { n: x.trips.length })}${x.ai_group ? ` - ${x.ai_group}` : ''}</span>
        </button>`;
      })}
    </div>
  </div>`;
}

/** What a tour can be given as a whole: a copy later in the day, a copy every so many minutes,
 * or nothing at all. */
function TourTools({ s, line, tour }) {
  const [open, setOpen] = useState(false);
  const off = s.offset.trim() === '' ? NaN : Number(s.offset);
  const until = parseTime(s.until);
  const pick = (i) => up({ tour: i, drafts: {}, sel: -1 });
  const copy = () => {
    line.tours.push({ ...tour, number: nextNumber(line.tours), trips: tour.trips.map((x) => ({ ...x, departure: Math.max(0, x.departure + off) })) });
    pick(line.tours.length - 1);
    touch(line);
    setOpen(false);
  };
  const repeat = () => {
    const made = repeatTour(line.tours, tour, off, until);
    if (made > 0) {
      pick(line.tours.length - 1);
      touch(line);
    }
    toast(t('{made} tour(s) made, every {every} min up to {until}', { made, every: off, until: fmtTime(until) }), 'ok');
    setOpen(false);
  };
  const remove = () => {
    line.tours.splice(s.tour, 1);
    pick(Math.max(0, s.tour - 1));
    touch(line);
  };
  return html`<div class="menu-box">
    <${Button} small kind=${open ? 'primary' : ''} icon="copy" label="Copies" title="Copy the tour later in the day, or every few minutes" onClick=${() => setOpen(!open)} />
    ${open && html`<div class="menu tte-menu" style="bottom:auto;top:calc(100% + 6px)">
      <div class="tte-tool">
        <b>${t('Copy this tour later')}</b>
        <div class="tt-pair"><${Input} type="number" value=${s.offset} placeholder="minutes" onInput=${(v) => up({ offset: v })} /><${Button} icon="copy" label="Copy +min" disabled=${!tour.trips.length || !Number.isFinite(off)} onClick=${copy} /></div>
      </div>
      <div class="tte-tool">
        <b>${t('Repeat it every few minutes')}</b>
        <div class="tt-pair"><${Input} class=${'input' + (until == null ? ' bad' : '')} value=${s.until} placeholder="until h:mm" onInput=${(v) => up({ until: v })} /><${Button} icon="refresh" label="Repeat" disabled=${!tour.trips.length || !(off >= 1) || until == null} onClick=${repeat} /></div>
        <span class="muted small">${t('every {n} min (the number above), up to the time', { n: Number.isFinite(off) ? off : '?' })}</span>
      </div>
    </div>`}
  </div>
  <${Button} small kind="ghost" icon="trash" title="Delete this tour" onClick=${remove} />`;
}

function TripRow({ s, ix, x, i, line, tour }) {
  const info = ix.trips.get(x.trip.toLowerCase());
  const [from, to] = ends(ix, info);
  const draft = s.drafts[`${line.name}|${s.tour}|${i}`];
  const key = `${line.name}|${s.tour}|${i}`;
  const depart = (v) => {
    const m = parseTime(v);
    if (m != null) x.departure = m;
    up({ drafts: { ...tt().drafts, [key]: v } });
    touch(line);
  };
  const step = (d) => {
    x.departure = Math.max(0, x.departure + d);
    const { [key]: _, ...drafts } = tt().drafts;
    up({ drafts });
    touch(line);
  };
  const profiles = info?.profiles.length > 1 ? info.profiles.map((p, k) => [k, p]) : null;
  return html`<div class=${'tte-trip' + (i === s.sel ? ' on' : '')} onClick=${() => up({ sel: i, build: null, stop: null })}>
    <div class="tte-time" onClick=${(e) => e.stopPropagation()}>
      <input class=${'input tnum' + (draft != null && parseTime(draft) == null ? ' bad' : '')} value=${draft ?? fmtTime(x.departure)} placeholder="h:mm" onInput=${(e) => depart(e.currentTarget.value)} spellcheck="false" />
      <div class="tte-step"><button type="button" title=${t('1 minute later')} onClick=${() => step(1)}>+</button><button type="button" title=${t('1 minute earlier')} onClick=${() => step(-1)}>-</button></div>
    </div>
    <div class="grow tte-trip-text">
      <span class="tte-trip-line">${info?.line ? html`<span class="line-badge">${info.line}</span>` : html`<span class="muted small">${t('Depot run')}</span>`}<small class="muted ellipsis" title=${x.trip}>${x.trip}</small></span>
      ${info ? html`<span class="trip-ends stacked" title=${`${from} → ${to}`}><span class="from">${from}</span><span class="to">${to}</span></span>` : html`<span class="ellipsis danger-fg">${t('The map has no trip {name}', { name: x.trip })}</span>`}
      ${profiles && html`<span class="tte-profile" onClick=${(e) => e.stopPropagation()}><${Select} raw value=${x.profile} options=${profiles} onChange=${(v) => ((x.profile = Number(v)), touch(line))} /></span>`}
    </div>
    <div class="tte-trip-tools" onClick=${(e) => e.stopPropagation()}>
      ${info && html`<${Button} small kind="ghost" icon="edit" title="Change this trip's stops (saved as a new trip)" onClick=${() => up({ build: { stops: [...info.stops], name: `${x.trip} 2`, named: true, line: info.line, to: info.terminus, edit: x.trip }, sel: -1, stop: null })} />`}
      <${Button} small kind="ghost" icon="x" title="Take this trip out of the tour" onClick=${() => (tour.trips.splice(i, 1), up({ drafts: {}, sel: -1 }), touch(line))} />
    </div>
  </div>`;
}

function TripPicker({ s, ix, line, tour }) {
  const [q, setQ] = useState('');
  const lq = q.trim().toLowerCase();
  const all = useMemo(() => {
    const mine = s.data.trips.filter((x) => same(x.line, line.name));
    return (mine.length ? [...mine, ...s.data.trips.filter((x) => !mine.includes(x))] : s.data.trips).slice().sort((a, b) => (same(b.line, line.name) - same(a.line, line.name)) || byName(a.name, b.name));
  }, [s.data, line.name]);
  const found = all.filter((x) => !lq || `${x.name} ${x.line} ${x.terminus} ${x.stops.map((id) => stopName(ix, id)).join(' ')}`.toLowerCase().includes(lq)).slice(0, 60);
  return html`<div class="tte-pick">
    <${Search} value=${q} onInput=${setQ} autofocus placeholder="Trip, line or a stop it calls at..." />
    <div class="pick-list tte-pick-list" onPointerLeave=${previewOff}>
      ${found.length === 0 && html`<p class="muted small" style="padding:8px">${t('No trip matches.')}</p>`}
      ${found.map((x) => {
        const [from, to] = ends(ix, x);
        return html`<button type="button" class="tour-row" onPointerEnter=${() => previewLater(x.name)} onClick=${() => addTripTo(line, tour, x.name)}>
          <span class="row"><span class="grow ellipsis">${from} <span class="muted">→</span> ${to}</span>${x.line && html`<span class="line-badge">${x.line}</span>`}</span>
          <span class="sub ellipsis">${x.name} - ${t('{n} stops', { n: x.stops.length })}${x.minutes ? ` - ${Math.round(x.minutes)} min` : ''}</span>
        </button>`;
      })}
    </div>
  </div>`;
}

function TourPanel({ s, ix, line, tour }) {
  const shift = (d) => {
    for (const x of tour.trips) x.departure = Math.max(0, x.departure + d);
    up({ drafts: {} });
    touch(line);
  };
  return html`<div class="tte-side-body">
    <div class="tte-title">
      <div class="grow">
        <div class="section-title">${t('Tour')}</div>
        <b class="ellipsis">${tour.number}</b><span class="muted small"> ${tour.ai_group || ''}</span>
      </div>
      <${TourTools} s=${s} line=${line} tour=${tour} />
    </div>
    <div class="row wrap tte-shift">
      <${Button} small label="-1 min" disabled=${!tour.trips.length} onClick=${() => shift(-1)} />
      <${Button} small label="+1 min" disabled=${!tour.trips.length} onClick=${() => shift(1)} />
      <span class="muted small">${t('moves the whole tour')}</span>
    </div>
    <div class="tte-trips">
      ${tour.trips.length === 0 && html`<p class="muted small" style="padding:8px 2px">${t('No trips yet. Add one that exists, or click the stops of a new one on the map.')}</p>`}
      ${tour.trips.map((x, i) => html`<${TripRow} key=${`${line.name}|${s.tour}|${i}|${x.trip}`} s=${s} ix=${ix} x=${x} i=${i} line=${line} tour=${tour} />`)}
    </div>
    ${s.picker ? html`<${TripPicker} s=${s} ix=${ix} line=${line} tour=${tour} />` : ''}
    <div class="tte-add">
      <${Button} icon=${s.picker ? 'x' : 'plus'} kind=${s.picker ? '' : 'primary'} label=${s.picker ? 'Close the list' : 'Add a trip'} onClick=${() => up({ picker: !s.picker, preview: null })} />
      <${Button} icon="route" label="New trip on the map" onClick=${() => startBuild(line)} />
    </div>
  </div>`;
}

function BuildPanel({ s, ix, line, tour }) {
  const b = s.build;
  const [add, setAdd] = useState(true);
  const set_ = (patch) => up({ build: { ...b, ...patch } });
  const name = b.named ? b.name : autoName(ix, b);
  const ok = b.stops.length >= 2 && fileNameOk(name.trim()) && !ix.trips.has(name.trim().toLowerCase());
  const last = b.stops[b.stops.length - 1];
  const reach = last !== undefined ? (ix.next.get(last) || []).length : 0;
  let metres = 0;
  for (let i = 0; i + 1 < b.stops.length; i++) metres += (ix.next.get(b.stops[i]) || []).find(([to]) => to === b.stops[i + 1])?.[1] || 0;
  return html`<div class="tte-side-body">
    <div class="tte-title">
      <div class="grow"><div class="section-title">${b.edit ? t('Trip {name}, changed', { name: b.edit }) : t('New trip')}</div><b class="ellipsis">${b.stops.length ? t(b.stops.length === 1 ? '{n} stop' : '{n} stops', { n: b.stops.length }) : t('Click the first stop')}</b>${metres > 0 && html`<span class="muted small"> ${(metres / 1000).toFixed(1)} km</span>`}</div>
      <${Button} small kind="ghost" icon="x" title="Cancel the new trip" onClick=${() => up({ build: null })} />
    </div>
    <p class="muted small">${b.stops.length === 0 ? t('Click a stop on the map, or search for one by name. Depots are the amber squares.') : reach ? t('Click the next stop: the ringed ones are the stops the map has a way to from {name}.', { name: stopName(ix, last) }) : t('The map has no way on from {name}: this is where the trip ends.', { name: stopName(ix, last) })}</p>
    <ol class="tte-stops">
      ${b.stops.map((id, k) => html`<li class=${k === 0 || k === b.stops.length - 1 ? 'end' : ''}>
        <span class="n">${k + 1}</span><span class="grow ellipsis">${stopName(ix, id)}</span>
        <${Button} small kind="ghost" icon="x" title="Take this stop off" onClick=${() => removeBuildStop(k)} />
      </li>`)}
    </ol>
    ${b.stops.length >= 2 && html`<div class="stack" style="gap:10px">
      <div class="tt-fields">
        <label class="field"><span>${t('Line shown')}</span><${Input} value=${b.line} placeholder="empty: a depot run" onInput=${(v) => set_({ line: v })} /></label>
        <label class="field"><span>${t('Destination')}</span><${Input} value=${b.to} placeholder=${stopName(ix, last)} onInput=${(v) => set_({ to: v })} /></label>
      </div>
      <label class="field"><span>${t('Name of the trip (its file)')}</span><${Input} class=${'input' + (ok ? '' : ' bad')} value=${name} onInput=${(v) => set_({ name: v, named: true })} /></label>
      ${ix.trips.has(name.trim().toLowerCase()) && html`<${Notice} kind="warn">${t('A trip named {name} is there already', { name })}</${Notice}>`}
      ${tour && html`<label class="toggle"><input type="checkbox" checked=${add} onChange=${(e) => setAdd(e.currentTarget.checked)} /><span class="text"><b>${t('Add it to tour {n}', { n: tour.number })}</b></span></label>`}
    </div>`}
    <div class="tte-add">
      <${Button} kind="primary" icon="save" label="Save the trip" disabled=${!ok || s.busy} onClick=${() => saveBuild(line, tour, add)} />
      <${Button} icon="left" label="Take the last stop off" disabled=${!b.stops.length} onClick=${() => set_({ stops: b.stops.slice(0, -1) })} />
    </div>
  </div>`;
}

/** A stop clicked on the map: where it is, and the trips that call there. */
function StopCard({ s, ix, id, line, tour, close }) {
  const trips = (ix.calls.get(id) || []).slice().sort((a, b) => (same(b.line, line?.name || '') - same(a.line, line?.name || '')) || byName(a.name, b.name));
  return html`<div class="tte-float tte-stop">
    <div class="row"><b class="grow ellipsis">${stopName(ix, id)}${ix.calls.get(id)?.find((x) => x.line) ? html` <span class="muted small">→ ${ix.calls.get(id).find((x) => x.line).terminus}</span>` : ''}</b>${ix.depots.has(id) && html`<span class="tag">${t('Depot')}</span>`}<${Button} small kind="ghost" icon="x" title="Close" onClick=${close} /></div>
    <div class="row wrap"><${Button} small icon="route" label="Start a new trip here" onClick=${() => startBuild(line, id)} /></div>
    <div class="section-title" style="margin:6px 0 4px">${trips.length ? t('{n} trips call here', { n: trips.length }) : t('No trip calls here yet')}</div>
    <div class="tte-stop-list" onPointerLeave=${previewOff}>
      ${trips.slice(0, 40).map((x) => {
        const [from, to] = ends(ix, x);
        return html`<div class="tte-stop-trip" onPointerEnter=${() => previewLater(x.name)}>
          ${x.line ? html`<span class="line-badge">${x.line}</span>` : html`<span class="muted small">${t('Depot run')}</span>`}
          <span class="grow ellipsis" title=${x.name}>${from} <span class="muted">→</span> ${to}</span>
          ${tour && html`<${Button} small icon="plus" title="Add this trip to the tour" onClick=${() => addTripTo(line, tour, x.name)} />`}
        </div>`;
      })}
    </div>
  </div>`;
}

/** Finds a stop by its name, flies the map there and picks it. */
function StopSearch({ s, ix, pos, line, tour }) {
  const [q, setQ] = useState('');
  const lq = q.trim().toLowerCase();
  const found = lq ? s.data.stops.filter((x) => x.name.toLowerCase().includes(lq)).slice(0, 8) : [];
  const go = (st) => {
    const at = pos.get(st.id);
    up({ focus: at ? { x: at[0], y: at[1], mpp: 1.6, n: Date.now() } : s.focus });
    pickStop(st.id, line, tour);
    setQ('');
  };
  return html`<div class="tte-float tte-search">
    <${Search} value=${q} onInput=${setQ} placeholder="Find a stop..." />
    ${found.length > 0 && html`<div class="tte-found">
      ${found.map((st) => {
        // (several stops of a name - the two sides of a street: told apart by where the buses go)
        const to = ix.calls.get(st.id)?.find((x) => x.line)?.terminus;
        return html`<button type="button" onClick=${() => go(st)}><${Icon} name=${ix.depots.has(st.id) ? 'garage' : 'stop'} size=${15} /><span class="ellipsis grow">${st.name}${to ? html` <span class="muted small">→ ${to}</span>` : ''}</span></button>`;
      })}
    </div>`}
  </div>`;
}

function SaveBar({ s, maps }) {
  const n = s.dirty.length;
  return html`<div class="tte-float tte-save">
    <${Select} raw disabled=${s.busy} value=${s.map} options=${maps.map((m) => [m.file, m.friendly || m.name])} onChange=${switchMap} />
    <${Button} small kind=${s.armed ? 'danger' : 'ghost'} icon="refresh" disabled=${s.busy} title="Put the map's own timetable back" label=${s.armed ? 'Press again to reset' : undefined} onClick=${reset} />
    <${Button} kind=${n ? 'primary' : ''} icon="save" disabled=${!n || s.busy} label=${n === 0 ? 'Saved' : n === 1 ? 'Save' : undefined} onClick=${saveAll}>${n > 1 ? t('Save all ({n} lines)', { n }) : null}</${Button}>
  </div>`;
}

export function TimetablePage() {
  const s = useStore();
  const maps = s.maps || [];
  if (!s.tt) S.tt = { map: null, loaded: null, data: null, error: null, line: null, tour: 0, dirty: [], ver: {}, drafts: {}, offset: '20', until: '22:00', newLine: '', adding: false, armed: false, busy: false, sel: -1, build: null, stop: null, preview: null, picker: false, focus: null };
  const v = s.tt;
  const want = v.map && (v.dirty.length || maps.some((m) => m.file === v.map)) ? v.map : maps.find((m) => m.file === s.choice?.map)?.file || maps[0]?.file;
  useEffect(() => {
    if (want && v.loaded !== want) read(want);
  }, [want, v.loaded]);
  // where the stops stand (the map's own answer, kept: the map shows the same one)
  const [pos, setPos] = useState(() => new Map());
  useEffect(() => {
    if (!v.map) return;
    let alive = true;
    editorMap(v.map, '').then((d) => alive && setPos(new Map((d.busstops || []).map((b) => [b.id, b.at]))));
    return () => (alive = false);
  }, [v.map]);
  // Escape leaves what is being made
  useEffect(() => {
    const key = (e) => e.key === 'Escape' && !e.target.closest?.('input, textarea') && up({ build: null, stop: null, picker: false });
    addEventListener('keydown', key);
    return () => removeEventListener('keydown', key);
  }, []);
  const line = v.data?.lines.find((l) => l.name === v.line);
  const tour = line?.tours[Math.min(v.tour, line.tours.length - 1)];
  if (line && tour && line.tours[v.tour] !== tour) v.tour = line.tours.indexOf(tour);
  const ix = v.data ? indexOf(v.data) : null;
  const picked = tour && v.sel >= 0 ? tour.trips[v.sel]?.trip : '';
  const b = v.build;
  // the route on the map: the trip being made, else the one pointed at, else the chosen one
  const route = b ? (b.stops.length >= 2 ? `@stops:${b.stops.join(',')}` : '') : v.preview || picked || '';
  const editor = useMemo(() => {
    if (!ix) return null;
    const chain = b ? b.stops : [];
    const last = chain[chain.length - 1];
    const reach = b && last !== undefined ? new Set((ix.next.get(last) || []).map(([to]) => to)) : null;
    return { chain, reach, depots: ix.depots, onStop: (id) => pickStop(id, line, tour), focus: v.focus, fit: !b };
  }, [ix, b, line, tour, v.focus]);
  if (!maps.length) return html`<div class="tte-empty"><${Notice}>${t('No maps found.')}</${Notice}></div>`;
  const stopId = v.stop;
  return html`<div class="tte">
    <aside class="tte-side card tte-left">
      ${!v.data && !v.error && html`<div class="row muted" style="padding:16px"><${Spinner} /> ${t('Reading the timetable…')}</div>`}
      ${v.error && html`<div style="padding:12px"><${Notice} kind="danger">${v.error}</${Notice}></div>`}
      ${v.data && html`<${LinesPanel} s=${v} line=${line} />${line ? html`<${ToursPanel} s=${v} line=${line} />` : ''}`}
    </aside>
    <div class="tte-map">
      ${v.map && html`<${RouteMap} map=${v.map} date="" trip=${route} height=${0} editor=${editor} />`}
      ${v.data && html`<${StopSearch} s=${v} ix=${ix} pos=${pos} line=${line} tour=${tour} />`}
      ${stopId != null && ix && !b && html`<${StopCard} s=${v} ix=${ix} id=${stopId} line=${line} tour=${tour} close=${() => up({ stop: null })} />`}
      <${SaveBar} s=${v} maps=${maps} />
      <div class="tte-legend"><span><i class="sq"></i>${t('Depot')}</span><span><i class="dot"></i>${t('Stop')}</span>${b && html`<span><i class="ring"></i>${t('Next stop')}</span>`}</div>
    </div>
    <aside class="tte-side card tte-right">
      ${v.data && b && html`<${BuildPanel} s=${v} ix=${ix} line=${line} tour=${tour} />`}
      ${v.data && !b && line && tour && html`<${TourPanel} s=${v} ix=${ix} line=${line} tour=${tour} />`}
      ${v.data && !b && line && !tour && html`<div class="tte-side-body"><p class="muted">${t('Make a tour on the left: it is the day of one bus, trip after trip.')}</p><div class="tte-add"><${Button} icon="route" label="New trip on the map" onClick=${() => startBuild(line)} /></div></div>`}
      ${v.data && !b && !line && html`<div class="tte-side-body"><p class="muted">${t('Choose a line, or make one.')}</p><div class="tte-add"><${Button} icon="route" label="New trip on the map" onClick=${() => startBuild(null)} /></div></div>`}
    </aside>
  </div>`;
}
