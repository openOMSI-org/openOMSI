// Controls: the keys of OMSI's keyboard.cfg (the bus's own and the game's), changed by
// pressing the new one, and a live look at the game controllers connected.

import { html, useState, useEffect } from '../../vendor/preact-htm.js';
import { S, useStore, set, toast } from '../store.js';
import { t } from '../i18n.js';
import { call } from '../api.js';
import { saveSettings, cached } from '../data.js';
import { Button, Card, Search, Segmented, Tabs, Notice, Empty, Spinner, Dialog, useConfirm, PageHead } from '../ui.js';
import { KNOWN, actionText } from './actions.js';
import { Controllers } from './controllers.js';

// --- keys ---------------------------------------------------------------------------------

/** The entry's third value: the action is told the key's state every frame (no part of the
 * chord), and the modifier keys held with it. Omsi.exe reads Shift and Ctrl; Alt is ours. */
const KEY_HOLD = 1;
const KEY_SHIFT = 2;
const KEY_CTRL = 4;
const KEY_ALT = 8;
const CHORD = KEY_SHIFT | KEY_CTRL | KEY_ALT;

/** `KeyboardEvent.code` → DirectInput scan code and the key's name (the launcher's table:
 * winit's key codes, whose names the browser's codes share but for the logo keys). */
const KEYS = [
  ['Escape', 1], ['Digit1', 2], ['Digit2', 3], ['Digit3', 4], ['Digit4', 5], ['Digit5', 6], ['Digit6', 7], ['Digit7', 8], ['Digit8', 9], ['Digit9', 10], ['Digit0', 11],
  ['Minus', 12], ['Equal', 13], ['Backspace', 14], ['Tab', 15],
  ['KeyQ', 16], ['KeyW', 17], ['KeyE', 18], ['KeyR', 19], ['KeyT', 20], ['KeyY', 21], ['KeyU', 22], ['KeyI', 23], ['KeyO', 24], ['KeyP', 25],
  ['BracketLeft', 26], ['BracketRight', 27], ['Enter', 28], ['ControlLeft', 29],
  ['KeyA', 30], ['KeyS', 31], ['KeyD', 32], ['KeyF', 33], ['KeyG', 34], ['KeyH', 35], ['KeyJ', 36], ['KeyK', 37], ['KeyL', 38],
  ['Semicolon', 39], ['Quote', 40], ['Backquote', 41], ['ShiftLeft', 42], ['Backslash', 43],
  ['KeyZ', 44], ['KeyX', 45], ['KeyC', 46], ['KeyV', 47], ['KeyB', 48], ['KeyN', 49], ['KeyM', 50],
  ['Comma', 51], ['Period', 52], ['Slash', 53], ['ShiftRight', 54], ['NumpadMultiply', 55], ['AltLeft', 56], ['Space', 57], ['CapsLock', 58],
  ['F1', 59], ['F2', 60], ['F3', 61], ['F4', 62], ['F5', 63], ['F6', 64], ['F7', 65], ['F8', 66], ['F9', 67], ['F10', 68],
  ['NumLock', 69], ['ScrollLock', 70], ['Numpad7', 71], ['Numpad8', 72], ['Numpad9', 73], ['NumpadSubtract', 74], ['Numpad4', 75], ['Numpad5', 76], ['Numpad6', 77],
  ['NumpadAdd', 78], ['Numpad1', 79], ['Numpad2', 80], ['Numpad3', 81], ['Numpad0', 82], ['NumpadDecimal', 83], ['IntlBackslash', 86], ['F11', 87], ['F12', 88],
  ['F13', 100], ['F14', 101], ['F15', 102], ['NumpadEqual', 141], ['NumpadEnter', 156], ['ControlRight', 157], ['NumpadDivide', 181], ['PrintScreen', 183], ['AltRight', 184],
  ['Pause', 197], ['Home', 199], ['ArrowUp', 200], ['PageUp', 201], ['ArrowLeft', 203], ['ArrowRight', 205], ['End', 207], ['ArrowDown', 208], ['PageDown', 209],
  ['Insert', 210], ['Delete', 211], ['MetaLeft', 219, 'SuperLeft'], ['MetaRight', 220, 'SuperRight'], ['ContextMenu', 221],
];
const SCAN_OF = Object.fromEntries(KEYS.map(([code, scan]) => [code, scan]));
const PUNCT = { Period: '.', Comma: ',', Minus: '-', Equal: '=', Slash: '/', Semicolon: ';' };
const NAME_OF = Object.fromEntries(
  KEYS.map(([code, scan, winit]) => {
    let n = winit || code;
    n = n.startsWith('Key') ? n.slice(3) : n.startsWith('Digit') ? n.slice(5) : n;
    n = n.replace('Numpad', 'Num').replace('Arrow', '');
    return [scan, PUNCT[n] || n];
  }),
);
const MODIFIER_CODES = new Set(['ShiftLeft', 'ShiftRight', 'ControlLeft', 'ControlRight', 'AltLeft', 'AltRight', 'MetaLeft', 'MetaRight', 'OSLeft', 'OSRight']);

