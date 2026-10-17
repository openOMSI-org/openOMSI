import { html, useState, useEffect, useRef } from '../../vendor/preact-htm.js';
import { useStore, remembered, remember } from '../store.js';
import { t } from '../i18n.js';
import { Icon } from '../icons.js';
import { Button, Select, Dialog, Notice, Spinner } from '../ui.js';
import { choose, curBus, curTour, pickedTrip, weatherName, realTime, realDate, askJoined, noServerBus, depotBusOf } from '../data.js';
import { BusPreview } from '../preview.js';
import { RouteMap } from '../routemap.js';
import { BusList, BusOptions, withoutMaker } from './bus-list.js';
import { BusPicker } from './bus-picker.js';
import { DayStep } from './drive-day.js';
import { DutyStep, Roadbook } from './drive-duty.js';
import { StartBar, BusStage } from './drive-classic.js';
import { hms, tripAt, TimeInput, DateInput, localNow } from './drive.js';

export function LayoutDrive({ layout }) {
  const s = useStore();
  const c = s.choice;
  useEffect(() => {
    askJoined();
  }, [c?.lan_mode, c?.lan_addr]);
  if (!c) return null;
  const Page = layout === 'control' ? ControlRoom : layout === 'classicplus' ? ClassicPlus : BigMap;
  return html`<h1 class="sr-only">${t('Drive')}</h1>
    ${s.errors.maps && html`<${Notice} kind="danger">${s.errors.maps}</${Notice}>`}
    <${Page} />`;
}

function useDuty() {
  const c = useStore().choice;
  const tour = curTour();
  const now = c.time * 60;
  const trips = tour?.trips || [];
  const picked = pickedTrip();
  const current = tour ? (picked != null ? trips.findIndex((x) => x.index === picked) : tripAt(tour, now)) : -1;
  return { c, tour, trips, current, now, trip: trips[current] };
}

function TripMap({ inset }) {
  const { c, trip } = useDuty();
  if (!c.map) return html`<div class="stage-empty"><${Icon} name="map" size=${40} /></div>`;
  const on = !c.free && trip;
  return html`<${RouteMap} map=${c.map} date=${c.date} trip=${on ? trip.name : ''} entry=${c.entry} height=${0} inset=${inset}
    stops=${on ? trip.stops.map((x) => ({ time: hms(x.dep || x.arr), name: x.name })) : []} onEntry=${(entry) => choose({ entry })} />`;
}

function Book() {
  const { c, tour, trips, current, now } = useDuty();
  if (c.free || !tour) return null;
  return html`<${Roadbook} tour=${tour} trips=${trips} current=${current} now=${now} />`;
}

function weatherOptions(s) {
  const c = s.choice;
  const out = [['', t('Natural weather')], ['cycle', t('Weather cycle')], ['tomorrow', 'Tomorrow.io'], ...(s.weather || []).map((x) => [x.file, x.name])];
  if (c.weather && !out.some(([f]) => f === c.weather)) out.push([c.weather, weatherName(c.weather, s.weather)]);
  return out;
}

function DayCompact() {
  const s = useStore();
  const c = s.choice;
  const [open, setOpen] = useState(false);
  const joined = c.lan_mode === 'join';
  return html`<div class="day-compact">
    <div class="dc-pair time">
      <label class="dc-field"><span>${t('Start time')}</span><${TimeInput} minutes=${c.time} disabled=${realTime() || joined} onChange=${(time) => choose({ time })} /></label>
      <${Button} icon="clock" title="Current time" disabled=${realTime() || joined} onClick=${() => choose({ time: localNow().time })} />
    </div>
    <div class="dc-pair date">
      <label class="dc-field"><span>${t('Date')}</span><${DateInput} value=${c.date} disabled=${realDate() || joined} onChange=${(date) => choose({ date, season: 'auto', own_date: null })} /></label>
      <${Button} icon="calendar" title="Current date" disabled=${realDate() || joined} onClick=${() => choose({ date: localNow().date, season: 'auto', own_date: null })} />
    </div>
    <label class="dc-field weather"><span>${t('Weather')}</span>${joined
      ? html`<span class="input dc-fixed ellipsis">${c.lan_server?.weather || t("the map's (the server's)")}</span>`
      : html`<${Select} raw value=${c.weather || ''} options=${weatherOptions(s)} onChange=${(weather) => choose({ weather })} />`}</label>
    <div class="dc-more"><${Button} icon="sliders" title="Day & weather" onClick=${() => setOpen(true)} /></div>
    ${open && html`<${Dialog} wide title="Day & weather" onClose=${() => setOpen(false)}><div class="day-dialog"><${DayStep} /></div></${Dialog}>`}
  </div>`;
}

