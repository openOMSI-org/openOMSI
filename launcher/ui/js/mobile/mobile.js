// The launcher on a phone: a layout of its own for a landscape phone and a finger, on the
// same data and commands as the desktop launcher. A rail of icons on the left; Drive is the
// bus turning on the left half with the Start button over it, and the duty as a list of rows
// on the right, each opening a full-screen sheet to choose from.

import { html, useState, useEffect } from '../../vendor/preact-htm.js';
import { S, useStore, set } from '../store.js';
import { t, hhmm, longDate } from '../i18n.js';
import { Icon } from '../icons.js';
import { Spinner, Toggle, Slider } from '../ui.js';
import { BusPreview } from '../preview.js';
import { RouteMap } from '../routemap.js';
import { choose, curMap, curBus, curLine, curTour, pickedTrip, realTime, realDate, start } from '../data.js';
import { hms, mapPicture, tripAt, depotRun, pickTrip, setSeason, localNow, SEASONS, PHASES } from '../pages/drive.js';
import { MultiplayerPage } from '../pages/multiplayer.js';
import { ProfilePage } from '../pages/profile.js';
import { ModsPage } from '../pages/mods.js';
import { SettingsPage } from '../pages/settings.js';
import { GamesPage } from '../pages/games.js';
import { TutorialsPage } from '../pages/tutorials.js';
import { SetupPage } from '../pages/setup.js';

const TABS = [
  ['drive', 'Drive', 'bus'],
  ['multiplayer', 'Multiplayer', 'users'],
  ['profile', 'Profile', 'user'],
  ['mods', 'Mods', 'puzzle'],
  ['settings', 'Settings', 'sliders'],
  ['more', 'More', 'help'],
];

export function MobileApp() {
  const s = useStore();
  const page = TABS.some((x) => x[0] === s.page) || ['games', 'tutorials', 'setup'].includes(s.page) ? s.page : 'drive';
  const Body = { drive: MobileDrive, multiplayer: MultiplayerPage, profile: ProfilePage, mods: ModsPage, settings: SettingsPage, more: More, games: GamesPage, tutorials: TutorialsPage, setup: SetupPage }[page];
  const running = s.poll.instances?.length || 0;
  return html`<div class="m-shell">
    <nav class="m-rail">
      ${TABS.map(([id, label, icon]) => html`<button class=${'m-tab' + (page === id || (id === 'more' && ['games', 'tutorials', 'setup'].includes(page)) ? ' on' : '')} onClick=${() => set({ page: id })}>
        <${Icon} name=${icon} size=${22} /><span>${t(label)}</span>${id === 'more' && running > 0 && html`<i class="m-dot"></i>`}
      </button>`)}
    </nav>
    <main class=${'m-main' + (page === 'drive' ? ' m-flush' : '')}>${html`<${Body} />`}</main>
  </div>`;
}

function More() {
  const s = useStore();
  const running = s.poll.instances?.length || 0;
  const item = (id, label, icon, note) => html`<button class="m-row" onClick=${() => set({ page: id })}>
    <span class="m-row-icon"><${Icon} name=${icon} /></span><span class="m-row-label">${t(label)}</span>${note && html`<span class="m-row-value">${note}</span>`}<${Icon} name="right" />
  </button>`;
  return html`<div class="m-page">
    <h1 class="m-title">${t('More')}</h1>
    <div class="m-group">
      ${item('games', 'Running games', 'gamepad', running ? String(running) : null)}
      ${item('tutorials', 'Tutorials', 'book')}
      ${item('setup', 'Setup', 'folder')}
    </div>
    <p class="m-foot">openOMSI ${s.boot?.version || ''}</p>
  </div>`;
}

// --- Drive ---------------------------------------------------------------------------------

