// Depot: the player's own buses (`depot.json`), each keeping its plate, its odometer, its fuel
// and its wear from one drive to the next.

import { html, useState, useEffect } from '../../vendor/preact-htm.js';
import { S, useStore, set, toast, remember } from '../store.js';
import { t, ago } from '../i18n.js';
import { call } from '../api.js';
import { Icon } from '../icons.js';
import { BusPreview } from '../preview.js';
import { Button, Card, Dialog, Field, Input, Select, Empty, Notice, PageHead, useConfirm } from '../ui.js';
import { busName, mapName, busFile, loadDepot, driveDepotBus } from '../data.js';
import { BusPicker } from './bus-picker.js';

const hours = (secs) => t('{h} h {m} min', { h: Math.floor(secs / 3600), m: String(Math.floor((secs % 3600) / 60)).padStart(2, '0') });

const vehicleOf = (b) => S.vehicles?.find((v) => busFile(v.file) === busFile(b.bus));

async function update(id, patch) {
  const before = S.depot.find((b) => b.id === id);
  set({ depot: S.depot.map((b) => (b.id === id ? { ...b, ...patch } : b)) });
  try {
    await call('depot_update', { id, patch });
  } catch (e) {
    // (the change did not reach the file: the page shows what it holds again)
    if (before) set({ depot: S.depot.map((b) => (b.id === id ? { ...b, ...Object.fromEntries(Object.keys(patch).map((k) => [k, before[k]])) } : b)) });
    toast(e.message, 'error');
  }
}

