// What the Settings page shows: per tab its two columns, per column its rows, in the order
// the game's own launcher window had them (it is gone). The English labels are the
// translation keys, so they stay word for word as they were.
//
// A row is one of:
//   { type: 'select', key, label, options, raw?, help?, recheckUpdates? }   options: [value, label] pairs, or (s, boot) => pairs
//   { type: 'toggle', key, label, help? }
//   { type: 'slider', key, label, min, max, step, def, format, store?, also? }
//   { type: 'section', title }        { type: 'note', text }
//   { type: 'button', label, icon, kind?, patch?: (s) => patch, controls?: tab }
//   { type: 'custom', id }            drawn by the page (presets, profiles, language ...)
// and any of them may carry `when: (s, platform) => bool`.
// A slider's `key` may be a function of the settings; `store` rounds the value as the game
// expects it; `also` is saved alongside.

import { t } from '../i18n.js';

// --- value formats ---------------------------------------------------------------------

const pct = (v) => `${(v * 100).toFixed(0)}%`;
const deg = (v) => `${v.toFixed(0)}°`;
const mm = (v) => `${v.toFixed(0)} mm`;
const signed = (n, unit) => {
  const r = Math.round(n);
  return `${r >= 0 ? '+' : ''}${r}${unit}`;
};
/** 100 % reads "OMSI": the game's own response. */
const omsiPct = (v) => (Math.abs(v - 1) < 0.01 ? t('OMSI') : pct(v));
const offMs = (v) => (v <= 0 ? t('Off') : `${v.toFixed(0)} ms`);
const offPct100 = (v) => (v <= 0 ? t('Off') : `${v.toFixed(0)}%`);

// --- how a value is kept ----------------------------------------------------------------

const r2 = (v) => Math.round(v * 100) / 100;
const r1 = (v) => Math.round(v * 10) / 10;
const round = (v) => Math.round(v);
/** To the nearest `step`, without the float's tail (0.30000000000000004). */
const snap = (step) => (v) => Number((Math.round(v / step) * step).toFixed(4));

// --- conditions -------------------------------------------------------------------------

const mode = (s) => s.graphics || 'vanilla_plus';
const classic = (s) => mode(s) === 'vanilla';
const traced = (s) => mode(s) === 'enhanced_plus';
const enhanced = (s) => mode(s) === 'enhanced' || traced(s);
const mobile = (s, p) => p === 'android' || p === 'ios';
const windows = (s, p) => p === 'windows';
const tripleOn = (s) => !!s.triple_screen;
const vrOn = (s, p) => windows(s, p) && !!s.vr;

/** The window sizes the settings offer (`resolution`). */
export const RESOLUTIONS = [
  ['auto', 'Automatic'], ['1280x720', '1280 x 720'], ['1280x800', '1280 x 800 (Steam Deck)'], ['1366x768', '1366 x 768'],
  ['1600x900', '1600 x 900'], ['1920x1080', '1920 x 1080'], ['1920x1200', '1920 x 1200'], ['2560x1440', '2560 x 1440'], ['3840x2160', '3840 x 2160'],
];

/** The video memory's choices: all the game holds on the graphics card (textures, meshes,
 * render targets). "Automatic" is nine tenths of the card's own memory and the driver's
 * budget, which the launcher does not know. */
const videoMemory = [
  ['0', 'Automatic'],
  ['1000', '1 GB'], ['1500', '1.5 GB'], ['2000', '2 GB'], ['3000', '3 GB'], ['4000', '4 GB'], ['6000', '6 GB'], ['8000', '8 GB'], ['12000', '12 GB'], ['16000', '16 GB'],
];

/** The pedals' response: softer below 1, stronger above. */
const pedal = (v) => (Math.abs(v - 1) < 0.01 ? t('Normal') : v < 1 ? `${t('Softer')} x${v.toFixed(2)}` : `${t('Stronger')} x${v.toFixed(2)}`);
const vibration = (v) => (v < 0.01 ? t('Off') : Math.abs(v - 1) < 0.01 ? t('Normal') : pct(v));

const headTrackingKeys = ['head_tracking_yaw_sens', 'head_tracking_pitch_sens', 'head_tracking_roll_sens', 'head_tracking_x_sens', 'head_tracking_y_sens', 'head_tracking_z_sens'];
const headTrackingInverts = ['head_tracking_invert_yaw', 'head_tracking_invert_pitch', 'head_tracking_invert_roll', 'head_tracking_invert_x', 'head_tracking_invert_y', 'head_tracking_invert_z'];
const trackSlider = (key, label) => ({ type: 'slider', key, label, min: 0, max: 100, step: 1, def: 100, format: offPct100, store: (v) => Math.min(100, Math.max(0, Math.round(v))) });

// --- the tabs ---------------------------------------------------------------------------