/** The caps of a key and its modifiers ("Shift", "D"); null for none. */
function keyParts(scan, modifier) {
  if (!scan) return null;
  const parts = [];
  if (modifier & KEY_SHIFT) parts.push('Shift');
  if (modifier & KEY_CTRL) parts.push('Ctrl');
  if (modifier & KEY_ALT) parts.push('Alt');
  parts.push(NAME_OF[scan] || `scan ${scan}`);
  return parts;
}

/** As the player reads it ("Ctrl+D"); `(unbound)` for none. */
const keyName = (scan, modifier) => keyParts(scan, modifier)?.join('+') || '(unbound)';

function Keys({ scan, modifier }) {
  const parts = keyParts(scan, modifier);
  if (!parts) return html`<span class="muted small">${t('(unbound)')}</span>`;
  return html`<span class="row" style="gap:4px;flex-wrap:wrap">${parts.map((p) => html`<span class="kbd">${p}</span>`)}</span>`;
}

// --- OMSI 2's own keys (Inputs/keyboard.cfg as it comes; the game's section ends where the
// bus's begins, at the throttle) ------------------------------------------------------------

const STOCK = [
  ['debug_start_bench2', 48, 6], ['sim_pause', 25, 0], ['debug_window', 1, 2], ['open_mainmenue', 1, 0], ['exit', 16, 4],
  ['view_toggle_viewpoint', 31, 0], ['view_set_driver', 59, 0], ['view_set_passenger', 60, 0], ['view_set_outside', 61, 0], ['view_set_map', 62, 0],
  ['view_set_ego', 87, 0], ['view_set_schedule', 210, 1], ['view_set_ticketselling', 199, 1], ['view_reset_direction', 46, 0], ['view_reset_all_directions', 57, 0],
  ['view_interiorcam_minus', 203, 0], ['view_interiorcam_plus', 205, 0], ['view_toggle_informationdisplay', 21, 2], ['screenshot', 25, 6], ['toggel_ctrler', 37, 0],
  ['toggel_mouse_ctrl', 24, 0], ['open_menue', 92, 0], ['debug_threadcalc', 88, 4], ['scendes_grabmode', 34, 0], ['scendes_rotmode', 19, 0],
  ['scendes_set_x', 45, 0], ['scendes_set_y', 44, 0], ['scendes_set_z', 21, 0], ['scendes_enter', 28, 0], ['scendes_cancel', 1, 0],
  ['scendes_aitraffic_desel', 90, 3], ['scendes_aitraffic_deact', 91, 5], ['scendes_new', 49, 0], ['scendes_delete', 211, 0], ['scendes_nodechange_pri', 67, 0],
  ['scendes_rast_strg', 91, 4], ['ego_forward', 17, 1], ['quicksave', 31, 4],
  ['throttle', 72, 1], ['brake', 80, 1], ['throttle_amplify', 78, 1], ['clutch', 15, 1], ['steering_left', 75, 1], ['steering_neutral', 76, 1], ['steering_right', 77, 1],
  ['ticket_give', 20, 0], ['change_give', 20, 4], ['parking_brake_toggle', 52, 0], ['blinker_left_set', 71, 1], ['blinker_right_set', 73, 1], ['blinker_off', 83, 0],
  ['blinker_warn_toggle', 48, 0], ['kw_scheinwerfer_toggle', 38, 0], ['kw_standlicht_toggle', 38, 2], ['kw_fernlicht_toggle', 33, 0], ['kw_m_enginestart', 50, 0],
  ['kw_wipermode_up', 17, 0], ['horn', 35, 1], ['cp_microphone', 16, 1], ['cp_fahrerlicht_toggle', 7, 0], ['cp_licht_untenrechts_toggle', 8, 0],
  ['cp_licht_oberdeck_toggle', 9, 0], ['cp_licht_unterdeck_toggle', 10, 0], ['kw_s_R', 19, 0], ['kw_s_N', 49, 0], ['kw_s_1', 2, 0], ['kw_s_2', 3, 0], ['kw_s_3', 4, 0],
  ['kw_s_4', 5, 0], ['kw_s_5', 6, 0], ['kw_s_6', 7, 0], ['automatic_R', 19, 0], ['automatic_N', 49, 0], ['automatic_1', 2, 0], ['automatic_2', 3, 0], ['automatic_D', 32, 0],
  ['bus_doorfront0', 181, 0], ['bus_doorfront1', 55, 0], ['bus_dooraft', 74, 0], ['bus_linie_plus', 66, 0], ['bus_linie_minus', 63, 0], ['bus_ziel_plus', 65, 0],
  ['bus_ziel_minus', 64, 0], ['bus_rollband_setL1', 63, 0], ['bus_rollband_setL2', 64, 0], ['bus_rollband_setL3', 65, 0], ['bus_rollband_setT', 66, 0],
  ['bus_rollband_up', 201, 1], ['bus_rollband_dn', 209, 1], ['bus_rollband_up_step', 201, 0], ['bus_rollband_dn_step', 209, 0], ['IBIS_eingabe', 156, 4],
  ['IBIS_1', 79, 4], ['IBIS_2', 80, 4], ['IBIS_3', 81, 4], ['IBIS_4', 75, 4], ['IBIS_5', 76, 4], ['IBIS_6', 77, 4], ['IBIS_7', 71, 4], ['IBIS_8', 72, 4], ['IBIS_9', 73, 4],
  ['IBIS_0', 82, 4], ['IBIS_loeschen', 83, 4], ['IBIS_setmode_linie_kurs', 55, 4], ['IBIS_setmode_route', 181, 4], ['IBIS_setmode_ziel', 74, 4],
  ['cashdesk_changer_8_00', 7, 6], ['cashdesk_changer_4_00', 8, 6], ['cashdesk_changer_1_50', 9, 6], ['cashdesk_changer_0_30', 10, 6], ['cashdesk_changer_0_15', 11, 6],
  ['cashdesk_changer_2_00', 7, 4], ['cashdesk_changer_1_00', 8, 4], ['cashdesk_changer_0_50', 9, 4], ['cashdesk_changer_0_10', 10, 4], ['cashdesk_changer_0_05', 11, 4],
  ['cp_wischer_intervall_toggle', 17, 2], ['cp_wischer_wascher_button', 17, 4], ['cp_batterietrennschalter_toggle', 18, 0], ['bus_20h-switch', 70, 0],
  ['cashdesk_changer_5_00', 6, 4], ['cashdesk_changer_15_00', 6, 6], ['bus_doorfront5', 78, 0], ['taster_nebelschluss', 33, 4], ['cp_schalter_kinderwagen', 88, 0],
  ['kw_s_plus', 26, 0], ['kw_s_minus', 53, 0], ['change_take', 20, 2],
].map(([action, scan_code, modifier]) => ({ action, scan_code, modifier }));
const STOCK_SPLIT = STOCK.findIndex((b) => b.action === 'throttle');
/** The lists as OMSI 2 ships them (the game adds its own keys and the VR ones on reading). */
const stockBindings = () => ({ game: STOCK.slice(0, STOCK_SPLIT).map((b) => ({ ...b })), vehicles: STOCK.slice(STOCK_SPLIT).map((b) => ({ ...b })) });

