import { html, useState, useEffect } from '../../vendor/preact-htm.js';
import { useStore } from '../store.js';
import { t } from '../i18n.js';
import { call } from '../api.js';
import { Icon } from '../icons.js';
import { Button, Field, Select, Toggle, Segmented, Slider } from '../ui.js';
import { choose, realTime, realDate, weatherName, weatherOfItsOwn } from '../data.js';
import { WeatherEditor, isCustomWeather, editableWeather } from './weather-editor.js';
import { SEASONS, PHASES, setSeason, localNow, TimeInput, DateInput } from './drive.js';

function WeatherTile({ on, icon, name, sub, onClick }) {
  return html`<button type="button" class=${'weather-tile' + (on ? ' on' : '')} aria-pressed=${on} onClick=${onClick} title=${sub || name}>
    <${Icon} name=${icon} size=${22} />
    <span class="grow"><b>${name}</b>${sub && html`<span>${sub}</span>`}</span>
  </button>`;
}

function Weather() {
  const s = useStore();
  const c = s.choice;
  const w = s.weather || [];
  const [airport, setAirport] = useState('');
  const [airports, setAirports] = useState([]);
  const [editing, setEditing] = useState(null);
  useEffect(() => {
    setAirport('');
    if (c.map) call('nearest_airport', { map: c.map }).then(setAirport).catch(() => {});
  }, [c.map]);
  const metarOn = c.weather?.startsWith('metar:');
  const metar = metarOn ? c.weather.slice(6) || airport : airport;
  useEffect(() => {
    if (metarOn && !airports.length) call('metar_airports').then(setAirports).catch(() => {});
  }, [c.weather]);
  const custom = isCustomWeather(c.weather);
  const other = weatherOfItsOwn(c.weather) && !custom && !metarOn && c.weather !== 'cycle';
  const pick = (weather) => (isCustomWeather(weather) ? setEditing(weather) : choose({ weather }));
  return html`<div class="stack" style="gap:10px">
    <div class="section-title">${t('Weather')}</div>
    <div class="weather-tiles">
      <${WeatherTile} on=${!c.weather} icon="sun" name=${t('Natural weather')} sub=${t('Develops by itself through the day and the season')} onClick=${() => pick('')} />
      <${WeatherTile} on=${custom} icon="sliders" name=${t('Custom weather')} sub=${t('Set visibility, wind, clouds, rain, temperature and road state')} onClick=${() => setEditing(custom ? c.weather : 'custom:')} />
      ${metar && html`<${WeatherTile} on=${metarOn} icon="globe" name=${t('Current weather')} sub=${t('METAR of {} (fetched at the start)', [metar])} onClick=${() => pick(`metar:${metar}`)} />`}
      <${WeatherTile} on=${c.weather === 'cycle'} icon="refresh" name=${t('Weather cycle')} sub=${t('Changes every 25-60 minutes, as the month allows')} onClick=${() => pick('cycle')} />
      ${other && html`<${WeatherTile} on icon="cloud" name=${weatherName(c.weather)} onClick=${() => {}} />`}
      ${w.map((x) => html`<${WeatherTile} on=${c.weather === x.file} icon="cloud" name=${x.name} sub=${x.description} onClick=${() => pick(x.file)} />`)}
    </div>
    ${metarOn && airports.length > 0 && html`<${Field} label="Airport">
      <${Select} raw value=${metar} options=${airports} onChange=${(code) => choose({ weather: `metar:${code}` })} />
    </${Field}>`}
    ${editableWeather(c.weather) && !custom && html`<div><${Button} small icon="sliders" label="Edit selected weather as custom" onClick=${() => setEditing(c.weather)} /></div>`}
    ${editing != null && html`<${WeatherEditor} from=${editing} onClose=${() => setEditing(null)} />`}
  </div>`;
}

export function DayStep() {
  const s = useStore();
  const c = s.choice;
  const rt = realTime();
  const rd = realDate();
  const joined = c.lan_mode === 'join';
  const srv = c.lan_server || {};
  return html`<div class="stack">
    ${joined
      ? html`<div class="stack" style="gap:10px">
          <div class="muted small">${t('Set by {server}', { server: srv.name || c.lan_addr })}</div>
          <dl class="facts">
            <dt>${t('Time')}</dt><dd>${srv.time || '-'}</dd>
            <dt>${t('Weather')}</dt><dd>${srv.weather || t("the map's (the server's)")}</dd>
          </dl>
          <p class="muted small">${t('On a server the map, the time, the date and the weather are the same for everybody: the server keeps the world\'s clock. You choose your bus and your duty.')}</p>
        </div>`
      : html`<div class="grid-2">
          <${Field} label="Start time" help=${rt ? 'Follows the computer\'s clock (Settings)' : null}>
            <div class="row" style="gap:6px">
              <div class="grow"><${TimeInput} minutes=${c.time} disabled=${rt} onChange=${(time) => choose({ time })} /></div>
              ${!rt && html`<${Button} icon="clock" title="Current time" onClick=${() => choose({ time: localNow().time })} />`}
            </div>
          </${Field}>
          <${Field} label="Date" help=${rd ? 'Today (Settings)' : null}>
            <div class="row" style="gap:6px">
              <div class="grow"><${DateInput} value=${c.date} disabled=${rd} onChange=${(date) => choose({ date, season: 'auto', own_date: null })} /></div>
              ${!rd && html`<${Button} icon="calendar" title="Current date" onClick=${() => choose({ date: localNow().date, season: 'auto', own_date: null })} />`}
            </div>
          </${Field}>
        </div>
        <${Field} label="Season">
          <${Segmented} full value=${c.season} options=${SEASONS} onChange=${(v) => setSeason(v, c.phase)} />
        </${Field}>
        ${c.season !== 'auto' && html`<${Segmented} full value=${c.phase} options=${PHASES} onChange=${(p) => setSeason(c.season, p)} />`}`}
    <${Slider} label="Cars around" value=${c.traffic} min=${0} max=${120} step=${1} onChange=${(traffic) => choose({ traffic })} format=${(v) => (v ? String(Math.round(v)) : t('None'))} />
    <div class="toggles-2">
      ${[
        ['passengers', 'Passengers', 'People wait at the stops and ride along'],
        ['schedule', 'Timetable buses', 'Other buses of the timetable drive their lines'],
        ['autostart', 'Bus ready to go', 'Engine running, doors and lights set'],
        ['on_foot', 'Start on foot', 'Walk to the bus, or place one later from the menu'],
      ].map(([key, label, help]) => html`<div title=${t(help)}><${Toggle} checked=${c[key]} onChange=${(v) => choose({ [key]: v })} label=${label} /></div>`)}
    </div>
    ${!joined && html`<${Weather} />`}
  </div>`;
}