const graphics = {
  id: 'graphics',
  label: 'Graphics',
  cols: [
    {
      title: 'Graphics',
      rows: [
        // the mode first, then the quality presets for it: they set most of what follows
        { type: 'select', key: 'graphics', label: 'Graphics', options: [['vanilla', 'Vanilla (as OMSI 2)'], ['vanilla_plus', 'Vanilla+'], ['enhanced', 'Enhanced'], ['enhanced_plus', 'Enhanced+']] },
        { type: 'custom', id: 'preset' },
        { type: 'custom', id: 'optimise' },
        { type: 'select', key: 'msaa', label: 'Anti-aliasing', options: [['1', 'Off'], ['2', '2x MSAA'], ['4', '4x MSAA'], ['8', '8x MSAA']], heavy: (v, s) => v === '8' && s.graphics !== 'enhanced_plus' },
        { type: 'select', key: 'render_scale', label: 'Render scale', options: [['auto', 'Auto'], ['1', '100%'], ['0.85', '85%'], ['0.75', '75%'], ['0.67', '67%'], ['0.5', '50%']] },
        { type: 'select', key: 'anisotropy', label: 'Anisotropic', options: [['1', 'Off'], ['2', '2x'], ['4', '4x'], ['8', '8x'], ['16', '16x']] },
        // Vanilla draws what OMSI 2 draws: no sun shadows, ambient occlusion or detail grain
        { type: 'select', key: 'shadow_size', label: 'Shadow map', options: [['1024', '1024'], ['2048', '2048'], ['4096', '4096']], when: (s) => !classic(s) },
        // (Enhanced+ traces its shadows, occlusion and reflections: always on there)
        { type: 'toggle', key: 'ssao', label: 'Ambient occlusion', when: (s) => !classic(s) && !traced(s) },
        { type: 'toggle', key: 'shadows', label: 'Sun shadows', when: (s) => !classic(s) && !traced(s) },
        { type: 'select', key: 'shadow_casters', label: 'Shadows cast by', options: [['all', 'Every solid mesh'], ['omsi', '[shadow] meshes, as OMSI']], when: (s) => !classic(s) },
        { type: 'toggle', key: 'detail_textures', label: 'Detail texturing up close', when: (s) => !classic(s) },
        { type: 'slider', key: 'night_brightness', label: 'Night brightness', min: 0, max: 3, step: 0.25, def: 0, format: (v) => (v < 0.01 ? t('Off') : `+${v.toFixed(2)}`), store: snap(0.25), when: enhanced },
        { type: 'slider', key: 'led_glow', label: 'LED glow', min: 0, max: 15, step: 1, def: 6, format: (v) => (v < 0.5 ? t('Off') : String(Math.trunc(v))), store: round, when: enhanced },
        { type: 'slider', key: 'led_mips', label: 'LED mip strength', min: 0, max: 4, step: 0.05, def: 1.3, format: (v) => (v < 0.005 ? t('Off') : v.toFixed(2)), store: snap(0.05), when: enhanced },
        // (the `[isshadow]` blob under a vehicle is drawn in every mode, the vanilla one included)
        { type: 'toggle', key: 'shadow_blobs', label: "OMSI's shadow meshes (under vehicles)" },
        { type: 'slider', key: 'reflections', label: 'Reflection maps (paint, chrome, glass)', min: 0, max: 1, step: 0.05, def: 1, format: (v) => (v < 0.005 ? t('Off') : pct(v)), store: snap(0.05), when: (s) => !traced(s) },
        { type: 'toggle', key: 'clouds', label: 'Clouds' },
        // (only the enhanced modes march volumetric clouds; the vanilla ones are a texture)
        { type: 'select', key: 'cloud_quality', label: 'Cloud quality', options: [['high', 'High'], ['low', 'Low']], when: (s) => enhanced(s) && s.clouds !== false },
        { type: 'select', key: 'rain_quality', label: 'Rain quality', options: [['high', 'High'], ['medium', 'Medium'], ['low', 'Low']] },
        // (the wet roads' reflections need the reflection maps on)
        { type: 'select', key: 'puddles', label: 'Puddles', options: [['high', 'High'], ['low', 'Low'], ['off', 'Off']], when: (s) => Number(s.reflections ?? 1) > 0.005 },
        { type: 'toggle', key: 'windy_trees', label: 'Windy trees' },
      ],
    },
    {
      title: 'Display',
      rows: [
        { type: 'toggle', key: 'fullscreen', label: 'Fullscreen' },
        { type: 'select', key: 'resolution', label: 'Window size', options: RESOLUTIONS },
        { type: 'toggle', key: 'vsync', label: 'V-sync' },
        { type: 'toggle', key: 'sim_thread', label: 'Simulation on its own thread' },
        { type: 'select', key: 'max_fps', label: 'Frame limit', options: [['0', 'Screen refresh rate'], ['30', '30 fps'], ['45', '45 fps'], ['60', '60 fps'], ['120', '120 fps'], ['144', '144 fps'], ['1000', 'Unlimited']] },
        // (a Mac has Metal only; ANGLE is Windows' path for a chip with none of the others)
        { type: 'select', key: 'graphics_api', label: 'Graphics API', options: [['auto', 'Automatic'], ['vulkan', 'Vulkan'], ['dx12', 'DirectX 12'], ['gl', 'OpenGL'], ['angle', 'ANGLE (DirectX 11)']], when: windows },
        { type: 'select', key: 'graphics_api', label: 'Graphics API', options: [['auto', 'Automatic'], ['vulkan', 'Vulkan'], ['gl', 'OpenGL']], when: (s, p) => p !== 'windows' && p !== 'macos' },
        { type: 'section', title: 'World & memory' },
        { type: 'select', key: 'view_distance', label: 'View distance', options: [['auto', 'Default (1200 m)'], ['600', '600 m - fastest'], ['900', '900 m'], ['1200', '1200 m'], ['1500', '1500 m'], ['2000', '2000 m'], ['2500', '2500 m']], heavy: (v) => Number(v) >= 2000 },
        { type: 'select', key: 'max_obj_dist', label: 'Object distance', options: [['auto', 'Automatic'], ['500', '500 m'], ['750', '750 m'], ['900', '900 m'], ['1500', '1500 m'], ['3000', '3000 m']], heavy: (v) => Number(v) >= 1500 },
        { type: 'select', key: 'min_obj_size', label: 'Small objects', options: [['0.005', 'All'], ['0.013', 'Normal'], ['0.02', 'Fewer (faster)'], ['0.03', 'Few (fastest)']] },
        { type: 'select', key: 'mirror_size', label: 'Mirrors', options: [['0', 'Off'], ['128', 'Low (128)'], ['256', 'Normal (256)'], ['512', 'High (512)'], ['1024', 'Very high (1024)']] },
        { type: 'select', key: 'mirror_refresh', label: 'Bus mirror refresh', options: [['off', 'None (frozen picture)'], ['eco', 'Economical'], ['full', 'Full'], ['every', 'Every frame (smoothest)']], heavy: (v, s) => v === 'every' && String(s.mirror_size) !== '0' },
        { type: 'select', key: 'mirror_quality', label: 'Mirror quality', options: [['standard', 'Standard'], ['enhanced', 'As the picture (Enhanced)']], when: enhanced },
        { type: 'toggle', key: 'mirror_hud_every_frame', label: 'Mirror panels every frame' },
        { type: 'select', key: 'video_memory', label: 'Video memory', options: videoMemory, help: 'Everything the game keeps on the graphics card: textures, models and the picture itself. Automatic is nine tenths of the card.' },
        { type: 'toggle', key: 'texture_compression', label: 'Compress textures on loading' },
        // (off: block formats decoded on loading, for a driver that mishandles them)
        { type: 'toggle', key: 'gpu_texture_compression', label: 'DXT/BC textures stay compressed on the GPU' },
        { type: 'section', title: 'Profiles' },
        { type: 'custom', id: 'profiles' },
      ],
    },
  ],
};