/** The game's own actions, which the bus's section has no use for. */
const GAME_ONLY = /^(vr_|chat_|view_|door_|doors_all$|gear_(up|down)$|sim_pause$|open_menu$|screenshot$|photo_mode$|quicksave$|toggel_|exit$|voice_radio$)/;

const SECTIONS = [
  ['vehicles', 'Driving & the bus', 'The bus\'s own keys'],
  ['game', 'The game', 'Menus, views, pausing'],
];

const isVr = (a) => (a || '').startsWith('vr_');
const chordOf = (m) => (m || 0) & CHORD;

// --- the page -----------------------------------------------------------------------------

export function ControlsPage() {
  const s = useStore();
  const tab = s.controlsTab === 1 || s.controlsTab === 'pads' ? 'pads' : 'keyboard';
  const setTab = (v) => set({ controlsTab: v });
  return html`
    <${PageHead}
      title="Controls"
      text=${tab === 'keyboard' ? 'Click a key and press the new one (hold Shift, Ctrl or Alt for a combination); Escape leaves it as it is.' : 'Your wheel, pedals, gamepad or button box: set them up, give their buttons what they do and check that they answer.'}
      actions=${html`<${Segmented} value=${tab} onChange=${setTab} options=${[['keyboard', 'Keyboard'], ['pads', 'Game controllers']]} />`}
    />
    ${tab === 'keyboard' ? html`<${Keyboard} />` : html`<${Controllers} />`}
  `;
}

