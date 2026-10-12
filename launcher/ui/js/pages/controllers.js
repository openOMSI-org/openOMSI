import { html, useState, useEffect, useRef } from '../../vendor/preact-htm.js';
import { S, useStore, toast } from '../store.js';
import { t } from '../i18n.js';
import { call } from '../api.js';
import { saveSettings, cached } from '../data.js';
import { Icon } from '../icons.js';
import { Button, Card, Search, Segmented, Select, Toggle, Notice, Empty, Spinner, Tag, Slider, useConfirm } from '../ui.js';
import { SettingRow, settingRow } from './settings.js';
import { actionText, padActions, PAD_ACTION_GROUPS } from './actions.js';
import { SetupWizard, FeedbackTest, useMoving, Meter } from './controller-setup.js';

export const AXES = ['X axis', 'Y axis', 'Z axis', 'X rotation', 'Y rotation', 'Z rotation', 'Slider 1', 'Slider 2'];
export const FUNCS = [
  [-1, 'Not used'],
  [0, 'Steering'],
  [1, 'Throttle'],
  [2, 'Brake'],
  [3, 'Clutch'],
  [4, 'Throttle and brake'],
  [5, 'Look left / right'],
  [6, 'Look up / down'],
];
const SHAPES = [
  [0, 'Linear'],
  [8, 'Progressive'],
  [4, 'Degressive'],
  [24, 'Bi-progressive'],
  [20, 'Bi-degressive'],
];
const CURVE = 4 | 8 | 0x10;
const DEAD_ZONE = { type: 'slider', key: 'ctrl_deadzone', label: 'Dead zone', min: 0, max: 0.3, step: 0.01, def: 0, format: (v) => `${Math.round(v * 100)} %`, store: (v) => Math.round(v * 100) / 100 };
const HATS = 128;
const PAD_WORDS = new Set(['Cross', 'Circle', 'Square', 'Triangle', 'D-pad up', 'D-pad down', 'D-pad left', 'D-pad right', 'Right stick click', 'View', 'Share']);
const DIRS = ['Hat {n} up', 'Hat {n} right', 'Hat {n} down', 'Hat {n} left'];

const norm = (s) => (s || '').toLowerCase().replace(/[^\p{L}\p{N}]/gu, '');
export function namesMatch(a, b) {
  const [x, y] = [norm(a), norm(b)];
  return !!x && (x === y || x.includes(y) || y.includes(x));
}
export const findConnected = (list, name) => list.find((c) => c.name === name) || list.find((c) => namesMatch(c.name, name));
const findDevice = (list, name) => list.find((d) => d.name === name) || list.find((d) => namesMatch(d.name, name));

export function buttonName(n) {
  if (n >= HATS) return t(DIRS[(n - HATS) % 4], { n: Math.floor((n - HATS) / 4) + 1 });
  return t('Button {n}', { n: n + 1 });
}

export const newDevice = (name) => ({ name, second: '0', axes: Array.from({ length: 8 }, () => [-1, false, 0]), buttons: [], ff_scale: null, ff_invert: null, latching: [] });
const offList = (s) => (s.ctrl_off || '').split('|').filter(Boolean);

let saveTimer = 0;
let saving$ = Promise.resolve();

function useControllers() {
  const [cfg, setCfg] = useState(null);
  const [error, setError] = useState('');
  const [live, setLive] = useState([]);
  const [saving, setSaving] = useState('');
  const paused = useRef(false);
  const listeners = useRef(new Set());
  const devicesRef = useRef(null);

  const load = () => {
    setError('');
    call('controllers_load').then(
      (v) => {
        devicesRef.current = v.devices;
        setCfg(v);
      },
      (e) => setError(e.message),
    );
  };

  useEffect(() => {
    saving$.then(load);
    let alive = true;
    let wait = 0;
    const loop = async () => {
      while (alive) {
        if (!paused.current) {
          try {
            const st = await call('controllers_state');
            if (!alive) break;
            setLive(st.connected || []);
            for (const [name, n] of st.pressed || []) listeners.current.forEach((f) => f(name, n));
            wait = 50;
          } catch {
            wait = 2000;
          }
        }
        await new Promise((r) => setTimeout(r, paused.current ? 200 : wait));
      }
    };
    loop();
    return () => {
      alive = false;
      clearTimeout(saveTimer);
      if (saveTimer) flush();
      call('controllers_release').catch(() => {});
    };
  }, []);

  function flush() {
    saveTimer = 0;
    setSaving('saving');
    const devices = devicesRef.current;
    saving$ = saving$.then(() =>
      call('controllers_save', { devices }).then(
        () => setSaving('saved'),
        (e) => {
          setSaving('');
          toast(t('Not saved: {e}', { e: e.message }), 'error');
        },
      ),
    );
  }

  function setDevices(devices) {
    devicesRef.current = devices;
    setCfg((c) => ({ ...c, devices }));
    clearTimeout(saveTimer);
    saveTimer = setTimeout(flush, 400);
  }

  return { cfg, error, live, saving, load, setDevices, devicesRef, paused, listeners };
}

