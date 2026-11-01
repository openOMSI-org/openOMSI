// Server-fleet state and launch contract, without a live VPS or OMSI installation.
// Run: node --experimental-vm-modules --test launcher/tests/server-fleet.test.mjs
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

async function setup() {
  const calls = [];
  let server = null;
  const context = vm.createContext({
    window: {
      __TAURI__: {
        core: { invoke: async (_method, { cmd, args } = {}) => {
          calls.push({ cmd, args });
          if (cmd === 'server_query') return typeof server === 'function' ? server(args) : server;
          if (cmd === 'launch') return { others: 0 };
          if (cmd === 'poll') return { stamp: '', jobs: [], instances: [], started: [] };
          if (['choice_save', 'hof', 'default_hof', 'lines', 'saved_situations'].includes(cmd)) return null;
          throw new Error('Unexpected launcher command: ' + cmd);
        } },
      },
      addEventListener: () => {},
    },
    document: { documentElement: { dataset: {} } },
    navigator: { userAgent: 'Windows' },
    localStorage: { getItem: () => null, setItem: () => {} },
    matchMedia: () => ({ matches: false }),
    requestAnimationFrame: () => {},
    setTimeout: () => 0,
    clearTimeout: () => {},
  });
  const modules = new Map();
  async function module(url) {
    if (!modules.has(url.href)) {
      const code = url.pathname.endsWith('/vendor/preact-htm.js')
        ? 'export const useEffect = () => {}; export const useState = () => {};'
        : await readFile(url, 'utf8');
      modules.set(url.href, new vm.SourceTextModule(code, { context, identifier: url.href }));
    }
    return modules.get(url.href);
  }
  const entry = await module(new URL('../ui/js/data.js', import.meta.url));
  await entry.link((path, parent) => module(new URL(path, parent.identifier)));
  await entry.evaluate();
  const state = (await module(new URL('../ui/js/store.js', import.meta.url))).namespace.S;
  const data = entry.namespace;
  const bus = 'Vehicles/Example/S21.bus';
  const another = 'Vehicles/Example/NotAllowed.bus';
  const local = { file: bus, name: 'Local S21', manufacturer: 'Example', type_name: 'S21',
    folder: 'Vehicles/Example', transport: ['bus'], paints: ['Local Red'], installed: true };
  const offer = { bus, paints: ['Official Blue', 'Local Red'] };
  Object.assign(state, {
    vehicles: [local, { ...local, file: another, name: 'Not allowed' }],
    maps: [{ file: 'maps/CI/global.cfg' }],
    config: { profile: 'Tester' }, lines: { key: '', list: [] },
    depot: [], favourites: [], poll: { stamp: '', jobs: [], instances: [] },
    toasts: [], page: 'drive',
    choice: { ...data.CHOICE_DEFAULTS, bus, map: 'maps/CI/global.cfg',
      free: true, lan_mode: 'join', lan_addr: 'ci.example:27015',
      lan_map: 'maps/CI/global.cfg',
      lan_server: { asked: true, vehicles: [bus, another], fleet_version: 1, fleet: [offer] } },
  });
  return { state, data, calls, bus, another, offer,
    setServer: (value) => { server = value; },
    launched: () => calls.filter((c) => c.cmd === 'launch') };
}

