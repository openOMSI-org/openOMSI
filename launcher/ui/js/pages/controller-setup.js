import { html, useState, useRef } from '../../vendor/preact-htm.js';
import { S } from '../store.js';
import { t } from '../i18n.js';
import { call } from '../api.js';
import { Icon } from '../icons.js';
import { Button, Dialog, Notice, Slider, Toggle, Spinner } from '../ui.js';

const AXIS_NAMES = ['X axis', 'Y axis', 'Z axis', 'X rotation', 'Y rotation', 'Z rotation', 'Slider 1', 'Slider 2'];
const PULSE = 0.2;
const MAX_PULSE = 0.5;

const STEPS = [
  ['Let go of everything', 'Take your hands off the wheel and your feet off the pedals (the wheel in the middle), then press Next.'],
  ['Steering', 'Turn the wheel (or move the stick) all the way to the LEFT and hold it there, then press Next.'],
  ['Throttle', 'Press the throttle pedal all the way down and hold it, then press Next. No pedals: Skip.'],
  ['Brake', 'Press the brake pedal all the way down and hold it, then press Next. No brake pedal: Skip.'],
  ['Clutch', 'Press the clutch pedal all the way down and hold it, then press Next. No clutch: Skip.'],
];

export function Meter({ value, centre = true, fill = false }) {
  const v = Math.max(centre ? -1 : 0, Math.min(1, Number(value) || 0));
  const pos = centre ? (v + 1) * 50 : v * 100;
  const from = centre ? Math.min(50, pos) : 0;
  const width = centre ? Math.abs(pos - 50) : pos;
  return html`<div class=${'meter' + (centre ? ' centre' : '')}>
    ${fill && html`<div class="meter-fill" style=${`left:${from}%;width:${width}%`}></div>`}
    <div class="meter-mark" style=${`left:${pos}%`}></div>
  </div>`;
}

export function useMoving(axes) {
  const ref = useRef({ prev: new Map(), heat: new Map(), at: 0 });
  const r = ref.current;
  const now = performance.now();
  const decay = r.at ? Math.exp(-(now - r.at) / 400) : 0;
  r.at = now;
  let best = null;
  let most = 0.04;
  for (const [k, v] of axes || []) {
    const before = r.prev.get(k);
    const heat = (r.heat.get(k) || 0) * decay + (before == null ? 0 : Math.abs(v - before));
    r.heat.set(k, heat);
    r.prev.set(k, v);
    if (heat > most) {
      most = heat;
      best = k;
    }
  }
  return best;
}

function snapshot(live) {
  const a = Array(8).fill(null);
  for (const [k, v] of live?.axes || []) if (k < 8) a[k] = v;
  return a;
}

export function movedMost(rest, now, exclude) {
  let best = null;
  for (let k = 0; k < 8; k++) {
    if (exclude.includes(k) || now[k] == null) continue;
    const d = now[k] - (rest[k] ?? 0);
    if (Math.abs(d) > 0.33 && (!best || Math.abs(d) > Math.abs(best[1]))) best = [k, d];
  }
  return best;
}

export function wizardResult(rest, at) {
  const axes = Array(8).fill(null);
  const steer = at[0] ? movedMost(rest, at[0], []) : null;
  if (steer) axes[steer[0]] = [0, steer[1] > 0];
  const taken = steer ? [steer[0]] : [];
  const pedal = (i, ex) => (at[i] ? movedMost(rest, at[i], ex) : null);
  const throttle = pedal(1, taken);
  const brake = pedal(2, taken);
  if (throttle && brake && throttle[0] === brake[0] && throttle[1] * brake[1] < 0) {
    axes[throttle[0]] = [4, throttle[1] < 0];
  } else {
    if (throttle) axes[throttle[0]] = [1, throttle[1] < 0];
    if (brake && (!throttle || brake[0] !== throttle[0])) axes[brake[0]] = [2, brake[1] < 0];
  }
  const ex = [...taken, ...(throttle ? [throttle[0]] : []), ...(brake ? [brake[0]] : [])];
  const clutch = pedal(3, ex);
  if (clutch) axes[clutch[0]] = [3, clutch[1] < 0];
  return axes;
}