function MobileDrive() {
  const s = useStore();
  const [sheet, setSheet] = useState(null);
  const [busy, setBusy] = useState(false);
  const c = s.choice;
  if (!c) return null;
  const m = curMap();
  const v = curBus();
  const line = curLine();
  const tour = curTour();
  const trips = tour?.trips || [];
  const picked = pickedTrip();
  const trip = c.free ? null : picked != null ? trips.find((x) => x.index === picked) : trips[tripAt(tour || { trips: [] }, c.time * 60)];
  const ready = m && v && (c.free || (line && tour));
  const weather = s.weather?.find((x) => x.file === c.weather);
  const go = async (extra) => {
    setBusy(true);
    await start(extra);
    setBusy(false);
  };
  const row = (id, icon, label, value, sub) => html`<button class="m-row" onClick=${() => setSheet(id)}>
    <span class="m-row-icon"><${Icon} name=${icon} /></span>
    <span class="m-row-label">${t(label)}${sub && html`<small>${sub}</small>`}</span>
    <span class="m-row-value">${value}</span><${Icon} name="right" />
  </button>`;
  return html`<div class="m-drive">
    <section class="m-stage">
      ${v ? html`<${BusPreview} class="m-preview" look=${{ bus: v.file, paint: c.paint, map: c.map, weather: c.weather, time: c.time, date: c.date }} />` : html`<div class="viewer m-preview"><div class="state"><${Spinner} /></div></div>`}
      <div class="m-chips">
        <button class="m-chip" onClick=${() => setSheet('map')}><${Icon} name="map" size=${16} />${m ? m.friendly || m.name : '-'}</button>
        <button class="m-chip" onClick=${() => setSheet('time')}><${Icon} name="clock" size=${16} />${hhmm(c.time)}</button>
      </div>
      <div class="m-go">
        ${trip && !depotRun(trip) ? html`<div class="sign"><span class="no">${trip.line}</span><span class="to">${trip.terminus}</span></div>` : c.free && html`<div class="sign"><span class="no"><${Icon} name="route" /></span><span class="to">${t('Free drive')}</span></div>`}
        <button class="m-start" disabled=${!ready || busy} onClick=${() => go()}>
          <${Icon} name="play" size=${24} />${busy ? t('Starting…') : c.free ? t('Start driving') : t('Start the duty')}
        </button>
      </div>
    </section>
    <section class="m-list">
      <div class="m-group">
        ${row('map', 'map', 'Map', m ? m.friendly || m.name : '-')}
        ${row('bus', 'bus', 'Bus', v ? v.name : s.vehicles || !s.boot?.root ? '-' : html`<${Spinner} />`)}
        ${v && row('paint', 'sun', 'Paint', c.paint || v.default_paint || t('Standard'))}
      </div>
      <div class="m-seg">
        <button class=${!c.free ? 'on' : ''} onClick=${() => choose({ free: false })}>${t('Timetable')}</button>
        <button class=${c.free ? 'on' : ''} onClick=${() => choose({ free: true })}>${t('Free drive')}</button>
      </div>
      <div class="m-group">
        ${!c.free && row('line', 'route', 'Line', line ? line.name : s.lines.loading ? html`<${Spinner} />` : '-')}
        ${!c.free && row('tour', 'calendar', 'Tour', tour ? `${tour.number} - ${tour.days}` : '-')}
        ${!c.free && row('trip', 'history', 'First trip', trip ? `${hms(trip.departure)} ${trip.from}` : '-')}
        ${row('entry', 'person', 'Start at', c.entry < 0 ? t('Automatic') : m?.entry_points[c.entry]?.name || '-')}
      </div>
      <div class="m-group">
        ${row('time', 'clock', 'Time and date', `${hhmm(c.time)} - ${c.date}`, rt(c))}
        ${row('weather', 'cloud', 'Weather', weather?.name || t('Natural weather'))}
      </div>
      <div class="m-group m-pad">
        <${Slider} label="Cars around" value=${c.traffic} min=${0} max=${120} step=${1} onChange=${(traffic) => choose({ traffic })} format=${(x) => (x ? String(Math.round(x)) : t('None'))} />
        <${Toggle} checked=${c.passengers} onChange=${(passengers) => choose({ passengers })} label="Passengers" />
        <${Toggle} checked=${c.schedule} onChange=${(schedule) => choose({ schedule })} label="Timetable buses" />
        <${Toggle} checked=${c.autostart} onChange=${(autostart) => choose({ autostart })} label="Bus ready to go" />
        <${Toggle} checked=${c.on_foot} onChange=${(on_foot) => choose({ on_foot })} label="Start on foot" />
      </div>
      ${s.situations?.length > 0 && html`<div class="m-group">
        ${s.situations.slice(0, 3).map((x) => html`<button class="m-row" onClick=${() => !busy && go({ situation: x.file })}>
          <span class="m-row-icon"><${Icon} name="history" /></span><span class="m-row-label">${t(x.name)}</span><${Icon} name="play" />
        </button>`)}
      </div>`}
    </section>
    ${sheet && html`<${Sheet} which=${sheet} close=${() => setSheet(null)} />`}
  </div>`;
}

const rt = (c) => (realTime() || realDate() ? t('Follows the computer\'s clock (Settings)') : null);

// --- the sheets ----------------------------------------------------------------------------

