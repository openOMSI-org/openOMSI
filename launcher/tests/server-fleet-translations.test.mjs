// Lightweight coverage gate for the server-fleet YAML catalogue.
// No npm or YAML parser required: these entries deliberately use quoted JSON strings,
// which are also valid YAML 1.2 scalars.
// Run: node --test launcher/tests/server-fleet-translations.test.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const yaml = readFileSync(new URL('../../crates/omsi-app/locales/server_fleet.yml', import.meta.url), 'utf8');
const sources = [
  '../ui/js/data.js',
  '../ui/js/pages/bus-picker.js',
  '../ui/js/pages/drive-classic.js',
].map((path) => readFileSync(new URL(path, import.meta.url), 'utf8')).join('\n');
const languages = [
  'es', 'zh-tw', 'pt', 'ca', 'fr', 'sr', 'ru', 'de', 'pt-pt', 'ka',
  'sv', 'uk', 'be', 'kk', 'pl', 'cs', 'hu', 'it', 'nl', 'tr',
  'ja', 'zh', 'hi', 'ko', 'th', 'vi', 'id', 'ms', 'tl',
];
const required = [
  'Download server vehicle',
  'Download and join',
  'The selected bus or repaint may need files from this server. Matching local files are reused; other files are checked and cached for this session without replacing your installed mods. Continue?',
  'Preview available after the server bus is downloaded',
  'This bus and its selected paint will be checked against the server and downloaded when needed.',
  'Confirm the server download before joining.',
  'The server has not announced a downloadable version of this bus. Choose another bus or contact the server owner.',
  'Offered by the joined server; its verified version is downloaded for this session.',
];
const placeholders = (value) => [...value.matchAll(/%\{[\w]+\}|\{[^{}]*\}/g)].map((m) => m[0]).sort();

// Decode exactly the controlled subset used in server_fleet.yml. Invalid
// escapes, duplicate keys, duplicate locale codes and unexpected syntax fail.
function parse() {
  const entries = new Map();
  let current = null;
  assert.match(yaml, /^_version: 2$/m);
  for (const [i, line] of yaml.split('\n').entries()) {
    if (!line.trim() || line.startsWith('#') || line === '_version: 2') continue;
    if (line.startsWith('"') && line.endsWith(':')) {
      const key = JSON.parse(line.slice(0, -1));
      assert(!entries.has(key), 'duplicate key: ' + key);
      current = new Map();
      entries.set(key, current);
    } else {
      const matched = /^  ([a-z]{2,3}(?:-[a-z]{2,3})?): (".*")$/.exec(line);
      assert(matched && current, 'malformed YAML at line ' + (i + 1));
      const [, lang, translated] = matched;
      assert(!current.has(lang), 'duplicate ' + lang + ' at line ' + (i + 1));
      current.set(lang, JSON.parse(translated));
    }
  }
  return entries;
}

test('all eight fleet messages are referenced by real launcher code', () => {
  assert.equal(required.length, 8);
  for (const key of required) {
    assert(sources.includes(key), 'missing runtime string: ' + key);
  }
});

test('fleet translations cover exactly 29 locales with no missing keys or invalid placeholders', () => {
  const entries = parse();
  assert.deepEqual([...entries.keys()].sort(), [...required].sort());
  for (const [key, translations] of entries) {
    assert.deepEqual([...translations.keys()].sort(), [...languages].sort(), key);
    for (const [lang, translated] of translations) {
      assert.equal(typeof translated, 'string', lang + ': ' + key);
      assert(translated.trim().length > 0, lang + ': empty translation for ' + key);
      assert.deepEqual(placeholders(translated), placeholders(key), lang + ': invalid placeholders for ' + key);
    }
  }
  assert.equal([...entries.values()].reduce((sum, translations) => sum + translations.size, 0), 232);
});