export function SetupWizard({ device, live, paused, onClose, onDone }) {
  const busy = useRef(false);
  const close = () => !busy.current && onClose();
  const [step, setStep] = useState(0);
  const [rest, setRest] = useState(null);
  const [at, setAt] = useState([]);
  const [error, setError] = useState('');
  const cur = snapshot(live);
  const used = at.map((a) => (a && rest ? movedMost(rest, a, [])?.[0] : null)).filter((k) => k != null);
  const steerUsed = at[0] && rest ? movedMost(rest, at[0], [])?.[0] : null;
  const exclude = step === 3 ? (steerUsed != null ? [steerUsed] : []) : used;
  const found = step > 0 && step <= 4 && rest ? movedMost(rest, cur, exclude) : null;
  const result = rest ? wizardResult(rest, at) : null;
  const steerAxis = result ? result.findIndex((a) => a && a[0] === 0) : -1;
  const feedback = step === STEPS.length && live?.ff && !live?.gamepad && steerAxis >= 0;

  function next(skip) {
    setError('');
    if (step === 0) {
      if (!live?.axes?.length) return setError('The device shows no axis yet: move the wheel and the pedals a little, let go, and press Next again.');
      setRest(cur);
    } else if (skip) {
      setAt([...at, null]);
    } else if (!found) {
      return setError('Nothing moved far enough. Hold it all the way, then press Next (or Skip).');
    } else {
      setAt([...at, cur]);
    }
    const n = step + 1;
    const r = step === 0 ? cur : rest;
    const all = step === 0 ? [] : [...at, skip ? null : cur];
    if (n === STEPS.length) {
      const axes = wizardResult(r, all);
      const steers = axes.findIndex((a) => a && a[0] === 0);
      if (!(live?.ff && !live?.gamepad && steers >= 0)) return onDone(axes, null);
    }
    setStep(n);
  }

  function back() {
    setError('');
    if (step === 1) setRest(null);
    else if (step > 1) setAt(at.slice(0, -1));
    setStep(step - 1);
  }

  const total = STEPS.length + (live?.ff && !live?.gamepad ? 1 : 0);
  const [title, text] = feedback ? ['Force feedback direction', ''] : STEPS[Math.min(step, STEPS.length - 1)];

  return html`<${Dialog} title="Set up step by step" onClose=${close}>
    <div class="stack wizard" style="gap:16px">
      <div class="steps">${[...Array(total).keys()].map((i) => html`<span class=${i < step ? 'done' : i === step ? 'on' : ''}></span>`)}</div>
      <div>
        <div class="muted small">${device.name}</div>
        <h3 style="font-size:18px;margin-top:2px">${t(title)}</h3>
        ${text && html`<p class="dim" style="margin-top:6px">${t(text)}</p>`}
      </div>
      ${!live && html`<${Notice} kind="danger">${t('The device is not connected: plug it in (the list on the left marks it green).')}</${Notice}>`}
      ${feedback
        ? html`<${FeedbackBody} device=${device} axis=${steerAxis} live=${live} paused=${paused} initial=${device.ff_invert ?? !!S.settings?.ff_invert} busy=${busy} onDone=${(v) => onDone(result, v)} onCancel=${close} />`
        : html`
          <div class="wizard-axes">
            ${(live?.axes || []).map(([k, v]) => {
              const isFound = found && found[0] === k;
              const isUsed = used.includes(k);
              return html`<div class=${'live-axis' + (isFound ? ' moving' : '') + (isUsed ? ' used' : '')}>
                <span>${t(AXIS_NAMES[k])}</span><${Meter} value=${v} />${isFound ? html`<${Icon} name="check" size=${16} />` : html`<i></i>`}
              </div>`;
            })}
          </div>
          ${found && html`<div class="row" style="gap:8px;color:var(--ok)"><${Icon} name="check" />${t('Found: {axis}', { axis: t(AXIS_NAMES[found[0]]) })}</div>`}
          ${error && html`<${Notice} kind="danger">${t(error)}</${Notice}>`}
          <div class="row" style="justify-content:space-between">
            <div class="row">
              <${Button} kind="ghost" label="Cancel" onClick=${onClose} />
              ${step > 0 && html`<${Button} kind="ghost" icon="left" label="Back" onClick=${back} />`}
            </div>
            <div class="row">
              ${step >= 1 && html`<${Button} label="Skip" onClick=${() => next(true)} />`}
              <${Button} kind="primary" icon="right" label=${step === STEPS.length - 1 && total === STEPS.length ? 'Finish' : 'Next'} disabled=${!live} onClick=${() => next(false)} />
            </div>
          </div>`}
    </div>
  </${Dialog}>`;
}

