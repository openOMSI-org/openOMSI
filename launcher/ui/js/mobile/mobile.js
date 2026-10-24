import { CabinTemperatureOption } from '../pages/drive-day.js';
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
import { tourId, choose, curMap, curBus, curLine, curTour, pickedTrip, realTime, realDate, start, weatherName, weatherOfItsOwn, askJoined, noServerBus, mapName, serverBuses, busFile } from '../data.js';
import { hms, mapPicture, tripAt, depotRun, pickTrip, lineMatches, findTours, chooseLine, chooseTour, setSeason, localNow, SEASONS, PHASES, TripEnds, tripTime, weatherFacts, entryName } from '../pages/drive.js';
import { MultiplayerPage } from '../pages/multiplayer.js';
import { ProfilePage } from '../pages/profile.js';
import { ModsPage } from '../pages/mods.js';
import { TroubleshootPage, problems } from '../pages/troubleshoot.js';
import { SettingsPage } from '../pages/settings.js';
import { ControlsPage } from '../pages/controls.js';
import { GamesPage } from '../pages/games.js';
import { TutorialsPage } from '../pages/tutorials.js';
import { SetupPage } from '../pages/setup.js';
import { AddonsPage } from '../pages/addons.js';

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
  // (the keys and the game controllers open from Settings, #2308: they fell back to Drive)
  const page = TABS.some((x) => x[0] === s.page) || ['games', 'tutorials', 'setup', 'controls', 'addons', 'troubleshoot'].includes(s.page) ? s.page : 'drive';
  const Body = { drive: MobileDrive, multiplayer: MultiplayerPage, profile: ProfilePage, mods: ModsPage, troubleshoot: TroubleshootPage, settings: SettingsPage, more: More, games: GamesPage, tutorials: TutorialsPage, setup: SetupPage, controls: ControlsPage, addons: AddonsPage }[page];
  const running = s.poll.instances?.length || 0;
  return html`<div class="m-shell">
    <nav class="m-rail">
      ${TABS.map(([id, label, icon]) => html`<button class=${'m-tab' + (page === id || (id === 'more' && ['games', 'tutorials', 'setup', 'addons', 'troubleshoot'].includes(page)) || (id === 'settings' && page === 'controls') ? ' on' : '')} onClick=${() => set({ page: id })}>
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
      ${item('troubleshoot', 'Troubleshooting', 'alert', problems(s).length ? String(problems(s).length) : null)}
      ${item('tutorials', 'Tutorials', 'book')}
      ${item('setup', 'Setup', 'folder')}
      ${item('addons', 'Add-ons', 'globe')}
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
  useEffect(() => {
    askJoined();
  }, [c?.lan_mode, c?.lan_addr]);
  if (!c) return null;
  const joined = c.lan_mode === 'join';
  const srv = c.lan_server || {};
  const m = curMap();
  const v = curBus();
  const line = curLine();
  const tour = curTour();
  const trips = tour?.trips || [];
  const picked = pickedTrip();
  const trip = c.free ? null : picked != null ? trips.find((x) => x.index === picked) : trips[tripAt(tour || { trips: [] }, c.time * 60)];
  const ready = m && v && (c.free || (line && tour)) && !noServerBus();
  const mapLabel = joined && c.lan_map ? mapName(c.lan_map.replace(/\\/g, '/')) : m ? m.friendly || m.name : '-';
  const go = async (extra) => {
    setBusy(true);
    await start(extra);
    setBusy(false);
  };
  const row = (id, icon, label, value, sub, locked) => html`<button class="m-row" disabled=${locked} onClick=${() => !locked && setSheet(id)}>
    <span class="m-row-icon"><${Icon} name=${icon} /></span>
    <span class="m-row-label">${t(label)}${sub && html`<small>${sub}</small>`}</span>
    <span class="m-row-value">${value}</span>${!locked && html`<${Icon} name="right" />`}
  </button>`;
  return html`<div class="m-drive">
    <section class="m-stage">
      ${v ? html`<${BusPreview} class="m-preview" look=${{ bus: v.file, paint: c.paint, map: c.map, weather: c.weather, time: c.time, date: c.date }} />` : html`<div class="viewer m-preview"><div class="state"><${Spinner} /></div></div>`}
      <div class="m-chips">
        <button class="m-chip" disabled=${joined} onClick=${() => setSheet('map')}><${Icon} name="map" size=${16} />${mapLabel}</button>
        <button class="m-chip" disabled=${joined} onClick=${() => setSheet('time')}><${Icon} name="clock" size=${16} />${joined ? srv.time || '-' : hhmm(c.time)}</button>
      </div>
      <div class="m-go">
        ${trip && !depotRun(trip) ? html`<div class="sign"><span class="no">${trip.line}</span><span class="to">${trip.terminus}</span></div>` : c.free && html`<div class="sign"><span class="no"><${Icon} name="route" /></span><span class="to">${t('Free drive')}</span></div>`}
        <button class="m-start" disabled=${!ready || busy} onClick=${() => go()}>
          <${Icon} name="play" size=${24} />${busy ? t('Starting…') : joined ? t('Join and drive') : c.free ? t('Start driving') : t('Start the duty')}
        </button>
      </div>
    </section>
    <section class="m-list">
      ${joined && html`<div class="m-group">
        <button class="m-row" onClick=${() => choose({ lan_mode: 'off' })}>
          <span class="m-row-icon"><${Icon} name="users" /></span>
          <span class="m-row-label">${t('Joining {server}', { server: srv.name || c.lan_addr })}${noServerBus() && html`<small>${t('The server offers none of the vehicles installed here')}</small>`}</span>
          <span class="m-row-value">${t('Leave the server')}</span>
        </button>
      </div>`}
      ${c.lan_mode === 'host' && html`<div class="m-group">
        <button class="m-row" onClick=${() => choose({ lan_mode: 'off' })}>
          <span class="m-row-icon"><${Icon} name="users" /></span>
          <span class="m-row-label">${t('Host my next duty')}<small>${t('Start the duty to create your session, then return to Multiplayer to copy the code.')}</small></span>
          <span class="m-row-value">${t('Cancel')}</span>
        </button>
      </div>`}
      <div class="m-group">
        ${row('map', 'map', 'Map', mapLabel, joined ? t('Set by {server}', { server: srv.name || c.lan_addr }) : null, joined)}
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
        ${row('entry', 'person', 'Start at', c.entry < 0 ? t('Automatic') : (m?.entry_points[c.entry] ? entryName(m.entry_points[c.entry]) : '-'))}
      </div>
      <div class="m-group">
        ${row('time', 'clock', 'Time and date', joined ? srv.time || '-' : `${hhmm(c.time)} - ${c.date}`, joined ? t("(the server's clock)") : rt(c), joined)}
        ${row('weather', 'cloud', 'Weather', joined ? srv.weather || t("the map's (the server's)") : weatherName(c.weather, s.weather), null, joined)}
      </div>
      <div class="m-group m-pad">
        <${Slider} label="Cars around" value=${c.traffic} min=${0} max=${120} step=${1} onChange=${(traffic) => choose({ traffic })} format=${(x) => (x ? String(Math.round(x)) : t('None'))} />
        <${Toggle} checked=${c.passengers} onChange=${(passengers) => choose({ passengers })} label="Passengers" />
        <${Toggle} checked=${c.schedule} onChange=${(schedule) => choose({ schedule })} label="Timetable buses" />
        <${Toggle} checked=${c.autostart} onChange=${(autostart) => choose({ autostart })} label="Bus ready to go" />
        <${CabinTemperatureOption} />
        <${Toggle} checked=${c.on_foot} onChange=${(on_foot) => choose({ on_foot })} label="Start on foot" />
      </div>
      ${!joined && s.situations?.length > 0 && html`<div class="m-group">
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
  const allowed = serverBuses(s.choice);
  const list = (s.vehicles || []).filter((v) => !allowed || allowed.has(busFile(v.file))).filter((v) => !q || `${v.name} ${v.manufacturer}`.toLowerCase().includes(q.toLowerCase())).sort((a, b) => (a.manufacturer || '').localeCompare(b.manufacturer || '') || a.name.localeCompare(b.name));
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
  const [q, setQ] = useState('');
  const lines = (s.lines.list || []).filter((l) => l.user_allowed && l.tours.length && lineMatches(l, q.trim().toLowerCase()));
  return html`<div>
    <input class="m-search" value=${q} placeholder=${t('Filter lines...')} onInput=${(e) => setQ(e.currentTarget.value)} />
    ${lines.length === 0 && html`<p class="muted small" style="padding:8px">${t('No line matches.')}</p>`}
    ${lines.map((l) => pick(l.name === s.choice.line, () => (chooseLine(l), close()), html`<span class="line-badge">${l.name}</span><span class="m-pick-text"><b>${l.termini.join(' - ')}</b><small>${t('{n} tours', { n: l.tours.length })}</small></span>`))}
  </div>`;
}