function Keyboard() {
  const s = useStore();
  const [kb, setKb] = useState(null);
  const [loadError, setLoadError] = useState('');
  const [saveError, setSaveError] = useState('');
  const [sec, setSec] = useState('vehicles');
  const [filters, setFilters] = useState({ vehicles: '', game: '' });
  // the entry waiting for its key: { sec, index } (an entry of the list) or { sec, insert,
  // action, modifier } (a new one, put in at `insert` once its key is pressed)
  const [capture, setCapture] = useState(null);
  const [picker, setPicker] = useState(false);
  const [ask, confirmNode] = useConfirm();

  const load = () => {
    setLoadError('');
    cached('keys', 'keybindings_get').then((v) => (v && !v.error ? setKb(v) : setLoadError(v?.error || 'no key bindings')));
  };
  useEffect(load, []);

  /** Keep the lists (the whole object, as `keybindings_get` gave it) and take back what the
   * file says now. A key changed is a key the player wants: a ready-made driving layout
   * would win over it, so the game is switched to the keys of this page. */
  async function persist(next, vr, keepLayout = false) {
    setKb(next);
    try {
      setKb(await call('keybindings_save', { bindings: next }));
      setSaveError('');
      const preset = S.settings?.drive_keys ?? 'simple';
      if (!vr && !keepLayout && S.settings && preset !== 'omsi') {
        await saveSettings({ drive_keys: 'omsi' });
        toast(t('Key bindings saved; Driving keys switched to Custom controls so the game uses them.'), 'ok');
      } else {
        toast(t('Key bindings saved.'), 'ok');
      }
    } catch (e) {
      setSaveError(e.message);
      toast(e.message, 'error');
    }
  }

  const edit = (key, f) => {
    const next = { ...kb, [key]: (kb[key] || []).map((b) => ({ ...b })) };
    f(next[key]);
    return next;
  };

  if (loadError) {
    return html`<${Notice} kind="danger"><div class="stack" style="gap:8px"><div>${loadError}</div><div><${Button} small icon="refresh" label="Try again" onClick=${load} /></div></div></${Notice}>`;
  }
  if (!kb) return html`<div class="row muted"><${Spinner} /> ${t('Reading the keys…')}</div>`;

  const list = kb[sec] || [];
  const filter = filters[sec];
  const q = filter.trim().toLowerCase();
  // the same key and combination twice in a section
  const counts = {};
  for (const b of list) if (b.scan_code) counts[`${b.scan_code}:${chordOf(b.modifier)}`] = (counts[`${b.scan_code}:${chordOf(b.modifier)}`] || 0) + 1;
  let shown = list
    .map((b, i) => ({ b, i, label: actionText(b.action), key: keyName(b.scan_code, b.modifier) }))
    .filter(({ b, label, key }) => !q || label.toLowerCase().includes(q) || b.action.toLowerCase().includes(q) || key.toLowerCase().includes(q));
  if (sec === 'game') shown = [...shown.filter((x) => x.label.startsWith('VR:')), ...shown.filter((x) => !x.label.startsWith('VR:'))];

  // a name the list does not have (a bus's own trigger a mod's readme gives a key): added as
  // a key of its own, as an [entry] added to keyboard.cfg by hand is
  const newAction = filter.trim();
  const canAdd = !shown.length && newAction.length > 1 && /^[A-Za-z0-9_]+$/.test(newAction);

  const preset = s.settings?.drive_keys ?? 'simple';
  const presetName = { wasd: 'W A S D only', arrows: 'Arrow keys only' }[preset] || 'W A S D + arrows';

  const clear = (i) => persist(edit(sec, (a) => Object.assign(a[i], { scan_code: 0, modifier: 0 })), isVr(list[i]?.action));
  const remove = (i) => persist(edit(sec, (a) => a.splice(i, 1)), isVr(list[i]?.action));

  /** The key pressed for the entry waiting (0 clears it). */
  function assign(scan, chord) {
    const c = capture;
    setCapture(null);
    if (c.index != null) {
      const b = kb[c.sec][c.index];
      // (the entry's held bit is the action's, not the key's: it stays)
      persist(edit(c.sec, (a) => Object.assign(a[c.index], { scan_code: scan, modifier: scan ? chord | ((b.modifier || 0) & KEY_HOLD) : 0 })), isVr(b.action));
    } else if (scan) {
      persist(edit(c.sec, (a) => a.splice(c.insert, 0, { action: c.action, scan_code: scan, modifier: chord | (c.modifier & KEY_HOLD) })), isVr(c.action));
    }
  }

  async function reset() {
    const ok = await ask({
      title: 'Reset to defaults',
      text: "Every key goes back to OMSI 2's own keyboard.cfg. Keys you added for a bus's own triggers are removed.",
      yes: 'Reset',
      danger: true,
    });
    if (ok) persist(stockBindings(), false, true);
  }

  const waiting = capture && (capture.index != null ? kb[capture.sec][capture.index] : capture);

  return html`<div class="stack">
    ${preset !== 'omsi' &&
    html`<${Notice} icon="keyboard">
      <div class="row wrap" style="justify-content:space-between">
        <div class="grow" style="min-width:240px">${t('Driving keys: {name} (Settings). Those keys drive the bus and win over the list below. Change any key here and your own layout (Custom controls) is used from then on.', { name: t(presetName) })}</div>
        <${Button} kind="primary" small icon="keyboard" label="Use these keys" onClick=${async () => {
          await saveSettings({ drive_keys: 'omsi' });
          toast(t('Driving keys: Custom controls - the game uses the keys of this page.'), 'ok');
        }} />
      </div>
    </${Notice}>`}
    ${saveError && html`<${Notice} kind="danger">${saveError}</${Notice}>`}

    <${Card} body=${false}>
      <div class="card-body stack" style="gap:14px">
        <div class="row wrap" style="justify-content:space-between;align-items:flex-end">
          <${Tabs} value=${sec} onChange=${setSec} options=${SECTIONS.map(([k, l]) => [k, l])} />
          <${Button} small kind="ghost" icon="refresh" label="Reset to defaults" onClick=${reset} />
        </div>
        <div class="muted small" style="margin-top:-10px">${t(SECTIONS.find((x) => x[0] === sec)[2])}</div>
        <div class="row">
          <div class="grow"><${Search} value=${filter} placeholder="Filter..." onInput=${(v) => setFilters({ ...filters, [sec]: v })} /></div>
          ${sec === 'vehicles' && html`<${Button} icon="plus" label="Add binding" onClick=${() => setPicker(true)} />`}
        </div>
        ${canAdd &&
        html`<div><${Button} icon="plus" onClick=${() => setCapture({ sec, insert: list.length, action: newAction, modifier: 0 })}>${t('Add "{name}" and give it a key', { name: newAction })}</${Button}></div>`}
        ${shown.length
          ? html`<table class="table">
              <thead><tr><th>${t('Action')}</th><th>${t('Key')}</th><th style="width:1%"></th></tr></thead>
              <tbody>
                ${shown.map(({ b, i, label }) => {
                  const clash = !!b.scan_code && counts[`${b.scan_code}:${chordOf(b.modifier)}`] > 1;
                  return html`<tr key=${sec + i}>
                    <td>
                      <div>${label}</div>
                      ${label !== b.action && html`<div class="muted small">${b.action}</div>`}
                    </td>
                    <td>
                      <div class="row" style="gap:8px">
                        <${Keys} scan=${b.scan_code} modifier=${b.modifier} />
                        ${clash && html`<span class="tag danger" title=${t('Another entry of this list has the same key.')}>${t('Same key')}</span>`}
                      </div>
                    </td>
                    <td>
                      <div class="row" style="gap:4px;justify-content:flex-end;flex-wrap:nowrap">
                        <${Button} small label="Edit" onClick=${() => setCapture({ sec, index: i })} />
                        <${Button} small kind="ghost" icon="plus" title="Add another key for this action" onClick=${() => setCapture({ sec, insert: i + 1, action: b.action, modifier: b.modifier || 0 })} />
                        <${Button} small kind="ghost" icon="trash" title="Clear the key (the entry stays)" disabled=${!b.scan_code} onClick=${() => clear(i)} />
                        <${Button} small kind="ghost" icon="x" title="Remove the entry" onClick=${() => remove(i)} />
                      </div>
                    </td>
                  </tr>`;
                })}
              </tbody>
            </table>`
          : !canAdd && html`<${Empty} icon="search" title="No matching actions." />`}
      </div>
    </${Card}>

    ${capture &&
    html`<${CaptureDialog}
      label=${actionText(waiting?.action || '')}
      action=${waiting?.action || ''}
      current=${capture.index != null ? waiting : null}
      list=${kb[capture.sec] || []}
      skip=${capture.index}
      onKey=${assign}
      onClose=${() => setCapture(null)}
    />`}
    ${picker &&
    html`<${ActionPicker}
      bindings=${kb}
      onClose=${() => setPicker(false)}
      onPick=${(action) => {
        setPicker(false);
        setFilters({ ...filters, vehicles: action });
        setCapture({ sec: 'vehicles', insert: (kb.vehicles || []).length, action, modifier: 0 });
      }}
    />`}
    ${confirmNode}
  </div>`;
}

