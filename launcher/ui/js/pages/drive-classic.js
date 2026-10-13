import { html, useState, useEffect, useRef } from '../../vendor/preact-htm.js';
import { useStore, set, remembered, remember } from '../store.js';
import { t, hhmm, longDate, language } from '../i18n.js';
import { Icon } from '../icons.js';
import { Button, Notice } from '../ui.js';
import { choose, curMap, curBus, curLine, curTour, pickedTrip, start, loadSituations, weatherName, mapName, noServerBus, askJoined } from '../data.js';
import { BusPreview } from '../preview.js';
import { BusList, BusOptions, withoutMaker } from './bus-list.js';
import { DayStep } from './drive-day.js';
import { DutyStep, DutyStage } from './drive-duty.js';
import { depotRun, tripAt } from './drive.js';

const STEPS = [['bus', 'Bus'], ['day', 'Day & weather'], ['duty', 'Map & duty']];

export function ClassicDrive() {
  const s = useStore();
  const c = s.choice;
  const [step, setStep] = useState(() => (STEPS.some(([id]) => id === remembered('drive-step', 'bus')) ? remembered('drive-step', 'bus') : 'bus'));
  useEffect(() => {
    askJoined();
  }, [c?.lan_mode, c?.lan_addr]);
  if (!c) return null;
  const go = (id) => {
    setStep(id);
    remember('drive-step', id);
  };
  return html`<div class="classic-drive">
    <h1 class="sr-only">${t('Drive')}</h1>
    <section class="drive-panel">
      <nav class="drive-steps" role="tablist">
        ${STEPS.map(([id, label], i) => html`<button type="button" role="tab" aria-selected=${step === id} class=${'drive-step' + (step === id ? ' on' : '')} onClick=${() => go(id)}>
          <span class="n">${i + 1}</span><span class="ellipsis">${t(label)}</span>
        </button>`)}
      </nav>
      ${s.errors.maps && html`<div class="drive-note"><${Notice} kind="danger">${s.errors.maps}</${Notice}></div>`}
      <div class=${'drive-panel-body step-' + step} key=${step}>
        ${step === 'bus' ? html`<${BusList} /><${BusOptions} />` : step === 'day' ? html`<${DayStep} />` : html`<${DutyStep} />`}
      </div>
    </section>
    <div class="drive-stage">${step === 'duty' ? html`<${DutyStage} />` : html`<${BusStage} />`}</div>
    <${StartBar} step=${step} go=${go} />
  </div>`;
}

function BusStage() {
  const s = useStore();
  const c = s.choice;
  const v = curBus();
  if (!v) return html`<div class="stage-empty"><${Icon} name="bus" size=${40} /></div>`;
  return html`<div class="bus-stage">
    <${BusPreview} class="stage" look=${{ bus: v.file, paint: c.paint, map: c.map, weather: c.weather, time: c.time, date: c.date }} />
    <div class="stage-caption">
      <b>${withoutMaker(v.name, v.manufacturer)}</b>
      <span>${[v.manufacturer, c.paint || v.default_paint].filter(Boolean).join(' | ')}</span>
    </div>
  </div>`;
}

function StartBar({ step, go }) {
  const s = useStore();
  const c = s.choice;
  const m = curMap();
  const v = curBus();
  const line = curLine();
  const tour = curTour();
  const [busy, setBusy] = useState(false);
  const trip = tour && !c.free ? (pickedTrip() != null ? tour.trips.find((x) => x.index === pickedTrip()) : tour.trips[tripAt(tour, c.time * 60)]) : null;
  const joined = c.lan_mode === 'join';
  const ready = m && v && (c.free || (line && tour)) && !noServerBus();
  const running = s.poll.instances?.length || 0;
  const next = STEPS.findIndex(([id]) => id === step) + 1;
  const launch = async (extra) => {
    setBusy(true);
    await start(extra);
    setBusy(false);
  };
  const where = joined && c.lan_map ? mapName(c.lan_map.replace(/\\/g, '/')) : m ? m.friendly || m.name : '';
  const when = joined ? `${c.lan_server?.time || ''} ${t("(the server's clock)")}` : `${hhmm(c.time)}, ${longDate(c.date)}`;
  const service = c.free ? t('Free drive') : [line && `${t('Line')} ${line.name}`, tour && `${t('Tour')} ${tour.number}`].filter(Boolean).join(' | ');
  return html`<div class="drive-bar">
    ${trip && !depotRun(trip)
      ? html`<div class="sign"><span class="no">${trip.line}</span><span class="to">${trip.terminus}</span></div>`
      : html`<div class="sign icon"><${Icon} name=${c.free ? 'route' : 'bus'} /></div>`}
    <div class="what">
      <b class="ellipsis">${v ? withoutMaker(v.name, v.manufacturer) : t('No bus is installed.')}${service ? ` | ${service}` : ''}</b>
      <span class="ellipsis">${[when, joined ? c.lan_server?.weather || t("the map's (the server's)") : weatherName(c.weather, s.weather), where].filter(Boolean).join(' | ')}</span>
    </div>
    <div class="bar-actions">
      ${joined && html`<span class="tag accent"><${Icon} name="users" size=${14} />${t('Joining {server}', { server: c.lan_server?.name || c.lan_addr })}</span>
        <${Button} kind="ghost" label="Leave" onClick=${() => choose({ lan_mode: 'off' })} />`}
      ${running > 0 && html`<a class="small" onClick=${() => set({ page: 'games' })}>${t('{n} game(s) running', { n: running })}</a>`}
      <${Continue} go=${launch} busy=${busy} />
      ${next < STEPS.length && html`<${Button} big onClick=${() => go(STEPS[next][0])}>${t('Next:')} ${t(STEPS[next][1])}<${Icon} name="right" /></${Button}>`}
      <${Button} kind="primary" big icon="play" disabled=${!ready || busy} onClick=${() => launch()}>
        ${busy ? t('Starting…') : joined ? t('Join and drive') : c.free ? t('Start driving') : t('Start the duty')}
      </${Button}>
    </div>
  </div>`;
}

function Continue({ go, busy }) {
  const s = useStore();
  const [open, setOpen] = useState(false);
  const box = useRef();
  useEffect(() => {
    loadSituations();
  }, []);
  useEffect(() => {
    if (!open) return;
    const out = (e) => !box.current?.contains(e.target) && setOpen(false);
    document.addEventListener('mousedown', out);
    return () => document.removeEventListener('mousedown', out);
  }, [open]);
  if (!s.situations?.length || s.choice.lan_mode === 'join') return null;
  return html`<div class="menu-box" ref=${box}>
    <${Button} big icon="history" label="Continue" onClick=${() => setOpen(!open)} />
    ${open && html`<div class="menu up">
      ${s.situations.slice(0, 6).map((x) => html`<button type="button" class="menu-item" disabled=${busy} onClick=${() => (setOpen(false), go({ situation: x.file }))}>
        <${Icon} name="play" size=${16} />
        <span class="grow"><b class="ellipsis">${t(x.name)}</b><span class="muted small">${new Date(x.saved * 1000).toLocaleString(language())}</span></span>
      </button>`)}
    </div>`}
  </div>`;
}
