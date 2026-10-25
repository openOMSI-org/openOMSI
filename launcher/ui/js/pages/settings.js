// Settings: one tab at a time (the whole page at once was taller than two screens), each in
// two columns. What the tabs hold is in settings-schema.js; here they are drawn, and every
// change goes to settings.cfg at once.

import { html, useState, useEffect, useRef } from '../../vendor/preact-htm.js';
import { S, useStore, set, toast, remembered, remember } from '../store.js';
import { t } from '../i18n.js';
import { call, onPhone, inTauri } from '../api.js';
import { Button, Card, Select, Input, Toggle, Tabs, Slider, PageHead, useConfirm } from '../ui.js';
import { saveSettings, applyTheme, fitWindow, launcherSize, LAUNCHER_SIZES, launcherLayout, LAUNCHER_LAYOUTS } from '../data.js';
import { UpdateRow, checkUpdates } from '../updates.js';
import { SETTINGS_TABS, GRAPHICS_PROFILE_KEYS } from './settings-schema.js';
import { VersionsDialog } from './versions.js';
import { RadioStations } from './radio-stations.js';
import { AiListsPage } from './ai-lists.js';
import { OptimiseBlock } from './optimise.js';

const REPO_URL = 'https://github.com/openOmsi-project/openOMSI';

/** A number, also of the text a select keeps ("0.65"); NaN for anything else. */
function num(x) {
  if (typeof x === 'number') return x;
  if (typeof x === 'string' && x.trim() !== '' && isFinite(Number(x))) return Number(x);
  return NaN;
}

/** Whether a stored value is the one a preset or a choice means: 4 and "4" are. */
function same(cur, v) {
  if (cur === v) return true;
  if (typeof cur === 'boolean' || typeof v === 'boolean') return false;
  if (typeof cur !== 'number' && typeof v !== 'number') return false;
  const a = num(cur);
  const b = num(v);
  return !isNaN(a) && !isNaN(b) && Math.abs(a - b) < 1e-6;
}

/** A choice kept in the type the setting already has (the game reads `msaa` as a number,
 * `time_speed` as text). */
function asStored(cur, v) {
  if (typeof cur === 'boolean') return v === '1';
  if (typeof cur === 'number') {
    const f = parseFloat(v);
    return isFinite(f) ? f : v;
  }
  return v;
}

const visible = (row, s, platform) => !row.when || row.when(s, platform);

export function SettingsPage() {
  const s = useStore();
  const [tab, setTab] = useState(() => {
    const id = remembered('settings-tab', 'graphics');
    return SETTINGS_TABS.some((x) => x.id === id) ? id : 'graphics';
  });
  const [ask, dialog] = useConfirm();
  if (!s.settings) return null;
  const platform = s.boot?.platform || '';
  // (the model test runs the game program: not on a phone)
  const phone = ['android', 'ios'].includes(platform);
  const tabs = SETTINGS_TABS.filter((x) => visible(x, s.settings, platform));
  const current = tabs.find((x) => x.id === tab) || tabs[0];
  const pick = (id) => {
    // (Troubleshooting is no tab of its own: it opens that page at its model test)
    if (id === 'troubleshoot') {
      remember('troubleshoot-tab', 'content');
      return set({ page: 'troubleshoot' });
    }
    setTab(id);
    remember('settings-tab', id);
  };
  return html`
    <${PageHead} title="Settings" text="Every change is saved at once; the game reads them when it starts." />
    <${Tabs} value=${current.id} options=${[...tabs.map((x) => [x.id, x.label]), ...(phone ? [] : [['troubleshoot', 'Troubleshooting']])]} onChange=${pick} />
    ${current.page === 'ailists'
      ? html`<div key=${tab}><${AiListsPage} /></div>`
      : html`<div class="settings-grid" key=${tab}>
      ${current.cols.map((col) => html`<${Card} title=${col.title}>
        ${col.rows.filter((row) => visible(row, s.settings, platform)).map((row, i) => html`<${Row} key=${i + (row.key || row.id || row.label || row.title || '')} row=${row} ask=${ask} />`)}
      </${Card}>`)}
    </div>`}
    ${dialog}
  `;
}

export function settingRow(key) {
  for (const tab of SETTINGS_TABS) for (const col of tab.cols) for (const row of col.rows) if (row.key === key) return row;
  return null;
}