function TourSheet({ close }) {
  const s = useStore();
  const [q, setQ] = useState('');
  const [showEnded, setShowEnded] = useState(false);
  const line = curLine();
  if (!line) return null;
  const tq = q.trim().toLowerCase();
  const { shown, endedCount } = findTours(line, s.choice.time * 60, tq, showEnded, s.choice.tour);
  return html`<div>
    <input class="m-search" value=${q} placeholder=${t('Filter tours: number, route, stop...')} onInput=${(e) => setQ(e.currentTarget.value)} />
    ${endedCount > 0 && html`<div style="padding:4px 4px 10px"><${Toggle} checked=${showEnded} onChange=${setShowEnded} label=${t('Show ended tours ({n})', { n: endedCount })} /></div>`}
    ${shown.length === 0 && html`<p class="muted small" style="padding:8px">${t(tq ? 'No tour has a trip still to come that matches.' : 'No tour left at this time: show the ended tours, or start earlier.')}</p>`}
    ${shown.map(({ tour: x, ended, at, via }) => {
      const trip = x.trips[at];
      const found = tq && trip && !x.number.toLowerCase().includes(tq);
      const what = found ? html`${hms(trip.departure)} <${TripEnds} trip=${trip} />${via ? ` (${t('via {stop}', { stop: via })})` : ''}` : `${x.days} - ${t('{n} trips', { n: x.trips.length })}${x.trips.length ? ` - ${hms(x.first)}-${hms(x.last)}` : ''}`;
      return pick(tourId(x) === s.choice.tour, () => (chooseTour(x, found ? trip : null), close()), html`<span class="m-pick-text"><b>${t('Tour')} ${x.number}</b><small>${what}${!x.runs ? ` - ${t('not on this day')}` : ended ? ` - ${t('ended')}` : ''}</small></span>`, x.runs && !ended ? '' : ' dim');
    })}
  </div>`;
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
      ${tour.trips.map((x, i) => pick(i === cur, () => pickTrip(x), html`<span class="m-time">${hms(x.departure)}<small>${hms(x.arrival)}</small><small>${tripTime(x)}</small></span>${depotRun(x) ? html`<span class="muted">${t('Depot run')}</span>` : html`<span class="line-badge">${x.line}</span>`}<span class="m-pick-text"><${TripEnds} trip=${x} stack /></span>`))}
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
      ${m.entry_points.map((e, i) => pick(c.entry === i, () => (choose({ entry: i }), close()), html`<span class="m-pick-text"><b>${entryName(e)}</b></span>`))}
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
      <button title=${t('Current time')} onClick=${() => choose({ time: localNow().time })} disabled=${realTime()}><${Icon} name="clock" /></button>
    </div>
    <div class="m-dial">
      <button onClick=${() => day(-7)} disabled=${realDate()}>-7</button><button onClick=${() => day(-1)} disabled=${realDate()}>-1</button>
      <b>${longDate(c.date)}</b>
      <button onClick=${() => day(1)} disabled=${realDate()}>+1</button><button onClick=${() => day(7)} disabled=${realDate()}>+7</button>
      <button title=${t('Current date')} onClick=${() => choose({ date: localNow().date, season: 'auto', own_date: null })} disabled=${realDate()}><${Icon} name="calendar" /></button>
    </div>
    <div class="m-seg">${SEASONS.map(([v, l]) => html`<button class=${c.season === v ? 'on' : ''} onClick=${() => setSeason(v, c.phase)}>${t(l)}</button>`)}</div>
    ${c.season !== 'auto' && html`<div class="m-seg">${PHASES.map(([v, l]) => html`<button class=${c.phase === v ? 'on' : ''} onClick=${() => setSeason(c.season, v)}>${t(l)}</button>`)}</div>`}
  </div>`;
}

function WeatherSheet({ close }) {
  const s = useStore();
  return html`<div>
    ${pick(!s.choice.weather, () => (choose({ weather: '' }), close()), html`<span class="m-pick-text"><b>${t('Natural weather')}</b><small>${t('Develops by itself through the day and the season')}</small></span>`)}
    ${weatherOfItsOwn(s.choice.weather) && pick(true, close, html`<span class="m-pick-text"><b>${weatherName(s.choice.weather)}</b></span>`)}
    ${(s.weather || []).map((w) => pick(s.choice.weather === w.file, () => (choose({ weather: w.file }), close()), html`<span class="m-pick-text"><b>${w.name}</b><small>${weatherFacts(w)}</small></span>`))}
  </div>`;
}
