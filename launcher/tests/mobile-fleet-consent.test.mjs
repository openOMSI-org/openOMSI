// Execute the real mobile Start handler with launcher services and hooks stubbed.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const source = await readFile(new URL('../ui/js/mobile/mobile.js', import.meta.url), 'utf8');

async function mobile(approved, startError = false) {
  const calls = [];
  const busy = [];
  const state = { choice: { lan_mode: 'join', lan_addr: 'server', free: true, entry: -1, time: 8, date: '1989-05-30' }, poll: {}, boot: {} };
  let hook = 0;
  const html = (strings, ...values) => ({ strings, values });
  const services = {
    html,
    useStore: () => state,
    useState: (initial) => { const index = hook++; return [initial, value => { if (index === 1) busy.push(value); }]; },
    useEffect: () => {},
    useConfirm: () => [async () => { calls.push('consent'); return approved; }, null],
    t: text => text,
    curMap: () => ({ name: 'Test', entry_points: [] }),
    curBus: () => ({ file: 'Vehicles/Bus.bus', name: 'Bus' }),
    curLine: () => null,
    curTour: () => null,
    pickedTrip: () => null,
    noServerBus: () => false,
    transportRefusal: () => false,
    askJoined: async options => { assert.equal(options.refresh, true); calls.push('refresh'); },
    serverFleetSelection: () => true,
    start: async extra => { calls.push(['start', extra]); if (startError) throw new Error('start failed'); },
    mapName: name => name,
  };
  const context = vm.createContext({ console });
  const module = new vm.SourceTextModule(source + '\nexport { MobileDrive };', { context });
  const imports = new Map();
  for (const match of source.matchAll(/import\s*\{([^}]+)\}\s*from\s*['"]([^'"]+)['"]/g)) {
    imports.set(match[2], match[1].split(',').map(name => name.trim()));
  }
  await module.link(specifier => {
    const names = imports.get(specifier);
    return new vm.SyntheticModule(names, function () {
      for (const name of names) this.setExport(name, services[name] ?? (() => null));
    }, { context });
  });
  await module.evaluate();
  const rendered = module.namespace.MobileDrive();
  const handlers = [];
  function visit(node) {
    if (Array.isArray(node)) { node.forEach(visit); return; }
    if (!node?.strings) return;
    node.values.forEach((value, index) => {
      if (node.strings[index].includes('class="m-start"') && node.strings[index].endsWith('disabled=')) {
        // The following interpolated value after disabled is its onClick callback.
        handlers.push(node.values[index + 1]);
      }
      visit(value);
    });
  }
  visit(rendered);
  assert.equal(handlers.length, 1, 'find the actual mobile Start button');
  return { go: handlers[0], calls, busy };
}

test('mobile fleet join refreshes metadata, asks consent and passes per-session approval', async () => {
  const { go, calls, busy } = await mobile(true);
  await go();
  assert.deepEqual(calls.slice(0, 2), ['refresh', 'consent']);
  assert.equal(calls[2][0], 'start');
  assert.equal(calls[2][1].__fleetApproved, true);
  assert.deepEqual(busy, [true, false]);
});

test('declining a mobile fleet download never starts the game', async () => {
  const { go, calls, busy } = await mobile(false);
  await go();
  assert.deepEqual(calls, ['refresh', 'consent']);
  assert.deepEqual(busy, [true, false]);
});

test('mobile Start clears its busy state after a failed launch', async () => {
  const { go, busy } = await mobile(true, true);
  await assert.rejects(go(), /start failed/);
  assert.deepEqual(busy, [true, false]);
});
