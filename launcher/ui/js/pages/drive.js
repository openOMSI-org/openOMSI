// Drive: the duty is put together here - map, bus, line and tour (or a free drive), the
// start time and date, the weather and the traffic - and started.

import { html, useState, useEffect, useRef } from '../../vendor/preact-htm.js';
import { S, useStore, set, toast } from '../store.js';
import { t, hhmm, longDate, language } from '../i18n.js';
import { call, contentUrl } from '../api.js';
import { Icon } from '../icons.js';
import { Button, Card, Field, Select, Input, Toggle, Segmented, Slider, Picture, Notice, Spinner, PageHead, Tag } from '../ui.js';
import { choose, curMap, curBus, curLine, curTour, pickedTrip, realTime, realDate, start, loadSituations, duty } from '../data.js';
import { MapPicker } from './map-picker.js';
import { BusPicker } from './bus-picker.js';
import { BusPreview } from '../preview.js';
import { RouteMap } from '../routemap.js';

/** HH:MM of seconds of the day (the timetables' unit). */
export const hms = (s) => hhmm((s || 0) / 60);

export const mapPicture = (file) => (file ? contentUrl(file.replace(/\\/g, '/').replace(/[^/]*$/, 'picture.jpg')) : '');

/** A trip without passengers (to or from the depot): its line is empty or it says so. */
export const depotRun = (x) => !x.line?.trim() || /betriebsfahrt|leerfahrt|dienstfahrt|not in service|out of service|hors service|zjazd do zajezdni/i.test(x.terminus || '');

/** The trip a start at `now` (seconds) takes: the one under way or the next. */
export function tripAt(tour, now) {
  const i = tour.trips.findIndex((x) => x.departure >= now - 120);
  return i >= 0 ? i : tour.trips.length ? tour.trips.length - 1 : -1;
}

export function DrivePage() {
  const s = useStore();
  const [picker, setPicker] = useState(null);
  const c = s.choice;
  if (!c) return null;
  return html`
    <${PageHead} title="Drive" text="Choose where and what you drive, then start." />
    ${s.errors.maps && html`<${Notice} kind="danger">${s.errors.maps}</${Notice}>`}
    <div class="drive">
      <div class="drive-cols">
        <div class="stack">
          <${MapCard} onPick=${() => setPicker('map')} />
          <${BusCard} onPick=${() => setPicker('bus')} preview=${picker !== 'bus'} />
        </div>
        <div class="stack">
          <${DutyCard} />
          <div class="grid-2" style="gap:16px">
            <${TimeCard} />
            <${ConditionsCard} />
          </div>
        </div>
      </div>
      <div class="side">
        <${Summary} />
      </div>
    </div>
    ${picker === 'map' && html`<${MapPicker} onClose=${() => setPicker(null)} />`}
    ${picker === 'bus' && html`<${BusPicker} onClose=${() => setPicker(null)} />`}
  `;
}

function MapCard({ onPick }) {
  const s = useStore();
  const m = curMap();
  return html`<${Card} title="Map" actions=${html`<${Button} small icon="map" label="Change map" onClick=${onPick} disabled=${!s.maps?.length} />`}>
    ${!s.maps
      ? html`<div class="row muted"><${Spinner} /> ${t('Reading the maps…')}</div>`
      : !m
        ? html`<${Notice} kind="warn">${t('No map is installed.')}</${Notice}>`
        : html`<div class="pick">
            <${Picture} src=${mapPicture(m.file)} />
            <div class="what">
              <h3>${m.friendly || m.name}</h3>
              <p>${m.description || ''}</p>
              <div class="row" style="margin-top:8px">${m.installed && html`<${Tag}>${t('Mod')}</${Tag}>`}<span class="muted small">${t('{n} entry points', { n: m.entry_points.length })}</span></div>
            </div>
          </div>`}
  </${Card}>`;
}