const driving = {
  id: 'driving',
  label: 'Driving',
  cols: [
    {
      title: 'Keyboard & mouse',
      rows: [
        { type: 'section', title: 'Touch controls', when: mobile },
        { type: 'select', key: 'touch_door_mode', label: 'Door buttons', options: [['standard', 'Standard (physical doors)'], ['direct', 'Direct OMSI triggers']], when: mobile },
        { type: 'slider', key: 'touch_pedal_length', label: 'Pedal length', min: 0.8, max: 1.6, step: 0.05, def: 1, format: pct, store: r2, when: mobile },
        { type: 'slider', key: 'touch_pedal_deadzone', label: 'Pedal rest zone (a finger at the foot presses nothing)', min: 0, max: 0.4, step: 0.01, def: 0.15, format: pct, store: r2, when: mobile },
        { type: 'section', title: 'Keyboard & mouse', when: mobile },
        { type: 'select', key: 'drive_keys', label: 'Driving keys', options: [['omsi', 'Custom controls (Controls page)'], ['simple', 'W A S D + arrows'], ['wasd', 'W A S D only'], ['arrows', 'Arrow keys only']] },
        { type: 'toggle', key: 'steering_linear', label: "Steering linearity (keys at OMSI's steady pace)" },
        { type: 'toggle', key: 'old_steering', label: 'Old Steering (the wheel stays, turn it back yourself)' },
        { type: 'toggle', key: 'red_steer_spd', label: "Dynamic steering (slower keys at speed, OMSI's redSteerSpd)" },
        { type: 'slider', key: 'mouse_sens', label: 'Mouse steering sensitivity (O)', min: 0.1, max: 3, step: 0.05, def: 1, format: omsiPct, store: r2 },
        { type: 'slider', key: 'mouse_pedal_strength', label: 'Mouse pedal strength', min: 0.5, max: 2, step: 0.05, def: 1, format: omsiPct, store: r2 },
        { type: 'toggle', key: 'mouse_smooth', label: 'Smooth mouse steering (off: the wheel follows the cursor at once, as in OMSI)' },
        { type: 'toggle', key: 'mouse_pedals', label: 'The mouse works the pedals', help: 'Up the throttle, down the brake, as in OMSI; off: the mouse only steers, the pedals stay on the keys' },
        { type: 'toggle', key: 'mouse_hold', label: "Hold the cursor while the mouse steers (off: the crosshair stays free, the window's edges are the lock)" },
        { type: 'toggle', key: 'mouse_right_off', label: 'A right click ends the mouse steering (as in OMSI)' },
        { type: 'toggle', key: 'blinker_cancel', label: "Indicators cancel themselves (as the bus's script does)" },
        { type: 'toggle', key: 'brake_hold', label: 'The keyboard brake stays on until the throttle (as in OMSI)' },
        { type: 'slider', key: 'key_pedal_speed', label: 'Pedal keys speed (100 % = OMSI)', min: 0.25, max: 2, step: 0.05, def: 1, format: omsiPct, store: r2 },
        { type: 'toggle', key: 'auto_clutch', label: 'Automatic clutch (manual gearboxes)' },
        { type: 'toggle', key: 'auto_shift', label: 'Automated manual gearbox (shifts a manual gearbox for you by the engine speed)' },
        { type: 'button', label: 'Change the keys', icon: 'keyboard', controls: 0 },
      ],
    },
    {
      title: 'Game controllers',
      rows: [
        { type: 'button', label: 'Set up a wheel or pedals', icon: 'gamepad', controls: 1 },
        { type: 'section', title: 'Wheel and pedals' },
        // the wheel's own rotation, and how much of it is the bus's full lock (a real bus:
        // about two and a half turns)
        { type: 'slider', key: 'wheel_range', label: 'Wheel rotation', min: 180, max: 1800, step: 30, def: 900, format: deg, store: round },
        { type: 'slider', key: 'wheel_lock', label: 'Full lock at', min: 0, max: 1800, step: 30, def: 0, format: (v) => (v < 45 ? t('OMSI') : deg(v)), store: (v) => (v < 45 ? 0 : Math.round(v)) },
        { type: 'slider', key: 'pedal_throttle', label: 'Throttle pedal strength', min: 0.5, max: 2, step: 0.05, def: 1, format: pedal, store: r2 },
        { type: 'slider', key: 'pedal_brake', label: 'Brake pedal strength', min: 0.5, max: 2, step: 0.05, def: 1, format: pedal, store: r2 },
        { type: 'toggle', key: 'momentary_gears', label: 'H-pattern shifter: return to neutral when the gear is released' },
        { type: 'toggle', key: 'arrows_switch_cams', label: 'Arrow keys switch the cameras with a wheel too (no glance)' },
        { type: 'toggle', key: 'ctrl_background', label: 'Keep driving in the background' },
        { type: 'section', title: 'Gamepad' },
        { type: 'select', key: 'pad_type', label: 'Gamepad type', options: [['auto', 'Automatic'], ['xbox', 'Xbox'], ['ps4', 'PlayStation 4'], ['ps5', 'PlayStation 5']] },
        { type: 'toggle', key: 'pad_buttons', label: 'Default gamepad buttons (indicators, doors, views, gears, menu, pause)' },
        { type: 'slider', key: 'pad_steer_speed', label: 'Stick steering speed', min: 0.8, max: 5, step: 0.1, def: 2, format: (v) => `${v.toFixed(1)} s`, store: r1 },
        { type: 'slider', key: 'pad_steer_smooth', label: 'Stick steering smoothing', min: 0, max: 300, step: 10, def: 120, format: offMs, store: round },
        { type: 'slider', key: 'pad_deadzone', label: 'Stick dead zone', min: 0, max: 0.4, step: 0.01, def: 0.08, format: (v) => `${(v * 100).toFixed(0)} %`, store: r2 },
        { type: 'toggle', key: 'pad_steer_linear', label: 'Stick steers like a wheel (a wheel seen as a gamepad)' },
        { type: 'section', title: 'Force feedback' },
        { type: 'toggle', key: 'ff_enabled', label: 'Force feedback and vibration' },
        { type: 'toggle', key: 'ff_invert', label: 'Invert force feedback by default' },
        { type: 'note', text: 'Wheels with a saved direction use their own setting under Controls → Game controllers.' },
        // what the wheel feels all the time: the road's grain, the engine's buzz, and how
        // long a jolt eases away
        { type: 'slider', key: 'ff_road_vib', label: 'Road texture vibration', min: 0, max: 4, step: 0.05, def: 1, format: vibration, store: (v) => Math.round(v * 20) / 20 },
        { type: 'slider', key: 'ff_engine_vib', label: 'Engine vibration', min: 0, max: 4, step: 0.05, def: 1, format: vibration, store: (v) => Math.round(v * 20) / 20 },
        { type: 'slider', key: 'ff_fade', label: 'Vibration fade-out', min: 0, max: 1.5, step: 0.05, def: 0.28, format: (v) => (v < 0.01 ? t('Off') : `${Math.round(v * 1000)} ms`), store: r2 },
        { type: 'button', label: 'Reset wheel settings', icon: 'refresh', patch: () => ({ wheel_range: 900, wheel_lock: 0, ff_invert: false, ff_enabled: true, ff_road_vib: 1, ff_engine_vib: 1, ff_fade: 0.28 }) },
      ],
    },
  ],
};