export function SettingRow({ row, ask }) {
  return row ? html`<${Row} row=${row} ask=${ask} />` : null;
}

function Row({ row, ask }) {
  switch (row.type) {
    case 'section':
      return html`<div class="section-title">${t(row.title)}</div>`;
    case 'note':
      return html`<p class="muted small" style="margin:8px 0">${t(row.text)}</p>`;
    case 'select':
      return html`<${SelectRow} row=${row} />`;
    case 'toggle':
      return html`<${ToggleRow} row=${row} />`;
    case 'slider':
      return html`<${SliderRow} row=${row} />`;
    case 'button':
      return html`<${ButtonRow} row=${row} />`;
    case 'custom':
      return CUSTOM[row.id]?.({ ask }) || null;
  }
  return null;
}

function Name({ label, help, warn }) {
  return html`<div class="name">${t(label)}${help && html`<span>${t(help)}</span>`}${warn && html`<span class="warn-fg">${t(warn)}</span>`}</div>`;
}

function SelectRow({ row }) {
  const s = S.settings;
  const options = typeof row.options === 'function' ? row.options(s, S.boot) : row.options;
  const cur = s[row.key];
  // the choice the value means ("4" for 4); a value written by hand shows as itself
  const match = options.find(([v]) => same(cur, v) || String(cur) === String(v));
  const value = match ? match[0] : cur == null ? '' : typeof cur === 'boolean' ? (cur ? '1' : '0') : String(cur);
  return html`<div class="setting-row">
    <${Name} label=${row.label} help=${row.help} warn=${row.heavy?.(value, s) && 'Heavy: can cost many frames per second on most graphics cards'} />
    <${Select} value=${value} options=${options} raw=${row.raw} onChange=${async (v) => (await saveSettings({ [row.key]: asStored(cur, v) })) && row.recheckUpdates && checkUpdates()} />
  </div>`;
}

function ToggleRow({ row }) {
  return html`<div class="setting-row">
    <${Name} label=${row.label} help=${row.help} />
    <${Toggle} checked=${!!S.settings[row.key]} onChange=${(v) => saveSettings({ [row.key]: v })} />
  </div>`;
}

/** A slider shows its value at once and saves it a moment after the hand stops (a save per
 * step would write settings.cfg dozens of times a drag). */
function SliderRow({ row }) {
  const key = typeof row.key === 'function' ? row.key(S.settings) : row.key;
  const stored = num(S.settings[key]);
  const [shown, setShown] = useState(null);
  const pending = useRef(null);
  const timer = useRef(0);
  const flush = () => {
    clearTimeout(timer.current);
    if (pending.current) saveSettings(pending.current);
    pending.current = null;
  };
  // (a tab left within the moment still keeps the change)
  useEffect(() => flush, []);
  useEffect(() => {
    flush();
    setShown(null);
  }, [key]);
  const onChange = (v) => {
    setShown(v);
    pending.current = { [key]: row.store ? row.store(v) : v, ...(row.also || {}) };
    clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      flush();
      setShown(null);
    }, 250);
  };
  const value = shown ?? (isNaN(stored) ? row.def : stored);
  return html`<div class="setting-row full">
    <${Slider} label=${row.label} value=${value} min=${row.min} max=${row.max} step=${row.step} format=${row.format} onChange=${onChange} />
  </div>`;
}

function ButtonRow({ row }) {
  const click = () => {
    if (row.patch) saveSettings(row.patch(S.settings));
    // (the Controls page opens at the tab asked for: the keys, or the game controllers)
    if (row.controls != null) set({ page: 'controls', controlsTab: row.controls });
  };
  return html`<div class="setting-row full">
    <div><${Button} kind=${row.kind || ''} icon=${row.icon} label=${row.label} onClick=${click} /></div>
  </div>`;
}

// --- rows of their own ------------------------------------------------------------------

const CUSTOM = {
  preset: () => html`<${PresetRow} />`,
  optimise: () => html`<${OptimiseBlock} />`,
  profiles: () => html`<${ProfilesBlock} />`,
  theme: () => html`<${ThemeRow} />`,
  launcher_layout: () => !onPhone && html`<${LauncherLayoutRow} />`,
  launcher_size: () => inTauri && html`<${LauncherSizeRow} />`,
  corner: () => html`<${CornerRow} />`,
  version: () => html`<${VersionRow} />`,
  reset: ({ ask }) => html`<${ResetRow} ask=${ask} />`,
  support: () => html`<${SupportRow} />`,
  radio: () => html`<${RadioStations} />`,
};

