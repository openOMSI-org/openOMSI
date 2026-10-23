// Drive: the duty is put together here - map, bus, line and tour (or a free drive), the
// start time and date, the weather and the traffic - and started.

import { html, useState, useEffect, useRef } from '../../vendor/preact-htm.js';
import { S, useStore, toast } from '../store.js';
import { t, hhmm, language } from '../i18n.js';
import { call, contentUrl } from '../api.js';
import { Button, Dialog } from '../ui.js';
import { choose, realTime, realDate, launcherLayout } from '../data.js';
import { MultiplayerDutyNotice } from './multiplayer.js';
import { ClassicDrive } from './drive-classic.js';
import { LayoutDrive } from './drive-layouts.js';

/** HH:MM of seconds of the day (the timetables' unit). */
export const hms = (s) => hhmm((s || 0) / 60);

/** An entry point's name, or its number when the map gives it none. */
export const entryName = (e) => e.name?.trim() || `${t('Entry point')} ${e.index + 1}`;

/** A length of time in seconds as the player's language writes it, short: "22 min", "1 hr 5 min"
 *  (a narrow column breaks it between the hours and the minutes, never inside "5 min"). */
export function duration(seconds) {
  const m = Math.max(1, Math.round(seconds / 60));
  const unit = (n, u) => new Intl.NumberFormat(language(), { style: 'unit', unit: u, unitDisplay: 'short' }).format(n).replace(/ /g, '\u00a0');
  if (m < 60) return unit(m, 'minute');
  return m % 60 ? `${unit(Math.floor(m / 60), 'hour')} ${unit(m % 60, 'minute')}` : unit(m / 60, 'hour');
}

/** How long a trip takes, from its departure to its arrival (#2813). The arrival of a trip
 *  over midnight runs on past 24:00, so the difference is its length. */
export const tripTime = (x) => duration(x.arrival - x.departure);

export function tripLength(tour) {
  const runs = tour.trips.filter((x) => !depotRun(x)).map((x) => x.arrival - x.departure).sort((a, b) => a - b);
  return runs.length ? duration(runs[runs.length >> 1]) : '';
}

export const mapPicture = (file) => (file ? contentUrl(file.replace(/\\/g, '/').replace(/[^/]*$/, 'picture.jpg')) : '');

export const TripEnds = ({ trip, stack }) => html`<span class=${'trip-ends' + (stack ? ' stacked' : '')}><span class="from">${trip.from}</span>${stack ? '' : ' → '}<span class="to">${trip.terminus}</span></span>`;

/** A trip without passengers (to or from the depot): its line is empty or it says so. */
export const depotRun = (x) => !x.line?.trim() || /betriebsfahrt|leerfahrt|dienstfahrt|not in service|out of service|hors service|zjazd do zajezdni/i.test(x.terminus || '');

/** The trip a start at `now` (seconds) takes: the one under way or the next. */
export function tripAt(tour, now) {
  // a night tour's times run on past 24:00: at 03:25, before the tour's first trip of the
  // day, its trip of 27:30 is the one at 03:30 (the tour begun the evening before, #2166)
  const n = tour.trips.length;
  if (n && now < 86400 && tour.trips[0].departure > now && tour.trips[n - 1].departure >= now + 86400 - 120) now += 86400;
  const i = tour.trips.findIndex((x) => x.departure >= now - 120);
  return i >= 0 ? i : tour.trips.length ? tour.trips.length - 1 : -1;
}

export const lineMatches = (l, q) => !q || `${l.name} ${l.termini.join(' ')}`.toLowerCase().includes(q);

export function tourTrip(tour, now, q) {
  const ended = tour.runs && tour.trips.every((x) => x.departure < now - 120);
  const first = tour.trips.length ? tripAt(tour, now) : -1;
  const has = (s) => (s || '').toLowerCase().includes(q);
  if (!q || has(tour.number)) return { ended, at: first };
  const from = ended ? 0 : Math.max(first, 0);
  const at = tour.trips.findIndex((x, i) => i >= from && (has(x.name) || has(x.line) || has(x.from) || has(x.terminus) || x.stops?.some((st) => has(st.name))));
  const hit = tour.trips[at];
  const via = hit && !has(hit.line) && !has(hit.from) && !has(hit.terminus) ? hit.stops?.find((st) => has(st.name))?.name : null;
  return { ended, at, via };
}

