import { html, useEffect } from '../../vendor/preact-htm.js';
import { S, useStore, set, toast } from '../store.js';
import { t } from '../i18n.js';
import { call } from '../api.js';
import { Button, Card, Empty, Input, Notice, PageHead, Select, Spinner } from '../ui.js';
import { loadLines } from '../data.js';

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
const up = (patch) => set({ tt: { ...S.tt, ...patch } });
const same = (a, b) => a.trim().toLowerCase() === b.trim().toLowerCase();

function tripChoices(data, line) {
  const named = data.trips.filter((x) => same(x.line, line.name)).map((x) => x.name);
  const names = named.length ? named : data.trips.map((x) => x.name);
  for (const tour of line.tours) for (const x of tour.trips) if (!names.some((n) => same(n, x.trip))) names.push(x.trip);
  return names.sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()));
}

async function read(map) {
  up({ map, loaded: map, data: null, error: null, line: null, tour: 0, dirty: [], ver: {}, drafts: {}, armed: false });
  try {
    const data = await call('timetable', { map });
    if (tt().map === map) up({ data, line: data.lines[0]?.name ?? null });
  } catch (e) {
    if (tt().map === map) up({ error: e.message });
  }
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
    toast(e.message, 'error');
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
  s.data.lines.sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
  up({ line: name, tour: 0, newLine: '', drafts: {} });
  touch(s.data.lines.find((l) => l.name === name));
  toast(t('Line {name} added: give it tours, then save', { name }), 'ok');
}

function LinesCard({ s, maps }) {
  const lines = s.data?.lines || [];
  return html`<${Card} title="Lines" class="tt-lines">
    <div class="stack" style="gap:12px">
      <div class="field"><label>${t('Map')}</label><${Select} raw disabled=${s.busy} value=${s.map} options=${maps.map((m) => [m.file, m.friendly || m.name])} onChange=${switchMap} /></div>
      ${s.error && html`<${Notice} kind="danger">${s.error}</${Notice}>`}
      ${!s.data && !s.error && html`<div class="row muted"><${Spinner} /></div>`}
      ${s.data && !lines.length && html`<p class="muted small">${t('This map has no timetable (no TTData lines).')}</p>`}
      ${lines.length > 0 && html`<div class="tt-list">
        ${lines.map((l) => html`<button class=${'list-item' + (l.name === s.line ? ' on' : '')} onClick=${() => up({ line: l.name, tour: 0, drafts: {} })}>
          <span class="grow ellipsis title">${l.name}</span>
          ${s.dirty.includes(l.name) && html`<span class="dot accent"></span>`}
          <span class="muted small tnum">${t('{n} tours', { n: l.tours.length })}</span>
        </button>`)}
      </div>`}
      ${s.data && html`<div class="row">
        <div class="grow"><${Input} value=${s.newLine} placeholder="New line (name)" onInput=${(v) => up({ newLine: v })} onKeyDown=${(e) => e.key === 'Enter' && addLine()} /></div>
        <${Button} icon="plus" label="Add" onClick=${addLine} />
      </div>
      <${Button} wide kind=${s.armed ? 'danger' : ''} icon="refresh" disabled=${s.busy} label=${s.armed ? 'Press again to reset' : 'Reset timetable'} onClick=${reset} />`}
    </div>
  </${Card}>`;
}

function ToursCard({ s, line, choices }) {
  const off = s.offset.trim() === '' ? NaN : Number(s.offset);
  const until = parseTime(s.until);
  const base = line.tours[s.tour];
  const pick = (i) => up({ tour: i, drafts: {} });
  const newTour = () => {
    const last = line.tours[line.tours.length - 1];
    line.tours.push({ number: nextNumber(line.tours), ai_group: last?.ai_group ?? 'Busses', extra: last?.extra ?? '', trips: [{ trip: choices[0] || '', profile: 0, departure: 360 }] });
    pick(line.tours.length - 1);
    touch(line);
  };
  const copy = () => {
    line.tours.push({ ...base, number: nextNumber(line.tours), trips: base.trips.map((x) => ({ ...x, departure: Math.max(0, x.departure + off) })) });
    pick(line.tours.length - 1);
    touch(line);
  };
  const repeat = () => {
    const made = repeatTour(line.tours, base, off, until);
    if (made > 0) {
      pick(line.tours.length - 1);
      touch(line);
    }
    toast(t('{made} tour(s) made, every {every} min up to {until}', { made, every: off, until: fmtTime(until) }), 'ok');
  };
  const remove = () => {
    line.tours.splice(s.tour, 1);
    pick(Math.max(0, s.tour - 1));
    touch(line);
  };
  return html`<section class="card tt-tours">
    <div class="card-head"><h2 class="ellipsis">${t('Line')} ${line.name}</h2></div>
    <div class="card-body stack" style="gap:12px">
      ${line.tours.length > 0 && html`<div class="tt-list">
        ${line.tours.map((x, i) => html`<button class=${'list-item' + (i === s.tour ? ' on' : '')} onClick=${() => pick(i)}>
          <span class="grow ellipsis title">${t('Tour')} ${x.number}</span>
          <span class="muted small tnum">${x.trips.length ? fmtTime(x.trips[0].departure) : ''}</span>
          <span class="muted small tnum tt-count">${t('{n} trips', { n: x.trips.length })}</span>
        </button>`)}
      </div>`}
      <${Button} wide icon="plus" label="New tour" onClick=${newTour} />
      <div class="tt-pair">
        <${Input} type="number" value=${s.offset} placeholder="minutes" onInput=${(v) => up({ offset: v })} />
        <${Button} icon="copy" label="Copy +min" disabled=${!base || !Number.isFinite(off)} onClick=${copy} />
      </div>
      <div class="tt-pair">
        <${Input} class=${'input' + (until == null ? ' bad' : '')} value=${s.until} placeholder="until h:mm" onInput=${(v) => up({ until: v })} />
        <${Button} icon="refresh" label="Repeat" disabled=${!base || !(off >= 1) || until == null} onClick=${repeat} />
      </div>
      <${Button} wide kind="danger" icon="trash" label="Delete tour" disabled=${!base} onClick=${remove} />
    </div>
  </section>`;
}

