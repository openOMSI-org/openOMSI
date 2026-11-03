import { useState, useEffect, useRef } from '../../vendor/preact-htm.js';
import { t, language } from '../i18n.js';
import { call } from '../api.js';

export const KNOWN = {
  throttle: 'Throttle',
  brake: 'Brake',
  throttle_amplify: 'Throttle (full, kickdown)',
  clutch: 'Clutch',
  steering_left: 'Steer left',
  steering_right: 'Steer right',
  steering_neutral: 'Steering to centre',
  parking_brake_toggle: 'Parking brake',
  blinker_left_set: 'Indicator left',
  blinker_right_set: 'Indicator right',
  blinker_left_toggle: 'Indicator left (toggle)',
  blinker_right_toggle: 'Indicator right (toggle)',
  blinker_off: 'Indicators off',
  blinker_warn_toggle: 'Hazard lights',
  gear_up: 'Gear up (manual gearbox)',
  gear_down: 'Gear down (manual gearbox)',
  horn: 'Horn',
  kw_scheinwerfer_toggle: 'Headlights',
  kw_standlicht_toggle: 'Sidelights',
  kw_fernlicht_toggle: 'High beam',
  kw_m_enginestart: 'Starter',
  kw_wipermode_up: 'Wipers (next mode)',
  cp_batterietrennschalter_toggle: 'Battery / ignition',
  automatic_D: 'Gear D',
  // (OMSI 2 has R and N twice, for each gearbox: said which, #1691)
  automatic_N: 'Gear N (automatic gearbox)',
  automatic_R: 'Gear R (automatic gearbox)',
  kw_s_N: 'Gear N (manual gearbox)',
  kw_s_R: 'Gear R (manual gearbox)',
  bus_doorfront0: 'Front door (leaf 1)',
  bus_doorfront1: 'Front door (leaf 2)',
  bus_dooraft: 'Release rear doors',
  door_1: 'Door 1 (front), any bus',
  door_2: 'Door 2, any bus',
  door_3: 'Door 3, any bus',
  door_4: 'Door 4, any bus',
  doors_all: 'All doors, any bus',
  doors_release: 'Door release (passengers open), any bus',
  ticket_give: 'Sell the requested ticket',
  view_set_driver: "Driver's view",
  view_set_passenger: 'Passenger view',
  view_set_outside: 'Outside view',
  view_toggle_viewpoint: 'Next view',
  view_toggle_interior: 'Cabin and outside, one key',
  view_interiorcam_plus: 'Next interior camera',
  view_interiorcam_minus: 'Previous interior camera',
  view_reset_direction: 'Reset the view direction',
  view_reset_all_directions: 'Reset all view directions',
  head_tracking_recenter: 'Head tracking: centre again',
  view_look_left: 'Look left (hold)',
  view_look_right: 'Look right (hold)',
  view_look_up: 'Look up (hold)',
  view_look_down: 'Look down (hold)',
  vr_recenter: 'VR: Reset view',
  vr_toggle_desktop_mirror: 'VR: Monitor preview',
  vr_toggle_mode: 'VR: Switch VR / desktop',
  vr_toggle_navigator: 'VR: Toggle navigator',
  vr_position_navigator: 'VR: Position navigator',
  exit: 'Quit',
  pax_door_request: 'Passenger: request entry / exit',
  chat_open: 'Multiplayer: write in the chat',
  chat_toggle: 'Multiplayer: show / hide the chat',
  voice_nearby: 'Multiplayer voice: nearby players (hold)',
  voice_radio: 'Multiplayer voice: radio, whole session (hold)',
  sim_pause: 'Pause',
  open_menu: 'Open / close the main menu',
  open_menue: 'Open / close the quick menu (OMSI: Alt)',
  view_zoom_in: 'Zoom in',
  view_zoom_out: 'Zoom out',
  view_zoom_hold: 'Zoom while held',
  view_camera_keys_toggle: 'Free camera keys on/off (off: they work the bus)',
  radio_power: 'Radio: on / off',
  radio_next: 'Radio: next station',
  radio_previous: 'Radio: previous station',
  radio_volume_up: 'Radio: louder',
  radio_volume_down: 'Radio: quieter',
  screenshot: 'Screenshot',
  photo_mode: 'Photo mode',
  quicksave: 'Quicksave',
  clean_screen: 'Hide / show the whole interface',
  get_up: 'Get up from the seat (on foot)',
  toggel_mouse_ctrl: 'Toggle mouse steering',
  toggel_ctrler: 'Toggle game controllers',
};

function humanize(name) {
  const words = name
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .replace(/(\d)(\D)|(\D)(\d)/g, (m, a, b, c, d) => (a ? `${a} ${b}` : `${c} ${d}`))
    .split(/[_\-\s./\\]+/)
    .filter((w) => w && !['cp', 'kw', 'bus', 'toggle', 'mouse', 'lod'].includes(w.toLowerCase()))
    .map((w) => (w === w.toUpperCase() ? w : w.toLowerCase()));
  const s = words.join(' ') || name;
  return s ? s[0].toUpperCase() + s.slice(1) : s;
}

const names = { lang: '', labels: new Map(), asked: new Set(), version: 0 };
const listeners = new Set();

