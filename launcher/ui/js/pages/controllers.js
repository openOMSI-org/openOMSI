import { html, useState, useEffect, useRef, useMemo } from '../../vendor/preact-htm.js';
import { S, useStore, toast } from '../store.js';
import { t } from '../i18n.js';
import { call } from '../api.js';
import { saveSettings, cached } from '../data.js';
import { Icon } from '../icons.js';
import { Button, Card, Search, Segmented, Select, Toggle, Notice, Empty, Spinner, Tag, Slider, useConfirm } from '../ui.js';
import { SettingRow, settingRow } from './settings.js';
import { actionText, actionNamesVersion, padActions, useActionNames, useKeyCatalog, PAD_ACTION_GROUPS } from './actions.js';
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

export const newDevice = (name) => ({ name, second: '0', axes: Array.from({ length: 8 }, () => [-1, false, 0]), buttons: [], ff_scale: null, ff_invert: null, ff_tune: null, latching: [], more: [] });
// a wheel's force feedback tuning: multipliers of the self-centring, the damping and the parking
// resistance, the minimum force (a share of full force) and the smoothing (seconds); none saved
// (null) is these
const FF_TUNE = { centring: 1, damping: 1, parking: 1, min_force: 0, smoothing: 0 };
export const tuneOf = (c) => Object.fromEntries(Object.entries(FF_TUNE).map(([k, v]) => [k, Number.isFinite(c?.ff_tune?.[k]) ? c.ff_tune[k] : v]));
/** Changes some of the tuning; all back at the defaults saves none, so the device stays as it was. */
export function setTune(c, patch) {
  const next = { ...(c.ff_tune || {}), ...tuneOf(c), ...patch };
  c.ff_tune = Object.keys(FF_TUNE).every((k) => Math.abs(next[k] - FF_TUNE[k]) < 1e-6) ? null : next;
}
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
  const off = useRef([]);

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
            const st = await call('controllers_state', { off: off.current });
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
        (r) => {
          setSaving('saved');
          if (r?.path) setCfg((c) => c && { ...c, path: r.path });
        },
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

  return { cfg, error, live, saving, load, setDevices, devicesRef, paused, listeners, off };
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
  const actions = padActions(kb, devices);
  useActionNames(actions);
  const offs = offList(s.settings || {});
  pads.off.current = offs;
  const list = [
    ...devices.map((d) => ({ name: d.name, cfg: d, live: findConnected(pads.live, d.name) })),
    ...pads.live.filter((c) => !devices.some((d) => namesMatch(d.name, c.name))).map((c) => ({ name: c.name, cfg: null, live: c })),
  ];
  const entry = list.find((e) => e.name === sel) || list[0];

  // a wheel, pedals or a joystick nobody set up is set up by itself from its axes at rest
  // (the game does the same when it meets one): the player only changes what is wrong
  useEffect(() => {
    if (!pads.cfg || pads.cfg.error || wizard) return;
    const fresh = pads.live.filter((c) => c.auto && !findDevice(pads.devicesRef.current || [], c.name) && !offs.some((o) => namesMatch(o, c.name)));
    if (!fresh.length) return;
    pads.setDevices([...(pads.devicesRef.current || []), ...fresh.map((c) => c.auto.device)]);
    for (const c of fresh) toast(t('{name} was set up by itself ({what}). Check it below and change what is wrong.', { name: c.name, what: c.auto.what }));
    setSel(fresh[0].name);
  }, [pads.cfg, pads.live, wizard]);

  useEffect(() => {
    const press = (name, n) => {
      const c = capture.current;
      if (c && namesMatch(c.device, name)) {
        capture.current = null;
        setCapturing(null);
        assign(c.device, n, c.action);
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
      let more = c.more || [];
      let next = action;
      if (!next) {
        const i = more.findIndex(([b]) => b === n);
        if (i >= 0) [next, more] = [more[i][1], more.filter((_, k) => k !== i)];
      } else more = more.filter(([b, a]) => b !== n || a.toLowerCase() !== action.toLowerCase());
      c.more = more;
      c.buttons[n] = [next, c.buttons[n][1] || '0'];
      if (!next) c.latching = c.latching.filter((b) => b !== n);
    });
    if (action && before && before.toLowerCase() !== action.toLowerCase()) toast(t('{button} now does {action} instead of {old}', { button: buttonName(n), action: actionText(action), old: actionText(before) }), 'info', 6000);
  }

  function assign(name, n, action) {
    const d = findDevice(pads.devicesRef.current || [], name);
    if (!d) return;
    const has = [d.buttons[n]?.[0] || '', ...(d.more || []).filter(([b]) => b === n).map(([, a]) => a)].filter(Boolean);
    if (!has.length) return bind(name, n, action);
    if (has.some((a) => a.toLowerCase() === action.toLowerCase())) return;
    change(d.name, (c) => (c.more = [...(c.more || []), [n, action]]));
    toast(t('{button} now does {action} as well', { button: buttonName(n), action: actionText(action) }), 'info', 6000);
  }

  function unbind(name, n, action) {
    const d = findDevice(pads.devicesRef.current || [], name);
    if (!d) return;
    if ((d.buttons[n]?.[0] || '').toLowerCase() === action.toLowerCase()) return bind(name, n, '');
    change(d.name, (c) => (c.more = (c.more || []).filter(([b, a]) => b !== n || a.toLowerCase() !== action.toLowerCase())));
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

  const wizardDevice = wizard && (findDevice(devices, wizard) || newDevice(wizard));

  return html`<div class="pads">
    <${Card} class="pads-list" title="Devices" actions=${pads.saving && html`<span class="muted small row" style="gap:6px">${pads.saving === 'saving' ? html`<${Spinner} />` : html`<${Icon} name="check" size=${16} />${t('Saved')}`}</span>`}>
      ${list.length
        ? html`<div class="list">${list.map((e) => html`<${DeviceRow} e=${e} on=${e === entry} off=${offs.some((o) => namesMatch(o, e.name))} onClick=${() => pick(e.name)} />`)}</div>`
        : html`<${Empty} icon="gamepad" text="No game controller is connected, and none is set up. Connect a wheel, pedals or a joystick; a gamepad works without setting up (left stick steers, the triggers are the pedals)." />`}
      ${pads.cfg.error && html`<div style="margin-top:12px"><${Notice} kind="warn">${pads.cfg.error}</${Notice}></div>`}
      ${pads.cfg.path && html`<div class="cfg-path muted small" title=${pads.cfg.path}><${Icon} name="file" size=${14} /><span class="ellipsis">${pads.cfg.path}</span></div>`}
    </${Card}>
    <div class="pads-detail stack">
      ${entry &&
      (entry.cfg
        ? html`<${DeviceDetail}
            e=${entry}
            s=${s.settings || {}}
            off=${offs.some((o) => namesMatch(o, entry.name))}
            gears=${devices.some((x) => x.buttons.some(([a]) => /^kw_s_.+_fest$/.test(a)))}
            actions=${actions}
            lit=${lit && namesMatch(lit.name, entry.name) ? lit : null}
            capturing=${capturing && namesMatch(capturing.device, entry.name) ? capturing.action : null}
            padButtons=${pads.cfg.pad_buttons}
            change=${(f) => change(entry.cfg.name, f)}
            bind=${(n, a) => bind(entry.cfg.name, n, a)}
            assign=${(n, a) => assign(entry.cfg.name, n, a)}
            unbind=${(n, a) => unbind(entry.cfg.name, n, a)}
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

function DeviceDetail({ e, s, off, gears, actions, lit, capturing, padButtons, change, bind, assign, unbind, startCapture, stopCapture, setOn, setUp, remove, paused }) {
  const d = e.cfg;
  const kind = kindOf(e);
  const ff = e.live?.ff || d.ff_invert != null || d.ff_tune != null || (d.ff_scale || [1, 1]).some((v) => v !== 1);
  const steers = d.axes.some((a) => a[0] === 0);
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
      ${gears && !s.momentary_gears && html`<div style="margin-top:14px"><${Notice} kind="warn"><div class="stack" style="gap:6px"><span>${t('H-pattern gears are assigned. If your shifter has no neutral button, turn on return to neutral:')}</span><${SettingRow} row=${settingRow('momentary_gears')} /></div></${Notice}></div>`}
    </${Card}>
    ${kind === 'pad' && html`<${GamepadCard} e=${e} padButtons=${padButtons} change=${change} />`}
    ${kind !== 'buttons' && html`<${AxesCard} d=${d} live=${e.live} change=${change} gamepad=${kind === 'pad'} setUp=${setUp} />`}
    ${(steers || ff) && kind === 'wheel' && html`<${WheelCard} d=${d} live=${e.live} ff=${ff} change=${change} paused=${paused} />`}
    <${ButtonsCard} d=${d} live=${e.live} actions=${actions} lit=${lit} capturing=${capturing} change=${change} bind=${bind} assign=${assign} unbind=${unbind} startCapture=${startCapture} stopCapture=${stopCapture} />
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

function curves(a) {
  const now = a[2] & CURVE;
  const fit = a[0] === 0 || a[0] === 4 ? SHAPES.filter(([v]) => v === 0 || v & 0x10) : SHAPES;
  return fit.some(([v]) => v === now) ? fit : [...fit, ...SHAPES.filter(([v]) => v === now)];
}

function AxesCard({ d, live, change, gamepad, setUp }) {
  const liveOf = (k) => live?.axes?.find((x) => x[0] === k)?.[1];
  const slots = [...Array(8).keys()].filter((k) => !live || d.axes[k][0] >= 0 || liveOf(k) != null);
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
            <div class="axis-curve">${a[0] >= 0 && a[0] < 5 && html`<${Select} value=${a[2] & CURVE} options=${curves(a)} onChange=${(v) => change((c) => (c.axes[k][2] = (c.axes[k][2] & ~CURVE) | Number(v)))} />`}</div>
          </div>`;
        })}
      </div>
      ${!gamepad && html`<div class="stack" style="gap:2px"><${SettingRow} row=${DEAD_ZONE} /><${SettingRow} row=${settingRow('pedal_throttle')} /><${SettingRow} row=${settingRow('pedal_brake')} /></div>`}
    </div>
  </${Card}>`;
}

const LOOK = ['view_look_up', 'view_look_right', 'view_look_down', 'view_look_left'];

function ButtonsCard({ d, live, actions, lit, capturing, change, bind, assign, unbind, startCapture, stopCapture }) {
  const [view, setView] = useState('actions');
  const [q, setQ] = useState('');
  const fresh = lit && Date.now() - lit.at < 4000 ? lit.n : null;
  const seenHats = useRef(new Map());
  if (!seenHats.current.has(d.name)) seenHats.current.set(d.name, new Set());
  const hats = seenHats.current.get(d.name);
  if (fresh != null && fresh >= HATS) hats.add(Math.floor((fresh - HATS) / 4));
  const [, tick] = useState(0);
  useEffect(() => {
    if (fresh == null) return;
    const id = setTimeout(() => tick((x) => x + 1), 4100);
    return () => clearTimeout(id);
  }, [lit]);
  const by = new Map();
  [...d.buttons.map(([a], n) => [n, a]), ...(d.more || [])].forEach(([n, a]) => {
    if (!a) return;
    const k = a.toLowerCase();
    if (!by.has(k)) by.set(k, { action: a, buttons: [] });
    if (!by.get(k).buttons.includes(n)) by.get(k).buttons.push(n);
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
    html`<div class="lit-bar"><span class="kbd on">${buttonName(fresh)}</span><span>${actionsOf(d, fresh).length ? actionsOf(d, fresh).map(actionText).join(' + ') : t('Nothing')}</span></div>`}
    ${view === 'actions'
      ? html`<${ByAction} actions=${actions} by=${by} q=${q} setQ=${setQ} fresh=${fresh} capturing=${capturing} latching=${d.latching} startCapture=${startCapture} unbind=${unbind} live=${live} />`
      : html`<${ByButton} d=${d} live=${live} actions=${actions} fresh=${fresh} hats=${hats} change=${change} bind=${bind} assign=${assign} unbind=${unbind} />`}
  </${Card}>`;
}

const actionsOf = (d, n) => [d.buttons[n]?.[0] || '', ...(d.more || []).filter(([b]) => b === n).map(([, a]) => a)].filter(Boolean);

function ByAction({ actions, by, q, setQ, fresh, capturing, latching, startCapture, unbind, live }) {
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
              (n) => html`<span class=${'kbd chip' + (n === fresh ? ' on' : '')}>${buttonName(n)}${latching.includes(n) && html`<small>${t('Latching')}</small>`}<button title=${t('Remove')} onClick=${(e) => (e.stopPropagation(), unbind(n, a))}><${Icon} name="x" size=${14} /></button></span>`,
            )}
            ${can && html`<span class="assign">${bound.length ? html`<${Icon} name="plus" size=${16} />` : t('Assign')}</span>`}
          </div>`}
    </div>`;
  };
  if (query) {
    return html`<div class="stack" style="gap:14px">
      <${Search} value=${q} onInput=${setQ} placeholder="Search actions..." />
      <${SearchHits} q=${q} all=${[...new Set([...actions, ...others])]} row=${row} />
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

const HITS = 100;

function SearchHits({ q, all, row }) {
  const kb = S.cache?.keys;
  const [cat] = useKeyCatalog((kb?.vehicles || []).map((b) => b.action), !!S.poll?.instances?.length);
  const [rows, setRows] = useState(HITS);
  useEffect(() => setRows(HITS), [q]);
  const more = useMemo(() => {
    const ours = new Set(all.map((a) => a.toLowerCase()));
    const game = new Set((kb?.game || []).map((b) => b.action.toLowerCase()));
    return (cat?.options || []).map((o) => o.action).filter((a) => !ours.has(a.toLowerCase()) && !game.has(a.toLowerCase()) && !/^(vr_|chat_)/.test(a));
  }, [cat, all.join('\n'), kb]);
  const query = q.trim().toLowerCase();
  const hits = useMemo(() => {
    const find = (a) => actionText(a).toLowerCase().includes(query) || a.toLowerCase().includes(query);
    return [...all.filter(find), ...more.filter(find)];
  }, [query, all.join('\n'), more, actionNamesVersion()]);
  const typed = q.trim();
  const custom = !hits.some((a) => a.toLowerCase() === typed.toLowerCase()) && /^[A-Za-z0-9_]{2,}$/.test(typed);
  return html`
    <div class="binds">${hits.slice(0, rows).map(row)}${custom && row(typed)}</div>
    ${hits.length > rows && html`<div class="row muted small wrap" style="justify-content:space-between">
      <span>${t('Showing the first {shown} of {n}: search to narrow the list.', { shown: rows, n: hits.length })}</span>
      <${Button} small label="Show more" onClick=${() => setRows(rows + HITS)} />
    </div>`}
    ${!hits.length && !custom && html`<${Empty} icon="search" title="No matching actions." />`}`;
}

function ByButton({ d, live, actions, fresh, hats: seen, change, bind, assign, unbind }) {
  const [adding, setAdding] = useState(null);
  const physical = live?.buttons || 0;
  const last = d.buttons.reduce((m, [a], i) => (a && i < HATS ? i + 1 : m), 0);
  const count = Math.min(HATS, Math.max(physical, last, fresh != null && fresh < HATS ? fresh + 1 : 0, live ? 0 : last));
  const bound = [...d.buttons.map(([a], i) => (a ? i : -1)), ...(d.more || []).map(([b]) => b)].filter((i) => i >= HATS);
  const hats = [...new Set([...bound.map((i) => Math.floor((i - HATS) / 4)), ...seen])].sort((a, b) => a - b);
  const options = [['', t('Nothing')], ...actions.map((a) => [a, actionText(a)]).sort((x, y) => x[1].localeCompare(y[1]))];
  const opts = (a) => (options.some(([v]) => v === a) ? options : [...options, [a, actionText(a)]]);
  const ref = useRef();
  useEffect(() => {
    if (fresh != null) ref.current?.querySelector(`[data-n="${fresh}"]`)?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }, [fresh]);
  if (!count && !hats.length) return html`<${Empty} icon="gamepad" text="Press a button on the device: it lights up here." />`;
  const lookAround = (h) => change((c) => {
    while (c.buttons.length < HATS + h * 4 + 4) c.buttons.push(['', '0']);
    LOOK.forEach((a, k) => {
      const n = HATS + h * 4 + k;
      c.buttons[n] = [a, '0'];
      c.more = (c.more || []).filter(([b]) => b !== n);
      c.latching = c.latching.filter((b) => b !== n);
    });
  });
  const row = (n) => {
    const a = d.buttons[n]?.[0] || '';
    const more = (d.more || []).filter(([b]) => b === n).map(([, x]) => x);
    const flag = (c, on) => {
      while (c.buttons.length <= n) c.buttons.push(['', '0']);
      c.buttons[n][1] = on ? '1' : '0';
    };
    return html`<div class=${'button-row' + (n === fresh ? ' lit' : '')} data-n=${n}>
      <span class=${'kbd' + (n === fresh ? ' on' : '')}>${buttonName(n)}</span>
      <div class="button-actions">
        <div class="button-action">
          <${Select} value=${a} options=${opts(a)} raw onChange=${(v) => bind(n, v)} />
          <${Button} small kind="ghost" icon="plus" title="Add another action" disabled=${!a || adding === n} onClick=${() => setAdding(n)} />
        </div>
        ${more.map(
          (x) => html`<div class="button-action">
            <${Select} value=${x} options=${opts(x)} raw onChange=${(v) => (unbind(n, x), v && assign(n, v))} />
            <${Button} small kind="ghost" icon="x" title="Remove" onClick=${() => unbind(n, x)} />
          </div>`,
        )}
        ${adding === n &&
        html`<div class="button-action">
          <${Select} value="" options=${options.slice(1)} raw placeholder="Choose…" onChange=${(v) => (setAdding(null), v && assign(n, v))} />
          <${Button} small kind="ghost" icon="x" title="Cancel" onClick=${() => setAdding(null)} />
        </div>`}
        <div class="button-switches">
          <${Toggle} checked=${d.latching.includes(n)} disabled=${!a} label="Latching" onChange=${(v) => change((c) => (c.latching = v ? [...c.latching.filter((x) => x !== n), n].sort((x, y) => x - y) : c.latching.filter((x) => x !== n)))} />
          <${Toggle} checked=${d.buttons[n]?.[1] === '1'} disabled=${!a} label="Continuous" onChange=${(v) => change((c) => flag(c, v))} />
        </div>
      </div>
    </div>`;
  };
  return html`<div class="button-grid" ref=${ref}>
    ${[...Array(count).keys()].map(row)}
    ${hats.map(
      (h) => html`<div class="hat-head">
          <b>${t('Hat {n}', { n: h + 1 })}</b>
          <${Button} small label="Look around with it" onClick=${() => lookAround(h)} />
        </div>
        ${[0, 1, 2, 3].map((k) => row(HATS + h * 4 + k))}`,
    )}
  </div>`;
}

function WheelCard({ d, live, ff, change, paused }) {
  const [test, setTest] = useState(false);
  const axis = d.axes.findIndex((a) => a[0] === 0);
  return html`<${Card} title="Steering wheel">
      <div class="stack" style="gap:6px">
        <div class="section-title">${t('For every wheel')}</div>
        <${SettingRow} row=${settingRow('wheel_range')} />
        <${SettingRow} row=${settingRow('wheel_lock')} />
        <${SettingRow} row=${settingRow('cab_wheel_rotation')} />
        ${ff && html`<${SettingRow} row=${settingRow('ff_enabled')} />`}
      </div>
    </${Card}>
    ${ff && html`<${FeedbackCard} d=${d} live=${live} axis=${axis} change=${change} onTest=${() => setTest(true)} />`}
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
    />`}`;
}

const pct = (v) => `${Math.round(v * 100)} %`;
const smoothMs = (v) => (v < 0.0005 ? t('Off') : `${Math.round(v * 1000)} ms`);

/** A force feedback slider with a sentence on what it does. */
function FeedbackSlider({ label, help, value, max, step = 0.05, format = pct, onChange }) {
  return html`<div class="setting-row full"><${Slider} label=${label} help=${help} value=${value} min=${0} max=${max} step=${step} format=${format} onChange=${onChange} /></div>`;
}

/** This wheel's force feedback: its strengths and tuning (saved with the device), then the
 * vibration every wheel shares and the direction. */
function FeedbackCard({ d, live, axis, change, onTest }) {
  const [steer, vib] = d.ff_scale || [1, 1];
  const tune = tuneOf(d);
  const scale = (i) => (v) =>
    change((c) => {
      const next = [...(c.ff_scale || [1, 1])];
      next[i] = v;
      c.ff_scale = next;
    });
  const tuned = (key) => (v) => change((c) => setTune(c, { [key]: v }));
  // (the five tuning values only, as the game's own Reset tuning: the strengths have their sliders)
  const reset = () => change((c) => (c.ff_tune = null));
  const custom = d.ff_tune != null;
  return html`<${Card} title="Force feedback" actions=${html`<${Button} small kind="ghost" icon="refresh" label="Reset tuning" disabled=${!custom} onClick=${reset} />`}>
    <div class="stack" style="gap:6px">
      ${!!S.poll?.instances?.length && html`<${Notice} kind="warn">${t("Changes take effect the next time you drive, or after Reload saved controllers in the game's controller menu.")}</${Notice}>`}
      <div class="section-title">${t('Steering')}</div>
      <div class="ff-grid">
        <${FeedbackSlider} label="Steering force" help="Overall strength of the steering forces on this wheel; lower it on a strong direct-drive base, raise it on a weak wheel." value=${steer} max=${2} onChange=${scale(0)} />
        <${FeedbackSlider} label="Self-centring" help="How strongly the moving bus turns the wheel back to straight after a turn; it never pulls while the bus is stopped." value=${tune.centring} max=${2} onChange=${tuned('centring')} />
        <${FeedbackSlider} label="Damping" help="How much the wheel resists fast movement, so a released wheel settles at the centre instead of swinging past it; raise it if the wheel oscillates." value=${tune.damping} max=${3} onChange=${tuned('damping')} />
        <${FeedbackSlider} label="Parking resistance" help="How heavy the wheel is to turn while the bus stands or crawls; it eases as the bus rolls." value=${tune.parking} max=${2} onChange=${tuned('parking')} />
      </div>
      <div class="section-title">${t('Fine tuning')}</div>
      <div class="ff-grid">
        <${FeedbackSlider} label="Minimum force" help="Lifts the smallest forces above the point where your wheel starts to feel them; useful on belt and gear wheels with a dead centre, leave at 0 on direct drive." value=${tune.min_force} max=${0.2} step=${0.01} onChange=${tuned('min_force')} />
        <${FeedbackSlider} label="Smoothing" help="Softens sudden steps in the steering force that can feel grainy on strong wheels at low frame rates; higher values add a little delay." value=${tune.smoothing} max=${0.05} step=${0.005} format=${smoothMs} onChange=${tuned('smoothing')} />
      </div>
      <div class="section-title">${t('Vibration')}</div>
      <div class="ff-grid">
        <${FeedbackSlider} label="Vibration" help="Overall strength of shakes, kerb jolts and the road and engine tremble on this wheel." value=${vib} max=${2} onChange=${scale(1)} />
        <${SettingRow} row=${settingRow('ff_road_vib')} />
        <${SettingRow} row=${settingRow('ff_engine_vib')} />
        <${SettingRow} row=${settingRow('ff_fade')} />
      </div>
      <div class="setting-row">
        <div class="name">${t('Invert force feedback')}</div>
        <div class="row" style="gap:10px;justify-content:flex-end">
          <${Button} small icon="play" label="Test the direction" disabled=${!live || axis < 0} onClick=${onTest} />
          <${Toggle} checked=${d.ff_invert ?? !!S.settings?.ff_invert} onChange=${(v) => change((c) => (c.ff_invert = v))} />
        </div>
      </div>
    </div>
  </${Card}>`;
}

function padKind(e) {
  const set = S.settings?.pad_type;
  return set && set !== 'auto' ? set : e.live?.pad || 'xbox';
}

function GamepadCard({ e, padButtons, change }) {
  const kind = padKind(e);
  const custom = e.cfg?.buttons.some(([a]) => a);
  const [steer, force] = e.cfg?.ff_scale || [1, 1];
  return html`<${Card} title="Gamepad">
    <div class="stack" style="gap:6px">
      <${SettingRow} row=${settingRow('pad_type')} />
      ${change && e.live?.ff && html`<div class="setting-row full"><${Slider} label="Vibration" value=${force} min=${0} max=${2} step=${0.05} format=${(v) => `${Math.round(v * 100)} %`} onChange=${(v) => change((c) => (c.ff_scale = [steer, v]))} /></div>`}
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