export function Controllers() {
  const s = useStore();
  const pads = useControllers();
  const [sel, setSel] = useState(null);
  const [wizard, setWizard] = useState(null);
  const [lit, setLit] = useState(null);
  const capture = useRef(null);
  const [capturing, setCapturing] = useState(null);
  const [ask, confirmNode] = useConfirm();
  const [kb, setKb] = useState(null);

  useEffect(() => {
    cached('keys', 'keybindings_get').then((v) => v && !v.error && setKb(v));
  }, []);

  const devices = pads.cfg?.devices || [];
  const offs = offList(s.settings || {});
  const list = [
    ...devices.map((d) => ({ name: d.name, cfg: d, live: findConnected(pads.live, d.name) })),
    ...pads.live.filter((c) => !devices.some((d) => namesMatch(d.name, c.name))).map((c) => ({ name: c.name, cfg: null, live: c })),
  ];
  const entry = list.find((e) => e.name === sel) || list[0];

  useEffect(() => {
    const press = (name, n) => {
      const c = capture.current;
      if (c && namesMatch(c.device, name)) {
        capture.current = null;
        setCapturing(null);
        bind(c.device, n, c.action);
        return;
      }
      if (c || wizard) return;
      const hit = list.find((e) => e.cfg && namesMatch(e.name, name)) || list.find((e) => namesMatch(e.name, name));
      if (hit) setSel(hit.name);
      setLit({ name, n, at: Date.now() });
    };
    pads.listeners.current.add(press);
    return () => pads.listeners.current.delete(press);
  });

  useEffect(() => {
    if (!capturing) return;
    const k = (e) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        capture.current = null;
        setCapturing(null);
      }
    };
    window.addEventListener('keydown', k, true);
    return () => window.removeEventListener('keydown', k, true);
  }, [capturing]);

  function pick(name) {
    capture.current = null;
    setCapturing(null);
    setSel(name);
  }

  function change(name, f) {
    const next = (pads.devicesRef.current || []).map((d) => {
      if (d.name !== name) return d;
      const c = JSON.parse(JSON.stringify(d));
      f(c);
      return c;
    });
    pads.setDevices(next);
  }

  function bind(name, n, action) {
    const d = findDevice(pads.devicesRef.current || [], name);
    if (!d) return;
    const before = d.buttons[n]?.[0] || '';
    change(d.name, (c) => {
      while (c.buttons.length <= n) c.buttons.push(['', '0']);
      c.buttons[n] = [action, c.buttons[n][1] || '0'];
      if (!action) c.latching = c.latching.filter((b) => b !== n);
    });
    if (action && before && before.toLowerCase() !== action.toLowerCase()) toast(t('{button} now does {action} instead of {old}', { button: buttonName(n), action: actionText(action), old: actionText(before) }), 'info', 6000);
  }

  function startCapture(device, action) {
    capture.current = { device, action };
    setCapturing({ device, action });
  }

  function add(name) {
    if (!findDevice(devices, name)) {
      const d = newDevice(name);
      const live = findConnected(pads.live, name);
      if (live && !live.gamepad && live.axes.length > 1 && live.axes.some(([k]) => k === 0)) d.axes[0] = [0, false, 0];
      pads.setDevices([...(pads.devicesRef.current || []), d]);
    }
    pick(name);
  }

  async function remove(name) {
    const ok = await ask({ title: 'Remove this device', text: 'Its axes and buttons are forgotten. While it is connected it can be set up again.', yes: 'Remove', danger: true });
    if (!ok) return;
    pads.setDevices((pads.devicesRef.current || []).filter((d) => d.name !== name));
    if (!findConnected(pads.live, name) && offs.some((o) => namesMatch(o, name))) setOn(name, true);
  }

  function setOn(name, on) {
    const rest = offs.filter((o) => !namesMatch(o, name));
    saveSettings({ ctrl_off: (on ? rest : [...rest, name]).join('|') });
  }

  if (pads.error) {
    return html`<${Notice} kind="danger"><div class="stack" style="gap:8px"><div>${pads.error}</div><div><${Button} small icon="refresh" label="Try again" onClick=${pads.load} /></div></div></${Notice}>`;
  }
  if (!pads.cfg) return html`<div class="row muted"><${Spinner} /></div>`;

  const actions = padActions(kb, devices);
  const wizardDevice = wizard && (findDevice(devices, wizard) || newDevice(wizard));

  return html`<div class="pads">
    <${Card} class="pads-list" title="Devices" actions=${pads.saving && html`<span class="muted small row" style="gap:6px">${pads.saving === 'saving' ? html`<${Spinner} />` : html`<${Icon} name="check" size=${16} />${t('Saved')}`}</span>`}>
      ${list.length
        ? html`<div class="list">${list.map((e) => html`<${DeviceRow} e=${e} on=${e === entry} off=${offs.some((o) => namesMatch(o, e.name))} onClick=${() => pick(e.name)} />`)}</div>`
        : html`<${Empty} icon="gamepad" text="No game controller is connected, and none is set up. Connect a wheel, pedals or a joystick; a gamepad works without setting up (left stick steers, the triggers are the pedals)." />`}
      ${pads.cfg.error && html`<div style="margin-top:12px"><${Notice} kind="warn">${pads.cfg.error}</${Notice}></div>`}
    </${Card}>
    <div class="pads-detail stack">
      ${entry &&
      (entry.cfg
        ? html`<${DeviceDetail}
            e=${entry}
            s=${s.settings || {}}
            off=${offs.some((o) => namesMatch(o, entry.name))}
            actions=${actions}
            lit=${lit && namesMatch(lit.name, entry.name) ? lit : null}
            capturing=${capturing && namesMatch(capturing.device, entry.name) ? capturing.action : null}
            padButtons=${pads.cfg.pad_buttons}
            change=${(f) => change(entry.cfg.name, f)}
            bind=${(n, a) => bind(entry.cfg.name, n, a)}
            startCapture=${(a) => startCapture(entry.cfg.name, a)}
            stopCapture=${() => ((capture.current = null), setCapturing(null))}
            setOn=${(on) => setOn(entry.cfg.name, on)}
            setUp=${() => ((capture.current = null), setCapturing(null), setWizard(entry.cfg.name))}
            remove=${() => remove(entry.cfg.name)}
            paused=${pads.paused}
          />`
        : entry.live.gamepad
          ? html`<${PadDetail} e=${entry} padButtons=${pads.cfg.pad_buttons} customise=${() => add(entry.name)} />`
          : html`<${NewDevice} e=${entry} off=${offs.some((o) => namesMatch(o, entry.name))} setOn=${(on) => setOn(entry.name, on)} add=${() => add(entry.name)} setUp=${() => ((capture.current = null), setCapturing(null), setWizard(entry.name))} />`)}
    </div>
    ${wizardDevice &&
    html`<${SetupWizard}
      device=${wizardDevice}
      live=${findConnected(pads.live, wizardDevice.name)}
      paused=${pads.paused}
      onClose=${() => setWizard(null)}
      onDone=${(axes, ffInvert) => {
        if (!findDevice(pads.devicesRef.current || [], wizardDevice.name)) pads.setDevices([...(pads.devicesRef.current || []), newDevice(wizardDevice.name)]);
        pick(wizardDevice.name);
        change(wizardDevice.name, (c) => {
          c.axes = axes.map((a, k) => (a ? [a[0], a[1], c.axes[k][2] & ~1] : [-1, false, c.axes[k][2] & ~1]));
          if (ffInvert != null) c.ff_invert = ffInvert;
        });
        setWizard(null);
      }}
    />`}
    ${confirmNode}
  </div>`;
}