function look(v, c) {
  return { bus: v.file, paint: c.paint, map: c.map, weather: c.weather, time: c.time, date: c.date };
}

function BusThumb({ compact }) {
  const s = useStore();
  const c = s.choice;
  const v = curBus();
  const [pick, setPick] = useState(false);
  const [more, setMore] = useState(false);
  if (!s.vehicles) return html`<div class="row muted bus-thumb-wait"><${Spinner} /> ${s.vehiclesProgress ? t('Reading the buses… {done} of {total}', s.vehiclesProgress) : t('Reading the buses…')}</div>`;
  const paints = v ? [['', v.default_paint ? `${t('Standard')} (${v.default_paint})` : t('Standard')], ...v.paints.map((p) => [p, p])] : [];
  const at = Math.max(0, paints.findIndex(([p]) => p === c.paint));
  const step = (d) => choose({ paint: paints[(at + d + paints.length) % paints.length][0] });
  const buttons = html`<${Button} small icon="bus" label=${compact ? undefined : 'Change bus'} title="Change bus" onClick=${() => setPick(true)} disabled=${!s.vehicles.length} />
    ${v && html`<${Button} small icon="sliders" title="Vehicle settings & details" onClick=${() => setMore(true)} />`}`;
  return html`<div class="bus-thumb">
    <div class="bt-view">${v && !pick ? html`<${BusPreview} class="stage" look=${look(v, c)} />` : html`<div class="stage-empty"><${Icon} name="bus" size=${32} /></div>`}${compact && v && html`<div class="bt-tools">${buttons}</div>`}</div>
    <div class="bt-info">
      <div class="row bt-head">
        <div class="grow"><b class="ellipsis bt-name">${v ? withoutMaker(v.name, v.manufacturer) : t('No bus is installed.')}</b><div class="muted small ellipsis">${v?.manufacturer || ''}</div></div>
        ${!(compact && v) && buttons}
      </div>
      ${depotBusOf() && html`<div class="row small depot-tag"><${Icon} name="garage" size=${16} /><span class="ellipsis">${t('Your bus: {name}', { name: `${depotBusOf().name} (${depotBusOf().plate})` })}</span></div>`}
      ${noServerBus() && html`<${Notice} kind="danger">${t('The server offers none of the vehicles installed here')}</${Notice}>`}
      ${v && html`<div class="row livery">
        <div class="grow"><${Select} raw value=${c.paint} options=${paints} onChange=${(paint) => choose({ paint })} /></div>
        <${Button} icon="left" title="Preview previous livery" disabled=${paints.length < 2} onClick=${() => step(-1)} />
        <${Button} icon="right" title="Preview next livery" disabled=${paints.length < 2} onClick=${() => step(1)} />
      </div>`}
    </div>
    ${pick && html`<${BusPicker} onClose=${() => setPick(false)} />`}
    ${more && html`<${Dialog} title="Vehicle settings & details" onClose=${() => setMore(false)}><${BusOptions} /></${Dialog}>`}
  </div>`;
}

function Bar() {
  return html`<${StartBar} step="duty" go=${() => {}} />`;
}