export function findTours(line, now, q, showEnded, keep) {
  const all = (line?.tours || []).map((tour) => ({ tour, ...tourTrip(tour, now, q) })).filter((x) => x.at >= 0 || (!x.tour.trips.length && (!q || x.tour.number.toLowerCase().includes(q))));
  const endedCount = all.filter((x) => x.ended).length;
  const dep = (x) => x.tour.trips[x.at]?.departure ?? Infinity;
  const shown = all
    .filter((x) => showEnded || !x.ended || (!q && x.tour.number === keep))
    .sort((a, b) => b.tour.runs - a.tour.runs || a.ended - b.ended || (q ? dep(a) - dep(b) : 0) || a.tour.number.localeCompare(b.tour.number, undefined, { numeric: true }));
  return { shown, endedCount };
}

export function tourRoute(tour) {
  const service = tour.trips.filter((x) => !depotRun(x));
  const from = (service[0] || tour.trips[0])?.from || '';
  const to = (service[service.length - 1] || tour.trips[tour.trips.length - 1])?.terminus || '';
  return from || to ? `${from} → ${to}` : '';
}

export function chooseLine(l) {
  const tr = findTours(l, S.choice.time * 60, '', false).shown[0]?.tour || l?.tours.find((x) => x.runs) || l?.tours[0];
  choose({ line: l.name, tour: tr?.number ?? null, start_trip: null });
}

export function chooseTour(x, trip) {
  if (x.number === S.choice.tour && !trip) return;
  const patch = { tour: x.number, start_trip: null };
  if (!x.runs && x.next_run && !realDate()) patch.date = x.next_run;
  choose(patch);
  if (trip) pickTrip(trip);
}

export function DrivePage() {
  useStore();
  const layout = launcherLayout();
  return html`<${MultiplayerDutyNotice} />${layout === 'classic' ? html`<${ClassicDrive} />` : html`<${LayoutDrive} layout=${layout} />`}`;
}

// the player's notes on maps and buses (#1791): read once, saved as they are typed
let notesNow = null;
const noteKey = (kind, file) => `${kind}:${(file || '').replace(/\\/g, '/').toLowerCase()}`;

/** A note of the player's own on a map or a bus, kept on this computer. */
export function Notes({ kind, file }) {
  const key = noteKey(kind, file);
  const [text, setText] = useState(notesNow?.[key] || '');
  const [open, setOpen] = useState(!!notesNow?.[key]);
  const timer = useRef(0);
  useEffect(() => {
    let alive = true;
    const show = (all) => alive && (setText(all[key] || ''), setOpen(!!all[key]));
    if (notesNow) show(notesNow);
    else call('notes_get').then((all) => ((notesNow = all || {}), show(notesNow)), () => {});
    return () => {
      alive = false;
    };
  }, [key]);
  const save = (v) => {
    setText(v);
    if (notesNow) notesNow[key] = v;
    clearTimeout(timer.current);
    timer.current = setTimeout(() => call('notes_save', { key, text: v }).catch(() => {}), 600);
  };
  if (!file) return null;
  if (!open) return html`<div><${Button} small kind="ghost" icon="file" label="Add a note" onClick=${() => setOpen(true)} /></div>`;
  return html`<textarea class="input" rows="3" style="width:100%" placeholder=${t('Your own note, kept on this computer')} value=${text} onInput=${(e) => save(e.currentTarget.value)}></textarea>`;
}