export function FeedbackTest({ device, axis, live, paused, invert, onClose, onDone }) {
  const busy = useRef(false);
  const close = () => !busy.current && onClose();
  return html`<${Dialog} title="Force feedback direction" onClose=${close}>
    <${FeedbackBody} device=${device} axis=${axis} live=${live} paused=${paused} initial=${invert} busy=${busy} onDone=${onDone} onCancel=${close} />
  </${Dialog}>`;
}

function FeedbackBody({ device, axis, live, paused, initial, busy, onDone, onCancel }) {
  const [strength, setStrength] = useState(PULSE);
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState(null);
  const [choice, setChoice] = useState(initial);

  async function start() {
    setRunning(true);
    setResult(null);
    busy.current = true;
    paused.current = true;
    try {
      const r = await call('controllers_ffb_test', { name: live?.name || device.name, axis, strength });
      setChoice(r.invert);
      setResult({ ok: true, invert: r.invert });
    } catch (e) {
      setResult({ ok: false, text: e.message });
    }
    paused.current = false;
    busy.current = false;
    setRunning(false);
  }

  return html`<div class="stack" style="gap:14px">
    <div class="injury"><${Icon} name="alert" size=${26} /><b>${t('INJURY RISK: TAKE YOUR HANDS OFF THE WHEEL. Keep hands and fingers clear before starting and throughout the test.')}</b></div>
    <p class="dim">${t('The test applies two short forces in opposite directions.')}</p>
    ${running && html`<div class="row muted"><${Spinner} />${t('Testing: keep your hands off the wheel...')}</div>`}
    ${result && (result.ok ? html`<div class="row" style="gap:8px;color:var(--ok)"><${Icon} name="check" />${t(result.invert ? 'Direction detected: inverted' : 'Direction detected: normal')}</div>` : html`<${Notice} kind="danger">${t(result.text)}</${Notice}>`)}
    <${Slider} label="Test strength" value=${strength} min=${PULSE} max=${MAX_PULSE} step=${0.01} format=${(v) => `${Math.round(v * 100)} %`} onChange=${setStrength} />
    <p class="muted small">${t('If the wheel barely moves, increase Test strength and retry. Keep your hands clear.')}</p>
    <${Toggle} checked=${choice} label="Invert force feedback" onChange=${setChoice} />
    <p class="muted small">${t("If detection is inconclusive, retry or choose the direction manually. You can change it later on this device's page.")}</p>
    <div class="row" style="justify-content:space-between">
      <${Button} kind="ghost" label="Cancel" disabled=${running} onClick=${onCancel} />
      <div class="row">
        <${Button} icon="play" label="Start test" disabled=${running || !live} onClick=${start} />
        <${Button} kind="primary" icon="check" label="Finish" disabled=${running} onClick=${() => onDone(choice)} />
      </div>
    </div>
  </div>`;
}
