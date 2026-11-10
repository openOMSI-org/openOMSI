// Settings controls keep their existing contracts; settings-navigation.js arranges them
// into short topics. Every change goes to settings.cfg at once.

import { html, useState, useEffect, useRef } from '../../vendor/preact-htm.js';
import { S, useStore, set, toast, remembered, remember } from '../store.js';
import { t } from '../i18n.js';
import { call, onPhone, inTauri } from '../api.js';
import { Button, Select, Input, Toggle, Slider } from '../ui.js';
import { saveSettings, applyTheme, fitWindow, launcherSize, LAUNCHER_SIZES, launcherLayout, LAUNCHER_LAYOUTS } from '../data.js';
import { UpdateRow, checkUpdates } from '../updates.js';
import { SETTINGS_TABS, GRAPHICS_PROFILE_KEYS } from './settings-schema.js';
import { VersionsDialog } from './versions.js';
import { RadioStations } from './radio-stations.js';
import { SettingsNavigation } from './settings-navigation.js';
import { OptimiseBlock } from './optimise.js';

const REPO_URL = 'https://github.com/openOmsi-project/openOMSI';
const PRIVACY_URL = 'https://openomsi.org/privacy/';

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

export function SettingsPage() {
  return html`<${SettingsNavigation} Row=${Row} />`;
}

export function settingRow(key) {
  for (const tab of SETTINGS_TABS) for (const col of tab.cols) for (const row of col.rows) if (row.key === key) return row;
  return null;
}

export function SettingRow({ row, ask }) {
  return row ? html`<${Row} row=${row} ask=${ask} />` : null;
}

function Row({ row, ask, compact = false }) {
  switch (row.type) {
    case 'section':
      return html`<div class="section-title">${t(row.title)}</div>`;
    case 'note':
      return html`<p class="muted small" style="margin:8px 0">${t(row.text)}</p>`;
    case 'select':
      return html`<${SelectRow} row=${row} compact=${compact} />`;
    case 'toggle':
      return html`<${ToggleRow} row=${row} compact=${compact} />`;
    case 'slider':
      return html`<${SliderRow} row=${row} compact=${compact} />`;
    case 'button':
      return html`<${ButtonRow} row=${row} />`;
    case 'custom':
      return CUSTOM[row.id]?.({ ask, compact }) || null;
  }
  return null;
}

function Name({ label, help, warn }) {
  return html`<div class="name">${t(label)}${help && html`<span>${t(help)}</span>`}${warn && html`<span class="warn-fg">${t(warn)}</span>`}</div>`;
}

function SelectRow({ row, compact }) {
  const s = S.settings;
  const options = typeof row.options === 'function' ? row.options(s, S.boot) : row.options;
  const cur = s[row.key];
  // the choice the value means ("4" for 4); a value written by hand shows as itself
  const match = options.find(([v]) => same(cur, v) || String(cur) === String(v));
  const value = match ? match[0] : row.unknown ?? (cur == null ? '' : typeof cur === 'boolean' ? (cur ? '1' : '0') : String(cur));
  return html`<div class="setting-row">
    <${Name} label=${row.label} help=${!compact && (typeof row.help === 'function' ? row.help(s) : row.help)} warn=${row.heavy?.(value, s) && 'Heavy: can cost many frames per second on most graphics cards'} />
    <${Select} value=${value} options=${options} raw=${row.raw} always=${!match && row.unknown != null} onChange=${async (v) => (await saveSettings({ [row.key]: asStored(cur, v) })) && row.recheckUpdates && checkUpdates()} />
  </div>`;
}

function ToggleRow({ row, compact }) {
  return html`<div class="setting-row">
    <${Name} label=${row.label} help=${!compact && (typeof row.help === 'function' ? row.help(S.settings) : row.help)} />
    <${Toggle} checked=${!!S.settings[row.key]} onChange=${(v) => saveSettings({ [row.key]: v })} />
  </div>`;
}

/** A slider shows its value at once and saves it a moment after the hand stops (a save per
 * step would write settings.cfg dozens of times a drag). */