const camera = {
  id: 'camera',
  label: 'Camera',
  cols: [
    {
      title: "Driver's view",
      rows: [
        // the driver's eye, moved from the bus's own camera
        { type: 'section', title: 'Seat position' },
        { type: 'slider', key: 'seat_y', label: 'Seat forward / back', min: -0.6, max: 0.6, step: 0.01, def: 0, format: (v) => signed(v * 100, ' cm'), store: r2 },
        { type: 'slider', key: 'seat_z', label: 'Seat up / down', min: -0.6, max: 0.6, step: 0.01, def: 0, format: (v) => signed(v * 100, ' cm'), store: r2 },
        { type: 'slider', key: 'seat_x', label: 'Seat right / left', min: -0.6, max: 0.6, step: 0.01, def: 0, format: (v) => signed(v * 100, ' cm'), store: r2 },
        { type: 'slider', key: 'seat_pitch_deg', label: 'Head pitch', min: -45, max: 45, step: 1, def: 0, format: (v) => signed(v, '°'), store: round },
        { type: 'button', label: 'Reset the seat position', icon: 'refresh', patch: () => ({ seat_x: 0, seat_y: 0, seat_z: 0, seat_pitch_deg: 0 }) },
        // (three screens have a field of view of their own, unless the headset draws)
        { type: 'slider', key: (s) => (s.triple_screen && !s.vr ? 'triple_fov_deg' : 'fov'), label: 'Field of view', min: 0, max: 120, step: 1, def: 0, format: (v) => (v < 20 ? t('Default') : deg(v)), store: (v) => (v < 20 ? 0 : Math.round(v)) },
        { type: 'slider', key: 'look_sens', label: 'Mouse look sensitivity', min: 0.1, max: 2, step: 0.05, def: 1, format: omsiPct, store: r2 },
        { type: 'toggle', key: 'right_stick_look', label: 'Right stick turns the view' },
        { type: 'slider', key: 'look_smoothing_ms', label: 'Smooth the mouse look', min: 0, max: 200, step: 10, def: 0, format: offMs, store: round },
        { type: 'section', title: 'A head at rest' },
        { type: 'slider', key: 'head_idle', label: 'Head sway at a standstill', min: 0, max: 1, step: 0.05, def: 0, format: (v) => (v <= 0 ? t('Off') : pct(v)), store: r2 },
        { type: 'slider', key: 'head_idle_pace', label: 'Sway pace', min: 0.5, max: 2, step: 0.05, def: 1, format: pct, store: r2 },
        { type: 'toggle', key: 'steer_look', label: "Driver's view turns with the steering" },
        { type: 'slider', key: 'steer_look_angle', label: 'Steering view angle', min: 0, max: 60, step: 1, def: 30, format: deg },
        { type: 'slider', key: 'steer_look_response', label: 'Steering view response', min: 0.05, max: 1, step: 0.05, def: 0.25, format: (v) => `${(v * 1000).toFixed(0)} ms` },
        { type: 'toggle', key: 'head_movement', label: 'Head moves with the bus' },
        { type: 'toggle', key: 'driverview_smooth', label: 'Camera glides between viewpoints' },
        { type: 'toggle', key: 'hands_in_cab', label: "Driver's hands in the cab view" },
        { type: 'toggle', key: 'alt_view', label: 'Right mouse button turns the view, Shift+right zooms (off: right zooms as in OMSI, the wheel button turns)' },
        { type: 'toggle', key: 'precision_zoom', label: 'Precision mouse zoom (FOV curve instead of the linear way)' },
      ],
    },
    {
      title: 'Outside views',
      rows: [
        { type: 'toggle', key: 'camera_collision', label: 'Camera collisions (outside view)' },
        { type: 'toggle', key: 'driver', label: 'Driver at the wheel (outside views)' },
        { type: 'section', title: 'Head tracking' },
        { type: 'toggle', key: 'head_tracking', label: 'Head tracking (native TrackIR / OpenTrack)' },
        // (percent per axis: 0 switches an axis off, 100 is 1:1)
        { type: 'section', title: 'Rotation' },
        trackSlider('head_tracking_yaw_sens', 'Yaw sensitivity (left / right)'),
        trackSlider('head_tracking_pitch_sens', 'Pitch sensitivity (up / down)'),
        trackSlider('head_tracking_roll_sens', 'Roll sensitivity'),
        { type: 'toggle', key: 'head_tracking_invert_yaw', label: 'Invert yaw' },
        { type: 'toggle', key: 'head_tracking_invert_pitch', label: 'Invert pitch' },
        { type: 'toggle', key: 'head_tracking_invert_roll', label: 'Invert roll' },
        { type: 'section', title: 'Position' },
        trackSlider('head_tracking_x_sens', 'X sensitivity (left / right)'),
        trackSlider('head_tracking_y_sens', 'Y sensitivity (up / down)'),
        trackSlider('head_tracking_z_sens', 'Z sensitivity (forward / back)'),
        { type: 'toggle', key: 'head_tracking_invert_x', label: 'Invert X' },
        { type: 'toggle', key: 'head_tracking_invert_y', label: 'Invert Y' },
        { type: 'toggle', key: 'head_tracking_invert_z', label: 'Invert Z' },
        // (15 %, not the 100 % a new file starts with: as the old page reset them)
        { type: 'button', label: 'Reset head-tracking axes', icon: 'refresh', patch: () => Object.fromEntries([...headTrackingKeys.map((k) => [k, 15]), ...headTrackingInverts.map((k) => [k, false])]) },
        { type: 'section', title: 'Triple screen' },
        { type: 'toggle', key: 'triple_screen', label: 'Three screen projections' },
        { type: 'toggle', key: 'triple_span', label: 'Span three monitors at startup' },
        { type: 'toggle', key: 'triple_hud_center', label: 'HUD on centre screen' },
        { type: 'note', text: 'Three equal screens in a horizontal row. OpenXR takes priority.', when: tripleOn },
        { type: 'slider', key: 'triple_width_mm', label: 'Visible width of one panel', min: 200, max: 2000, step: 10, def: 600, format: mm, when: tripleOn },
        // (a new distance gives the field of view back to the geometry)
        { type: 'slider', key: 'triple_distance_mm', label: 'Eye to centre screen', min: 200, max: 3000, step: 10, def: 650, format: mm, also: { triple_fov_deg: 0 }, when: tripleOn },
        { type: 'slider', key: 'triple_bezel_mm', label: 'Both frames at each join', min: 0, max: 100, step: 1, def: 0, format: mm, when: tripleOn },
        { type: 'slider', key: 'triple_left_angle_deg', label: 'Left screen inward angle', min: 0, max: 90, step: 1, def: 45, format: deg, when: tripleOn },
        { type: 'slider', key: 'triple_right_angle_deg', label: 'Right screen inward angle', min: 0, max: 90, step: 1, def: 45, format: deg, when: tripleOn },
        { type: 'slider', key: 'triple_eye_height_mm', label: 'Eye above screen centre', min: -500, max: 500, step: 1, def: 0, format: mm, when: tripleOn },
        // (OpenXR is built into the Windows game only)
        { type: 'section', title: 'Virtual reality', when: windows },
        { type: 'toggle', key: 'vr', label: 'Use OpenXR headset', when: windows },
        { type: 'select', key: 'vr_scale', label: 'Eye resolution', options: [['0.5', '50%'], ['0.65', '65%'], ['0.8', '80%'], ['1', '100%']], when: vrOn },
        { type: 'select', key: 'vr_head_smoothing_ms', label: 'Head tracking smoothing', options: [['0', 'Off'], ['5', '5 ms'], ['10', '10 ms'], ['20', '20 ms'], ['30', '30 ms']], when: vrOn },
        { type: 'select', key: 'vr_mirror_rate', label: 'Bus mirror refresh', options: [['0', 'Off'], ['8', '8/s'], ['16', '16/s'], ['24', '24/s'], ['32', '32/s'], ['48', '48/s'], ['60', '60/s'], ['90', '90/s'], ['120', '120/s'], ['180', '180/s'], ['240', '240/s'], ['360', '360/s'], ['-1', 'Every frame']], when: vrOn },
        { type: 'note', text: 'The rate is shared by all bus mirrors. Higher rates can reduce game FPS.', when: vrOn },
        { type: 'toggle', key: 'vr_desktop_mirror', label: 'Show headset picture on monitor', when: vrOn },
        // (the VR keys head the Controls page's game list)
        { type: 'button', label: 'Change the VR keys', icon: 'keyboard', controls: 0, when: vrOn },
      ],
    },
  ],
};

