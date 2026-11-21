// Run the real Profile bar's Delete with the launcher's services stubbed.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const source = await readFile(new URL('../ui/js/pages/input-profiles.js', import.meta.url), 'utf8');

async function bar(profiles, { confirmed = true, refuse = null } = {}) {
  const calls = [];
  const toasts = [];
  let hook = 0;
  const services = {
    html: (strings, ...values) => ({ strings, values }),
    useState: (initial) => [hook++ === 0 ? profiles : initial, () => {}],
    useEffect: () => {},
    useStore: () => ({ choice: {} }),
    S: { cache: {} },
    set: () => {},
    toast: (text, kind) => toasts.push([text, kind]),
    t: (text, args) => (args ? text.replace('{name}', args.name) : text),
    call: async (cmd, args) => {
      calls.push([cmd, args]);
      if (cmd === refuse) throw new Error(`${cmd} refused`);
      return profiles;
    },
    busName: (b) => b,
    useConfirm: () => [async () => confirmed, null],
  };
  const context = vm.createContext({ console });
  const module = new vm.SourceTextModule(source, { context });
  const imports = new Map();
  for (const match of source.matchAll(/import\s*\{([^}]+)\}\s*from\s*['"]([^'"]+)['"]/g)) {
    imports.set(match[2], match[1].split(',').map((name) => name.trim()));
  }
  await module.link((specifier) => {
    const names = imports.get(specifier);
    return new vm.SyntheticModule(names, function () {
      for (const name of names) this.setExport(name, services[name] ?? (() => null));
    }, { context });
  });
  await module.evaluate();
  const found = [];
  (function visit(node) {
    if (Array.isArray(node)) return node.forEach(visit);
    if (!node?.strings) return;
    node.values.forEach((value, i) => {
      if (node.strings[i].includes('title="Delete"') && node.strings[i].endsWith('onClick=')) found.push(value);
      visit(value);
    });
  })(module.namespace.InputProfiles());
  assert.equal(found.length, 1, 'find the Delete button');
  // (copied out of the module's realm, so that deepEqual compares values only)
  const changes = () => JSON.parse(JSON.stringify(calls.filter(([cmd]) => cmd !== 'input_profiles')));
  return { remove: found[0], changes, toasts };
}

const three = { active: 'Wheel', default: 'Default', profiles: ['Default', 'Pad', 'Wheel'], buses: {} };

test('deleting the profile in use switches to the default one first', async () => {
  const { remove, changes, toasts } = await bar(three);
  await remove();
  assert.deepEqual(changes(), [
    ['input_profile_switch', { name: 'Default' }],
    ['input_profile_delete', { name: 'Wheel' }],
  ]);
  assert.deepEqual(toasts, [['Profile Default in use', 'ok']]);
});

test('deleting the default profile while in use switches to another one', async () => {
  const { remove, changes } = await bar({ ...three, active: 'Default' });
  await remove();
  assert.deepEqual(changes().map(([cmd, a]) => [cmd, a.name]), [['input_profile_switch', 'Pad'], ['input_profile_delete', 'Default']]);
});

test('a refused switch deletes nothing', async () => {
  const { remove, changes, toasts } = await bar(three, { refuse: 'input_profile_switch' });
  await remove();
  assert.deepEqual(changes().map(([cmd]) => cmd), ['input_profile_switch']);
  assert.equal(toasts[0][1], 'error');
});

test('nothing happens when the deletion is not confirmed', async () => {
  const { remove, changes } = await bar(three, { confirmed: false });
  await remove();
  assert.deepEqual(changes(), []);
});
