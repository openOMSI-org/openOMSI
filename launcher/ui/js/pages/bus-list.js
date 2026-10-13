import { html, useState, useMemo, useEffect, useRef } from '../../vendor/preact-htm.js';
import { S, useStore, remembered, remember } from '../store.js';
import { t } from '../i18n.js';
import { Icon } from '../icons.js';
import { Search, Segmented, Empty, Spinner, Notice, Field, Select, Input, Button, Tag } from '../ui.js';
import { choose, curBus, busFile, serverBuses, noServerBus } from '../data.js';
import { families, toggleFavourite } from './bus-picker.js';

const favKey = (file) => file.replace(/\\/g, '/').toLowerCase();

export function withoutMaker(name, maker) {
  if (!maker || !name.toLowerCase().startsWith(maker.toLowerCase())) return name;
  const rest = name.slice(maker.length).replace(/^[\s\-–|:,/]+/, '');
  return rest || name;
}

export const isFavourite = (file) => S.favourites.includes(favKey(file));

function takeBus(file) {
  const c = S.choice;
  if (file === c.bus) return;
  choose({ bus: file, paint: '', number: '', hof_manual: false });
}

const press = (fn) => ({
  tabIndex: 0,
  onClick: fn,
  onKeyDown: (e) => {
    if (e.target !== e.currentTarget || (e.key !== 'Enter' && e.key !== ' ')) return;
    e.preventDefault();
    fn();
  },
});

function Star({ file }) {
  const on = isFavourite(file);
  return html`<button type="button" class=${'star' + (on ? ' on' : '')} title=${t(on ? 'Remove from the favourites' : 'Add to the favourites')}
    onClick=${(e) => (e.stopPropagation(), toggleFavourite(file))}><${Icon} name="star" /></button>`;
}

function BusRow({ v, label, sub, indent, chosen }) {
  return html`<div class=${'bus-row bus' + (indent === 2 ? ' deep' : '') + (chosen ? ' on' : '')} aria-current=${chosen} ...${press(() => takeBus(v.file))}>
    <div class="grow">
      <div class="title ellipsis" title=${label}>${label}</div>
      ${sub && html`<div class="sub ellipsis">${sub}</div>`}
    </div>
    ${v.missing_packs?.length > 0 && html`<span class="warn-fg" title=${t('Parts missing')}><${Icon} name="alert" size=${16} /></span>`}
    ${v.installed && html`<${Tag}>${t('Mod')}</${Tag}>`}
    <${Star} file=${v.file} />
  </div>`;
}

function FamilyRows({ f, chosen, open, toggle, all }) {
  if (f.versions.length === 1) {
    const v = f.versions[0].v;
    const label = withoutMaker(v.name, f.maker);
    const paints = v.paints.length ? t('{n} paints', { n: v.paints.length + 1 }) : null;
    return html`<${BusRow} v=${v} label=${label} sub=${paints} indent=${1} chosen=${v.file === chosen} />`;
  }
  const shown = all || open.has(f.key);
  const has = f.versions.some((x) => x.v.file === chosen);
  return html`<div class=${'bus-row family' + (has ? ' has' : '')} aria-expanded=${shown} ...${press(() => toggle(f.key))}>
      <div class="grow">
        <div class="title ellipsis" title=${f.name}>${withoutMaker(f.name, f.maker)}</div>
        <div class="sub ellipsis">${t('{n} versions', { n: f.versions.length })}</div>
      </div>
      <${Icon} name=${shown ? 'down' : 'right'} size=${18} />
    </div>
    ${shown && f.versions.map((x) => html`<${BusRow} key=${x.v.file} v=${x.v} label=${x.label} indent=${2} chosen=${x.v.file === chosen} />`)}`;
}

