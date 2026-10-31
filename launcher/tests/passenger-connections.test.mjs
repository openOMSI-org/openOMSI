import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

async function launcher(settings, storage = new Map(), systemLight = false) {
  const patches = [];
  let persisted = { ...settings };
  const context = vm.createContext({
    document: { documentElement: { dataset: {} } },
    navigator: { userAgent: 'Windows' },
    localStorage: { getItem: (key) => storage.get(key) ?? null, setItem: (key, value) => storage.set(key, value) },
    matchMedia: () => ({ matches: systemLight }),
    requestAnimationFrame: () => {}, setTimeout: () => {}, setInterval: () => {},
    window: {
      addEventListener: () => {},
      __TAURI__: {
        event: { listen: async () => () => {} },
        core: { invoke: async (_, { cmd, args }) => {
          if (cmd === 'bootstrap') return { language: 'en', settings: { ...persisted }, config: {}, choice: {}, root: null };
          if (cmd === 'settings_get') return { ...persisted };
          if (cmd === 'settings_patch') {
            patches.push({ ...args.patch });
            Object.assign(persisted, args.patch);
            return { ...persisted };
          }
          if (cmd === 'poll') return { stamp: '', jobs: [], instances: [] };
          throw new Error(`Unexpected command: ${cmd}`);
        } },
      },
    },
  });
  const modules = new Map();
  async function module(url) {
    if (!modules.has(url.href)) {
      const source = url.pathname.endsWith('/vendor/preact-htm.js')
        ? 'export const useEffect = () => {}; export const useState = () => {};'
        : await readFile(url, 'utf8');
      modules.set(url.href, new vm.SourceTextModule(source, { context, identifier: url.href }));
    }
    return modules.get(url.href);
  }
  const data = await module(new URL('../ui/js/data.js', import.meta.url));
  await data.link((specifier, parent) => module(new URL(specifier, parent.identifier)));
  await data.evaluate();
  const schema = await module(new URL('../ui/js/pages/settings-schema.js', import.meta.url));
  await schema.link((specifier, parent) => module(new URL(specifier, parent.identifier)));
  await schema.evaluate();
  const store = await module(new URL('../ui/js/store.js', import.meta.url));
  return {
    tabs: schema.namespace.SETTINGS_TABS, data: data.namespace, state: store.namespace.S, patches, storage,
    theme: () => context.document.documentElement.dataset.theme,
    persisted: () => ({ ...persisted }),
  };
}


const key = 'passenger_connections';
test('optional passenger setting survives launcher writes and restart', async () => {
  let settings = { [key]: false };
  for (const enabled of [false, true, false]) {
    const app = await launcher(settings);
    await app.data.boot();
    const rows = app.tabs.flatMap((tab) => (tab.cols || []).flatMap((col) => col.rows || []));
    assert.equal(rows.find((row) => row.key === key)?.type, 'toggle');
    assert.equal(await app.data.saveSettings({ [key]: enabled }), true);
    assert.equal(app.state.settings[key], enabled);
    assert.equal(app.persisted()[key], enabled);
    assert.deepEqual(app.patches.at(-1), { [key]: enabled });
    settings = app.persisted();
    const restarted = await launcher(settings);
    await restarted.data.boot();
    assert.equal(restarted.state.settings[key], enabled);
    const chance = rows.find((row) => row.key === 'passenger_transfer_percent');
    assert.equal(chance.type, 'slider');
    assert.equal(chance.when({ [key]: enabled }), enabled);
    assert.equal(chance.def, 20);
    assert.equal(chance.min, 0);
    assert.equal(chance.max, 100);
  }
});

test('all three passenger options remain independent across launcher restart', async () => {
  const keys = ['passenger_hailing', 'passenger_full_bus_reaction', 'passenger_connections'];
  for (let bits = 0; bits < 8; bits++) {
    const settings = Object.fromEntries(keys.map((key, index) => [key, !!(bits & (1 << index))]));
    settings.passenger_transfer_percent = 35;
    const app = await launcher(settings);
    await app.data.boot();
    const rows = app.tabs.flatMap((tab) => (tab.cols || []).flatMap((col) => col.rows || []));
    for (const key of [...keys, 'passenger_transfer_percent']) {
      assert.equal(rows.filter((row) => row.key === key).length, 1, key);
    }
    await app.data.saveSettings({ passenger_connections: !settings.passenger_connections });
    const restarted = await launcher(app.persisted());
    await restarted.data.boot();
    assert.equal(restarted.state.settings.passenger_hailing, settings.passenger_hailing);
    assert.equal(restarted.state.settings.passenger_full_bus_reaction, settings.passenger_full_bus_reaction);
    assert.equal(restarted.state.settings.passenger_connections, !settings.passenger_connections);
    assert.equal(restarted.state.settings.passenger_transfer_percent, 35);
  }
});
