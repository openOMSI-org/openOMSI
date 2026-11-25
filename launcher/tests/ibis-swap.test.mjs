// The launcher's onboard-computer (IBIS) swap (#4077): the variables a bus's paint files set,
// offered in the vehicle's options, so that the player picks the unit the pack's repaints name
// instead of authoring another `.bus`.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const ibis = {
  name: 'vis_ibis_type',
  onboard_computer: true,
  values: [
    { value: '0', paints: ['IBIS-2 classic'] },
    { value: '1', paints: ['AFR4', 'Almex'] },
  ],
};
const doors = { name: 'v_drzwi', onboard_computer: false, values: [{ value: '1', paints: ['Glazed doors'] }] };

/** `bus-list.js` with its imports stubbed, the chosen bus carrying `variables`. */
async function load(variables, setvars = {}) {
  const source = await readFile(new URL('../ui/js/pages/bus-list.js', import.meta.url), 'utf8');
  const calls = [];
  const services = {
    html: (strings, ...values) => ({ strings, values }),
    S: { favourites: [] },
    useStore: () => ({ choice: { bus: 'Vehicles/Test.bus', paint: '', plate: '', number: '', hof: '', hof_manual: false, setvars }, vehicles: [] }),
    useState: (initial) => [initial, () => {}],
    useMemo: (fn) => fn(),
    useEffect: () => {},
    useRef: () => ({ current: null }),
    remembered: () => '1',
    remember: () => {},
    t: (text) => text,
    choose: (patch) => calls.push(patch),
    curBus: () => ({ file: 'Vehicles/Test.bus', name: 'Test', paints: [], numbers: [], hofs: [], variables }),
    noServerBus: () => false,
    curMap: () => null,
  };
  const context = vm.createContext({ console });
  const module = new vm.SourceTextModule(source, { context });
  const imports = new Map([...source.matchAll(/import\s*\{([^}]+)\}\s*from\s*['"]([^'"]+)['"]/g)].map((m) => [m[2], m[1].split(',').map((n) => n.trim())]));
  await module.link((specifier) => new vm.SyntheticModule(imports.get(specifier), function () {
    for (const name of imports.get(specifier)) this.setExport(name, services[name] ?? (() => null));
  }, { context }));
  await module.evaluate();
  const values = [];
  const visit = (node) => {
    if (Array.isArray(node)) return node.forEach(visit);
    if (!node) return;
    for (const value of node.values || []) {
      values.push(value);
      visit(value);
    }
  };
  visit(module.namespace.BusOptions());
  // (a select's [value, label] pairs: the ones whose labels mention `needle`)
  const options = (needle) => values.find((v) => Array.isArray(v) && v.some((o) => Array.isArray(o) && String(o[1]).includes(needle)));
  const call = (arg) => {
    for (const fn of values.filter((v) => typeof v === 'function')) {
      const before = calls.length;
      try { fn(arg); } catch { continue; }
      const made = calls.slice(before).find((c) => c && 'setvars' in c);
      if (made) return made;
    }
    return null;
  };
  return { values, options, call, calls };
}

test('a bus whose paints name an IBIS unit offers it as the onboard computer', async () => {
  const { values, options } = await load([ibis, doors]);
  assert(values.includes('Onboard computer'), 'the vehicle options must offer the onboard computer');
  assert(!values.includes('v_drzwi'), 'a variable that is not an onboard computer stays out of it');
  const units = options('AFR4');
  assert(units, 'the values the paints give the variable must be offered');
  assert.deepEqual(JSON.parse(JSON.stringify(units.map(([value]) => value))), ['', '0', '1']);
  assert(units[2][1].includes('AFR4') && units[2][1].includes('Almex'), 'each value names the paints that use it');
});

test('the unit the player picks is the variable the bus is started with', async () => {
  const { call } = await load([ibis]);
  const picked = call('1');
  assert(picked, 'the options must choose the variable');
  assert.equal(picked.setvars?.vis_ibis_type, '1');
  const automatic = call('');
  assert.deepEqual(JSON.parse(JSON.stringify(automatic?.setvars)), {}, 'Automatic leaves the variable to the paint');
});

test('a bus whose paints name no unit shows no onboard computer', async () => {
  const { values } = await load([doors]);
  assert(!values.includes('Onboard computer'));
});

test('the unit picked before is the one the list shows', async () => {
  const { values, options } = await load([ibis], { vis_ibis_type: '1' });
  assert(options('AFR4'));
  assert(values.includes('1'), 'the chosen value is what the select holds');
});
