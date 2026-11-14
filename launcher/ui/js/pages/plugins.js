// Plugins (#3919, #3920): switch, check and set up the game's plugins; install reviewed ones.

import { html, useState, useEffect } from '../../vendor/preact-htm.js';
import { useStore, toast } from '../store.js';
import { t } from '../i18n.js';
import { call } from '../api.js';
import { Icon } from '../icons.js';
import { Button, Card, Dialog, Empty, Notice, PageHead, Search, Select, Slider, Spinner, Tabs, Tag, Toggle, Input, useConfirm } from '../ui.js';
import { cached } from '../data.js';

const KIND = { lua: 'Lua', oop: '.oop', opl: 'OMSI' };
const STATE = {
  loaded: ['ok', 'Ran in the last game'],
  failed: ['danger', 'Did not load in the last game'],
  switched_off: ['warn', 'Stopped in the last game after errors'],
};

/** The value a setting has: the saved one, else its default. */
export function settingValue(setting, values) {
  const v = values?.[setting.key];
  return v === undefined || v === null ? setting.default : v;
}

/** One control of a plugin's settings form. */
function SettingControl({ s, value, onChange }) {
  const label = s.label || s.key;
  if (s.type === 'bool') return html`<${Toggle} label=${label} checked=${!!value} onChange=${onChange} />`;
  if (s.type === 'number') {
    return html`<${Slider} label=${label} value=${value} min=${s.min ?? 0} max=${s.max ?? 100} step=${s.step || 'any'} onChange=${onChange} />`;
  }
  if (s.type === 'choice') {
    return html`<div class="field"><label>${label}</label><${Select} raw value=${value} options=${(s.choices || []).map((c) => [c, c])} onChange=${onChange} /></div>`;
  }
  return html`<div class="field"><label>${label}</label><${Input} value=${value} onInput=${onChange} maxlength="256" /></div>`;
}

function SettingsDialog({ plugin, onClose, onSaved }) {
  const [values, setValues] = useState(() => Object.fromEntries((plugin.settings || []).map((s) => [s.key, settingValue(s, plugin.values)])));
  const [saving, setSaving] = useState(false);
  const save = async () => {
    setSaving(true);
    try {
      await call('plugin_settings_save', { name: plugin.name, values });
      toast(t('Saved'), 'ok');
      onSaved();
      onClose();
    } catch (e) {
      toast(t(e.message), 'error');
    }
    setSaving(false);
  };
  return html`<${Dialog} title=${plugin.title} onClose=${onClose} footer=${html`
      <${Button} label="Cancel" onClick=${onClose} />
      <${Button} kind="primary" label="Save" disabled=${saving} onClick=${save} />`}>
    <div class="stack">
      ${(plugin.settings || []).map((s) => html`<${SettingControl} key=${s.key} s=${s} value=${values[s.key]} onChange=${(v) => setValues((x) => ({ ...x, [s.key]: v }))} />`)}
    </div>
  </${Dialog}>`;
}

/** The catalog: the plugins of openOMSI-org/openOMSI-Plugins, to install, update or remove. */
function Browse({ onChanged }) {
  const [list, setList] = useState(null);
  const [error, setError] = useState('');
  const [q, setQ] = useState('');
  const [busy, setBusy] = useState({});
  const [ask, confirm] = useConfirm();
  const read = () => call('plugin_catalog').then((c) => (setList(c.plugins || []), setError(''))).catch((e) => setError(e.message));
  useEffect(() => {
    read();
  }, []);
  const source = (p) => call('open_url', { url: p.source }).catch((e) => toast(t(e.message), 'error'));
  const install = async (p) => {
    if (!p.installed) {
      const ok = await ask({ title: t('Install {name}?', { name: p.name }), text: ['Plugins run code in your game. The catalog\'s plugins are open source and read by the openOMSI maintainers before they are added; you can read the code first.'], yes: 'Install' });
      if (!ok) return;
    }
    setBusy((b) => ({ ...b, [p.id]: true }));
    try {
      await call('plugin_install', { id: p.id });
      toast(t(p.installed ? '{name} updated' : '{name} installed', { name: p.name }), 'ok');
      await read();
      onChanged();
    } catch (e) {
      toast(t(e.message), 'error');
    }
    setBusy((b) => ({ ...b, [p.id]: false }));
  };
  const remove = async (p) => {
    if (!(await ask({ title: t('Remove {name}?', { name: p.name }), text: ['Its files, settings and saved data are deleted.'], yes: 'Remove', danger: true }))) return;
    setBusy((b) => ({ ...b, [p.id]: true }));
    try {
      await call('plugin_remove', { id: p.id });
      toast(t('{name} removed', { name: p.name }), 'ok');
      await read();
      onChanged();
    } catch (e) {
      toast(t(e.message), 'error');
    }
    setBusy((b) => ({ ...b, [p.id]: false }));
  };
  const shown = (list || []).filter((p) => !q || `${p.name} ${p.author} ${p.description} ${(p.tags || []).join(' ')}`.toLowerCase().includes(q.toLowerCase()));
  return html`
    <${Card} title="Browse" hint="Reviewed, open source plugins of the openOMSI plugin catalog." actions=${html`<div style="width:220px;max-width:100%"><${Search} value=${q} onInput=${setQ} /></div>`}>
      ${error && html`<${Notice} kind="danger"><span class="selectable">${t(error)}</span> <${Button} small kind="ghost" icon="refresh" label="Refresh" onClick=${read} /></${Notice}>`}
      ${list == null && !error
        ? html`<div class="row muted"><${Spinner} /></div>`
        : list && shown.length === 0
          ? html`<${Empty} icon="puzzle" text="No plugin matches." />`
          : html`<div class="list" style="margin:0 -8px">${shown.map((p) => html`<div class="list-item" key=${p.id}>
              <${Icon} name="puzzle" />
              <div class="grow" style="min-width:0">
                <div class="title">${p.name} <span class="muted small">${p.version}</span></div>
                <div class="sub">${[p.author, p.license, ...(p.tags || [])].join(' - ')}</div>
                <div class="small dim" style="margin-top:2px">${p.description}</div>
                ${p.update && html`<div style="margin-top:4px"><${Tag} kind="warn">${t('Update available')}: ${p.installed} → ${p.version}</${Tag}></div>`}
              </div>
              ${busy[p.id] && html`<${Spinner} />`}
              <${Button} small kind="ghost" icon="external" title="Read the code" onClick=${() => source(p)} />
              ${p.installed && html`<${Button} small kind="ghost" icon="trash" title="Remove" disabled=${!!busy[p.id]} onClick=${() => remove(p)} />`}
              ${!p.installed
                ? html`<${Button} small kind="primary" icon="download" label="Install" disabled=${!!busy[p.id]} onClick=${() => install(p)} />`
                : p.update
                  ? html`<${Button} small kind="primary" icon="download" label="Update" disabled=${!!busy[p.id]} onClick=${() => install(p)} />`
                  : html`<${Tag} kind="ok">${t('Installed')}</${Tag}>`}
            </div>`)}</div>`}
    </${Card}>
    ${confirm}
  `;
}