function SliderRow({ row, compact }) {
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
    <${Slider} label=${row.label} help=${!compact && (typeof row.help === 'function' ? row.help(S.settings) : row.help)} value=${value} min=${row.min} max=${row.max} step=${row.step} format=${row.format} onChange=${onChange} />
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
  nvidia_dlss_install: () => html`<${NvidiaDlssInstall} />`,
  profiles: () => html`<${ProfilesBlock} />`,
  theme: () => html`<${ThemeRow} />`,
  launcher_layout: () => !onPhone && html`<${LauncherLayoutRow} />`,
  launcher_size: ({ compact }) => inTauri && html`<${LauncherSizeRow} compact=${compact} />`,
  corner: () => html`<${CornerRow} />`,
  version: () => html`<${VersionRow} />`,
  reset: ({ ask }) => html`<${ResetRow} ask=${ask} />`,
  support: () => html`<${SupportRow} />`,
  radio: () => html`<${RadioStations} />`,
};


// The game executable is AGPLv3. NVIDIA's runtime is never part of the
// openOMSI release: the player may separately obtain official vendor files.
const NVIDIA_STREAMLINE_URL = 'https://github.com/NVIDIA-RTX/Streamline/releases/tag/v2.14.1';
const NVIDIA_STREAMLINE_LICENSE = 'https://github.com/NVIDIA-RTX/Streamline/blob/v2.14.1/license.txt';
const NVIDIA_NGX_LICENSE = 'https://github.com/NVIDIA-RTX/Streamline/blob/v2.14.1/external/ngx-sdk/license.txt';

function NvidiaDlssInstall() {
  const [status, setStatus] = useState(null);
  const [accepted, setAccepted] = useState(false);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState('');
  useEffect(() => {
    let active = true;
    call('nvidia_dlss_status')
      .then((result) => active && setStatus(result))
      .catch((error) => active && setProblem(error.message));
    return () => { active = false; };
  }, []);
  const open = (url) => call('open_url', { url }).catch((e) => toast(e.message, 'error'));
  const installed = !!status?.installed;
  // (nothing to offer without an RTX card or beside no game)
  if (status && !installed && (!status.rtx || !status.supported)) return null;
  const install = async () => {
    if (!accepted || busy || installed) return;
    setBusy(true);
    setProblem('');
    try {
      const result = await call('nvidia_dlss_install', { acceptedTerms: true });
      if (!result?.installed) throw new Error(t('The NVIDIA runtime was not found after installation'));
      toast(t('NVIDIA DLSS installed. Restarting the launcher to refresh available upscalers.'), 'success');
      window.location.reload();
    } catch (error) {
      setProblem(error.message || String(error));
      setBusy(false);
    }
  };
  return html`
    <div class="setting-row full">
      <div class="stack" style="gap:10px">
        <${Name} label="NVIDIA DLSS runtime (optional)"
          help="Not included with openOMSI. Installs NVIDIA's own DLLs only after your approval. Windows, NVIDIA RTX and a game restart required." />
        <p class="muted small" style="margin:0">${t(status == null ? 'Checking NVIDIA DLSS files…'
          : installed ? 'NVIDIA DLSS runtime is installed.'
          : 'DLSS files are not installed. The game works without them.')}</p>
        <div class="grid-2">
          <${Button} kind="ghost" icon="external" label="NVIDIA RTX/NGX license" onClick=${() => open(NVIDIA_NGX_LICENSE)} />
          <${Button} kind="ghost" icon="external" label="Streamline MIT license" onClick=${() => open(NVIDIA_STREAMLINE_LICENSE)} />
        </div>
        <${Button} kind="ghost" icon="external" label="View official NVIDIA SDK 2.14.1" onClick=${() => open(NVIDIA_STREAMLINE_URL)} />
        ${!installed && html`
          <label class="small" style="display:flex;align-items:flex-start;gap:10px">
            <input type="checkbox" checked=${accepted} onChange=${(e) => setAccepted(e.currentTarget.checked)} />
            <span>${t("I have read and agree to NVIDIA's applicable Streamline and RTX/NGX license terms. The launcher will download the SDK directly from NVIDIA and install the four verified DLSS DLLs plus notices.")}</span>
          </label>
          <${Button} kind="primary" icon="download"
            label=${busy ? 'Downloading and verifying NVIDIA DLSS…' : 'Install NVIDIA DLSS'}
            disabled=${busy || !accepted || !status?.supported || !status?.rtx}
            onClick=${install} />
        `}
        ${problem && html`<p class="warn-fg small" role="alert">${problem}</p>`}
      </div>
    </div>`;
}