function BigMap() {
  const parts = useRef({});
  const keep = (k) => (el) => (parts.current[k] = el);
  const inset = (box) => {
    const m = box.getBoundingClientRect();
    const rect = (k) => {
      const el = parts.current[k];
      return el?.childElementCount ? el.getBoundingClientRect() : null;
    };
    const over = (p) => p && p.width && p.right > m.left && p.left < m.right && p.bottom > m.top && p.top < m.bottom;
    const [l, r, top, bar] = ['l', 'r', 't', 'b'].map(rect);
    return { l: over(l) ? l.right - m.left : 0, r: over(r) ? m.right - r.left : 0, t: over(top) ? top.bottom - m.top : 0, b: over(bar) ? m.bottom - bar.top : 0 };
  };
  return html`<div class="lay lay-bigmap">
    <div class="bm-map"><${TripMap} inset=${inset} /></div>
    <section class="bm-float bm-left drive-panel-body" ref=${keep('l')}><${DutyStep} /></section>
    <div class="bm-float bm-right" ref=${keep('r')}><${Book} /></div>
    <div class="bm-float bm-top" ref=${keep('t')}><${DayCompact} /></div>
    <div class="bm-float bm-bus"><${BusThumb} compact /></div>
    <div class="bm-bar" ref=${keep('b')}><${Bar} /></div>
  </div>`;
}

function ControlRoom() {
  return html`<div class="lay lay-control">
    <section class="lay-panel drive-panel-body lc-duty"><${DutyStep} /></section>
    <section class="lay-panel lc-map"><${TripMap} /></section>
    <section class="lay-panel lc-book"><${Book} /></section>
    <div class="lc-band">
      <section class="lay-panel lc-bus"><${BusThumb} /></section>
      <section class="lc-go">
        <div class="lay-panel lc-day"><${DayCompact} /></div>
        <${Bar} />
      </section>
    </div>
  </div>`;
}

function ClassicPlus() {
  const c = useStore().choice;
  const v = curBus();
  const tour = curTour();
  const [tab, setTab] = useState(() => (remembered('drive-tab', 'duty') === 'bus' ? 'bus' : 'duty'));
  const go = (id) => (setTab(id), remember('drive-tab', id));
  const tabs = [['duty', 'Map & duty'], ['bus', 'Bus']];
  const bus = tab === 'bus';
  return html`<div class="lay lay-classicplus classic-drive">
    <section class="drive-panel">
      <nav class="drive-steps" role="tablist">
        ${tabs.map(([id, label], i) => html`<button type="button" role="tab" aria-selected=${tab === id} class=${'drive-step' + (tab === id ? ' on' : '')} onClick=${() => go(id)}>
          <span class="n">${i + 1}</span><span class="ellipsis">${t(label)}</span>
        </button>`)}
      </nav>
      <div class="cp-day"><${DayCompact} /></div>
      <div class=${'drive-panel-body step-' + tab} key=${tab}>${bus ? html`<${BusList} /><${BusOptions} />` : html`<${DutyStep} />`}</div>
    </section>
    <div class="drive-stage">
      ${bus ? html`<${BusStage} />` : html`<div class=${'duty-stage' + (c.free || !tour ? ' alone' : '')}><${TripMap} /><${Book} /></div>`}
      <button type="button" class="cp-pip" title=${t(bus ? 'Map & duty' : 'Bus')} onClick=${() => go(bus ? 'duty' : 'bus')}>
        ${bus ? html`<${TripMap} />` : v && html`<${BusPreview} class="stage" look=${look(v, c)} />`}
        <span class="cp-pip-tag"><${Icon} name=${bus ? 'map' : 'bus'} size=${14} />${bus ? t('Map & duty') : v ? withoutMaker(v.name, v.manufacturer) : t('Bus')}</span>
      </button>
    </div>
    <${Bar} />
  </div>`;
}