function BusCard({ onPick, preview }) {
  const s = useStore();
  const c = s.choice;
  const v = curBus();
  const paints = v ? [['', v.default_paint ? `${t('Standard')} (${v.default_paint})` : t('Standard')], ...v.paints.map((p) => [p, p])] : [];
  const numbers = v?.numbers || [];
  const hofs = v ? [['', `${t('Automatic')}${s.hof ? ` (${s.hof})` : ''}`], ...v.hofs.map((h) => [h, h])] : [];
  return html`<${Card} title="Bus" actions=${html`<${Button} small icon="bus" label="Change bus" onClick=${onPick} disabled=${!s.vehicles?.length} />`}>
    ${!s.vehicles
      ? html`<div class="row muted"><${Spinner} /> ${s.vehiclesProgress ? t('Reading the buses… {done} of {total}', s.vehiclesProgress) : t('Reading the buses…')}</div>`
      : !v
        ? html`<${Notice} kind="warn">${t('No bus is installed.')}</${Notice}>`
        : html`<div class="stack">
            ${preview && html`<${BusPreview} class="in-card" look=${{ bus: v.file, paint: c.paint, map: c.map, weather: c.weather, time: c.time, date: c.date }} />`}
            <div class="row">
              <div class="grow">
                <h3 style="font-size:17px">${v.name}</h3>
                <div class="muted small">${[v.manufacturer, v.type_name].filter(Boolean).join(' - ')}</div>
              </div>
              ${v.installed && html`<${Tag}>${t('Mod')}</${Tag}>`}
            </div>
            ${v.missing_packs?.length > 0 && html`<${Notice} kind="warn">${t('Parts of this bus come from packs that are not installed: {packs}', { packs: v.missing_packs.join(', ') })}</${Notice}>`}
            <div class="grid-2">
              <${Field} label="Paint"><${Select} raw value=${c.paint} options=${paints} onChange=${(paint) => choose({ paint })} /></${Field}>
              <${Field} label="Depot file">
                <${Select} raw value=${c.hof_manual ? c.hof : ''} options=${hofs} onChange=${(h) => choose({ hof: h, hof_manual: !!h })} />
              </${Field}>
              ${numbers.length > 0 && html`<${Field} label="Fleet number">
                <${Select} raw value=${c.number} options=${[['', `${t('Automatic')} (${numbers[0][0]})`], ...numbers.map(([n, p]) => [n, p ? `${n} - ${p}` : n])]} onChange=${(number) => choose({ number })} />
              </${Field}>`}
              <${Field} label="Number plate"><${Input} value=${c.plate} placeholder="Automatic" onInput=${(plate) => choose({ plate })} /></${Field}>
            </div>
          </div>`}
  </${Card}>`;
}

