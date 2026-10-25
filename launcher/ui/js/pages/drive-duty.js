import { html, useState, useEffect, useRef, useMemo } from '../../vendor/preact-htm.js';
import { useStore } from '../store.js';
import { t, hhmm, longDate } from '../i18n.js';
import { Icon } from '../icons.js';
import { Button, Field, Select, Toggle, Segmented, Picture, Notice, Spinner, Search } from '../ui.js';
import { tourId, choose, curMap, curLine, curTour, pickedTrip, mapName, duty, busFile, realDate, loadIbis } from '../data.js';
import { call } from '../api.js';
import { RouteMap } from '../routemap.js';
import { MapPicker } from './map-picker.js';
import { hms, mapPicture, depotRun, tripAt, lineMatches, findTours, tourRoute, chooseLine, chooseTour, pickTrip, Notes, tripLength, tripTime, TripEnds, entryName } from './drive.js';

export function MapRow() {
  const s = useStore();
  const c = s.choice;
  const m = curMap();
  const [picking, setPicking] = useState(false);
  const srv = c.lan_mode === 'join' ? c.lan_server || { name: c.lan_addr } : null;
  if (srv) {
    return html`<div class="map-row">
      <${Picture} src=${mapPicture(c.lan_map || m?.file)} />
      <div class="grow">
        <b class="ellipsis">${c.lan_map ? mapName(c.lan_map.replace(/\\/g, '/')) : m ? m.friendly || m.name : ''}</b>
        <span class="muted small ellipsis">${t('Set by {server}', { server: srv.name || c.lan_addr })}</span>
      </div>
      <${Button} small icon="x" label="Leave the server" onClick=${() => choose({ lan_mode: 'off' })} />
    </div>`;
  }
  if (!s.maps) return html`<div class="row muted"><${Spinner} /> ${t('Reading the maps…')}</div>`;
  if (!m) return html`<${Notice} kind="warn">${t('No map is installed.')}</${Notice}>`;
  return html`<button type="button" class="map-row click" onClick=${() => setPicking(true)} title=${t('Change map')}>
      <${Picture} src=${mapPicture(m.file)} />
      <span class="grow">
        <b class="ellipsis">${m.friendly || m.name}</b>
        <span class="muted small ellipsis">${m.installed ? `${t('Mod')} - ` : ''}${t('{n} entry points', { n: m.entry_points.length })}</span>
      </span>
      <span class="btn small icon"><${Icon} name="map" /></span>
    </button>
    ${picking && html`<${MapPicker} onClose=${() => setPicking(false)} />`}`;
}

function useScrollToChosen(ref, q, deps) {
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const on = !q && el.querySelector('.on');
    el.scrollTop = on ? el.scrollTop + on.getBoundingClientRect().top - el.getBoundingClientRect().top - 8 : 0;
  }, deps);
}

export function Lines({ lines }) {
  const s = useStore();
  const c = s.choice;
  const lineKey = `${s.lines.key}|${!!s.lines.list}`;
  const [q, setQ] = useState('');
  const ref = useRef();
  useEffect(() => setQ(''), [c.map]);
  useScrollToChosen(ref, q, [c.map, c.free, lineKey, lines.length, q]);
  const lq = q.trim().toLowerCase();
  const found = lines.filter((l) => lineMatches(l, lq));
  return html`<div class="duty-col lines">
    <div class="row duty-head"><span class="section-title grow">${t('Line')}</span><span class="muted small">${lq ? `${found.length} / ${lines.length}` : lines.length}</span></div>
    ${lines.length > 8 && html`<${Search} value=${q} onInput=${setQ} placeholder="Filter lines..." />`}
    <div class="pick-list" ref=${ref} role="listbox">
      ${found.length === 0 && html`<p class="muted small" style="padding:8px">${t('No line matches.')}</p>`}
      ${found.map((l) => html`<button type="button" role="option" aria-selected=${l.name === c.line} class=${'line-row' + (l.name === c.line ? ' on' : '')} onClick=${() => l.name !== c.line && chooseLine(l)} title=${[l.name, ...l.termini].join(' - ')}>
        <span class="line-badge ellipsis">${l.name}</span>
        <span class="grow ellipsis muted">${l.termini.join(' - ')}</span>
        <span class="muted small row count" title=${t('Tours of this line on the chosen day')}><${Icon} name="calendar" size=${13} />${l.tours.filter((x) => x.runs).length}</span>
      </button>`)}
    </div>
  </div>`;
}