test('server catalogue is authoritative, including the absence of preview and missing paints', async () => {
  const { state, data, bus, another } = await setup();
  const list = data.selectableVehicles();
  assert.equal(list.length, 1, 'an allowed path without an advertised package cannot be selected');
  assert.equal(list[0].file, bus);
  assert.equal(list[0].paints.length, 2);
  assert.equal(data.canPreviewBus(list[0], 'Local Red'), true);
  assert.equal(data.canPreviewBus(list[0], 'Official Blue'), false, 'server-only repaint has no local preview');
  assert.equal(data.serverFleetSelection(), true);
  state.choice = { ...state.choice, paint: 'No longer on server' };
  assert.equal(data.allowedBus().paint, '', 'removed server paint must fall back to default');
  state.choice = { ...state.choice, paint: 'Official Blue' };
  assert.equal(Object.keys(data.allowedBus()).length, 0, 'still-offered paint must remain');
  state.vehicles = state.vehicles.filter((v) => v.file !== bus);
  const remote = data.selectableVehicles();
  assert.equal(remote.length, 1);
  assert.equal(remote[0].server_only, true);
  assert.equal(data.canPreviewBus(remote[0], ''), false, 'never preview an uninstalled bus');
  assert.equal(data.canPreviewBus(remote[0], 'Official Blue'), false);
  state.choice = { ...state.choice, bus: another };
  assert.equal(data.serverFleetSelection(), false, 'whitelisted but unadvertised buses cannot be downloaded');
  assert.equal(data.allowedBus().bus, bus, 'an unavailable bus should switch to the first available offer');
  state.choice = { ...state.choice, bus, lan_server: { asked: true, vehicles: [another],
    fleet_version: 1, fleet: [{ bus, paints: [] }] } };
  assert.equal(data.serverFleetSelection(), false, 'catalogue entries outside the whitelist cannot authorize downloads');
});

test('launch requires positive per-session consent and never serializes the UI consent marker', async () => {
  const { state, data, launched, another } = await setup();
  assert.equal(await data.start(), null, 'no automatic download without consent');
  assert.equal(launched().length, 0);
  state.choice = { ...state.choice, bus: another };
  assert.equal(await data.start({ __fleetApproved: true }), null, 'consent cannot bypass a missing fleet offer');
  assert.equal(launched().length, 0);
  state.choice = { ...state.choice, bus: 'Vehicles/Example/S21.bus' };
  await data.start({ __fleetApproved: true });
  assert.equal(launched().length, 1);
  assert.equal(launched()[0].args.duty.allow_server_fleet, true);
  assert.equal(Object.hasOwn(launched()[0].args.duty, '__fleetApproved'), false);
  state.choice = { ...state.choice, lan_mode: 'off' };
  await data.start({ __fleetApproved: true });
  assert.equal(launched().length, 2);
  assert.equal(launched()[1].args.duty.allow_server_fleet, false, 'offline must never enable server download');
});

test('refresh selects only a valid advertised bus and ignores a response for a previous server', async () => {
  const { state, data, calls, bus, another, setServer } = await setup();
  state.choice = { ...state.choice, bus: another };
  setServer({ name: 'Official CI host', map: 'maps/CI/global.cfg',
    vehicles: [bus, another], fleet_version: 1, fleet: [{ bus, paints: ['Blue'] }] });
  await data.askJoined({ refresh: true });
  assert.equal(state.choice.bus, bus);
  assert.equal(data.selectableVehicles()[0].paints[0], 'Blue');
  state.choice = { ...state.choice, paint: 'Retired paint' };
  await data.askJoined({ refresh: true });
  assert.equal(state.choice.paint, '', 'refresh must clear a removed repaint');
  assert.equal(calls.filter((c) => c.cmd === 'server_query').length, 2);
  let finish;
  setServer(() => new Promise((resolve) => { finish = resolve; }));
  const pending = data.askJoined({ refresh: true });
  state.choice = { ...state.choice, lan_addr: 'different.example:27015', lan_server: { asked: false } };
  finish({ vehicles: [another], fleet_version: 1, fleet: [{ bus: another, paints: [] }] });
  await pending;
  assert.equal(state.choice.lan_addr, 'different.example:27015');
  assert.equal(state.choice.lan_server.asked, false, 'a stale response must not rewrite a different session');
});

test('legacy unrestricted sessions are never implicitly treated as an official downloaded fleet', async () => {
  const { state, data, another } = await setup();
  state.choice = { ...state.choice, bus: another,
    lan_server: { asked: true, vehicles: [], fleet_version: 0, fleet: [] } };
  assert.equal(data.serverBuses(), null);
  assert.equal(data.serverFleetSelection(), false);
  assert.equal(data.selectableVehicles().length, 2, 'legacy local selection remains available');
});