function DutyCard() {
  const s = useStore();
  const c = s.choice;
  const m = curMap();
  const lines = (s.lines.list || []).filter((l) => l.user_allowed && l.tours.length);
  const line = curLine();
  const tour = curTour();
  const [showEnded, setShowEnded] = useState(false);
  const now = c.time * 60;
  // the trip a start takes is brought into view when the duty changes (not while one scrolls)
  const tripsRef = useRef();
  const tripKey = `${c.line}|${c.tour}|${c.time}|${pickedTrip()}|${!!s.lines.list}`;
  useEffect(() => {
    const el = tripsRef.current;
    const on = el?.querySelector('tr.on');
    if (on) el.scrollTop += on.getBoundingClientRect().top - el.getBoundingClientRect().top - 70;
  }, [tripKey]);
  const entries = m ? [[-1, c.free ? 'Automatic (the map\'s first)' : 'Automatic (nearest to the first stop)'], ...m.entry_points.map((e, i) => [i, e.name])] : [];

  let body;
  if (c.free) {
    body = html`<div class="stack">
      <p class="muted">${t('Free driving: the bus starts where you chose, with the traffic and the timetable\'s buses around it, but no line of your own.')}</p>
      <${Field} label="Start at"><${Select} value=${c.entry} options=${entries} onChange=${(v) => choose({ entry: Number(v) })} /></${Field}>
      <${RouteMap} map=${c.map} date=${c.date} trip="" entry=${c.entry} onEntry=${(entry) => choose({ entry })} />
    </div>`;
  } else if (s.lines.loading && !s.lines.list) {
    body = html`<div class="row muted"><${Spinner} /> ${t('Reading the timetable…')}</div>`;
  } else if (s.lines.error) {
    body = html`<${Notice} kind="danger">${s.lines.error}</${Notice}>`;
  } else if (!lines.length) {
    body = html`<${Notice}>${t('This map has no lines to drive on this date. Choose a free drive, or another date.')}</${Notice}>`;
  } else {
    const tours = line ? [...line.tours] : [];
    const ended = (x) => x.runs && x.trips.every((tr) => tr.departure < now - 120);
    const endedCount = tours.filter(ended).length;
    const shown = tours.filter((x) => showEnded || !ended(x)).sort((a, b) => b.runs - a.runs || ended(a) - ended(b));
    const picked = pickedTrip();
    const trips = tour?.trips || [];
    const auto = tour ? tripAt(tour, now) : -1;
    const current = picked != null ? trips.findIndex((x) => x.index === picked) : auto;
    body = html`<div class="stack">
      <div class="grid-2">
        <${Field} label="Line">
          <${Select} raw value=${c.line ?? ''} options=${lines.map((l) => [l.name, l.termini.length ? `${l.name} - ${l.termini.join(' - ')}` : l.name])} onChange=${(name) => {
            const l = lines.find((x) => x.name === name);
            const tr = l?.tours.find((x) => x.runs) || l?.tours[0];
            choose({ line: name, tour: tr?.number ?? null, start_trip: null });
          }} />
        </${Field}>
        <${Field} label="Tour">
          <${Select} raw value=${c.tour ?? ''} options=${shown.map((x) => [x.number, `${x.number} - ${x.days}${!x.runs ? ` - ${t('not on this day')}` : ended(x) ? ` - ${t('ended')}` : ''}`])} onChange=${(number) => {
            const x = tours.find((y) => y.number === number);
            const patch = { tour: number, start_trip: null };
            // a tour of another day moves the date to the next day it runs (OMSI lists only the day's tours)
            if (x && !x.runs && x.next_run && !realDate()) patch.date = x.next_run;
            choose(patch);
          }} />
        </${Field}>
      </div>
      ${endedCount > 0 && html`<${Toggle} checked=${showEnded} onChange=${setShowEnded} label=${t('Show ended tours ({n})', { n: endedCount })} />`}
      ${tour && !tour.runs && html`<${Notice} kind="warn">${tour.next_run ? t('This tour does not run on the chosen date; it runs next on {date}.', { date: longDate(tour.next_run) }) : t('This tour does not run within a year of the chosen date.')}</${Notice}>`}
      ${tour && html`<div>
        <div class="row" style="margin-bottom:8px"><span class="label">${t('Trips of the tour')}</span><span class="muted small right">${t('Click a trip to start with it')}</span></div>
        <div class="trips" ref=${tripsRef}>
          <table class="table">
            <thead><tr><th>${t('Departs')}</th><th>${t('Line')}</th><th>${t('From')}</th><th>${t('To')}</th><th>${t('Arrives')}</th></tr></thead>
            <tbody>
              ${trips.map((x, i) => html`<tr class=${'click' + (i === current ? ' on' : '') + (x.arrival < now - 60 && i !== current ? ' past' : '')} onClick=${() => pickTrip(x)}>
                <td class="num"><b>${hms(x.departure)}</b></td>
                <td>${depotRun(x) ? html`<span class="muted">${t('Depot run')}</span>` : html`<span class="line-badge">${x.line}</span>`}</td>
                <td class="ellipsis" style="max-width:180px">${x.from}</td>
                <td class="ellipsis" style="max-width:180px">${x.terminus}</td>
                <td class="num muted">${hms(x.arrival)}</td>
              </tr>`)}
            </tbody>
          </table>
        </div>
      </div>`}
      <${Field} label="Start at"><${Select} value=${c.entry} options=${entries} onChange=${(v) => choose({ entry: Number(v) })} /></${Field}>
      ${trips[current] && html`<${RouteMap} map=${c.map} date=${c.date} trip=${trips[current].name} entry=${c.entry}
        stops=${trips[current].stops.map((x) => ({ time: hms(x.dep || x.arr), name: x.name }))} onEntry=${(entry) => choose({ entry })} />`}
    </div>`;
  }
  return html`<${Card} title="Duty" actions=${html`<${Segmented} value=${c.free ? 'free' : 'line'} options=${[['line', 'Timetable'], ['free', 'Free drive']]} onChange=${(v) => choose({ free: v === 'free' })} />`}>
    ${body}
  </${Card}>`;
}

/** HH:MM typed in 24 hours (the timetables' clock, whatever the system's locale says). */
function TimeInput({ minutes, onChange, disabled }) {
  const [text, setText] = useState(hhmm(minutes));
  useEffect(() => setText(hhmm(minutes)), [minutes]);
  const commit = () => {
    const m = /^\s*(\d{1,2})\s*[:.\s]?\s*(\d{2})?\s*$/.exec(text);
    const h = m ? Number(m[1]) : NaN;
    const mi = m ? Number(m[2] || 0) : NaN;
    if (h >= 0 && h < 24 && mi >= 0 && mi < 60) onChange(h * 60 + mi);
    else setText(hhmm(minutes));
  };
  return html`<input class="input num" value=${text} disabled=${disabled} maxlength="5" placeholder="09:00" onInput=${(e) => setText(e.currentTarget.value)} onBlur=${commit} onKeyDown=${(e) => e.key === 'Enter' && e.currentTarget.blur()} />`;
}