/** The quality presets of the graphics mode chosen: the one the settings match, else
 * "Custom". A preset keeps the mode. */
function PresetRow() {
  const mode = S.settings.graphics || 'vanilla_plus';
  const [presets, setPresets] = useState([]);
  useEffect(() => {
    let gone = false;
    call('graphics_presets', { mode })
      .then((p) => !gone && setPresets(p))
      .catch(() => !gone && setPresets([]));
    return () => (gone = true);
  }, [mode]);
  const s = S.settings;
  const matched = presets.find((p) => Object.entries(p.settings).every(([k, v]) => same(s[k], v)));
  const options = [...presets.map((p) => [p.name, p.name]), ['', 'Custom']];
  return html`<div class="setting-row">
    <${Name} label="Quality preset" />
    <${Select} value=${matched ? matched.name : ''} options=${options} onChange=${(name) => {
      const p = presets.find((x) => x.name === name);
      if (p) saveSettings(p.settings);
    }} />
  </div>`;
}

/** Save, load and delete the graphics settings as named profiles. */
function ProfilesBlock() {
  const [names, setNames] = useState(null);
  const [sel, setSel] = useState('');
  const [name, setName] = useState('');
  const [msg, setMsg] = useState('');
  const reload = async (pick) => {
    const all = await call('graphics_profiles').catch(() => ({}));
    const list = Object.keys(all || {});
    setNames(list);
    setSel(pick != null && list.includes(pick) ? pick : list.includes(sel) ? sel : list[0] || '');
  };
  useEffect(() => {
    reload();
  }, []);
  const list = names || [];
  const load = async () => {
    if (!sel) return;
    const all = await call('graphics_profiles').catch(() => ({}));
    const p = all?.[sel];
    if (!p) {
      setMsg(t('"{name}" is gone.', { name: sel }));
      return;
    }
    // (only what a profile may hold: not the machine's own fullscreen or graphics API)
    const patch = {};
    for (const k of GRAPHICS_PROFILE_KEYS) if (k in p) patch[k] = p[k];
    await saveSettings(patch);
    setMsg(t('Loaded "{name}".', { name: sel }));
  };
  const remove = async () => {
    if (!sel) return;
    try {
      await call('graphics_profile_delete', { name: sel });
      setMsg(t('Deleted "{name}".', { name: sel }));
    } catch (e) {
      setMsg(e.message);
    }
    reload();
  };
  const save = async () => {
    try {
      const kept = await call('graphics_profile_save', { name, settings: S.settings });
      setName(kept);
      setMsg(t('Saved "{name}".', { name: kept }));
      reload(kept);
    } catch (e) {
      setMsg(e.message);
    }
  };
  return html`
    <div class="setting-row">
      <${Name} label="Saved profile" />
      <${Select} raw value=${sel} disabled=${!list.length} options=${list.length ? list.map((n) => [n, n]) : [['', t('No saved profiles')]]} onChange=${(n) => {
        setSel(n);
        setName(n);
      }} />
    </div>
    <div class="setting-row full">
      <div class="grid-2">
        <${Button} icon="download" label="Load" disabled=${!list.length} onClick=${load} />
        <${Button} kind="danger" icon="trash" label="Delete" disabled=${!list.length} onClick=${remove} />
      </div>
    </div>
    <div class="setting-row full">
      <div class="stack" style="gap:10px">
        <${Input} value=${name} placeholder="Profile name" onInput=${setName} maxlength="40" />
        <${Button} kind="primary" icon="save" label="Save current graphics as profile" onClick=${save} />
      </div>
    </div>
    ${msg && html`<p class="muted small" style="margin:8px 0">${msg}</p>`}
  `;
}

/** The launcher's own look; the game does not read it, so it stays in this window. */
function ThemeRow() {
  const [theme, setTheme] = useState(() => remembered('theme', 'dark'));
  return html`<div class="setting-row">
    <${Name} label="Launcher theme" />
    <${Select} value=${theme} options=${[['dark', 'Dark'], ['light', 'Light theme'], ['system', 'System']]} onChange=${(v) => {
      setTheme(v);
      remember('theme', v);
      try {
        applyTheme();
      } catch {
        /* storage blocked: the look stays as it is */
      }
    }} />
  </div>`;
}

