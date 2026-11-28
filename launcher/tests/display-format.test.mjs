import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

async function load(path, services = {}) {
  const source = await readFile(new URL(path, import.meta.url), 'utf8');
  const context = vm.createContext({ window: {} });
  const module = new vm.SourceTextModule(source, { context });
  const imports = new Map([...source.matchAll(/import\s*\{([^}]+)\}\s*from\s*['"]([^'"]+)['"]/g)].map((m) => [m[2], m[1].split(',').map((s) => s.trim())]));
  await module.link((specifier) => new vm.SyntheticModule(imports.get(specifier), function () {
    for (const name of imports.get(specifier)) this.setExport(name, services[name] ?? (() => null));
  }, { context }));
  await module.evaluate();
  return module.namespace;
}

test('12-hour display keeps midnight, noon and next-day times distinct; launch input stays canonical', async () => {
  const { clockTime, hhmm } = await load('../ui/js/i18n.js');
  for (const [minutes, twelve, twentyFour] of [[0, '12:00 AM', '00:00'], [720, '12:00 PM', '12:00'], [780, '01:00 PM', '13:00'], [1439, '11:59 PM', '23:59'], [1440, '12:00 AM', '00:00']]) {
    assert.equal(clockTime(minutes, '12h'), twelve);
    assert.equal(clockTime(minutes, '24h'), twentyFour);
    assert.equal(clockTime(minutes, 'invalid'), twentyFour);
    assert.equal(hhmm(minutes), twentyFour);
  }
  assert.equal(clockTime(-1, '12h'), '11:59 PM');
});

test('General exposes metric/imperial and independent 12/24-hour settings on every platform', async () => {
  const { SETTINGS_TABS } = await load('../ui/js/pages/settings-schema.js', { t: (s) => s });
  const rows = SETTINGS_TABS.find((t) => t.id === 'general').cols.flatMap((c) => c.rows);
  for (const [key, values] of [['units', ['metric', 'uk', 'us']], ['time_format', ['24h', '12h']]]) {
    const row = rows.find((r) => r.key === key);
    assert(row);
    assert.equal(row.type, 'select');
    assert.equal(row.when, undefined);
    assert.deepEqual(Array.from(row.options, (o) => o[0]), values);
  }
});