/** A date typed as the timetables write it (YYYY-MM-DD), whatever the system's locale. */
function DateInput({ value, onChange, disabled }) {
  const [text, setText] = useState(value);
  useEffect(() => setText(value), [value]);
  const commit = () => {
    const m = /^\s*(\d{4})-(\d{1,2})-(\d{1,2})\s*$/.exec(text);
    const d = m && new Date(Date.UTC(+m[1], +m[2] - 1, +m[3]));
    if (d && d.getUTCMonth() === +m[2] - 1) onChange(d.toISOString().slice(0, 10));
    else setText(value);
  };
  return html`<input class="input num" value=${text} disabled=${disabled} maxlength="10" placeholder="1989-05-30" onInput=${(e) => setText(e.currentTarget.value)} onBlur=${commit} onKeyDown=${(e) => e.key === 'Enter' && e.currentTarget.blur()} />`;
}

/** Start the tour at this trip: at its departure (or at the real time: the bus waits). */
export function pickTrip(x) {
  const c = S.choice;
  const time = realTime() ? c.time : Math.floor(x.departure / 60);
  choose({ time, start_trip: [c.line, c.tour, x.index, time] });
}

export const SEASONS = [['auto', 'By date'], ['spring', 'Spring'], ['summer', 'Summer'], ['autumn', 'Autumn'], ['winter', 'Winter']];
export const PHASES = [['early', 'Early'], ['mid', 'Mid'], ['late', 'Late']];

export async function setSeason(season, phase) {
  const c = S.choice;
  if (season === 'auto') {
    choose({ season, date: c.own_date || c.date, own_date: null });
    return;
  }
  const own = c.season === 'auto' ? c.date : c.own_date;
  const r = await call('season_date', { season, phase, date: c.date, map: c.map, keep: false }).catch(() => null);
  choose({ season, phase, own_date: own, ...(r?.date ? { date: r.date } : {}) });
}

function TimeCard() {
  const s = useStore();
  const c = s.choice;
  const w = s.weather || [];
  const cur = w.find((x) => x.file === c.weather);
  const rt = realTime();
  const rd = realDate();
  return html`<${Card} title="Time and weather">
    <div class="stack">
      <div class="grid-2">
        <${Field} label="Start time" help=${rt ? 'Follows the computer\'s clock (Settings)' : null}>
          <${TimeInput} minutes=${c.time} disabled=${rt} onChange=${(time) => choose({ time })} />
        </${Field}>
        <${Field} label="Date" help=${rd ? 'Today (Settings)' : null}>
          <${DateInput} value=${c.date} disabled=${rd} onChange=${(date) => choose({ date, season: 'auto', own_date: null })} />
        </${Field}>
      </div>
      <${Field} label="Season">
        <${Select} value=${c.season} options=${SEASONS} onChange=${(v) => setSeason(v, c.phase)} />
      </${Field}>
      ${c.season !== 'auto' && html`<${Segmented} full value=${c.phase} options=${PHASES} onChange=${(p) => setSeason(c.season, p)} />`}
      <${Field} label="Weather" help=${cur ? cur.description : c.weather ? null : 'Develops by itself through the day and the season'}>
        <${Select} raw value=${c.weather} options=${[['', t('Natural weather')], ...w.map((x) => [x.file, x.name])]} onChange=${(weather) => choose({ weather })} />
      </${Field}>
    </div>
  </${Card}>`;
}

function ConditionsCard() {
  const c = useStore().choice;
  return html`<${Card} title="On the road">
    <div class="stack" style="gap:6px">
      <${Slider} label="Cars around" value=${c.traffic} min=${0} max=${120} step=${1} onChange=${(traffic) => choose({ traffic })} format=${(v) => (v ? String(Math.round(v)) : t('None'))} />
      <${Toggle} checked=${c.passengers} onChange=${(passengers) => choose({ passengers })} label="Passengers" help="People wait at the stops and ride along" />
      <${Toggle} checked=${c.schedule} onChange=${(schedule) => choose({ schedule })} label="Timetable buses" help="Other buses of the timetable drive their lines" />
      <${Toggle} checked=${c.autostart} onChange=${(autostart) => choose({ autostart })} label="Bus ready to go" help="Engine running, doors and lights set" />
      <${Toggle} checked=${c.on_foot} onChange=${(on_foot) => choose({ on_foot })} label="Start on foot" help="Walk to the bus, or place one later from the menu" />
    </div>
  </${Card}>`;
}