function kindOf(e) {
  if (e.live?.gamepad) return 'pad';
  if (e.live?.buttons_only || (e.cfg && !e.live && e.cfg.axes.every((a) => a[0] < 0) && e.cfg.buttons.some((b) => b[0]))) return 'buttons';
  return 'wheel';
}

function DeviceRow({ e, on, off, onClick }) {
  const kind = kindOf(e);
  const [dot, text] = off ? ['', 'Switched off'] : e.cfg ? (e.live ? ['ok', 'Connected'] : ['', 'Not connected']) : kind === 'pad' ? ['ok', 'Ready to play'] : ['accent', 'Not set up yet'];
  return html`<button class=${'list-item device' + (on ? ' on' : '') + (e.live ? '' : ' away')} aria-pressed=${on} title=${e.name} onClick=${onClick}>
    <span class="device-icon"><${Icon} name=${kind === 'pad' ? 'gamepad' : kind === 'buttons' ? 'keyboard' : 'sliders'} /></span>
    <span class="grow" style="min-width:0">
      <span class="title ellipsis">${e.name}</span>
      <span class="sub row" style="gap:6px"><span class=${'dot ' + dot}></span>${t(text)}</span>
    </span>
  </button>`;
}

function NewDevice({ e, off, setOn, add, setUp }) {
  const buttonsOnly = e.live?.buttons_only || !e.live?.axes?.length;
  const wheel = !buttonsOnly && e.live.axes.length > 1;
  return html`<${Card}>
    <div class="stack" style="gap:16px;align-items:flex-start">
      <div class="device-head" style="width:100%">
        <div style="min-width:0">
          <h2 class="device-name">${e.name}</h2>
          <p class="muted" style="margin-top:6px;max-width:62ch">
            ${t(buttonsOnly ? 'Not set up yet: give its buttons what they do.' : 'Not set up yet: the assistant finds the wheel and the pedals in a few steps. Until then its X axis steers.')}
          </p>
        </div>
        <${Toggle} checked=${!off} label="Use this device" onChange=${setOn} />
      </div>
      <${LiveAxes} live=${e.live} />
      <div class="row wrap">
        ${!buttonsOnly && html`<${Button} kind=${wheel ? 'primary' : ''} icon="play" label="Set up step by step" onClick=${setUp} />`}
        <${Button} kind=${wheel ? '' : 'primary'} icon="plus" label="Set up its buttons only" onClick=${add} />
      </div>
    </div>
  </${Card}>`;
}