/** HH:MM typed in 24 hours (the timetables' clock, whatever the system's locale says). */
export function TimeInput({ minutes, onChange, disabled }) {
  const [text, setText] = useState(hhmm(minutes));
  useEffect(() => setText(hhmm(minutes)), [minutes]);
  const commit = () => {
    const m = /^\s*(\d{1,2})\s*[:.\s]?\s*(\d{2})?\s*$/.exec(text);
    const h = m ? Number(m[1]) : NaN;
    const mi = m ? Number(m[2] || 0) : NaN;
    if (h >= 0 && h < 24 && mi >= 0 && mi < 60) onChange(h * 60 + mi);
    else setText(hhmm(minutes));
  };
  // the mouse wheel turns the clock: a minute over the minutes, an hour over the hours (#2351)
  const wheel = (e) => {
    if (disabled) return;
    e.preventDefault();
    const d = e.deltaY || e.deltaX;
    if (!d) return;
    const box = e.currentTarget.getBoundingClientRect();
    const hours = e.shiftKey || e.clientX - box.left < Math.min(box.width / 2, 34);
    const step = (d < 0 ? 1 : -1) * (hours ? 60 : 1);
    onChange((((minutes + step) % 1440) + 1440) % 1440);
  };
  return html`<input class="input num" value=${text} disabled=${disabled} maxlength="5" placeholder="09:00" title=${t('The mouse wheel turns the hours (left) or the minutes (right)')} onWheel=${wheel} onInput=${(e) => setText(e.currentTarget.value)} onBlur=${commit} onKeyDown=${(e) => e.key === 'Enter' && e.currentTarget.blur()} />`;
}

/** A month to click a day in (#2351, #1625): the week starts on Monday, as the timetables'. */
export function DatePicker({ value, onPick, onClose }) {
  const at = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value || '');
  const [month, setMonth] = useState(() => (at ? new Date(Date.UTC(+at[1], +at[2] - 1, 1)) : new Date(Date.UTC(new Date().getFullYear(), new Date().getMonth(), 1))));
  const y = month.getUTCFullYear();
  const m = month.getUTCMonth();
  const step = (n) => setMonth(new Date(Date.UTC(y, m + n, 1)));
  const loc = language();
  const title = new Intl.DateTimeFormat(loc, { month: 'long', year: 'numeric', timeZone: 'UTC' }).format(month);
  const dayName = new Intl.DateTimeFormat(loc, { weekday: 'short', timeZone: 'UTC' });
  // (2024-01-01 was a Monday)
  const heads = [0, 1, 2, 3, 4, 5, 6].map((i) => dayName.format(new Date(Date.UTC(2024, 0, 1 + i))));
  const lead = (month.getUTCDay() + 6) % 7;
  const days = new Date(Date.UTC(y, m + 1, 0)).getUTCDate();
  const iso = (d) => new Date(Date.UTC(y, m, d)).toISOString().slice(0, 10);
  const today = localNow().date;
  const cell = 'height:34px;border-radius:6px;border:none;font:inherit;cursor:pointer;';
  return html`<${Dialog} title="Choose a date" onClose=${onClose}>
    <div class="stack" style="gap:10px;min-width:280px">
      <div class="row" style="gap:6px;align-items:center">
        <${Button} small kind="ghost" icon="left" title="Previous month" onClick=${() => step(-1)} />
        <div class="grow" style="text-align:center;font-weight:600">${title}</div>
        <${Button} small kind="ghost" icon="right" title="Next month" onClick=${() => step(1)} />
      </div>
      <div style="display:grid;grid-template-columns:repeat(7,1fr);gap:4px;text-align:center">
        ${heads.map((h) => html`<div class="muted small">${h}</div>`)}
        ${Array.from({ length: lead }, () => html`<div></div>`)}
        ${Array.from({ length: days }, (_, i) => {
          const d = iso(i + 1);
          const on = d === value;
          const bg = on ? 'background:var(--accent);color:#000;font-weight:600;' : 'background:var(--surface-2);color:var(--text);';
          const ring = d === today && !on ? 'box-shadow:inset 0 0 0 1px var(--accent);' : '';
          return html`<button type="button" style=${cell + bg + ring} onClick=${() => onPick(d)}>${i + 1}</button>`;
        })}
      </div>
    </div>
  </${Dialog}>`;
}