export function PluginsPage() {
  const s = useStore();
  const [tab, setTab] = useState('installed');
  const status = s.cache.plugins ?? null;
  const [busy, setBusy] = useState({});
  const [editing, setEditing] = useState(null);
  const read = () => cached('plugins', 'plugins_status');
  useEffect(() => {
    read();
  }, []);
  const list = status?.plugins || [];
  const toggle = async (p, on) => {
    setBusy((b) => ({ ...b, [p.name]: true }));
    try {
      await call('plugin_set_enabled', { name: p.name, on });
      toast(t(on ? '{name} switched on' : '{name} switched off', { name: p.title }), 'ok');
      read();
    } catch (e) {
      toast(t(e.message), 'error');
    }
    setBusy((b) => ({ ...b, [p.name]: false }));
  };
  const dir = status?.dirs?.[0];
  return html`
    ${editing && html`<${SettingsDialog} plugin=${editing} onClose=${() => setEditing(null)} onSaved=${read} />`}
    <${PageHead} title="Plugins" text="Lua and OMSI plugins in the game's plugins folders. One switched off here is left out of the next game; its files and settings stay." />
    ${s.settings?.disable_plugins && html`<${Notice} kind="warn">${t('Plugins are disabled in Settings, so none of them runs.')}</${Notice}>`}
    <div style="margin-bottom:12px"><${Tabs} value=${tab} options=${[['installed', 'Installed'], ['browse', 'Browse']]} onChange=${setTab} /></div>
    ${tab === 'browse' ? html`<${Browse} onChanged=${read} />` : html`<${Card} title="Installed" actions=${dir && html`<${Button} small icon="folder" label="Plugins folder" onClick=${() => call('open_path', { path: dir }).catch((e) => toast(t(e.message), 'error'))} />`}>
      ${status == null
        ? html`<div class="row muted"><${Spinner} /></div>`
        : list.length === 0
          ? html`<${Empty} icon="puzzle" text="No plugin yet. Put a .lua file, a plugin folder or a .oop file into the plugins folder." />`
          : html`<div class="list" style="margin:0 -8px">${list.map((p) => {
              const state = p.enabled ? STATE[p.state] : null;
              return html`<div class="list-item" key=${p.name + p.kind}>
                <${Icon} name=${state?.[0] === 'danger' ? 'alert' : 'puzzle'} />
                <div class="grow" style="min-width:0">
                  <div class="title ellipsis" style=${p.enabled ? '' : 'opacity:.55'}>${p.title}</div>
                  <div class="sub ellipsis" title=${p.path}>${[KIND[p.kind] || p.kind, p.catalog && `v${p.catalog}`, p.path].filter(Boolean).join(' - ')}</div>
                  ${state && html`<div class="row" style="gap:6px;margin-top:4px;flex-wrap:wrap"><${Tag} kind=${state[0]}>${t(state[1])}</${Tag}>${p.error && html`<span class="small selectable" style="color:var(--danger);overflow-wrap:anywhere">${p.error}</span>`}</div>`}
                </div>
                ${busy[p.name] && html`<${Spinner} />`}
                ${p.settings && html`<${Button} small kind="ghost" icon="sliders" title="Settings" onClick=${() => setEditing(p)} />`}
                <${Toggle} checked=${p.enabled} disabled=${!!busy[p.name]} onChange=${(on) => toggle(p, on)} />
              </div>`;
            })}</div>`}
    </${Card}>`}
  `;
}