function LiveAxes({ live }) {
  const moving = useMoving(live?.axes);
  if (!live?.axes?.length) return null;
  return html`<div class="live-axes">
    ${live.axes.map(([k, v]) => html`<div class=${'live-axis' + (moving === k ? ' moving' : '')}><span>${t(AXES[k])}</span><${Meter} value=${v} /></div>`)}
  </div>`;
}

function DeviceDetail({ e, s, off, actions, lit, capturing, padButtons, change, bind, startCapture, stopCapture, setOn, setUp, remove, paused }) {
  const d = e.cfg;
  const kind = kindOf(e);
  const ff = e.live?.ff || d.ff_scale != null || d.ff_invert != null;
  const steers = d.axes.some((a) => a[0] === 0);
  const gears = d.buttons.some(([a]) => /^kw_s_.+_fest$/.test(a));
  return html`
    <${Card}>
      <div class="device-head">
        <div style="min-width:0">
          <h2 class="device-name">${d.name}</h2>
          <div class="row wrap" style="gap:8px;margin-top:6px">
            ${off ? html`<${Tag}>${t('Switched off')}</${Tag}>` : e.live ? html`<${Tag} kind="ok">${t('Connected')}</${Tag}>` : html`<${Tag}>${t('Not connected')}</${Tag}>`}
          </div>
        </div>
        <div class="row wrap" style="gap:8px;justify-content:flex-end">
          <${Toggle} checked=${!off} label="Use this device" onChange=${setOn} />
          ${kind !== 'buttons' && html`<${Button} icon="play" label="Set up step by step" disabled=${!e.live} title=${e.live ? undefined : 'Not connected'} onClick=${setUp} />`}
          <${Button} kind="ghost danger" icon="trash" title="Remove this device" onClick=${remove} />
        </div>
      </div>
      ${!e.live && html`<div style="margin-top:14px"><${Notice}>${t('Not connected: plug it in to see it answer. What it does is kept.')}</${Notice}></div>`}
      ${gears && !s.momentary_gears && html`<div style="margin-top:14px"><${Notice} kind="warn">${t('H-pattern gears are assigned. Enable return to neutral under Settings → Driving → Game controllers if your shifter has no neutral button.')}</${Notice}></div>`}
    </${Card}>
    ${kind === 'pad' && html`<${GamepadCard} e=${e} padButtons=${padButtons} />`}
    ${kind !== 'buttons' && html`<${AxesCard} d=${d} live=${e.live} change=${change} gamepad=${kind === 'pad'} setUp=${setUp} />`}
    ${steers && kind !== 'pad' && html`<${WheelCard} d=${d} live=${e.live} ff=${ff} change=${change} paused=${paused} />`}
    <${ButtonsCard} d=${d} live=${e.live} actions=${actions} lit=${lit} capturing=${capturing} change=${change} bind=${bind} startCapture=${startCapture} stopCapture=${stopCapture} />
  `;
}