const vol = (key, label, def) => ({ type: 'slider', key, label, min: 0, max: 1, step: 0.05, def, format: pct, store: r2 });

const sound = {
  id: 'sound',
  label: 'Sound',
  cols: [
    {
      title: 'Volume',
      rows: [
        vol('volume', 'Volume', 0.6),
        vol('vol_ai', 'Traffic', 1),
        vol('vol_scenery', 'Surroundings', 1),
        { type: 'toggle', key: 'doppler', label: 'Doppler effect' },
        { type: 'toggle', key: 'ambient', label: 'Ambience (places, weather, nature, town)' },
        vol('vol_ambient', 'Ambience volume', 0.8),
        { type: 'select', key: 'pax_voices', label: 'Passenger voices', options: [['all', 'Greetings and tickets'], ['tickets', 'Only the ticket asked for'], ['off', 'Silent']] },
        // the bus radio's own sound (all off at first)
        vol('radio_speaker', 'Radio: small speaker', 0),
        vol('radio_hiss', 'Radio: FM hiss', 0),
        vol('radio_dropouts', 'Radio: signal dropouts', 0),
      ],
    },
    {
      title: 'Radio stations',
      rows: [
        { type: 'note', text: "A radio's station button n plays the n-th station, a cassette player the first; Shift+R steps through them. An address is an MP3, AAC or Ogg stream or an .m3u/.pls playlist." },
        { type: 'custom', id: 'radio' },
      ],
    },
  ],
};

