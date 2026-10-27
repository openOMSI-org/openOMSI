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
  const store = await module(new URL('../ui/js/store.js', import.meta.url));
  return {
    data: data.namespace, state: store.namespace.S, patches, storage,
    theme: () => context.document.documentElement.dataset.theme,
    persisted: () => ({ ...persisted }),
  };
}

for (const [name, settings, preference] of [
  ['empty storage', { ui_theme: 'light' }, undefined],
  ['stale dark launcher preference', { ui_theme: 'light' }, 'dark'],
  ['stale light launcher preference', { ui_theme: 'dark' }, 'light'],
  // bootstrap 已由 Rust settings_from_text 將 nav_light=1 遷移成 ui_theme=light。
  ['legacy nav_light=1 migration', { ui_theme: 'light', nav_light: true }, 'dark'],
]) {
  test(`startup and restart preserve ${name}`, async () => {
    const storage = new Map(preference ? [['theme', preference]] : []);
    for (let restart = 0; restart < 2; restart++) {
      const app = await launcher(settings, storage);
      await app.data.boot();
      assert.equal(app.theme(), settings.ui_theme);
      assert.deepEqual(app.patches, []);
      await app.data.reloadSettings();
      assert.equal(app.theme(), settings.ui_theme);
      assert.equal(app.state.settings.ui_theme, settings.ui_theme);
      assert.equal(storage.get('theme'), settings.ui_theme);
      assert.deepEqual(app.patches, [], 'boot must not overwrite the persisted game theme');
    }
  });
}

for (const systemLight of [false, true]) {
  for (const saved of ['light', 'dark']) {
    test(`System ${systemLight ? 'light' : 'dark'} reconciles saved ${saved}`, async () => {
      const storage = new Map([['theme', 'system']]);
      for (let restart = 0; restart < 2; restart++) {
        const app = await launcher({ ui_theme: saved }, storage, systemLight);
        await app.data.boot();
        assert.equal(app.theme(), saved);
        assert.deepEqual(app.patches, []);
        await app.data.reloadSettings();
        assert.equal(app.theme(), saved);
        assert.equal(storage.get('theme'), (systemLight ? 'light' : 'dark') === saved ? 'system' : saved);
        assert.deepEqual(app.patches, []);
      }
    });
  }
}

test('a game-side change while the launcher is closed survives restart', async () => {
  const storage = new Map([['theme', 'dark']]);
  const before = await launcher({ ui_theme: 'dark' }, storage);
  await before.data.boot();
  const after = await launcher({ ui_theme: 'light' }, storage);
  await after.data.boot();
  await after.data.reloadSettings();
  assert.equal(after.theme(), 'light');
  assert.equal(after.persisted().ui_theme, 'light');
  assert.deepEqual(after.patches, []);
});

test('an explicit launcher theme change persists across restart', async () => {
  const storage = new Map([['theme', 'dark']]);
  const before = await launcher({ ui_theme: 'dark' }, storage);
  await before.data.boot();
  storage.set('theme', 'light');
  await before.data.applyTheme();
  assert.deepEqual(before.patches, [{ ui_theme: 'light' }]);
  const after = await launcher(before.persisted(), storage);
  await after.data.boot();
  assert.equal(after.theme(), 'light');
  assert.deepEqual(after.patches, []);
});