function gameValue(v, a) {
  const [f, rev, flags] = a;
  let x = rev ? -v : v;
  if (f === 5 || f === 6) return { centre: true, x };
  if (f === 0) return { centre: true, x: shape((x + 1) / 2, flags) * 2 - 1 };
  const p = shape((x + 1) / 2, flags);
  if (f === 4) return { centre: true, x: p * 2 - 1 };
  return { centre: false, x: p };
}

function shape(v, flags) {
  if (flags & 2) v = v * 1.25 - 0.125;
  const bi = flags & 0x10;
  if (flags & 4) v = bi ? Math.sin((v - 0.5) * Math.PI) * 0.5 + 0.5 : Math.sin((v * Math.PI) / 2);
  else if (flags & 8) {
    if (bi) {
      const d = (v - 0.5) * 2;
      v = 0.5 + Math.sign(d) * d * d * 0.5;
    } else v = v * v;
  }
  return Math.min(1, Math.max(0, v));
}

function AxesCard({ d, live, change, gamepad, setUp }) {
  const liveOf = (k) => live?.axes?.find((x) => x[0] === k)?.[1];
  const slots = [...Array(8).keys()].filter((k) => d.axes[k][0] >= 0 || liveOf(k) != null);
  const moving = useMoving(live?.axes);
  const has = (f) => d.axes.findIndex((a) => a[0] === f || (a[0] === 4 && (f === 1 || f === 2)));
  const main = [[0, 'Steering'], [1, 'Throttle'], [2, 'Brake'], [3, 'Clutch']];
  return html`<${Card} title="Axes and pedals" hint=${gamepad ? 'With no axis given here, the left stick steers and the triggers are the pedals.' : 'Move a control: its line lights up.'}>
    <div class="stack" style="gap:14px">
      <div class="row wrap" style="gap:8px">
        ${main.map(([f, label]) => {
          const k = has(f);
          return html`<span class=${'axis-chip' + (k >= 0 ? ' set' : '')}>${t(label)}<b>${k >= 0 ? t(AXES[k]) : t('Not used')}</b></span>`;
        })}
      </div>
      ${!gamepad && has(0) < 0 && html`<${Notice} kind="warn"><div class="row wrap" style="justify-content:space-between;gap:10px"><span class="grow">${t('No axis steers yet.')}</span><${Button} small kind="primary" icon="play" label="Set up step by step" disabled=${!live} onClick=${setUp} /></div></${Notice}>`}
      <div class="axis-table">
        <div class="axis-row axis-head"><span></span><span></span><span></span><span>${t('Reversed')}</span><span>${t('Response curve')}</span></div>
        ${slots.map((k) => {
          const a = d.axes[k];
          const raw = liveOf(k);
          const g = raw != null && a[0] >= 0 ? gameValue(raw, a) : null;
          return html`<div class=${'axis-row' + (moving === k ? ' moving' : '') + (a[0] < 0 ? ' unused' : '')}>
            <div class="axis-name">${t(AXES[k])}</div>
            <div class="axis-meter">${raw != null ? html`<${Meter} value=${g ? g.x : raw} centre=${g ? g.centre : true} fill=${!!g} />` : html`<${Meter} value=${0} centre=${false} />`}</div>
            <${Select} value=${a[0]} options=${FUNCS} onChange=${(v) => change((c) => (c.axes[k][0] = Number(v)))} />
            <div class="axis-flip">${a[0] >= 0 && html`<${Toggle} checked=${a[1]} onChange=${(v) => change((c) => (c.axes[k][1] = v))} />`}</div>
            <div class="axis-curve">${a[0] >= 0 && a[0] < 5 && html`<${Select} value=${a[2] & CURVE} options=${a[0] === 0 || a[0] === 4 ? SHAPES.filter(([v]) => v === 0 || v & 0x10) : SHAPES} onChange=${(v) => change((c) => (c.axes[k][2] = (c.axes[k][2] & ~CURVE) | Number(v)))} />`}</div>
          </div>`;
        })}
      </div>
      ${!gamepad && html`<div class="stack" style="gap:2px"><${SettingRow} row=${DEAD_ZONE} /><${SettingRow} row=${settingRow('pedal_throttle')} /><${SettingRow} row=${settingRow('pedal_brake')} /></div>`}
    </div>
  </${Card}>`;
}

