// The map picker: every installed map as a picture with its name.

import { html, useState } from '../../vendor/preact-htm.js';
import { useStore } from '../store.js';
import { t } from '../i18n.js';
import { Dialog, Search, Picture, Segmented, Empty } from '../ui.js';
import { choose, MODES, modeLabel, shownModes } from '../data.js';
import { mapPicture } from './drive.js';

export function MapPicker({ onClose }) {
  const s = useStore();
  const [q, setQ] = useState('');
  const [which, setWhich] = useState('all');
  const [mode, setMode] = useState('');
  const ql = q.trim().toLowerCase();
  const maps = (s.maps || [])
    .filter((m) => which === 'all' || (which === 'mods') === !!m.installed)
    .filter((m) => !mode || !m.transport?.length || m.transport.includes(mode))
    .filter((m) => !ql || `${m.friendly} ${m.name} ${m.file}`.toLowerCase().includes(ql))
    .sort((a, b) => (a.friendly || a.name).localeCompare(b.friendly || b.name));
  const mods = (s.maps || []).filter((m) => m.installed).length;
  const modes = MODES.filter((x) => (s.maps || []).some((m) => m.transport?.includes(x)));
  return html`<${Dialog} wide title="Choose a map" onClose=${onClose}>
    <div class="row" style="margin-bottom:16px">
      <div class="grow" style="max-width:360px"><${Search} value=${q} onInput=${setQ} placeholder="Search maps" autofocus /></div>
      ${mods > 0 && html`<${Segmented} value=${which} options=${[['all', 'All'], ['stock', 'OMSI 2'], ['mods', 'Mods']]} onChange=${setWhich} />`}
      ${modes.length > 0 && html`<${Segmented} value=${mode} options=${[['', 'Any mode'], ...modes.map((x) => [x, modeLabel(x)])]} onChange=${setMode} />`}
      <span class="muted small right">${t('{n} maps', { n: maps.length })}</span>
    </div>
    ${maps.length === 0 && html`<${Empty} icon="map" title="No map found" />`}
    <div class="tiles">
      ${maps.map((m) => html`<button class=${'tile' + (m.file === s.choice.map ? ' on' : '')} onClick=${() => (choose({ map: m.file, entry: -1, start_trip: null }), onClose())}>
        <${Picture} src=${mapPicture(m.file)} />
        <div class="cap"><b>${m.friendly || m.name}</b><span>${shownModes(m).length ? `${shownModes(m).map(modeLabel).join(', ')} - ` : ''}${m.installed ? t('Mod') : 'OMSI 2'} - ${t('{n} entry points', { n: m.entry_points.length })}</span></div>
      </button>`)}
    </div>
  </${Dialog}>`;
}