export function Tours({ line }) {
  const s = useStore();
  const c = s.choice;
  const lineKey = `${s.lines.key}|${!!s.lines.list}`;
  const [showEnded, setShowEnded] = useState(false);
  const [q, setQ] = useState('');
  const ref = useRef();
  const now = c.time * 60;
  useEffect(() => setQ(''), [c.line]);
  useScrollToChosen(ref, q, [c.map, c.free, c.line, lineKey, q]);
  const tq = q.trim().toLowerCase();
  const { shown, endedCount } = useMemo(() => findTours(line, now, tq, showEnded, c.tour), [line, now, tq, showEnded, c.tour]);
  return html`<div class="duty-col tours">
    <div class="row duty-head"><span class="section-title grow">${t('Tour')}</span>${endedCount > 0 && html`<${Toggle} checked=${showEnded} onChange=${setShowEnded} label=${t('Show ended tours ({n})', { n: endedCount })} />`}</div>
    <${Search} value=${q} onInput=${setQ} placeholder="Filter tours: number, route, stop..." />
    <div class="pick-list" ref=${ref} role="listbox">
      ${shown.length === 0 && html`<p class="muted small" style="padding:8px">${t(tq ? 'No tour has a trip still to come that matches.' : 'No tour left at this time: show the ended tours, or start earlier.')}</p>`}
      ${shown.map(({ tour: x, ended: done, at, via }) => {
        const trip = x.trips[at];
        const live = x.runs && !done;
        const hit = tq && trip && !x.number.toLowerCase().includes(tq);
        const days = !x.runs ? (x.next_run ? t('runs {date}', { date: longDate(x.next_run) }) : t('never within a year')) : done ? t('ended') : x.days;
        return html`<button type="button" role="option" aria-selected=${tourId(x) === c.tour} class=${'tour-row' + (tourId(x) === c.tour ? ' on' : '') + (live ? '' : ' dim')} onClick=${() => chooseTour(x, hit ? trip : null)}>
          <span class="row"><b class="grow ellipsis">${x.number}</b>${trip && html`<span class=${'num small' + (live ? ' accent-fg' : ' muted')}>${hms(trip.departure)} - ${hms(trip.arrival)}</span>`}</span>
          <span class="sub ellipsis">${hit ? html`${trip.line ? `${trip.line} | ` : ''}<${TripEnds} trip=${trip} />${via ? ` | ${t('via {stop}', { stop: via })}` : ''}` : tourRoute(x)}</span>
          <span class="sub ellipsis">${t('{n} trips', { n: x.trips.length })} - ${days}${tripLength(x) ? ` - ${t('{time} per trip', { time: tripLength(x) })}` : ''}</span>
        </button>`;
      })}
    </div>
  </div>`;
}

function TourBus({ tour }) {
  const s = useStore();
  const c = s.choice;
  const [found, setFound] = useState([]);
  useEffect(() => {
    setFound([]);
    if (!tour.ai_group || !c.map) return;
    let alive = true;
    call('tour_buses', { map: c.map, group: tour.ai_group, date: c.date }).then((r) => alive && setFound(r || []), () => {});
    return () => (alive = false);
  }, [c.map, c.date, tour.ai_group]);
  const buses = found
    .map((f) => {
      const v = (s.vehicles || []).find((x) => busFile(x.file) === busFile(f.file));
      if (!v) return null;
      const paint = v.paints.includes(f.paint) ? f.paint : '';
      const number = (v.numbers || []).some(([n]) => n === f.number) ? f.number : '';
      return { v, paint, number };
    })
    .filter(Boolean)
    .slice(0, 10);
  if (!buses.length) return null;
  return html`<div class="row muted small tour-buses" style="gap:6px;flex-wrap:wrap;align-items:center">
    <span>${t('Buses the timetable gives this tour:')}</span>
    ${buses.map(({ v, paint, number }) => {
      const on = v.file === c.bus && (c.paint || '') === paint;
      const name = [v.name, paint].filter(Boolean).join(' - ');
      return html`<${Button} small kind=${on ? 'primary' : ''} icon=${on ? 'check' : 'bus'} title=${name} onClick=${() => choose({ bus: v.file, paint, number, hof_manual: false })}><span class="ellipsis">${name}</span></${Button}>`;
    })}
  </div>`;
}