function Detail({ b, groups = [] }) {
  const s = useStore();
  const c = s.choice;
  const v = vehicleOf(b);
  const [name, setName] = useState(b.name);
  const [plate, setPlate] = useState(b.plate);
  const [ask, confirm] = useConfirm();
  useEffect(() => (setName(b.name), setPlate(b.plate)), [b.id, b.name, b.plate]);
  const paints = v ? [['', v.default_paint ? `${t('Standard')} (${v.default_paint})` : t('Standard')], ...v.paints.map((p) => [p, p])] : [];
  const numbers = (v?.numbers || []).map(([n]) => [n, n]);
  const reroll = async () => {
    const p = await call('depot_plate').catch(() => '');
    if (!p) return;
    setPlate(p);
    update(b.id, { plate: p });
  };
  const service = async (kind) => {
    try {
      await call('depot_service', { id: b.id, kind });
      await loadDepot();
    } catch (e) {
      toast(e.message, 'error');
    }
  };
  const remove = async () => {
    if (!(await ask({ title: 'Remove from the depot', text: t('{name} leaves the depot with its kilometres and its wear.', { name: b.name }), yes: 'Remove', danger: true }))) return;
    await call('depot_remove', { id: b.id }).catch((e) => toast(e.message, 'error'));
    set({ depotSel: null });
    loadDepot();
  };
  const tiles = [
    ['route', b.odometer != null ? `${Math.round(b.odometer).toLocaleString()} km` : t('never'), 'Odometer'],
    ['route', `${(b.metres / 1000).toFixed(1)} km`, 'distance'],
    ['clock', hours(b.seconds), 'hours driven'],
    ['gamepad', b.sessions, 'runs'],
    ['map', b.stops, 'stops served'],
    ['ticket', b.tickets, 'tickets sold'],
    ['save', b.takings.toFixed(2), 'takings'],
    ['fuel', `${b.fuel_used.toFixed(1)} L`, 'fuel used'],
    ['alert', b.crashes, 'crashes'],
  ];
  return html`<div class="depot-detail stack">
    <${Card} body=${false}>
      ${v ? html`<${BusPreview} class="depot-stage" look=${{ bus: v.file, paint: b.paint, map: c.map, weather: c.weather, time: c.time, date: c.date }} />` : html`<div class="depot-stage"><div class="stage-empty"><${Icon} name="bus" size=${40} /></div></div>`}
      <div class="card-body stack">
        <div class="row wrap">
          <div class="grow" style="min-width:180px"><h2 style="font-size:20px">${b.name}</h2><div class="muted">${busName(b.bus)}${b.last_used ? ` - ${ago(b.last_used)}` : ''}</div></div>
          <${Button} kind="danger" icon="trash" title="Remove from the depot" onClick=${remove} />
          <${Button} kind="primary" icon="play" label="Drive this bus" disabled=${!v} onClick=${() => driveDepotBus(b)} />
        </div>
        ${!v && html`<${Notice} kind="warn">${busName(b.bus)}: ${t('not installed')}</${Notice}>`}
        <div class="grid-2">
          <${Field} label="Name"><${Input} value=${name} placeholder="Name (e.g. my Citaro)" onInput=${setName} onBlur=${() => name.trim() && name !== b.name && update(b.id, { name: name.trim() })} /></${Field}>
          <${Field} label="Number plate">
            <div class="row">
              <div class="grow"><${Input} value=${plate} placeholder="AB-123-CD" onInput=${setPlate} onBlur=${() => plate.trim() && plate !== b.plate && update(b.id, { plate: plate.trim().toUpperCase() })} /></div>
              <${Button} icon="refresh" title="Another plate" onClick=${reroll} />
            </div>
          </${Field}>
          <${Field} label="Livery"><${Select} raw value=${b.paint} options=${paints} disabled=${!v} onChange=${(paint) => update(b.id, { paint })} /></${Field}>
          ${groups.length > 0 && html`<${Field} label="Depot"><${Select} raw value=${b.group || ''} options=${[['', t('No depot')], ...groups.map((g) => [g, g])]} onChange=${(group) => update(b.id, { group })} /></${Field}>`}
          <${Field} label="Fleet number">${numbers.length
            ? html`<${Select} raw value=${b.number} options=${numbers} onChange=${(number) => update(b.id, { number })} />`
            : html`<span class="input dc-fixed muted">${t('None')}</span>`}</${Field}>
        </div>
        <div class="row wrap depot-state">
          ${b.fuel != null && html`<span><${Icon} name="fuel" size=${16} /> ${Math.round(b.fuel)} L</span>`}
          <span><${Icon} name="sun" size=${16} /> ${t('Dirt')} ${b.dirt != null ? `${Math.round(b.dirt * 100)} %` : '0 %'}</span>
        </div>
        <div class="depot-faults">
          ${(b.faults || []).length
            ? b.faults.map((f) => html`<div class="row depot-fault-row">
                <span class="depot-fault"><${Icon} name="alert" size=${16} /></span>
                <span class="grow">${t(f.label)}</span>
                <${Button} small icon="repair" label="Repair" onClick=${() => service(`repair:${f.id}`)} />
              </div>`)
            : html`<div class="row muted small"><${Icon} name="check" size=${16} /> ${t('No fault')}</div>`}
          ${String(s.settings?.maintenance ?? '0') === '0' && html`<div class="muted small">${t('Parts never wear out with Maintenance set to Infinite: choose Normal under Settings so that bulbs blow and the doors wear as in OMSI.')}</div>`}
        </div>
        <div class="row wrap">
          <${Button} small icon="fuel" label="Refuel" disabled=${b.fuel == null} onClick=${() => service('refuel')} />
          <${Button} small icon="wash" label="Wash" disabled=${!b.dirt} onClick=${() => service('wash')} />
          <${Button} small icon="repair" label="Repair all" disabled=${!b.faults?.length} onClick=${() => service('repair')} />
        </div>
      </div>
    </${Card}>
    <div class="stats">${tiles.map(([icon, val, label]) => html`<div class="stat"><div class="row muted" style="gap:6px;margin-bottom:6px"><${Icon} name=${icon} size=${16} /><span>${t(label)}</span></div><b>${val}</b></div>`)}</div>
    <${Card} title="Recent runs">
      ${!b.history?.length
        ? html`<${Empty} icon="history" text="No runs yet. Drive a duty and it shows up here." />`
        : html`<table class="table">
            <thead><tr><th>${t('When')}</th><th>${t('Duty')}</th><th>${t('Time')}</th><th>${t('Distance')}</th><th>${t('Stops')}</th><th>${t('Tickets')}</th><th>${t('Crashes')}</th></tr></thead>
            <tbody>${b.history.map((x) => html`<tr>
              <td class="muted">${ago(x.time)}</td>
              <td>${x.line ? html`<span class="line-badge">${x.line}</span> ${x.tour ? `/ ${x.tour}` : ''} <span class="muted">${mapName(x.map)}</span>` : html`${t('Free drive')} <span class="muted">${mapName(x.map)}</span>`}</td>
              <td class="num">${Math.floor(x.seconds / 3600)}:${String(Math.floor((x.seconds % 3600) / 60)).padStart(2, '0')}</td>
              <td class="num">${(x.metres / 1000).toFixed(1)} km</td>
              <td class="num">${x.stops}</td><td class="num">${x.tickets}</td><td class="num">${x.crashes}</td>
            </tr>`)}</tbody>
          </table>`}
    </${Card}>
    ${confirm}
  </div>`;
}

// (the depot list's choice for the buses kept in no depot)
const NONE = '\u0001';

/** A depot's name, asked in a dialog. */
function NameDialog({ title, value = '', onSave, onClose }) {
  const [name, setName] = useState(value);
  const save = () => name.trim() && onSave(name.trim());
  return html`<${Dialog} title=${title} onClose=${onClose} footer=${html`
      <${Button} label="Cancel" onClick=${onClose} />
      <${Button} kind="primary" icon="check" label="Save" disabled=${!name.trim() || name.trim() === value} onClick=${save} />`}>
    <${Field} label="Depot name"><${Input} value=${name} placeholder="Grand Paris-Moulon" autofocus onInput=${setName} onKeyDown=${(e) => e.key === 'Enter' && save()} /></${Field}>
  </${Dialog}>`;
}