function LauncherLayoutRow() {
  const [layout, setLayout] = useState(launcherLayout);
  return html`<div class="setting-row">
    <${Name} label="Launcher layout" />
    <${Select} value=${layout} options=${LAUNCHER_LAYOUTS} onChange=${(v) => {
      setLayout(v);
      remember('launcher_layout', v);
      applyTheme();
    }} />
  </div>`;
}

function LauncherSizeRow() {
  const [size, setSize] = useState(launcherSize);
  return html`<div class="setting-row">
    <${Name} label="Launcher size" help="Grows with the window; this sets how big it is" />
    <${Select} value=${size} options=${LAUNCHER_SIZES} onChange=${(v) => {
      setSize(v);
      remember('launcher_size', v);
      fitWindow();
    }} />
  </div>`;
}

const CORNERS = [
  ['top-left', 'Top left'],
  ['top-right', 'Top right'],
  ['bottom-left', 'Bottom left'],
  ['bottom-right', 'Bottom right'],
];

/** Where the navigator sits: a little screen with four corners to click. */
function CornerRow() {
  const cur = S.settings.navigator_corner || 'bottom-left';
  return html`<div class="setting-row">
    <${Name} label="Corner" />
    <div style="display:grid;grid-template-columns:1fr 1fr;gap:6px;width:110px;height:64px;padding:5px;border-radius:6px;border:1px solid var(--line);background:var(--surface-2)">
      ${CORNERS.map(([v, label]) => html`<button type="button" title=${t(label)} aria-label=${t(label)} aria-pressed=${cur === v}
        style=${`border:0;border-radius:3px;cursor:pointer;background:${cur === v ? 'var(--accent)' : 'var(--surface-3)'}`}
        onClick=${() => saveSettings({ navigator_corner: v })}></button>`)}
    </div>
  </div>`;
}

/** The version, and where the releases are. */
function VersionRow() {
  const [open, setOpen] = useState(false);
  return html`<${UpdateRow} /><div class="setting-row full">
    <div class="row wrap">
      <span class="muted small grow">${t('This is openOMSI {version}', { version: S.boot?.version || '' })}</span>
      ${S.boot?.versions && html`<${Button} small icon="history" label="Choose a version…" onClick=${() => setOpen(true)} />`}
      <${Button} small kind="ghost" icon="external" onClick=${() => call('open_url', { url: REPO_URL }).catch((e) => toast(t(e.message), 'error'))}>github.com/openOmsi-project/openOMSI</${Button}>
    </div>
    ${open && html`<${VersionsDialog} onClose=${() => setOpen(false)} />`}
  </div>`;
}

/** Every setting back to how it came; the language stays. */
function ResetRow({ ask }) {
  const reset = async () => {
    const yes = await ask({
      title: 'Reset every setting?',
      text: 'Graphics, sound, controllers and game settings go back to how they came. The language, the drivers, the key bindings and the game folder stay.',
      yes: 'Reset',
      danger: true,
    });
    if (!yes) return;
    try {
      const defaults = await call('settings_defaults');
      // (the whole set replaced, not merged: keys the defaults lack go back to the game's own)
      await saveSettings({ ...defaults, language: S.settings.language }, true);
      toast(t('Every setting is back to how it came.'), 'ok');
    } catch (e) {
      toast(t(e.message), 'error');
    }
  };
  return html`<div class="setting-row full">
    <div><${Button} kind="danger" icon="refresh" label="Reset all settings..." onClick=${reset} /></div>
  </div>`;
}

/** Logs, settings and a description of the machine in one zip, for a bug report. */
function SupportRow() {
  const [busy, setBusy] = useState(false);
  const make = async () => {
    setBusy(true);
    try {
      const path = await call('support_package');
      toast(t('Support package saved: {path}', { path }), 'ok', 10000);
    } catch (e) {
      toast(t(e.message), 'error');
    }
    setBusy(false);
  };
  return html`<div class="setting-row full">
    <div><${Button} icon="file" label=${busy ? 'Writing the support package…' : 'Support package'} disabled=${busy} onClick=${make} /></div>
  </div>`;
}