function NextDepartures({ lines }) {
  const c = useStore().choice;
  const now = c.time * 60;
  const upcoming = useMemo(() => {
    const out = [];
    for (const l of lines) for (const x of l.tours) {
      if (!x.runs) continue;
      const trip = x.trips[tripAt(x, now)];
      if (trip && trip.departure >= now - 120 && !depotRun(trip)) out.push({ l, x, trip });
    }
    return out.sort((a, b) => a.trip.departure - b.trip.departure).slice(0, 15);
  }, [lines, now]);
  if (!upcoming.length) return null;
  return html`<details class="next-tours">
    <summary class="section-title">${t('Next departures from {time}', { time: hhmm(c.time) })}</summary>
    <div class="next-list">
      ${upcoming.map(({ l, x, trip }) => html`<button type="button" class=${'trip-row' + (l.name === c.line && tourId(x) === c.tour ? ' on' : '')} onClick=${() => (choose({ line: l.name, tour: tourId(x), start_trip: null }), pickTrip(trip))}>
        <span class="num time">${hms(trip.departure)}</span>
        <span class="line-badge">${trip.line || l.name}</span>
        <span class="grow"><span class="ellipsis"><${TripEnds} trip=${trip} /></span><span class="muted small ellipsis">${t('Tour')} ${x.number}</span></span>
      </button>`)}
    </div>
  </details>`;
}

export function DutyStep() {
  const s = useStore();
  const c = s.choice;
  const m = curMap();
  const lines = (s.lines.list || []).filter((l) => l.user_allowed && l.tours.length);
  const line = curLine();
  const tour = curTour();
  const entries = m ? [[-1, c.free ? 'Automatic (the map\'s first)' : 'Automatic (nearest to the first stop)'], ...m.entry_points.map((e, i) => [i, entryName(e)])] : [];
  let body;
  if (c.free) body = html`<p class="muted">${t('Free driving: the bus starts where you chose, with the traffic and the timetable\'s buses around it, but no line of your own.')}</p>`;
  else if (s.lines.loading && !s.lines.list) body = html`<div class="row muted"><${Spinner} /> ${t('Reading the timetable…')}</div>`;
  else if (s.lines.error) body = html`<${Notice} kind="danger">${s.lines.error}</${Notice}>`;
  else if (!lines.length) body = html`<${Notice}>${t('This map has no lines to drive on this date. Choose a free drive, or another date.')}</${Notice}>`;
  else {
    body = html`<${Lines} lines=${lines} />
      <${Tours} line=${line} />
      ${tour && html`<${TourBus} tour=${tour} />`}
      <${NextDepartures} lines=${lines} />
      ${tour && !tour.runs && html`<${Notice} kind="warn">${tour.next_run ? t('This tour does not run on the chosen date; it runs next on {date}.', { date: longDate(tour.next_run) }) : t('This tour does not run within a year of the chosen date.')}
        ${tour.next_run && !realDate() && html`<div style="margin-top:8px"><${Button} small icon="calendar" label="Drive it on that day" onClick=${() => choose({ date: tour.next_run, season: 'auto', own_date: null })} /></div>`}</${Notice}>`}`;
  }
  return html`<${MapRow} />
    ${m && c.lan_mode !== 'join' && html`<${Notes} kind="map" file=${m.file} />`}
    <${Segmented} full value=${c.free ? 'free' : 'line'} options=${[['line', 'Timetable'], ['free', 'Free drive']]} onChange=${(v) => choose({ free: v === 'free' })} />
    ${body}
    ${m && html`<${Field} label="Start at"><${Select} value=${c.entry} options=${entries} onChange=${(v) => choose({ entry: Number(v) })} /></${Field}>`}
    <${CommandLine} />`;
}

function CommandLine() {
  const c = useStore().choice;
  const [args, setArgs] = useState(null);
  useEffect(() => setArgs(null), [c]);
  const flip = async () => setArgs(args ? null : await call('duty_args', { duty: await duty() }).catch((e) => [e.message]));
  return html`<div class="stack" style="gap:8px">
    <a class="small muted" onClick=${flip}>${args ? t('Hide the command line') : t('Show the command line')}</a>
    ${args && html`<div class="log selectable">${args.map((a) => (/\s/.test(a) ? `"${a}"` : a)).join(' ')}</div>`}
  </div>`;
}

function ibisCodes(info, code, line) {
  return code.length > 2 ? [code.slice(0, -2), code.slice(-2)] : [info.line_code || line, code];
}