export function DepotPage() {
  const s = useStore();
  const [adding, setAdding] = useState(false);
  const [naming, setNaming] = useState(null);
  const [ask, confirm] = useConfirm();
  useEffect(() => {
    loadDepot();
  }, []);
  const groups = s.depotGroups || [];
  const all = s.depot || [];
  const group = s.depotGroup === NONE || groups.includes(s.depotGroup) ? s.depotGroup : '';
  const pick = (g) => (set({ depotGroup: g }), remember('depot-group', g));
  const list = group === '' ? all : all.filter((x) => (group === NONE ? !x.group : x.group === group));
  const b = list.find((x) => x.id === s.depotSel) || list[0];
  const loose = groups.length > 0 && all.some((x) => !x.group);
  const options = [['', `${t('All buses')} (${all.length})`], ...groups.map((g) => [g, `${g} (${all.filter((x) => x.group === g).length})`]), ...(loose ? [[NONE, t('No depot')]] : [])];
  const groupCall = async (cmd, args, then) => {
    try {
      set({ depotGroups: await call(cmd, args) });
      then?.();
      setNaming(null);
      await loadDepot();
    } catch (e) {
      toast(e.message, 'error');
    }
  };
  const removeGroup = async () => {
    if (!(await ask({ title: 'Delete the depot', text: t('The buses of {name} stay in the list, in no depot.', { name: group }), yes: 'Delete', danger: true }))) return;
    groupCall('depot_group_remove', { name: group }, () => pick(''));
  };
  const add = async ({ bus, paint }) => {
    const v = S.vehicles?.find((x) => x.file === bus);
    const numbers = v?.numbers || [];
    const number = numbers.length ? numbers[Math.floor(Math.random() * numbers.length)][0] : '';
    try {
      const id = await call('depot_add', { bus, paint: paint || null, number: number || null, name: v?.name || busName(bus), group: groups.includes(group) ? group : null });
      await loadDepot();
      set({ depotSel: id });
      toast(t('Bus added to your depot.'), 'ok');
    } catch (e) {
      toast(e.message, 'error');
    }
  };
  return html`
    <${PageHead} title="Depot" text="Your own buses: each keeps its number plate, its kilometres and the state you left it in."
      actions=${html`<${Button} kind="primary" icon="plus" label="Add a bus" disabled=${!s.vehicles?.length} onClick=${() => setAdding(true)} />`} />
    ${s.errors.depot && html`<${Notice} kind="danger">${s.errors.depot}</${Notice}>`}
    ${!all.length && !groups.length
      ? html`<${Card}><${Empty} icon="garage" text="Your buses show up here once added." /></${Card}>`
      : html`<div class="depot">
          <div class="stack depot-side">
            <div class="row depot-groups">
              <div class="grow"><${Select} raw value=${group} options=${options} onChange=${pick} /></div>
              <${Button} icon="plus" title="New depot" onClick=${() => setNaming({ title: 'New depot', save: (name) => groupCall('depot_group_add', { name }, () => pick(name)) })} />
              ${groups.includes(group) && html`
                <${Button} icon="edit" title="Rename the depot" onClick=${() => setNaming({ title: 'Rename the depot', value: group, save: (to) => groupCall('depot_group_rename', { name: group, to }, () => pick(to)) })} />
                <${Button} kind="danger" icon="trash" title="Delete the depot" onClick=${removeGroup} />`}
            </div>
            <${Card} body=${false} class="depot-list">
              ${!list.length && html`<${Empty} icon="garage" text="Your buses show up here once added." />`}
              ${list.map((x) => html`<div class=${'list-item' + (x.id === b?.id ? ' on' : '')} onClick=${() => set({ depotSel: x.id })}>
                <${Icon} name="bus" />
                <div class="grow" style="min-width:0">
                  <div class="title ellipsis">${x.name}${x.faults?.length > 0 && html` <span class="depot-fault" title=${x.faults.map((f) => t(f.label)).join(', ')}><${Icon} name="alert" size=${14} /></span>`}</div>
                  <div class="sub ellipsis">${[x.plate, x.number && `#${x.number}`, x.odometer != null && `${Math.round(x.odometer).toLocaleString()} km`].filter(Boolean).join(' - ')}</div>
                </div>
              </div>`)}
            </${Card}>
          </div>
          ${b && html`<${Detail} key=${b.id} b=${b} groups=${groups} />`}
        </div>`}
    ${adding && html`<${BusPicker} title="Add a bus" yes="Add to the depot" onTake=${add} onClose=${() => setAdding(false)} />`}
    ${naming && html`<${NameDialog} title=${naming.title} value=${naming.value} onSave=${naming.save} onClose=${() => setNaming(null)} />`}
    ${confirm}
  `;
}
