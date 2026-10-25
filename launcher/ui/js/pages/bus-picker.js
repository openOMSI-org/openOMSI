// The bus picker: the buses in a list (search, makers, favourites), the chosen one turning in
// a 3D picture beside it with its paints and description.

import { html, useState, useMemo } from '../../vendor/preact-htm.js';
import { BusPreview } from '../preview.js';
import { S, useStore, set } from '../store.js';
import { t } from '../i18n.js';
import { call } from '../api.js';
import { Icon } from '../icons.js';
import { Dialog, Search, Select, Segmented, Empty, Spinner, Button, Tag, Notice, Field } from '../ui.js';
import { choose, busFile, serverBuses, MODES, modeLabel, shownModes, transportRefusal, curMap } from '../data.js';
import { Notes } from './drive.js';

const favKey = (file) => file.replace(/\\/g, '/').toLowerCase();

const words = (x) => (x || '').replace(/_/g, ' ').split(/\s+/).filter(Boolean);

/** One vehicle folder's buses of a maker as one model, named by what their types share. */
export function families(list) {
  const by = new Map();
  for (const v of list) {
    const key = `${(v.manufacturer || '').toLowerCase()}|${favKey(v.file).replace(/[^/]*$/, '')}`;
    if (!by.has(key)) by.set(key, []);
    by.get(key).push(v);
  }
  return [...by.entries()].map(([key, vs]) => {
    const types = vs.map((v) => words(v.type_name || v.name));
    let n = 0;
    while (types.every((w) => n < w.length && w[n].toLowerCase() === types[0][n].toLowerCase())) n++;
    if (vs.length > 1 && types.some((w) => w.length === n)) n = Math.max(0, n - 1);
    while (n > 0 && !/[\p{L}\p{N}]/u.test(types[0][n - 1])) n--;
    const name = vs.length === 1 ? vs[0].name : n > 0 ? types[0].slice(0, n).join(' ') : vs[0].file.replace(/\\/g, '/').split('/').slice(-2, -1)[0].replace(/_/g, ' ');
    const rest = (w) => {
      let i = n;
      while (i < w.length - 1 && !/[\p{L}\p{N}]/u.test(w[i])) i++;
      return w.slice(i).join(' ');
    };
    const versions = vs.map((v, i) => ({ v, label: vs.length > 1 && n > 0 ? rest(types[i]) : v.type_name || v.name }));
    versions.sort((a, b) => a.label.localeCompare(b.label, undefined, { numeric: true }));
    return { key, name, maker: vs[0].manufacturer || '', versions };
  }).sort((a, b) => a.maker.localeCompare(b.maker) || a.name.localeCompare(b.name, undefined, { numeric: true }));
}

export function toggleFavourite(file) {
  const k = favKey(file);
  const list = S.favourites.includes(k) ? S.favourites.filter((x) => x !== k) : [...S.favourites, k];
  set({ favourites: list });
  call('favourites_save', { list }).catch(() => {});
}

