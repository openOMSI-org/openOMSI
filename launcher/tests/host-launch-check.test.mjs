import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

async function setup({ renderDelay = 0, hideCode = false } = {}) {
  const timers = new Set();
  const later = (fn, ms) => {
    const timer = setTimeout(() => { timers.delete(timer); fn(); }, ms);
    if (ms >= 400) timer.unref();
    timers.add(timer);
    return timer;
  };
  let finishPoll;
  const pendingPoll = new Promise(resolve => { finishPoll = resolve; });
  const originalFetch = async () => {
    await pendingPoll;
    return Response.json({ stamp: '', jobs: [], instances: [] });
  };
  let state, visibleAt;
  const context = vm.createContext({
    console, Response,
    window: { fetch: originalFetch, addEventListener() {} },
    fetch: (...args) => context.window.fetch(...args),
    navigator: { userAgent: 'Windows' },
    localStorage: { getItem: () => null, setItem() {} },
    requestAnimationFrame() {}, setTimeout: later, clearTimeout,
    document: {
      documentElement: { dataset: {} },
      querySelector(selector) {
        assert.equal(selector, '.drive-mode');
        const code = state.poll.instances[0]?.lan_status?.code;
        if (code && visibleAt === undefined) visibleAt = Date.now() + renderDelay;
        return { textContent: !hideCode && code && Date.now() >= visibleAt ? code : '' };
      },
    },
  });
  const modules = new Map();
  async function module(url) {
    if (!modules.has(url.href)) {
      modules.set(url.href, (async () => {
        let source;
        if (url.pathname.endsWith('/vendor/preact-htm.js')) {
          source = 'export const useEffect = () => {}, useState = () => {}, html = () => {}, render = () => {};';
        } else if (url.pathname.endsWith('/pages/drive-mode.js')) {
          source = 'export const DriveMode = () => {};';
        } else if (url.pathname.endsWith('/pages/multiplayer.js')) {
          source = 'export const MultiplayerPage = () => {};';
        } else source = await readFile(url, 'utf8');
        return new vm.SourceTextModule(source, {
          context, identifier: url.href,
          importModuleDynamically: (specifier, parent) => module(new URL(specifier, parent.identifier)),
        });
      })());
    }
    return modules.get(url.href);
  }
  const entry = await module(new URL('./drive-mode-check.js', import.meta.url));
  await entry.link((specifier, parent) => module(new URL(specifier, parent.identifier)));
  await entry.evaluate();
  const data = (await module(new URL('../ui/js/data.js', import.meta.url))).namespace;
  state = (await module(new URL('../ui/js/store.js', import.meta.url))).namespace.S;
  Object.assign(state, {
    choice: { ...data.CHOICE_DEFAULTS, free: true }, page: 'drive',
    maps: [], vehicles: [], depot: [], config: {},
  });
  return {
    state, data, check: entry.namespace.hostLaunchStaysOnDrive,
    startOldPoll() {
      const old = data.pollNow();
      later(finishPoll, 250);
      return old;
    },
    verifyRestored(saved) {
      assert.equal(context.window.fetch, originalFetch);
      for (const [key, value] of Object.entries(saved)) assert.equal(state[key], value, key);
    },
    close() { finishPoll(); for (const timer of timers) clearTimeout(timer); },
  };
}

test('host launch check waits for a pre-existing poll before checking the code', async () => {
  const app = await setup({ renderDelay: 200 });
  const saved = Object.fromEntries(['choice', 'page', 'poll', 'gameStarted', 'toasts'].map(key => [key, app.state[key]]));
  try {
    const oldPoll = app.startOldPoll();
    assert.match(await app.check(), /^PASS /);
    await oldPoll;
    app.verifyRestored(saved);
  } finally { app.close(); }
});

test('host launch check still fails if the session code never appears', async () => {
  const app = await setup({ hideCode: true });
  const saved = Object.fromEntries(['choice', 'page', 'poll', 'gameStarted', 'toasts'].map(key => [key, app.state[key]]));
  try {
    await assert.rejects(app.check(), /The launched host code must appear on the same Drive page/);
    app.verifyRestored(saved);
  } finally { app.close(); }
});