/** "Press a key": the key pressed (with Shift, Ctrl, Alt held) is the entry's; Escape leaves
 * it as it is, Backspace or Delete clears it. A key another entry has is said first. */
function CaptureDialog({ label, action, current, list, skip, onKey, onClose }) {
  const [pending, setPending] = useState(null);
  useEffect(() => {
    const down = (e) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.repeat) return;
      if (e.code === 'Escape') return onClose();
      if (e.code === 'Backspace' || e.code === 'Delete') {
        if (current) onKey(0, 0);
        else onClose();
        return;
      }
      if (MODIFIER_CODES.has(e.code)) return;
      const scan = SCAN_OF[e.code];
      if (!scan) {
        toast(t('{code} has no DirectInput scan code the game understands.', { code: e.code || e.key }), 'error');
        return;
      }
      const chord = (e.shiftKey ? KEY_SHIFT : 0) | (e.ctrlKey ? KEY_CTRL : 0) | (e.altKey ? KEY_ALT : 0);
      const taken = list.filter((b, i) => i !== skip && b.scan_code === scan && chordOf(b.modifier) === chord && b.action !== action);
      if (taken.length) setPending({ scan, chord, taken });
      else onKey(scan, chord);
    };
    // (before the dialog's own Escape, and before any button the key would press)
    window.addEventListener('keydown', down, true);
    return () => window.removeEventListener('keydown', down, true);
  }, [list, skip, action, current, onKey, onClose]);

  return html`<${Dialog}
    title=${label}
    onClose=${onClose}
    footer=${html`
      <${Button} label="Cancel" onClick=${onClose} />
      ${pending && html`<${Button} kind="primary" label="Use it anyway" onClick=${() => onKey(pending.scan, pending.chord)} />`}
    `}
  >
    <div class="stack" style="align-items:center;text-align:center;gap:14px;padding:8px 0">
      ${label !== action && html`<div class="muted small">${action}</div>`}
      ${pending
        ? html`<div style="font-size:16px"><${Keys} scan=${pending.scan} modifier=${pending.chord} /></div>`
        : html`<div class="kbd" style="height:40px;padding:0 18px;font-size:14px">${t('press a key...')}</div>`}
      ${current && !pending && html`<div class="row muted small" style="gap:6px">${t('Now:')} <${Keys} scan=${current.scan_code} modifier=${current.modifier} /></div>`}
      ${pending &&
      html`<div style="width:100%;text-align:left"><${Notice} kind="warn">
        ${t('This key is already used by: {actions}', { actions: pending.taken.map((b) => actionText(b.action)).join(', ') })}
        <div class="muted small" style="margin-top:4px">${t('Both will react to it. Press another key, or use it anyway.')}</div>
      </${Notice}></div>`}
      <div class="muted small">${t('Hold Shift, Ctrl or Alt for a combination. Escape cancels; Backspace or Delete clears the key.')}</div>
    </div>
  </${Dialog}>`;
}

