// Reading the Mods list again (`reread`, `rereadUntil`, `cached`): overlapping reads, failed
// reads and what the page holds until a read confirms it (#4016, #4017).
// Run: node --experimental-vm-modules --test launcher/tests/mods-list-read.test.mjs
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

async function setup() {
  // every mods_status read waits here until the test answers it
  const reads = [];
  let poll = null;
  const timers = [];
  const context = vm.createContext({
    window: {
      __TAURI__: {
        core: { invoke: async (_method, { cmd } = {}) => {
          if (cmd === 'mods_status') {
            let answer, fail;
            const p = new Promise((res, rej) => ((answer = res), (fail = rej)));
            reads.push({ answer, fail });
            return p;
          }
          if (cmd === 'poll') return poll;
          if (cmd === 'maps' || cmd === 'weather') return [];
          // (the rest of loadContent is not what these tests look at)
          return new Promise(() => {});
        } },
      },
      addEventListener: () => {},
    },
    document: { documentElement: { dataset: {} } },
    navigator: { userAgent: 'Windows' },
    localStorage: { getItem: () => null, setItem: () => {} },
    matchMedia: () => ({ matches: false }),
    requestAnimationFrame: () => {},
    setTimeout: (f, ms) => timers.push({ f, ms }),
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
  const flush = () => new Promise((r) => setImmediate(r));
  // runs the waits `rereadUntil` asked for (not the toasts' own timers)
  const retry = async () => {
    const waits = timers.splice(0).filter((x) => x.ms <= 30000 && !String(x.f).includes('toasts'));
    for (const x of waits) x.f();
    await flush();
  };
  return { state, data, reads, timers, flush, retry, setPoll: (p) => { poll = p; } };
}

const list = (...ids) => ({ installed: ids.map((id) => ({ id, name: id, enabled: true })) });
const settledState = async (p, flush) => {
  let out = 'pending';
  p.then((v) => (out = { ok: v }), (e) => (out = { error: e.message }));
  await flush();
  return out;
};

test('overlapping reads: the last asked wins, whichever answers first', async () => {
  const { state, data, reads, flush } = await setup();
  const first = data.reread('mods', 'mods_status');
  const second = data.reread('mods', 'mods_status');
  assert.equal(state.reading.mods, true);
  // the later read answers first, the earlier one after it
  reads[1].answer(list('b'));
  await flush();
  assert.deepEqual(state.cache.mods, list('b'));
  assert.equal(state.reading.mods, false);
  reads[0].answer(list('a'));
  await flush();
  assert.deepEqual(state.cache.mods, list('b'), 'an earlier read answering late must not overwrite a later one');
  assert.deepEqual(await first, list('b'), 'the earlier caller gets the later answer');
  assert.deepEqual(await second, list('b'));
});

test('overlapping reads: an earlier answer is dropped while the later read still runs', async () => {
  const { state, data, reads, flush } = await setup();
  state.cache = { mods: list('old') };
  const first = data.reread('mods', 'mods_status');
  const second = data.reread('mods', 'mods_status');
  reads[0].answer(list('a'));
  await flush();
  assert.deepEqual(state.cache.mods, list('old'), 'the earlier answer is not the newest');
  assert.equal(state.reading.mods, true, 'still reading: the later read has not answered');
  assert.equal(await settledState(first, flush), 'pending', 'the earlier caller waits for the later read');
  reads[1].answer(list('b'));
  assert.deepEqual(await first, list('b'));
  assert.deepEqual(await second, list('b'));
});

test('a later read failing rejects, and an earlier caller sees that failure too', async () => {
  const { state, data, reads, flush } = await setup();
  state.cache = { mods: list('a') };
  const first = data.reread('mods', 'mods_status');
  const second = data.reread('mods', 'mods_status');
  const failed = Promise.all([
    assert.rejects(second, /content folder unreadable/),
    assert.rejects(first, /content folder unreadable/, 'a failed read confirms nothing to anyone waiting on it'),
  ]);
  reads[0].answer(list('b'));
  reads[1].fail('content folder unreadable');
  await failed;
  await flush();
  assert.deepEqual({ ...state.cache.mods }, { error: 'content folder unreadable' }, 'the page shows the error');
  assert.equal(state.reading.mods, false);
});

test('an earlier read failing does not matter once a later one answers', async () => {
  const { state, data, reads } = await setup();
  const first = data.reread('mods', 'mods_status');
  const second = data.reread('mods', 'mods_status');
  reads[0].fail('busy');
  reads[1].answer(list('b'));
  assert.deepEqual(await first, list('b'));
  assert.deepEqual(await second, list('b'));
  assert.deepEqual(state.cache.mods, list('b'));
});

test('cached keeps its contract: answers from the cache, null on a failed first read', async () => {
  const { state, data, reads, flush } = await setup();
  const p = data.cached('keys', 'mods_status');
  reads[0].fail('no key bindings');
  assert.equal(await p, null);
  assert.deepEqual({ ...state.cache.keys }, { error: 'no key bindings' });
  state.cache = { ...state.cache, keys: { a: 1 } };
  assert.deepEqual(await data.cached('keys', 'mods_status'), { a: 1 });
  reads[1].fail('later failure');
  await flush(); // (no unhandled rejection from the read behind it)
});

test('an answer a page wrote itself while a read ran is kept', async () => {
  const { state, data, reads } = await setup();
  state.cache = { keys: { old: 1 } };
  const p = data.reread('keys', 'mods_status');
  state.cache = { ...state.cache, keys: { saved: 1 } };
  reads[0].answer({ old: 1 });
  assert.deepEqual(await p, { saved: 1 });
  assert.deepEqual(state.cache.keys, { saved: 1 }, 'a save made during the read is newer than the read');
  assert.equal(state.reading.keys, false);
});

test('rereadUntil (a switch, a delete) waits through a failed read and asks again (#4017)', async () => {
  const { state, data, reads, flush, retry } = await setup();
  state.cache = { mods: list('a') };
  const p = data.rereadUntil('mods', 'mods_status');
  reads[0].fail('content folder unreadable');
  await flush();
  assert.equal(await settledState(p, flush), 'pending', 'a failed read must not end the switch\'s wait');
  assert.equal(state.toasts.filter((x) => x.kind === 'error').length, 1, 'the failure is told');
  await retry();
  assert.equal(reads.length, 2, 'the read is asked again');
  reads[1].fail('content folder unreadable');
  await flush();
  await retry();
  assert.equal(state.toasts.filter((x) => x.kind === 'error').length, 1, 'told once, not at every try');
  reads[2].answer(list('a', 'b'));
  assert.deepEqual(await p, list('a', 'b'));
});

test('an installed mod keeps its row through a failed list read (#4016)', async () => {
  const { state, data, reads, flush, retry, setPoll } = await setup();
  Object.assign(state, {
    cache: { mods: list('a') },
    poll: { stamp: '1', jobs: [{ id: 'j1', name: 'New bus', state: 'extracting' }], instances: [] },
    settling: [],
  });
  setPoll({ stamp: '2', jobs: [{ id: 'j1', name: 'New bus', state: 'done' }], instances: [] });
  await data.pollNow();
  await flush();
  assert.deepEqual([...state.settling], ['j1'], 'the finished install is held until the list has it');
  reads.at(-1).fail('content folder unreadable');
  await flush();
  assert.deepEqual([...state.settling], ['j1'], 'a failed read must not drop the pending row');
  await retry();
  reads.at(-1).answer(list('a', 'New bus'));
  await flush();
  assert.deepEqual([...state.settling], [], 'the row goes once a read has answered');
  assert.deepEqual(state.cache.mods, list('a', 'New bus'));
});