/** A full-screen sheet: a title bar with Back, and what to choose from. */
function Sheet({ which, close }) {
  const titles = { map: 'Choose a map', bus: 'Choose a bus', paint: 'Paint', line: 'Line', tour: 'Tour', trip: 'Trips of the tour', entry: 'Start at', time: 'Time and date', weather: 'Weather' };
  const Body = { map: MapSheet, bus: BusSheet, paint: PaintSheet, line: LineSheet, tour: TourSheet, trip: TripSheet, entry: EntrySheet, time: TimeSheet, weather: WeatherSheet }[which];
  useEffect(() => {
    // the phone's Back closes the sheet
    const k = (e) => (e.key === 'Escape' || e.key === 'GoBack' || e.key === 'BrowserBack') && close();
    window.addEventListener('keydown', k);
    // (the chosen one in view: the trip of a tour of seventy is far down the list)
    setTimeout(() => document.querySelector('.m-sheet .m-pick.on')?.scrollIntoView({ block: 'center' }), 0);
    return () => window.removeEventListener('keydown', k);
  }, []);
  return html`<div class="m-sheet">
    <header><button class="m-back" onClick=${close}><${Icon} name="left" size=${24} /></button><h2>${t(titles[which])}</h2></header>
    <div class="m-sheet-body"><${Body} close=${close} /></div>
  </div>`;
}

const pick = (on, onClick, children, extra = '') => html`<button class=${'m-pick' + (on ? ' on' : '') + extra} onClick=${onClick}>${children}${on && html`<span class="m-check"><${Icon} name="check" /></span>`}</button>`;

function MapSheet({ close }) {
  const s = useStore();
  return html`<div class="m-tiles">
    ${(s.maps || []).map((m) => html`<button class=${'m-tile' + (m.file === s.choice.map ? ' on' : '')} onClick=${() => (choose({ map: m.file, entry: -1, start_trip: null }), close())}>
      <span class="m-tile-pic" style=${`background-image:url("${mapPicture(m.file)}")`}></span>
      <b>${m.friendly || m.name}</b><small>${t('{n} entry points', { n: m.entry_points.length })}</small>
    </button>`)}
  </div>`;
}

function BusSheet({ close }) {
  const s = useStore();
  const [q, setQ] = useState('');
  const list = (s.vehicles || []).filter((v) => !q || `${v.name} ${v.manufacturer}`.toLowerCase().includes(q.toLowerCase())).sort((a, b) => (a.manufacturer || '').localeCompare(b.manufacturer || '') || a.name.localeCompare(b.name));
  return html`<div>
    <input class="m-search" value=${q} placeholder=${t('Search buses')} onInput=${(e) => setQ(e.currentTarget.value)} />
    ${list.map((v) => pick(v.file === s.choice.bus, () => (choose({ bus: v.file, paint: '', number: '', hof_manual: false }), close()), html`<span class="m-pick-text"><b>${v.name}</b><small>${[v.manufacturer, v.paints.length ? t('{n} paints', { n: v.paints.length + 1 }) : null].filter(Boolean).join(' - ')}</small></span>`))}
  </div>`;
}

function PaintSheet({ close }) {
  const s = useStore();
  const v = curBus();
  if (!v) return null;
  return html`<div>${[['', v.default_paint ? `${t('Standard')} (${v.default_paint})` : t('Standard')], ...v.paints.map((p) => [p, p])].map(([p, label]) => pick(s.choice.paint === p, () => (choose({ paint: p }), close()), html`<span class="m-pick-text"><b>${label}</b></span>`))}</div>`;
}

function LineSheet({ close }) {
  const s = useStore();
  const lines = (s.lines.list || []).filter((l) => l.user_allowed && l.tours.length);
  return html`<div>${lines.map((l) => pick(l.name === s.choice.line, () => {
    const tr = l.tours.find((x) => x.runs) || l.tours[0];
    choose({ line: l.name, tour: tr?.number ?? null, start_trip: null });
    close();
  }, html`<span class="line-badge">${l.name}</span><span class="m-pick-text"><b>${l.termini.join(' - ')}</b><small>${t('{n} tours', { n: l.tours.length })}</small></span>`))}</div>`;
}

function TourSheet({ close }) {
  const s = useStore();
  const line = curLine();
  if (!line) return null;
  return html`<div>${line.tours.map((x) => pick(x.number === s.choice.tour, () => {
    const patch = { tour: x.number, start_trip: null };
    if (!x.runs && x.next_run && !realDate()) patch.date = x.next_run;
    choose(patch);
    close();
  }, html`<span class="m-pick-text"><b>${t('Tour')} ${x.number}</b><small>${x.days} - ${x.trips.length} ${t('trips')}${x.trips.length ? ` - ${hms(x.first)}-${hms(x.last)}` : ''}${!x.runs ? ` - ${t('not on this day')}` : ''}</small></span>`, x.runs ? '' : ' dim'))}</div>`;
}