export function BusList() {
  const s = useStore();
  const [q, setQ] = useState('');
  const [show, setShow] = useState('all');
  const joined = serverBuses(s.choice);
  const all = (s.vehicles || []).filter((v) => !joined || joined.has(busFile(v.file)));
  const ql = q.trim().toLowerCase();
  const list = all
    .filter((v) => show !== 'fav' || s.favourites.includes(favKey(v.file)))
    .filter((v) => show !== 'mods' || v.installed)
    .filter((v) => !ql || `${v.name} ${v.manufacturer} ${v.type_name} ${v.folder}`.toLowerCase().includes(ql));
  const fams = useMemo(() => families(list), [list.map((x) => x.file).join('|')]);
  const makers = useMemo(() => {
    const by = new Map();
    for (const f of fams) {
      if (!by.has(f.maker)) by.set(f.maker, []);
      by.get(f.maker).push(f);
    }
    return [...by.entries()].map(([maker, fs]) => ({ maker, fams: fs, count: fs.reduce((n, f) => n + f.versions.length, 0) }));
  }, [fams]);
  const chosen = s.choice.bus;
  const [open, setOpen] = useState(() => new Set());
  const box = useRef();
  useEffect(() => {
    const f = fams.find((x) => x.versions.some((y) => y.v.file === chosen));
    if (!f) return;
    setOpen((o) => new Set([...o, 'm:' + f.maker, f.key]));
    requestAnimationFrame(() => box.current?.querySelector('.bus-row.on')?.scrollIntoView({ block: 'nearest' }));
  }, [!!s.vehicles, chosen]);
  const toggle = (key) => setOpen((o) => {
    const n = new Set(o);
    if (n.has(key)) n.delete(key);
    else n.add(key);
    return n;
  });
  if (!s.vehicles) {
    return html`<div class="row muted"><${Spinner} /> ${s.vehiclesProgress ? t('Reading the buses… {done} of {total}', s.vehiclesProgress) : t('Reading the buses…')}</div>`;
  }
  if (!all.length) return html`<${Notice} kind="warn">${t(joined ? 'The server offers none of the vehicles installed here' : 'No bus is installed.')}</${Notice}>`;
  const everything = !!ql || show !== 'all';
  return html`<div class="bus-pick">
    <${Search} value=${q} onInput=${setQ} placeholder="Search buses" />
    <div class="row">
      <span class="muted small grow">${t('{n} buses', { n: list.length })}</span>
      <${Segmented} value=${show} options=${[['all', 'All'], ['fav', 'Starred'], ['mods', 'Mods']]} onChange=${setShow} />
    </div>
    <div class="bus-list" ref=${box}>
      ${makers.length === 0 && html`<${Empty} icon="bus" title=${show === 'fav' ? 'No starred bus' : 'No bus found'} text=${show === 'fav' ? 'Star a bus to find it here.' : null} />`}
      ${makers.map((m) => {
        const key = 'm:' + m.maker;
        if (m.count === 1) {
          const v = m.fams[0].versions[0].v;
          return html`<div key=${key} class=${'bus-row maker' + (v.file === chosen ? ' on' : '')} aria-current=${v.file === chosen} ...${press(() => takeBus(v.file))}>
            <${Icon} name="bus" size=${20} />
            <div class="grow">
              <div class="title ellipsis">${m.maker || v.name}</div>
              <div class="sub ellipsis">${m.maker ? withoutMaker(v.name, m.maker) : v.type_name}</div>
            </div>
            <${Star} file=${v.file} />
          </div>`;
        }
        const shown = everything || open.has(key);
        const has = m.fams.some((f) => f.versions.some((x) => x.v.file === chosen));
        return html`<div key=${key} class=${'bus-row maker' + (has ? ' has' : '') + (shown ? ' open' : '')} aria-expanded=${shown} ...${press(() => toggle(key))}>
            <${Icon} name="bus" size=${20} />
            <div class="grow">
              <div class="title ellipsis">${m.maker || t('Other')}</div>
              <div class="sub ellipsis">${t('{n} buses', { n: m.count })}</div>
            </div>
            <${Icon} name=${shown ? 'down' : 'right'} size=${18} />
          </div>
          ${shown && m.fams.map((f) => html`<${FamilyRows} key=${f.key} f=${f} chosen=${chosen} open=${open} toggle=${toggle} all=${!!ql} />`)}`;
      })}
    </div>
  </div>`;
}

export function BusOptions() {
  const s = useStore();
  const c = s.choice;
  const v = curBus();
  const [more, setMore] = useState(() => remembered('bus-details', '0') === '1');
  if (!v) return null;
  const paints = [['', v.default_paint ? `${t('Standard')} (${v.default_paint})` : t('Standard')], ...v.paints.map((p) => [p, p])];
  const at = Math.max(0, paints.findIndex(([p]) => p === c.paint));
  const step = (d) => choose({ paint: paints[(at + d + paints.length) % paints.length][0] });
  const numbers = v.numbers || [];
  const hofs = [['', `${t('Automatic')}${s.hof ? ` (${s.hof})` : ''}`], ...v.hofs.map((h) => [h, h])];
  const flip = () => {
    setMore(!more);
    remember('bus-details', more ? '0' : '1');
  };
  return html`<div class="stack bus-options">
    ${noServerBus() && html`<${Notice} kind="danger">${t('The server offers none of the vehicles installed here')}</${Notice}>`}
    <${Field} label="Livery">
      <div class="row livery">
        <div class="grow"><${Select} raw value=${c.paint} options=${paints} onChange=${(paint) => choose({ paint })} /></div>
        <${Button} icon="left" title="Preview previous livery" disabled=${paints.length < 2} onClick=${() => step(-1)} />
        <${Button} icon="right" title="Preview next livery" disabled=${paints.length < 2} onClick=${() => step(1)} />
        <span class="muted small num count">${at + 1} / ${paints.length}</span>
      </div>
    </${Field}>
    ${v.missing_packs?.length > 0 && html`<${Notice} kind="warn">${t('Parts of this bus come from packs that are not installed: {packs}', { packs: v.missing_packs.join(', ') })}</${Notice}>`}
    <button type="button" class="disclose" aria-expanded=${more} onClick=${flip}>
      <${Icon} name=${more ? 'down' : 'right'} size=${18} /><span>${t('Vehicle settings & details')}</span>
    </button>
    ${more && html`<div class="stack" style="gap:12px">
      <div class="grid-2">
        ${numbers.length > 0 && html`<${Field} label="Fleet number">
          <${Select} raw value=${c.number} options=${[['', `${t('Automatic')} (${numbers[0][0]})`], ...numbers.map(([n, p]) => [n, p ? `${n} - ${p}` : n])]} onChange=${(number) => choose({ number })} />
        </${Field}>`}
        <${Field} label="Number plate"><${Input} value=${c.plate} placeholder="Automatic" onInput=${(plate) => choose({ plate })} /></${Field}>
      </div>
      <${Field} label="Depot file">
        <${Select} raw value=${c.hof_manual ? c.hof : ''} options=${hofs} onChange=${(h) => choose({ hof: h, hof_manual: !!h })} />
      </${Field}>
      ${v.description && html`<p class="dim selectable bus-about">${v.description}</p>`}
      <div class="muted small selectable ellipsis" title=${v.file}>${v.file}</div>
    </div>`}
  </div>`;
}