function ButtonsCard({ d, live, actions, lit, capturing, change, bind, startCapture, stopCapture }) {
  const [view, setView] = useState('actions');
  const [q, setQ] = useState('');
  const fresh = lit && Date.now() - lit.at < 4000 ? lit.n : null;
  const [, tick] = useState(0);
  useEffect(() => {
    if (fresh == null) return;
    const id = setTimeout(() => tick((x) => x + 1), 4100);
    return () => clearTimeout(id);
  }, [lit]);
  const by = new Map();
  d.buttons.forEach(([a], n) => {
    if (!a) return;
    const k = a.toLowerCase();
    if (!by.has(k)) by.set(k, { action: a, buttons: [] });
    by.get(k).buttons.push(n);
  });
  return html`<${Card}
    title="Buttons"
    hint=${capturing ? '' : 'Press a button on the device: it lights up here.'}
    actions=${html`<${Segmented} value=${view} onChange=${setView} options=${[['actions', 'By action'], ['buttons', 'By button']]} />`}
  >
    ${capturing &&
    html`<div class="capture-bar">
      <span class="pulse"></span>
      <div class="grow"><b>${actionText(capturing)}</b><div class="muted small">${t('Press a button on the device...')}</div></div>
      <${Button} small label="Cancel" onClick=${stopCapture} />
    </div>`}
    ${fresh != null &&
    !capturing &&
    html`<div class="lit-bar"><span class="kbd on">${buttonName(fresh)}</span><span>${d.buttons[fresh]?.[0] ? actionText(d.buttons[fresh][0]) : t('Nothing')}</span></div>`}
    ${view === 'actions'
      ? html`<${ByAction} actions=${actions} by=${by} q=${q} setQ=${setQ} fresh=${fresh} capturing=${capturing} latching=${d.latching} startCapture=${startCapture} bind=${bind} live=${live} />`
      : html`<${ByButton} d=${d} live=${live} actions=${actions} fresh=${fresh} change=${change} bind=${bind} />`}
  </${Card}>`;
}