function TripSheet({ close }) {
  const s = useStore();
  const c = s.choice;
  const tour = curTour();
  if (!tour) return null;
  const picked = pickedTrip();
  const cur = picked != null ? tour.trips.findIndex((x) => x.index === picked) : tripAt(tour, c.time * 60);
  const trip = tour.trips[cur];
  return html`<div class="m-split">
    <div class="m-split-list">
      ${tour.trips.map((x, i) => pick(i === cur, () => pickTrip(x), html`<span class="m-time">${hms(x.departure)}</span>${depotRun(x) ? html`<span class="muted">${t('Depot run')}</span>` : html`<span class="line-badge">${x.line}</span>`}<span class="m-pick-text"><b>${x.terminus}</b><small>${t('From')} ${x.from} - ${hms(x.arrival)}</small></span>`))}
    </div>
    <div class="m-split-map">
      ${trip && html`<${RouteMap} map=${c.map} date=${c.date} trip=${trip.name} entry=${c.entry} height=${0} stops=${trip.stops.map((x) => ({ time: hms(x.dep || x.arr), name: x.name }))} onEntry=${(entry) => choose({ entry })} />`}
    </div>
  </div>`;
}

function EntrySheet({ close }) {
  const s = useStore();
  const c = s.choice;
  const m = curMap();
  if (!m) return null;
  return html`<div class="m-split">
    <div class="m-split-list">
      ${pick(c.entry < 0, () => (choose({ entry: -1 }), close()), html`<span class="m-pick-text"><b>${t('Automatic')}</b><small>${t(c.free ? "Automatic (the map's first)" : 'Automatic (nearest to the first stop)')}</small></span>`)}
      ${m.entry_points.map((e, i) => pick(c.entry === i, () => (choose({ entry: i }), close()), html`<span class="m-pick-text"><b>${e.name}</b></span>`))}
    </div>
    <div class="m-split-map"><${RouteMap} map=${c.map} date=${c.date} trip="" entry=${c.entry} height=${0} onEntry=${(entry) => choose({ entry })} /></div>
  </div>`;
}

function TimeSheet() {
  const s = useStore();
  const c = s.choice;
  const step = (min) => choose({ time: (c.time + min + 1440) % 1440 });
  const day = (d) => {
    const x = new Date(c.date + 'T12:00:00Z');
    x.setUTCDate(x.getUTCDate() + d);
    choose({ date: x.toISOString().slice(0, 10), season: 'auto', own_date: null });
  };
  return html`<div class="m-time-sheet">
    <div class="m-dial">
      <button onClick=${() => step(-60)} disabled=${realTime()}>-1 h</button><button onClick=${() => step(-5)} disabled=${realTime()}>-5</button>
      <b class="num">${hhmm(c.time)}</b>
      <button onClick=${() => step(5)} disabled=${realTime()}>+5</button><button onClick=${() => step(60)} disabled=${realTime()}>+1 h</button>
      ${!realTime() && html`<button title=${t('Current time')} onClick=${() => choose({ time: localNow().time })}><${Icon} name="clock" /></button>`}
    </div>
    <div class="m-dial">
      <button onClick=${() => day(-7)} disabled=${realDate()}>-7</button><button onClick=${() => day(-1)} disabled=${realDate()}>-1</button>
      <b>${longDate(c.date)}</b>
      <button onClick=${() => day(1)} disabled=${realDate()}>+1</button><button onClick=${() => day(7)} disabled=${realDate()}>+7</button>
      ${!realDate() && html`<button title=${t('Current date')} onClick=${() => choose({ date: localNow().date, season: 'auto', own_date: null })}><${Icon} name="calendar" /></button>`}
    </div>
    <div class="m-seg">${SEASONS.map(([v, l]) => html`<button class=${c.season === v ? 'on' : ''} onClick=${() => setSeason(v, c.phase)}>${t(l)}</button>`)}</div>
    ${c.season !== 'auto' && html`<div class="m-seg">${PHASES.map(([v, l]) => html`<button class=${c.phase === v ? 'on' : ''} onClick=${() => setSeason(c.season, v)}>${t(l)}</button>`)}</div>`}
  </div>`;
}

function WeatherSheet({ close }) {
  const s = useStore();
  return html`<div>
    ${pick(!s.choice.weather, () => (choose({ weather: '' }), close()), html`<span class="m-pick-text"><b>${t('Natural weather')}</b><small>${t('Develops by itself through the day and the season')}</small></span>`)}
    ${(s.weather || []).map((w) => pick(s.choice.weather === w.file, () => (choose({ weather: w.file }), close()), html`<span class="m-pick-text"><b>${w.name}</b><small>${Math.round(w.temp)} °C - ${w.precip} - ${w.fog_m >= 20000 ? t('clear air') : `${Math.round(w.fog_m)} m`}</small></span>`))}
  </div>`;
}