function TripsCard({ s, data, line, choices }) {
  const tour = line.tours[s.tour];
  const key = (i) => `${line.name}|${s.tour}|${i}`;
  const change = () => touch(line);
  const depart = (x, i, v) => {
    const m = parseTime(v);
    if (m != null) x.departure = m;
    up({ drafts: { ...tt().drafts, [key(i)]: v } });
    change();
  };
  const addTrip = () => {
    const n = tour.trips.length;
    const a = tour.trips[n - 2];
    const b = tour.trips[n - 1];
    const x = n === 0 ? { trip: choices[0] || '', profile: 0, departure: 360 } : n === 1 ? { ...b, departure: b.departure + 30 } : { ...a, departure: b.departure + Math.max(1, b.departure - a.departure) };
    tour.trips.push(x);
    change();
  };
  const shift = (d) => {
    for (const x of tour.trips) x.departure = Math.max(0, x.departure + d);
    up({ drafts: {} });
    change();
  };
  const remove = (i) => {
    tour.trips.splice(i, 1);
    up({ drafts: {} });
    change();
  };
  const trips = choices.map((n) => [n, n]);
  return html`<section class="card tt-trips">
    <div class="card-head tt-trips-head">
      <h2 class="ellipsis">${t('Tour')} ${tour.number} - ${tour.ai_group}</h2>
      <div class="row wrap">
        <${Button} small icon="plus" label="Add trip" onClick=${addTrip} />
        <${Button} small label="-1 min" disabled=${!tour.trips.length} onClick=${() => shift(-1)} />
        <${Button} small label="+1 min" disabled=${!tour.trips.length} onClick=${() => shift(1)} />
      </div>
    </div>
    <div class="card-body">
      <div class="tt-rows">
        <div class="tt-row tt-head"><span>${t('Departs')}</span><span>${t('Trip')}</span><span>${t('Profile')}</span><span>${t('To')}</span><span></span></div>
        ${tour.trips.map((x, i) => {
          const info = data.trips.find((y) => same(y.name, x.trip));
          const profiles = info?.profiles.length ? info.profiles.map((p, k) => [k, p]) : [[x.profile, String(x.profile)]];
          const draft = s.drafts[key(i)];
          return html`<div class="tt-row">
            <${Input} class=${'input tnum' + (draft != null && parseTime(draft) == null ? ' bad' : '')} value=${draft ?? fmtTime(x.departure)} placeholder="h:mm" onInput=${(v) => depart(x, i, v)} />
            <${Select} raw value=${choices.find((n) => same(n, x.trip)) ?? x.trip} options=${trips} onChange=${(v) => ((x.trip = v), (x.profile = 0), change())} />
            <${Select} raw value=${profiles.some(([k]) => k === x.profile) ? x.profile : profiles[0][0]} options=${profiles} onChange=${(v) => ((x.profile = Number(v)), change())} />
            <span class="ellipsis dim" title=${info?.terminus || ''}>${info?.terminus || ''}</span>
            <${Button} small kind="ghost" icon="x" title="Remove this trip" onClick=${() => remove(i)} />
          </div>`;
        })}
      </div>
    </div>
  </section>`;
}

export function TimetablePage() {
  const s = useStore();
  const maps = s.maps || [];
  if (!s.tt) S.tt = { map: null, loaded: null, data: null, error: null, line: null, tour: 0, dirty: [], ver: {}, drafts: {}, offset: '20', until: '22:00', newLine: '', armed: false, busy: false };
  const v = s.tt;
  const want = v.map && (v.dirty.length || maps.some((m) => m.file === v.map)) ? v.map : maps.find((m) => m.file === s.choice?.map)?.file || maps[0]?.file;
  useEffect(() => {
    if (want && v.loaded !== want) read(want);
  }, [want, v.loaded]);
  const n = v.dirty.length;
  const save = html`<${Button} kind=${n ? 'primary' : ''} icon="save" disabled=${!n || v.busy} label=${n === 0 ? 'Saved' : n === 1 ? 'Save' : undefined} onClick=${saveAll}>${n > 1 ? t('Save all ({n} lines)', { n }) : null}</${Button}>`;
  const head = html`<${PageHead} title="Timetable" text="A map's lines: their tours and when each trip leaves. Saved as the line's .ttl (in the content folder; OMSI 2's own files stay as they are)." actions=${v.data && save} />`;
  if (!maps.length) return html`${head}<${Card}><${Empty} icon="map" text="No maps found." /></${Card}>`;
  const line = v.data?.lines.find((l) => l.name === v.line);
  const choices = line ? tripChoices(v.data, line) : [];
  const tour = line?.tours[Math.min(v.tour, line.tours.length - 1)];
  if (line && tour && line.tours[v.tour] !== tour) v.tour = line.tours.indexOf(tour);
  return html`${head}
    <div class="tt">
      <${LinesCard} s=${v} maps=${maps} />
      ${line && html`<${ToursCard} s=${v} line=${line} choices=${choices} />`}
      ${tour && html`<${TripsCard} s=${v} data=${v.data} line=${line} choices=${choices} />`}
    </div>`;
}
