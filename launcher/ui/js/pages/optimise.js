import { html, useState, useEffect } from '../../vendor/preact-htm.js';
import { S, set, toast, useStore } from '../store.js';
import { t } from '../i18n.js';
import { call } from '../api.js';
import { Button, Select } from '../ui.js';
import { settingRow } from './settings.js';

const TARGETS = [['0', 'Automatic'], ['60', '60 fps'], ['90', '90 fps'], ['120', '120 fps'], ['144', '144 fps']];

function shown(key, v) {
  if (typeof v === 'boolean') return t(v ? 'On' : 'Off');
  const row = settingRow(key);
  const options = row && Array.isArray(row.options) ? row.options : [];
  const match = options.find(([o]) => String(o) === String(v) || (Number(o) === Number(v) && String(v).trim() !== ''));
  return match ? t(match[1]) : String(v);
}

function label(key) {
  const row = settingRow(key);
  return row ? t(row.label) : key;
}

export function OptimiseBlock() {
  const s = useStore();
  const [status, setStatus] = useState(null);
  const [target, setTarget] = useState('0');
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let gone = false;
    const load = () =>
      call('optimise_status')
        .then((st) => {
          if (gone) return;
          setStatus(st);
          if (st?.armed != null) setTarget(String(st.armed));
        })
        .catch(() => !gone && setStatus({}));
    load();
    const every = setInterval(load, 5000);
    return () => {
      gone = true;
      clearInterval(every);
    };
  }, []);
  const gameRuns = (s.poll?.instances || []).some((i) => i.running !== false);
  const act = async (cmd, args) => {
    setBusy(true);
    try {
      setStatus(await call(cmd, args));
    } catch (e) {
      toast(t(e.message), 'error');
    }
    setBusy(false);
  };
  const restore = async () => {
    setBusy(true);
    try {
      set({ settings: await call('optimise_restore') });
      setStatus(await call('optimise_status'));
      toast(t('Your previous settings are back'), 'ok');
    } catch (e) {
      toast(t(e.message), 'error');
    }
    setBusy(false);
  };
  const st = status || {};
  const last = st.last;
  const armed = st.armed != null;
  return html`<div class="optimise-block">
    <div class="section-title">${t('Optimise for my PC')}</div>
    <p class="muted small" style="margin:0 0 4px">${t('During your next drive the game measures for about a minute and adjusts the graphics to reach the frame rate you aim at. Your settings are saved first.')}</p>
    <div class="setting-row">
      <div class="name">${t('Frame rate aimed at')}<span>${t('Automatic: the screen\'s refresh rate, at most 100 fps')}</span></div>
      <${Select} value=${target} options=${TARGETS.some(([v]) => v === target) ? TARGETS : [...TARGETS, [target, `${target} fps`]]} disabled=${busy || armed} onChange=${setTarget} />
    </div>
    <div class="setting-row full">
      ${armed
        ? html`<div class="grid-2">
            <span class="muted small" style="align-self:center">${t('Waiting for your next drive')}</span>
            <${Button} icon="x" label="Cancel" disabled=${busy} onClick=${() => act('optimise_cancel')} />
          </div>`
        : html`<div><${Button} kind="primary" icon="sliders" label="Optimise on my next drive" disabled=${busy || !status} onClick=${() => act('optimise_arm', { target: Number(target) })} /></div>`}
    </div>
    ${last &&
    html`<div class="optimise-result">
      <div class="small">${t('Last optimisation: {before} → {after} fps', { before: last.before_fps, after: last.after_fps })}</div>
      ${(last.changes || []).length
        ? html`<ul class="muted small">
            ${last.changes.map((c) => html`<li>${label(c.key)}: ${shown(c.key, c.from)} → ${shown(c.key, c.to)}</li>`)}
          </ul>`
        : html`<div class="muted small">${t('Nothing needed changing')}</div>`}
      ${last.restart && html`<div class="muted small">${t('Takes effect when the game starts the next time')}</div>`}
    </div>`}
    ${st.backup &&
    html`<div class="setting-row full">
      <div><${Button} icon="history" label="Restore my previous settings" disabled=${busy || gameRuns} onClick=${restore} /></div>
    </div>`}
  </div>`;
}