function ByAction({ actions, by, q, setQ, fresh, capturing, latching, startCapture, bind, live }) {
  const query = q.trim().toLowerCase();
  const grouped = new Set(PAD_ACTION_GROUPS.flatMap(([, a]) => a.map((x) => x.toLowerCase())));
  const others = [...by.values()].map((x) => x.action).filter((a) => !grouped.has(a.toLowerCase()));
  const row = (a) => {
    const bound = by.get(a.toLowerCase())?.buttons || [];
    const waiting = capturing && capturing.toLowerCase() === a.toLowerCase();
    const can = !!live || waiting;
    const go = () => can && !waiting && startCapture(a);
    return html`<div class=${'bind-row' + (waiting ? ' waiting' : '') + (bound.includes(fresh) ? ' lit' : '') + (bound.length ? ' bound' : '') + (can ? ' can' : '')} role="button" tabindex=${can ? 0 : -1} onClick=${go} onKeyDown=${(e) => (e.key === 'Enter' || e.key === ' ') && e.target === e.currentTarget && (e.preventDefault(), go())}>
      <div class="grow ellipsis">${actionText(a)}</div>
      ${waiting
        ? html`<span class="muted small">${t('Press a button on the device...')}</span>`
        : html`<div class="row chips">
            ${bound.map(
              (n) => html`<span class=${'kbd chip' + (n === fresh ? ' on' : '')}>${buttonName(n)}${latching.includes(n) && html`<small>${t('Latching')}</small>`}<button title=${t('Remove')} onClick=${(e) => (e.stopPropagation(), bind(n, ''))}><${Icon} name="x" size=${14} /></button></span>`,
            )}
            ${can && html`<span class="assign">${bound.length ? html`<${Icon} name="plus" size=${16} />` : t('Assign')}</span>`}
          </div>`}
    </div>`;
  };
  const find = (a) => actionText(a).toLowerCase().includes(query) || a.toLowerCase().includes(query);
  const all = [...new Set([...actions, ...others])];
  if (query) {
    const hits = all.filter(find);
    const typed = q.trim();
    const custom = !all.some((a) => a.toLowerCase() === typed.toLowerCase()) && /^[A-Za-z0-9_]{2,}$/.test(typed);
    return html`<div class="stack" style="gap:14px">
      <${Search} value=${q} onInput=${setQ} placeholder="Search actions..." />
      <div class="binds">${hits.map(row)}${custom && row(typed)}</div>
      ${!hits.length && !custom && html`<${Empty} icon="search" title="No matching actions." />`}
    </div>`;
  }
  return html`<div class="stack" style="gap:14px">
    <${Search} value=${q} onInput=${setQ} placeholder="Search actions..." />
    <div class="bind-groups">
      ${PAD_ACTION_GROUPS.map(([title, list]) => html`<div><div class="section-title">${t(title)}</div><div class="binds">${list.map(row)}</div></div>`)}
      ${others.length > 0 && html`<div><div class="section-title">${t('Other')}</div><div class="binds">${others.map(row)}</div></div>`}
    </div>
  </div>`;
}

function ByButton({ d, live, actions, fresh, change, bind }) {
  const physical = live?.buttons || 0;
  const last = d.buttons.reduce((m, [a], i) => (a && i < HATS ? i + 1 : m), 0);
  const count = Math.min(HATS, Math.max(physical, last, fresh != null && fresh < HATS ? fresh + 1 : 0, live ? 0 : last));
  const hats = [...new Set([...d.buttons.map(([a], i) => (a && i >= HATS ? i : -1)), fresh != null && fresh >= HATS ? fresh : -1])].filter((i) => i >= 0).sort((a, b) => a - b);
  const numbers = [...Array(count).keys(), ...hats];
  const options = [['', t('Nothing')], ...actions.map((a) => [a, actionText(a)]).sort((x, y) => x[1].localeCompare(y[1]))];
  const ref = useRef();
  useEffect(() => {
    if (fresh != null) ref.current?.querySelector(`[data-n="${fresh}"]`)?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }, [fresh]);
  if (!numbers.length) return html`<${Empty} icon="gamepad" text="Press a button on the device: it lights up here." />`;
  return html`<div class="button-grid" ref=${ref}>
    ${numbers.map((n) => {
      const a = d.buttons[n]?.[0] || '';
      const latched = d.latching.includes(n);
      return html`<div class=${'button-row' + (n === fresh ? ' lit' : '')} data-n=${n}>
        <span class=${'kbd' + (n === fresh ? ' on' : '')}>${buttonName(n)}</span>
        <${Select} value=${a} options=${options.some(([v]) => v === a) ? options : [...options, [a, actionText(a)]]} raw onChange=${(v) => bind(n, v)} />
        <${Toggle} checked=${latched} disabled=${!a} label="Latching" onChange=${(v) => change((c) => (c.latching = v ? [...c.latching.filter((x) => x !== n), n].sort((x, y) => x - y) : c.latching.filter((x) => x !== n)))} />
      </div>`;
    })}
  </div>`;
}