/** A date typed as the timetables write it (YYYY-MM-DD), whatever the system's locale. */
export function DateInput({ value, onChange, disabled }) {
  const [text, setText] = useState(value);
  useEffect(() => setText(value), [value]);
  const commit = () => {
    const m = /^\s*(\d{4})-(\d{1,2})-(\d{1,2})\s*$/.exec(text);
    const d = m && new Date(Date.UTC(+m[1], +m[2] - 1, +m[3]));
    if (d && d.getUTCMonth() === +m[2] - 1) onChange(d.toISOString().slice(0, 10));
    else setText(value);
  };
  // the mouse wheel steps a day (#2351)
  const wheel = (e) => {
    if (disabled) return;
    e.preventDefault();
    const d = e.deltaY || e.deltaX;
    const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value || '');
    if (!d || !m) return;
    onChange(new Date(Date.UTC(+m[1], +m[2] - 1, +m[3] + (d < 0 ? 1 : -1))).toISOString().slice(0, 10));
  };
  return html`<input class="input num" value=${text} disabled=${disabled} maxlength="10" placeholder="1989-05-30" onWheel=${wheel} onInput=${(e) => setText(e.currentTarget.value)} onBlur=${commit} onKeyDown=${(e) => e.key === 'Enter' && e.currentTarget.blur()} />`;
}

/** Start the tour at this trip: at its departure (or at the real time: the bus waits). */
export function pickTrip(x) {
  const c = S.choice;
  // (a trip after midnight of a night tour, 24:30, starts at 00:30)
  const time = realTime() ? c.time : Math.floor(x.departure / 60) % (24 * 60);
  choose({ time, start_trip: [c.line, c.tour, x.index, time] });
}

export const SEASONS = [['auto', 'By date'], ['spring', 'Spring'], ['summer', 'Summer'], ['autumn', 'Autumn'], ['winter', 'Winter']];

/** The season the chosen date (or the season chosen instead) means. */
export function seasonOf(c) {
  if (c.season && c.season !== 'auto') return c.season;
  const m = Number((c.date || '').slice(5, 7)) || 5;
  return m === 12 || m <= 2 ? 'winter' : m <= 5 ? 'spring' : m <= 8 ? 'summer' : 'autumn';
}

/** A weather preset that makes sense in the season (no summer shower in the snow), as the
 *  game's own launcher chose them. */
export function weatherFits(w, season) {
  const rain = (w.precip || '').startsWith('rain');
  if (season === 'winter') return w.snow || (rain ? w.temp <= 10 : w.temp <= 16);
  if (w.snow || w.temp <= 0) return false;
  return season !== 'summer' || w.temp >= 8;
}

const precipText = (p) => {
  const m = /^(rain|snow) (\d+)%$/.exec(p || '');
  return m ? (m[1] === 'rain' ? t('rain {n}%', { n: m[2] }) : t('snow {n}%', { n: m[2] })) : t(p || '');
};

/** What a weather file says about itself, short: "15 °C - rain 68% - 200 m". */
export const weatherFacts = (w) => `${Math.round(w.temp)} °C - ${precipText(w.precip)} - ${w.fog_m >= 20000 ? t('clear air') : `${Math.round(w.fog_m)} m`}`;

/** The icon a weather file deserves. */
export function weatherIcon(w) {
  const clouds = (w.clouds || '').toLowerCase();
  if (w.snow || (w.precip || '').startsWith('snow')) return 'snow';
  if ((w.precip || '').startsWith('rain')) return 'rain';
  if (w.fog_m < 1500) return 'fog';
  if (clouds.includes('overcast')) return 'cloud';
  if (clouds.includes('cumulus')) return 'partly';
  return 'sun';
}
export const PHASES = [['early', 'Early'], ['mid', 'Mid'], ['late', 'Late']];

export async function setSeason(season, phase) {
  const c = S.choice;
  if (season === 'auto') {
    const date = c.own_date || c.date;
    choose({ season, date, own_date: null, ...unfit(c, seasonOf({ season, date })) });
    return;
  }
  const own = c.season === 'auto' ? c.date : c.own_date;
  const r = await call('season_date', { season, phase, date: c.date, map: c.map, keep: false }).catch(() => null);
  choose({ season, phase, own_date: own, ...(r?.date ? { date: r.date } : {}), ...unfit(c, season) });
}

/** A weather preset chosen that does not fit the new season goes, natural weather instead
 *  (said once): what to choose for it. */
function unfit(c, season) {
  const w = (S.weather || []).find((x) => x.file === c.weather);
  if (!w || weatherFits(w, season)) return {};
  toast(t('{name} does not fit the season: natural weather instead', { name: w.name }));
  return { weather: '' };
}

export function localNow() {
  const d = new Date();
  const p = (n) => String(n).padStart(2, '0');
  return { time: d.getHours() * 60 + d.getMinutes(), date: `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}` };
}
