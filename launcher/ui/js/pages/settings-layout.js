const groups = {
  graphics: [
    ['display', 'Display', 'fullscreen resolution vsync max_fps graphics_api sim_thread'],
    ['quality', 'Quality', 'graphics @preset msaa upscaler @nvidia_dlss_install upscale_quality fsr_quality fsr_sharpness render_scale anisotropy detail_textures led_glow led_mips'],
    ['lighting', 'Lighting', 'ray_tracing shadow_size ssao shadows shadow_casters night_brightness shadow_blobs reflections road_glare traffic_headlights street_light light_shadows light_glow puddles'],
    ['weather', 'Weather & effects', 'clouds cloud_quality cloud_shadows rain_quality vehicle_smoke windy_trees detailed_trees road_snow'],
    ['mirrors', 'Mirrors', 'mirror_*'],
    ['resources', 'World & memory', '#World_&_memory view_distance max_obj_dist min_obj_size video_memory texture_compression gpu_texture_compression @optimise'],
    ['profiles', 'Profiles', '#Profiles @profiles'],
  ],
  driving: [
    ['keyboard', 'Keyboard & mouse', '#Keyboard_&_mouse drive_keys steering_linear old_steering red_steer_spd blinker_cancel brake_hold key_pedal_speed auto_clutch auto_shift'],
    ['mouse', 'Mouse', 'mouse_*'],
    ['wheel', 'Wheel and pedals', '#Wheel_and_pedals wheel_* cab_wheel_rotation pedal_* momentary_gears arrows_switch_cams ctrl_background'],
    ['gamepad', 'Gamepad', '#Gamepad pad_*'],
    ['feedback', 'Force feedback', '#Force_feedback ff_*'],
    ['touch', 'Touch controls', '#Touch_controls touch_*'],
  ],
  camera: [
    ['view', "Driver's view", 'look_* right_stick_look hands_in_cab alt_view mirror_hud precision_zoom zoom_* flashlight_cursor'],
    ['seat', 'Seat position', '#Seat_position seat_*'],
    ['movement', 'A head at rest', '#A_head_at_rest head_idle* steer_look* head_movement driverview_smooth'],
    ['outside', 'Outside views', 'camera_collision driver'],
    ['tracking', 'Head tracking', '#Head_tracking #Rotation #Position head_tracking* tobii_edges'],
    ['triple', 'Triple screen', '#Triple_screen triple_*'],
    ['vr', 'Virtual reality', '#Virtual_reality vr*'],
  ],
  sound: [
    ['volume', 'Volume', 'volume vol_* doppler ambient pax_voices'],
    ['radio', 'Radio stations', 'radio_* @radio'],
    ['voice', 'Voice chat in multiplayer', '#Voice_chat_in_multiplayer voice_*'],
  ],
  gameplay: [
    ['passengers', 'Passengers', 'boarding standing_chance passenger_* character_models exact_fare pax_* auto_announce'],
    ['traffic', 'Traffic', '#Traffic ai_*'],
    ['simulation', 'Simulation', 'maintenance collision_* realistic_road_physics use_real_* time_* metar_sync get_up full_tank trip_summary'],
  ],
  general: [
    ['appearance', 'Interface', '@theme @launcher_layout @launcher_size language machine_translation launcher_rest ui_* tooltips'],
    ['hud', 'Information on screen', 'show_fps perf_overlay notes* chat* name_tags photo_mode'],
    ['online', 'Online', 'discord_* disable_plugins presence hardware_id'],
    ['navigator', 'Navigator', '#Navigator navigator nav_* @corner'],
    ['updates', 'Updates', 'update_* @version'],
    ['support', 'Support & reset', '#Reset #Support @reset @support'],
  ],
};

const defaults = { graphics: 'quality', driving: 'keyboard', camera: 'view', sound: 'volume', gameplay: 'passengers', general: 'appearance' };
const customLabels = { profiles: 'Profiles', preset: 'Quality preset', optimise: 'Optimise for my PC', nvidia_dlss_install: 'NVIDIA DLSS runtime (optional)', theme: 'Launcher theme', launcher_layout: 'Launcher layout', launcher_size: 'Launcher size', corner: 'Corner', version: 'Updates', reset: 'Reset', support: 'Support', radio: 'Radio stations' };

export const settingToken = (row) => typeof row.key === 'string' ? row.key : row.id ? '@' + row.id : row.type === 'section' ? '#' + row.title.replaceAll(' ', '_') : row.label || row.text;
export const settingLabel = (row) => row.label || customLabels[row.id] || row.title || '';
export const settingId = (tab, row) => tab.id + ':' + settingToken(row);
export const isSettingVisible = (row, settings, platform) => !row.when || row.when(settings, platform);
export const isSettingControl = (row) => !['section', 'note'].includes(row.type);

export function settingsGroups(tab) {
  const definitions = groups[tab.id];
  if (!definitions) return tab.cols.map((col, i) => ({ id: String(i), label: col.title, rows: col.rows }));
  const rules = definitions.map(([id, label, keys]) => ({ id, label, keys: keys.split(' '), rows: [] }));
  for (const col of tab.cols) {
    let previous = rules.find((rule) => rule.label === col.title) || rules.find((rule) => rule.id === defaults[tab.id]) || rules[0];
    for (const row of col.rows) {
      const token = settingToken(row);
      let target = rules.find((rule) => rule.keys.some((key) => key.endsWith('*') ? token?.startsWith(key.slice(0, -1)) : token === key));
      if (!target && row.type === 'button') {
        if (row.controls === 1 || row.label === 'Reset wheel settings') target = rules.find((rule) => rule.id === 'wheel');
        if (row.label === 'Reset the seat position') target = rules.find((rule) => rule.id === 'seat');
        if (row.label === 'Reset head-tracking axes') target = rules.find((rule) => rule.id === 'tracking');
        if (row.label === 'Change the VR keys') target = rules.find((rule) => rule.id === 'vr');
      }
      target ||= row.type === 'note' || row.type === 'section' ? previous : rules.find((rule) => rule.id === defaults[tab.id]) || rules[0];
      target.rows.push(row);
      previous = target;
    }
  }
  return rules.filter((rule) => rule.rows.length);
}

export function readFavourites(text) {
  try { const value = JSON.parse(text); return new Set(Array.isArray(value) ? value.filter((id) => typeof id === 'string') : []); }
  catch { return new Set(); }
}
