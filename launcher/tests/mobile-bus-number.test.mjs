// The phone launcher must let the player set the bus's fleet number before the session starts
// (#4056): some vehicles cannot change it any more once they stand on the map, and the phone
// layout used to offer only the number plate.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const bus = { file: 'Vehicles/Test.bus', name: 'Test', manufacturer: 'Test', paints: [], numbers: [['4711', 'B-OM 123'], ['4712', 'B-OM 124']] };

/** `mobile.js` with its imports stubbed, its choice as given; `numbers` off: a bus that lists none. */
async function load(choice, numbers = bus.numbers) {
  const source = await readFile(new URL('../ui/js/mobile/mobile.js', import.meta.url), 'utf8');
  const chosen = { entry: -1, time: 600, date: '1989-05-30', free: true, bus: bus.file, paint: '', plate: '', number: '', ...choice };
  const state = { choice: chosen, poll: { instances: [] }, boot: {}, lines: {}, weather: [], vehicles: [bus], page: 'drive' };
  const calls = [];
  const labels = [];
  const services = {
    html: (strings, ...values) => ({ strings, values }),
    useStore: () => state,
    useState: (initial) => [initial, () => {}],
    useEffect: () => {},
    useConfirm: () => [async () => true, null],
    t: (text) => { labels.push(text); return text; },
    choose: (patch) => calls.push(patch),
    curMap: () => ({ name: 'Test', entry_points: [] }),
    curBus: () => ({ ...bus, numbers }),
    curLine: () => null,
    curTour: () => null,
    pickedTrip: () => null,
    noServerBus: () => false,
    transportRefusal: () => false,
    realTime: () => false,
    realDate: () => false,
    DriveMode: () => null,
  };
  const context = vm.createContext({ console });
  const module = new vm.SourceTextModule(`${source}\nexport { MobileDrive, NumberSheet };`, { context });
  const imports = new Map([...source.matchAll(/import\s*\{([^}]+)\}\s*from\s*['"]([^'"]+)['"]/g)].map((m) => [m[2], m[1].split(',').map((n) => n.trim())]));
  await module.link((specifier) => new vm.SyntheticModule(imports.get(specifier), function () {
    for (const name of imports.get(specifier)) this.setExport(name, services[name] ?? (() => null));
  }, { context }));
  await module.evaluate();
  return { module, labels, calls, state };
}

/** Every value a rendered tree carries (the handlers are values of the templates). */
function values(node, out = []) {
  if (Array.isArray(node)) return node.forEach((x) => values(x, out)), out;
  if (!node?.values) return out;
  for (const value of node.values) {
    out.push(value);
    values(value, out);
  }
  return out;
}

test('the phone Drive offers the fleet number beside the bus', async () => {
  const { module, labels } = await load({ number: '4711' });
  const shown = values(module.namespace.MobileDrive());
  assert(labels.includes('Fleet number'), 'the Drive list must have a Fleet number row');
  assert(shown.includes('4711'), 'the row must show the number the duty will start with');
});

test('a bus that lists no number still shows the row, as Automatic', async () => {
  const { module, labels } = await load({ number: '' }, []);
  const shown = values(module.namespace.MobileDrive());
  assert(labels.includes('Fleet number'));
  assert(shown.includes('Automatic'), 'an empty number means the bus keeps its own');
});

test('the number sheet types a number and offers the bus\'s own', async () => {
  const { module, calls } = await load({ number: '' });
  const sheet = values(module.namespace.NumberSheet({ close() {} }));
  for (const value of sheet.filter((x) => typeof x === 'function')) value({ currentTarget: { value: '1234' } });
  assert.equal(calls.find((c) => c.number === '1234')?.number, '1234', 'typing a number must choose it');
  assert(calls.some((c) => c.number === '4711'), 'the numbers the bus lists must be offered');
  assert(calls.some((c) => c.number === '4712'), 'the numbers the bus lists must be offered');
});

/** The desktop launcher's vehicle details, its imports stubbed. */
async function loadBusOptions(numbers) {
  const source = await readFile(new URL('../ui/js/pages/bus-list.js', import.meta.url), 'utf8');
  const services = {
    html: (strings, ...values) => ({ strings, values }),
    S: { favourites: [] },
    useStore: () => ({ choice: { bus: bus.file, paint: '', plate: '', number: '', hof: '', hof_manual: false }, vehicles: [bus] }),
    useState: (initial) => [initial, () => {}],
    useMemo: (fn) => fn(),
    useEffect: () => {},
    useRef: () => ({ current: null }),
    remembered: () => '1',
    remember: () => {},
    t: (text) => text,
    choose: () => {},
    curBus: () => ({ ...bus, numbers, hofs: [] }),
    noServerBus: () => false,
  };
  const context = vm.createContext({ console });
  const module = new vm.SourceTextModule(source, { context });
  const imports = new Map([...source.matchAll(/import\s*\{([^}]+)\}\s*from\s*['"]([^'"]+)['"]/g)].map((m) => [m[2], m[1].split(',').map((n) => n.trim())]));
  await module.link((specifier) => new vm.SyntheticModule(imports.get(specifier), function () {
    for (const name of imports.get(specifier)) this.setExport(name, services[name] ?? (() => null));
  }, { context }));
  await module.evaluate();
  const tree = module.namespace.BusOptions();
  const shown = values(tree);
  const texts = [];
  const visit = (node) => {
    if (Array.isArray(node)) return node.forEach(visit);
    if (!node?.strings) return;
    texts.push(node.strings.join(''));
    (node.values || []).forEach(visit);
  };
  visit(tree);
  return { shown, texts: texts.join(' ') };
}

test('the desktop details offer the fleet number of a bus that lists none', async () => {
  const { shown, texts } = await loadBusOptions([]);
  assert(texts.includes('Fleet number'), 'the field must be there for every bus');
  assert(shown.includes('Automatic'), 'a bus with no [number] list must still be typed a number');
  assert(!shown.includes('Custom'));
});

test('a bus with its own numbers is offered them as well', async () => {
  const { shown, texts } = await loadBusOptions(bus.numbers);
  assert(texts.includes('Fleet number'));
  assert(shown.includes('Custom'), 'the list and a typed number must both be possible');
});
