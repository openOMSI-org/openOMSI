// AI Lists (a tab of Settings): which AI cars, lorries and buses a map's traffic and depots are made of, and which
// buses run on which line. Nothing new is added to the game here: every choice is from the
// library of the AI vehicles installed (and the buses, with their repaints). The changes are kept
// beside the map, in the content folder, as the files OMSI reads (`omsi-launcher-core`'s
// `ai_lists`); the map's own files stay as they are, and "Reset" deletes the changes.

import { html, useEffect, useMemo, useState } from '../../vendor/preact-htm.js';
import { S, useStore, set, toast } from '../store.js';
import { t } from '../i18n.js';
import { call } from '../api.js';
import { Icon } from '../icons.js';
import { Button, Dialog, Empty, Notice, Search, Segmented, Select, Slider, Spinner, Tag, useConfirm } from '../ui.js';
import { BusPreview } from '../preview.js';
import { listsModel, dirty, shouldRead } from './ai-lists-model.js';

const ai = () => S.ai;
const up = (patch) => set({ ai: { ...S.ai, ...patch } });
const norm = (f) => String(f || '').replace(/\//g, '\\').trim().toLowerCase();
const same = (a, b) => String(a || '').trim().toLowerCase() === String(b || '').trim().toLowerCase();
const clone = (x) => JSON.parse(JSON.stringify(x));
const key = (g) => `${g.depot ? 'd' : 'c'}:${String(g.name).trim().toLowerCase()}`;
const isRail = (g) => g.vehicles.length > 0 && g.vehicles.every((v) => /\.zug$/i.test(v.file));

/** The name of a vehicle file as the library knows it, else its file name. */
function nameOf(file) {
  const v = ai().libIndex?.get(norm(file));
  if (v) return v.name;
  const stem = String(file).split(/[\\/]/).pop().replace(/\.[^.]+$/, '');
  return stem.replace(/_/g, ' ');
}

const folderOf = (file) => String(file).split(/[\\/]/).slice(0, -1).join('\\');
const slash = (file) => String(file).replace(/\\/g, '/');
const focusOn = (file, paint = '') => up({ focus: { file, paint } });
const focused = (file) => ai().focus && norm(ai().focus.file) === norm(file);

/** The vehicle pointed at, drawn by the game's own renderer (the same preview as Drive's). */
function Stage({ file, paint = '' }) {
  const s = useStore();
  const c = s.choice || {};
  return html`<div class="card ail-stage">
    ${file
      ? html`<${BusPreview} class="ail-view" look=${{ bus: slash(file), paint, map: s.ai.map, weather: c.weather, time: c.time, date: c.date }} />
        <div class="card-body"><b>${nameOf(file)}</b><div class="muted small">${folderOf(file)}</div>${paint && html`<div class="muted small">${paint}</div>`}</div>`
      : html`<div class="ail-view empty"><${Icon} name="bus" size=${40} /><p class="muted">${t('Click a vehicle to see it here.')}</p></div>`}
  </div>`;
}

async function readLibrary(refresh = false) {
  if ((ai().lib && !refresh) || ai().libLoading) return;
  up({ libLoading: true });
  try {
    const lib = await call('ai_vehicles', { refresh });
    up({ lib, libIndex: new Map(lib.map((v) => [norm(v.file), v])), libLoading: false });
  } catch (e) {
    up({ libLoading: false, error: String(e.message || e) });
  }
}

const { read, save, reset: resetMap, change } = listsModel({ call, get: ai, up, toast, t });

const setGroup = (g, fn) => {
  const groups = clone(ai().groups);
  const i = groups.findIndex((x) => key(x) === key(g));
  if (i < 0) return;
  fn(groups[i]);
  change({ groups });
};

const modified = (g) => {
  const was = ai().data?.defaults.find((d) => key(d) === key(g));
  return !was || JSON.stringify(was) !== JSON.stringify(g);
};

const resetGroup = (g) => {
  const was = ai().data?.defaults.find((d) => key(d) === key(g));
  if (!was) return;
  change({ groups: ai().groups.map((x) => (key(x) === key(g) ? clone(was) : x)) });
};

/** The next fleet number of a depot: one above the largest number there. */
function nextNumber(g) {
  const nums = g.typgroups.flatMap((tg) => tg.entries.map((e) => e.number)).filter((n) => /^\d+$/.test(String(n).trim())).map(Number);
  return String((nums.length ? Math.max(...nums) : 0) + 1);
}

function newEntries(g, n) {
  const out = [];
  const probe = { typgroups: [...g.typgroups, { entries: [] }] };
  for (let i = 0; i < n; i++) {
    const number = nextNumber(probe);
    out.push({ number, registration: '', paint: '', from: null, to: null });
    probe.typgroups[probe.typgroups.length - 1].entries.push(out[out.length - 1]);
  }
  return out;
}

/** A type the depot lacks is added to it with a few buses, so that a line can run it. */
function ensureInDepot(groups, name, file) {
  const g = groups.find((x) => x.depot && same(x.name, name));
  if (!g || g.typgroups.some((tg) => norm(tg.file) === norm(file))) return false;
  g.typgroups.push({ file, entries: newEntries(g, 3) });
  return true;
}

// --- the vehicle picker ---------------------------------------------------------------

function Picker({ title, kinds, onPick, onClose, hint }) {
  const s = useStore();
  const a = s.ai;
  const [q, setQ] = useState('');
  const [kind, setKind] = useState(kinds[0][0]);
  const [sel, setSel] = useState(null);
  useEffect(() => {
    readLibrary();
  }, []);
  const list = useMemo(() => {
    const words = q.toLowerCase().split(/\s+/).filter(Boolean);
    return (a.lib || []).filter((v) => (kind === 'all' || v.kind === kind) && words.every((w) => (v.name + ' ' + v.folder).toLowerCase().includes(w)));
  }, [a.lib, q, kind]);
  const c = s.choice || {};
  return html`<${Dialog} title=${title} onClose=${onClose} wide class="ail-dialog"
    footer=${html`<${Button} label="Cancel" onClick=${onClose} /><${Button} kind="primary" label="Use this vehicle" disabled=${!sel} onClick=${() => sel && onPick(sel)} />`}>
    ${hint && html`<p class="dim" style="margin-bottom:10px">${t(hint)}</p>`}
    <div class="ail-picker">
      <div class="ail-picker-left">
        <div class="ail-picker-bar">
          <${Search} value=${q} onInput=${setQ} autofocus />
          ${kinds.length > 1 && html`<${Segmented} value=${kind} options=${kinds} onChange=${setKind} />`}
        </div>
        ${!a.lib && html`<div class="row muted" style="padding:16px"><${Spinner} /> ${t('Reading the vehicles…')}</div>`}
        ${a.lib && !list.length && html`<${Empty} icon="search" title="Nothing found" />`}
        <div class="ail-picker-list">
          ${list.slice(0, 300).map((v) => html`<button class=${'ail-pick' + (sel && sel.file === v.file ? ' on' : '')} onClick=${() => setSel(v)} onDblClick=${() => onPick(v)}>
            <span class="grow"><b>${v.name}</b><small>${v.folder}</small></span>
            <${Tag}>${t(v.kind === 'bus' ? 'Bus' : v.kind === 'aircraft' ? 'Aircraft' : 'AI vehicle')}</${Tag}>
          </button>`)}
          ${list.length > 300 && html`<p class="muted" style="padding:8px 4px">${t('Type to narrow the list down.')}</p>`}
        </div>
      </div>
      <div class="ail-picker-right">
        ${sel
          ? html`<${BusPreview} class="ail-view" look=${{ bus: slash(sel.file), paint: '', map: a.map, weather: c.weather, time: c.time, date: c.date }} /><div class="card-body"><b>${sel.name}</b><div class="muted small">${sel.folder}</div></div>`
          : html`<div class="ail-view empty"><${Icon} name="bus" size=${40} /><p class="muted">${t('Click a vehicle to see it before you choose it.')}</p></div>`}
      </div>
    </div>
  </${Dialog}>`;
}

const CAR_KINDS = [['all', 'All'], ['ai', 'Cars and lorries'], ['bus', 'Buses']];
const BUS_KINDS = [['bus', 'Buses'], ['all', 'All']];

// --- groups of random traffic and other pools -----------------------------------------

function Vehicles({ g }) {
  const total = g.vehicles.reduce((n, v) => n + Math.max(0, Number(v.weight) || 0), 0) || 1;
  return html`<div class="ail-rows">
    ${g.vehicles.map((v, i) => html`<div class=${'ail-row' + (focused(v.file) ? ' on' : '')} key=${i} onClick=${() => focusOn(v.file)}>
      <span class="grow"><b>${nameOf(v.file)}</b><small>${folderOf(v.file)}</small></span>
      <label class="ail-weight" title=${t('How often it appears next to the others')}>
        <input class="input" type="number" min="0" step="1" value=${v.weight} onInput=${(e) => setGroup(g, (x) => (x.vehicles[i].weight = Math.max(0, Number(e.currentTarget.value) || 0)))} />
        <span class="muted">${Math.round((100 * Math.max(0, Number(v.weight) || 0)) / total)}%</span>
      </label>
      <${Button} kind="ghost" small label="Replace" onClick=${() => up({ pick: { kind: 'replace', group: g, index: i } })} />
      <${Button} kind="ghost" small icon="trash" title="Remove" onClick=${() => setGroup(g, (x) => x.vehicles.splice(i, 1))} />
    </div>`)}
    ${!g.vehicles.length && html`<p class="muted">${t('This group has no vehicles: the map takes nothing from it.')}</p>`}
  </div>`;
}

function TrafficTab() {
  const s = useStore();
  const a = s.ai;
  const groups = a.groups.filter((g) => !g.depot && !isRail(g));
  const sel = groups.find((g) => key(g) === a.sel) || groups.find((g) => a.data.groups.find((d) => key(d) === key(g))?.random) || groups[0];
  const random = a.data.groups.filter((g) => g.random).map(key);
  return html`<div class="ail-cols">
    <div class="card ail-side">
      ${groups.map((g) => html`<button class=${'ail-item' + (sel && key(g) === key(sel) ? ' on' : '')} onClick=${() => up({ sel: key(g), focus: null })}>
        <span class="grow"><b>${g.name}</b><small>${t(random.includes(key(g)) ? 'Random traffic' : 'Used by the timetable or flight paths')}</small></span>
        ${modified(g) && html`<span class="ail-dot" title=${t('Changed')}></span>`}
        <span class="muted">${g.vehicles.length}</span>
      </button>`)}
    </div>
    <div class="card ail-main">
      ${sel
        ? html`<div class="card-head"><div><h2>${sel.name}</h2><div class="hint">${t(random.includes(key(sel)) ? 'The cars that make up the random traffic of this kind.' : 'Not part of the random traffic: the timetable or the flight paths use it.')}</div></div>
            <div class="row"><${Button} small icon="refresh" label="Reset group" disabled=${!modified(sel)} onClick=${() => resetGroup(sel)} /><${Button} small kind="primary" icon="plus" label="Add vehicle" onClick=${() => up({ pick: { kind: 'add', group: sel } })} /></div></div>
          <div class="card-body"><${Vehicles} g=${sel} /></div>`
        : html`<${Empty} icon="traffic" title="No groups" />`}
    </div>
    <${Stage} file=${a.focus?.file || sel?.vehicles[0]?.file} paint=${a.focus?.paint || ''} />
  </div>`;
}

// --- depots ---------------------------------------------------------------------------

function PaintSelect({ file, value, onChange }) {
  const s = useStore();
  const k = norm(file);
  const known = s.ai.paints?.[k];
  useEffect(() => {
    if (known || s.ai.paintsAsked?.has(k)) return;
    (s.ai.paintsAsked ||= new Set()).add(k);
    call('ai_vehicle_paints', { file })
      .then((p) => up({ paints: { ...ai().paints, [k]: p } }))
      .catch(() => up({ paints: { ...ai().paints, [k]: [] } }));
  }, [k]);
  const options = [['', 'Its own paint'], ...(known || []).map((p) => [p, p])];
  return html`<${Select} value=${value || ''} options=${options} onChange=${onChange} raw=${false} />`;
}

const dateText = (d) => (d ? `${String(d).slice(0, 4)}-${String(d).slice(4, 6)}-${String(d).slice(6, 8)}` : '');

function TypGroup({ g, ti }) {
  const tg = g.typgroups[ti];
  return html`<div class="ail-type">
    <div class=${'ail-type-head' + (focused(tg.file) && !ai().focus.paint ? ' on' : '')} onClick=${() => focusOn(tg.file)}>
      <span class="grow"><b>${nameOf(tg.file)}</b><small>${folderOf(tg.file)}</small></span>
      <${Button} small label="Replace type" onClick=${() => up({ pick: { kind: 'type', group: g, index: ti } })} />
      <${Button} small icon="plus" label="Add bus" onClick=${() => setGroup(g, (x) => x.typgroups[ti].entries.push(...newEntries(x, 1)))} />
      <${Button} kind="ghost" small icon="trash" title="Remove" onClick=${() => setGroup(g, (x) => x.typgroups.splice(ti, 1))} />
    </div>
    <div class="ail-entries">
      ${tg.entries.map((e, i) => html`<div class=${'ail-entry' + (focused(tg.file) && ai().focus.paint && ai().focus.paint === e.paint ? ' on' : '')} key=${i} onClick=${() => focusOn(tg.file, e.paint)}>
        <input class="input" value=${e.number} title=${t('Fleet number')} placeholder=${t('Number')} onInput=${(ev) => setGroup(g, (x) => (x.typgroups[ti].entries[i].number = ev.currentTarget.value))} />
        <input class="input" value=${e.registration} title=${t('Plate')} placeholder=${t('Plate')} onInput=${(ev) => setGroup(g, (x) => (x.typgroups[ti].entries[i].registration = ev.currentTarget.value))} />
        <${PaintSelect} file=${tg.file} value=${e.paint} onChange=${(p) => setGroup(g, (x) => (x.typgroups[ti].entries[i].paint = p))} />
        <span class="muted ail-dates">${e.from || e.to ? t('{from} to {to}', { from: dateText(e.from) || t('always'), to: dateText(e.to) || t('always') }) : ''}</span>
        <${Button} kind="ghost" small icon="x" title="Remove" onClick=${() => setGroup(g, (x) => x.typgroups[ti].entries.splice(i, 1))} />
      </div>`)}
      ${!tg.entries.length && html`<p class="muted">${t('No bus of this type: it never runs.')}</p>`}
    </div>
  </div>`;
}

function DepotTab() {
  const s = useStore();
  const a = s.ai;
  const groups = a.groups.filter((g) => g.depot);
  const sel = groups.find((g) => key(g) === a.sel) || groups[0];
  if (!groups.length) return html`<${Empty} icon="garage" title="This map has no depot buses" text="Its timetable buses come from the plain groups of the other tab, or it has no timetable." />`;
  return html`<div class="ail-cols">
    <div class="card ail-side">
      ${groups.map((g) => html`<button class=${'ail-item' + (key(g) === key(sel) ? ' on' : '')} onClick=${() => up({ sel: key(g), focus: null })}>
        <span class="grow"><b>${g.name}</b><small>${g.hof || ''}</small></span>
        ${modified(g) && html`<span class="ail-dot" title=${t('Changed')}></span>`}
        <span class="muted">${g.typgroups.reduce((n, tg) => n + tg.entries.length, 0)}</span>
      </button>`)}
    </div>
    <div class="card ail-main">
      <div class="card-head"><div><h2>${sel.name}</h2><div class="hint">${t('The buses of this depot: the timetable draws its buses from here, in proportion to how many of each type there are.')}</div></div>
        <div class="row"><${Button} small icon="refresh" label="Reset group" disabled=${!modified(sel)} onClick=${() => resetGroup(sel)} /><${Button} small kind="primary" icon="plus" label="Add type" onClick=${() => up({ pick: { kind: 'newtype', group: sel } })} /></div></div>
      <div class="card-body">${sel.typgroups.map((_, i) => html`<${TypGroup} g=${sel} ti=${i} key=${i} />`)}</div>
    </div>
    <${Stage} file=${a.focus?.file || sel.typgroups[0]?.file} paint=${a.focus?.paint || ''} />
  </div>`;
}

// --- lines ----------------------------------------------------------------------------

function ruleOf(line) {
  return ai().rules.find((r) => same(r.line, line.name));
}

function setRule(line, patch) {
  const rules = clone(ai().rules);
  const i = rules.findIndex((r) => same(r.line, line.name));
  if (patch === null) {
    if (i >= 0) rules.splice(i, 1);
  } else if (i >= 0) rules[i] = { ...rules[i], ...patch };
  else rules.push({ line: line.name, mode: 'only', factor: 1, types: [], ...patch });
  change({ rules });
}

function LinesTab() {
  const s = useStore();
  const a = s.ai;
  const [q, setQ] = useState('');
  const lines = a.data.lines.filter((l) => l.groups.some((n) => a.groups.some((g) => g.depot && same(g.name, n))) && (!q || l.name.toLowerCase().includes(q.toLowerCase())));
  const sel = lines.find((l) => l.name === a.line) || lines[0];
  if (!a.data.lines.length) return html`<${Empty} icon="route" title="This map has no lines" />`;
  const rule = sel && ruleOf(sel);
  return html`<div class="ail-cols">
    <div class="card ail-side">
      <div style="padding:8px"><${Search} value=${q} onInput=${setQ} /></div>
      ${lines.map((l) => html`<button class=${'ail-item' + (sel && l.name === sel.name ? ' on' : '')} onClick=${() => up({ line: l.name, focus: null })}>
        <span class="grow"><b>${l.name}</b><small>${l.groups.join(', ')}</small></span>
        ${ruleOf(l) && html`<span class="ail-dot" title=${t('Changed')}></span>`}
      </button>`)}
    </div>
    <div class="card ail-main">
      ${sel
        ? html`<div class="card-head"><div><h2>${t('Line {name}', { name: sel.name })}</h2><div class="hint">${t('Which buses run on this line. The line keeps its tours and times: only the bus changes.')}</div></div>
            <div class="row"><${Button} small icon="refresh" label="Reset line" disabled=${!rule} onClick=${() => setRule(sel, null)} /></div></div>
          <div class="card-body ail-stack">
            ${sel.map_rule && !rule && html`<${Notice}>${t('The map has rules of its own for this line (a bus type or a fleet number for some of its tours). Choosing one here goes before them.')}</${Notice}>`}
            <${Segmented} value=${rule ? rule.mode : 'map'} onChange=${(m) => (m === 'map' ? setRule(sel, null) : setRule(sel, { mode: m }))}
              options=${[['map', 'As the map has it'], ['only', 'Only these types'], ['prefer', 'Mostly these types']]} />
            ${rule && rule.mode === 'prefer' && html`<${Slider} label="Tours that get one of these types" value=${Math.round(rule.factor * 100)} min=${10} max=${100} step=${5} format=${(v) => v + '%'} onChange=${(v) => setRule(sel, { factor: v / 100 })} />`}
            ${rule && html`<div class="ail-rows">
              ${rule.types.map((f, i) => html`<div class=${'ail-row' + (focused(f) ? ' on' : '')} key=${i} onClick=${() => focusOn(f)}>
                <span class="grow"><b>${nameOf(f)}</b><small>${folderOf(f)}</small></span>
                <${Button} kind="ghost" small icon="trash" title="Remove" onClick=${() => setRule(sel, { types: rule.types.filter((_, j) => j !== i) })} />
              </div>`)}
              ${!rule.types.length && html`<p class="muted">${t('Add the types this line runs.')}</p>`}
              <div><${Button} small kind="primary" icon="plus" label="Add type" onClick=${() => up({ pick: { kind: 'rule', line: sel } })} /></div>
            </div>`}
            ${!rule && html`<p class="muted">${t('The buses of this line are drawn from its depot as the map says.')}</p>`}
          </div>`
        : html`<${Empty} icon="route" title="No lines use a depot here" />`}
    </div>
    <${Stage} file=${a.focus?.file || rule?.types[0]} />
  </div>`;
}

// --- the page -------------------------------------------------------------------------

function onPick(v) {
  const a = ai();
  if (a.busy) return;
  const p = a.pick;
  const groups = clone(a.groups);
  const g = p.group && groups.find((x) => key(x) === key(p.group));
  if (p.kind === 'replace') g.vehicles[p.index].file = v.file;
  else if (p.kind === 'add') {
    if (!g.vehicles.some((x) => norm(x.file) === norm(v.file))) g.vehicles.push({ file: v.file, weight: 1 });
  } else if (p.kind === 'type') {
    // the buses keep their numbers; a repaint of another model does not exist on this one
    g.typgroups[p.index].file = v.file;
    g.typgroups[p.index].entries.forEach((e) => (e.paint = ''));
  } else if (p.kind === 'newtype') {
    if (!g.typgroups.some((x) => norm(x.file) === norm(v.file))) g.typgroups.push({ file: v.file, entries: newEntries(g, 3) });
  } else if (p.kind === 'rule') {
    const rules = clone(a.rules);
    const r = rules.find((x) => same(x.line, p.line.name));
    if (r && !r.types.some((f) => norm(f) === norm(v.file))) {
      r.types.push(v.file);
      // a type the line's depots do not have is added to them
      const added = p.line.groups.filter((n) => ensureInDepot(groups, n, v.file));
      if (added.length) toast(t('{type} was added to {depot}, with a few buses.', { type: v.name, depot: added.join(', ') }), 'info');
    }
    change({ groups, rules, pick: null });
    return;
  }
  change({ groups, pick: null });
}

export function AiListsPage() {
  const s = useStore();
  const maps = s.maps || [];
  if (!s.ai) S.ai = { map: null, date: '', loaded: null, data: null, groups: [], saved: [], rules: [], savedRules: [], tab: 'traffic', sel: null, line: null, pick: null, busy: false, lib: null, libIndex: null, paints: {} };
  const a = s.ai;
  const date = s.choice?.date || s.boot?.now?.date || '';
  const want = a.map && maps.some((m) => m.file === a.map) ? a.map : maps.find((m) => m.file === s.choice?.map)?.file || maps[0]?.file;
  const [ask, confirm] = useConfirm();
  useEffect(() => {
    if (shouldRead(S.ai, want, date)) {
      S.ai.date = date;
      read(want, date);
    }
  }, [want, date, a.loaded, a.busy, dirty(a)]);
  useEffect(() => {
    // (a vehicle installed since the last visit shows up: the library is read again, the old one shown meanwhile)
    readLibrary(!!ai().lib);
  }, []);
  if (!maps.length) return html`<${Notice}>${t('No maps found.')}</${Notice}>`;
  const d = a.data;
  const changed = d && dirty(a);
  const pk = a.pick;
  return html`<div class=${'ail' + (a.busy ? ' busy' : '')} inert=${a.busy || undefined}>
    <p class="dim">${t("Choose which AI cars and buses a map's traffic is made of, and which buses run on which line. You choose from the vehicles you have installed; nothing new is added, and the map's own lists stay as they are.")}</p>
    <div class="ail-bar">
      <div style="min-width:240px;flex:1;max-width:420px"><${Select} value=${want} raw options=${maps.map((m) => [m.file, m.name])} onChange=${(f) => !changed && !a.busy && up({ map: f, loaded: null })} disabled=${changed || a.busy} /></div>
      <${Segmented} value=${a.tab} onChange=${(tab) => up({ tab })} options=${[['traffic', 'Traffic'], ['depot', 'Depot buses'], ['lines', 'Lines']]} />
      <div class="grow"></div>
      ${d?.edited && html`<${Tag}>${t('This map has your changes')}</${Tag}>`}
      <${Button} icon="refresh" label="Reset the map" disabled=${!d?.edited || a.busy} onClick=${async () => (await ask({ title: 'Reset the AI lists?', text: 'The map goes back to its own AI lists and line rules. Your changes to this map are deleted.', yes: 'Reset', danger: true })) && resetMap()} />
    </div>
    ${a.error && html`<${Notice} kind="danger">${a.error}</${Notice}>`}
    ${!d && !a.error && html`<div class="row muted" style="padding:16px"><${Spinner} /> ${t('Reading the lists…')}</div>`}
    ${d && html`<${Notice}>${t('Changes hold on every date of this map. The lists shown are those of {date}.', { date: d.date })}</${Notice}>`}
    ${d && a.tab === 'traffic' && html`<${TrafficTab} />`}
    ${d && a.tab === 'depot' && html`<${DepotTab} />`}
    ${d && a.tab === 'lines' && html`<${LinesTab} />`}
    ${changed && html`<div class="ail-save"><span>${t('You have unsaved changes.')}</span>
      <${Button} label="Discard" onClick=${() => change({ groups: clone(a.saved), rules: clone(a.savedRules) })} />
      <${Button} kind="primary" icon="save" label="Save" disabled=${a.busy} onClick=${save} /></div>`}
    ${pk && html`<${Picker}
      title=${pk.kind === 'replace' ? 'Replace with' : pk.kind === 'rule' ? 'Add a type to the line' : pk.kind === 'add' ? 'Add a vehicle' : pk.kind === 'type' ? 'Replace the type with' : 'Add a type to the depot'}
      kinds=${pk.kind === 'type' || pk.kind === 'newtype' || pk.kind === 'rule' ? BUS_KINDS : CAR_KINDS}
      hint=${pk.kind === 'rule' ? 'A type the depot does not have yet is added to it with a few buses.' : ''}
      onPick=${onPick} onClose=${() => up({ pick: null })} />`}
    ${confirm}
  </div>`;
}
