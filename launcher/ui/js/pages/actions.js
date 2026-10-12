import { t } from '../i18n.js';

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
  automatic_N: 'Gear N',
  automatic_R: 'Gear R',
  bus_doorfront0: 'Front door (leaf 1)',
  bus_doorfront1: 'Front door (leaf 2)',
  bus_dooraft: 'Release rear doors',
  door_1: 'Door 1 (front), any bus',
  door_2: 'Door 2, any bus',
  door_3: 'Door 3, any bus',
  door_4: 'Door 4, any bus',
  doors_all: 'All doors, any bus',
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
  chat_open: 'Multiplayer: write in the chat',
  chat_toggle: 'Multiplayer: show / hide the chat',
  voice_radio: 'Multiplayer: bus radio (hold)',
  sim_pause: 'Pause',
  open_menu: 'Open / close the main menu',
  screenshot: 'Screenshot',
  photo_mode: 'Photo mode',
  quicksave: 'Quicksave',
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

export function actionText(a) {
  const gear = /^kw_s_(.+)_fest$/.exec(a);
  if (gear) return t('Gear {gear} (H-pattern)', { gear: gear[1] });
  return KNOWN[a] ? t(KNOWN[a]) : humanize(a);
}

export const H_GEARS = ['R', '1', '2', '3', '4', '5', '6'].map((g) => `kw_s_${g}_fest`);

export const PAD_ACTION_GROUPS = [
  ['Driving', ['horn', 'parking_brake_toggle', 'kw_m_enginestart', 'cp_batterietrennschalter_toggle', 'throttle_amplify']],
  ['Gearbox', ['gear_up', 'gear_down', 'automatic_D', 'automatic_N', 'automatic_R', ...H_GEARS]],
  ['Lights and indicators', ['blinker_left_set', 'blinker_right_set', 'blinker_off', 'blinker_left_toggle', 'blinker_right_toggle', 'blinker_warn_toggle', 'kw_scheinwerfer_toggle', 'kw_fernlicht_toggle', 'kw_wipermode_up']],
  ['Doors and tickets', ['doors_all', 'door_1', 'door_2', 'door_3', 'door_4', 'bus_doorfront0', 'bus_doorfront1', 'bus_dooraft', 'ticket_give']],
  ['Views', ['view_look_left', 'view_look_right', 'view_look_up', 'view_look_down', 'view_reset_direction', 'view_reset_all_directions', 'view_toggle_viewpoint', 'view_toggle_interior', 'view_set_driver', 'view_set_passenger', 'view_set_outside', 'view_interiorcam_plus', 'view_interiorcam_minus']],
  ['The game', ['open_menu', 'sim_pause', 'screenshot', 'quicksave', 'voice_radio', 'toggel_ctrler', 'toggel_mouse_ctrl']],
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