/** "Add binding": every action this page knows for the bus's section - OMSI 2's own, the
 * ones the list has and the ones the launcher has a name for. */
function ActionPicker({ bindings, onClose, onPick }) {
  const [q, setQ] = useState('');
  const seen = new Map();
  const add = (a) => {
    if (a && !GAME_ONLY.test(a) && !seen.has(a.toLowerCase())) seen.set(a.toLowerCase(), a);
  };
  STOCK.slice(STOCK_SPLIT).forEach((b) => add(b.action));
  (bindings.vehicles || []).forEach((b) => add(b.action));
  Object.keys(KNOWN).forEach(add);
  for (const g of ['R', '1', '2', '3', '4', '5', '6', '7', '8', '9', '10']) add(`kw_s_${g}_fest`);
  const query = q.trim().toLowerCase();
  const options = [...seen.values()]
    .map((action) => ({ action, label: actionText(action) }))
    .filter((o) => !query || o.action.toLowerCase().includes(query) || o.label.toLowerCase().includes(query))
    .sort((a, b) => a.label.toLowerCase().localeCompare(b.label.toLowerCase()) || a.action.toLowerCase().localeCompare(b.action.toLowerCase()));
  return html`<${Dialog} title="Add a key binding" onClose=${onClose} footer=${html`<${Button} label="Cancel" onClick=${onClose} />`}>
    <div class="stack" style="gap:12px">
      <div class="muted small">${t('Driving & the bus')}</div>
      <${Search} value=${q} onInput=${setQ} placeholder="Search actions..." autofocus />
      <div class="muted small">${t('{n} results', { n: options.length })}</div>
      <div class="list" style="max-height:420px;overflow:auto">
        ${options.length
          ? options.map(
              (o) => html`<div class="list-item" onClick=${() => onPick(o.action)}>
                <div class="grow"><div class="title">${o.label}</div><div class="sub">${o.action}</div></div>
              </div>`,
            )
          : html`<div class="muted" style="padding:8px">${t('No matching actions.')}</div>`}
      </div>
      <div class="muted small">${t("The installed buses' own script triggers are not listed here: type the trigger's name in the list's filter to add it.")}</div>
    </div>
  </${Dialog}>`;
}