function Ibis({ trip }) {
  const s = useStore();
  const x = s.ibis;
  useEffect(() => void loadIbis(trip || null), [trip]);
  if (!x || s.choice.free) return null;
  const i = x.info;
  let body;
  if ('error' in x) body = html`<${Notice} kind="warn">${x.error}</${Notice}>`;
  else if (!i.routes.length) body = html`<p class="muted small">${t('Depot file {hof} has no entries for this line. Type line {line} and the terminus code by hand, or press Shift+U.', { hof: i.hof, line: i.line_code || s.choice.line })}</p>`;
  else {
    body = html`<table class="table ibis">
        <thead><tr><th></th><th>${t('Line')}</th><th>${t('Route code')}</th></tr></thead>
        <tbody>
          ${[...i.routes].sort((a, b) => (b.code === i.trip_route) - (a.code === i.trip_route)).slice(0, 3).map((r) => {
            const [line, route] = ibisCodes(i, r.code, s.choice.line);
            return html`<tr class=${r.code === i.trip_route ? 'on' : ''}>
              <td class="ellipsis" title=${(r.name || r.terminus).trim()}>${(r.name || r.terminus).trim()}</td>
              <td><span class="line-badge">${line}</span></td>
              <td><span class="line-badge">${route}</span></td>
            </tr>`;
          })}
        </tbody>
      </table>
      <div class="muted small">${t('Depot file {hof} | Shift+U in the game types it for you', { hof: i.hof })}</div>`;
  }
  return html`<div class="ibis-box"><div class="section-title">IBIS</div>${body}</div>`;
}

/** The stops of the trip a start takes, a timeline with their times under its row (and the
 *  first one's departure), as the game's own launcher's roadbook had them. */
function TripStops({ stops }) {
  const last = stops.length - 1;
  return html`<ol class="trip-stops">
    ${stops.map((st, k) => html`<li class=${k === 0 || k === last ? 'end' : ''}>
      <span class="num">${hms(k === 0 ? st.dep ?? st.arr : st.arr ?? st.dep)}</span><span class="ellipsis grow">${st.name}</span>${k === 0 && st.dep != null && html`<span class="muted">${t('Departs')}</span>`}
    </li>`)}
  </ol>`;
}

export function Roadbook({ tour, trips, current, now }) {
  const ref = useRef();
  const c = useStore().choice;
  useEffect(() => {
    const el = ref.current;
    const on = el?.querySelector('.on');
    if (on) el.scrollTop += on.getBoundingClientRect().top - el.getBoundingClientRect().top - 60;
  }, [c.line, c.tour, c.time, current]);
  return html`<aside class="roadbook">
    <div class="roadbook-head">
      <div class="section-title">${t('Roadbook')}</div>
      <b class="ellipsis">${t('Tour')} ${tour.number}</b>
      <span class="muted small">${t('Click a trip to start with it')}</span>
    </div>
    <div class="roadbook-trips" ref=${ref}>
      ${trips.map((x, i) => html`<button type="button" class=${'trip-row' + (i === current ? ' on' : '') + (x.arrival < now - 60 && i !== current ? ' past' : '')} onClick=${() => pickTrip(x)}>
        <span class="num time" title=${`${t('Trip duration')}: ${tripTime(x)}`}>${hms(x.departure)}<small>${hms(x.arrival)}</small><small>${tripTime(x)}</small></span>
        <span class="grow" title=${`${x.from} → ${x.terminus}`}>
          <span class="trip-tags">${depotRun(x) ? html`<span class="depot">${t('Depot run')}</span>` : html`<span class="line-badge">${x.line}</span>`}${x.km > 0 && html`<small class="muted">${x.km.toFixed(1)} km</small>`}</span>
          <${TripEnds} trip=${x} stack />
        </span>
      </button>
      ${i === current && x.stops?.length > 0 && html`<${TripStops} stops=${x.stops} />`}`)}
    </div>
    <${Ibis} trip=${trips[current]} />
  </aside>`;
}

export function DutyStage() {
  const s = useStore();
  const c = s.choice;
  const tour = curTour();
  const now = c.time * 60;
  if (!c.map) return html`<div class="stage-empty"><${Icon} name="map" size=${40} /></div>`;
  if (c.free || !tour) {
    return html`<div class="duty-stage alone">
      <${RouteMap} map=${c.map} date=${c.date} trip="" entry=${c.entry} height=${0} onEntry=${(entry) => choose({ entry })} />
    </div>`;
  }
  const trips = tour.trips || [];
  const picked = pickedTrip();
  const current = picked != null ? trips.findIndex((x) => x.index === picked) : tripAt(tour, now);
  const trip = trips[current];
  return html`<div class="duty-stage">
    ${trip
      ? html`<${RouteMap} map=${c.map} date=${c.date} trip=${trip.name} entry=${c.entry} height=${0}
          stops=${trip.stops.map((x) => ({ time: hms(x.dep || x.arr), name: x.name }))} onEntry=${(entry) => choose({ entry })} />`
      : html`<div class="stage-empty"><${Icon} name="route" size=${40} /></div>`}
    <${Roadbook} tour=${tour} trips=${trips} current=${current} now=${now} />
  </div>`;
}