export const actionNamesVersion = () => names.version;

function namesNow() {
  if (names.lang !== language()) Object.assign(names, { lang: language(), labels: new Map(), asked: new Set(), version: names.version + 1 });
  return names;
}

export function addActionNames(labels, lang = language()) {
  const n = namesNow();
  if (n.lang !== lang) return;
  for (const [a, label] of Object.entries(labels || {})) if (label) n.labels.set(a.toLowerCase(), label);
  n.version++;
  listeners.forEach((f) => f());
}

export function useActionNames(actions) {
  const [, tick] = useState(0);
  useEffect(() => {
    const f = () => tick((x) => x + 1);
    listeners.add(f);
    return () => listeners.delete(f);
  }, []);
  const lang = language();
  const key = [...new Set((actions || []).filter((a) => a && !KNOWN[a]))].sort().join('\n');
  useEffect(() => {
    const n = namesNow();
    const want = key ? key.split('\n').filter((a) => !n.asked.has(a.toLowerCase())) : [];
    if (!want.length) return;
    want.forEach((a) => n.asked.add(a.toLowerCase()));
    call('keyboard_actions', { lang, actions: want })
      .then((r) => addActionNames(r.labels, lang))
      .catch(() => names.lang === lang && want.forEach((a) => names.asked.delete(a.toLowerCase())));
  }, [key, lang]);
}

export function useKeyCatalog(vehicles, playing) {
  const [cat, setCat] = useState(null);
  const [scan, setScan] = useState(null);
  const [failed, setFailed] = useState(false);
  const wanted = useRef(vehicles);
  wanted.current = vehicles;
  useEffect(() => {
    let alive = true;
    let timer = 0;
    let last = null;
    let misses = 0;
    const lang = language();
    setFailed(false);
    const ask = () =>
      call('keyboard_actions', { lang, vehicles: wanted.current, catalog: true, scan: !playing, have: last?.stamp || '' })
        .then((r) => {
          if (!alive) return;
          misses = 0;
          if (r.options) {
            last = r;
            addActionNames(Object.fromEntries(r.options.map((o) => [o.action, o.label])), lang);
            setCat(r);
          }
          setScan(r.scan);
          if (!r.scan?.complete && !playing) timer = setTimeout(ask, 1000);
        })
        .catch(() => {
          if (!alive) return;
          if (!last) return setFailed(true);
          if (++misses < 5) timer = setTimeout(ask, 3000 * misses);
        });
    ask();
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [playing]);
  return [cat, failed, scan];
}

export function actionText(a) {
  const gear = /^kw_s_(.+)_fest$/.exec(a);
  if (gear) return t('Gear {gear} (H-pattern)', { gear: gear[1] });
  if (KNOWN[a]) return t(KNOWN[a]);
  return namesNow().labels.get((a || '').toLowerCase()) || humanize(a);
}

export const H_GEARS = ['R', '1', '2', '3', '4', '5', '6', '7', '8', '9', '10'].map((g) => `kw_s_${g}_fest`);

export const PAD_ACTION_GROUPS = [
  ['Driving', ['horn', 'parking_brake_toggle', 'kw_m_enginestart', 'cp_batterietrennschalter_toggle', 'throttle_amplify']],
  ['Gearbox', ['gear_up', 'gear_down', 'automatic_D', 'automatic_N', 'automatic_R', ...H_GEARS]],
  ['Lights and indicators', ['blinker_left_set', 'blinker_right_set', 'blinker_off', 'blinker_left_toggle', 'blinker_right_toggle', 'blinker_warn_toggle', 'kw_scheinwerfer_toggle', 'kw_fernlicht_toggle', 'kw_wipermode_up']],
  ['Doors and tickets', ['doors_all', 'door_1', 'door_2', 'door_3', 'door_4', 'doors_release', 'bus_doorfront0', 'bus_doorfront1', 'bus_dooraft', 'ticket_give', 'pax_door_request']],
  ['Views', ['view_look_left', 'view_look_right', 'view_look_up', 'view_look_down', 'view_reset_direction', 'view_reset_all_directions', 'view_toggle_viewpoint', 'view_toggle_interior', 'view_set_driver', 'view_set_passenger', 'view_set_outside', 'view_interiorcam_plus', 'view_interiorcam_minus', 'view_zoom_in', 'view_zoom_out', 'view_zoom_hold', 'view_camera_keys_toggle', 'head_tracking_recenter']],
  ['The game', ['open_menu', 'sim_pause', 'screenshot', 'quicksave', 'voice_nearby', 'voice_radio', 'toggel_ctrler', 'toggel_mouse_ctrl', 'radio_power', 'radio_next', 'radio_previous', 'radio_volume_up', 'radio_volume_down', 'clean_screen', 'get_up']],
];

export function padActions(bindings, devices) {
  const seen = new Map();
  const add = (a) => {
    if (a && !seen.has(a.toLowerCase())) seen.set(a.toLowerCase(), a);
  };
  PAD_ACTION_GROUPS.forEach(([, list]) => list.forEach(add));
  (bindings?.vehicles || []).forEach((b) => add(b.action));
  (devices || []).forEach((d) => d.buttons.forEach(([a]) => add(a)));
  return [...seen.values()];
}