function WheelCard({ d, live, ff, change, paused }) {
  const [test, setTest] = useState(false);
  const [steer, force] = d.ff_scale || [1, 1];
  const axis = d.axes.findIndex((a) => a[0] === 0);
  return html`<${Card} title="Steering wheel">
    <div class="stack" style="gap:6px">
      ${ff &&
      html`<div class="setting-row full"><${Slider} label="Steering force" value=${steer} min=${0} max=${2} step=${0.05} format=${(v) => `${Math.round(v * 100)} %`} onChange=${(v) => change((c) => (c.ff_scale = [v, force]))} /></div>
        <div class="setting-row full"><${Slider} label="Vibration" value=${force} min=${0} max=${2} step=${0.05} format=${(v) => `${Math.round(v * 100)} %`} onChange=${(v) => change((c) => (c.ff_scale = [steer, v]))} /></div>
        <div class="setting-row">
          <div class="name">${t('Invert force feedback')}</div>
          <div class="row" style="gap:10px;justify-content:flex-end">
            <${Button} small icon="play" label="Test the direction" disabled=${!live || axis < 0} onClick=${() => setTest(true)} />
            <${Toggle} checked=${d.ff_invert ?? !!S.settings?.ff_invert} onChange=${(v) => change((c) => (c.ff_invert = v))} />
          </div>
        </div>`}
      <div class="section-title">${t('For every wheel')}</div>
      <${SettingRow} row=${settingRow('wheel_range')} />
      <${SettingRow} row=${settingRow('wheel_lock')} />
      ${ff &&
      html`<${SettingRow} row=${settingRow('ff_enabled')} />
        <${SettingRow} row=${settingRow('ff_road_vib')} />
        <${SettingRow} row=${settingRow('ff_engine_vib')} />
        <${SettingRow} row=${settingRow('ff_fade')} />`}
    </div>
    ${test &&
    html`<${FeedbackTest}
      device=${d}
      axis=${axis}
      live=${live}
      paused=${paused}
      invert=${d.ff_invert ?? !!S.settings?.ff_invert}
      onClose=${() => setTest(false)}
      onDone=${(v) => {
        change((c) => (c.ff_invert = v));
        setTest(false);
      }}
    />`}
  </${Card}>`;
}

function padKind(e) {
  const set = S.settings?.pad_type;
  return set && set !== 'auto' ? set : e.live?.pad || 'xbox';
}

function GamepadCard({ e, padButtons }) {
  const kind = padKind(e);
  const custom = e.cfg?.buttons.some(([a]) => a);
  return html`<${Card} title="Gamepad">
    <div class="stack" style="gap:6px">
      <${SettingRow} row=${settingRow('pad_type')} />
      <${SettingRow} row=${settingRow('pad_buttons')} />
      ${S.settings?.pad_buttons !== false &&
      !custom &&
      html`<div class="pad-legend">
        ${(padButtons || []).map((b) => html`<div><span class="kbd">${PAD_WORDS.has(b[kind]) ? t(b[kind]) : b[kind]}</span><span>${actionText(b.action)}</span></div>`)}
      </div>`}
      <div class="section-title">${t('Sticks')}</div>
      <${SettingRow} row=${settingRow('pad_steer_speed')} />
      <${SettingRow} row=${settingRow('pad_steer_smooth')} />
      <${SettingRow} row=${settingRow('pad_deadzone')} />
      <${SettingRow} row=${settingRow('pad_steer_linear')} />
    </div>
  </${Card}>`;
}

function PadDetail({ e, padButtons, customise }) {
  return html`
    <${Card}>
      <div class="device-head">
        <div style="min-width:0">
          <h2 class="device-name">${e.name}</h2>
          <p class="muted" style="margin-top:6px;max-width:62ch">${t('Ready to play: the left stick steers, the right trigger accelerates and the left one brakes.')}</p>
        </div>
        <${Button} icon="sliders" label="Change its buttons" onClick=${customise} />
      </div>
      <div style="margin-top:14px"><${LiveAxes} live=${e.live} /></div>
    </${Card}>
    <${GamepadCard} e=${e} padButtons=${padButtons} />
  `;
}
