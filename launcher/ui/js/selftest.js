// The page checks itself in a hidden window (OPENOMSI_LAUNCHER_SELFTEST=1): the commands
// answer, the content is read, a map picture comes through `omsi://`, the font is Inter, and
// every page draws without an error. The report goes back to the program, which prints it.

import { call, contentUrl } from './api.js';
import { S, set, remembered, remember } from './store.js';
import { applyTheme, LAUNCHER_LAYOUTS } from './data.js';
import { t } from './i18n.js';
import { readRouteMap } from './routemap.js';

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function until(f, ms = 60000) {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) {
    if (f()) return true;
    await sleep(100);
  }
  return false;
}

export async function selftest() {
  const passed = [];
  const failed = [];
  const errors = [];
  window.addEventListener('error', (e) => errors.push(String(e.message)));
  window.addEventListener('unhandledrejection', (e) => errors.push(String(e.reason?.message || e.reason)));
  const check = (name, ok, detail) => (ok ? passed : failed).push(detail ? `${name}: ${detail}` : name);
  const t0 = performance.now();

  check('bootstrap', !!S.boot, S.boot?.version);
  check('OMSI 2 found', !!S.boot?.root, S.boot?.root || S.boot?.root_error);
  check('maps read', await until(() => S.maps?.length > 0), `${S.maps?.length ?? 0}`);
  check('weather read', await until(() => S.weather != null), `${S.weather?.length ?? 0}`);
  check('buses read', await until(() => S.vehicles?.length > 0, 180000), `${S.vehicles?.length ?? 0}`);
  check('timetable read', await until(() => S.lines.list != null || S.lines.error), S.lines.error || `${S.lines.list?.length ?? 0} lines`);
  check('translations', !S.boot?.language || S.boot.language === 'en' || t('Settings') !== 'Settings', S.boot?.language || 'en');
  await document.fonts.ready;
  check('font Inter', document.fonts.check('16px Inter'));
  const map = S.maps?.[0];
  if (map) {
    const url = contentUrl(map.file.replace(/[^/]*$/, 'picture.jpg'));
    const r = await fetch(url).catch((e) => ({ ok: false, status: e.message }));
    check('omsi:// picture', r.ok, `${url} ${r.status}`);
  }
  // (OPENOMSI_LAUNCHER_SELFTEST=full: the game program draws the bus and reads the route map)
  if (S.boot?.selftest === 'full' && S.vehicles?.length) {
    const c = S.choice;
    const bus = S.vehicles.find((v) => /SD77|SD202|NL202/i.test(v.file)) || S.vehicles[0];
    try {
      await call('preview_look', { bus: bus.file, paint: '', map: c.map, weather: c.weather, time: c.time, date: c.date });
      let head = null;
      let bytes = 0;
      for (let i = 0; i < 600 && !(head?.picture && !head.busy && bytes); i++) {
        const buf = new Uint8Array(await call('preview_frame', { w: 640, h: 360, force: i === 0 }));
        const n = new DataView(buf.buffer, buf.byteOffset).getUint32(0, true);
        head = JSON.parse(new TextDecoder().decode(buf.subarray(4, 4 + n)));
        bytes = Math.max(bytes, buf.length - 4 - n);
        if (head.failed) break;
        await sleep(50);
      }
      check('bus preview (game renderer)', !!head?.picture && bytes > 1000, `${bus.file}: ${bytes} bytes of JPEG ${head?.error || ''}`);
    } catch (e) {
      check('bus preview (game renderer)', false, `${bus.file}: ${e.message || e}`);
    }
    const tour = S.lines.list?.flatMap((l) => l.tours)[0];
    const trip = tour?.trips[0]?.name || '';
    try {
      const m = await readRouteMap(c.map, c.date, trip);
      check('route map', m.roads?.length > 0 && (!trip || m.route.length > 0), `${m.roads?.length} roads, ${m.route?.length} route lanes, ${m.stops?.length} stops, ${m.entries?.length} entry points`);
    } catch (e) {
      check('route map', false, e.message || String(e));
    }
  }
  for (const page of ['drive', 'depot', 'multiplayer', 'profile', 'mods', 'settings', 'controls', 'games', 'tutorials', 'timetable', 'addons', 'setup']) {
    const before = errors.length;
    set({ page });
    await sleep(500);
    const shown = document.querySelector('.page h1')?.textContent;
    check(`page ${page}`, !!shown && errors.length === before, shown || errors.slice(before).join('; '));
    // (no dash of any kind, no raised dot: a plain hyphen stands for them)
    await sleep(50);
    const dash = document.body.textContent.match(/.{0,20}[\u2010-\u2015\u2212\u2E3A\u2E3B\uFE58\uFF0D\u00B7].{0,20}/);
    if (dash) check(`no dashes on ${page}`, false, dash[0]);
  }
  const layout = remembered('launcher_layout', 'bigmap');
  for (const [other] of LAUNCHER_LAYOUTS) {
    const before = errors.length;
    remember('launcher_layout', other);
    applyTheme();
    set({ page: 'profile' });
    await sleep(100);
    set({ page: 'drive' });
    await sleep(500);
    const shown = document.querySelector('.page h1')?.textContent;
    check(`page drive (${other})`, !!shown && errors.length === before && !!document.querySelector(other === 'classic' ? '.classic-drive' : `.lay-${other}`), shown || errors.slice(before).join('; '));
  }
  remember('launcher_layout', layout);
  applyTheme();
  try {
    const [c, st] = [await call('controllers_load'), await call('controllers_state')];
    check('game controllers', Array.isArray(c.devices) && Array.isArray(st.connected), `${c.devices.length} set up, ${st.connected.length} connected`);
  } catch (e) {
    check('game controllers', false, e.message || String(e));
  }
  try {
    const k = await call('keyboard_actions', { lang: 'en', actions: ['horn'] });
    check('key action names', typeof k.labels?.horn === 'string', k.labels?.horn);
  } catch (e) {
    check('key action names', false, e.message || String(e));
  }
  set({ page: 'controls', controlsTab: 'pads' });
  for (let i = 0; i < 50 && !document.querySelector('.pads'); i++) await sleep(100);
  check('page controls: game controllers', !!document.querySelector('.pads') && !document.querySelector('.pads .notice.danger'), document.querySelector('.page')?.textContent.slice(0, 200));
  set({ page: 'drive', controlsTab: 'keyboard' });
  check('no script errors', errors.length === 0, errors.join('; '));
  const report = { version: S.boot?.version, seconds: Math.round(performance.now() - t0) / 1000, passed, failed, errors };
  await call('selftest_done', { report });
}
