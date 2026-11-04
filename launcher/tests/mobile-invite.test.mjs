import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

async function component(file, choice) {
  const source = await readFile(new URL(file, import.meta.url), 'utf8');
  const labels = [];
  const DriveMode = () => null;
  const state = { choice: { entry: -1, time: 600, date: '1989-05-30', ...choice }, poll: { instances: [] }, boot: {}, lines: {}, weather: [], vehicles: [] };
  const services = {
    html: (strings, ...values) => ({ strings, values }),
    useStore: () => state,
    useState: initial => [initial, () => {}],
    useEffect: () => {},
    useConfirm: () => [async () => true, null],
    t: text => { labels.push(text); return text; },
    curMap: () => ({ name: 'Test', entry_points: [] }),
    curBus: () => ({ file: 'Vehicles/Test.bus', name: 'Test' }),
    curLine: () => null, curTour: () => null, pickedTrip: () => null,
    noServerBus: () => false, transportRefusal: () => false,
    DriveMode,
  };
  const context = vm.createContext({ console });
  const mobile = file.includes('/mobile/');
  const module = new vm.SourceTextModule(source + (mobile ? '\nexport { MobileDrive, TABS };' : ''), { context });
  const imports = new Map([...source.matchAll(/import\s*\{([^}]+)\}\s*from\s*['"]([^'"]+)['"]/g)].map(match => [match[2], match[1].split(',').map(name => name.trim())]));
  await module.link(specifier => new vm.SyntheticModule(imports.get(specifier), function () {
    for (const name of imports.get(specifier)) this.setExport(name, services[name] ?? (() => null));
  }, { context }));
  await module.evaluate();
  const rendered = mobile ? module.namespace.MobileDrive() : module.namespace.StartBar({ step: 'duty', go() {} });
  const values = [];
  const visit = node => {
    if (Array.isArray(node)) return node.forEach(visit);
    if (!node?.values) return;
    for (const value of node.values) { values.push(value); visit(value); }
  };
  visit(rendered);
  return { labels, values, DriveMode, tabs: mobile ? module.namespace.TABS : null };
}

const phone = '../ui/js/mobile/mobile.js';
const desktop = '../ui/js/pages/drive-classic.js';

test('phone Drive renders the shared hosting, invitation and session-code controls', async () => {
  const result = await component(phone, { lan_mode: 'off', free: true });
  assert(result.values.includes(result.DriveMode), 'MobileDrive must render DriveMode');
  assert.equal(result.tabs.find(([id]) => id === 'multiplayer')[1], 'Servers');
});

test('phone hosting never directs players back to Multiplayer for the code', async () => {
  const { labels } = await component(phone, { lan_mode: 'host', free: false });
  assert(!labels.some(text => text.includes('return to Multiplayer')));
});

for (const [name, file] of [['desktop', desktop], ['phone', phone]]) {
  for (const [lan_mode, free, expected] of [
    ['off', false, 'Start the duty'],
    ['off', true, 'Start driving'],
    ['host', false, 'Host and drive'],
    ['host', true, 'Host and drive'],
    ['join', true, 'Join and drive'],
  ]) {
    test(`${name} start label: ${lan_mode}, free=${free} -> ${expected}`, async () => {
      const { labels } = await component(file, { lan_mode, free });
      assert(labels.includes(expected), `Missing start label: ${expected}`);
      assert(!labels.includes('Start solo driving'));
    });
  }
}