const gameplay = {
  id: 'gameplay',
  label: 'Gameplay',
  cols: [
    {
      title: 'Passengers',
      rows: [
        { type: 'select', key: 'boarding', label: 'Boarding', options: [['auto', 'Pay and take the ticket'], ['pay', 'The driver sells the ticket'], ['walk', 'Just walk in']] },
        { type: 'select', key: 'standing_chance', label: 'Standing passengers', options: [['0', 'Never'], ['5', '5%'], ['10', '10%'], ['25', '25%'], ['50', '50%'], ['100', 'Always']] },
        { type: 'select', key: 'passenger_animation', label: 'Passenger animations', options: [['original', 'Original OMSI'], ['enhanced', 'Natural (procedural)']] },
        // (drawn on this computer only; the avatars are downloaded once, about 310 MB)
        { type: 'select', key: 'character_models', label: 'Character models', options: [['original', 'Original'], ['improved', 'High quality (improved)']], help: 'High quality draws the people with the Microsoft Rocketbox avatars and their living faces, on this computer only: servers and other players keep their own. They are downloaded once (about 310 MB).' },
        { type: 'toggle', key: 'exact_fare', label: 'Passengers pay the exact fare' },
        { type: 'toggle', key: 'pax_bad_driving', label: 'Passengers react to bad driving' },
        { type: 'slider', key: 'pax_density', label: 'How many passengers', min: 0, max: 2, step: 0.1, def: 1, format: pct, store: r2 },
        { type: 'toggle', key: 'get_up', label: 'Ability to get up (Ctrl+Shift+G)' },
        { type: 'toggle', key: 'full_tank', label: 'Start with a full tank' },
        { type: 'toggle', key: 'trip_summary', label: 'Trip summary' },
        { type: 'section', title: 'Traffic' },
        { type: 'select', key: 'ai_unsched_factor', label: 'Random traffic', options: [['25', '25%'], ['50', '50%'], ['75', '75%'], ['100', '100%'], ['150', '150%'], ['200', '200%']] },
        { type: 'select', key: 'ai_max_scheduled', label: 'Timetable vehicles', options: [['0', 'All'], ['10', 'At most 10'], ['25', 'At most 25'], ['50', 'At most 50']] },
        { type: 'select', key: 'ai_max_parked', label: 'Parked cars', options: [['-1', 'None'], ['0', 'Every space'], ['35', 'At most 35'], ['100', 'At most 100'], ['250', 'At most 250']] },
        // (OMSI's AIMaxHumans, #2087)
        { type: 'select', key: 'ai_max_humans', label: 'People at once', options: [['50', '50'], ['100', '100'], ['200', '200'], ['400', '400'], ['720', '720'], ['1000', '1000']] },
        // (off: an early bus waits at every stop, as in OMSI)
        { type: 'toggle', key: 'ai_wait_timed_stops_only', label: 'Timetable buses ahead of time wait only at timed stops' },
      ],
    },
    {
      // OMSI's own options (options.cfg)
      title: 'Simulation',
      rows: [
        { type: 'select', key: 'maintenance', label: 'Maintenance', options: [['0', 'Infinite (no wear)'], ['1', 'Very bad'], ['2', 'Bad'], ['3', 'Normal'], ['4', 'Good']] },
        { type: 'toggle', key: 'collision_vehicles', label: 'Collisions with vehicles' },
        { type: 'toggle', key: 'collision_objects', label: 'Collisions with objects (walls, poles)' },
        { type: 'toggle', key: 'collision_pedestrians', label: 'Collisions with people' },
        { type: 'toggle', key: 'use_real_time', label: 'Start at the real time' },
        { type: 'toggle', key: 'use_real_date', label: "Start on today's date" },
        // the game's clock follows this device's (the host's in multiplayer)
        { type: 'toggle', key: 'time_sync', label: 'Sync the clock with the real time (locks the time)' },
        // the weather follows the METAR report of the airport nearest the map
        { type: 'toggle', key: 'metar_sync', label: 'Sync the weather with METAR (locks the weather)' },
        // (in multiplayer the host's or the server's speed counts)
        { type: 'select', key: 'time_speed', label: 'Time speed (not in multiplayer or with the real-time sync)', options: [['1', 'Real time'], ['2', 'x2'], ['4', 'x4'], ['8', 'x8'], ['15', 'x15'], ['30', 'x30']] },
      ],
    },
  ],
};