export function BusPicker({ onClose, onTake, title = 'Choose a bus', yes = 'Take this bus' }) {
  const s = useStore();
  const [q, setQ] = useState('');
  const [maker, setMaker] = useState('');
  const [show, setShow] = useState('all');
  const [mode, setMode] = useState('');
  const map = curMap();
  const [sel, setSel] = useState(s.choice.bus);
  const [paint, setPaint] = useState(s.choice.bus === sel ? s.choice.paint : '');
  const joined = serverBuses(s.choice);
  const all = (s.vehicles || []).filter((v) => !joined || joined.has(busFile(v.file)));
  const modes = all.some((v) => v.transport?.some((m) => m !== 'bus')) ? MODES.filter((x) => all.some((v) => v.transport?.includes(x))) : [];
  const makers = useMemo(() => [...new Set(all.map((v) => v.manufacturer).filter(Boolean))].sort(), [all]);
  const ql = q.trim().toLowerCase();
  const list = all
    .filter((v) => !maker || v.manufacturer === maker)
    .filter((v) => !mode || v.transport?.includes(mode))
    .filter((v) => show !== 'fav' || s.favourites.includes(favKey(v.file)))
    .filter((v) => show !== 'mods' || v.installed)
    .filter((v) => !ql || `${v.name} ${v.manufacturer} ${v.type_name} ${v.folder}`.toLowerCase().includes(ql))
    .sort((a, b) => (a.manufacturer || '').localeCompare(b.manufacturer || '') || a.name.localeCompare(b.name));
  const fams = useMemo(() => families(list), [list.map((x) => x.file).join('|')]);
  const [open, setOpen] = useState(() => new Set(fams.filter((f) => f.versions.some((x) => x.v.file === s.choice.bus)).map((f) => f.key)));
  const toggle = (key) => setOpen((o) => {
    const n = new Set(o);
    if (n.has(key)) n.delete(key);
    else n.add(key);
    return n;
  });
  const v = all.find((x) => x.file === sel);
  const take = () => {
    if (onTake) {
      onTake({ bus: sel, paint });
      return onClose();
    }
    choose({ bus: sel, paint, number: sel === s.choice.bus ? s.choice.number : '', hof_manual: sel === s.choice.bus ? s.choice.hof_manual : false });
    onClose();
  };
  return html`<${Dialog} wide title=${title} onClose=${onClose} footer=${html`
      <span class="muted small" style="margin-right:auto">${t('{n} buses', { n: list.length })}</span>
      <${Button} label="Cancel" onClick=${onClose} />
      <${Button} kind="primary" icon="check" label=${yes} disabled=${!v} onClick=${take} />`}>
    <div class="picker" style="margin:-18px -20px;height:calc(100% + 36px)">
      <div class="left">
        <div class="tools">
          <${Search} value=${q} onInput=${setQ} placeholder="Search buses" autofocus />
          <div class="row wrap">
            <div class="grow" style="min-width:140px"><${Select} raw value=${maker} options=${[['', t('All makers')], ...makers.map((m) => [m, m])]} onChange=${setMaker} /></div>
            ${modes.length > 0 && html`<div class="grow" style="min-width:120px"><${Select} raw value=${mode} options=${[['', t('Any mode')], ...modes.map((m) => [m, modeLabel(m)])]} onChange=${setMode} /></div>`}
            <${Segmented} value=${show} options=${[['all', 'All'], ['fav', 'Starred'], ['mods', 'Mods']]} onChange=${setShow} />
          </div>
        </div>
        <div class="list">
          ${list.length === 0 && html`<${Empty} icon="bus" title=${show === 'fav' ? 'No starred bus' : 'No bus found'} text=${show === 'fav' ? 'Star a bus to find it here.' : null} />`}
          ${fams.map((f) => {
            const one = f.versions.length === 1;
            const shown = one || open.has(f.key) || !!ql;
            const has = f.versions.some((x) => x.v.file === sel);
            const row = (x, sub, indent) => html`<div class=${'list-item' + (x.file === sel ? ' on' : '')} style=${indent ? 'padding-left:30px' : ''} onClick=${() => (setSel(x.file), setPaint(x.file === s.choice.bus ? s.choice.paint : ''))} onDblClick=${() => (setSel(x.file), setTimeout(take))}>
              <div class="grow" style="min-width:0">
                <div class="title" style="overflow-wrap:anywhere" title=${sub}>${sub}</div>
                <div class="sub ellipsis">${[indent ? null : x.manufacturer, shownModes(x).map(modeLabel).join(', ') || null, x.paints.length ? t('{n} paints', { n: x.paints.length + 1 }) : null].filter(Boolean).join(' - ')}</div>
              </div>
              ${transportRefusal(map, x) && html`<span title=${transportRefusal(map, x)}><${Icon} name="alert" size=${16} /></span>`}
              ${x.missing_packs?.length > 0 && html`<span title=${t('Parts missing')}><${Icon} name="alert" size=${16} /></span>`}
              <button class=${'star' + (s.favourites.includes(favKey(x.file)) ? ' on' : '')} title=${t('Star')} onClick=${(e) => (e.stopPropagation(), toggleFavourite(x.file))}>
                <${Icon} name="star" fill=${s.favourites.includes(favKey(x.file))} />
              </button>
            </div>`;
            // (the type's name: the maker is on the line under it, #2355)
            if (one) return row(f.versions[0].v, f.versions[0].v.type_name || f.versions[0].v.name, false);
            return html`<div class=${'list-item' + (has && !shown ? ' on' : '')} onClick=${() => toggle(f.key)}>
                <div class="grow" style="min-width:0">
                  <div class="title" style="overflow-wrap:anywhere" title=${f.name}>${f.name}</div>
                  <div class="sub ellipsis">${[f.maker, t('{n} versions', { n: f.versions.length })].filter(Boolean).join(' - ')}</div>
                </div>
                <${Icon} name=${shown ? 'down' : 'right'} size=${18} />
              </div>
              ${shown && f.versions.map((x) => row(x.v, x.label, true))}`;
          })}
        </div>
      </div>
      <div class="detail">
        ${v ? html`<${BusPreview} look=${{ bus: v.file, paint, map: s.choice.map, weather: s.choice.weather, time: s.choice.time, date: s.choice.date }} />
          <div class="card-body stack">
            <div class="row">
              <div class="grow"><h2 style="font-size:20px">${v.name}</h2><div class="muted">${[v.manufacturer, v.type_name, ...shownModes(v).map(modeLabel)].filter(Boolean).join(' - ')}</div></div>
              ${v.installed && html`<${Tag}>${t('Mod')}</${Tag}>`}
            </div>
            ${transportRefusal(map, v) && html`<${Notice} kind="warn">${transportRefusal(map, v)} ${t('Choose another map or vehicle.')}</${Notice}>`}
            ${v.missing_packs?.length > 0 && html`<${Notice} kind="warn">${t('Parts of this bus come from packs that are not installed: {packs}', { packs: v.missing_packs.join(', ') })}</${Notice}>`}
            <${Field} label="Paint">
              <${Select} raw value=${paint} options=${[['', v.default_paint ? `${t('Standard')} (${v.default_paint})` : t('Standard')], ...v.paints.map((p) => [p, p])]} onChange=${setPaint} />
            </${Field}>
            ${v.description && html`<p class="dim selectable" style="white-space:pre-line;max-height:220px;overflow:auto">${v.description}</p>`}
            <${Notes} kind="bus" file=${v.file} />
            <div class="muted small selectable">${v.file}</div>
          </div>`
          : html`<${Empty} icon="bus" title="Choose a bus on the left" />`}
      </div>
    </div>
  </${Dialog}>`;
}