function Summary() {
  const s = useStore();
  const c = s.choice;
  const m = curMap();
  const v = curBus();
  const line = curLine();
  const tour = curTour();
  const [busy, setBusy] = useState(false);
  const [args, setArgs] = useState(null);
  const trip = tour && !c.free ? (pickedTrip() != null ? tour.trips.find((x) => x.index === pickedTrip()) : tour.trips[tripAt(tour, c.time * 60)]) : null;
  const ready = m && v && (c.free || (line && tour));
  const running = s.poll.instances?.length || 0;
  const joined = c.lan_mode === 'join';
  const go = async (extra) => {
    setBusy(true);
    await start(extra);
    setBusy(false);
  };
  useEffect(() => setArgs(null), [c]);
  return html`
    <section class="card summary">
      <${Picture} src=${m ? mapPicture(m.file) : ''}>
        ${m && html`<div class="over"><b>${m.friendly || m.name}</b><span>${longDate(c.date)} - ${hhmm(c.time)}</span></div>`}
      </${Picture}>
      <div class="card-body stack" style="gap:14px">
        ${trip && !depotRun(trip)
          ? html`<div class="sign"><span class="no">${trip.line}</span><span class="to">${trip.terminus}</span></div>`
          : c.free && html`<div class="sign"><span class="no"><${Icon} name="route" /></span><span class="to">${t('Free drive')}</span></div>`}
        <dl class="facts">
          <dt>${t('Bus')}</dt><dd>${v ? v.name : '-'}</dd>
          ${!c.free && html`<dt>${t('Line')}</dt><dd>${line ? line.name : '-'}</dd>`}
          ${!c.free && html`<dt>${t('Tour')}</dt><dd>${tour ? tour.number : '-'}</dd>`}
          ${trip && html`<dt>${t('First trip')}</dt><dd class="num">${hms(trip.departure)} ${trip.from}</dd>`}
          <dt>${t('Weather')}</dt><dd>${s.weather?.find((x) => x.file === c.weather)?.name || t('Natural weather')}</dd>
          <dt>${t('Driver')}</dt><dd>${s.config?.profile || '-'}</dd>
        </dl>
        ${joined && html`<${Notice} icon="users">
          <div class="row"><span class="grow">${t('Joining {server}', { server: c.lan_addr })}</span><${Button} small kind="ghost" label="Leave" onClick=${() => choose({ lan_mode: 'off' })} /></div>
        </${Notice}>`}
        <${Button} kind="primary" big wide icon="play" disabled=${!ready || busy} onClick=${() => go()}>
          ${busy ? t('Starting…') : joined ? t('Join and drive') : c.free ? t('Start driving') : t('Start the duty')}
        </${Button}>
        ${running > 0 && html`<div class="muted small center">${t('{n} game(s) running', { n: running })} - <a onClick=${() => set({ page: 'games' })}>${t('Show')}</a></div>`}
        <a class="small muted" onClick=${async () => setArgs(args ? null : await call('duty_args', { duty: await duty() }).catch((e) => [e.message]))}>${args ? t('Hide the command line') : t('Show the command line')}</a>
        ${args && html`<div class="log selectable">${args.map((a) => (/\s/.test(a) ? `"${a}"` : a)).join(' ')}</div>`}
      </div>
    </section>
    <${Continue} go=${go} busy=${busy} />
  `;
}

/** The situations saved on this map: the last one, and the save slots. */
function Continue({ go, busy }) {
  const s = useStore();
  useEffect(() => {
    loadSituations();
  }, []);
  if (!s.situations?.length) return null;
  return html`<${Card} title="Continue">
    <div class="list" style="margin:-6px -8px">
      ${s.situations.slice(0, 5).map((x) => html`<div class="list-item" onClick=${() => !busy && go({ situation: x.file })}>
        <${Icon} name="history" />
        <div class="grow"><div class="title ellipsis">${t(x.name)}</div><div class="sub">${new Date(x.saved * 1000).toLocaleString(language())}</div></div>
        <${Icon} name="play" size=${16} />
      </div>`)}
    </div>
  </${Card}>`;
}