const general = {
  id: 'general',
  label: 'General',
  cols: [
    {
      title: 'Interface & online',
      rows: [
        { type: 'custom', id: 'theme' },
        { type: 'custom', id: 'launcher_layout' },
        { type: 'custom', id: 'launcher_size' },
        { type: 'select', key: 'language', label: 'Language', raw: true, options: (s, boot) => (boot?.languages || []).map((l) => [l.code, l.name]) },
        // (texts nobody has translated: translated on this machine)
        { type: 'toggle', key: 'machine_translation', label: 'Translate the remaining texts automatically (offline, downloads 620 MB once)' },
        { type: 'toggle', key: 'launcher_rest', label: 'The launcher rests while a game runs (gives the graphics card to the game)' },
        { type: 'toggle', key: 'discord_status', label: 'Discord Rich Presence', help: 'Shows the launcher or your map, bus, line and multiplayer status in Discord.' },
        { type: 'toggle', key: 'voice_chat', label: 'Voice chat through GreenTeaSpeak (multiplayer)', help: 'Players near you are heard from where they stand, when GreenTeaSpeak runs with the openOMSI plugin and the server names a voice server.' },
        // (larger for those who find the texts hard to read, smaller for more of the picture)
        { type: 'slider', key: 'ui_scale', label: 'Game interface size', min: 0.5, max: 2, step: 0.05, def: 1, format: pct, store: r2 },
        { type: 'toggle', key: 'ui_scale_window', label: 'Interface grows with the window' },
        // (the backgrounds only, the texts stay solid; 85 % as designed)
        { type: 'slider', key: 'ui_opacity', label: 'Interface opacity', min: 0.2, max: 1, step: 0.05, def: 0.85, format: pct, store: r2 },
        { type: 'toggle', key: 'tooltips', label: 'Name of the button under the mouse' },
        { type: 'toggle', key: 'show_fps', label: 'Frame rate in the corner' },
        { type: 'select', key: 'perf_overlay', label: 'Performance details', options: [['off', 'Off'], ['compact', 'Compact'], ['detailed', 'Detailed']] },
        { type: 'toggle', key: 'notes', label: 'Notes in the top-left corner' },
        { type: 'toggle', key: 'chat', label: 'Chat in online games' },
        // (on top of the interface's size; Ctrl + the wheel over it in the game)
        { type: 'slider', key: 'chat_size', label: 'Chat size', min: 0.5, max: 3, step: 0.1, def: 1, format: pct, store: r1 },
        { type: 'toggle', key: 'name_tags', label: "Other players' names above their buses" },
        { type: 'select', key: 'photo_mode', label: 'Photo mode', options: [['simple', 'Simple'], ['advanced', 'Advanced (for photographers)']], help: 'Simple: the lens, the look and the frame. Advanced: a camera body - sensor size, P/A/S/M exposure with ISO and metering, white balance in kelvin, tilt-shift, histogram, focus peaking, bracketing and 16-bit PNG.' },
        { type: 'section', title: 'Navigator' },
        { type: 'toggle', key: 'navigator', label: 'Navigator (Shift+N: map, schedule, off)' },
        { type: 'toggle', key: 'nav_arrows', label: 'Route arrows (as in OMSI 2)' },
        { type: 'toggle', key: 'nav_ai', label: 'AI vehicles on the map' },
        { type: 'toggle', key: 'nav_surface', label: 'Map surface under the roads' },
        { type: 'toggle', key: 'nav_light', label: 'Light map' },
        { type: 'toggle', key: 'nav_3d', label: '3D buildings on the map' },
        { type: 'toggle', key: 'nav_widget', label: 'Navigator as a widget (drag it anywhere, size it at its corner)' },
        { type: 'slider', key: 'nav_size', label: 'Navigator size', min: 0.6, max: 2, step: 0.05, def: 1, format: (v) => `${Math.round(v * 100)} %` },
        { type: 'custom', id: 'corner' },
      ],
    },
    {
      // updates from the GitHub releases
      title: 'Updates',
      rows: [
        { type: 'toggle', key: 'update_check', label: 'Look for updates when the launcher starts' },
        { type: 'select', key: 'update_channel', label: 'Update channel', options: [['stable', 'Stable'], ['beta', 'Beta (for testers)']], help: 'Beta: every new version as soon as it is built, before it is tested. Back on Stable, the beta you have stays until a newer stable version comes out.', recheckUpdates: true },
        { type: 'toggle', key: 'update_auto', label: 'Install updates without asking' },
        { type: 'toggle', key: 'update_notify', label: 'Tell me about a new version during a session' },
        { type: 'toggle', key: 'presence', label: 'Count me in the website\'s "playing now" (anonymous)' },
        { type: 'custom', id: 'version' },
        // every setting at once: at the end, not the first control one sees
        { type: 'section', title: 'Reset' },
        { type: 'custom', id: 'reset' },
        { type: 'section', title: 'Support' },
        { type: 'custom', id: 'support' },
      ],
    },
  ],
};

/** Content test: no settings - the page draws the test itself (content-test.js). */
const contentTest = { id: 'content', label: 'Content test', page: 'content', cols: [], when: (s, p) => !mobile(s, p) };

export const SETTINGS_TABS = [graphics, driving, camera, sound, gameplay, general, contentTest];

/** The settings a saved graphics profile holds (`GRAPHICS_PROFILE_KEYS`). */
export const GRAPHICS_PROFILE_KEYS = [
  'graphics', 'msaa', 'render_scale', 'anisotropy', 'shadow_size', 'ssao', 'shadows', 'shadow_casters', 'detail_textures', 'night_brightness', 'led_glow', 'led_mips', 'shadow_blobs', 'reflections', 'clouds', 'cloud_quality', 'rain_quality', 'puddles', 'windy_trees',
  'vsync', 'max_fps', 'view_distance', 'max_obj_dist', 'min_obj_size', 'mirror_size', 'mirror_refresh', 'mirror_quality', 'mirror_hud_every_frame', 'video_memory', 'texture_compression', 'gpu_texture_compression',
];