/** The quality presets, Low to Ultra: the one the settings match, else "Custom". A preset sets
 * the graphics mode too (Low is Vanilla, Ultra Enhanced with everything on). */
function PresetRow() {
  const [presets, setPresets] = useState([]);
  useEffect(() => {
    let gone = false;
    call('graphics_presets', {})
      .then((p) => !gone && setPresets(p))
      .catch(() => !gone && setPresets([]));
    return () => (gone = true);
  }, []);
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

/** Save, load, share and delete the graphics settings as named profiles. */
function ProfilesBlock() {
  const [names, setNames] = useState(null);
  const [sel, setSel] = useState('');
  const [name, setName] = useState('');
  const [msg, setMsg] = useState('');
  const [busy, setBusy] = useState(false);
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
  const transfer = async (importing) => {
    if (busy) return;
    setBusy(true);
    setMsg('');
    try {
      const result = await call(importing ? 'graphics_profile_import' : 'graphics_profile_export', importing ? {} : { name: sel });
      if (result == null) return;
      if (importing) {
        setName(result);
        await reload(result);
        setMsg(t('Imported "{name}". Load it to apply these settings.', { name: result }));
      } else {
        setMsg(t('Exported "{name}".', { name: sel }));
      }
    } catch (e) {
      setMsg(e.message);
    } finally {
      setBusy(false);
    }
  };
  return html`
    <div class="settings-profiles">
      <section class="settings-profile-saved" aria-labelledby="saved-graphics-profiles">
        <h4 id="saved-graphics-profiles">${t('Saved profile')}</h4>
        <${Select} raw value=${sel} disabled=${busy || !list.length} options=${list.length ? list.map((n) => [n, n]) : [['', t('No saved profiles')]]} onChange=${(n) => {
          setSel(n);
          setName(n);
        }} />
        <div class="settings-profile-actions">
          <${Button} icon="download" label="Load" disabled=${busy || !list.length} onClick=${load} />
          <${Button} kind="danger" icon="trash" label="Delete" disabled=${busy || !list.length} onClick=${remove} />
        </div>
        ${!onPhone && html`<div class="settings-profile-actions">
          <${Button} icon="file" label="Import graphics profile" disabled=${busy} onClick=${() => transfer(true)} />
          <${Button} icon="download" label="Export graphics profile" disabled=${busy || !list.length} onClick=${() => transfer(false)} />
        </div>`}
      </section>
      <section class="settings-profile-save" aria-labelledby="save-graphics-profile">
        <h4 id="save-graphics-profile">${t('Save current graphics settings')}</h4>
        <${Input} id="graphics-profile-name" aria-label=${t('Profile name')} value=${name} placeholder="Profile name" onInput=${setName} maxlength="40" disabled=${busy} />
        <div class="settings-profile-actions"><${Button} kind="primary" icon="save" label="Save current graphics as profile" disabled=${busy} onClick=${save} /></div>
      </section>
      <p class="muted small">${t('Profiles only store graphics settings; window size and fullscreen are kept.')}</p>
      ${msg && html`<p class="muted small" role="status">${msg}</p>`}
    </div>
  `;
}

/** The shared UI look; the game stores the resolved look so both UIs follow it. */
function ThemeRow() {
  const s = useStore();
  const [theme, setTheme] = useState(() => remembered('theme', 'dark'));
  useEffect(() => {
    const pick = remembered('theme', 'dark');
    if (pick !== theme) setTheme(pick);
  }, [s.settings?.ui_theme]);
  return html`<div class="setting-row">
    <${Name} label="Launcher theme" />
    <${Select} value=${theme} options=${[['dark', 'Dark'], ['light', 'Light theme'], ['system', 'System']]} onChange=${async (v) => {
      setTheme(v);
      remember('theme', v);
      try {
        await applyTheme();
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

function LauncherSizeRow({ compact }) {
  const [size, setSize] = useState(launcherSize);
  return html`<div class="setting-row">
    <${Name} label="Launcher size" help=${!compact && 'Grows with the window; this sets how big it is'} />
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
      <${Button} small kind="ghost" label="Privacy" onClick=${() => call('open_url', { url: PRIVACY_URL }).catch((e) => toast(t(e.message), 'error'))} />
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
